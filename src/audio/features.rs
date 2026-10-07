//! 整曲特征表 —— 渲染线程**唯一**的数据来源。
//!
//! 启动时一次性算完；播放期只做 `at(t)` 查表与相邻帧插值，
//! 因此渲染耗时与音频复杂度无关，导出与实时播放共享同一份数据。

use serde::{Deserialize, Serialize};

/// 频段数量。
pub const BAND_COUNT: usize = 6;
/// 对数频点数量。
pub const SPECTRUM_BINS: usize = 128;
/// 频段名称（用于调试面板）。
pub const BAND_NAMES: [&str; BAND_COUNT] = ["sub", "bass", "lowmid", "mid", "highmid", "high"];
/// 频段边界（Hz）。
pub const BAND_EDGES: [(f64, f64); BAND_COUNT] = [
    (20.0, 60.0),
    (60.0, 250.0),
    (250.0, 500.0),
    (500.0, 2000.0),
    (2000.0, 4000.0),
    (4000.0, 20000.0),
];

/// 单帧（1/fps 秒）音乐特征。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FrameFeatures {
    /// 该帧对应的音频时间（秒）
    pub time: f64,
    /// 总能量（RMS，线性）
    pub energy: f32,
    /// 整曲归一化后的能量（0..1）
    pub energy_norm: f32,
    /// RMS 的分贝值
    pub rms_db: f32,
    /// 6 个频段能量（线性）
    pub bands: [f32; BAND_COUNT],
    /// 6 个频段能量（0..1，按整曲 95 分位归一）
    pub bands_norm: [f32; BAND_COUNT],
    /// 128 个对数频点能量（已做逐点滑动 AGC）
    pub spectrum: Vec<f32>,
    /// 频谱质心（Hz）
    pub centroid: f32,
    /// 归一化质心（0..1，映射 20Hz..16kHz 的对数轴）
    pub centroid_norm: f32,
    /// 过零率（0..1）
    pub zcr: f32,
    /// 人声比（0..1，估算）
    pub vocal: f32,
    /// 鼓组能量（0..1）
    pub drums: f32,
    /// 贝斯能量（0..1）
    pub bass: f32,
    /// 高频闪烁（0..1）
    pub shimmer: f32,
    /// 节拍脉冲（0..1，触发后指数衰减）
    pub beat: f32,
    /// 该帧是否为起音点
    pub onset: bool,
    /// 该帧是否接近静音
    pub silence: bool,
}

impl FrameFeatures {
    /// 按名称取频段能量（归一化）。
    pub fn band(&self, name: &str) -> f32 {
        BAND_NAMES
            .iter()
            .position(|n| *n == name)
            .map(|i| self.bands_norm[i])
            .unwrap_or(0.0)
    }

    /// 在 `spectrum` 上按 0..1 位置采样（线性插值）。
    pub fn spec_at(&self, u: f32) -> f32 {
        if self.spectrum.is_empty() {
            return 0.0;
        }
        let u = u.clamp(0.0, 1.0);
        let x = u * (self.spectrum.len() - 1) as f32;
        let i = x.floor() as usize;
        let j = (i + 1).min(self.spectrum.len() - 1);
        let f = x - i as f32;
        self.spectrum[i] * (1.0 - f) + self.spectrum[j] * f
    }
}

/// 整曲特征轨道。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FeatureTrack {
    /// 分析帧率
    pub fps: f64,
    /// 采样率
    pub sample_rate: u32,
    /// 总时长（秒）
    pub duration: f64,
    /// 估计 BPM
    pub bpm: f32,
    /// 每帧特征
    pub frames: Vec<FrameFeatures>,
}

impl FeatureTrack {
    /// 新建。
    pub fn new(fps: f64, sample_rate: u32, duration: f64, frames: Vec<FrameFeatures>) -> Self {
        Self {
            fps,
            sample_rate,
            duration,
            bpm: 0.0,
            frames,
        }
    }

    /// 帧数。
    pub fn len(&self) -> usize {
        self.frames.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// 时间 → 帧下标（钳位）。
    pub fn index_at(&self, t: f64) -> usize {
        if self.frames.is_empty() || self.fps <= 0.0 {
            return 0;
        }
        let i = (t * self.fps).floor();
        if i <= 0.0 {
            0
        } else if i as usize >= self.frames.len() {
            self.frames.len() - 1
        } else {
            i as usize
        }
    }

    /// 时间 → 帧引用（钳位）。
    pub fn at(&self, t: f64) -> &FrameFeatures {
        static EMPTY: std::sync::OnceLock<FrameFeatures> = std::sync::OnceLock::new();
        if self.frames.is_empty() {
            return EMPTY.get_or_init(FrameFeatures::default);
        }
        &self.frames[self.index_at(t)]
    }

    /// 时间 → 相邻两帧插值后的特征。
    ///
    /// 连续量（能量、频段、频谱、质心…）线性插值，布尔量取最近帧，
    /// 这样 60fps 渲染在 30fps 分析下也不会出现台阶感。
    pub fn blend(&self, t: f64) -> FrameFeatures {
        if self.frames.is_empty() {
            return FrameFeatures::default();
        }
        let x = (t * self.fps).max(0.0);
        let i = self.index_at(t);
        let j = (i + 1).min(self.frames.len() - 1);
        let f = (x - i as f64).clamp(0.0, 1.0) as f32;
        let a = &self.frames[i];
        let b = &self.frames[j];
        if i == j || f <= 0.0 {
            return a.clone();
        }

        let mut out = a.clone();
        out.time = t;
        out.energy = lerp(a.energy, b.energy, f);
        out.energy_norm = lerp(a.energy_norm, b.energy_norm, f);
        out.rms_db = lerp(a.rms_db, b.rms_db, f);
        for k in 0..BAND_COUNT {
            out.bands[k] = lerp(a.bands[k], b.bands[k], f);
            out.bands_norm[k] = lerp(a.bands_norm[k], b.bands_norm[k], f);
        }
        out.centroid = lerp(a.centroid, b.centroid, f);
        out.centroid_norm = lerp(a.centroid_norm, b.centroid_norm, f);
        out.zcr = lerp(a.zcr, b.zcr, f);
        out.vocal = lerp(a.vocal, b.vocal, f);
        out.drums = lerp(a.drums, b.drums, f);
        out.bass = lerp(a.bass, b.bass, f);
        out.shimmer = lerp(a.shimmer, b.shimmer, f);
        out.beat = lerp(a.beat, b.beat, f);

        let n = a.spectrum.len().min(b.spectrum.len());
        if n > 0 {
            out.spectrum = Vec::with_capacity(n);
            for k in 0..n {
                out.spectrum.push(lerp(a.spectrum[k], b.spectrum[k], f));
            }
        }
        out
    }

    /// 整曲平均能量。
    pub fn mean_energy(&self) -> f32 {
        if self.frames.is_empty() {
            return 0.0;
        }
        self.frames.iter().map(|f| f.energy).sum::<f32>() / self.frames.len() as f32
    }
}

#[inline]
fn lerp(a: f32, b: f32, f: f32) -> f32 {
    a + (b - a) * f
}

/// 计算归一化分位点（用于把线性特征压到 0..1）。
pub fn percentile(values: &[f32], p: f32) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    let mut v: Vec<f32> = values.iter().copied().filter(|x| x.is_finite()).collect();
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let idx = ((v.len() - 1) as f32 * p.clamp(0.0, 1.0)).round() as usize;
    v[idx]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk(time: f64, e: f32) -> FrameFeatures {
        let mut f = FrameFeatures {
            time,
            energy: e,
            ..Default::default()
        };
        f.spectrum = vec![e; SPECTRUM_BINS];
        f
    }

    #[test]
    fn index_at_clamps_both_ends() {
        let t = FeatureTrack::new(60.0, 44_100, 1.0, vec![mk(0.0, 1.0), mk(1.0, 2.0)]);
        assert_eq!(t.index_at(-5.0), 0);
        assert_eq!(t.index_at(0.0), 0);
        assert_eq!(t.index_at(100.0), 1);
    }

    #[test]
    fn blend_midpoint_is_average() {
        let t = FeatureTrack::new(
            1.0,
            44_100,
            2.0,
            vec![mk(0.0, 0.0), mk(1.0, 1.0), mk(2.0, 2.0)],
        );
        let f = t.blend(0.5);
        assert!((f.energy - 0.5).abs() < 1e-6, "实际 {}", f.energy);
        assert!((f.spectrum[0] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn blend_on_exact_frame_returns_that_frame() {
        let t = FeatureTrack::new(1.0, 44_100, 2.0, vec![mk(0.0, 0.0), mk(1.0, 7.0)]);
        assert!((t.blend(1.0).energy - 7.0).abs() < 1e-6);
    }

    #[test]
    fn blend_handles_empty_track() {
        let t = FeatureTrack::default();
        let f = t.blend(3.0);
        assert_eq!(f.energy, 0.0);
    }

    #[test]
    fn spec_at_interpolates_and_clamps() {
        let mut f = FrameFeatures::default();
        f.spectrum = vec![0.0, 1.0];
        assert!((f.spec_at(0.0) - 0.0).abs() < 1e-6);
        assert!((f.spec_at(1.0) - 1.0).abs() < 1e-6);
        assert!((f.spec_at(0.5) - 0.5).abs() < 1e-6);
        assert!((f.spec_at(-3.0) - 0.0).abs() < 1e-6);
        assert!((f.spec_at(9.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn band_lookup_by_name_works() {
        let mut f = FrameFeatures::default();
        f.bands_norm = [0.1, 0.2, 0.3, 0.4, 0.5, 0.6];
        assert!((f.band("bass") - 0.2).abs() < 1e-6);
        assert!((f.band("high") - 0.6).abs() < 1e-6);
        assert_eq!(f.band("nope"), 0.0);
    }

    #[test]
    fn percentile_picks_expected_values() {
        let v: Vec<f32> = (0..=100).map(|i| i as f32).collect();
        assert!((percentile(&v, 0.5) - 50.0).abs() < 1.0);
        assert!((percentile(&v, 0.95) - 95.0).abs() < 1.0);
        assert_eq!(percentile(&[], 0.5), 0.0);
    }

    #[test]
    fn band_edges_are_contiguous_and_ascending() {
        for i in 1..BAND_COUNT {
            assert!(
                BAND_EDGES[i].0 >= BAND_EDGES[i - 1].1,
                "频段 {i} 与前一段不连续"
            );
        }
        for &(a, b) in &BAND_EDGES {
            assert!(b > a);
        }
    }
}

//! 分轨（stems）加载与仿真。
//!
//! 若 `--stems-dir` 下存在 `vocals.wav` / `drums.wav` / `bass.wav` / `other.wav`，
//! 就用真实分轨；否则按频段比例**仿真**一份。
//! 两条路径产出同样的结构，下游不需要分支 —— 拿到的是「更准」或「近似」的差别，
//! 而不是「有」和「没有」的差别。

use std::path::Path;

use crate::audio::features::FrameFeatures;
use crate::audio::loader::decode_file;

/// 四路分轨（单声道、统一采样率、统一长度）。
#[derive(Debug, Clone, Default)]
pub struct Stems {
    /// 人声
    pub vocals: Option<Vec<f32>>,
    /// 鼓组
    pub drums: Option<Vec<f32>>,
    /// 贝斯
    pub bass: Option<Vec<f32>>,
    /// 其它（和声/合成器）
    pub other: Option<Vec<f32>>,
}

impl Stems {
    /// 是否加载到了任意一路真实分轨。
    pub fn has_any(&self) -> bool {
        self.vocals.is_some() || self.drums.is_some() || self.bass.is_some() || self.other.is_some()
    }

    /// 是否四路齐全。
    pub fn is_complete(&self) -> bool {
        self.vocals.is_some() && self.drums.is_some() && self.bass.is_some() && self.other.is_some()
    }

    /// 已加载的轨道名。
    pub fn loaded_names(&self) -> Vec<&'static str> {
        let mut v = Vec::new();
        if self.vocals.is_some() {
            v.push("vocals");
        }
        if self.drums.is_some() {
            v.push("drums");
        }
        if self.bass.is_some() {
            v.push("bass");
        }
        if self.other.is_some() {
            v.push("other");
        }
        v
    }
}

/// 从目录加载分轨。缺失的文件静默跳过（返回 `None` 而非报错）。
///
/// 每路都会：多声道混合成单声道 → 线性重采样到 `target_sr` → 截断/补零到 `target_frames`。
pub fn load_dir(dir: &Path, target_sr: u32, target_frames: usize) -> Stems {
    let mut s = Stems::default();

    let try_load = |name: &str| -> Option<Vec<f32>> {
        // 优先 wav，其次 flac
        for ext in ["wav", "flac"] {
            let p = dir.join(format!("{name}.{ext}"));
            if p.exists() {
                match decode_file(&p) {
                    Ok((pcm, sr, ch)) => {
                        return Some(to_mono_resampled(&pcm, sr, ch, target_sr, target_frames));
                    }
                    Err(e) => {
                        tracing::warn!(path = %p.display(), error = %e, "分轨解码失败，忽略");
                    }
                }
            }
        }
        None
    };

    s.vocals = try_load("vocals");
    s.drums = try_load("drums");
    s.bass = try_load("bass");
    s.other = try_load("other");
    s
}

/// 交错多声道 PCM → 单声道 → 重采样 → 定长。
pub fn to_mono_resampled(
    pcm: &[f32],
    src_sr: u32,
    channels: u16,
    dst_sr: u32,
    dst_frames: usize,
) -> Vec<f32> {
    let ch = channels.max(1) as usize;
    let mono_len = pcm.len() / ch;

    // 1) 混单声道
    let mut mono = Vec::with_capacity(mono_len);
    for i in 0..mono_len {
        let mut acc = 0.0f32;
        for c in 0..ch {
            acc += pcm[i * ch + c];
        }
        mono.push(acc / ch as f32);
    }

    // 2) 重采样（线性插值）
    let resampled = if src_sr == dst_sr || mono.is_empty() {
        mono
    } else {
        let ratio = dst_sr as f64 / src_sr as f64;
        let out_len = ((mono.len() as f64) * ratio).round() as usize;
        let mut out = Vec::with_capacity(out_len);
        for i in 0..out_len {
            let x = i as f64 / ratio;
            let i0 = x.floor() as usize;
            let i1 = (i0 + 1).min(mono.len() - 1);
            let f = (x - i0 as f64) as f32;
            out.push(mono[i0] * (1.0 - f) + mono[i1] * f);
        }
        out
    };

    // 3) 定长
    let mut out = resampled;
    out.resize(dst_frames.max(1), 0.0);
    out
}

/// 用频段能量仿真「人声比」「鼓组」「贝斯」，供无分轨时使用。
///
/// 依据：
/// - 人声基频落在 lowmid/mid，共振峰落在 highmid；三者同时抬升才判为人声。
/// - 鼓组看 sub 的瞬态 + high 的镲片。
/// - 贝斯几乎只体现在 sub/bass。
pub fn simulate_from_bands(f: &mut FrameFeatures) {
    let sub = f.bands_norm[0];
    let bass = f.bands_norm[1];
    let lowmid = f.bands_norm[2];
    let mid = f.bands_norm[3];
    let highmid = f.bands_norm[4];
    let high = f.bands_norm[5];

    // 人声：中频主体 + 高频共振峰，且不能是纯低频轰鸣
    let voice_body = (lowmid * 0.6 + mid * 0.4).clamp(0.0, 1.0);
    let voice_presence = highmid.clamp(0.0, 1.0);
    let low_mask = (sub * 0.9).clamp(0.0, 1.0);
    f.vocal =
        ((voice_body * 0.65 + voice_presence * 0.35) * (1.0 - 0.5 * low_mask)).clamp(0.0, 1.0);

    // 鼓组：低频冲击 + 高频镲片，起音帧额外加权
    let transient = if f.onset { 1.25 } else { 1.0 };
    f.drums = ((sub * 0.7 + high * 0.3) * transient).clamp(0.0, 1.0);

    // 贝斯：几乎全在 sub/bass
    f.bass = (sub * 0.55 + bass * 0.45).clamp(0.0, 1.0);

    // 高频闪烁：high/highmid 的高频细节
    f.shimmer = (high * 0.7 + highmid * 0.3).clamp(0.0, 1.0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::features::BAND_COUNT;

    #[test]
    fn mono_downmix_averages_channels() {
        // 两声道：L=1.0, R=0.0 → 0.5
        let pcm = vec![1.0f32, 0.0, 1.0, 0.0];
        let out = to_mono_resampled(&pcm, 100, 2, 100, 2);
        assert_eq!(out.len(), 2);
        assert!((out[0] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn resample_upsamples_to_target_length() {
        let mono = vec![0.0f32, 1.0, 0.0, 1.0]; // 4 帧 @ 4Hz
        let out = to_mono_resampled(&mono, 4, 1, 8, 8); // → 8 帧 @ 8Hz
        assert_eq!(out.len(), 8);
    }

    #[test]
    fn short_input_is_zero_padded() {
        let mono = vec![0.5f32, 0.5];
        let out = to_mono_resampled(&mono, 10, 1, 10, 10);
        assert_eq!(out.len(), 10);
        assert_eq!(out[9], 0.0);
    }

    #[test]
    fn long_input_is_truncated() {
        let mono = vec![1.0f32; 100];
        let out = to_mono_resampled(&mono, 10, 1, 10, 10);
        assert_eq!(out.len(), 10);
    }

    #[test]
    fn empty_input_yields_silence_of_target_length() {
        let out = to_mono_resampled(&[], 44_100, 2, 44_100, 32);
        assert_eq!(out.len(), 32);
        assert!(out.iter().all(|&x| x == 0.0));
    }

    #[test]
    fn simulate_marks_low_heavy_frames_as_bass() {
        let mut f = FrameFeatures::default();
        f.bands_norm = [0.95, 0.9, 0.1, 0.05, 0.05, 0.05];
        simulate_from_bands(&mut f);
        assert!(f.bass > 0.6, "低频占优应判为贝斯强，实际 {}", f.bass);
        assert!(f.vocal < 0.4, "纯低频不该判为人声，实际 {}", f.vocal);
    }

    #[test]
    fn simulate_marks_mid_heavy_frames_as_vocal() {
        let mut f = FrameFeatures::default();
        f.bands_norm = [0.05, 0.1, 0.7, 0.8, 0.75, 0.2];
        simulate_from_bands(&mut f);
        assert!(f.vocal > 0.4, "中频占优应判为人声，实际 {}", f.vocal);
    }

    #[test]
    fn simulated_values_stay_in_unit_range() {
        for a in 0..=10 {
            for b in 0..=10 {
                let mut f = FrameFeatures::default();
                let mut bands = [0.0f32; BAND_COUNT];
                bands[0] = a as f32 / 10.0;
                bands[5] = b as f32 / 10.0;
                f.bands_norm = bands;
                f.onset = a % 2 == 0;
                simulate_from_bands(&mut f);
                for v in [f.vocal, f.drums, f.bass, f.shimmer] {
                    assert!((0.0..=1.0).contains(&v), "越界: {v}");
                }
            }
        }
    }

    #[test]
    fn onset_boosts_drum_estimate() {
        let mut quiet = FrameFeatures::default();
        quiet.bands_norm = [0.5, 0.2, 0.2, 0.2, 0.2, 0.3];
        let mut loud = quiet.clone();
        loud.onset = true;
        simulate_from_bands(&mut quiet);
        simulate_from_bands(&mut loud);
        assert!(loud.drums > quiet.drums);
    }

    #[test]
    fn stems_reports_loaded_names() {
        let s = Stems {
            vocals: Some(vec![]),
            bass: Some(vec![]),
            ..Default::default()
        };
        assert_eq!(s.loaded_names(), vec!["vocals", "bass"]);
        assert!(s.has_any());
        assert!(!s.is_complete());
    }
}

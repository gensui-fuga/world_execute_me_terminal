//! 节拍检测（整曲后处理，不参与实时渲染）。
//!
//! 判据：能量超过**局部均值**一定倍数 + 超过绝对下限 + 谱通量确认 + 局部极大值，
//! 再做去抖。比单看能量阈值稳得多 —— 副歌整体变响时不会把每一帧都当鼓点。

use serde::{Deserialize, Serialize};

/// 检测参数。
#[derive(Debug, Clone)]
pub struct BeatParams {
    /// 局部均值窗口（帧）。43 帧 ≈ 1 秒 @43fps
    pub history: usize,
    /// 超过局部均值的倍数阈值
    pub sensitivity: f32,
    /// 绝对能量下限（防止静音段落误触发）
    pub abs_floor: f32,
    /// 谱通量相对均值的最低比例
    pub flux_ratio: f32,
    /// 脉冲衰减时间常数（秒）
    pub decay_tau: f32,
    /// 去抖窗口（帧）
    pub debounce: usize,
}

impl Default for BeatParams {
    fn default() -> Self {
        Self {
            history: 43,
            sensitivity: 1.30,
            abs_floor: 0.010,
            flux_ratio: 0.90,
            decay_tau: 0.15,
            debounce: 3,
        }
    }
}

/// 检测结果：逐帧脉冲强度 + 起音标记 + 估计 BPM。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BeatGrid {
    /// 逐帧脉冲（0..1，触发后指数衰减）
    pub strength: Vec<f32>,
    /// 逐帧起音标记
    pub onset: Vec<bool>,
    /// 估计 BPM（60..200）
    pub bpm: f32,
    /// 起音总数
    pub onset_count: usize,
}

impl BeatGrid {
    /// 取某帧的脉冲强度。
    pub fn at(&self, i: usize) -> f32 {
        self.strength.get(i).copied().unwrap_or(0.0)
    }

    /// 某帧是否为起音。
    pub fn is_onset(&self, i: usize) -> bool {
        self.onset.get(i).copied().unwrap_or(false)
    }

    /// 距上一拍多少秒（用于节拍同步的视觉动画）。
    pub fn seconds_since_onset(&self, i: usize, fps: f64) -> f64 {
        let mut k = i.min(self.onset.len().saturating_sub(1));
        let mut steps = 0usize;
        loop {
            if self.is_onset(k) {
                return steps as f64 / fps.max(1.0);
            }
            if k == 0 || steps > 300 {
                return f64::INFINITY;
            }
            k -= 1;
            steps += 1;
        }
    }
}

/// 对整曲的能量包络与谱通量做节拍检测。
///
/// `energy` 与 `flux` 必须等长（逐帧）。
pub fn detect(energy: &[f32], flux: &[f32], fps: f64, p: &BeatParams) -> BeatGrid {
    let n = energy.len();
    let mut grid = BeatGrid {
        strength: vec![0.0; n],
        onset: vec![false; n],
        bpm: 0.0,
        onset_count: 0,
    };
    if n == 0 {
        return grid;
    }

    let half = (p.history / 2).max(1);
    let mut cand: Vec<(usize, f32)> = Vec::new();

    for i in 0..n {
        let a = i.saturating_sub(half);
        let b = (i + half + 1).min(n);
        let w = (b - a) as f32;
        let e = energy[i];
        let e_mean = energy[a..b].iter().sum::<f32>() / w;

        if e <= e_mean * p.sensitivity || e <= p.abs_floor {
            continue;
        }

        // 局部极大值：左右邻帧都不比它大
        let lo = i.saturating_sub(1);
        let hi = (i + 2).min(n);
        if energy[lo..hi].iter().any(|&x| x > e) {
            continue;
        }

        // 谱通量确认：新的频谱成分出现，才算打击乐起音
        let (f, f_mean) = if flux.len() == n {
            let f = flux[i];
            let fm = flux[a..b].iter().sum::<f32>() / w;
            (f, fm)
        } else {
            (0.0, 0.0)
        };
        if f_mean > 1e-6 && f < f_mean * p.flux_ratio {
            continue;
        }

        let e_ratio = (e / e_mean.max(1e-9)).min(4.0);
        let f_ratio = if f_mean > 1e-6 {
            (f / f_mean).min(4.0)
        } else {
            1.0
        };
        cand.push((i, 0.7 * e_ratio + 0.3 * f_ratio));
    }

    // 去抖：窗口内只保留最强者
    let mut kept: Vec<(usize, f32)> = Vec::new();
    for (i, s) in cand {
        match kept.last().copied() {
            Some((j, js)) if i.saturating_sub(j) < p.debounce => {
                if s > js {
                    kept.pop();
                    kept.push((i, s));
                }
            }
            _ => kept.push((i, s)),
        }
    }

    // 生成衰减脉冲
    let dt = 1.0 / fps.max(1.0);
    let decay = (-(dt / p.decay_tau.max(1e-3) as f64)).exp() as f32;
    let mut cur = 0.0f32;
    let mut ki = 0usize;
    for i in 0..n {
        cur *= decay;
        if ki < kept.len() && kept[ki].0 == i {
            let s = kept[ki].1;
            cur = (0.78 + 0.22 * (s / 4.0)).clamp(0.0, 1.0);
            grid.onset[i] = true;
            ki += 1;
        }
        grid.strength[i] = cur.clamp(0.0, 1.0);
    }

    // BPM：起音间隔的中位数，规整到 60..200
    if kept.len() >= 2 {
        let mut gaps: Vec<f64> = kept
            .windows(2)
            .map(|w| (w[1].0 - w[0].0) as f64 / fps.max(1.0))
            .filter(|g| *g > 1e-6)
            .collect();
        if !gaps.is_empty() {
            gaps.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let med = gaps[gaps.len() / 2];
            if med > 0.0 {
                let mut bpm = 60.0 / med;
                while bpm < 60.0 {
                    bpm *= 2.0;
                }
                while bpm > 200.0 {
                    bpm /= 2.0;
                }
                grid.bpm = bpm as f32;
            }
        }
    }

    grid.onset_count = kept.len();
    grid
}

/// 谱通量：相邻帧幅度谱的正向差分之和，用于区分「新起音」与「持续音」。
pub fn spectral_flux(prev: &[f32], cur: &[f32]) -> f32 {
    let n = prev.len().min(cur.len());
    let mut sum = 0.0f32;
    for i in 0..n {
        let d = cur[i] - prev[i];
        if d > 0.0 {
            sum += d;
        }
    }
    sum
}

/// 能量包络（RMS）→ 归一化 0..1。
pub fn normalize_envelope(values: &[f32], p: f32) -> Vec<f32> {
    let peak = crate::audio::features::percentile(values, p).max(1e-9);
    values.iter().map(|v| (v / peak).clamp(0.0, 1.0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FPS: f64 = 100.0;

    /// 生成规则脉冲：每 `period` 帧一个尖峰。
    fn pulse_train(n: usize, period: usize, amp: f32) -> Vec<f32> {
        let mut v = vec![0.02f32; n];
        let mut i = 0;
        while i < n {
            v[i] = amp;
            i += period;
        }
        v
    }

    #[test]
    fn detects_regular_pulses() {
        let n = 1_000;
        let period = 50; // 2 Hz → 120 BPM
        let energy = pulse_train(n, period, 1.0);
        let flux = pulse_train(n, period, 0.5);
        let grid = detect(&energy, &flux, FPS, &BeatParams::default());

        let expected = n / period;
        let got = grid.onset_count;
        assert!(
            got.abs_diff(expected) <= 2,
            "期望约 {expected} 个起音，实际 {got}"
        );
        assert!((grid.bpm - 120.0).abs() < 8.0, "BPM 估计 {}", grid.bpm);
    }

    #[test]
    fn silence_produces_no_onsets() {
        let n = 500;
        let energy = vec![0.0f32; n];
        let flux = vec![0.0f32; n];
        let grid = detect(&energy, &flux, FPS, &BeatParams::default());
        assert_eq!(grid.onset_count, 0);
        assert!(grid.strength.iter().all(|&s| s == 0.0));
    }

    #[test]
    fn steady_loud_tone_is_not_all_onsets() {
        // 恒定响度不该被当成连续鼓点
        let n = 600;
        let energy = vec![0.8f32; n];
        let flux = vec![0.0f32; n];
        let grid = detect(&energy, &flux, FPS, &BeatParams::default());
        assert!(
            grid.onset_count <= 2,
            "恒定音被误判 {} 次",
            grid.onset_count
        );
    }

    #[test]
    fn pulse_decays_between_onsets() {
        let n = 400;
        let period = 100;
        let energy = pulse_train(n, period, 1.0);
        let flux = pulse_train(n, period, 0.5);
        let grid = detect(&energy, &flux, FPS, &BeatParams::default());
        // 起音后第 1 帧应高于第 30 帧（衰减中）
        if grid.onset_count >= 1 {
            let first = grid.onset.iter().position(|&x| x).unwrap();
            let a = grid.strength[first];
            let b = grid.strength[(first + 30).min(n - 1)];
            assert!(a > b, "脉冲未衰减: {a} → {b}");
        }
    }

    #[test]
    fn debounce_suppresses_adjacent_peaks() {
        // 每 2 帧一个峰，去抖窗口 3 帧 → 应被压到远少于原始峰数
        let n = 200;
        let mut energy = vec![0.01f32; n];
        let mut i = 0;
        while i < n {
            energy[i] = 1.0;
            i += 2;
        }
        let flux = vec![0.5f32; n];
        let grid = detect(&energy, &flux, FPS, &BeatParams::default());
        assert!(grid.onset_count < 60, "去抖失效，起音 {}", grid.onset_count);
    }

    #[test]
    fn spectral_flux_only_counts_positive_change() {
        let a = [0.0, 0.5, 1.0];
        let b = [0.2, 0.2, 1.0];
        // +0.2 + 0 + 0
        assert!((spectral_flux(&a, &b) - 0.2).abs() < 1e-6);
    }

    #[test]
    fn normalize_envelope_maps_peak_to_one() {
        let v = vec![0.1f32, 0.5, 1.0, 0.25];
        let out = normalize_envelope(&v, 1.0);
        assert!((out[2] - 1.0).abs() < 1e-6);
        assert!(out.iter().all(|&x| (0.0..=1.0).contains(&x)));
    }

    #[test]
    fn seconds_since_onset_is_zero_on_onset() {
        let n = 200;
        let energy = pulse_train(n, 100, 1.0);
        let flux = pulse_train(n, 100, 0.5);
        let grid = detect(&energy, &flux, FPS, &BeatParams::default());
        if let Some(i) = grid.onset.iter().position(|&x| x) {
            assert_eq!(grid.seconds_since_onset(i, FPS), 0.0);
            assert!((grid.seconds_since_onset(i + 10, FPS) - 0.1).abs() < 1e-6);
        }
    }
}

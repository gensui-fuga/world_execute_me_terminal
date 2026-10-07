//! STFT 与频谱工具。
//!
//! 全曲预分析：`hop = sample_rate / fps`，窗口 4096 点 Hann。
//! 渲染线程**永远不**调用这里 —— 所有频谱在启动时算完，播放期只查表。

use realfft::num_complex::Complex;
use realfft::{RealFftPlanner, RealToComplex};
use std::sync::Arc;

/// 生成长度为 `n` 的 Hann 窗。
///
/// `n == 1` 时退化为单点窗（避免除零）。
pub fn hann_window(n: usize) -> Vec<f32> {
    if n <= 1 {
        return vec![1.0; n];
    }
    let denom = (n - 1) as f32;
    (0..n)
        .map(|i| {
            let x = std::f32::consts::TAU * i as f32 / denom;
            0.5 * (1.0 - x.cos())
        })
        .collect()
}

/// 频率 → FFT bin 下标（就近取整，钳到 `[0, bins-1]`）。
pub fn hz_to_bin(hz: f64, sample_rate: u32, fft_size: usize) -> usize {
    let bins = fft_size / 2 + 1;
    if hz <= 0.0 {
        return 0;
    }
    let k = (hz * fft_size as f64 / sample_rate as f64).round() as i64;
    k.clamp(0, bins as i64 - 1) as usize
}

/// FFT bin 下标 → 中心频率（Hz）。
pub fn bin_to_hz(k: usize, sample_rate: u32, fft_size: usize) -> f64 {
    k as f64 * sample_rate as f64 / fft_size as f64
}

/// 一个可复用的短时傅里叶分析器。
///
/// 内部持有 FFT 计划、窗函数与工作缓冲；`&mut self` 保证缓冲可安全复用，
/// 预分析是单线程顺序执行，因此不需要内部锁。
pub struct Stft {
    fft: Arc<dyn RealToComplex<f32>>,
    window: Vec<f32>,
    fft_size: usize,
    hop: usize,
    input: Vec<f32>,
    output: Vec<Complex<f32>>,
    scratch: Vec<Complex<f32>>,
}

impl Stft {
    /// 新建分析器。`hop` 为相邻帧的样本间隔。
    pub fn new(fft_size: usize, hop: usize) -> Self {
        assert!(fft_size >= 2, "fft_size 至少为 2");
        let fft_size = fft_size.next_power_of_two().max(2);
        let mut planner = RealFftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(fft_size);
        let input = fft.make_input_vec();
        let output = fft.make_output_vec();
        let scratch = fft.make_scratch_vec();
        Self {
            fft,
            window: hann_window(fft_size),
            fft_size,
            hop: hop.max(1),
            input,
            output,
            scratch,
        }
    }

    /// 窗长。
    pub fn fft_size(&self) -> usize {
        self.fft_size
    }

    /// 帧间隔。
    pub fn hop(&self) -> usize {
        self.hop
    }

    /// 幅度谱长度（`fft_size/2 + 1`）。
    pub fn bins(&self) -> usize {
        self.fft_size / 2 + 1
    }

    /// 分析一帧，把**幅度谱**写入 `out`（长度须等于 `bins()`）。
    ///
    /// 输入帧不足 `fft_size` 时右侧补零；超出时只取前 `fft_size` 个样本。
    /// 幅度已按窗增益归一化，因此不同窗长下的量级可比。
    pub fn magnitude_into(&mut self, frame: &[f32], out: &mut [f32]) {
        debug_assert_eq!(out.len(), self.bins());
        let n = self.fft_size;

        for i in 0..n {
            let s = if i < frame.len() { frame[i] } else { 0.0 };
            self.input[i] = s * self.window[i];
        }

        if self
            .fft
            .process_with_scratch(&mut self.input, &mut self.output, &mut self.scratch)
            .is_err()
        {
            out.iter_mut().for_each(|v| *v = 0.0);
            return;
        }

        // 幅度归一化：2 / Σ窗，使正弦输入的峰值幅度 ≈ 其线性幅度。
        let win_sum: f32 = self.window.iter().sum();
        let norm = if win_sum > 0.0 { 2.0 / win_sum } else { 0.0 };

        for (k, o) in out.iter_mut().enumerate() {
            let c = self.output[k];
            *o = (c.re * c.re + c.im * c.im).sqrt() * norm;
        }
    }

    /// 便捷版：返回新分配的幅度谱。
    pub fn magnitude(&mut self, frame: &[f32]) -> Vec<f32> {
        let mut out = vec![0.0; self.bins()];
        self.magnitude_into(frame, &mut out);
        out
    }
}

/// 把 `[lo_hz, hi_hz]` 平均分成 `n` 个频段，返回每个频段的 `(k_start, k_end)`。
///
/// 用于固定频段能量（sub / bass / lowmid / ...）。
pub fn linear_bands(
    n: usize,
    lo_hz: f64,
    hi_hz: f64,
    sample_rate: u32,
    fft_size: usize,
) -> Vec<(usize, usize)> {
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let a = lo_hz + (hi_hz - lo_hz) * i as f64 / n as f64;
        let b = lo_hz + (hi_hz - lo_hz) * (i + 1) as f64 / n as f64;
        let ka = hz_to_bin(a, sample_rate, fft_size);
        let kb = hz_to_bin(b, sample_rate, fft_size).max(ka + 1);
        out.push((ka, kb));
    }
    out
}

/// 对数间隔频段（音乐感知更自然），返回 `(k_start, k_end)`。
///
/// 低频按等比划分，避免低频段过窄、高频段过宽。
pub fn log_bands(
    n: usize,
    lo_hz: f64,
    hi_hz: f64,
    sample_rate: u32,
    fft_size: usize,
) -> Vec<(usize, usize)> {
    let mut out = Vec::with_capacity(n);
    let ratio = (hi_hz / lo_hz).powf(1.0 / n as f64);
    for i in 0..n {
        let a = lo_hz * ratio.powi(i as i32);
        let b = lo_hz * ratio.powi(i as i32 + 1);
        let ka = hz_to_bin(a, sample_rate, fft_size);
        let kb = hz_to_bin(b, sample_rate, fft_size).max(ka + 1);
        out.push((ka, kb));
    }
    out
}

/// 峰值频率（Hz）—— 用于音高类调试显示。
pub fn peak_hz(mag: &[f32], sample_rate: u32, fft_size: usize) -> f64 {
    let mut best = 0usize;
    let mut best_v = f32::MIN;
    // 跳过直流与超低频噪声
    for (k, &v) in mag.iter().enumerate().skip(1) {
        if v > best_v {
            best_v = v;
            best = k;
        }
    }
    bin_to_hz(best, sample_rate, fft_size)
}

/// 频谱质心（Hz）—— 亮度指标。
pub fn spectral_centroid(mag: &[f32], sample_rate: u32, fft_size: usize) -> f64 {
    let mut num = 0.0f64;
    let mut den = 0.0f64;
    for (k, &v) in mag.iter().enumerate() {
        let v = v as f64;
        num += bin_to_hz(k, sample_rate, fft_size) * v;
        den += v;
    }
    if den <= 1e-12 {
        0.0
    } else {
        num / den
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f64, sr: u32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| (std::f64::consts::TAU * freq * i as f64 / sr as f64).sin() as f32)
            .collect()
    }

    #[test]
    fn hann_window_endpoints_are_zero_and_center_is_one() {
        let w = hann_window(9);
        assert!(w[0].abs() < 1e-6);
        assert!(w[8].abs() < 1e-6);
        assert!((w[4] - 1.0).abs() < 1e-6, "中心点应为 1，实际 {}", w[4]);
    }

    #[test]
    fn hann_window_is_symmetric() {
        let w = hann_window(16);
        for i in 0..8 {
            assert!((w[i] - w[15 - i]).abs() < 1e-6);
        }
    }

    #[test]
    fn hann_window_handles_degenerate_sizes() {
        assert!(hann_window(0).is_empty());
        assert_eq!(hann_window(1), vec![1.0]);
    }

    #[test]
    fn hz_bin_roundtrip() {
        let sr = 48_000;
        let n = 4096;
        for &hz in &[100.0, 440.0, 1000.0, 5000.0, 12_000.0] {
            let k = hz_to_bin(hz, sr, n);
            let back = bin_to_hz(k, sr, n);
            // 一个 bin 的宽度
            let width = sr as f64 / n as f64;
            assert!((back - hz).abs() <= width, "{hz} → {back} 误差超过一个 bin");
        }
    }

    #[test]
    fn hz_to_bin_clamps_out_of_range() {
        let sr = 48_000;
        let n = 1024;
        assert_eq!(hz_to_bin(-10.0, sr, n), 0);
        assert_eq!(hz_to_bin(1e9, sr, n), n / 2);
    }

    #[test]
    fn sine_peak_lands_on_expected_bin() {
        let sr = 44_100;
        let n = 4096;
        let mut stft = Stft::new(n, n / 2);
        let frame = sine(1000.0, sr, n);
        let mag = stft.magnitude(&frame);
        let got = peak_hz(&mag, sr, n);
        assert!((got - 1000.0).abs() < 15.0, "峰值 {got}Hz 偏离 1kHz 过多");
    }

    #[test]
    fn sine_amplitude_is_normalized_near_unit() {
        let sr = 44_100;
        let n = 4096;
        let mut stft = Stft::new(n, n / 2);
        let frame = sine(1000.0, sr, n);
        let mag = stft.magnitude(&frame);
        let peak = mag.iter().fold(0.0f32, |a, &b| a.max(b));
        assert!(
            (peak - 1.0).abs() < 0.15,
            "归一化后峰值应接近 1.0，实际 {peak}"
        );
    }

    #[test]
    fn magnitude_bins_count_is_correct() {
        let mut stft = Stft::new(1024, 256);
        assert_eq!(stft.bins(), 513);
        assert_eq!(stft.magnitude(&vec![0.0; 1024]).len(), 513);
    }

    #[test]
    fn silence_gives_zero_spectrum() {
        let mut stft = Stft::new(1024, 256);
        let mag = stft.magnitude(&vec![0.0; 1024]);
        assert!(mag.iter().all(|&v| v < 1e-9));
    }

    #[test]
    fn fft_size_is_rounded_to_power_of_two() {
        let stft = Stft::new(1000, 100);
        assert_eq!(stft.fft_size(), 1024);
    }

    #[test]
    fn centroid_of_low_sine_is_lower_than_high_sine() {
        let sr = 44_100;
        let n = 4096;
        let mut stft = Stft::new(n, n / 2);
        let low = stft.magnitude(&sine(200.0, sr, n));
        let high = stft.magnitude(&sine(6000.0, sr, n));
        let c_low = spectral_centroid(&low, sr, n);
        let c_high = spectral_centroid(&high, sr, n);
        assert!(c_low < c_high, "低频质心 {c_low} 应低于高频 {c_high}");
        assert!(c_low > 100.0 && c_low < 400.0);
    }

    #[test]
    fn linear_bands_cover_range_without_gaps() {
        let bands = linear_bands(6, 20.0, 20_000.0, 44_100, 4096);
        assert_eq!(bands.len(), 6);
        for w in bands.windows(2) {
            assert!(w[0].1 <= w[1].0.max(w[0].1), "频段不应倒退");
        }
        for &(a, b) in &bands {
            assert!(b > a, "频段 [{a},{b}) 为空");
        }
    }

    #[test]
    fn log_bands_increase_in_width() {
        let bands = log_bands(8, 20.0, 20_000.0, 44_100, 8192);
        assert_eq!(bands.len(), 8);
        // 相邻频段宽度比大致恒定（等比划分）
        let ratios: Vec<f64> = bands
            .windows(2)
            .map(|w| (w[1].1 - w[1].0) as f64 / (w[0].1 - w[0].0) as f64)
            .collect();
        let mean = ratios.iter().sum::<f64>() / ratios.len() as f64;
        assert!(mean > 1.0, "对数频段应逐段变宽");
    }
}

//! 音频子系统。
//!
//! 分层：`loader` 负责拿到 PCM，`fft` / `beat` / `stem` / `features` 负责
//! 在**启动时一次性**把整首歌分析成 `Vec<FrameFeatures>`；
//! 渲染线程只按音频时间查表，绝不在帧内做 FFT。
//!
//! 实时播放与离线导出调用同一个 [`analyze`]，因此导出的画面与终端所见一致。

pub mod beat;
pub mod features;
pub mod fft;
pub mod loader;
pub mod stem;

use std::path::Path;

use anyhow::Result;

use crate::audio::beat::BeatParams;
use crate::audio::features::{
    percentile, FeatureTrack, FrameFeatures, BAND_COUNT, BAND_EDGES, SPECTRUM_BINS,
};
use crate::audio::fft::{hz_to_bin, log_bands, spectral_centroid, Stft};
use crate::audio::stem::Stems;

/// 分析窗口长度选择：保证窗口至少覆盖 4 个采样周期，低采样率时自动缩小。
fn pick_fft_size(sample_rate: u32) -> usize {
    let mut n = 4096usize;
    while n > 64 && (n as u32) * 4 > sample_rate {
        n /= 2;
    }
    n
}

/// 交错多声道 → 单声道。
pub fn to_mono(pcm: &[f32], channels: u16) -> Vec<f32> {
    let ch = channels.max(1) as usize;
    if ch == 1 {
        return pcm.to_vec();
    }
    let mut out = Vec::with_capacity(pcm.len() / ch);
    for frame in pcm.chunks_exact(ch) {
        out.push(frame.iter().sum::<f32>() / ch as f32);
    }
    out
}

/// 一帧的 RMS。
fn rms(frame: &[f32]) -> f32 {
    if frame.is_empty() {
        return 0.0;
    }
    let s: f32 = frame.iter().map(|v| v * v).sum();
    (s / frame.len() as f32).sqrt()
}

/// 过零率（0..1）。
fn zero_crossing_rate(frame: &[f32]) -> f32 {
    if frame.len() < 2 {
        return 0.0;
    }
    let mut n = 0usize;
    for w in frame.windows(2) {
        if (w[0] >= 0.0) != (w[1] >= 0.0) {
            n += 1;
        }
    }
    n as f32 / (frame.len() - 1) as f32
}

/// 整曲分析：PCM → 逐帧特征。
///
/// `fps` 决定时间分辨率（也是 `hop = sample_rate / fps`）。
/// `stems_dir` 存在时使用真实分轨，否则仿真。
pub fn analyze(
    pcm: &[f32],
    channels: u16,
    sample_rate: u32,
    fps: f64,
    stems_dir: Option<&Path>,
) -> Result<FeatureTrack> {
    let fps = fps.clamp(1.0, 240.0);
    let mono = to_mono(pcm, channels);
    let duration = mono.len() as f64 / sample_rate as f64;

    let fft_size = pick_fft_size(sample_rate);
    let hop = ((sample_rate as f64 / fps).round() as usize).max(1);
    let n_frames = ((duration * fps).ceil() as usize).max(1);

    tracing::info!(
        duration,
        fps,
        fft_size,
        hop,
        n_frames,
        sample_rate,
        "开始整曲分析"
    );

    let mut stft = Stft::new(fft_size, hop);
    let bins = stft.bins();
    let spec_bands = log_bands(SPECTRUM_BINS, 40.0, 16_000.0, sample_rate, fft_size);
    let band_bins: Vec<(usize, usize)> = BAND_EDGES
        .iter()
        .map(|&(a, b)| {
            let ka = hz_to_bin(a, sample_rate, fft_size);
            let kb = hz_to_bin(b, sample_rate, fft_size).max(ka + 1).min(bins);
            (ka, kb)
        })
        .collect();

    let mut frames: Vec<FrameFeatures> = Vec::with_capacity(n_frames);
    let mut mag = vec![0.0f32; bins];
    let mut prev_mag: Vec<f32> = vec![0.0f32; bins];
    let mut raw_spectrum: Vec<Vec<f32>> = Vec::with_capacity(n_frames);
    let mut flux: Vec<f32> = Vec::with_capacity(n_frames);

    for i in 0..n_frames {
        let start = i * hop;
        let end = (start + fft_size).min(mono.len());
        let frame = if start < mono.len() {
            &mono[start..end]
        } else {
            &[][..]
        };

        stft.magnitude_into(frame, &mut mag);

        let e = rms(frame);
        let zcr = zero_crossing_rate(frame);
        let centroid = spectral_centroid(&mag, sample_rate, fft_size);

        let mut bands = [0.0f32; BAND_COUNT];
        for (bi, &(ka, kb)) in band_bins.iter().enumerate() {
            let mut acc = 0.0f32;
            for k in ka..kb.min(bins) {
                acc += mag[k];
            }
            bands[bi] = acc / (kb.saturating_sub(ka).max(1)) as f32;
        }

        // 128 个对数频点的原始能量
        let mut spec = Vec::with_capacity(SPECTRUM_BINS);
        for &(ka, kb) in &spec_bands {
            let mut acc = 0.0f32;
            let mut cnt = 0usize;
            for k in ka..kb.min(bins) {
                acc += mag[k];
                cnt += 1;
            }
            spec.push(if cnt > 0 { acc / cnt as f32 } else { 0.0 });
        }

        let fl = if i == 0 {
            0.0
        } else {
            beat::spectral_flux(&prev_mag, &mag)
        };
        flux.push(fl);

        // 归一化质心：20Hz..16kHz 的对数轴
        let cn =
            ((centroid.max(20.0).log2() - 20f64.log2()) / (16_000f64.log2() - 20f64.log2())) as f32;

        frames.push(FrameFeatures {
            time: i as f64 / fps,
            energy: e,
            rms_db: if e > 0.0 { 20.0 * e.log10() } else { -120.0 },
            bands,
            centroid: centroid as f32,
            centroid_norm: cn.clamp(0.0, 1.0),
            zcr,
            spectrum: spec.clone(),
            ..Default::default()
        });

        raw_spectrum.push(spec);
        prev_mag.copy_from_slice(&mag);
    }

    // ── 归一化：能量与频段按分位点压到 0..1 ─────────────────────
    let energies: Vec<f32> = frames.iter().map(|f| f.energy).collect();
    let e_peak = percentile(&energies, 0.98).max(1e-9);
    for f in frames.iter_mut() {
        f.energy_norm = (f.energy / e_peak).clamp(0.0, 1.0);
    }

    for bi in 0..BAND_COUNT {
        let col: Vec<f32> = frames.iter().map(|f| f.bands[bi]).collect();
        let peak = percentile(&col, 0.95).max(1e-9);
        for f in frames.iter_mut() {
            f.bands_norm[bi] = (f.bands[bi] / peak).clamp(0.0, 1.0);
        }
    }

    // ── 逐频点滑动 AGC：抑制「整段变响」造成的假动态 ─────────────
    agc_spectrum(&mut raw_spectrum, fps);
    for (f, spec) in frames.iter_mut().zip(raw_spectrum) {
        f.spectrum = spec;
    }

    // ── 节拍 ────────────────────────────────────────────────────
    let energies_norm: Vec<f32> = frames.iter().map(|f| f.energy_norm).collect();
    let grid = beat::detect(&energies_norm, &flux, fps, &BeatParams::default());
    for (i, f) in frames.iter_mut().enumerate() {
        f.beat = grid.at(i);
        f.onset = grid.is_onset(i);
    }

    // ── 分轨 / 仿真 ─────────────────────────────────────────────
    let stems = match stems_dir {
        Some(dir) if dir.is_dir() => stem::load_dir(dir, sample_rate, mono.len()),
        _ => Stems::default(),
    };
    if stems.has_any() {
        tracing::info!(tracks = ?stems.loaded_names(), "使用真实分轨");
        apply_stems(&mut frames, &stems, hop, fft_size, fps);
    } else {
        tracing::info!("未提供分轨，按频段仿真人声/鼓/贝斯");
        for f in frames.iter_mut() {
            stem::simulate_from_bands(f);
        }
    }

    // ── 静音检测 ────────────────────────────────────────────────
    for f in frames.iter_mut() {
        f.silence = f.rms_db < -50.0 && f.energy_norm < 0.02;
    }

    let mut track = FeatureTrack::new(fps, sample_rate, duration, frames);
    track.bpm = grid.bpm;

    tracing::info!(
        bpm = grid.bpm,
        onsets = grid.onset_count,
        mean_energy = track.mean_energy(),
        "整曲分析完成"
    );
    Ok(track)
}

/// 逐频点滑动平均归一化（AGC）。
///
/// 每个频点除以自己 ±1.5 秒窗口内的均值，从而突出**相对**变化；
/// 结果 clamp 到 `[0, 2]` 再除以 2，得到 0..1。
fn agc_spectrum(spec: &mut [Vec<f32>], fps: f64) {
    if spec.is_empty() {
        return;
    }
    let n = spec.len();
    let bins = spec[0].len();
    let half = ((fps * 1.5).round() as usize).max(1);

    // 先按频点求前缀和
    let mut prefix: Vec<Vec<f64>> = Vec::with_capacity(bins);
    for k in 0..bins {
        let mut p = Vec::with_capacity(n + 1);
        p.push(0.0f64);
        let mut acc = 0.0f64;
        for i in 0..n {
            acc += spec[i][k] as f64;
            p.push(acc);
        }
        prefix.push(p);
    }

    for i in 0..n {
        let a = i.saturating_sub(half);
        let b = (i + half + 1).min(n);
        let cnt = (b - a) as f64;
        for k in 0..bins {
            let mean = (prefix[k][b] - prefix[k][a]) / cnt;
            let v = if mean > 1e-9 {
                spec[i][k] as f64 / mean
            } else {
                0.0
            };
            spec[i][k] = ((v / 2.0).clamp(0.0, 1.0)) as f32;
        }
    }
}

/// 用真实分轨覆盖 vocal / drums / bass 估计。
fn apply_stems(frames: &mut [FrameFeatures], stems: &Stems, hop: usize, fft_size: usize, fps: f64) {
    let _ = fps;
    let track_rms = |data: &Option<Vec<f32>>, i: usize| -> Option<f32> {
        let d = data.as_ref()?;
        let start = i * hop;
        if start >= d.len() {
            return Some(0.0);
        }
        let end = (start + fft_size).min(d.len());
        Some(rms(&d[start..end]))
    };

    let mut peaks = [0.0f32; 4];
    let mut cols: [Vec<f32>; 4] = [Vec::new(), Vec::new(), Vec::new(), Vec::new()];
    for i in 0..frames.len() {
        for (t, col) in cols.iter_mut().enumerate() {
            let data = match t {
                0 => &stems.vocals,
                1 => &stems.drums,
                2 => &stems.bass,
                _ => &stems.other,
            };
            let v = track_rms(data, i).unwrap_or(0.0);
            col.push(v);
        }
    }
    for (t, col) in cols.iter().enumerate() {
        peaks[t] = percentile(col, 0.95).max(1e-9);
    }

    for i in 0..frames.len() {
        let f = &mut frames[i];
        if stems.vocals.is_some() {
            f.vocal = (cols[0][i] / peaks[0]).clamp(0.0, 1.0);
        }
        if stems.drums.is_some() {
            f.drums = (cols[1][i] / peaks[1]).clamp(0.0, 1.0);
        }
        if stems.bass.is_some() {
            f.bass = (cols[2][i] / peaks[2]).clamp(0.0, 1.0);
        }
        // 有分轨时 shimmer 仍由高频决定
        f.shimmer = f.bands_norm[5];
    }
}

/// 用能量新颖度找建议的场景切分点。
///
/// 返回 `(时间秒, 新颖度)`，按新颖度从高到低排序，且两点间隔不小于 `min_gap`。
pub fn novelty_candidates(track: &FeatureTrack, min_gap: f64) -> Vec<(f64, f32)> {
    if track.is_empty() {
        return Vec::new();
    }
    let n = track.len();
    let win = (track.fps * 0.5).round().max(1.0) as usize;
    let mut scored: Vec<(usize, f32)> = Vec::with_capacity(n);

    for i in 0..n {
        let a = i.saturating_sub(win);
        let b = (i + win + 1).min(n);
        let left: f32 = (a..i).map(|k| track.frames[k].energy_norm).sum::<f32>()
            / (i.saturating_sub(a).max(1)) as f32;
        let right: f32 = (i..b).map(|k| track.frames[k].energy_norm).sum::<f32>()
            / (b.saturating_sub(i).max(1)) as f32;
        // 频谱变化 + 能量跳变
        let mut spec_diff = 0.0f32;
        if i > 0 {
            let p = &track.frames[i - 1].spectrum;
            let c = &track.frames[i].spectrum;
            let m = p.len().min(c.len());
            for k in 0..m {
                spec_diff += (c[k] - p[k]).abs();
            }
            if m > 0 {
                spec_diff /= m as f32;
            }
        }
        scored.push((i, (right - left).abs() * 2.0 + spec_diff * 3.0));
    }

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let mut out: Vec<(f64, f32)> = Vec::new();
    for (i, s) in scored {
        let t = i as f64 / track.fps;
        if out.iter().all(|(ot, _)| (t - ot).abs() >= min_gap) {
            out.push((t, s));
        }
        if out.len() >= 64 {
            break;
        }
    }
    out.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f64, sr: u32, n: usize, amp: f32) -> Vec<f32> {
        (0..n)
            .map(|i| amp * (std::f64::consts::TAU * freq * i as f64 / sr as f64).sin() as f32)
            .collect()
    }

    #[test]
    fn to_mono_passthrough_for_single_channel() {
        let pcm = vec![0.1f32, 0.2, 0.3];
        assert_eq!(to_mono(&pcm, 1), pcm);
    }

    #[test]
    fn to_mono_averages_stereo() {
        let pcm = vec![1.0f32, -1.0, 0.5, 0.5];
        let m = to_mono(&pcm, 2);
        assert_eq!(m.len(), 2);
        assert!(m[0].abs() < 1e-6);
        assert!((m[1] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn rms_of_full_scale_sine_is_root_half() {
        let s = sine(1000.0, 44_100, 4096, 1.0);
        let r = rms(&s);
        assert!(
            (r - std::f32::consts::FRAC_1_SQRT_2).abs() < 0.01,
            "RMS = {r}"
        );
    }

    #[test]
    fn zcr_of_silence_is_zero_and_of_fast_sine_is_high() {
        assert_eq!(zero_crossing_rate(&vec![0.0; 100]), 0.0);
        let slow = sine(100.0, 44_100, 4096, 1.0);
        let fast = sine(8000.0, 44_100, 4096, 1.0);
        assert!(zero_crossing_rate(&fast) > zero_crossing_rate(&slow));
    }

    #[test]
    fn analyze_produces_frames_covering_duration() {
        let sr = 22_050;
        let secs = 2.0;
        let pcm = sine(440.0, sr, (sr as f64 * secs) as usize, 0.5);
        let track = analyze(&pcm, 1, sr, 60.0, None).unwrap();
        assert!((track.duration - secs).abs() < 0.05);
        let expect = (secs * 60.0).ceil() as usize;
        assert!(
            track.len().abs_diff(expect) <= 2,
            "帧数 {} 与期望 {} 不符",
            track.len(),
            expect
        );
    }

    #[test]
    fn analyze_detects_silence() {
        let sr = 22_050;
        let pcm = vec![0.0f32; sr as usize];
        let track = analyze(&pcm, 1, sr, 30.0, None).unwrap();
        assert!(track.frames.iter().all(|f| f.silence), "静音段未被标记");
    }

    #[test]
    fn analyze_puts_pure_bass_energy_in_low_bands() {
        let sr = 22_050;
        let pcm = sine(45.0, sr, sr as usize, 0.8);
        let track = analyze(&pcm, 1, sr, 30.0, None).unwrap();
        let mid: f32 =
            track.frames.iter().map(|f| f.bands_norm[3]).sum::<f32>() / track.len() as f32;
        let sub: f32 =
            track.frames.iter().map(|f| f.bands_norm[0]).sum::<f32>() / track.len() as f32;
        assert!(sub > mid, "45Hz 正弦应主要落在 sub 段: sub={sub} mid={mid}");
    }

    #[test]
    fn analyze_puts_high_energy_in_high_bands() {
        let sr = 22_050;
        let pcm = sine(9000.0, sr, sr as usize, 0.8);
        let track = analyze(&pcm, 1, sr, 30.0, None).unwrap();
        let high: f32 =
            track.frames.iter().map(|f| f.bands_norm[5]).sum::<f32>() / track.len() as f32;
        let sub: f32 =
            track.frames.iter().map(|f| f.bands_norm[0]).sum::<f32>() / track.len() as f32;
        assert!(
            high > sub,
            "9kHz 正弦应主要落在 high 段: high={high} sub={sub}"
        );
    }

    #[test]
    fn analyze_marks_onsets_on_pulse_train() {
        let sr = 22_050;
        let fps = 100.0;
        let secs = 3.0;
        let n = (sr as f64 * secs) as usize;
        let mut pcm = vec![0.0f32; n];
        // 每 0.5 秒一个短促脉冲
        let period = (sr as f64 * 0.5) as usize;
        let burst = (sr as f64 * 0.05) as usize;
        let mut i = 0;
        while i < n {
            for k in 0..burst.min(n - i) {
                let env = 1.0 - k as f32 / burst as f32;
                pcm[i + k] =
                    0.9 * env * (std::f64::consts::TAU * 120.0 * k as f64 / sr as f64).sin() as f32;
            }
            i += period;
        }
        let track = analyze(&pcm, 1, sr, fps, None).unwrap();
        let onsets = track.frames.iter().filter(|f| f.onset).count();
        assert!(onsets >= 3, "应至少检出 3 个起音，实际 {onsets}");
        assert!(track.bpm > 60.0 && track.bpm <= 200.0, "BPM {}", track.bpm);
    }

    #[test]
    fn spectrum_is_normalized_to_unit_range() {
        let sr = 22_050;
        let pcm = sine(1000.0, sr, sr as usize, 0.6);
        let track = analyze(&pcm, 1, sr, 30.0, None).unwrap();
        for f in &track.frames {
            assert_eq!(f.spectrum.len(), SPECTRUM_BINS);
            for &v in &f.spectrum {
                assert!((0.0..=1.0).contains(&v), "频谱值越界 {v}");
            }
        }
    }

    #[test]
    fn centroid_norm_stays_in_range() {
        let sr = 22_050;
        let pcm = sine(3000.0, sr, sr as usize, 0.5);
        let track = analyze(&pcm, 1, sr, 30.0, None).unwrap();
        for f in &track.frames {
            assert!((0.0..=1.0).contains(&f.centroid_norm));
        }
    }

    #[test]
    fn fft_size_shrinks_for_low_sample_rate() {
        assert_eq!(pick_fft_size(44_100), 4096);
        assert!(pick_fft_size(8_000) <= 2048);
        assert!(pick_fft_size(8_000) >= 64);
    }

    #[test]
    fn novelty_finds_a_hard_cut() {
        // 前半静音、后半全响，切点应落在中间
        let sr = 22_050;
        let secs = 4.0;
        let n = (sr as f64 * secs) as usize;
        let mut pcm = vec![0.0f32; n];
        for i in n / 2..n {
            pcm[i] = 0.7 * (std::f64::consts::TAU * 440.0 * i as f64 / sr as f64).sin() as f32;
        }
        let track = analyze(&pcm, 1, sr, 60.0, None).unwrap();
        let cands = novelty_candidates(&track, 0.5);
        assert!(!cands.is_empty());
        let best = cands
            .iter()
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
            .unwrap();
        assert!(
            (best.0 - 2.0).abs() < 0.5,
            "切点应接近 2.0s，实际 {:.2}s",
            best.0
        );
    }

    #[test]
    fn novelty_respects_min_gap() {
        let sr = 22_050;
        let n = (sr as f64 * 3.0) as usize;
        let pcm = sine(440.0, sr, n, 0.5);
        let track = analyze(&pcm, 1, sr, 60.0, None).unwrap();
        let cands = novelty_candidates(&track, 1.0);
        for w in cands.windows(2) {
            assert!(w[1].0 - w[0].0 >= 1.0 - 1e-9, "间隔小于 min_gap");
        }
    }

    #[test]
    fn analyze_handles_tiny_input() {
        let pcm = vec![0.1f32; 10];
        let track = analyze(&pcm, 1, 44_100, 60.0, None).unwrap();
        assert!(!track.is_empty());
    }
}

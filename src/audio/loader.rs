//! 音频加载与解码。
//!
//! 支持 MP3 / WAV / FLAC / OGG（经 rodio 的解码后端）。
//! 未提供 `--audio` 时合成一段 132 BPM 的测试曲，保证项目开箱即可运行。
//!
//! 同时暴露解码后的原始 PCM：整曲预分析（FFT / 节拍 / 分轨）直接吃这份数据，
//! 保证实时播放与离线导出走**同一套**特征。

use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};
use rodio::buffer::SamplesBuffer;
use rodio::{Decoder, OutputStream, Sink, Source};

use crate::config::Config;

/// 合成测试曲速度。
pub const TEST_BPM: f64 = 132.0;
/// 合成测试曲长度（小节，4/4）。
pub const TEST_BARS: usize = 32;
/// 合成测试曲采样率。
pub const SYNTH_SR: u32 = 44_100;

/// 一段可播放音频，同时持有解码后的 PCM 供离线分析使用。
///
/// `OutputStream` 内部持有 cpal 句柄，因此本结构**不是** `Send`；
/// 它只在主线程创建与使用。
pub struct AudioSource {
    _stream: Option<OutputStream>,
    sink: Option<Sink>,
    pcm: Arc<Vec<f32>>,
    sample_rate: u32,
    channels: u16,
    duration: Duration,
}

impl AudioSource {
    /// 按配置打开音频；`--no-audio` 或导出模式下不会占用声卡。
    pub fn open(cfg: &Config) -> Result<Self> {
        let (pcm, sample_rate, channels) = match &cfg.audio {
            Some(path) => {
                decode_file(path).with_context(|| format!("解码音频失败: {}", path.display()))?
            }
            None => {
                let data = synth_test_song(SYNTH_SR, cfg.seed);
                (data, SYNTH_SR, 2u16)
            }
        };

        let frames = pcm.len() / channels.max(1) as usize;
        let duration = Duration::from_secs_f64(frames as f64 / sample_rate.max(1) as f64);

        let (stream, sink) = if cfg.no_audio {
            (None, None)
        } else {
            match OutputStream::try_default() {
                Ok((s, h)) => {
                    let sink = Sink::try_new(&h).context("创建音频输出 Sink 失败")?;
                    sink.set_volume(1.0);
                    (Some(s), Some(sink))
                }
                Err(e) => {
                    // 没有可用声卡时不致命：退化为静音播放，画面照常走时钟。
                    tracing::warn!(error = %e, "无可用音频输出设备，退化为静音模式");
                    (None, None)
                }
            }
        };

        let mut me = Self {
            _stream: stream,
            sink,
            pcm: Arc::new(pcm),
            sample_rate,
            channels,
            duration,
        };

        if !cfg.no_audio {
            if let Some(sink) = &me.sink {
                let buf = SamplesBuffer::new(me.channels, me.sample_rate, (*me.pcm).clone());
                sink.append(buf);
                sink.pause();
            }
        }

        // 让「解码出来的时长」成为唯一事实来源。
        let _ = &mut me;
        Ok(me)
    }

    /// 开始播放。
    pub fn play(&self) -> Result<()> {
        if let Some(sink) = &self.sink {
            sink.play();
        }
        Ok(())
    }

    /// 暂停。
    pub fn pause(&self) {
        if let Some(sink) = &self.sink {
            sink.pause();
        }
    }

    /// 继续。
    pub fn resume(&self) {
        if let Some(sink) = &self.sink {
            sink.play();
        }
    }

    /// 停止并丢弃队列。
    pub fn stop(&self) {
        if let Some(sink) = &self.sink {
            sink.stop();
        }
    }

    /// 当前播放位置。静音模式下返回零（调用方应改用墙钟）。
    pub fn position(&self) -> Duration {
        self.sink
            .as_ref()
            .map(|s| s.get_pos())
            .unwrap_or(Duration::ZERO)
    }

    /// 队列是否已播完。
    pub fn finished(&self) -> bool {
        self.sink.as_ref().map(|s| s.empty()).unwrap_or(true)
    }

    /// 总时长。
    pub fn duration(&self) -> Duration {
        self.duration
    }

    /// 解码后的交错 PCM。
    pub fn pcm(&self) -> Arc<Vec<f32>> {
        Arc::clone(&self.pcm)
    }

    /// 采样率。
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// 声道数。
    pub fn channels(&self) -> u16 {
        self.channels
    }

    /// 是否真的有音频输出。
    pub fn has_output(&self) -> bool {
        self.sink.is_some()
    }
}

/// 解码一个音频文件为交错 f32 样本。
pub fn decode_file(path: &Path) -> Result<(Vec<f32>, u32, u16)> {
    let file = File::open(path).with_context(|| format!("打开失败: {}", path.display()))?;
    let decoder = Decoder::new(BufReader::new(file))
        .with_context(|| format!("无法识别音频格式: {}", path.display()))?;

    let channels = decoder.channels();
    let sample_rate = decoder.sample_rate();
    let samples: Vec<f32> = decoder.convert_samples::<f32>().collect();

    anyhow::ensure!(sample_rate > 0, "音频采样率为 0");
    anyhow::ensure!(channels > 0, "音频声道数为 0");
    Ok((samples, sample_rate, channels))
}

/// 合成一段 132 BPM 的测试曲：底鼓 + 军鼓 + 高帽 + 贝斯 + 简单旋律。
///
/// 固定种子，因此同一 `seed` 永远产出同一段音频 —— 便于回归比对。
pub fn synth_test_song(sample_rate: u32, seed: u64) -> Vec<f32> {
    let sr = sample_rate as f64;
    let beat = 60.0 / TEST_BPM;
    let total_beats = TEST_BARS * 4;
    let total_samples = (total_beats as f64 * beat * sr) as usize;

    // 单声道先合成，最后复制成立体声。
    let mut mono = vec![0.0f32; total_samples];
    let mut rng = SmallRng::seed_from_u64(seed);

    // 小调音阶（A 小调自然音级），用于贝斯与旋律。
    let scale = [
        220.0f32, 246.94, 261.63, 293.66, 329.63, 349.23, 392.0, 440.0,
    ];

    let mut add = |idx: usize, v: f32| {
        if idx < mono.len() {
            mono[idx] += v;
        }
    };

    for b in 0..total_beats {
        let t0 = b as f64 * beat;
        let start = (t0 * sr) as usize;
        let in_bar = b % 4;

        // ── 底鼓：每拍一次，频率 110Hz → 45Hz 快速下滑，指数衰减 ──
        let kick_len = (0.28 * sr) as usize;
        let mut phase = 0.0f64;
        for i in 0..kick_len {
            let u = i as f64 / sr;
            let f = 110.0 * (-28.0 * u).exp() + 45.0;
            phase += std::f64::consts::TAU * f / sr;
            let env = (-9.0 * u).exp();
            add(start + i, (phase.sin() * env * 0.85) as f32);
        }

        // ── 军鼓：2、4 拍，噪声 + 200Hz 体音 ──
        if in_bar == 1 || in_bar == 3 {
            let snare_len = (0.20 * sr) as usize;
            for i in 0..snare_len {
                let u = i as f64 / sr;
                let env = (-16.0 * u).exp();
                let noise: f64 = rng.gen_range(-1.0..1.0);
                let body = (std::f64::consts::TAU * 196.0 * u).sin() * 0.35;
                add(start + i, ((noise * 0.55 + body) * env * 0.6) as f32);
            }
        }

        // ── 高帽：八分音符，短促高频噪声 ──
        for sub in 0..2 {
            let hs = (t0 + sub as f64 * beat * 0.5) * sr;
            let hs = hs as usize;
            let hat_len = (0.045 * sr) as usize;
            let open = in_bar == 3 && sub == 1;
            let len = if open { hat_len * 4 } else { hat_len };
            for i in 0..len {
                let u = i as f64 / sr;
                let env = (-55.0 * u).exp();
                let noise: f64 = rng.gen_range(-1.0..1.0);
                add(hs + i, (noise * env * 0.16) as f32);
            }
        }

        // ── 贝斯：每小节两个长音（根音 + 五度） ──
        let root = if b % 8 < 4 { 0 } else { 3 };
        let bass_f = scale[root] / 2.0;
        let bass_len = (beat * 2.0 * sr) as usize;
        for i in 0..bass_len {
            let u = i as f64 / sr;
            let env = (-2.2 * u).exp() * (1.0 - (-90.0 * u).exp());
            let s = (std::f64::consts::TAU * bass_f as f64 * u).sin();
            let s2 = (std::f64::consts::TAU * (bass_f * 2.0) as f64 * u).sin() * 0.25;
            add(start + i, ((s + s2) * env * 0.42) as f32);
        }

        // ── 旋律：每两拍一个音，四小节一循环 ──
        if b % 2 == 0 {
            let step = [0usize, 2, 4, 3, 5, 4, 2, 1];
            let deg = step[(b / 2) % step.len()];
            let f = scale[deg] * 2.0;
            let mel_len = (beat * 1.6 * sr) as usize;
            for i in 0..mel_len {
                let u = i as f64 / sr;
                let env = (-3.4 * u).exp() * (1.0 - (-120.0 * u).exp());
                let s = (std::f64::consts::TAU * f as f64 * u).sin()
                    + 0.3 * (std::f64::consts::TAU * (f * 2.0) as f64 * u).sin();
                add(start + i, (s * env * 0.18) as f32);
            }
        }
    }

    // 归一化 + 软削波，避免叠加后爆表。
    let peak = mono.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
    if peak > 0.0 {
        let g = 0.89 / peak;
        for s in mono.iter_mut() {
            *s = (*s * g).tanh();
        }
    }

    // 复制成立体声（左右一致）。
    let mut out = Vec::with_capacity(mono.len() * 2);
    for s in mono {
        out.push(s);
        out.push(s);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synth_produces_expected_length_and_range() {
        let sr = 8_000;
        let data = synth_test_song(sr, 42);
        let beat = 60.0 / TEST_BPM;
        let expect_mono = (TEST_BARS * 4) as f64 * beat * sr as f64;
        // 立体声，样本数应为单声道两倍（允许 ±2 的取整误差）
        let expect = (expect_mono * 2.0) as usize;
        assert!(
            (data.len() as i64 - expect as i64).abs() <= 2,
            "长度 {} 与期望 {} 不符",
            data.len(),
            expect
        );
    }

    #[test]
    fn synth_is_deterministic_for_same_seed() {
        let a = synth_test_song(8_000, 7);
        let b = synth_test_song(8_000, 7);
        assert_eq!(a.len(), b.len());
        assert!(a.iter().zip(&b).all(|(x, y)| (x - y).abs() < 1e-9));
    }

    #[test]
    fn synth_stays_within_unit_range() {
        let data = synth_test_song(8_000, 99);
        let peak = data.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
        assert!(peak <= 1.0, "峰值 {peak} 超出 [-1,1]");
        assert!(peak > 0.5, "峰值 {peak} 过低，合成可能失败");
    }

    #[test]
    fn synth_left_right_are_identical() {
        let data = synth_test_song(4_000, 3);
        for frame in data.chunks_exact(2) {
            assert_eq!(frame[0], frame[1]);
        }
    }
}

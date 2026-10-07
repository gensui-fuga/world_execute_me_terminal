//! CLI 参数、配置文件与运行期配置的合并。
//!
//! 优先级：命令行 > config.toml > 内置默认值。
//! 场景时间**不**在这里硬编码，一律来自 `timeline.toml`。

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use serde::{Deserialize, Serialize};

/// 渲染模式。`auto` 会按 `COLORTERM` / 终端能力挑选。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum RenderMode {
    /// 2×4 子像素点阵，密度最高
    Braille,
    /// 1×2 半块
    Half,
    /// 1×1 字符梯度
    Ascii,
    /// 2×2 象限块
    Quadrant,
    /// 自动探测
    #[default]
    Auto,
}

/// 命令行界面。
#[derive(Debug, Parser)]
#[command(
    name = "shunkashuto",
    version,
    about = "春・夏・秋・冬 —— 纯终端字符渲染的实时 MV",
    long_about = None
)]
pub struct Cli {
    /// 音频文件（MP3 / WAV / FLAC / OGG）。不提供则合成 132 BPM 测试曲
    #[arg(long)]
    pub audio: Option<PathBuf>,

    /// 歌词文件（LRC）。不提供则尝试与音频同名的 .lrc
    #[arg(long)]
    pub lrc: Option<PathBuf>,

    /// 配置文件（TOML）
    #[arg(long)]
    pub config: Option<PathBuf>,

    /// 时间轴文件（TOML）
    #[arg(long)]
    pub timeline: Option<PathBuf>,

    /// 整体时间偏移（秒）。正数表示画面提前
    #[arg(long, default_value_t = 0.0)]
    pub offset: f64,

    /// 渲染模式
    #[arg(long, value_enum, default_value_t = RenderMode::Auto)]
    pub mode: RenderMode,

    /// 目标帧率
    #[arg(long, default_value_t = 60.0)]
    pub fps: f64,

    /// 强制画布宽（字符列）。缺省取终端尺寸
    #[arg(long)]
    pub width: Option<u16>,

    /// 强制画布高（字符行）。缺省取终端尺寸
    #[arg(long)]
    pub height: Option<u16>,

    /// 只渲染前 N 秒（调试用）
    #[arg(long)]
    pub duration: Option<f64>,

    /// 音频时钟补偿（毫秒）。正值表示画面延后
    #[arg(long, default_value_t = 0.0)]
    pub latency_comp: f64,

    /// 随机种子
    #[arg(long, default_value_t = 0x5EED_1077)]
    pub seed: u64,

    /// 分轨目录（vocals.wav / drums.wav / bass.wav / other.wav）
    #[arg(long)]
    pub stems_dir: Option<PathBuf>,

    /// 导出帧序列到目录
    #[arg(long, value_name = "DIR")]
    pub export_frames: Option<PathBuf>,

    /// 导出带音轨的 MP4
    #[arg(long, value_name = "PATH")]
    pub export_video: Option<PathBuf>,

    /// 不播放声音（导出或离线调试）
    #[arg(long)]
    pub no_audio: bool,

    /// 文字自检：把这些时间点（秒，逗号分隔）的画面按**字符网格**打到 stdout 后退出。
    ///
    /// 终端艺术没法靠肉眼看截图自检 —— 它本身就是字符，
    /// 直接把字符读出来才是唯一可信的检查方式（也绕开了视觉模型）。
    #[arg(long, value_name = "T1,T2,...")]
    pub dump_text: Option<String>,

    /// 子命令
    #[command(subcommand)]
    pub command: Option<Command>,
}

/// 附加子命令。
#[derive(Debug, Clone, clap::Subcommand)]
pub enum Command {
    /// 用能量新颖度分析音频，输出建议的场景切分点
    Analyze {
        /// 建议切分点的最小间隔（秒）
        #[arg(long, default_value_t = 4.0)]
        min_gap: f64,
    },
    /// 打印解析后的配置与时间轴，不做渲染
    Info,
}

/// `config.toml` 的磁盘表示。所有字段可选。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FileConfig {
    pub audio: Option<PathBuf>,
    pub lrc: Option<PathBuf>,
    pub timeline: Option<PathBuf>,
    pub mode: Option<RenderMode>,
    pub fps: Option<f64>,
    pub offset: Option<f64>,
    pub latency_comp: Option<f64>,
    pub seed: Option<u64>,
    pub stems_dir: Option<PathBuf>,
    pub log_file: Option<PathBuf>,
}

/// 运行期最终配置。
#[derive(Debug, Clone)]
pub struct Config {
    pub audio: Option<PathBuf>,
    pub lrc: Option<PathBuf>,
    pub timeline: Option<PathBuf>,
    pub mode: RenderMode,
    pub fps: f64,
    pub width: Option<u16>,
    pub height: Option<u16>,
    pub duration: Option<f64>,
    pub offset: f64,
    pub latency_comp: f64,
    pub seed: u64,
    pub stems_dir: Option<PathBuf>,
    pub export_frames: Option<PathBuf>,
    pub export_video: Option<PathBuf>,
    pub no_audio: bool,
    pub command: Option<Command>,
    pub log_file: PathBuf,
}

impl Config {
    /// 合并 CLI 与配置文件。
    pub fn load(cli: &Cli) -> Result<Self> {
        let file = match &cli.config {
            Some(p) => {
                let text = std::fs::read_to_string(p)
                    .with_context(|| format!("读取配置文件失败: {}", p.display()))?;
                toml::from_str::<FileConfig>(&text)
                    .with_context(|| format!("解析配置文件失败: {}", p.display()))?
            }
            None => FileConfig::default(),
        };

        let pick_path = |a: &Option<PathBuf>, b: &Option<PathBuf>| a.clone().or_else(|| b.clone());
        let pick_f64 = |a: Option<f64>, b: Option<f64>| a.or(b);

        let log_file = file
            .log_file
            .clone()
            .unwrap_or_else(|| PathBuf::from("shunkashuto.log"));

        Ok(Config {
            audio: pick_path(&cli.audio, &file.audio),
            lrc: pick_path(&cli.lrc, &file.lrc),
            timeline: pick_path(&cli.timeline, &file.timeline),
            mode: if cli.mode == RenderMode::Auto {
                file.mode.unwrap_or(RenderMode::Auto)
            } else {
                cli.mode
            },
            fps: pick_f64(Some(cli.fps), file.fps)
                .unwrap_or(60.0)
                .clamp(1.0, 240.0),
            width: cli.width,
            height: cli.height,
            duration: cli.duration,
            offset: pick_f64(Some(cli.offset), file.offset).unwrap_or(0.0),
            latency_comp: pick_f64(Some(cli.latency_comp), file.latency_comp).unwrap_or(0.0),
            seed: file.seed.unwrap_or(cli.seed),
            stems_dir: pick_path(&cli.stems_dir, &file.stems_dir),
            export_frames: cli.export_frames.clone(),
            export_video: cli.export_video.clone(),
            no_audio: cli.no_audio,
            command: cli.command.clone(),
            log_file,
        })
    }

    /// 该帧的音频时间（秒）：音频时钟 + 用户偏移 + 延迟补偿。
    pub fn frame_time(&self, audio_pos: f64) -> f64 {
        audio_pos + self.offset - self.latency_comp / 1000.0
    }
}

/// 初始化 tracing，日志写文件（TUI 运行时 stdout/stderr 是画面）。
pub fn init_logging(cfg: &Config) {
    use tracing_subscriber::fmt::writer::MakeWriterExt;

    let path = cfg.log_file.clone();
    if let Some(dir) = path.parent() {
        if !dir.as_os_str().is_empty() {
            let _ = std::fs::create_dir_all(dir);
        }
    }

    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path);

    match file {
        Ok(f) => {
            let writer = std::sync::Mutex::new(f);
            let subscriber = tracing_subscriber::fmt()
                .with_writer(writer.with_max_level(tracing::Level::TRACE))
                .with_ansi(false)
                .with_target(true)
                .finish();
            let _ = tracing::subscriber::set_global_default(subscriber);
            tracing::info!(log = %path.display(), "logging started");
        }
        Err(_) => {
            // 打不开日志文件时静默降级为无日志，绝不污染 stdout。
        }
    }
}

/// 探测终端是否支持真彩色。
pub fn detect_true_color() -> bool {
    if let Ok(v) = std::env::var("COLORTERM") {
        let v = v.to_ascii_lowercase();
        if v.contains("truecolor") || v.contains("24bit") {
            return true;
        }
    }
    match std::env::var("TERM") {
        Ok(t) => t.contains("256color") || t.contains("kitty") || t.contains("direct"),
        Err(_) => false,
    }
}

/// 若 `lrc` 未显式给出，尝试找与音频同名的 `.lrc`。
pub fn sibling_lrc(audio: &Path) -> Option<PathBuf> {
    let cand = audio.with_extension("lrc");
    if cand.exists() {
        Some(cand)
    } else {
        None
    }
}

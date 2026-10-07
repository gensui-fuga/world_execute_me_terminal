//! 视频封装：调用 ffmpeg 把帧序列 + 音轨合成 MP4 / GIF。
//!
//! 不内嵌编码器 —— 纯 Rust 写 H.264 编码器不现实。
//! 检测不到 ffmpeg 时给出**可执行的**修复提示，而不是一句 "command not found"。

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};

/// 在 PATH 里找 ffmpeg。
pub fn find_ffmpeg() -> Option<PathBuf> {
    // 允许用户指定
    if let Ok(p) = std::env::var("WEM_FFMPEG") {
        let pb = PathBuf::from(&p);
        if pb.is_file() {
            return Some(pb);
        }
    }
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let cand = dir.join("ffmpeg");
        if cand.is_file() {
            return Some(cand);
        }
    }
    None
}

/// 检查 ffmpeg 是否可用，不可用则给出安装提示。
pub fn require_ffmpeg() -> Result<PathBuf> {
    match find_ffmpeg() {
        Some(p) => Ok(p),
        None => bail!(
            "未找到 ffmpeg。\n\
             视频封装需要它（本项目不内嵌编码器）。\n\
             安装：\n\
             \x20 Arch:   sudo pacman -S ffmpeg\n\
             \x20 Debian: sudo apt install ffmpeg\n\
             \x20 macOS:  brew install ffmpeg\n\
             或设置环境变量 WEM_FFMPEG 指向 ffmpeg 可执行文件。\n\
             提示：也可以只用 --export-frames 导出 PNG 序列，再用自己的工具合成。"
        ),
    }
}

/// 运行一个命令，失败时带上完整命令行与 stderr。
fn run(cmd: &mut Command) -> Result<()> {
    let debug = format!("{cmd:?}");
    let out = cmd
        .output()
        .with_context(|| format!("执行命令失败: {debug}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let tail: String = err.lines().rev().take(12).collect::<Vec<_>>().join("\n");
        bail!(
            "ffmpeg 返回非零状态 {}\n命令: {debug}\nstderr(尾部):\n{tail}",
            out.status
        );
    }
    Ok(())
}

/// 编码参数。
#[derive(Debug, Clone)]
pub struct EncodeOptions {
    /// 帧率
    pub fps: f64,
    /// 输出宽（像素）
    pub width: u32,
    /// 输出高（像素）
    pub height: u32,
    /// 恒定质量（CRF，越小越清晰，18~28 合理）
    pub crf: u32,
    /// 预设（ultrafast..veryslow）
    pub preset: String,
    /// 音频码率
    pub audio_bitrate: String,
    /// 音频文件（`None` 表示无声）
    pub audio: Option<PathBuf>,
}

impl Default for EncodeOptions {
    fn default() -> Self {
        Self {
            fps: 60.0,
            width: 0,
            height: 0,
            crf: 20,
            preset: "medium".to_string(),
            audio_bitrate: "192k".to_string(),
            audio: None,
        }
    }
}

/// 把帧序列编码成 H.264 + AAC 的 MP4。
///
/// `pattern` 形如 `/path/frame_%06d.png`。
pub fn encode_mp4(pattern: &str, output: &Path, opts: &EncodeOptions) -> Result<()> {
    let ffmpeg = require_ffmpeg()?;
    if let Some(dir) = output.parent() {
        if !dir.as_os_str().is_empty() {
            std::fs::create_dir_all(dir)
                .with_context(|| format!("创建输出目录失败: {}", dir.display()))?;
        }
    }

    let mut cmd = Command::new(&ffmpeg);
    cmd.arg("-y")
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        // 输入 0：帧序列
        .arg("-framerate")
        .arg(format!("{}", opts.fps))
        .arg("-i")
        .arg(pattern);

    // 输入 1：音轨（可选）
    let has_audio = match &opts.audio {
        Some(a) if a.exists() => {
            cmd.arg("-i").arg(a);
            true
        }
        Some(a) => {
            tracing::warn!(audio = %a.display(), "音频文件不存在，将导出无声视频");
            false
        }
        None => false,
    };

    cmd.arg("-c:v").arg("libx264");
    cmd.arg("-pix_fmt").arg("yuv420p");
    cmd.arg("-crf").arg(opts.crf.to_string());
    cmd.arg("-preset").arg(&opts.preset);
    // 偶数尺寸是 yuv420p 的硬要求
    if opts.width > 0 && opts.height > 0 {
        cmd.arg("-vf")
            .arg(format!("scale={}:{}", opts.width & !1, opts.height & !1));
    }

    if has_audio {
        cmd.arg("-c:a").arg("aac");
        cmd.arg("-b:a").arg(&opts.audio_bitrate);
        cmd.arg("-shortest");
    }

    cmd.arg("-movflags").arg("+faststart");
    cmd.arg(output);

    tracing::info!(output = %output.display(), "开始编码 MP4");
    run(&mut cmd)?;
    Ok(())
}

/// 把帧序列编码成 GIF（两遍调色板，画质远好于单遍）。
pub fn encode_gif(pattern: &str, output: &Path, fps: f64, width: u32) -> Result<()> {
    let ffmpeg = require_ffmpeg()?;
    let scale = if width > 0 {
        format!("scale={}:-1:flags=lanczos", width & !1)
    } else {
        "scale=trunc(iw/2)*2:trunc(ih/2)*2:flags=lanczos".to_string()
    };
    let filter = format!(
        "{scale},split[s0][s1];[s0]palettegen=max_colors=256[p];[s1][p]paletteuse=dither=bayer:bayer_scale=3"
    );
    let mut cmd = Command::new(&ffmpeg);
    cmd.arg("-y")
        .arg("-hide_banner")
        .arg("-loglevel")
        .arg("error")
        .arg("-framerate")
        .arg(format!("{fps}"))
        .arg("-i")
        .arg(pattern)
        .arg("-vf")
        .arg(filter)
        .arg("-loop")
        .arg("0")
        .arg(output);
    tracing::info!(output = %output.display(), "开始编码 GIF");
    run(&mut cmd)
}

/// 探测 ffmpeg 版本（用于 README 记录实际使用的版本）。
pub fn ffmpeg_version() -> Result<String> {
    let ffmpeg = require_ffmpeg()?;
    let out = Command::new(&ffmpeg)
        .arg("-version")
        .output()
        .context("执行 ffmpeg -version 失败")?;
    let s = String::from_utf8_lossy(&out.stdout);
    Ok(s.lines().next().unwrap_or("unknown").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_ffmpeg_never_panics() {
        let _ = find_ffmpeg();
    }

    #[test]
    fn require_ffmpeg_error_message_is_actionable() {
        // 只有在没有 ffmpeg 的环境里才检查提示文本
        if find_ffmpeg().is_none() {
            let e = require_ffmpeg().unwrap_err().to_string();
            assert!(e.contains("pacman"), "错误信息应包含安装命令");
            assert!(e.contains("--export-frames"), "错误信息应给出替代方案");
        }
    }

    #[test]
    fn default_options_are_sane() {
        let o = EncodeOptions::default();
        assert!(o.fps >= 24.0);
        assert!((18..=28).contains(&o.crf));
        assert!(!o.preset.is_empty());
    }

    #[test]
    fn encode_mp4_creates_output_dir_first() {
        // 用一个不存在的输出目录，且不存在的帧序列 ——
        // 应该先在「创建目录」或「ffmpeg 失败」处报错，而不是 panic
        let out = std::env::temp_dir()
            .join("wem_video_test_dir")
            .join("nested")
            .join("out.mp4");
        let _ = std::fs::remove_dir_all(out.parent().unwrap());
        let r = encode_mp4(
            "/nonexistent/frame_%06d.png",
            &out,
            &EncodeOptions::default(),
        );
        // 没有 ffmpeg 时是 Err（缺依赖）；有 ffmpeg 时也是 Err（输入不存在）
        assert!(r.is_err());
        if find_ffmpeg().is_some() {
            assert!(out.parent().unwrap().exists(), "应已创建输出目录");
        }
        let _ = std::fs::remove_dir_all(std::env::temp_dir().join("wem_video_test_dir"));
    }

    #[test]
    fn run_reports_command_on_failure() {
        // 用一个必定失败的命令验证错误信息包含命令行
        let mut cmd = Command::new("false");
        let e = run(&mut cmd).unwrap_err().to_string();
        assert!(e.contains("false"));
    }

    #[test]
    fn ffmpeg_version_is_reported_when_available() {
        if find_ffmpeg().is_some() {
            let v = ffmpeg_version().unwrap();
            assert!(v.to_lowercase().contains("ffmpeg"));
        }
    }
}

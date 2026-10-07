//! 离线帧序列导出。
//!
//! 与实时播放共用同一套 `FeatureTrack` 与场景代码，
//! 区别只是时钟：实时用音频时钟，导出用 `帧号 / fps`。
//! 因此导出结果与终端所见一致（除了终端分辨率与刷新率的差别）。

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::export::raster::Rasterizer;

/// 帧序列导出器。
pub struct FrameExporter {
    dir: PathBuf,
    raster: Rasterizer,
    /// 已写出的帧数
    written: u64,
}

impl FrameExporter {
    /// 新建，并确保输出目录存在。
    pub fn new(dir: &Path, cell_w: u32, cell_h: u32) -> Result<Self> {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("创建帧目录失败: {}", dir.display()))?;
        Ok(Self {
            dir: dir.to_path_buf(),
            raster: Rasterizer::new(cell_w, cell_h),
            written: 0,
        })
    }

    /// 输出目录。
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// 已写出的帧数。
    pub fn written(&self) -> u64 {
        self.written
    }

    /// 使用的字体路径（可能为 `None`）。
    pub fn font_path(&self) -> Option<&str> {
        self.raster.font_path()
    }

    /// 中日韩回退字体路径（可能为 `None`）。
    pub fn fallback_path(&self) -> Option<&str> {
        self.raster.fallback_path()
    }

    /// 单元像素尺寸。
    pub fn cell_size(&self) -> (u32, u32) {
        (self.raster.cell_w, self.raster.cell_h)
    }

    /// 帧文件名（ffmpeg 可直接吃 `frame_%06d.png`）。
    pub fn frame_name(index: u64) -> String {
        format!("frame_{index:06}.png")
    }

    /// 写出第 `index` 帧。
    pub fn write(&mut self, index: u64, buf: &Buffer, area: Rect) -> Result<PathBuf> {
        let img = self.raster.render(buf, area);
        let path = self.dir.join(Self::frame_name(index));

        // 显式指定编码参数：默认压缩级别跑满 CPU，
        // 而字符画是大片纯色 + 稀疏前景，Fast + Up 滤波体积几乎不变、速度快数倍。
        let file = std::fs::File::create(&path)
            .with_context(|| format!("创建帧文件失败: {}", path.display()))?;
        let mut w = std::io::BufWriter::new(file);
        let encoder = image::codecs::png::PngEncoder::new_with_quality(
            &mut w,
            image::codecs::png::CompressionType::Fast,
            image::codecs::png::FilterType::Up,
        );
        image::ImageEncoder::write_image(
            encoder,
            img.as_raw(),
            img.width(),
            img.height(),
            image::ExtendedColorType::Rgba8,
        )
        .with_context(|| format!("写入帧失败: {}", path.display()))?;

        self.written += 1;
        Ok(path)
    }

    /// 帧序列的 ffmpeg 输入模板。
    pub fn pattern(&self) -> String {
        self.dir
            .join("frame_%06d.png")
            .to_string_lossy()
            .to_string()
    }
}

/// 从目录里统计已有的 PNG 帧数量（用于断点续导/校验）。
pub fn count_frames(dir: &Path) -> Result<u64> {
    let mut n = 0u64;
    for e in std::fs::read_dir(dir).with_context(|| format!("读取帧目录失败: {}", dir.display()))?
    {
        let e = e?;
        let name = e.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("frame_") && name.ends_with(".png") {
            n += 1;
        }
    }
    Ok(n)
}

/// 校验帧序列是否连续（返回第一个缺口，若无缺口返回 `None`）。
pub fn first_gap(dir: &Path) -> Result<Option<u64>> {
    let mut idx: Vec<u64> = Vec::new();
    for e in std::fs::read_dir(dir).with_context(|| format!("读取帧目录失败: {}", dir.display()))?
    {
        let e = e?;
        let name = e.file_name();
        let name = name.to_string_lossy().to_string();
        if let Some(rest) = name.strip_prefix("frame_") {
            if let Some(num) = rest.strip_suffix(".png") {
                if let Ok(v) = num.parse::<u64>() {
                    idx.push(v);
                }
            }
        }
    }
    if idx.is_empty() {
        return Ok(None);
    }
    idx.sort_unstable();
    for (i, v) in idx.iter().enumerate() {
        if *v != i as u64 {
            return Ok(Some(i as u64));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("shunkashuto_frames_test_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        p
    }

    #[test]
    fn frame_name_is_zero_padded() {
        assert_eq!(FrameExporter::frame_name(0), "frame_000000.png");
        assert_eq!(FrameExporter::frame_name(1234), "frame_001234.png");
    }

    #[test]
    fn pattern_contains_printf_placeholder() {
        let dir = tmpdir("pattern");
        let ex = FrameExporter::new(&dir, 8, 16).unwrap();
        assert!(ex.pattern().contains("frame_%06d.png"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn new_creates_directory() {
        let dir = tmpdir("mkdir");
        assert!(!dir.exists());
        let _ = FrameExporter::new(&dir, 8, 16).unwrap();
        assert!(dir.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_produces_png_of_expected_size() {
        let dir = tmpdir("write");
        let mut ex = FrameExporter::new(&dir, 8, 16).unwrap();
        let area = Rect::new(0, 0, 4, 2);
        let buf = Buffer::empty(area);
        let p = ex.write(0, &buf, area).unwrap();
        assert!(p.exists());
        let img = image::open(&p).unwrap();
        assert_eq!((img.width(), img.height()), (32, 32));
        assert_eq!(ex.written(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn count_frames_counts_only_png_frames() {
        let dir = tmpdir("count");
        let mut ex = FrameExporter::new(&dir, 8, 16).unwrap();
        let area = Rect::new(0, 0, 2, 2);
        let buf = Buffer::empty(area);
        for i in 0..5 {
            ex.write(i, &buf, area).unwrap();
        }
        std::fs::write(dir.join("other.txt"), "x").unwrap();
        assert_eq!(count_frames(&dir).unwrap(), 5);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn first_gap_detects_missing_index() {
        let dir = tmpdir("gap");
        let mut ex = FrameExporter::new(&dir, 8, 16).unwrap();
        let area = Rect::new(0, 0, 2, 2);
        let buf = Buffer::empty(area);
        ex.write(0, &buf, area).unwrap();
        ex.write(1, &buf, area).unwrap();
        ex.write(3, &buf, area).unwrap();
        assert_eq!(first_gap(&dir).unwrap(), Some(2));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn first_gap_returns_none_for_contiguous() {
        let dir = tmpdir("nogap");
        let mut ex = FrameExporter::new(&dir, 8, 16).unwrap();
        let area = Rect::new(0, 0, 2, 2);
        let buf = Buffer::empty(area);
        for i in 0..4 {
            ex.write(i, &buf, area).unwrap();
        }
        assert_eq!(first_gap(&dir).unwrap(), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn first_gap_on_empty_dir_is_none() {
        let dir = tmpdir("empty");
        let _ = FrameExporter::new(&dir, 8, 16).unwrap();
        assert_eq!(first_gap(&dir).unwrap(), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cell_size_is_reported() {
        let dir = tmpdir("cell");
        let ex = FrameExporter::new(&dir, 10, 20).unwrap();
        assert_eq!(ex.cell_size(), (10, 20));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn count_frames_on_missing_dir_is_error() {
        let p = PathBuf::from("/nonexistent/wem/frames");
        assert!(count_frames(&p).is_err());
    }
}

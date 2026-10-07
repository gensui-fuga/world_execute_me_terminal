//! 导出子系统：字符 → 位图 → 帧序列 → 视频。
//!
//! 分三层，可单独使用：
//! - [`raster`]：`ratatui::Buffer` → `RgbaImage`（Braille 走几何绘制，不依赖字体）
//! - [`frames`]：逐帧写 PNG，带连续性校验
//! - [`video`]：调 ffmpeg 封装 MP4 / GIF

pub mod frames;
pub mod raster;
pub mod tui;
pub mod video;

// 这些是导出子系统的公开 API 表面，主流程只用到其中一部分。
#[allow(unused_imports)]
pub use frames::{count_frames, first_gap, FrameExporter};
#[allow(unused_imports)]
pub use raster::{load_font, Rasterizer};
pub use video::{encode_gif, encode_mp4, ffmpeg_version, find_ffmpeg, EncodeOptions};

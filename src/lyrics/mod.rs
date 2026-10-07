//! 歌词子系统。
//!
//! 只做**解析与时序**，不内置任何真实歌词文本 ——
//! 版权内容由使用者自备（`--lrc` 或与音频同名的 `.lrc`）。
//! 仓库里的 `assets/world_execute_me.lrc` 只是占位示例。

pub mod lrc;
pub mod motif;
pub mod words;

#[allow(unused_imports)]
pub use lrc::{LyricLine, Lyrics};

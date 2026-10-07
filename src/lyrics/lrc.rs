//! LRC 歌词解析。
//!
//! 支持的形式：
//! ```text
//! [ti:标题]                  ← 元数据，忽略
//! [00:12.34]一句歌词
//! [00:15.00][00:18.00]重复句  ← 一个文本多个时间戳
//! [01:02]无小数
//! ```
//!
//! 解析后按时间升序排序；同一时间的多行保持原始顺序。

use std::path::Path;

use anyhow::{Context, Result};

/// 一行歌词。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LyricLine {
    /// 起始时间（秒）
    pub time: f64,
    /// 文本
    pub text: String,
}

impl LyricLine {
    /// 新建。
    pub fn new(time: f64, text: impl Into<String>) -> Self {
        Self {
            time,
            text: text.into(),
        }
    }
}

/// 整首歌词。
#[derive(Debug, Clone, Default)]
pub struct Lyrics {
    /// 按时间升序
    pub lines: Vec<LyricLine>,
}

impl Lyrics {
    /// 是否没有内容。
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// 行数。
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    /// 从 LRC 文本解析。
    pub fn parse(text: &str) -> Self {
        let mut lines: Vec<LyricLine> = Vec::new();

        for raw in text.lines() {
            let raw = raw.trim_end_matches(['\r', '\n']);
            if raw.trim().is_empty() {
                continue;
            }
            let mut rest = raw.trim_start();
            let mut stamps: Vec<f64> = Vec::new();

            // 连续吃掉开头的 [..] 标签
            while rest.starts_with('[') {
                let Some(close) = rest.find(']') else {
                    break;
                };
                let inner = &rest[1..close];
                match parse_timestamp(inner) {
                    Some(t) => {
                        stamps.push(t);
                        rest = rest[close + 1..].trim_start();
                    }
                    None => {
                        // 元数据标签（ar/ti/al/by/offset…）或无法识别 —— 跳过这个标签
                        rest = rest[close + 1..].trim_start();
                        if stamps.is_empty() {
                            // 整行都是元数据，直接丢弃
                            break;
                        }
                    }
                }
            }

            if stamps.is_empty() {
                continue;
            }
            let content = rest.trim().to_string();
            if content.is_empty() {
                continue;
            }
            for t in stamps {
                lines.push(LyricLine::new(t, content.clone()));
            }
        }

        // 稳定排序：同一时间的行保持出现顺序
        lines.sort_by(|a, b| {
            a.time
                .partial_cmp(&b.time)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Self { lines }
    }

    /// 从文件加载。
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("读取歌词失败: {}", path.display()))?;
        Ok(Self::parse(&text))
    }

    /// 当前时间所在行的下标。
    ///
    /// 时间早于第一行时返回 `None`。
    pub fn index_at(&self, t: f64) -> Option<usize> {
        if self.lines.is_empty() || t < self.lines[0].time {
            return None;
        }
        // 二分找最后一个 time <= t
        let mut lo = 0usize;
        let mut hi = self.lines.len();
        while lo + 1 < hi {
            let mid = (lo + hi) / 2;
            if self.lines[mid].time <= t {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        Some(lo)
    }

    /// 当前行。
    pub fn current(&self, t: f64) -> Option<&LyricLine> {
        self.index_at(t).map(|i| &self.lines[i])
    }

    /// 下一行。
    pub fn next(&self, t: f64) -> Option<&LyricLine> {
        let i = self.index_at(t)?;
        self.lines.get(i + 1)
    }

    /// 当前行内的进度（0..1）。无当前行时返回 0。
    pub fn progress_in_line(&self, t: f64) -> f32 {
        let Some(i) = self.index_at(t) else {
            return 0.0;
        };
        let start = self.lines[i].time;
        let end = self.lines.get(i + 1).map(|l| l.time).unwrap_or(start + 4.0);
        let span = (end - start).max(1e-3);
        (((t - start) / span).clamp(0.0, 1.0)) as f32
    }

    /// 距离当前行出现已经过去多久（秒）。
    pub fn age_of_current(&self, t: f64) -> f64 {
        match self.index_at(t) {
            Some(i) => t - self.lines[i].time,
            None => f64::INFINITY,
        }
    }

    /// 占位歌词：**不含任何真实歌词内容**，仅用于演示与自测。
    ///
    /// 使用者应通过 `--lrc` 提供真实歌词文件。
    pub fn placeholder() -> Self {
        let entries = [
            (0.0, "[ 占位歌词 · placeholder ]"),
            (4.0, "提供 --lrc 或与音频同名的 .lrc 文件"),
            (9.0, "本仓库不分发任何歌词文本"),
            (14.0, "演示用占位行 A"),
            (19.0, "演示用占位行 B"),
            (24.0, "演示用占位行 C"),
        ];
        Self {
            lines: entries
                .iter()
                .map(|(t, s)| LyricLine::new(*t, *s))
                .collect(),
        }
    }
}

/// 解析 `[mm:ss.xx]` 里的 `mm:ss.xx` 部分。失败返回 `None`。
fn parse_timestamp(s: &str) -> Option<f64> {
    let s = s.trim();
    // 元数据标签形如 `ti:xxx`，含冒号但不是时间
    let (mm, rest) = s.split_once(':')?;
    let mm: f64 = mm.trim().parse().ok()?;
    let rest = rest.trim();
    // 允许 mm:ss 或 mm:ss.xx / mm:ss:xx
    let (ss, frac) = if let Some((a, b)) = rest.split_once('.') {
        (a, Some(b))
    } else if let Some((a, b)) = rest.split_once(':') {
        (a, Some(b))
    } else {
        (rest, None)
    };
    let ss: f64 = ss.trim().parse().ok()?;
    let frac_v = match frac {
        None => 0.0,
        Some(f) => {
            let f = f.trim();
            if f.is_empty() {
                0.0
            } else {
                let digits: String = f.chars().take_while(|c| c.is_ascii_digit()).collect();
                if digits.is_empty() {
                    0.0
                } else {
                    let v: f64 = digits.parse().ok()?;
                    v / 10f64.powi(digits.len() as i32)
                }
            }
        }
    };
    if mm < 0.0 || ss < 0.0 {
        return None;
    }
    Some(mm * 60.0 + ss + frac_v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_lines() {
        let l = Lyrics::parse("[00:01.00]A\n[00:02.50]B\n");
        assert_eq!(l.len(), 2);
        assert!((l.lines[0].time - 1.0).abs() < 1e-6);
        assert!((l.lines[1].time - 2.5).abs() < 1e-6);
        assert_eq!(l.lines[0].text, "A");
        assert_eq!(l.lines[1].text, "B");
    }

    #[test]
    fn parses_multiple_stamps_per_line() {
        let l = Lyrics::parse("[00:10.00][00:20.00]same\n");
        assert_eq!(l.len(), 2);
        assert_eq!(l.lines[0].text, "same");
        assert_eq!(l.lines[1].text, "same");
        assert!((l.lines[1].time - 20.0).abs() < 1e-6);
    }

    #[test]
    fn ignores_metadata_tags() {
        let l = Lyrics::parse("[ti:Title]\n[ar:Artist]\n[00:05.00]real\n");
        assert_eq!(l.len(), 1);
        assert_eq!(l.lines[0].text, "real");
    }

    #[test]
    fn skips_timestamp_only_lines() {
        let l = Lyrics::parse("[00:05.00]\n[00:06.00]text\n");
        assert_eq!(l.len(), 1);
        assert_eq!(l.lines[0].text, "text");
    }

    #[test]
    fn skips_blank_and_unstamped_lines() {
        let l = Lyrics::parse("\n\nno stamp here\n[00:01.00]ok\n");
        assert_eq!(l.len(), 1);
    }

    #[test]
    fn sorts_by_time() {
        let l = Lyrics::parse("[00:30.00]late\n[00:10.00]early\n");
        assert_eq!(l.lines[0].text, "early");
        assert_eq!(l.lines[1].text, "late");
    }

    #[test]
    fn handles_minutes_and_hour_scale() {
        let l = Lyrics::parse("[03:27.50]long\n");
        assert!((l.lines[0].time - 207.5).abs() < 1e-6);
    }

    #[test]
    fn handles_no_fraction() {
        let l = Lyrics::parse("[01:02]x\n");
        assert!((l.lines[0].time - 62.0).abs() < 1e-6);
    }

    #[test]
    fn handles_three_digit_fraction() {
        let l = Lyrics::parse("[00:01.250]x\n");
        assert!((l.lines[0].time - 1.25).abs() < 1e-6);
    }

    #[test]
    fn handles_colon_fraction_variant() {
        // 某些工具写成 [00:01:25]
        let l = Lyrics::parse("[00:01:25]x\n");
        assert!((l.lines[0].time - 1.25).abs() < 1e-6);
    }

    #[test]
    fn index_at_returns_last_line_at_or_before() {
        let l = Lyrics::parse("[00:01.00]A\n[00:05.00]B\n[00:09.00]C\n");
        assert_eq!(l.index_at(0.5), None);
        assert_eq!(l.index_at(1.0), Some(0));
        assert_eq!(l.index_at(4.9), Some(0));
        assert_eq!(l.index_at(5.0), Some(1));
        assert_eq!(l.index_at(100.0), Some(2));
    }

    #[test]
    fn current_and_next_agree() {
        let l = Lyrics::parse("[00:01.00]A\n[00:05.00]B\n");
        assert_eq!(l.current(2.0).unwrap().text, "A");
        assert_eq!(l.next(2.0).unwrap().text, "B");
        assert!(l.next(6.0).is_none());
    }

    #[test]
    fn progress_in_line_spans_to_next_line() {
        let l = Lyrics::parse("[00:00.00]A\n[00:04.00]B\n");
        assert!((l.progress_in_line(0.0) - 0.0).abs() < 1e-6);
        assert!((l.progress_in_line(2.0) - 0.5).abs() < 1e-6);
        assert!((l.progress_in_line(4.0) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn progress_before_first_line_is_zero() {
        let l = Lyrics::parse("[00:10.00]A\n");
        assert_eq!(l.progress_in_line(1.0), 0.0);
    }

    #[test]
    fn age_of_current_is_infinite_before_first() {
        let l = Lyrics::parse("[00:10.00]A\n");
        assert!(l.age_of_current(1.0).is_infinite());
        assert!((l.age_of_current(12.0) - 2.0).abs() < 1e-6);
    }

    #[test]
    fn empty_input_gives_empty_lyrics() {
        let l = Lyrics::parse("");
        assert!(l.is_empty());
        assert_eq!(l.index_at(5.0), None);
        assert!(l.current(5.0).is_none());
    }

    #[test]
    fn crlf_input_is_handled() {
        let l = Lyrics::parse("[00:01.00]A\r\n[00:02.00]B\r\n");
        assert_eq!(l.len(), 2);
        assert_eq!(l.lines[0].text, "A");
    }

    #[test]
    fn placeholder_has_content_and_is_sorted() {
        let p = Lyrics::placeholder();
        assert!(!p.is_empty());
        for w in p.lines.windows(2) {
            assert!(w[0].time <= w[1].time);
        }
        // 占位歌词不得包含真实歌词的已知首句
        for l in &p.lines {
            assert!(!l.text.to_lowercase().contains("power line"));
        }
    }

    #[test]
    fn timestamp_parser_rejects_garbage() {
        assert_eq!(parse_timestamp("ti:Title"), None);
        assert_eq!(parse_timestamp("abc"), None);
        assert_eq!(parse_timestamp(""), None);
    }

    #[test]
    fn unsorted_duplicate_times_keep_order() {
        let l = Lyrics::parse("[00:01.00]first\n[00:01.00]second\n");
        assert_eq!(l.lines[0].text, "first");
        assert_eq!(l.lines[1].text, "second");
    }
}

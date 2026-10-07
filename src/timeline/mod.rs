//! 时间轴：场景分段 + 关键事件。
//!
//! 分段与事件全部来自 `timeline.toml`（`--timeline` 指定），
//! 没有该文件时使用 [`Timeline::default_for`] 的内置结构。
//! **时间点从不写死在场景代码里**，换歌只需换时间轴。

pub mod beat;
pub mod chapters;
pub mod cues;
pub mod scene;

#[allow(unused_imports)]
pub use cues::{Cue, CueKind, CueTrack};
pub use scene::SceneKind;

use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// 一个场景分段。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Segment {
    /// 场景
    pub scene: SceneKind,
    /// 起始时间（秒）
    pub start: f64,
    /// 备注（可选）
    #[serde(default)]
    pub label: String,
}

impl Segment {
    /// 新建。
    pub fn new(scene: SceneKind, start: f64) -> Self {
        Self {
            scene,
            start,
            label: String::new(),
        }
    }

    /// 带备注。
    pub fn labeled(scene: SceneKind, start: f64, label: impl Into<String>) -> Self {
        Self {
            scene,
            start,
            label: label.into(),
        }
    }
}

/// `timeline.toml` 的磁盘表示。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TimelineFile {
    /// 场景分段
    pub segment: Vec<Segment>,
    /// 关键事件
    pub cue: Vec<Cue>,
}

/// 解析后的时间轴。
#[derive(Debug, Clone)]
pub struct Timeline {
    segments: Vec<Segment>,
    /// 关键事件
    pub cues: CueTrack,
    /// 总时长（秒），来自音频
    pub duration: f64,
}

impl Timeline {
    /// 用给定分段与事件构造（自动排序）。
    pub fn new(mut segments: Vec<Segment>, cues: Vec<Cue>, duration: f64) -> Self {
        segments.sort_by(|a, b| {
            a.start
                .partial_cmp(&b.start)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Self {
            segments,
            cues: CueTrack::new(cues),
            duration: duration.max(0.0),
        }
    }

    /// 内置默认时间轴。
    ///
    /// 结构锚点对齐《春・夏・秋・冬》的段落划分；若音频时长不同，
    /// 最后一段会自动拉伸到结尾。
    pub fn default_for(duration: f64) -> Self {
        // 10 段，锚点对齐歌曲实际结构（BPM≈92，全长 289.9s）。
        //
        //   0.0   序        晨雾未醒
        //  15.7   春        苔の産毛に日が射して
        //  43.3   夏        白雨来て蓮の葉叩ク
        //  68.4   桥        瞬き一ツスル間ニモ
        //  84.9   副歌      移ロフは空の気配と人の世
        // 118.6   秋        松ノ影畳ニ延ビテ
        // 157.9   冬        枯レ野原一夜ニシテ銀ノ花
        // 175.55  昇華      織リ成ス四季ハ絵巻物
        // 224.9   終章      記憶ノ中ノ美シサダケハ
        // 268.7   尾声      收卷归寂
        let anchors: [(SceneKind, f64, &str); 10] = [
            (SceneKind::Intro, 0.0, "mist"),
            (SceneKind::Spring, 15.7, "moss-and-dew"),
            (SceneKind::Summer, 43.3, "white-rain"),
            (SceneKind::Bridge, 68.4, "blink-phantom"),
            (SceneKind::Chorus, 84.9, "flowing-world"),
            (SceneKind::Autumn, 118.6, "pine-moon"),
            (SceneKind::Winter, 157.9, "silver-flowers"),
            (SceneKind::Ascension, 175.55, "picture-scroll"),
            (SceneKind::Finale, 224.9, "memory-rust"),
            (SceneKind::Outro, 268.7, "close-scroll"),
        ];
        let segments = anchors
            .iter()
            .map(|(s, t, l)| Segment::labeled(*s, *t, *l))
            .collect();
        let cues = CueTrack::default_track(duration).all().to_vec();
        Self::new(segments, cues, duration)
    }

    /// 从文件加载。
    pub fn load(path: &Path, duration: f64) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("读取时间轴失败: {}", path.display()))?;
        let file: TimelineFile =
            toml::from_str(&text).with_context(|| format!("解析时间轴失败: {}", path.display()))?;

        if file.segment.is_empty() {
            anyhow::bail!("时间轴 {} 里没有任何 [[segment]]", path.display());
        }
        Ok(Self::new(file.segment, file.cue, duration))
    }

    /// 分段数量。
    pub fn len(&self) -> usize {
        self.segments.len()
    }

    /// 是否为空（永远非空）。
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    /// 全部分段。
    pub fn segments(&self) -> &[Segment] {
        &self.segments
    }

    /// 第 `i` 段的结束时间。
    pub fn end_of(&self, i: usize) -> f64 {
        if i + 1 < self.segments.len() {
            self.segments[i + 1].start
        } else {
            self.duration.max(self.segments[i].start)
        }
    }

    /// 第 `i` 段的时长。
    pub fn duration_of(&self, i: usize) -> f64 {
        (self.end_of(i) - self.segments[i].start).max(0.0)
    }

    /// `t` 时刻所在的分段下标。
    pub fn index_at(&self, t: f64) -> usize {
        if self.segments.is_empty() {
            return 0;
        }
        // 找最后一个 start <= t
        let mut lo = 0usize;
        for (i, s) in self.segments.iter().enumerate() {
            if s.start <= t {
                lo = i;
            } else {
                break;
            }
        }
        lo
    }

    /// `t` 时刻所在的分段。
    pub fn segment_at(&self, t: f64) -> &Segment {
        &self.segments[self.index_at(t).min(self.segments.len() - 1)]
    }

    /// `t` 时刻的场景。
    pub fn scene_at(&self, t: f64) -> SceneKind {
        self.segment_at(t).scene
    }

    /// 段内已过去的时间（秒）。
    pub fn time_in_segment(&self, t: f64) -> f64 {
        let i = self.index_at(t);
        (t - self.segments[i].start).max(0.0)
    }

    /// 段内进度 0..1。
    pub fn progress_in_segment(&self, t: f64) -> f32 {
        let i = self.index_at(t);
        let d = self.duration_of(i);
        if d <= 1e-6 {
            return 1.0;
        }
        ((t - self.segments[i].start) / d).clamp(0.0, 1.0) as f32
    }

    /// 是否处于最后一段的后半部分（用于收尾动画）。
    pub fn is_ending(&self, t: f64, window: f64) -> bool {
        self.duration > 0.0 && t >= self.duration - window
    }

    /// 导出为 `timeline.toml` 文本（供 `--analyze` 输出建议）。
    pub fn to_toml(&self) -> Result<String> {
        let file = TimelineFile {
            segment: self.segments.clone(),
            cue: self.cues.all().to_vec(),
        };
        toml::to_string_pretty(&file).context("序列化时间轴失败")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_timeline_has_ten_segments() {
        let t = Timeline::default_for(289.9);
        assert_eq!(t.len(), 10);
        assert_eq!(t.segments()[0].scene, SceneKind::Intro);
        assert_eq!(t.segments()[1].scene, SceneKind::Spring);
        assert_eq!(t.segments()[9].scene, SceneKind::Outro);
    }

    #[test]
    fn default_timeline_covers_every_section_of_the_song() {
        // 段落锚点必须覆盖四季的全部关键段落。
        let t = Timeline::default_for(289.9);
        assert_eq!(t.scene_at(15.7), SceneKind::Spring, "15.7s 应进入春");
        assert_eq!(t.scene_at(47.8), SceneKind::Summer, "47.8s 应是夏");
        assert_eq!(t.scene_at(70.0), SceneKind::Bridge, "70s 应是桥");
        assert_eq!(t.scene_at(90.0), SceneKind::Chorus, "90s 应是副歌");
        assert_eq!(t.scene_at(120.0), SceneKind::Autumn, "120s 应是秋");
        assert_eq!(t.scene_at(160.0), SceneKind::Winter, "160s 应是冬");
        assert_eq!(t.scene_at(190.0), SceneKind::Ascension, "190s 应是昇華");
        assert_eq!(t.scene_at(240.0), SceneKind::Finale, "240s 应是終章");
    }

    #[test]
    fn default_timeline_is_sorted() {
        let t = Timeline::default_for(289.9);
        for w in t.segments().windows(2) {
            assert!(w[0].start <= w[1].start);
        }
    }

    #[test]
    fn segment_lookup_hits_expected_ranges() {
        let t = Timeline::default_for(289.9);
        assert_eq!(t.scene_at(0.0), SceneKind::Intro);
        assert_eq!(t.scene_at(15.6), SceneKind::Intro);
        assert_eq!(t.scene_at(15.7), SceneKind::Spring);
        assert_eq!(t.scene_at(43.3), SceneKind::Summer);
        assert_eq!(t.scene_at(118.6), SceneKind::Autumn);
        assert_eq!(t.scene_at(280.0), SceneKind::Outro);
        assert_eq!(t.scene_at(1e9), SceneKind::Outro);
    }

    #[test]
    fn negative_time_lands_on_first_segment() {
        let t = Timeline::default_for(289.9);
        assert_eq!(t.index_at(-10.0), 0);
    }

    #[test]
    fn last_segment_extends_to_duration() {
        let t = Timeline::default_for(289.9);
        // 用「最后一段」而不是写死的下标 —— 段落数会随分镜调整，
        // 写死下标会让每次扩段落都误报失败。
        let last = t.len() - 1;
        assert!((t.end_of(last) - 289.9).abs() < 1e-6);
        assert!(t.duration_of(last) > 5.0);
    }

    #[test]
    fn time_in_segment_is_relative() {
        let t = Timeline::default_for(211.9);
        let start = t.segments()[1].start;
        assert!((t.time_in_segment(start) - 0.0).abs() < 1e-6);
        assert!((t.time_in_segment(start + 2.0) - 2.0).abs() < 1e-6);
    }

    #[test]
    fn progress_in_segment_spans_zero_to_one() {
        let t = Timeline::default_for(211.9);
        let start = t.segments()[1].start;
        let p0 = t.progress_in_segment(start);
        let mid = (start + t.end_of(1)) / 2.0;
        let pm = t.progress_in_segment(mid);
        assert!(p0 < 0.01);
        assert!((0.4..0.6).contains(&pm), "中点进度 {pm}");
    }

    #[test]
    fn ending_detection_works() {
        let t = Timeline::default_for(211.9);
        assert!(!t.is_ending(100.0, 6.0));
        assert!(t.is_ending(208.0, 6.0));
    }

    #[test]
    fn custom_timeline_parses_from_toml() {
        let text = r#"
[[segment]]
scene = "intro"
start = 0.0
label = "a"

[[segment]]
scene = "chorus"
start = 10.0

[[cue]]
time = 5.0
kind = "flash"
name = "hit"
"#;
        let file: TimelineFile = toml::from_str(text).unwrap();
        assert_eq!(file.segment.len(), 2);
        assert_eq!(file.cue.len(), 1);
        let t = Timeline::new(file.segment, file.cue, 30.0);
        assert_eq!(t.scene_at(0.0), SceneKind::Intro);
        assert_eq!(t.scene_at(12.0), SceneKind::Chorus);
        assert_eq!(t.cues.len(), 1);
    }

    #[test]
    fn toml_roundtrip_preserves_structure() {
        let t = Timeline::default_for(100.0);
        let text = t.to_toml().unwrap();
        let back: TimelineFile = toml::from_str(&text).unwrap();
        assert_eq!(back.segment.len(), t.len());
        assert_eq!(back.cue.len(), t.cues.len());
    }

    #[test]
    fn empty_segment_list_is_rejected_on_load() {
        let dir = std::env::temp_dir();
        let p = dir.join("wem_timeline_empty_test.toml");
        std::fs::write(&p, "# nothing\n").unwrap();
        let r = Timeline::load(&p, 100.0);
        assert!(r.is_err());
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn duplicate_starts_do_not_panic() {
        let t = Timeline::new(
            vec![
                Segment::new(SceneKind::Intro, 0.0),
                Segment::new(SceneKind::Spring, 0.0),
            ],
            vec![],
            10.0,
        );
        let _ = t.scene_at(0.0);
        let _ = t.scene_at(5.0);
    }

    #[test]
    fn default_cues_are_present() {
        let t = Timeline::default_for(211.9);
        assert!(!t.cues.is_empty());
    }

    /// 真实资源文件必须能被加载。
    ///
    /// 这个测试存在的理由：`assets/timeline.toml` 曾经把字段写成 `t` / `strength`，
    /// 而 `Cue` 的字段是 `time` / `intensity`（`time` 还无默认值），
    /// 结果**运行时直接解析失败**，而当时所有单元测试都用内联字符串，一个都没拦住。
    /// 只要这里红了，就是资源文件与结构体脱节，别去改测试。
    #[test]
    fn shipped_assets_load() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");

        let tl = dir.join("timeline.toml");
        let t = Timeline::load(&tl, 289.9)
            .unwrap_or_else(|e| panic!("{} 加载失败: {e}", tl.display()));
        assert_eq!(t.len(), 10, "时间轴应为 10 段（序/春/夏/桥/副歌/秋/冬/昇華/終章/尾奏）");
        assert!(!t.cues.is_empty(), "时间轴没有任何 cue");
        // 段落起点必须严格递增，否则 scene_at 会选错段
        let mut prev = f64::NEG_INFINITY;
        for s in t.segments() {
            assert!(s.start > prev, "段落起点未递增: {} @ {}", s.scene.name(), s.start);
            prev = s.start;
        }
        assert_eq!(t.scene_at(0.0), SceneKind::Intro);
        assert_eq!(t.scene_at(20.0), SceneKind::Spring);
        assert_eq!(t.scene_at(50.0), SceneKind::Summer);
        assert_eq!(t.scene_at(70.0), SceneKind::Bridge);
        assert_eq!(t.scene_at(90.0), SceneKind::Chorus);
        assert_eq!(t.scene_at(130.0), SceneKind::Autumn);
        assert_eq!(t.scene_at(160.0), SceneKind::Winter);
        assert_eq!(t.scene_at(180.0), SceneKind::Ascension);
        assert_eq!(t.scene_at(230.0), SceneKind::Finale);
        assert_eq!(t.scene_at(275.0), SceneKind::Outro);

        let lrc = dir.join("shunkashuto.lrc");
        let text = std::fs::read_to_string(&lrc)
            .unwrap_or_else(|e| panic!("{} 读取失败: {e}", lrc.display()));
        let lyrics = crate::lyrics::Lyrics::parse(&text);
        assert!(lyrics.len() >= 30, "唱句只有 {} 句，疑似 LRC 解析异常", lyrics.len());
        assert!(lyrics.current(0.0).is_none(), "0 秒处不该有唱句");
        assert!(lyrics.current(20.0).is_some(), "20 秒处应有唱句");
        assert!(lyrics.current(246.5).is_some(), "末句附近应有唱句");
    }
}

//! 关键事件（cues）：在特定时间点触发一次性视觉动作。
//!
//! 与「场景」的区别：场景是**持续状态**，cue 是**瞬时事件** ——
//! 闪白、撕裂、心形脉冲、镜头猛推。两者配合才有节奏感。

use serde::{Deserialize, Serialize};

/// 事件类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CueKind {
    /// 全屏闪白
    Flash,
    /// 强烈故障撕裂
    Glitch,
    /// 镜头猛推（瞬间放大）
    Punch,
    /// 心跳脉冲（心形放大）
    Heartbeat,
    /// 画面反转
    Invert,
    /// 清屏重启
    Reset,
    /// 环形爆发
    Burst,
    /// 仅作标注（不产生视觉效果）
    Mark,
}

impl CueKind {
    /// 全部类型。
    pub const ALL: [CueKind; 8] = [
        CueKind::Flash,
        CueKind::Glitch,
        CueKind::Punch,
        CueKind::Heartbeat,
        CueKind::Invert,
        CueKind::Reset,
        CueKind::Burst,
        CueKind::Mark,
    ];

    /// 名称。
    pub fn name(&self) -> &'static str {
        match self {
            CueKind::Flash => "flash",
            CueKind::Glitch => "glitch",
            CueKind::Punch => "punch",
            CueKind::Heartbeat => "heartbeat",
            CueKind::Invert => "invert",
            CueKind::Reset => "reset",
            CueKind::Burst => "burst",
            CueKind::Mark => "mark",
        }
    }

    /// 该事件默认持续多少秒（0 表示瞬时）。
    pub fn default_duration(&self) -> f64 {
        match self {
            CueKind::Flash => 0.12,
            CueKind::Glitch => 0.25,
            CueKind::Punch => 0.20,
            CueKind::Heartbeat => 0.30,
            CueKind::Invert => 0.08,
            CueKind::Reset => 0.40,
            CueKind::Burst => 0.50,
            CueKind::Mark => 0.0,
        }
    }

    /// 是否有实际视觉效果。
    pub fn is_visual(&self) -> bool {
        !matches!(self, CueKind::Mark)
    }
}

/// 一个事件。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cue {
    /// 触发时间（秒）
    pub time: f64,
    /// 类型
    pub kind: CueKind,
    /// 持续时间（秒）。为 0 时取类型的默认值
    #[serde(default)]
    pub duration: f64,
    /// 强度倍率
    #[serde(default = "one")]
    pub intensity: f32,
    /// 备注（`--analyze` 生成的建议切点会写在这里）
    #[serde(default)]
    pub name: String,
}

fn one() -> f32 {
    1.0
}

impl Cue {
    /// 新建。
    pub fn new(time: f64, kind: CueKind) -> Self {
        Self {
            time,
            kind,
            duration: kind.default_duration(),
            intensity: 1.0,
            name: String::new(),
        }
    }

    /// 带名称。
    pub fn named(time: f64, kind: CueKind, name: impl Into<String>) -> Self {
        let mut c = Self::new(time, kind);
        c.name = name.into();
        c
    }

    /// 实际时长。
    pub fn effective_duration(&self) -> f64 {
        if self.duration > 0.0 {
            self.duration
        } else {
            self.kind.default_duration()
        }
    }

    /// 结束时间。
    pub fn end(&self) -> f64 {
        self.time + self.effective_duration()
    }

    /// 在 `t` 时刻的强度（0..1，两端为 0，中点最高）。
    pub fn envelope(&self, t: f64) -> f32 {
        let d = self.effective_duration();
        if d <= 0.0 {
            return 0.0;
        }
        if t < self.time || t > self.time + d {
            return 0.0;
        }
        let u = (t - self.time) / d;
        // 快起慢落：前半段线性上升，后半段指数衰减
        let v = if u < 0.25 {
            u / 0.25
        } else {
            (-(u - 0.25) / 0.4).exp()
        };
        ((v as f32) * self.intensity).clamp(0.0, 2.0)
    }
}

/// 一组事件（按时间升序）。
#[derive(Debug, Clone, Default)]
pub struct CueTrack {
    cues: Vec<Cue>,
}

impl CueTrack {
    /// 新建并排序。
    pub fn new(mut cues: Vec<Cue>) -> Self {
        cues.sort_by(|a, b| {
            a.time
                .partial_cmp(&b.time)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Self { cues }
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.cues.is_empty()
    }

    /// 数量。
    pub fn len(&self) -> usize {
        self.cues.len()
    }

    /// 全部事件。
    pub fn all(&self) -> &[Cue] {
        &self.cues
    }

    /// 在 `t` 时刻所有活跃事件叠加后的强度（按类型）。
    pub fn active(&self, t: f64, kind: CueKind) -> f32 {
        self.cues
            .iter()
            .filter(|c| c.kind == kind)
            .map(|c| c.envelope(t))
            .sum::<f32>()
            .clamp(0.0, 2.0)
    }

    /// 任意视觉事件的总强度。
    pub fn any_visual(&self, t: f64) -> f32 {
        self.cues
            .iter()
            .filter(|c| c.kind.is_visual())
            .map(|c| c.envelope(t))
            .sum::<f32>()
            .clamp(0.0, 2.0)
    }

    /// `t` 时刻是否恰好跨过某个事件的起点。
    ///
    /// `prev_t` 是上一帧时间，用于避免「一帧内触发多次」。
    pub fn crossed(&self, prev_t: f64, t: f64) -> Vec<&Cue> {
        self.cues
            .iter()
            .filter(|c| c.time > prev_t && c.time <= t)
            .collect()
    }

    /// 默认事件表：只使用**音乐结构**上的锚点，不含任何歌词文本。
    ///
    /// 时间点取自公开的歌曲结构分析（副歌起止），使用者可用
    /// `--timeline` 换成自己的 `timeline.toml`。
    pub fn default_track(duration: f64) -> Self {
        let mut cues = Vec::new();
        // 前奏：三次开机脉冲
        for (i, t) in [0.0f64, 0.8, 1.6].iter().enumerate() {
            cues.push(Cue::named(*t, CueKind::Glitch, format!("boot{}", i + 1)));
        }
        cues.push(Cue::named(3.0, CueKind::Burst, "signal acquired"));
        cues.push(Cue::named(33.0, CueKind::Punch, "verse1"));
        cues.push(Cue::named(47.2, CueKind::Flash, "prechorus"));
        cues.push(Cue::named(73.5, CueKind::Heartbeat, "chorus1"));
        cues.push(Cue::named(88.3, CueKind::Invert, "verse2"));
        cues.push(Cue::named(115.5, CueKind::Glitch, "bridge"));
        cues.push(Cue::named(127.0, CueKind::Punch, "chorus2"));
        // 副歌 2 里的心跳网格（每 2 秒一次，直到尾声）
        let mut t = 127.0;
        while t < 186.0 {
            cues.push(Cue::new(t, CueKind::Heartbeat));
            t += 2.0;
        }
        cues.push(Cue::named(187.0, CueKind::Reset, "outro"));
        if duration > 0.0 {
            cues.push(Cue::named((duration - 0.5).max(0.0), CueKind::Flash, "end"));
        }
        Self::new(cues)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cue_effective_duration_uses_default() {
        let c = Cue {
            time: 1.0,
            kind: CueKind::Flash,
            duration: 0.0,
            intensity: 1.0,
            name: String::new(),
        };
        assert!((c.effective_duration() - CueKind::Flash.default_duration()).abs() < 1e-9);
    }

    #[test]
    fn cue_effective_duration_uses_explicit() {
        let mut c = Cue::new(1.0, CueKind::Flash);
        c.duration = 2.5;
        assert!((c.effective_duration() - 2.5).abs() < 1e-9);
        assert!((c.end() - 3.5).abs() < 1e-9);
    }

    #[test]
    fn envelope_is_zero_outside_window() {
        let c = Cue::new(10.0, CueKind::Flash);
        assert_eq!(c.envelope(9.0), 0.0);
        assert_eq!(c.envelope(100.0), 0.0);
    }

    #[test]
    fn envelope_peaks_inside_window() {
        let c = Cue::new(10.0, CueKind::Flash);
        let peak = c.envelope(10.0 + c.effective_duration() * 0.25);
        assert!(peak > 0.9, "峰值 {peak} 过低");
    }

    #[test]
    fn envelope_respects_intensity() {
        let mut c = Cue::new(0.0, CueKind::Flash);
        c.intensity = 0.5;
        let peak = c.envelope(c.effective_duration() * 0.25);
        assert!(peak <= 0.6, "强度未生效: {peak}");
    }

    #[test]
    fn track_sorts_cues() {
        let t = CueTrack::new(vec![
            Cue::new(5.0, CueKind::Flash),
            Cue::new(1.0, CueKind::Glitch),
        ]);
        assert!((t.all()[0].time - 1.0).abs() < 1e-9);
    }

    #[test]
    fn active_sums_same_kind() {
        let mut a = Cue::new(0.0, CueKind::Flash);
        a.duration = 1.0;
        let mut b = Cue::new(0.0, CueKind::Flash);
        b.duration = 1.0;
        let t = CueTrack::new(vec![a, b]);
        assert!(t.active(0.25, CueKind::Flash) > 1.0);
    }

    #[test]
    fn active_filters_by_kind() {
        let t = CueTrack::new(vec![Cue::new(0.0, CueKind::Flash)]);
        assert!(t.active(0.03, CueKind::Glitch) == 0.0);
    }

    #[test]
    fn mark_cues_are_not_visual() {
        assert!(!CueKind::Mark.is_visual());
        let t = CueTrack::new(vec![Cue::new(0.0, CueKind::Mark)]);
        assert_eq!(t.any_visual(0.0), 0.0);
    }

    #[test]
    fn crossed_detects_events_between_frames() {
        let t = CueTrack::new(vec![
            Cue::new(1.0, CueKind::Flash),
            Cue::new(2.0, CueKind::Glitch),
        ]);
        let hit = t.crossed(0.9, 1.1);
        assert_eq!(hit.len(), 1);
        assert_eq!(hit[0].kind, CueKind::Flash);
        // 同一事件不该重复触发
        assert!(t.crossed(1.1, 1.2).is_empty());
    }

    #[test]
    fn crossed_handles_multiple_in_one_frame() {
        let t = CueTrack::new(vec![
            Cue::new(1.0, CueKind::Flash),
            Cue::new(1.01, CueKind::Glitch),
        ]);
        assert_eq!(t.crossed(0.99, 1.02).len(), 2);
    }

    #[test]
    fn default_track_is_sorted_and_non_empty() {
        let t = CueTrack::default_track(211.9);
        assert!(!t.is_empty());
        for w in t.all().windows(2) {
            assert!(w[0].time <= w[1].time);
        }
    }

    #[test]
    fn default_track_has_no_lyric_text() {
        let t = CueTrack::default_track(211.9);
        for c in t.all() {
            assert!(
                !c.name.to_lowercase().contains("power line"),
                "默认事件表不应包含歌词文本"
            );
        }
    }

    #[test]
    fn default_track_covers_outro() {
        let t = CueTrack::default_track(211.9);
        assert!(t
            .all()
            .iter()
            .any(|c| c.kind == CueKind::Reset && c.time > 180.0));
    }

    #[test]
    fn all_kinds_have_names() {
        for k in CueKind::ALL {
            assert!(!k.name().is_empty());
        }
    }
}

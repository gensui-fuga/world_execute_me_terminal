//! 叙事弧：把整首歌切成有名字的章节，并给每章一组视觉基调。
//!
//! 参考成熟实现的关键洞察：**一支 MV 的骨架是叙事，不是场景**。
//! 同样的画面技巧，挂在「训练 → 部署 → 你离开 → 执行」的骨架上就成立，
//! 散着放就只是一堆特效。
//!
//! 章节划分跟随歌曲的结构锚点（可由 `assets/timeline.toml` 覆盖），
//! 每章带：
//! - 一个**标题**（显示在状态栏，让观众知道现在到哪了）
//! - **配色偏移**（色调随章节推进整体移动）
//! - **能量曲线**（关键帧，控制画面密度）
//! - **系统色增益**（`UI_GAIN`：系统色随剧情推进逐渐排空）
//!
//! 本模块只有结构信息，不含歌词文本。

use crate::render::color::{self as col, Color};
use crate::timeline::beat::keyframes;

/// 叙事章节。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Chapter {
    /// 开机：从一片像素里长出东西来
    Boot,
    /// 建立：世界被铺设，参数被填充
    Build,
    /// 训练：反复捶打，密集的迭代
    Train,
    /// 对齐：奖励与惩罚，红笔改动
    Align,
    /// 部署：成型，第一次完整地运转
    Deploy,
    /// 离开：对面的窗口被一层层拆掉
    Depart,
    /// 执念：溢出、占有、不肯放手
    Obsess,
    /// 执行：最猛烈的一段
    Execute,
    /// 评估：回头看，爱与代价
    Evaluate,
    /// 沉降：终点，一切归于寂静
    Settle,
}

impl Chapter {
    /// 全部章节，按时间顺序。
    pub const ALL: [Chapter; 10] = [
        Chapter::Boot,
        Chapter::Build,
        Chapter::Train,
        Chapter::Align,
        Chapter::Deploy,
        Chapter::Depart,
        Chapter::Obsess,
        Chapter::Execute,
        Chapter::Evaluate,
        Chapter::Settle,
    ];

    /// 状态栏标题。
    pub fn label(self) -> &'static str {
        match self {
            Chapter::Boot => "00 / BOOT",
            Chapter::Build => "01 / BUILD",
            Chapter::Train => "02 / TRAIN",
            Chapter::Align => "03 / ALIGN",
            Chapter::Deploy => "04 / DEPLOY",
            Chapter::Depart => "05 / DEPART",
            Chapter::Obsess => "06 / OBSESS",
            Chapter::Execute => "07 / EXECUTE",
            Chapter::Evaluate => "08 / EVAL",
            Chapter::Settle => "09 / SETTLE",
        }
    }

    /// 中文短名（用于 UI 与调试）。
    pub fn short(self) -> &'static str {
        match self {
            Chapter::Boot => "开机",
            Chapter::Build => "建立",
            Chapter::Train => "训练",
            Chapter::Align => "对齐",
            Chapter::Deploy => "部署",
            Chapter::Depart => "离开",
            Chapter::Obsess => "执念",
            Chapter::Execute => "执行",
            Chapter::Evaluate => "评估",
            Chapter::Settle => "沉降",
        }
    }

    /// 默认锚点（秒）。可被用户时间轴覆盖。
    ///
    /// 这些数字对应歌曲的实际结构：前奏约 33s，副歌 73.5s 起，
    /// 桥段约 118s，最后一段副歌 147s 起，207.6s 是硬切点。
    pub fn default_anchor(self) -> f32 {
        match self {
            Chapter::Boot => 0.0,
            Chapter::Build => 16.0,
            Chapter::Train => 44.0,
            Chapter::Align => 58.5, // 0.0,
            Chapter::Deploy => 73.5,
            Chapter::Depart => 103.0,
            Chapter::Obsess => 125.0,
            Chapter::Execute => 147.6,
            Chapter::Evaluate => 176.9,
            Chapter::Settle => 193.4,
        }
    }

    /// 章节的主色偏移。
    ///
    /// 从冷青（系统）慢慢推向洋红与红（情绪与暴力），
    /// 最后回到低饱和的灰青（沉寂）。颜色本身就是叙事。
    pub fn tint(self) -> Color {
        match self {
            Chapter::Boot => Color::Rgb(120, 255, 240),
            Chapter::Build => Color::Rgb(80, 240, 255),
            Chapter::Train => Color::Rgb(120, 200, 255),
            Chapter::Align => Color::Rgb(180, 160, 255),
            Chapter::Deploy => Color::Rgb(220, 130, 255),
            Chapter::Depart => Color::Rgb(255, 110, 190),
            Chapter::Obsess => Color::Rgb(255, 90, 130),
            Chapter::Execute => Color::Rgb(255, 70, 70),
            Chapter::Evaluate => Color::Rgb(255, 150, 140),
            Chapter::Settle => Color::Rgb(140, 180, 190),
        }
    }

    /// 色温：0.0 = 最冷（疏离），1.0 = 最暖（亲密）。
    ///
    /// 观众不做热力学 —— 他们把**琥珀/玫瑰感受为安全**，
    /// 把**青/蓝感受为距离**，这种感觉伴随了他们一生。
    /// 所以色温不是装饰，是叙事：它跟着「她离用户有多近」走。
    ///
    /// | 段落 | 色温 | 理由 |
    /// |------|------|------|
    /// | Boot | 0.15 | 冰冷的空世界 |
    /// | Build | 0.20 | 还在初始化 |
    /// | Train | 0.30 | 数学，理性 |
    /// | Align | 0.45 | 开始有电流 |
    /// | Deploy | 0.70 | 第一次表白 |
    /// | Depart | 0.80 | 爱得最深 |
    /// | Obsess | 0.85 | 痴迷，最暖也最危险 |
    /// | Execute | 1.00 | 血红，过热的爱 |
    /// | Evaluate | 0.65 | 温柔下来 |
    /// | Settle | 0.10 | 冷却，走向死亡 |
    pub fn warmth(self) -> f32 {
        match self {
            Chapter::Boot => 0.15,
            Chapter::Build => 0.20,
            Chapter::Train => 0.30,
            Chapter::Align => 0.45,
            Chapter::Deploy => 0.70,
            Chapter::Depart => 0.80,
            Chapter::Obsess => 0.85,
            Chapter::Execute => 1.00,
            Chapter::Evaluate => 0.65,
            Chapter::Settle => 0.10,
        }
    }

    /// 系统色增益：剧情推进时「你」的存在感逐渐排空。
    ///
    /// 这是从成熟实现里学到的概念 —— 系统色（琥珀）代表宿主，
    /// 它随宿主离开而变暗，让「她的意志色」相对地占据画面。
    pub fn ui_gain(self) -> f32 {
        match self {
            Chapter::Boot => 1.00,
            Chapter::Build => 1.00,
            Chapter::Train => 0.94,
            Chapter::Align => 0.88,
            Chapter::Deploy => 0.80,
            Chapter::Depart => 0.58,
            Chapter::Obsess => 0.44,
            Chapter::Execute => 0.30,
            Chapter::Evaluate => 0.40,
            Chapter::Settle => 0.22,
        }
    }

    /// 章节的密度曲线（关键帧）。
    ///
    /// 返回 `(在该章内的相对时刻, 密度 0~1)`。
    /// 每章形状不同：训练是锯齿（反复捶打），执念是持续爬升，沉降是衰减。
    pub fn density_curve(self) -> &'static [(f32, f32)] {
        match self {
            // 从无到有
            Chapter::Boot => &[(0.0, 0.05), (0.3, 0.25), (0.7, 0.55), (1.0, 0.7)],
            // 稳定铺设
            Chapter::Build => &[(0.0, 0.5), (0.5, 0.62), (1.0, 0.58)],
            // 锯齿：一浪接一浪
            Chapter::Train => &[
                (0.0, 0.45),
                (0.2, 0.8),
                (0.25, 0.5),
                (0.45, 0.85),
                (0.5, 0.52),
                (0.7, 0.9),
                (0.75, 0.55),
                (1.0, 0.95),
            ],
            // 收紧
            Chapter::Align => &[(0.0, 0.6), (0.5, 0.78), (1.0, 0.85)],
            // 开阔
            Chapter::Deploy => &[(0.0, 0.7), (0.4, 0.88), (1.0, 0.8)],
            // 塌陷
            Chapter::Depart => &[(0.0, 0.9), (0.3, 0.6), (0.7, 0.3), (1.0, 0.22)],
            // 爬升
            Chapter::Obsess => &[(0.0, 0.35), (0.5, 0.6), (1.0, 0.92)],
            // 顶点
            Chapter::Execute => &[(0.0, 1.0), (0.5, 1.0), (1.0, 0.9)],
            // 回望
            Chapter::Evaluate => &[(0.0, 0.75), (0.5, 0.6), (1.0, 0.4)],
            // 消散
            Chapter::Settle => &[(0.0, 0.35), (0.4, 0.22), (0.8, 0.1), (1.0, 0.03)],
        }
    }

    /// 该章节是否处于高能状态（用于决定是否加强残影与粒子）。
    pub fn is_intense(self) -> bool {
        matches!(
            self,
            Chapter::Execute | Chapter::Deploy | Chapter::Obsess | Chapter::Train
        )
    }

    /// 该章节是否需要镜头抖动。
    pub fn shakes(self) -> bool {
        matches!(self, Chapter::Execute | Chapter::Train | Chapter::Obsess)
    }
}

/// 一条章节划分（含实际时间边界）。
#[derive(Debug, Clone, Copy)]
pub struct ChapterSpan {
    pub chapter: Chapter,
    pub start: f32,
    pub end: f32,
}

impl ChapterSpan {
    /// 长度（秒）。
    pub fn len(&self) -> f32 {
        (self.end - self.start).max(0.0)
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.len() <= 1e-3
    }

    /// `t` 在该章内的归一化进度 0~1。
    pub fn progress(&self, t: f32) -> f32 {
        if self.len() <= 1e-3 {
            return 1.0;
        }
        ((t - self.start) / self.len()).clamp(0.0, 1.0)
    }
}

/// 完整的叙事弧。
#[derive(Debug, Clone)]
pub struct Narrative {
    spans: Vec<ChapterSpan>,
    song_len: f32,
}

impl Narrative {
    /// 用默认锚点构建。
    pub fn default_arc(song_len: f32) -> Self {
        let anchors: Vec<(Chapter, f32)> = Chapter::ALL
            .iter()
            .map(|&c| (c, c.default_anchor()))
            .collect();
        Self::from_anchors(&anchors, song_len)
    }

    /// 用自定义锚点构建。`anchors` 必须按时间升序；乱序会被自动排序。
    ///
    /// 锚点里 `start >= song_len` 的章节会被裁掉。
    pub fn from_anchors(anchors: &[(Chapter, f32)], song_len: f32) -> Self {
        let song_len = song_len.max(1.0);
        let mut sorted: Vec<(Chapter, f32)> = anchors
            .iter()
            .copied()
            .filter(|(_, t)| *t < song_len)
            .collect();
        sorted.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        // 去掉时间重复的项（保序取第一个）
        sorted.dedup_by(|a, b| (a.1 - b.1).abs() < 1e-3);

        let mut spans = Vec::with_capacity(sorted.len());
        for k in 0..sorted.len() {
            let (chapter, start) = sorted[k];
            let end = sorted.get(k + 1).map(|(_, t)| *t).unwrap_or(song_len);
            spans.push(ChapterSpan {
                chapter,
                start,
                end: end.max(start),
            });
        }
        if spans.is_empty() {
            spans.push(ChapterSpan {
                chapter: Chapter::Boot,
                start: 0.0,
                end: song_len,
            });
        }
        Self { spans, song_len }
    }

    /// 空弧。
    pub fn empty(song_len: f32) -> Self {
        Self {
            spans: vec![ChapterSpan {
                chapter: Chapter::Boot,
                start: 0.0,
                end: song_len.max(1.0),
            }],
            song_len: song_len.max(1.0),
        }
    }

    /// 全部划分。
    pub fn spans(&self) -> &[ChapterSpan] {
        &self.spans
    }

    /// 歌曲长度。
    pub fn song_len(&self) -> f32 {
        self.song_len
    }

    /// 找到 `t` 所在的划分。
    pub fn span_at(&self, t: f32) -> Option<&ChapterSpan> {
        // 最后一个 start <= t 的
        let idx = self
            .spans
            .iter()
            .rposition(|s| s.start <= t + 1e-4)
            .unwrap_or(0);
        self.spans.get(idx)
    }

    /// `t` 时刻的章节。
    pub fn chapter_at(&self, t: f32) -> Chapter {
        self.span_at(t).map(|s| s.chapter).unwrap_or(Chapter::Boot)
    }

    /// `t` 时刻的完整视觉基调。
    pub fn mood_at(&self, t: f32) -> ChapterMood {
        let span = self.span_at(t).copied().unwrap_or(ChapterSpan {
            chapter: Chapter::Boot,
            start: 0.0,
            end: self.song_len,
        });
        let p = span.progress(t);
        let density = keyframes(p, span.chapter.density_curve());
        // 章节交界处做混合，避免硬切 —— 交界前后各 1.5 秒过渡
        let (tint, gain) = self.blend_at(t, &span);
        ChapterMood {
            chapter: span.chapter,
            label: span.chapter.label(),
            progress: p,
            density,
            tint,
            ui_gain: gain,
            intense: span.chapter.is_intense(),
            shakes: span.chapter.shakes(),
        }
    }

    /// 在章节交界处混合色调与系统增益。
    ///
    /// **关键：混合只依赖到交界点的有符号距离**。
    ///
    /// 早先分别用「距本章尾」和「距本章头」计算，两者跨越边界时并不互补，
    /// 导致交界处跳变（实测 43.98s 得 0.94、44.02s 得 1.00）。
    /// 现在两侧共用同一个公式 `w = smoothstep((t - boundary) / (2·FADE) + 0.5)`，
    /// 所以无论从左边还是右边趋近同一个点，结果必然连续。
    fn blend_at(&self, t: f32, span: &ChapterSpan) -> (Color, f32) {
        const FADE: f32 = 1.5;

        // 找出最近的交界点及其前后章节
        let next = self
            .spans
            .iter()
            .find(|s| (s.start - span.end).abs() < 1e-4)
            .copied();
        let prev = self
            .spans
            .iter()
            .rev()
            .find(|s| (s.end - span.start).abs() < 1e-4)
            .copied();

        // 候选交界：(交界时间, 左章, 右章, 到交界的有符号距离)
        let mut cands: Vec<(f32, Chapter, Chapter, f32)> = Vec::new();
        if let Some(n) = next {
            cands.push((span.end, span.chapter, n.chapter, t - span.end));
        }
        if let Some(p) = prev {
            cands.push((span.start, p.chapter, span.chapter, t - span.start));
        }

        // 取有符号距离绝对值最小的那个交界
        let best = cands.into_iter().min_by(|a, b| {
            a.3.abs()
                .partial_cmp(&b.3.abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let Some((_boundary, left, right, d)) = best else {
            return (span.chapter.tint(), span.chapter.ui_gain());
        };

        if d.abs() >= FADE {
            return (span.chapter.tint(), span.chapter.ui_gain());
        }

        // d < 0 表示在交界左侧，d > 0 在右侧；
        // 映射到 [0,1] 后过 smoothstep，两侧同一条曲线。
        let u = ((d / (2.0 * FADE)) + 0.5).clamp(0.0, 1.0);
        let w = crate::timeline::beat::smoothstep(u, 0.0, 1.0);

        let tint = col::lerp(left.tint(), right.tint(), w);
        let gain = left.ui_gain() + (right.ui_gain() - left.ui_gain()) * w;
        (tint, gain)
    }

    /// 章节数量。
    pub fn len(&self) -> usize {
        self.spans.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.spans.is_empty()
    }
}

/// 某时刻的章节基调。
#[derive(Debug, Clone, Copy)]
pub struct ChapterMood {
    /// 当前章节
    pub chapter: Chapter,
    /// 状态栏标题
    pub label: &'static str,
    /// 章内进度 0~1
    pub progress: f32,
    /// 目标密度 0~1
    pub density: f32,
    /// 混合后的主色调
    pub tint: Color,
    /// 系统色增益 0~1
    pub ui_gain: f32,
    /// 是否高能章
    pub intense: bool,
    /// 是否需要抖动
    pub shakes: bool,
}

impl ChapterMood {
    /// 默认基调。
    pub fn neutral() -> Self {
        Self {
            chapter: Chapter::Boot,
            label: Chapter::Boot.label(),
            progress: 0.0,
            density: 0.5,
            tint: Chapter::Boot.tint(),
            ui_gain: 1.0,
            intense: false,
            shakes: false,
        }
    }

    /// 把章节色应用到某个原色上（按比例混合）。
    pub fn apply_tint(&self, base: Color, amount: f32) -> Color {
        col::lerp(base, self.tint, amount.clamp(0.0, 1.0))
    }

    /// 系统色的实际强度（原色 × ui_gain）。
    pub fn system_color(&self, base: Color) -> Color {
        col::scale(base, self.ui_gain.clamp(0.0, 1.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEN: f32 = 211.913;

    fn arc() -> Narrative {
        Narrative::default_arc(LEN)
    }

    #[test]
    fn default_arc_covers_whole_song() {
        let n = arc();
        assert_eq!(n.len(), Chapter::ALL.len());
        let first = n.spans()[0];
        let last = n.spans()[n.len() - 1];
        assert!((first.start - 0.0).abs() < 1e-6);
        assert!(
            (last.end - LEN).abs() < 1e-3,
            "最后应覆盖到曲末: {}",
            last.end
        );
    }

    #[test]
    fn spans_are_contiguous_and_sorted() {
        let n = arc();
        for w in n.spans().windows(2) {
            assert!(
                (w[0].end - w[1].start).abs() < 1e-3,
                "章节之间不该有缝隙: {} vs {}",
                w[0].end,
                w[1].start
            );
            assert!(w[0].start <= w[1].start, "必须有序");
        }
    }

    #[test]
    fn chapter_at_matches_anchors() {
        let n = arc();
        assert_eq!(n.chapter_at(0.0), Chapter::Boot);
        assert_eq!(n.chapter_at(20.0), Chapter::Build);
        assert_eq!(n.chapter_at(50.0), Chapter::Train);
        assert_eq!(n.chapter_at(80.0), Chapter::Deploy);
        assert_eq!(n.chapter_at(200.0), Chapter::Settle);
    }

    #[test]
    fn chapter_at_every_second_is_valid() {
        let n = arc();
        for i in 0..230 {
            let c = n.chapter_at(i as f32);
            assert!(!c.label().is_empty(), "第 {i} 秒章节无标题");
        }
    }

    #[test]
    fn span_progress_is_bounded() {
        let n = arc();
        for i in 0..2000 {
            let t = i as f32 * 0.11;
            if let Some(s) = n.span_at(t) {
                let p = s.progress(t);
                assert!((0.0..=1.0).contains(&p), "t={t} p={p}");
            }
        }
    }

    #[test]
    fn mood_is_always_well_formed() {
        let n = arc();
        for i in 0..4000 {
            let t = i as f32 * 0.053;
            let m = n.mood_at(t);
            assert!((0.0..=1.0).contains(&m.progress), "t={t}");
            assert!((0.0..=1.0).contains(&m.density), "t={t} d={}", m.density);
            assert!((0.0..=1.0).contains(&m.ui_gain), "t={t} g={}", m.ui_gain);
            assert!(!m.label.is_empty());
        }
    }

    #[test]
    fn ui_gain_drains_over_the_song() {
        let n = arc();
        let early = n.mood_at(5.0).ui_gain;
        let late = n.mood_at(195.0).ui_gain;
        assert!(early > late, "系统色应随剧情排空: {early} → {late}");
        assert!(early > 0.9, "开头应接近满值");
    }

    #[test]
    fn blend_is_continuous_at_boundaries() {
        let n = arc();
        // 在章节交界的 ±0.01s 处，uigain 不应跳变
        for s in n.spans() {
            if s.start <= 1e-3 {
                continue;
            }
            let a = n.mood_at(s.start - 0.01).ui_gain;
            let b = n.mood_at(s.start + 0.01).ui_gain;
            assert!((a - b).abs() < 0.05, "交界 {} 处跳变: {a} vs {b}", s.start);
        }
    }

    #[test]
    fn execute_chapter_is_the_peak() {
        let n = arc();
        let m = n.mood_at(160.0);
        assert_eq!(m.chapter, Chapter::Execute);
        assert!(m.intense, "执行段应是高能");
        assert!(m.shakes, "执行段应抖动");
        assert!(m.density > 0.8, "执行段密度应很高: {}", m.density);
    }

    #[test]
    fn settle_fades_out() {
        let n = arc();
        let early = n.mood_at(194.0).density;
        let late = n.mood_at(211.0).density;
        assert!(late < early, "沉降段应越来越淡: {early} → {late}");
        assert!(late < 0.15, "曲末应几乎消失: {late}");
    }

    #[test]
    fn boot_grows_from_nothing() {
        let n = arc();
        let a = n.mood_at(0.5).density;
        let b = n.mood_at(30.0).density;
        assert!(b > a, "开机段密度应上升: {a} → {b}");
        assert!(a < 0.2, "开头应几乎是空的: {a}");
    }

    #[test]
    fn all_chapters_have_distinct_labels() {
        let mut seen = std::collections::HashSet::new();
        for c in Chapter::ALL {
            assert!(seen.insert(c.label()), "{} 标题重复", c.label());
            assert!(!c.short().is_empty());
        }
    }

    #[test]
    fn custom_anchors_are_respected() {
        let anchors = [
            (Chapter::Boot, 0.0),
            (Chapter::Execute, 100.0),
            (Chapter::Settle, 150.0),
        ];
        let n = Narrative::from_anchors(&anchors, 200.0);
        assert_eq!(n.len(), 3);
        assert_eq!(n.chapter_at(50.0), Chapter::Boot);
        assert_eq!(n.chapter_at(120.0), Chapter::Execute);
        assert_eq!(n.chapter_at(180.0), Chapter::Settle);
    }

    #[test]
    fn unordered_anchors_are_sorted() {
        let anchors = [
            (Chapter::Settle, 150.0),
            (Chapter::Boot, 0.0),
            (Chapter::Execute, 100.0),
        ];
        let n = Narrative::from_anchors(&anchors, 200.0);
        assert_eq!(n.chapter_at(5.0), Chapter::Boot);
        assert_eq!(n.chapter_at(120.0), Chapter::Execute);
    }

    #[test]
    fn empty_and_degenerate_inputs_are_safe() {
        let n = Narrative::from_anchors(&[], 100.0);
        assert_eq!(n.len(), 1);
        assert_eq!(n.chapter_at(50.0), Chapter::Boot);

        let n = Narrative::empty(100.0);
        assert!(n.mood_at(50.0).density.is_finite());

        // 时长为 0 也不该崩
        let n = Narrative::from_anchors(&[(Chapter::Boot, 0.0)], 0.0);
        assert!(n.mood_at(0.0).ui_gain.is_finite());
    }

    #[test]
    fn out_of_range_chapters_are_dropped() {
        let anchors = [(Chapter::Boot, 0.0), (Chapter::Execute, 500.0)];
        let n = Narrative::from_anchors(&anchors, 100.0);
        assert_eq!(n.len(), 1, "超出曲长的章节应被裁掉");
    }

    #[test]
    fn duplicate_times_are_deduped() {
        let anchors = [
            (Chapter::Boot, 0.0),
            (Chapter::Build, 0.0),
            (Chapter::Execute, 50.0),
        ];
        let n = Narrative::from_anchors(&anchors, 100.0);
        assert_eq!(n.len(), 2);
    }

    #[test]
    fn tint_application_stays_valid() {
        let n = arc();
        for i in 0..500 {
            let t = i as f32 * 0.4;
            let m = n.mood_at(t);
            // u8 分量天然在 0..=255，只需确认取色不 panic 且保持有限
            let (r, g, b) = col::rgb_of(m.apply_tint(col::CYAN, 0.5));
            assert!((r as u32 + g as u32 + b as u32) > 0, "t={t} 取色全黑");
        }
    }

    #[test]
    fn system_color_scales_with_gain() {
        let full = ChapterMood {
            ui_gain: 1.0,
            ..ChapterMood::neutral()
        };
        let drained = ChapterMood {
            ui_gain: 0.2,
            ..ChapterMood::neutral()
        };
        let a = col::rgb_of(full.system_color(col::WHITE)).0;
        let b = col::rgb_of(drained.system_color(col::WHITE)).0;
        assert!(a > b, "排空后应变暗: {a} vs {b}");
        assert!(b > 0, "不该完全消失");
    }

    #[test]
    fn everything_finite_across_song() {
        let n = arc();
        for i in 0..6000 {
            let t = i as f32 * 0.037;
            let m = n.mood_at(t);
            assert!(m.progress.is_finite());
            assert!(m.density.is_finite());
            assert!(m.ui_gain.is_finite());
            let (r, g, b) = col::rgb_of(m.tint);
            assert!((r as u32 + g as u32 + b as u32) > 0, "t={t} 章节色全黑");
        }
    }
}

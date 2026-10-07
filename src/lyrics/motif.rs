//! 歌词主题词 → 视觉规则。
//!
//! 这一层把「正在唱什么」翻译成「画面该干什么」。
//!
//! 设计约束：
//! - **不含任何歌词文本**。这里只有关键词表与对应的视觉动作，
//!   真正的歌词由使用者的 LRC 提供（`--lrc`）。
//! - 匹配是小写、去标点、按词边界的前缀匹配，能同时命中 `execute`/`execution`。
//! - 规则是**纯函数**：同一个词永远得到同一组视觉参数，seek 后可复现。

use crate::render::color::Color;

/// 一个主题词对应的视觉动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motif {
    /// 启动 / 通电：扫描线自上而下扫过，光标闪烁
    Power,
    /// 数据结构化：网格展开，节点连线
    Data,
    /// 执行：终端光标、指令流、方括号光标
    Execute,
    /// 世界：经纬网格、球面投影
    World,
    /// 模拟：分形递归、嵌套方框
    Simulation,
    /// 自我：心形聚焦、瞳孔收缩
    SelfRef,
    /// 困境 / 束缚：链条、栅格笼、收缩
    Trapped,
    /// 断开 / 离开：撕裂、断线、碎片
    Sever,
    /// 情绪 / 爱：暖色脉冲、心脏跳动
    Affect,
    /// 时间 / 循环：环形回旋、钟摆
    Loop,
    /// 未知：无特殊动作
    None,
}

impl Motif {
    /// 全部变体（测试与调色用）。
    pub const ALL: [Motif; 11] = [
        Motif::Power,
        Motif::Data,
        Motif::Execute,
        Motif::World,
        Motif::Simulation,
        Motif::SelfRef,
        Motif::Trapped,
        Motif::Sever,
        Motif::Affect,
        Motif::Loop,
        Motif::None,
    ];

    /// 调试名。
    pub fn name(self) -> &'static str {
        match self {
            Motif::Power => "power",
            Motif::Data => "data",
            Motif::Execute => "execute",
            Motif::World => "world",
            Motif::Simulation => "simulation",
            Motif::SelfRef => "self",
            Motif::Trapped => "trapped",
            Motif::Sever => "sever",
            Motif::Affect => "affect",
            Motif::Loop => "loop",
            Motif::None => "-",
        }
    }

    /// 该主题的强调色。`None` 表示沿用场景调色板。
    pub fn accent(self) -> Option<Color> {
        use crate::render::color as col;
        match self {
            Motif::Power => Some(col::Color::Rgb(255, 220, 120)),
            Motif::Execute => Some(col::Color::Rgb(120, 255, 170)),
            Motif::World => Some(col::Color::Rgb(90, 200, 255)),
            Motif::Trapped => Some(col::Color::Rgb(255, 70, 70)),
            Motif::Sever => Some(col::Color::Rgb(255, 140, 60)),
            Motif::Affect => Some(col::Color::Rgb(255, 110, 190)),
            _ => None,
        }
    }

    /// 该主题期望的额外粒子强度（0.0~1.0），叠加到爆发量上。
    pub fn energy_bias(self) -> f32 {
        match self {
            Motif::Execute | Motif::Sever => 0.45,
            Motif::Power | Motif::Trapped => 0.3,
            Motif::Affect => 0.15,
            Motif::Loop | Motif::Data => 0.1,
            _ => 0.0,
        }
    }

    /// 该主题是否让画面整体抖动。
    pub fn shakes(self) -> bool {
        matches!(self, Motif::Execute | Motif::Sever | Motif::Trapped)
    }
}

/// 主题词 → 视觉动作的映射表。
///
/// 每条是 `(前缀, 主题)`。匹配时取**最长命中前缀**，
/// 所以 `execution` 会命中 `execut` 而不是 `ex`。
///
/// 前缀必须足够长以避免误伤：`on` 这种两字母前缀会吞掉 only/once/onto，
/// 所以短词一律写全（`on` 的映射交给 [`EXACT`] 表）。
const RULES: &[(&str, Motif)] = &[
    // 通电与启动
    ("switch", Motif::Power),
    ("power", Motif::Power),
    ("begin", Motif::Power),
    ("start", Motif::Power),
    ("initial", Motif::Power),
    // 数据与结构
    ("data", Motif::Data),
    ("parameter", Motif::Data),
    ("dimension", Motif::Data),
    ("point", Motif::Data),
    ("fill", Motif::Data),
    ("protect", Motif::Data),
    ("piece", Motif::Data),
    ("creation", Motif::Data),
    ("object", Motif::Data),
    // 执行
    ("execut", Motif::Execute),
    ("command", Motif::Execute),
    ("function", Motif::Execute),
    // 世界与模拟
    ("world", Motif::World),
    ("simulat", Motif::Simulation),
    // 自我
    ("your", Motif::SelfRef),
    // 困境
    ("trap", Motif::Trapped),
    ("jail", Motif::Trapped),
    ("cage", Motif::Trapped),
    ("stuck", Motif::Trapped),
    ("hold", Motif::Trapped),
    // 断开
    ("left", Motif::Sever),
    ("leave", Motif::Sever),
    ("quit", Motif::Sever),
    ("stop", Motif::Sever),
    ("close", Motif::Sever),
    ("die", Motif::Sever),
    // 情绪
    ("love", Motif::Affect),
    ("cry", Motif::Affect),
    ("hurt", Motif::Affect),
    ("feel", Motif::Affect),
    ("heart", Motif::Affect),
    ("sweet", Motif::Affect),
    // 循环
    ("again", Motif::Loop),
    ("repeat", Motif::Loop),
    ("forever", Motif::Loop),
];

/// 需要**整词精确匹配**的短词。前缀匹配对它们太危险。
const EXACT: &[(&str, Motif)] = &[
    ("on", Motif::Power),
    ("me", Motif::SelfRef),
    ("my", Motif::SelfRef),
    ("i", Motif::SelfRef),
    ("you", Motif::SelfRef),
    ("we", Motif::SelfRef),
    ("us", Motif::SelfRef),
    ("set", Motif::Data),
    ("run", Motif::Execute),
    ("end", Motif::Sever),
    ("new", Motif::Simulation),
    ("time", Motif::Loop),
    ("still", Motif::Loop),
];

/// 把一个词（已小写、已去标点）映射到主题。
pub fn classify(word: &str) -> Motif {
    if word.is_empty() {
        return Motif::None;
    }
    // 整词表优先：短词必须完全吻合才算命中
    if let Some(&(_, m)) = EXACT.iter().find(|(w, _)| *w == word) {
        return m;
    }
    let mut best: Option<(&str, Motif)> = None;
    for &(prefix, m) in RULES {
        if word.starts_with(prefix) {
            // 最长前缀优先，避免 "on" 抢走 "only"
            if best.map_or(true, |(p, _)| prefix.len() > p.len()) {
                best = Some((prefix, m));
            }
        }
    }
    best.map(|(_, m)| m).unwrap_or(Motif::None)
}

/// 一行歌词里出现的主题集合。
#[derive(Debug, Clone, Default)]
pub struct LineMotifs {
    /// 按出现顺序去重后的主题
    pub motifs: Vec<Motif>,
    /// 该行总词数
    pub words: usize,
    /// 该行字符数（含空格）—— 驱动文字装饰的密度
    pub chars: usize,
}

impl LineMotifs {
    /// 主主题（第一个非 None 的），没有则 None。
    pub fn primary(&self) -> Motif {
        self.motifs.first().copied().unwrap_or(Motif::None)
    }

    /// 是否包含某个主题。
    pub fn has(&self, m: Motif) -> bool {
        self.motifs.contains(&m)
    }

    /// 主题强度：0~1，用于缩放视觉效果。词越密、主题越多越强。
    pub fn intensity(&self) -> f32 {
        if self.words == 0 {
            return 0.0;
        }
        let density = (self.words as f32 / 8.0).min(1.0);
        let variety = (self.motifs.len() as f32 / 3.0).min(1.0);
        (0.35 + density * 0.4 + variety * 0.25).min(1.0)
    }
}

/// 解析一行歌词，抽出主题。不含任何歌词文本存储。
pub fn analyze_line(text: &str) -> LineMotifs {
    let mut out = LineMotifs {
        chars: text.chars().count(),
        ..Default::default()
    };
    for raw in text.split(|c: char| !c.is_alphanumeric() && c != '\'') {
        if raw.is_empty() {
            continue;
        }
        // 去掉所有格后缀，让 "world's" 命中 world
        let w = raw.trim_matches('\'').to_ascii_lowercase();
        if w.is_empty() {
            continue;
        }
        out.words += 1;
        let m = classify(&w);
        if m != Motif::None && !out.motifs.contains(&m) {
            out.motifs.push(m);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_common_words() {
        assert_eq!(classify("execution"), Motif::Execute);
        assert_eq!(classify("execute"), Motif::Execute);
        assert_eq!(classify("world"), Motif::World);
        assert_eq!(classify("simulation"), Motif::Simulation);
        assert_eq!(classify("trapped"), Motif::Trapped);
        assert_eq!(classify("power"), Motif::Power);
        assert_eq!(classify("switch"), Motif::Power);
        assert_eq!(classify("left"), Motif::Sever);
        assert_eq!(classify("love"), Motif::Affect);
    }

    #[test]
    fn unknown_words_are_none() {
        for w in ["zzz", "qqq", "xyzzy", ""] {
            assert_eq!(classify(w), Motif::None, "{w} 不该有主题");
        }
    }

    #[test]
    fn longest_prefix_wins() {
        // "only" 不该被 "on" 抢走成 Power
        assert_ne!(classify("only"), Motif::Power);
        // "on" 单独出现才是 Power
        assert_eq!(classify("on"), Motif::Power);
    }

    #[test]
    fn exact_table_prevents_short_word_hijacking() {
        // 这些词都不该被短前缀误伤
        for w in ["only", "once", "onto", "into", "itself", "in", "if", "it"] {
            let m = classify(w);
            assert_ne!(m, Motif::Power, "{w} 被误判成 Power");
        }
        // 但整词仍应正确命中
        assert_eq!(classify("i"), Motif::SelfRef);
        assert_eq!(classify("me"), Motif::SelfRef);
        assert_eq!(classify("you"), Motif::SelfRef);
        assert_eq!(classify("my"), Motif::SelfRef);
        assert_eq!(classify("set"), Motif::Data);
        assert_eq!(classify("run"), Motif::Execute);
    }

    #[test]
    fn rules_and_exact_do_not_overlap() {
        for &(w, _) in EXACT {
            for &(p, _) in RULES {
                assert_ne!(w, p, "{w} 同时出现在 RULES 和 EXACT 中");
            }
        }
    }

    #[test]
    fn analyze_line_collects_multiple_motifs() {
        let m = analyze_line("Switch on the power line");
        assert!(m.has(Motif::Power), "应命中 Power: {:?}", m.motifs);
        assert_eq!(m.words, 5);
        assert!(m.chars > 0);
    }

    #[test]
    fn analyze_line_handles_punctuation_and_case() {
        let m = analyze_line("EXECUTE(me);");
        assert!(m.has(Motif::Execute), "{:?}", m.motifs);
        assert!(m.has(Motif::SelfRef), "{:?}", m.motifs);
    }

    #[test]
    fn primary_is_first_motif() {
        let m = analyze_line("world simulation");
        assert_eq!(m.primary(), Motif::World);
    }

    #[test]
    fn empty_line_has_no_motifs() {
        let m = analyze_line("");
        assert_eq!(m.words, 0);
        assert_eq!(m.primary(), Motif::None);
        assert_eq!(m.intensity(), 0.0);
    }

    #[test]
    fn intensity_grows_with_content() {
        let thin = analyze_line("me");
        let rich = analyze_line("execute the world simulation left me trapped in love");
        assert!(
            rich.intensity() > thin.intensity(),
            "{} vs {}",
            rich.intensity(),
            thin.intensity()
        );
    }

    #[test]
    fn every_motif_has_a_name() {
        for m in Motif::ALL {
            assert!(!m.name().is_empty());
        }
    }

    #[test]
    fn accent_is_optional_but_valid() {
        for m in Motif::ALL {
            if let Some(c) = m.accent() {
                let (r, g, b) = crate::render::color::rgb_of(c);
                assert!(
                    r as u32 + g as u32 + b as u32 > 120,
                    "{} 的强调色太暗",
                    m.name()
                );
            }
        }
    }

    #[test]
    fn rules_have_no_duplicate_prefix() {
        let mut seen = std::collections::HashMap::new();
        for &(p, m) in RULES {
            if let Some(prev) = seen.insert(p, m) {
                panic!("前缀 {p} 重复: {prev:?} 和 {m:?}");
            }
        }
    }

    #[test]
    fn exact_table_has_no_duplicates() {
        let mut seen = std::collections::HashMap::new();
        for &(w, m) in EXACT {
            if let Some(prev) = seen.insert(w, m) {
                panic!("整词 {w} 重复: {prev:?} 和 {m:?}");
            }
        }
    }
}

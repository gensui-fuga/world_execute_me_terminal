//! 逐词同步：把 LRC 的整行歌词拆成「正在唱哪个词」。
//!
//! LRC 只给整行的开始时间，但 MV 需要知道「此刻唱到第几个词了」。
//! 没有强制对齐数据时，用**音节时长估计**在行内分配时间：
//! 词越长分到的时间越多，但受上下限约束，保证短词落在拍子上。
//!
//! 这套规则来自对成熟实现的观察：
//! - 一个词从它的起音开始「打」出来，最多 `<MAX_TYPE>` 秒打完，
//!   这样短词会整块落在拍点，而不是慢吞吞地爬。
//! - 连音符（`lo-o-ove`）按整个长音时长打出，做出拖腔。
//! - 整行随第一个词出现；器乐间奏（与下一行间隔超过 `<BREAK>` 秒）
//!   时最后停一拍、淡出一拍，而不是硬切。
//!
//! **本模块不存储任何歌词文本的持久副本**，只在运行时持有从 LRC 读来的行。

use crate::lyrics::lrc::LyricLine;

/// 间隔超过这个秒数就认为是器乐间奏。
const BREAK: f32 = 2.0;
/// 一个词最多用多久打完。
const MAX_TYPE: f32 = 0.25;
/// 一个词最少用多久打完（避免瞬现）。
const MIN_TYPE: f32 = 0.06;
/// 一拍的默认时长（BPM 128.6 对应约 0.4666s），可被覆盖。
const DEFAULT_BEAT: f32 = 0.4666;

/// 一个词在时间轴上的位置。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Word {
    /// 在行文本中的字节起止（用于切片）
    pub byte_start: usize,
    pub byte_end: usize,
    /// 起音时间（秒）
    pub onset: f32,
    /// 打完全部字符所需时长
    pub type_dur: f32,
    /// 是否连音符（含 `-`）
    pub melisma: bool,
}

impl Word {
    /// 在 `t` 时刻这个词应该显示出多少个字符。
    pub fn typed_chars(&self, text: &str, t: f32) -> usize {
        if t < self.onset {
            return 0;
        }
        let slice = &text[self.byte_start..self.byte_end];
        let total = slice.chars().count();
        if total == 0 {
            return 0;
        }
        if self.type_dur <= 1e-4 {
            return total;
        }
        let u = ((t - self.onset) / self.type_dur).clamp(0.0, 1.0);
        // 前 35% 打出大部分字符，做出「啪」地一下落下的感觉
        let eased = 1.0 - (1.0 - u).powf(2.2);
        ((eased * total as f32).round() as usize).min(total)
    }

    /// 这个词是否已经唱完。
    pub fn is_done(&self, t: f32) -> bool {
        t >= self.onset + self.type_dur
    }

    /// 处于「正在唱」的窗口内。
    pub fn is_singing(&self, t: f32) -> bool {
        t >= self.onset && t < self.onset + self.type_dur
    }

    /// 词内进度 0~1（未开始为 0，已完成为 1）。
    pub fn progress(&self, t: f32) -> f32 {
        if t <= self.onset {
            0.0
        } else if self.type_dur <= 1e-4 {
            1.0
        } else {
            ((t - self.onset) / self.type_dur).clamp(0.0, 1.0)
        }
    }
}

/// 一行歌词的完整时间信息。
#[derive(Debug, Clone)]
pub struct SyncedLine {
    /// 行文本（运行时从 LRC 读入）
    pub text: String,
    /// 第一个词的起音
    pub start: f32,
    /// 最后一个词的结束
    pub end: f32,
    /// 保持显示到此刻
    pub show_until: f32,
    /// 淡出到此刻
    pub fade_until: f32,
    /// 行内所有词
    pub words: Vec<Word>,
    /// 该行的原始序号
    pub index: usize,
}

impl SyncedLine {
    /// 在 `t` 时刻，整行应该显示多少个字符。
    pub fn typed_chars(&self, t: f32) -> usize {
        let mut n = 0;
        for w in &self.words {
            let c = w.typed_chars(&self.text, t);
            if c == 0 {
                continue;
            }
            // 用字节边界换算成「到这个词结束为止的字符数」
            let upto = self.text[..w.byte_end].chars().count();
            let at_least_one = c > 0;
            if at_least_one {
                n = n.max(upto.min(self.text.chars().count()));
            }
        }
        n
    }

    /// 当前正在唱的那个词（没有则 None）。
    pub fn current_word(&self, t: f32) -> Option<&Word> {
        self.words.iter().find(|w| w.is_singing(t))
    }

    /// 当前词的序号（已经唱完最后一个词则返回 `words.len()`）。
    pub fn current_word_index(&self, t: f32) -> usize {
        self.words.iter().filter(|w| w.onset <= t).count()
    }

    /// 行内进度 0~1。
    pub fn progress(&self, t: f32) -> f32 {
        if t <= self.start {
            return 0.0;
        }
        let span = (self.end - self.start).max(1e-3);
        ((t - self.start) / span).clamp(0.0, 1.0)
    }

    /// 显示不透明度：出现时淡入，间奏时淡出。
    pub fn opacity(&self, t: f32) -> f32 {
        if t < self.start {
            return 0.0;
        }
        if t >= self.fade_until {
            return 0.0;
        }
        if t <= self.show_until {
            // 前 0.12 秒淡入
            let in_u = ((t - self.start) / 0.12).clamp(0.0, 1.0);
            return in_u;
        }
        // 淡出段
        let span = (self.fade_until - self.show_until).max(1e-3);
        1.0 - ((t - self.show_until) / span).clamp(0.0, 1.0)
    }

    /// 距下一个词的剩余时间（用于「即将到来」的预告效果）。
    pub fn next_onset(&self, t: f32) -> Option<f32> {
        self.words
            .iter()
            .map(|w| w.onset)
            .filter(|&o| o > t + 1e-4)
            .fold(None, |acc: Option<f32>, o| {
                Some(acc.map_or(o, |a: f32| a.min(o)))
            })
    }
}

/// 整个歌词时间轴。
#[derive(Debug, Clone, Default)]
pub struct WordTimeline {
    lines: Vec<SyncedLine>,
}

impl WordTimeline {
    /// 从 LRC 行构建。`beat` 是一拍时长，用于间奏时的保持量。
    pub fn build(lines: &[LyricLine], song_len: f32, beat: f32) -> Self {
        let beat = if beat > 1e-3 { beat } else { DEFAULT_BEAT };
        let mut out: Vec<SyncedLine> = Vec::with_capacity(lines.len());

        for (i, ln) in lines.iter().enumerate() {
            let text = ln.text.trim().to_string();
            if text.is_empty() {
                continue;
            }
            let words = split_words(&text, ln.time as f32);
            if words.is_empty() {
                continue;
            }
            let first = words[0].onset;
            let last_end = words
                .iter()
                .map(|w| w.onset + w.type_dur)
                .fold(f32::MIN, f32::max);
            out.push(SyncedLine {
                text,
                start: first,
                end: last_end,
                show_until: last_end,
                fade_until: last_end + beat,
                words,
                index: i,
            });
        }

        // 第二遍：确定每行保持到什么时候
        for k in 0..out.len() {
            let next_start = out
                .get(k + 1)
                .map(|n| n.start)
                .unwrap_or(song_len.max(out[k].end));
            let gap = next_start - out[k].end;
            if gap > BREAK {
                // 器乐间奏：停一拍再淡出一拍
                out[k].show_until = out[k].end + beat;
                out[k].fade_until = out[k].end + 2.0 * beat;
            } else {
                // 紧接下一行：保持到下一行出现，快速淡出
                out[k].show_until = next_start;
                out[k].fade_until = next_start + 0.18;
            }
            // 防御：保持区间不能倒挂
            if out[k].fade_until < out[k].show_until {
                out[k].fade_until = out[k].show_until + 0.05;
            }
        }

        Self { lines: out }
    }

    /// 空时间轴。
    pub fn empty() -> Self {
        Self::default()
    }

    /// 全部行。
    pub fn lines(&self) -> &[SyncedLine] {
        &self.lines
    }

    /// 行数。
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    /// 是否为空。
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// `t` 时刻正在（或即将）显示的行。
    pub fn line_at(&self, t: f32) -> Option<&SyncedLine> {
        // 取最后一个 start <= t 且尚未结束的行
        let mut best: Option<&SyncedLine> = None;
        for l in &self.lines {
            if l.start <= t + 1e-4 && t < l.fade_until {
                best = Some(l);
            }
        }
        // 若还没到第一行，返回第一行（用于预告）
        if best.is_none() {
            if let Some(first) = self.lines.first() {
                if t < first.start && first.start - t < 3.0 {
                    return Some(first);
                }
            }
        }
        best
    }

    /// 行号（找不到返回 None）。
    pub fn line_index_at(&self, t: f32) -> Option<usize> {
        self.line_at(t).map(|l| l.index)
    }

    /// 全曲任意时刻的「唱词活跃度」0~1，用于驱动画面。
    ///
    /// 正在唱 → 高；间奏 → 低但不为零（音乐还在）。
    pub fn vocal_activity(&self, t: f32) -> f32 {
        match self.line_at(t) {
            Some(l) => {
                let op = l.opacity(t);
                // 词内脉冲：每个词打出的瞬间冲一下
                let pulse = l
                    .current_word(t)
                    .map(|w| 1.0 - w.progress(t))
                    .unwrap_or(0.0);
                (op * 0.7 + pulse * 0.3).clamp(0.0, 1.0)
            }
            None => 0.0,
        }
    }

    /// 从 `t` 往后看 `lookahead` 秒内有没有新行要出现。
    pub fn upcoming(&self, t: f32, lookahead: f32) -> Option<(&SyncedLine, f32)> {
        self.lines
            .iter()
            .filter(|l| l.start > t)
            .map(|l| (l, l.start - t))
            .filter(|(_, d)| *d <= lookahead)
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
    }
}

/// 把一行文本切成词，并按音节长度分配时间。
///
/// 时间分配原则：每个词的时长正比于它的字母数（元音组数更准，但字母数足够），
/// 再夹到 `[MIN_TYPE, MAX_TYPE]` 之间 —— 上限保证了短词不会拖，下限保证了长词不瞬现。
/// 连音符（含 `-`）不受上限约束，按真实比例走。
fn split_words(text: &str, onset: f32) -> Vec<Word> {
    let mut spans: Vec<(usize, usize)> = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // 跳过分隔符
        while i < bytes.len() && !is_word_byte(bytes[i]) {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let start = i;
        while i < bytes.len() && is_word_byte(bytes[i]) {
            i += 1;
        }
        spans.push((start, i));
    }
    if spans.is_empty() {
        return Vec::new();
    }

    // 权重：字母数（去掉连字符后）
    let weights: Vec<f32> = spans
        .iter()
        .map(|&(a, b)| {
            text[a..b]
                .chars()
                .filter(|c| c.is_alphanumeric())
                .count()
                .max(1) as f32
        })
        .collect();
    let total: f32 = weights.iter().sum();

    // 这首歌的行一般 2~4 秒，按词数给个合理的总跨度
    let line_span = (0.9 + spans.len() as f32 * 0.62).clamp(1.2, 5.0);

    let mut out = Vec::with_capacity(spans.len());
    let mut cursor = onset;
    for (k, &(a, b)) in spans.iter().enumerate() {
        let frac = weights[k] / total;
        let raw = line_span * frac;
        let melisma = text[a..b].contains('-');
        let dur = if melisma {
            // 连音符按比例，不给上下限
            raw.max(MIN_TYPE)
        } else {
            raw.clamp(MIN_TYPE, MAX_TYPE)
        };
        out.push(Word {
            byte_start: a,
            byte_end: b,
            onset: cursor,
            type_dur: dur,
            melisma,
        });
        cursor += dur;
    }
    out
}

/// 字母、数字、连字符、撇号算作词的一部分。
fn is_word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b'\''
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(t: f64, s: &str) -> LyricLine {
        LyricLine {
            time: t,
            text: s.to_string(),
        }
    }

    fn build(ls: &[LyricLine], len: f32) -> WordTimeline {
        WordTimeline::build(ls, len, 0.4666)
    }

    #[test]
    fn splits_words_on_punctuation() {
        let w = split_words("hello, world!", 0.0);
        assert_eq!(w.len(), 2);
        assert_eq!(w[0].byte_start, 0);
        assert_eq!(w[0].byte_end, 5);
        assert_eq!(w[1].byte_start, 7);
    }

    #[test]
    fn splits_handles_hyphenated_melisma() {
        let w = split_words("lo-o-ove me", 0.0);
        assert_eq!(w.len(), 2);
        assert!(w[0].melisma, "连音符应被标记");
        assert!(!w[1].melisma);
    }

    #[test]
    fn split_handles_apostrophe() {
        let w = split_words("don't stop", 0.0);
        assert_eq!(w.len(), 2);
        assert_eq!(&"don't stop"[w[0].byte_start..w[0].byte_end], "don't");
    }

    #[test]
    fn onsets_are_monotonic() {
        let w = split_words("switch on the power line", 1.0);
        for pair in w.windows(2) {
            assert!(
                pair[1].onset >= pair[0].onset,
                "起音必须递增: {:?}",
                w.iter().map(|x| x.onset).collect::<Vec<_>>()
            );
        }
        assert!((w[0].onset - 1.0).abs() < 1e-5);
    }

    #[test]
    fn typed_chars_grow_to_full() {
        let w = split_words("world", 0.0)[0];
        assert_eq!(w.typed_chars("world", -0.1), 0);
        assert_eq!(w.typed_chars("world", 0.0), 0);
        assert_eq!(w.typed_chars("world", 10.0), 5);
        let mid = w.typed_chars("world", w.type_dur * 0.5);
        assert!((1..5).contains(&mid), "中途应有部分字符: {mid}");
    }

    #[test]
    fn typed_chars_are_monotonic() {
        let w = split_words("execution", 0.0)[0];
        let mut last = 0;
        for i in 0..60 {
            let t = i as f32 * 0.01;
            let n = w.typed_chars("execution", t);
            assert!(n >= last, "字符数不能回退: t={t} {n} < {last}");
            last = n;
        }
        assert_eq!(last, 9);
    }

    #[test]
    fn timeline_drops_empty_lines() {
        let ls = [
            line(0.0, "a"),
            line(1.0, ""),
            line(2.0, "  "),
            line(3.0, "b"),
        ];
        let tl = build(&ls, 10.0);
        assert_eq!(tl.len(), 2);
    }

    #[test]
    fn timeline_has_no_lines_for_empty_input() {
        let tl = build(&[], 10.0);
        assert!(tl.is_empty());
        assert!(tl.line_at(0.0).is_none());
        assert_eq!(tl.vocal_activity(0.0), 0.0);
    }

    #[test]
    fn line_at_finds_active_line() {
        let ls = [line(0.0, "first"), line(5.0, "second")];
        let tl = build(&ls, 20.0);
        assert_eq!(tl.line_at(0.5).map(|l| l.index), Some(0));
        assert_eq!(tl.line_at(5.5).map(|l| l.index), Some(1));
    }

    #[test]
    fn instrument_break_holds_then_fades() {
        // 第一行结束后间隔 5 秒（> BREAK），应停一拍再淡出
        let ls = [line(0.0, "a"), line(8.0, "b")];
        let tl = build(&ls, 20.0);
        let l0 = &tl.lines()[0];
        assert!(l0.show_until < l0.fade_until, "应有淡出区间");
        assert!(
            l0.fade_until < 8.0,
            "间奏时应在下一行之前就淡完: {}",
            l0.fade_until
        );
    }

    #[test]
    fn adjacent_lines_hold_until_next() {
        let ls = [line(0.0, "a"), line(1.5, "b")];
        let tl = build(&ls, 20.0);
        let l0 = &tl.lines()[0];
        assert!(
            (l0.show_until - 1.5).abs() < 0.01,
            "紧邻的行应保持到下一行开始: {}",
            l0.show_until
        );
    }

    #[test]
    fn opacity_is_bounded_and_bell_shaped() {
        let ls = [line(0.0, "hello"), line(9.0, "bye")];
        let tl = build(&ls, 20.0);
        let l = &tl.lines()[0];
        for i in 0..400 {
            let t = i as f32 * 0.02;
            let o = l.opacity(t);
            assert!((0.0..=1.0).contains(&o), "t={t} opacity={o}");
        }
        assert_eq!(l.opacity(l.fade_until + 1.0), 0.0);
        assert_eq!(l.opacity(-1.0), 0.0);
    }

    #[test]
    fn vocal_activity_peaks_during_line() {
        let ls = [line(0.0, "switch on the power line"), line(12.0, "later")];
        let tl = build(&ls, 20.0);
        let during = tl.vocal_activity(1.0);
        let after = tl.vocal_activity(11.5);
        assert!(during > after, "唱词期间活跃度应更高: {during} vs {after}");
    }

    #[test]
    fn current_word_advances_over_time() {
        let ls = [line(0.0, "one two three four five")];
        let tl = build(&ls, 20.0);
        let l = &tl.lines()[0];
        let early = l.current_word(0.01);
        let late = l.current_word(2.0);
        assert!(early.is_some() || late.is_some());
        if let (Some(a), Some(b)) = (early, late) {
            assert!(b.onset >= a.onset);
        }
    }

    #[test]
    fn progress_is_bounded() {
        let ls = [line(0.0, "abcdefg"), line(20.0, "x")];
        let tl = build(&ls, 30.0);
        let l = &tl.lines()[0];
        for i in 0..200 {
            let t = i as f32 * 0.1;
            let p = l.progress(t);
            assert!((0.0..=1.0).contains(&p), "t={t} p={p}");
        }
    }

    #[test]
    fn upcoming_reports_next_line() {
        let ls = [line(0.0, "a"), line(3.0, "b")];
        let tl = build(&ls, 20.0);
        let up = tl.upcoming(1.0, 3.0);
        assert!(up.is_some(), "应报告即将到来的行");
        let (l, dt) = up.unwrap();
        assert_eq!(l.index, 1);
        assert!((dt - 2.0).abs() < 0.01, "剩余 {dt}");
        assert!(tl.upcoming(1.0, 0.5).is_none());
    }

    #[test]
    fn long_line_words_stay_within_limits() {
        let ls = [line(0.0, "supercalifragilisticexpialidocious a b c d e")];
        let tl = build(&ls, 20.0);
        for w in &tl.lines()[0].words {
            if !w.melisma {
                assert!(
                    w.type_dur <= MAX_TYPE + 1e-4,
                    "词时长 {} 超过上限",
                    w.type_dur
                );
            }
            assert!(
                w.type_dur >= MIN_TYPE - 1e-4,
                "词时长 {} 低于下限",
                w.type_dur
            );
        }
    }

    #[test]
    fn no_panics_on_all_ascii_ranges() {
        for i in 0..128u8 {
            let s = format!("a{}b", i as char);
            let _ = split_words(&s, 0.0);
        }
    }

    #[test]
    fn beat_is_overridable() {
        let ls = [line(0.0, "a"), line(9.0, "b")];
        let slow = WordTimeline::build(&ls, 20.0, 1.0);
        let fast = WordTimeline::build(&ls, 20.0, 0.2);
        assert!(
            slow.lines()[0].fade_until > fast.lines()[0].fade_until,
            "慢歌的淡出应更晚"
        );
    }
}

//! 字符单元级效果 —— 与 `postfx` 的子像素级效果互补。
//!
//! 分工：
//! - **tachyonfx**：作用在 `ratatui::Buffer` 的字符单元上，擅长淡入淡出、
//!   溶解、扫入、色相漂移这类整块过渡。本项目用它做场景切换动画。
//! - **手写**：需要逐字符控制的（打字机、字符腐蚀、按亮度显现），
//!   tachyonfx 没有对应原语。
//!
//! 注意 tachyonfx 0.21 确实自带 `fx::glitch`，但它是**字符单元级**的，
//! 做不出子像素级的 RGB 通道分离与扫描线；本项目的故障效果主力在
//! [`crate::render::postfx`]，这里只负责切换时的溶解/扫入。

use std::time::Duration as StdDuration;

use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use tachyonfx::{fx, Effect, Interpolation, Motion};

/// 场景切换动画类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionKind {
    /// 溶解：随机字符逐个消失/出现
    Dissolve,
    /// 从左侧扫入
    SweepIn,
    /// 前景色淡入
    FadeFrom,
    /// 色相漂移
    HslShift,
    /// 反向溶解（出场）
    Coalesce,
}

impl TransitionKind {
    /// 全部类型（供调试轮换）。
    pub const ALL: [TransitionKind; 5] = [
        TransitionKind::Dissolve,
        TransitionKind::SweepIn,
        TransitionKind::FadeFrom,
        TransitionKind::HslShift,
        TransitionKind::Coalesce,
    ];
}

/// 一次场景切换动画。
pub struct Transition {
    kind: TransitionKind,
    effect: Option<Effect>,
    total: StdDuration,
    elapsed: StdDuration,
    /// 是否已经播完（或从未开始）
    finished: bool,
}

impl Transition {
    /// 新建。`ms` 为时长（毫秒）。
    pub fn new(kind: TransitionKind, ms: u64, color: Color) -> Self {
        let ms = ms.max(1);
        let timer = (ms as u32, Interpolation::QuadOut);
        let effect = match kind {
            TransitionKind::Dissolve => Some(fx::dissolve(timer)),
            TransitionKind::SweepIn => {
                Some(fx::sweep_in(Motion::LeftToRight, 16, 40, color, timer))
            }
            TransitionKind::FadeFrom => Some(fx::fade_from_fg(color, timer)),
            TransitionKind::HslShift => Some(fx::hsl_shift(Some([40.0, 0.3, 0.0]), None, timer)),
            TransitionKind::Coalesce => Some(fx::coalesce(timer)),
        };
        Self {
            kind,
            effect,
            total: StdDuration::from_millis(ms),
            elapsed: StdDuration::ZERO,
            finished: false,
        }
    }

    /// 动画类型。
    pub fn kind(&self) -> TransitionKind {
        self.kind
    }

    /// 是否已播完。
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    /// 进度 0..1。
    pub fn progress(&self) -> f32 {
        if self.total.is_zero() {
            return 1.0;
        }
        (self.elapsed.as_secs_f32() / self.total.as_secs_f32()).clamp(0.0, 1.0)
    }

    /// 推进一帧。返回 `true` 表示本次调用后动画结束。
    pub fn process(&mut self, dt: StdDuration, buf: &mut Buffer, area: Rect) -> bool {
        if self.finished {
            return true;
        }
        self.elapsed += dt;
        if let Some(e) = self.effect.as_mut() {
            // tachyonfx 在非 wasm 目标下用自己的 Duration 别名
            let tdt: tachyonfx::Duration = dt.into();
            e.process(tdt, buf, area);
            if e.done() {
                self.effect = None;
                self.finished = true;
                return true;
            }
        } else {
            self.finished = true;
            return true;
        }
        false
    }
}

/// 打字机：按 `progress` 显现 `text` 的前若干个字符。
///
/// 返回实际写出的字符数。
pub fn typewriter(
    buf: &mut Buffer,
    area: Rect,
    y: u16,
    x0: u16,
    text: &str,
    progress: f32,
    style: Style,
) -> usize {
    if y >= area.height {
        return 0;
    }
    let chars: Vec<char> = text.chars().collect();
    let n = ((chars.len() as f32) * progress.clamp(0.0, 1.0)).round() as usize;
    let mut written = 0usize;
    for (i, ch) in chars.iter().take(n).enumerate() {
        let x = x0 + i as u16;
        if x >= area.width {
            break;
        }
        let cell = &mut buf[(area.x + x, area.y + y)];
        cell.set_char(*ch);
        cell.set_style(style);
        written += 1;
    }
    written
}

/// 字符腐蚀：按 `amount` 比例把字符替换成 `charset` 里的随机符号。
///
/// 用于故障段落 —— 只动字符，不动颜色，因此不会破坏配色。
pub fn corrupt(buf: &mut Buffer, area: Rect, amount: f32, charset: &[char], rng: &mut SmallRng) {
    let amount = amount.clamp(0.0, 1.0);
    if amount <= 0.0 || charset.is_empty() {
        return;
    }
    for y in 0..area.height {
        for x in 0..area.width {
            if rng.gen::<f32>() >= amount {
                continue;
            }
            let c = charset[rng.gen_range(0..charset.len())];
            let cell = &mut buf[(area.x + x, area.y + y)];
            // 空格保持空格，否则会出现「空处冒字符」的脏感
            if cell.symbol() != " " {
                cell.set_char(c);
            }
        }
    }
}

/// 按亮度阈值显现：覆盖率低于阈值的单元被清空。
///
/// 用于「从下往上」或「按能量」逐级显现画面的效果。
pub fn reveal_by_luma(buf: &mut Buffer, area: Rect, threshold: f32) {
    let t = threshold.clamp(0.0, 1.0);
    for y in 0..area.height {
        for x in 0..area.width {
            let cell = &buf[(area.x + x, area.y + y)];
            if cell.symbol() == " " {
                continue;
            }
            let c = cell.fg;
            let luma = crate::render::color::luma(c);
            if luma < t {
                let cell = &mut buf[(area.x + x, area.y + y)];
                cell.set_char(' ');
            }
        }
    }
}

/// 按行擦除：保留前 `keep_rows` 行，其余清空。
///
/// 用于「逐行浮现」的转场。
pub fn reveal_rows(buf: &mut Buffer, area: Rect, keep_rows: u16, from_bottom: bool) {
    let keep = keep_rows.min(area.height);
    for y in 0..area.height {
        let visible = if from_bottom {
            y >= area.height - keep
        } else {
            y < keep
        };
        if visible {
            continue;
        }
        for x in 0..area.width {
            buf[(area.x + x, area.y + y)].set_char(' ');
        }
    }
}

/// 生成一段「字符损坏」用的字符集（半角片假名 + 数字 + 符号）。
///
/// 只使用**半角**字符：全角字符占两列，会撕裂布局。
pub fn glitch_charset() -> Vec<char> {
    let mut v: Vec<char> = Vec::with_capacity(80);
    // 半角片假名 U+FF66..U+FF9D
    for c in '\u{FF66}'..='\u{FF9D}' {
        v.push(c);
    }
    for c in '0'..='9' {
        v.push(c);
    }
    for c in [
        '!', '@', '#', '$', '%', '&', '*', '+', '-', '=', '/', '\\', '|', '<', '>', '?',
    ] {
        v.push(c);
    }
    v
}

/// 生成代码雨用的字符集（半角片假名 + 数字）。
pub fn rain_charset() -> Vec<char> {
    let mut v: Vec<char> = Vec::with_capacity(64);
    for c in '\u{FF66}'..='\u{FF9D}' {
        v.push(c);
    }
    for c in '0'..='9' {
        v.push(c);
    }
    v
}

/// 淡出：按 `t` 把整个区域推向背景色。
pub fn fade_out(buf: &mut Buffer, area: Rect, t: f32, to: Color) {
    let t = t.clamp(0.0, 1.0);
    for y in 0..area.height {
        for x in 0..area.width {
            let cell = &buf[(area.x + x, area.y + y)];
            if cell.symbol() == " " {
                continue;
            }
            let fg = crate::render::color::lerp(cell.fg, to, t);
            let cell = &mut buf[(area.x + x, area.y + y)];
            cell.set_style(Style::default().fg(fg));
        }
    }
}

/// 逐字符的「呼吸」亮度：按 `phase` 在基色与提亮之间摆动。
pub fn breathe(buf: &mut Buffer, area: Rect, phase: f32, amount: f32) {
    let k = 0.5 + 0.5 * (phase * std::f32::consts::TAU).sin();
    let amt = k * amount.clamp(0.0, 1.0);
    for y in 0..area.height {
        for x in 0..area.width {
            let cell = &buf[(area.x + x, area.y + y)];
            if cell.symbol() == " " {
                continue;
            }
            let fg = crate::render::color::shade(cell.fg, amt);
            let cell = &mut buf[(area.x + x, area.y + y)];
            cell.set_style(Style::default().fg(fg));
        }
    }
}

/// 确定性 RNG（供调用方复用）。
pub fn rng(seed: u64) -> SmallRng {
    SmallRng::seed_from_u64(seed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::color as col;

    fn buf(w: u16, h: u16) -> (Buffer, Rect) {
        let area = Rect::new(0, 0, w, h);
        (Buffer::empty(area), area)
    }

    #[test]
    fn transition_finishes_after_enough_time() {
        let (mut b, area) = buf(20, 10);
        let mut t = Transition::new(TransitionKind::Dissolve, 100, col::CYAN);
        let step = StdDuration::from_millis(20);
        let mut done = false;
        for _ in 0..20 {
            done = t.process(step, &mut b, area);
            if done {
                break;
            }
        }
        assert!(done, "过渡未在预期时间内结束");
        assert!(t.is_finished());
        assert!((t.progress() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn transition_reports_progress() {
        let (mut b, area) = buf(20, 10);
        let mut t = Transition::new(TransitionKind::Dissolve, 100, col::CYAN);
        t.process(StdDuration::from_millis(50), &mut b, area);
        let p = t.progress();
        assert!((0.2..0.8).contains(&p), "进度 {p} 不合理");
    }

    #[test]
    fn all_transition_kinds_can_run() {
        for kind in TransitionKind::ALL {
            let (mut b, area) = buf(10, 5);
            let mut t = Transition::new(kind, 50, col::MAGENTA);
            for _ in 0..10 {
                if t.process(StdDuration::from_millis(10), &mut b, area) {
                    break;
                }
            }
            assert!(t.is_finished(), "{kind:?} 未结束");
        }
    }

    #[test]
    fn finished_transition_stays_finished() {
        let (mut b, area) = buf(10, 5);
        let mut t = Transition::new(TransitionKind::Dissolve, 10, col::CYAN);
        for _ in 0..5 {
            t.process(StdDuration::from_millis(10), &mut b, area);
        }
        assert!(t.process(StdDuration::from_millis(10), &mut b, area));
    }

    #[test]
    fn typewriter_writes_prefix_only() {
        let (mut b, area) = buf(20, 3);
        let n = typewriter(
            &mut b,
            area,
            0,
            0,
            "HELLO",
            0.6,
            Style::default().fg(col::CYAN),
        );
        assert_eq!(n, 3);
        assert_eq!(b[(0, 0)].symbol(), "H");
        assert_eq!(b[(2, 0)].symbol(), "L");
        assert_eq!(b[(3, 0)].symbol(), " ");
    }

    #[test]
    fn typewriter_full_progress_writes_all() {
        let (mut b, area) = buf(20, 3);
        let n = typewriter(&mut b, area, 1, 2, "ABC", 1.0, Style::default());
        assert_eq!(n, 3);
        assert_eq!(b[(2, 1)].symbol(), "A");
        assert_eq!(b[(4, 1)].symbol(), "C");
    }

    #[test]
    fn typewriter_respects_right_edge() {
        let (mut b, area) = buf(3, 1);
        let n = typewriter(&mut b, area, 0, 0, "ABCDEF", 1.0, Style::default());
        assert_eq!(n, 3, "不应写出区域外");
    }

    #[test]
    fn typewriter_out_of_range_row_is_noop() {
        let (mut b, area) = buf(10, 2);
        assert_eq!(
            typewriter(&mut b, area, 99, 0, "X", 1.0, Style::default()),
            0
        );
    }

    #[test]
    fn corrupt_changes_only_non_blank_cells() {
        let (mut b, area) = buf(20, 5);
        for x in 0..20 {
            b[(x, 0)].set_char('#');
        }
        // 第二行保持空白
        let mut r = rng(1);
        corrupt(&mut b, area, 1.0, &['X'], &mut r);
        for x in 0..20 {
            assert_eq!(b[(x, 0)].symbol(), "X");
            assert_eq!(b[(x, 1)].symbol(), " ");
        }
    }

    #[test]
    fn corrupt_zero_amount_is_noop() {
        let (mut b, area) = buf(10, 3);
        b[(0, 0)].set_char('#');
        let mut r = rng(1);
        corrupt(&mut b, area, 0.0, &['X'], &mut r);
        assert_eq!(b[(0, 0)].symbol(), "#");
    }

    #[test]
    fn corrupt_with_empty_charset_is_noop() {
        let (mut b, area) = buf(10, 3);
        b[(0, 0)].set_char('#');
        let mut r = rng(1);
        corrupt(&mut b, area, 1.0, &[], &mut r);
        assert_eq!(b[(0, 0)].symbol(), "#");
    }

    #[test]
    fn corrupt_is_deterministic_for_same_seed() {
        let mk = || {
            let (mut b, area) = buf(30, 6);
            for y in 0..6 {
                for x in 0..30 {
                    b[(x, y)].set_char('#');
                }
            }
            let mut r = rng(1234);
            corrupt(&mut b, area, 0.5, &['A', 'B', 'C'], &mut r);
            b
        };
        let a = mk();
        let c = mk();
        for y in 0..6 {
            for x in 0..30 {
                assert_eq!(a[(x, y)].symbol(), c[(x, y)].symbol());
            }
        }
    }

    #[test]
    fn reveal_by_luma_hides_dark_cells() {
        let (mut b, area) = buf(4, 1);
        b[(0, 0)].set_char('#');
        b[(0, 0)].set_style(Style::default().fg(col::WHITE));
        b[(1, 0)].set_char('#');
        b[(1, 0)].set_style(Style::default().fg(col::Color::Rgb(20, 20, 20)));
        reveal_by_luma(&mut b, area, 0.5);
        assert_eq!(b[(0, 0)].symbol(), "#");
        assert_eq!(b[(1, 0)].symbol(), " ");
    }

    #[test]
    fn reveal_rows_keeps_top_when_from_bottom_false() {
        let (mut b, area) = buf(4, 4);
        for y in 0..4 {
            b[(0, y)].set_char('#');
        }
        reveal_rows(&mut b, area, 2, false);
        assert_eq!(b[(0, 0)].symbol(), "#");
        assert_eq!(b[(0, 1)].symbol(), "#");
        assert_eq!(b[(0, 2)].symbol(), " ");
        assert_eq!(b[(0, 3)].symbol(), " ");
    }

    #[test]
    fn reveal_rows_keeps_bottom_when_from_bottom_true() {
        let (mut b, area) = buf(4, 4);
        for y in 0..4 {
            b[(0, y)].set_char('#');
        }
        reveal_rows(&mut b, area, 2, true);
        assert_eq!(b[(0, 0)].symbol(), " ");
        assert_eq!(b[(0, 1)].symbol(), " ");
        assert_eq!(b[(0, 2)].symbol(), "#");
        assert_eq!(b[(0, 3)].symbol(), "#");
    }

    #[test]
    fn glitch_charset_is_halfwidth_only() {
        for c in glitch_charset() {
            // 半角字符宽度为 1；这里用 unicode 宽度近似判断
            assert!(
                (c as u32) < 0x3000 || (0xFF61..=0xFF9F).contains(&(c as u32)),
                "字符 {c:?} 可能占两列"
            );
        }
    }

    #[test]
    fn rain_charset_has_katakana_and_digits() {
        let v = rain_charset();
        assert!(v.iter().any(|c| ('\u{FF66}'..='\u{FF9D}').contains(c)));
        assert!(v.iter().any(|c| c.is_ascii_digit()));
        assert!(!v.is_empty());
    }

    #[test]
    fn fade_out_moves_toward_target_color() {
        let (mut b, area) = buf(2, 1);
        b[(0, 0)].set_char('#');
        b[(0, 0)].set_style(Style::default().fg(col::WHITE));
        fade_out(&mut b, area, 1.0, col::BLACK);
        assert_eq!(b[(0, 0)].fg, col::BLACK);
    }

    #[test]
    fn breathe_changes_colors_over_phase() {
        // breathe 会就地改写 fg，因此每次都要用干净的 buffer 采样
        let sample = |phase: f32| {
            let (mut b, area) = buf(2, 1);
            b[(0, 0)].set_char('#');
            b[(0, 0)].set_style(Style::default().fg(col::CYAN));
            breathe(&mut b, area, phase, 1.0);
            b[(0, 0)].fg
        };
        assert_ne!(sample(0.25), sample(0.75), "呼吸效果应随相位变化");
    }

    #[test]
    fn breathe_skips_empty_cells() {
        let (mut b, area) = buf(2, 1);
        breathe(&mut b, area, 0.25, 1.0);
        // 空格单元不应被改写
        assert_eq!(b[(0, 0)].fg, Color::Reset);
    }
}

//! 颜色：调色板、渐变、亮度与色相运算。
//!
//! 全部基于 `Color::Rgb`（真彩）。终端不支持真彩时由 `to_ansi256` 降级，
//! 保证在 256 色终端里依然有层次。

pub use ratatui::style::Color;

/// 青 —— 主色调，科技感
pub const CYAN: Color = Color::Rgb(0, 255, 255);
/// 品红 —— 强调色
pub const MAGENTA: Color = Color::Rgb(255, 0, 255);
/// 暗红 —— 危险/低能量
pub const DARK_RED: Color = Color::Rgb(139, 0, 0);
/// 亮红 —— 高能量/爆点
pub const BRIGHT_RED: Color = Color::Rgb(255, 68, 68);
/// 灰白 —— 正文
pub const GREY_WHITE: Color = Color::Rgb(170, 170, 170);
/// 深灰 —— 网格/背景
pub const DARK_GREY: Color = Color::Rgb(51, 51, 51);
/// 纯黑
pub const BLACK: Color = Color::Rgb(0, 0, 0);
/// 纯白 —— 闪光
pub const WHITE: Color = Color::Rgb(255, 255, 255);

/// 主题默认渐变（低 → 高能量）。
pub const THEME_RAMP: [Color; 5] = [DARK_GREY, DARK_RED, MAGENTA, CYAN, WHITE];

/// 取 RGB 分量。非 RGB 颜色按终端默认色近似。
pub fn rgb_of(c: Color) -> (u8, u8, u8) {
    match c {
        Color::Rgb(r, g, b) => (r, g, b),
        Color::Black => (0, 0, 0),
        Color::Red => (205, 0, 0),
        Color::Green => (0, 205, 0),
        Color::Yellow => (205, 205, 0),
        Color::Blue => (0, 0, 238),
        Color::Magenta => (205, 0, 205),
        Color::Cyan => (0, 205, 205),
        Color::Gray => (229, 229, 229),
        Color::DarkGray => (127, 127, 127),
        Color::LightRed => (255, 0, 0),
        Color::LightGreen => (0, 255, 0),
        Color::LightYellow => (255, 255, 0),
        Color::LightBlue => (92, 92, 255),
        Color::LightMagenta => (255, 0, 255),
        Color::LightCyan => (0, 255, 255),
        Color::White => (255, 255, 255),
        Color::Indexed(i) => ansi256_to_rgb(i),
        // Reset 是「终端默认色」；导出时没有终端可继承，按黑处理。
        // 本项目所有绘制都会显式设置颜色，Reset 只可能来自未初始化的单元。
        Color::Reset => (0, 0, 0),
    }
}

/// ANSI 256 调色板 → RGB。
pub fn ansi256_to_rgb(i: u8) -> (u8, u8, u8) {
    match i {
        0..=15 => {
            const BASE: [(u8, u8, u8); 16] = [
                (0, 0, 0),
                (128, 0, 0),
                (0, 128, 0),
                (128, 128, 0),
                (0, 0, 128),
                (128, 0, 128),
                (0, 128, 128),
                (192, 192, 192),
                (128, 128, 128),
                (255, 0, 0),
                (0, 255, 0),
                (255, 255, 0),
                (0, 0, 255),
                (255, 0, 255),
                (0, 255, 255),
                (255, 255, 255),
            ];
            BASE[i as usize]
        }
        16..=231 => {
            let i = i - 16;
            let r = i / 36;
            let g = (i % 36) / 6;
            let b = i % 6;
            let step = |v: u8| -> u8 {
                if v == 0 {
                    0
                } else {
                    55 + v * 40
                }
            };
            (step(r), step(g), step(b))
        }
        _ => {
            let v = 8 + (i - 232) * 10;
            (v, v, v)
        }
    }
}

/// 把真彩降级到最接近的 ANSI 256 索引。
pub fn to_ansi256(c: Color) -> u8 {
    let (r, g, b) = rgb_of(c);
    // 灰阶通道
    if r == g && g == b {
        if r < 8 {
            return 16;
        }
        if r > 238 {
            return 231;
        }
        return 232 + ((r as u16 - 8) / 10).min(23) as u8;
    }
    let q = |v: u8| -> u8 {
        if v < 48 {
            0
        } else if v < 115 {
            1
        } else {
            ((v as u16 - 35) / 40).min(5) as u8
        }
    };
    16 + 36 * q(r) + 6 * q(g) + q(b)
}

/// 线性插值两个颜色。`t` 会被钳到 `0..1`。
pub fn lerp(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let (ar, ag, ab) = rgb_of(a);
    let (br, bg, bb) = rgb_of(b);
    let m = |x: u8, y: u8| -> u8 { (x as f32 + (y as f32 - x as f32) * t).round() as u8 };
    Color::Rgb(m(ar, br), m(ag, bg), m(ab, bb))
}

/// 按亮度缩放。`k > 1` 提亮，`k < 1` 压暗。
pub fn scale(c: Color, k: f32) -> Color {
    let (r, g, b) = rgb_of(c);
    let f = |v: u8| -> u8 { ((v as f32 * k).clamp(0.0, 255.0)).round() as u8 };
    Color::Rgb(f(r), f(g), f(b))
}

/// 按感知亮度把颜色推向白（`k > 0`）或黑（`k < 0`）。
pub fn shade(c: Color, k: f32) -> Color {
    if k >= 0.0 {
        lerp(c, WHITE, k)
    } else {
        lerp(c, BLACK, -k)
    }
}

/// 感知亮度（0..1，Rec.709 权重）。
pub fn luma(c: Color) -> f32 {
    let (r, g, b) = rgb_of(c);
    (0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32) / 255.0
}

/// RGB → HSL。返回 `(h_deg, s, l)`。
pub fn to_hsl(c: Color) -> (f32, f32, f32) {
    let (r, g, b) = rgb_of(c);
    let (r, g, b) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    if (max - min).abs() < 1e-6 {
        return (0.0, 0.0, l);
    }
    let d = max - min;
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    let h = if max == r {
        ((g - b) / d + if g < b { 6.0 } else { 0.0 }) / 6.0
    } else if max == g {
        ((b - r) / d + 2.0) / 6.0
    } else {
        ((r - g) / d + 4.0) / 6.0
    };
    (h * 360.0, s, l)
}

/// HSL → RGB。
pub fn from_hsl(h: f32, s: f32, l: f32) -> Color {
    let h = (h.rem_euclid(360.0)) / 360.0;
    let s = s.clamp(0.0, 1.0);
    let l = l.clamp(0.0, 1.0);
    if s <= 1e-6 {
        let v = (l * 255.0).round() as u8;
        return Color::Rgb(v, v, v);
    }
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let f = |mut t: f32| -> f32 {
        if t < 0.0 {
            t += 1.0;
        }
        if t > 1.0 {
            t -= 1.0;
        }
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 1.0 / 2.0 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    let r = (f(h + 1.0 / 3.0) * 255.0).round() as u8;
    let g = (f(h) * 255.0).round() as u8;
    let b = (f(h - 1.0 / 3.0) * 255.0).round() as u8;
    Color::Rgb(r, g, b)
}

/// 色相旋转 `deg` 度（保持饱和度与亮度）。
pub fn hue_shift(c: Color, deg: f32) -> Color {
    let (h, s, l) = to_hsl(c);
    from_hsl(h + deg, s, l)
}

/// 在渐变带上取样。`stops` 至少 2 个，`t` 会被钳到 `0..1`。
pub fn ramp(stops: &[Color], t: f32) -> Color {
    if stops.is_empty() {
        return WHITE;
    }
    if stops.len() == 1 {
        return stops[0];
    }
    let t = t.clamp(0.0, 1.0);
    let x = t * (stops.len() - 1) as f32;
    let i = (x.floor() as usize).min(stops.len() - 2);
    lerp(stops[i], stops[i + 1], x - i as f32)
}

/// 主题渐变取样。
pub fn theme(t: f32) -> Color {
    ramp(&THEME_RAMP, t)
}

/// 闪烁：按相位在基色与提亮之间来回。
pub fn blink(c: Color, phase: f32) -> Color {
    let k = 0.5 + 0.5 * (phase * std::f32::consts::TAU).sin();
    shade(c, k * 0.6)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lerp_endpoints_are_exact() {
        assert_eq!(rgb_of(lerp(CYAN, MAGENTA, 0.0)), rgb_of(CYAN));
        assert_eq!(rgb_of(lerp(CYAN, MAGENTA, 1.0)), rgb_of(MAGENTA));
        assert_eq!(rgb_of(lerp(CYAN, MAGENTA, -5.0)), rgb_of(CYAN));
        assert_eq!(rgb_of(lerp(CYAN, MAGENTA, 5.0)), rgb_of(MAGENTA));
    }

    #[test]
    fn lerp_midpoint_is_average() {
        let c = lerp(Color::Rgb(0, 0, 0), Color::Rgb(100, 200, 50), 0.5);
        assert_eq!(rgb_of(c), (50, 100, 25));
    }

    #[test]
    fn scale_clamps_at_bounds() {
        assert_eq!(rgb_of(scale(WHITE, 10.0)), (255, 255, 255));
        assert_eq!(rgb_of(scale(WHITE, -3.0)), (0, 0, 0));
    }

    #[test]
    fn shade_moves_toward_white_and_black() {
        assert!(luma(shade(CYAN, 0.5)) > luma(CYAN));
        assert!(luma(shade(CYAN, -0.5)) < luma(CYAN));
    }

    #[test]
    fn hsl_roundtrip_is_stable() {
        for c in [
            CYAN,
            MAGENTA,
            DARK_RED,
            BRIGHT_RED,
            GREY_WHITE,
            Color::Rgb(12, 200, 77),
        ] {
            let (h, s, l) = to_hsl(c);
            let back = from_hsl(h, s, l);
            let a = rgb_of(c);
            let b = rgb_of(back);
            assert!(
                (a.0 as i32 - b.0 as i32).abs() <= 1
                    && (a.1 as i32 - b.1 as i32).abs() <= 1
                    && (a.2 as i32 - b.2 as i32).abs() <= 1,
                "HSL 往返失真: {a:?} → {b:?}"
            );
        }
    }

    #[test]
    fn hue_shift_by_zero_is_identity() {
        let c = Color::Rgb(12, 200, 77);
        let (a, b) = (rgb_of(c), rgb_of(hue_shift(c, 0.0)));
        assert!((a.0 as i32 - b.0 as i32).abs() <= 1);
        assert!((a.1 as i32 - b.1 as i32).abs() <= 1);
        assert!((a.2 as i32 - b.2 as i32).abs() <= 1);
    }

    #[test]
    fn hue_shift_by_360_is_identity() {
        let c = Color::Rgb(200, 30, 90);
        let (a, b) = (rgb_of(c), rgb_of(hue_shift(c, 360.0)));
        assert!((a.0 as i32 - b.0 as i32).abs() <= 2);
        assert!((a.1 as i32 - b.1 as i32).abs() <= 2);
        assert!((a.2 as i32 - b.2 as i32).abs() <= 2);
    }

    #[test]
    fn ramp_endpoints_and_middle() {
        let stops = [BLACK, WHITE];
        assert_eq!(rgb_of(ramp(&stops, 0.0)), (0, 0, 0));
        assert_eq!(rgb_of(ramp(&stops, 1.0)), (255, 255, 255));
        assert_eq!(rgb_of(ramp(&stops, 0.5)), (128, 128, 128));
    }

    #[test]
    fn ramp_handles_degenerate_stop_lists() {
        assert_eq!(ramp(&[], 0.5), WHITE);
        assert_eq!(ramp(&[CYAN], 0.9), CYAN);
    }

    #[test]
    fn ansi256_gray_ramp_is_monotonic() {
        let mut prev = -1i32;
        for i in 232..=255u8 {
            let (r, g, b) = ansi256_to_rgb(i);
            assert_eq!(r, g);
            assert_eq!(g, b);
            assert!(r as i32 > prev, "灰阶应单调递增");
            prev = r as i32;
        }
    }

    #[test]
    fn ansi256_roundtrip_for_pure_colors() {
        // 纯红/纯绿/纯蓝应映射到 16..231 立方体内并近似还原
        for c in [
            Color::Rgb(255, 0, 0),
            Color::Rgb(0, 255, 0),
            Color::Rgb(0, 0, 255),
        ] {
            let idx = to_ansi256(c);
            assert!((16..=231).contains(&idx), "索引 {idx} 不在立方体内");
            let (r, g, b) = ansi256_to_rgb(idx);
            let (cr, cg, cb) = rgb_of(c);
            assert!((r as i32 - cr as i32).abs() < 60);
            assert!((g as i32 - cg as i32).abs() < 60);
            assert!((b as i32 - cb as i32).abs() < 60);
        }
    }

    #[test]
    fn theme_ramp_is_monotonic_in_luma_at_ends() {
        assert!(luma(theme(0.0)) < luma(theme(1.0)));
    }

    #[test]
    fn blink_stays_in_range() {
        // u8 通道值域天然合法，这里真正要验证的是：
        // blink 不 panic，且确实随相位变化。
        let a = blink(CYAN, 0.0);
        let b = blink(CYAN, 0.25);
        let c = blink(CYAN, 0.75);
        assert!(a != b || b != c, "blink 应随相位变化");
    }
}

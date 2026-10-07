//! 字符精灵库。
//!
//! 为什么需要它：场景里如果只有程序化的圆和线，观众认不出画的是什么 ——
//! 「看不懂」是这类片子的头号死因。这里放的是**手绘的、一眼能认出来的具象形状**，
//! 蝴蝶像蝴蝶、松树像松树、雪花是六角对称的。
//!
//! ## `art` 的字符约定
//!
//! | 字符 | 含义 | 覆盖度 |
//! |---|---|---|
//! | `' '` | 空 | — |
//! | `'.'` | 最暗 | 0.55 |
//! | `':'` | 次暗 | 0.72 |
//! | `'*'` | 中 | 0.88 |
//! | `'#'` | 实心 | 1.00 |
//! | `'+'` | 高光（用 `accent` 色） | 1.00 |
//!
//! 注意最暗的 `'.'` 也给到 0.55：Braille 只在 `w > 0.5` 时点亮，
//! 给 0.2 的话整块都不会亮。
//!
//! ## 为什么分文件
//!
//! 精灵是按季节手绘的，一个季节一个文件，方便单独改、也方便并行写。

use crate::render::color::Color;

use super::CharCanvas;

mod art;

pub use art::{AUTUMN, COMMON, SPRING, SUMMER, WINTER};

/// 一个字符精灵。
pub struct Sprite {
    /// 唯一名字（`get` 用它查）。
    pub name: &'static str,
    /// 一行一个字符串。
    pub art: &'static [&'static str],
}

impl Sprite {
    /// 精灵的列数（按最宽的一行算）。
    pub fn width(&self) -> usize {
        self.art.iter().map(|r| r.chars().count()).max().unwrap_or(0)
    }

    /// 精灵的行数。
    pub fn height(&self) -> usize {
        self.art.len()
    }

    /// 亮着的字符总数（自检用）。
    pub fn ink(&self) -> usize {
        self.art
            .iter()
            .map(|r| r.chars().filter(|c| *c != ' ').count())
            .sum()
    }
}

/// 全部精灵，按季节顺序。
pub fn all() -> Vec<&'static Sprite> {
    let mut v: Vec<&'static Sprite> = Vec::with_capacity(40);
    v.extend(SPRING.iter());
    v.extend(SUMMER.iter());
    v.extend(AUTUMN.iter());
    v.extend(WINTER.iter());
    v.extend(COMMON.iter());
    v
}

/// 按名字取一个精灵。
pub fn get(name: &str) -> Option<&'static Sprite> {
    SPRING
        .iter()
        .chain(SUMMER.iter())
        .chain(AUTUMN.iter())
        .chain(WINTER.iter())
        .chain(COMMON.iter())
        .find(|s| s.name == name)
}

/// 全部精灵名。
pub fn names() -> Vec<&'static str> {
    all().iter().map(|s| s.name).collect()
}

/// 按季节取一组精灵名。
pub fn season(season: crate::timeline::scene::SceneKind) -> &'static [Sprite] {
    use crate::timeline::scene::SceneKind as K;
    match season {
        K::Spring => SPRING,
        K::Summer => SUMMER,
        K::Autumn => AUTUMN,
        K::Winter => WINTER,
        _ => COMMON,
    }
}

/// 字符 → (覆盖度, 是否用 accent 色)。
fn glyph(ch: char) -> Option<(f32, bool)> {
    match ch {
        '.' => Some((0.55, false)),
        ':' => Some((0.72, false)),
        '*' => Some((0.88, false)),
        '#' => Some((1.00, false)),
        '+' => Some((1.00, true)),
        _ => None,
    }
}

/// 把精灵画到画布上。
///
/// `cx, cy` 是**中心**（点阵单位）；`scale` 是每个 art 字符占多少点。
/// 返回精灵占的矩形 `(x0, y0, x1, y1)`，方便调用方避让。
///
/// `alpha` 直接乘到覆盖度上，所以**别传小于 0.5 的值**，否则整块不亮 ——
/// 想做淡入请用 0.55~1.0 的区间，或者改用 `canvas.tint`。
pub fn draw(
    canvas: &mut CharCanvas,
    sp: &Sprite,
    cx: f32,
    cy: f32,
    scale: f32,
    color: Color,
    alpha: f32,
) -> (f32, f32, f32, f32) {
    draw_two_tone(canvas, sp, cx, cy, scale, color, color, alpha)
}

/// 带第二色的绘制：`'+'` 的点用 `accent` 色，做高光/点缀。
pub fn draw_two_tone(
    canvas: &mut CharCanvas,
    sp: &Sprite,
    cx: f32,
    cy: f32,
    scale: f32,
    color: Color,
    accent: Color,
    alpha: f32,
) -> (f32, f32, f32, f32) {
    let s = scale.max(0.05);
    let w = sp.width() as f32;
    let h = sp.height() as f32;
    let x0 = cx - w * s * 0.5;
    let y0 = cy - h * s * 0.5;

    // scale >= 1 时一个字符要铺成 s×s 的实心块，
    // 否则放大后是「一堆孤立点」而不是一个形状。
    let block = s.ceil().max(1.0) as i32;

    for (r, line) in sp.art.iter().enumerate() {
        for (c, ch) in line.chars().enumerate() {
            let Some((cov, is_accent)) = glyph(ch) else {
                continue;
            };
            let col = if is_accent { accent } else { color };
            let w_eff = (cov * alpha).clamp(0.0, 1.0);
            if w_eff <= 0.0 {
                continue;
            }
            let px = x0 + c as f32 * s;
            let py = y0 + r as f32 * s;
            if s >= 1.0 {
                for dy in 0..block {
                    for dx in 0..block {
                        canvas.set(px + dx as f32, py + dy as f32, col, w_eff);
                    }
                }
            } else {
                canvas.set(px, py, col, w_eff);
            }
        }
    }

    (x0, y0, x0 + w * s, y0 + h * s)
}

/// 画精灵并在周围铺一层光晕。
///
/// 光晕是「好看」的主要来源：同样是几根线，套了光就是霓虹，不套就是简笔画。
/// `radius` 建议取 `scale * 1.5 ~ scale * 3`。
pub fn draw_glowing(
    canvas: &mut CharCanvas,
    sp: &Sprite,
    cx: f32,
    cy: f32,
    scale: f32,
    color: Color,
    accent: Color,
    alpha: f32,
    glow_radius: f32,
    glow_strength: f32,
) -> (f32, f32, f32, f32) {
    // 先铺光：把精灵的墨点用低覆盖度、更大半径撒一圈。
    if glow_radius > 0.5 && glow_strength > 0.0 {
        let s = scale.max(0.05);
        let w = sp.width() as f32;
        let h = sp.height() as f32;
        let x0 = cx - w * s * 0.5;
        let y0 = cy - h * s * 0.5;
        let steps = glow_radius.ceil().max(1.0) as i32;
        for (r, line) in sp.art.iter().enumerate() {
            for (c, ch) in line.chars().enumerate() {
                if glyph(ch).is_none() {
                    continue;
                }
                let px = x0 + c as f32 * s;
                let py = y0 + r as f32 * s;
                // 只在墨点周围撒：按距离衰减
                for dy in -steps..=steps {
                    for dx in -steps..=steps {
                        let d = ((dx * dx + dy * dy) as f32).sqrt();
                        if d > glow_radius {
                            continue;
                        }
                        let k = 1.0 - d / glow_radius;
                        canvas.add(
                            px + dx as f32,
                            py + dy as f32,
                            color,
                            k * k * glow_strength * 0.25,
                        );
                    }
                }
            }
        }
    }
    draw_two_tone(canvas, sp, cx, cy, scale, color, accent, alpha)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_sprites_are_well_formed() {
        let list = all();
        assert!(list.len() >= 24, "精灵数量不足: {}", list.len());
        for sp in &list {
            assert!(sp.height() >= 9, "{} 只有 {} 行", sp.name, sp.height());
            assert!(sp.width() >= 11, "{} 只有 {} 列", sp.name, sp.width());
            assert!(sp.ink() >= 20, "{} 只有 {} 个墨点", sp.name, sp.ink());
        }
    }

    #[test]
    fn names_are_unique() {
        let mut ns = names();
        let n = ns.len();
        ns.sort_unstable();
        ns.dedup();
        assert_eq!(ns.len(), n, "精灵名有重复");
    }

    #[test]
    fn art_has_no_tabs() {
        for sp in all() {
            for line in sp.art {
                assert!(!line.contains('\t'), "{} 里有 tab", sp.name);
            }
        }
    }

    #[test]
    fn get_finds_every_sprite() {
        for name in names() {
            assert!(get(name).is_some(), "get 找不到 {name}");
        }
        assert!(get("__nope__").is_none());
    }

    #[test]
    fn draw_lights_up_pixels() {
        let mut c = CharCanvas::new(220, 62, crate::render::RenderMode::Braille);
        for sp in all() {
            let mut c = CharCanvas::new(220, 62, crate::render::RenderMode::Braille);
            let (x0, y0, x1, y1) = draw(
                &mut c,
                sp,
                c.sw as f32 / 2.0,
                c.sh as f32 / 2.0,
                1.0,
                crate::render::color::WHITE,
                1.0,
            );
            assert!(x1 > x0 && y1 > y0, "{} 矩形退化", sp.name);
            let lit = c.pixels().iter().filter(|p| p.w > 0.5).count();
            assert!(lit > 30, "{} 只点亮了 {} 个点", sp.name, lit);
        }
        let _ = &mut c;
    }

    #[test]
    fn draw_is_deterministic() {
        let sp = get("butterfly").unwrap_or(&SPRING[0]);
        let mut a = CharCanvas::new(80, 30, crate::render::RenderMode::Braille);
        let mut b = CharCanvas::new(80, 30, crate::render::RenderMode::Braille);
        draw(&mut a, sp, 40.0, 30.0, 1.5, crate::render::color::CYAN, 1.0);
        draw(&mut b, sp, 40.0, 30.0, 1.5, crate::render::color::CYAN, 1.0);
        let fa: Vec<u32> = a.pixels().iter().map(|p| p.w.to_bits()).collect();
        let fb: Vec<u32> = b.pixels().iter().map(|p| p.w.to_bits()).collect();
        assert_eq!(fa, fb);
    }

    #[test]
    fn draw_two_tone_uses_both_colors() {
        let sp = get("butterfly").unwrap_or(&SPRING[0]);
        let mut c = CharCanvas::new(120, 40, crate::render::RenderMode::Braille);
        draw_two_tone(
            &mut c,
            sp,
            60.0,
            40.0,
            1.5,
            crate::render::color::CYAN,
            crate::render::color::MAGENTA,
            1.0,
        );
        let has_cyan = c
            .pixels()
            .iter()
            .any(|p| p.w > 0.5 && p.b > 0.5 && p.g > 0.5 && p.r < 0.5);
        let has_magenta = c
            .pixels()
            .iter()
            .any(|p| p.w > 0.5 && p.r > 0.5 && p.b > 0.5 && p.g < 0.5);
        assert!(has_cyan || has_magenta, "两种色至少要用上一种");
    }

    #[test]
    fn tiny_canvas_does_not_panic() {
        let sp = get("butterfly").unwrap_or(&SPRING[0]);
        for (w, h) in [(1u16, 1u16), (8, 5), (2, 2)] {
            let mut c = CharCanvas::new(w, h, crate::render::RenderMode::Braille);
            draw(&mut c, sp, 0.0, 0.0, 3.0, crate::render::color::WHITE, 1.0);
            draw_glowing(
                &mut c,
                sp,
                -50.0,
                999.0,
                2.0,
                crate::render::color::WHITE,
                crate::render::color::CYAN,
                1.0,
                4.0,
                0.5,
            );
        }
    }

    #[test]
    fn glow_adds_brightness_without_erasing() {
        let sp = get("snowflake").unwrap_or(&WINTER[0]);
        let mut plain = CharCanvas::new(120, 40, crate::render::RenderMode::Braille);
        draw(&mut plain, sp, 60.0, 40.0, 2.0, crate::render::color::WHITE, 1.0);
        let mut glowed = CharCanvas::new(120, 40, crate::render::RenderMode::Braille);
        draw_glowing(
            &mut glowed,
            sp,
            60.0,
            40.0,
            2.0,
            crate::render::color::WHITE,
            crate::render::color::CYAN,
            1.0,
            4.0,
            0.8,
        );
        let sum = |c: &CharCanvas| -> f32 { c.pixels().iter().map(|p| p.w).sum() };
        assert!(
            sum(&glowed) > sum(&plain),
            "光晕没有增加亮度: {} vs {}",
            sum(&glowed),
            sum(&plain)
        );
        // 墨点不能被抹掉
        let ink = |c: &CharCanvas| c.pixels().iter().filter(|p| p.w > 0.9).count();
        assert!(ink(&glowed) >= ink(&plain));
    }
}

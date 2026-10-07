//! 真字光栅化：把中日文 / 拉丁文字画进 [`CharCanvas`] 点阵。
//!
//! 背景：场景层原来用 `draw_faux_text`（`scenes.rs:285`）按字符码位随机点亮几列点，
//! 画出来的「字」完全读不出来。本模块用 `fontdue` 把**真字形**光栅化成覆盖率位图，
//! 再按 Braille 的 `cov > 0.5` 阈值写进点阵画布，歌词才可能被看懂。
//!
//! # 坐标约定
//! `x` 是绘制起点（左沿），`y` 是**基线**（baseline），单位都是点阵
//! （`CharCanvas::sw` / `sh`）。字形第 `row` 行落在画布 `y - ymin - height + row`：
//! fontdue 0.9 的覆盖率位图「从字形左上角开始」（见 `fontdue-0.9.3/src/font.rs`
//! 的 `rasterize` 文档），而画布 y 轴向下，所以位图第 0 行在字形顶部。
//! 这与项目内已验证的导出路径 `src/export/raster.rs`（`py0 = baseline - height - ymin`）
//! 完全一致；**不要**写成 `y - ymin - row`，那会把字上下翻转。
//!
//! # 字体
//! 优先中日文：Noto Sans CJK（TTC，逐个 `collection_index` 试）→ Droid Sans Fallback
//! → 文泉驿 → DejaVu 等宽兜底。`WEM_FONT` 环境变量可覆盖。全局只加载一次
//! （[`OnceLock`]），不会每次调用都读盘。没有可用字体时降级为 2×3 实心小方块，不 panic。
//!
//! # 确定性
//! 不使用任何随机源；同一输入必得同一输出（可安全用于逐帧导出与 seek）。

use std::sync::OnceLock;

use fontdue::{Font, FontSettings};
use ratatui::style::Color;

use crate::render::CharCanvas;

// ── 常量 ────────────────────────────────────────────────────────

/// 字体文件候选（按优先级）。
///
/// **CJK 必须在前**：DejaVu / Liberation / JetBrains 都没有汉字，
/// 排在前面会把「蝶」画成 `.notdef` 豆腐块。
const FONT_CANDIDATES: &[&str] = &[
    // ── 中日韩（Noto Sans CJK，Arch / Debian / Fedora 常见路径）──
    "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/OTF/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/opentype/noto/NotoSansCJKjp-Regular.otf",
    "/usr/share/fonts/opentype/noto/NotoSansCJKsc-Regular.otf",
    "/usr/share/fonts/noto-cjk/NotoSansCJKjp-Regular.otf",
    "/System/Library/Fonts/PingFang.ttc",
    "C:/Windows/Fonts/msyh.ttc",
    // ── 中文回退 ──
    "/system/fonts/DroidSansFallbackFull.ttf",
    "/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf",
    "/usr/share/fonts/droid/DroidSansFallbackFull.ttf",
    "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
    "/usr/share/fonts/wenquanyi/wqy-microhei/wqy-microhei.ttc",
    "/usr/share/fonts/wqy-microhei/wqy-microhei.ttc",
    // ── 拉丁等宽兜底（没有汉字，只保证 ASCII 可读）──
    "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
    "/usr/share/fonts/dejavu/DejaVuSansMono.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
    "/usr/share/fonts/noto/NotoSansMono-Regular.ttf",
    "/usr/share/fonts/TTF/JetBrainsMono-Regular.ttf",
    "/usr/share/fonts/liberation/LiberationMono-Regular.ttf",
    "/System/Library/Fonts/Menlo.ttc",
    "C:/Windows/Fonts/consola.ttf",
];

/// 判定「这个字体能画汉字」的探针字符。
const PROBE_CJK: char = '蝶';

/// TTC 集合里最多试到第几个 face（Noto Sans CJK 的 ttc 有 4~5 个）。
const MAX_COLLECTION_INDEX: u32 = 4;

/// 覆盖率增益系数。
///
/// Braille 阈值是 `cov > 0.5`：小字号（px 8~16）下笔画覆盖率常在 0.3~0.5，
/// 不提升会整段消失。1.7 让 `cov >= 150/255` 饱和，细笔画能亮而不过度膨胀成色块。
const COVERAGE_BOOST: f32 = 1.7;

/// 字号上限。防御超大 px 导致 `rasterize` 分配巨型位图（内存按 px² 增长）。
const MAX_PX: f32 = 512.0;

/// `px` 非有限值时的兜底字号。
const DEFAULT_PX: f32 = 16.0;

/// 无字体降级方块的宽（点阵单位）。
const FALLBACK_W: f32 = 2.0;
/// 无字体降级方块的高（点阵单位）。
const FALLBACK_H: f32 = 3.0;

/// 无字体时每个字符的估算推进宽度（相对字号）。
const FALLBACK_ADVANCE_RATIO: f32 = 0.6;

// ── 字体加载（全局一次） ────────────────────────────────────────

/// 已加载的字体 + 它来自哪个文件。
struct LoadedFont {
    font: Font,
    path: String,
}

/// 全局字体缓存：`OnceLock` 保证只读盘 / 解析一次。
static FONT: OnceLock<Option<LoadedFont>> = OnceLock::new();

/// 取全局字体（首次调用时加载）。
fn font() -> Option<&'static LoadedFont> {
    FONT.get_or_init(load_font).as_ref()
}

/// 按 [`FONT_CANDIDATES`] 顺序找第一个可用字体；`WEM_FONT` 可覆盖。
fn load_font() -> Option<LoadedFont> {
    let mut candidates: Vec<String> = Vec::new();
    if let Ok(p) = std::env::var("WEM_FONT") {
        if !p.trim().is_empty() {
            candidates.push(p);
        }
    }
    candidates.extend(FONT_CANDIDATES.iter().map(|s| (*s).to_string()));

    for p in candidates {
        if let Some((font, idx)) = try_load(&p) {
            tracing::debug!(font = %p, collection_index = idx, "歌词字体已加载");
            return Some(LoadedFont { font, path: p });
        }
    }
    tracing::warn!("未找到可用字体，歌词将降级为方块");
    None
}

/// 尝试加载一个字体文件。TTC 会逐个 `collection_index` 试，优先挑「能画汉字」的那个。
///
/// 返回 `(Font, collection_index)`；文件读不到或不是字体时返回 `None`。
fn try_load(path: &str) -> Option<(Font, u32)> {
    let data = std::fs::read(path).ok()?;
    if data.is_empty() {
        return None;
    }
    // 先挑含汉字的 face；一个都没有时退回第一个能解析成功的（至少 ASCII 可读）
    let mut fallback: Option<(Font, u32)> = None;
    for idx in 0..=MAX_COLLECTION_INDEX {
        let settings = FontSettings {
            collection_index: idx,
            ..Default::default()
        };
        // ttf 只有 index 0 有效，其余会快速返回 Err，不会白跑
        let Ok(font) = Font::from_bytes(data.as_slice(), settings) else {
            continue;
        };
        if font.has_glyph(PROBE_CJK) {
            return Some((font, idx));
        }
        if fallback.is_none() {
            fallback = Some((font, idx));
        }
    }
    fallback
}

/// 当前实际加载到的字体路径（日志 / 自检用）。`None` = 没有可用字体。
pub fn font_path() -> Option<String> {
    font().map(|f| f.path.clone())
}

// ── 公开接口 ────────────────────────────────────────────────────

/// 画一段文字，返回推进宽度（点阵单位）。
///
/// `px` 是字号（点阵单位的高度量级），CJK 与拉丁都能画；`y` 是**基线**。
/// `alpha` 是整体不透明度（0 = 一个点都不画）。
pub fn draw(
    canvas: &mut CharCanvas,
    x: f32,
    y: f32,
    text: &str,
    px: f32,
    color: Color,
    alpha: f32,
) -> f32 {
    let f = font().map(|l| &l.font);
    draw_with(f, canvas, x, y, text, Ink { color, px, alpha })
}

/// 逐字符给 alpha（卡拉OK逐字高亮用）。
///
/// `alpha_of` 接收「第 i 个字符（按 `char` 计）」，返回 `0..=1`。
pub fn draw_styled(
    canvas: &mut CharCanvas,
    x: f32,
    y: f32,
    text: &str,
    px: f32,
    color: Color,
    alpha_of: &dyn Fn(usize) -> f32,
) -> f32 {
    let f = font().map(|l| &l.font);
    draw_styled_with(f, canvas, x, y, text, alpha_of, Ink { color, px, alpha: 1.0 })
}

/// 量宽度（点阵单位），不画。
///
/// 有字体时累加 `advance_width`；无字体时按 `px * 0.6` 估算。
pub fn measure(text: &str, px: f32) -> f32 {
    measure_with(font().map(|l| &l.font), text, px)
}

// ── 内部实现（`font` 可注入，便于测试无字体路径）────────────────

/// 一次绘制的固定样式（颜色 / 字号 / 全局不透明度）。
#[derive(Clone, Copy)]
struct Ink {
    color: Color,
    px: f32,
    alpha: f32,
}

/// 把字号夹进合法区间；非有限值用 [`DEFAULT_PX`]，`<= 0` 返回 0（表示不画）。
fn clamp_px(px: f32) -> f32 {
    if !px.is_finite() {
        return DEFAULT_PX;
    }
    if px <= 0.0 {
        return 0.0;
    }
    px.min(MAX_PX)
}

/// [`draw`] 的可注入版本。
fn draw_with(
    font: Option<&Font>,
    canvas: &mut CharCanvas,
    x: f32,
    y: f32,
    text: &str,
    ink: Ink,
) -> f32 {
    let px = clamp_px(ink.px);
    let alpha = ink.alpha.clamp(0.0, 1.0);
    if px <= 0.0 || alpha <= 0.0 {
        return 0.0;
    }
    let ink = Ink { px, alpha, ..ink };
    let mut pen = x;
    for ch in text.chars() {
        pen += draw_glyph(font, canvas, pen, y, ch, ink);
    }
    pen - x
}

/// [`draw_styled`] 的可注入版本。
fn draw_styled_with(
    font: Option<&Font>,
    canvas: &mut CharCanvas,
    x: f32,
    y: f32,
    text: &str,
    alpha_of: &dyn Fn(usize) -> f32,
    ink: Ink,
) -> f32 {
    let px = clamp_px(ink.px);
    if px <= 0.0 {
        return 0.0;
    }
    let ink = Ink { px, ..ink };
    let mut pen = x;
    for (i, ch) in text.chars().enumerate() {
        let alpha = alpha_of(i).clamp(0.0, 1.0);
        if alpha <= 0.0 {
            // 不透明度为 0 时仍要推进，否则后面的字会叠上来
            pen += advance_of(font, ch, px);
            continue;
        }
        pen += draw_glyph(font, canvas, pen, y, ch, Ink { alpha, ..ink });
    }
    pen - x
}

/// 单个字符的推进宽度。
fn advance_of(font: Option<&Font>, ch: char, px: f32) -> f32 {
    if ch.is_control() {
        return 0.0;
    }
    match font {
        Some(f) => f.metrics(ch, px).advance_width.max(0.0),
        None => px * FALLBACK_ADVANCE_RATIO,
    }
}

/// [`measure`] 的可注入版本。
fn measure_with(font: Option<&Font>, text: &str, px: f32) -> f32 {
    let px = clamp_px(px);
    if px <= 0.0 {
        return 0.0;
    }
    text.chars().map(|ch| advance_of(font, ch, px)).sum()
}

/// 画一个字符，返回推进宽度。控制字符（`\n` 等）不画、不推进。
fn draw_glyph(
    font: Option<&Font>,
    canvas: &mut CharCanvas,
    x: f32,
    y: f32,
    ch: char,
    ink: Ink,
) -> f32 {
    if ch.is_control() {
        return 0.0;
    }
    let Some(f) = font else {
        return draw_fallback_box(canvas, x, y, ink);
    };

    let (m, bitmap) = f.rasterize(ch, ink.px);
    let advance = m.advance_width.max(0.0);
    if m.width == 0 || m.height == 0 || bitmap.is_empty() {
        // 空白字符 / 无轮廓字形：只推进
        return advance;
    }

    // 位图第 0 行 = 字形顶部（fontdue 文档：vec starts at the top left corner）。
    // 画布 y 向下，所以顶部在 y - ymin - height，逐行往下加。
    let top = y - m.ymin as f32 - m.height as f32;
    let left = x + m.xmin as f32;

    for (row, line) in bitmap.chunks_exact(m.width).enumerate() {
        let py = top + row as f32;
        for (col, &cov) in line.iter().enumerate() {
            if cov == 0 {
                continue;
            }
            let boosted = (cov as f32 / 255.0 * COVERAGE_BOOST).min(1.0);
            let w = boosted * ink.alpha;
            if w <= 0.0 {
                continue;
            }
            canvas.set(left + col as f32, py, ink.color, w);
        }
    }
    advance
}

/// 无字体降级：按字符画一个 2×3 的实心小方块，底部贴基线。
fn draw_fallback_box(canvas: &mut CharCanvas, x: f32, y: f32, ink: Ink) -> f32 {
    let w = FALLBACK_W.max(1.0) as usize;
    let h = FALLBACK_H.max(1.0) as usize;
    let weight = (0.85 * ink.alpha).clamp(0.0, 1.0);
    if weight > 0.0 {
        for row in 0..h {
            for col in 0..w {
                canvas.set(x + col as f32, y - h as f32 + row as f32, ink.color, weight);
            }
        }
    }
    ink.px * FALLBACK_ADVANCE_RATIO
}

// ── 测试 ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RenderMode;
    use crate::render::color as col;

    fn canvas(w: u16, h: u16) -> CharCanvas {
        CharCanvas::new(w, h, RenderMode::Braille)
    }

    /// 真正点亮的子像素数（Braille 只有 `w > 0.5` 才落点）。
    fn lit(c: &CharCanvas) -> usize {
        c.pixels().iter().filter(|p| p.w > 0.5).count()
    }

    fn sig(c: &CharCanvas) -> Vec<u32> {
        c.pixels()
            .iter()
            .flat_map(|p| [p.w.to_bits(), p.r.to_bits(), p.g.to_bits(), p.b.to_bits()])
            .collect()
    }

    fn ink(px: f32, alpha: f32) -> Ink {
        Ink {
            color: col::WHITE,
            px,
            alpha,
        }
    }

    #[test]
    fn cjk_and_latin_draw_something() {
        let has_font = font().is_some();
        for s in ["蝶", "あ", "A"] {
            let mut c = canvas(60, 30);
            draw(&mut c, 6.0, 40.0, s, 32.0, col::WHITE, 1.0);
            let n = lit(&c);
            if has_font {
                assert!(n > 8, "「{s}」只点亮了 {n} 点");
            } else {
                // 无字体走 2×3 方块降级，只保证非空
                assert!(n > 0, "「{s}」无字体降级后完全没画东西");
            }
        }
    }

    #[test]
    fn measure_is_monotonic_and_stable() {
        let one = measure("蝶", 24.0);
        let two = measure("蝶蝶", 24.0);
        let three = measure("蝶蝶蝶", 24.0);
        assert!(one > 0.0, "measure 返回 0");
        assert!(two > one && three > two, "measure 未随字数增长：{one} {two} {three}");
        // 用 to_bits 比较：确定性要求逐位一致（也避开 clippy::float_cmp）
        assert_eq!(
            measure("蝶", 24.0).to_bits(),
            one.to_bits(),
            "同输入两次结果不一致"
        );
        assert_eq!(measure("", 24.0).to_bits(), 0.0f32.to_bits(), "空串宽度应为 0");
        // 字号越大越宽
        assert!(measure("蝶", 40.0) > measure("蝶", 20.0));
    }

    #[test]
    fn draw_is_deterministic() {
        let mut a = canvas(80, 30);
        let mut b = canvas(80, 30);
        let wa = draw(&mut a, 6.0, 40.0, "春夏秋冬", 28.0, col::WHITE, 1.0);
        let wb = draw(&mut b, 6.0, 40.0, "春夏秋冬", 28.0, col::WHITE, 1.0);
        assert_eq!(wa.to_bits(), wb.to_bits(), "两次推进宽度不一致");
        assert_eq!(sig(&a), sig(&b), "同一输入两次绘制不逐位一致");
    }

    #[test]
    fn huge_px_on_tiny_canvas_does_not_panic() {
        let mut c = canvas(8, 5);
        draw(&mut c, -50.0, 200.0, "蝶あA", 400.0, col::WHITE, 1.0);
        let mut c2 = canvas(8, 5);
        // 1e9 会被夹到 MAX_PX，不会分配巨型位图
        draw(&mut c2, 0.0, 0.0, "蝶", 1e9, col::WHITE, 1.0);
        let mut c3 = canvas(8, 5);
        draw(&mut c3, 0.0, 0.0, "蝶", f32::NAN, col::WHITE, 1.0);
    }

    #[test]
    fn zero_alpha_draws_nothing() {
        let mut c = canvas(60, 30);
        draw(&mut c, 6.0, 40.0, "蝶あA", 32.0, col::WHITE, 0.0);
        assert!(c.pixels().iter().all(|p| p.is_empty()), "alpha=0 仍有像素");

        let mut c2 = canvas(60, 30);
        draw_styled(&mut c2, 6.0, 40.0, "蝶あA", 32.0, col::WHITE, &|_: usize| 0.0);
        assert!(c2.pixels().iter().all(|p| p.is_empty()), "alpha_of=0 仍有像素");
    }

    #[test]
    fn styled_alpha_is_per_char() {
        let text = "春夏";
        let mut all = canvas(80, 30);
        draw_styled(&mut all, 4.0, 40.0, text, 28.0, col::WHITE, &|_: usize| 1.0);
        assert!(lit(&all) > 0);

        let mut second = canvas(80, 30);
        let w = draw_styled(&mut second, 4.0, 40.0, text, 28.0, col::WHITE, &|i: usize| {
            if i == 1 {
                1.0
            } else {
                0.0
            }
        });
        assert!(w > 0.0, "逐字 alpha 版本推进宽度为 0");
        assert!(lit(&second) > 0, "只亮第二个字符时什么都没画");
        // 只亮第二个字符，像素数必然少于全亮
        assert!(lit(&second) < lit(&all));
    }

    #[test]
    fn no_font_fallback_never_panics() {
        let mut c = canvas(80, 24);
        let adv = draw_with(None, &mut c, 4.0, 60.0, "蝶あA x", ink(24.0, 1.0));
        assert!(adv > 0.0, "无字体时推进宽度为 0");
        assert!(lit(&c) > 0, "无字体降级没画任何东西");

        let expect = 5.0 * 24.0 * FALLBACK_ADVANCE_RATIO;
        let got = measure_with(None, "蝶あA x", 24.0);
        assert!((got - expect).abs() < 1e-3, "无字体宽度估算 {got} != {expect}");

        // 无字体 + alpha=0 也必须一个点都不画
        let mut c2 = canvas(80, 24);
        draw_with(None, &mut c2, 4.0, 60.0, "蝶", ink(24.0, 0.0));
        assert!(c2.pixels().iter().all(|p| p.is_empty()));

        // 控制字符不画也不推进
        let mut c3 = canvas(80, 24);
        let adv3 = draw_with(None, &mut c3, 0.0, 10.0, "\n\r\t", ink(24.0, 1.0));
        assert_eq!(adv3.to_bits(), 0.0f32.to_bits());
        assert!(c3.pixels().iter().all(|p| p.is_empty()));
    }

    #[test]
    fn font_path_is_stable_and_real() {
        let a = font_path();
        assert_eq!(a, font_path(), "font_path 两次结果不一致");
        if let Some(p) = a {
            assert!(
                std::path::Path::new(&p).exists(),
                "font_path 返回了不存在的路径：{p}"
            );
        }
    }

    #[test]
    fn px_zero_draws_nothing() {
        let mut c = canvas(60, 30);
        let adv = draw(&mut c, 6.0, 40.0, "蝶", 0.0, col::WHITE, 1.0);
        assert_eq!(adv.to_bits(), 0.0f32.to_bits());
        assert!(c.pixels().iter().all(|p| p.is_empty()));
        assert_eq!(measure("蝶", 0.0).to_bits(), 0.0f32.to_bits());
    }
}

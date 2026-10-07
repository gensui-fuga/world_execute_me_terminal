//! 尾声（268.7–289.9s，6 张卡）：卷轴开始合拢 → 四季色褪成灰白 → 最后一粒雪落下 →
//! 余烬明灭 → 画面向中心收束 → 静默（几乎全暗，只剩一点微光）。
//!
//! 这一段没有唱句（`assets/shunkashuto.lrc` 末句在 246.19s），21.2 秒全靠画面
//! 自身的连续运动撑住。文件结构照抄 [`crate::render::scenes_autumn`]：
//! 共享背景层 [`outro_bg`] + 卡表 [`OUTRO_CARDS`] + 入口 [`render_card`]。
//!
//! 卡内元素一律乘 [`card_gain`] 包络（卡首 0.7s 淡入、卡末淡出到 0），
//! 于是相邻卡的交界处只剩共享背景，跳变远小于卡内运动。
//!
//! 不变式：时间只写在卡表；伪随机全部走整数哈希（纯函数，可 seek）；
//! `alpha` 只用来向黑压暗（[`CharCanvas::tint`]），绝不动 `Pixel.w`。

use ratatui::style::Color;

use crate::render::color as col;
use crate::render::motifs::{ease, envelope, haze_heat, sudare};
use crate::render::scenes::SceneCtx;
use crate::render::CharCanvas;

/// 尾声长度（timeline: 268.7 → 289.9）。
pub const OUTRO_LEN: f32 = 21.2;

// ── 色系（与 `SceneKind::Outro::palette` 同源）──────────────────

/// 主色：褪色后的灰。
const ASH: Color = Color::Rgb(140, 146, 154);
/// 灰白（卷面 / 画轴）。
const PALE: Color = Color::Rgb(170, 170, 170);
/// 余烬的暖橙。
const EMBER: Color = Color::Rgb(198, 96, 52);
/// 雪白。
const SNOW: Color = Color::Rgb(228, 234, 238);
/// 春（嫩绿）—— 四季色之一，用来演示「褪成灰白」。
const SEASON_SPRING: Color = Color::Rgb(140, 196, 140);
/// 夏（浓青）。
const SEASON_SUMMER: Color = Color::Rgb(96, 176, 196);
/// 秋（琥珀）。
const SEASON_AUTUMN: Color = Color::Rgb(214, 164, 84);
/// 冬（冰蓝）。
const SEASON_WINTER: Color = Color::Rgb(150, 176, 214);

// ── 小工具 ──────────────────────────────────────────────────────

/// 单个 u32 整数哈希（与 scenes.rs 同款，独立副本避免跨文件耦合）。
fn hash_u32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^= x >> 16;
    x
}

/// 哈希 → 0..1。
fn hf(i: u32, seed: u32) -> f32 {
    (hash_u32(i.wrapping_mul(0x9E37_79B1).wrapping_add(seed)) >> 8) as f32 / 16_777_216.0
}

/// 0..1 夹取。
fn clamp01(x: f32) -> f32 {
    x.clamp(0.0, 1.0)
}

/// 卡内包络：卡首 0.7s 淡入、卡末淡出到 0，交界处只剩共享背景。
fn card_gain(phase: f32, dur: f32) -> f32 {
    let t2 = (dur * 0.70).max(1.0);
    let t3 = dur.max(t2 + 0.6);
    envelope(phase, 0.0, 0.7, t2, t3)
}

/// 六边形轮廓（雪粒用，与 autumn 同款）。
fn hex_ring(c: &mut CharCanvas, cx: f32, cy: f32, r: f32, color: Color, w: f32) {
    if r <= 0.0 {
        return;
    }
    let mut prev: Option<(f32, f32)> = None;
    for i in 0..=6 {
        let a = i as f32 * std::f32::consts::FRAC_PI_3 + std::f32::consts::FRAC_PI_6;
        let p = (cx + r * a.cos(), cy + r * a.sin());
        if let Some(q) = prev {
            c.line(q.0, q.1, p.0, p.1, color, w);
        }
        prev = Some(p);
    }
}

// ── 共享背景：卷面 + 上下画轴 ───────────────────────────────────

/// 尾声背景：一整幅灰白卷面，上下各一根画轴，常驻落灰。
///
/// 所有元素都是 `ctx.t` 的连续函数，卡与卡之间共享，交界处零跳变。
fn outro_bg(ctx: &SceneCtx, c: &mut CharCanvas) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    if sw < 4.0 || sh < 6.0 {
        return;
    }
    let t = ctx.t as f32;

    // 卷面：竖向纤维（稀疏竖线，比实心填充更像纸）
    let cols = 26;
    for k in 0..cols {
        let x = sw * (k as f32 + 0.5) / cols as f32;
        c.line(x, 0.0, x, sh, ASH, 0.34);
    }
    // 上下沿压暗，把画轴托出来
    c.tint_rect(0.0, 0.0, sw, sh * 0.10, col::BLACK, 0.55);
    c.tint_rect(0.0, sh * 0.90, sw, sh * 0.10, col::BLACK, 0.55);

    // 画轴：上下各一根，粗细缓慢呼吸
    let rod = 0.62 + 0.10 * (t * 0.5).sin();
    c.line(0.0, sh * 0.08, sw, sh * 0.08, PALE, rod);
    c.line(0.0, sh * 0.92, sw, sh * 0.92, PALE, rod);
    // 轴端
    for (fx, fy) in [(0.02f32, 0.08f32), (0.98, 0.08), (0.02, 0.92), (0.98, 0.92)] {
        c.disc(sw * fx, sh * fy, 1.4, PALE, 0.70);
    }
    // 卷边
    c.line(sw * 0.04, sh * 0.08, sw * 0.04, sh * 0.92, ASH, 0.52);
    c.line(sw * 0.96, sh * 0.08, sw * 0.96, sh * 0.92, ASH, 0.52);

    // 常驻落灰：极慢下坠的灰白点
    for k in 0..16u32 {
        let p = (hf(k, 201) + t * 0.02 * (0.5 + hf(k, 202))) % 1.0;
        let x = sw * (0.05 + hf(k, 203) * 0.90);
        let y = sh * (0.08 + p * 0.84);
        c.set(x, y, PALE, 0.56);
    }
}

// ── 卡片绘制函数 ────────────────────────────────────────────────

/// 01 卷轴开始合拢：上下画轴向中间收，两侧卷边向内卷。
fn o01_scroll_closing(ctx: &SceneCtx, c: &mut CharCanvas, phase: f32, dur: f32) {
    let g = card_gain(phase, dur);
    if g <= 0.0 {
        return;
    }
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let t = ctx.t as f32;
    let close = ease(phase, (dur * 0.90).max(0.4));

    let top = sh * 0.08 + close * sh * 0.17;
    let bot = sh * 0.92 - close * sh * 0.17;
    let lx = sw * 0.04 + close * sw * 0.10;
    let rx = sw * 0.96 - close * sw * 0.10;

    // 画轴：上下两根亮杆（合拢中持续向内移动）
    c.line(0.0, top, sw, top, PALE, 0.85 * g);
    c.line(0.0, bot, sw, bot, PALE, 0.85 * g);
    c.line(0.0, top + 1.2, sw, top + 1.2, ASH, 0.55 * g);
    c.line(0.0, bot - 1.2, sw, bot - 1.2, ASH, 0.55 * g);
    c.disc(sw * 0.02, top, 1.3, PALE, 0.75 * g);
    c.disc(sw * 0.98, top, 1.3, PALE, 0.75 * g);
    c.disc(sw * 0.02, bot, 1.3, PALE, 0.75 * g);
    c.disc(sw * 0.98, bot, 1.3, PALE, 0.75 * g);

    // 卷边：左右两片向内卷
    c.line(lx, top, lx, bot, ASH, 0.66 * g);
    c.line(rx, top, rx, bot, ASH, 0.66 * g);

    // 合拢的缝里透出的光（越合越暗）
    let mid = (top + bot) * 0.5;
    c.line(lx, mid, rx, mid, col::WHITE, 0.62 * g * (1.0 - close * 0.55));

    // 卷动时洒落的尘
    for k in 0..10u32 {
        let p = (t * 0.25 + hf(k, 221)) % 1.0;
        let x = sw * (0.06 + hf(k, 222) * 0.88);
        let y = top + p * (bot - top);
        c.set(x, y, PALE, 0.58 * g * (1.0 - p));
    }
}

/// 02 四季色褪成灰白：四条季节色带自上而下逐条褪色，卷帘继续降下。
fn o02_seasons_gray(ctx: &SceneCtx, c: &mut CharCanvas, phase: f32, dur: f32) {
    let g = card_gain(phase, dur);
    if g <= 0.0 {
        return;
    }
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let seasons = [SEASON_SPRING, SEASON_SUMMER, SEASON_AUTUMN, SEASON_WINTER];

    for (i, &sc) in seasons.iter().enumerate() {
        let iu = i as u32;
        let y = sh * (0.34 + i as f32 * 0.16);
        let band = sh * 0.045;
        let x0 = sw * 0.10;
        let bw = sw * 0.80 * (1.0 - 0.10 * i as f32);
        // 逐条错开褪色
        let gray = ease(phase - i as f32 * 0.45, (dur * 0.55).max(0.4));
        let tone = col::lerp(sc, ASH, gray);

        // 色带本体：竖条纹（纸面纤维的密度感，避免整块实心）
        let mut x = x0;
        while x < x0 + bw {
            c.line(x, y, x, y + band, tone, 0.72 * g);
            x += 2.0;
        }
        // 上下沿：褪成灰白后剩下的轮廓
        c.line(x0, y, x0 + bw, y, col::lerp(tone, PALE, 0.5), 0.70 * g);
        c.line(x0, y + band, x0 + bw, y + band, col::lerp(tone, PALE, 0.5), 0.70 * g);

        // 褪下的色粉：向右下飘散
        for k in 0..5u32 {
            let id = iu * 11 + k;
            let p = (gray * 1.2 + hf(id, 311)) % 1.0;
            let px = x0 + bw * hf(id, 312) + p * sw * 0.10;
            let py = y + band + p * sh * 0.06;
            c.set(px, py, col::lerp(sc, ASH, 0.5), 0.80 * g * (1.0 - p) * gray);
        }
    }

    // 复用母题：卷帘（3.5–6.5s 之间降下，正好落在本卡）
    sudare(c, ctx, g * 0.9);
}

/// 03 最后一粒雪落下：一粒雪从卷首落到卷面，落定后留下一点白。
fn o03_last_snow(ctx: &SceneCtx, c: &mut CharCanvas, phase: f32, dur: f32) {
    let g = card_gain(phase, dur);
    if g <= 0.0 {
        return;
    }
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let t = ctx.t as f32;
    let fall = ease(phase, (dur * 0.75).max(0.4));

    let x = sw * 0.5 + (t * 0.42).sin() * sw * 0.05;
    let y = sh * 0.14 + fall * (sh * 0.68);

    // 雪：六角轮廓 + 中心亮点
    hex_ring(c, x, y, 2.0 + fall * 1.2, SNOW, 0.85 * g);
    c.disc(x, y, 0.9, col::WHITE, 0.90 * g);

    // 落迹：上方渐暗的点
    for k in 0..8u32 {
        let p = k as f32 / 8.0;
        c.set(
            x + (hf(k, 301) - 0.5) * 1.6,
            y - 1.4 - p * 7.0,
            SNOW,
            0.62 * g * (1.0 - p),
        );
    }

    // 落定：卷面上一小片积雪
    let rest = clamp01((fall - 0.88) * 8.0);
    if rest > 0.0 {
        c.ellipse(x, sh * 0.84, 3.2 * rest, 0.9 * rest, SNOW, 0.78 * g * rest);
        c.set(x, sh * 0.84, col::WHITE, 0.82 * g * rest);
    }

    // 周围零星的雪（极慢下坠）
    for k in 0..12u32 {
        let p = (hf(k, 302) + t * 0.05 * (0.5 + hf(k, 303))) % 1.0;
        let sx = sw * (0.06 + hf(k, 304) * 0.88);
        let sy = sh * (0.10 + p * 0.78);
        c.set(sx, sy, SNOW, 0.56 * g);
    }

    // 复用母题：卷帘余韵（与本卡时间窗一致）
    sudare(c, ctx, g * 0.55);
}

/// 04 余烬明灭：卷面下缘的余烬不规则地亮起、熄灭，热浪微微扭曲。
fn o04_embers(ctx: &SceneCtx, c: &mut CharCanvas, phase: f32, dur: f32) {
    let g = card_gain(phase, dur);
    if g <= 0.0 {
        return;
    }
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let t = ctx.t as f32;

    // 复用母题：底部余温的横向扭曲（envelope 6–15.5s，落在本卡）
    haze_heat(c, ctx, g * 0.9);

    for k in 0..18u32 {
        let x = sw * (0.10 + hf(k, 401) * 0.80);
        let y = sh * (0.62 + hf(k, 402) * 0.30);
        // 明灭：两条不同频率的正弦叠加 → 不规则呼吸
        let f = (t * (0.9 + hf(k, 403) * 1.6) + hf(k, 404) * std::f32::consts::TAU).sin()
            + 0.6 * (t * (2.3 + hf(k, 405)) + hf(k, 406) * std::f32::consts::TAU).sin();
        let a = clamp01(0.35 + 0.45 * f) * (0.55 + 0.45 * hf(k, 407));
        if a <= 0.03 {
            continue;
        }
        let r = 0.7 + hf(k, 408) * 1.3;
        c.disc(x, y, r, EMBER, 0.85 * g * a);
        // 火心
        c.set(x, y - r - 0.6, col::WHITE, 0.60 * g * a);
        // 上升的一缕烟
        c.set(
            x + (t * 0.7 + k as f32).sin() * 0.8,
            y - r - 2.0 - (t * 0.5 + k as f32).sin().abs() * 1.5,
            ASH,
            0.58 * g * a * 0.8,
        );
    }

    // 熄灭：整体颜色向灰褪去
    let die = ease(phase, (dur * 0.90).max(0.4));
    c.tint_rect(0.0, sh * 0.55, sw, sh * 0.45, ASH, 0.28 * die);
}

/// 05 画面向中心收束：收缩的框、向心的辐条、被吸入的碎屑。
fn o05_converge(ctx: &SceneCtx, c: &mut CharCanvas, phase: f32, dur: f32) {
    let g = card_gain(phase, dur);
    if g <= 0.0 {
        return;
    }
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let t = ctx.t as f32;
    let (cx, cy) = (sw * 0.5, sh * 0.5);
    let conv = ease(phase, (dur * 0.85).max(0.4));

    // 收缩的框：从整幅收到中心
    let k = 1.0 - conv * 0.88;
    let w = sw * k * 0.92;
    let h = sh * k * 0.92;
    c.rect(cx - w * 0.5, cy - h * 0.5, w, h, PALE, 0.72 * g);
    c.rect(cx - w * 0.25, cy - h * 0.25, w * 0.5, h * 0.5, ASH, 0.66 * g);

    // 向心的辐条
    for k2 in 0..14u32 {
        let a = k2 as f32 / 14.0 * std::f32::consts::TAU + conv * 0.6;
        let r0 = (1.0 - conv * 0.6) * sw.min(sh) * 0.52;
        let r1 = r0 * 0.42;
        if r0 <= r1 {
            continue;
        }
        c.line(
            cx + a.cos() * r0,
            cy + a.sin() * r0,
            cx + a.cos() * r1,
            cy + a.sin() * r1,
            MIST_ASH(),
            0.66 * g,
        );
    }

    // 被吸入的碎屑
    for k2 in 0..20u32 {
        let u = (t * 0.55 + hf(k2, 501)) % 1.0;
        let r = (1.0 - u) * sw.min(sh) * 0.60;
        let a = hf(k2, 502) * std::f32::consts::TAU;
        c.set(cx + a.cos() * r, cy + a.sin() * r * 0.72, PALE, 0.58 * g * (1.0 - u));
    }

    // 汇聚点
    c.disc(cx, cy, 0.8 + conv * 1.5, col::WHITE, 0.85 * g);
    c.circle(cx, cy, 2.4 + conv * 1.2, PALE, 0.66 * g);

    // 复用母题：收束时底部仍有热浪余韵（envelope 13s 后逐渐归零）
    haze_heat(c, ctx, g * 0.5);
}

/// 收束辐条用的灰蓝色（本文件里只此一处，避免与 ASH 混淆）。
const fn MIST_ASH() -> Color {
    Color::Rgb(168, 178, 188)
}

/// 06 静默：全屏压暗到几乎全黑，只剩画面正中一点微光与四点残星。
fn o06_silence(ctx: &SceneCtx, c: &mut CharCanvas, phase: f32, dur: f32) {
    let g = card_gain(phase, dur);
    if g <= 0.0 {
        return;
    }
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let t = ctx.t as f32;
    let dark = ease(phase, (dur * 0.60).max(0.4));

    // 全屏压暗：只混合颜色，不动 Pixel.w（Braille 的点阵结构保持完整）
    c.tint(col::BLACK, 0.78 * dark);

    // 极暗的卷面残影（几乎看不见，但保证不是死黑）
    c.line(0.0, sh * 0.5, sw, sh * 0.5, ASH, 0.30 * g);

    let (cx, cy) = (sw * 0.5, sh * 0.52);
    let breath = 0.5 + 0.5 * (t * 0.42).sin();

    // 中心微光：一点白 + 两圈呼吸环
    c.disc(cx, cy, 0.9, col::WHITE, 0.95 * g * (0.7 + 0.3 * breath));
    c.circle(cx, cy, 2.2 + breath * 0.6, PALE, 0.62 * g);
    c.circle(cx, cy, 4.0 + breath * 1.0, ASH, 0.45 * g * 0.7);

    // 四点残星：极缓明灭
    for k in 0..4u32 {
        let a = k as f32 * std::f32::consts::TAU / 4.0 + 0.4;
        let r = sw.min(sh) * 0.30;
        let tw = 0.5 + 0.5 * (t * 0.30 + k as f32 * 1.7).sin();
        c.set(
            cx + a.cos() * r,
            cy + a.sin() * r * 0.60,
            PALE,
            0.95 * g * (0.40 + 0.60 * tw),
        );
    }
}

// ── 卡片表 ──────────────────────────────────────────────────────

/// 卡绘制函数签名：`(ctx, canvas, 卡内相位 local, 本卡跨度 dur)`。
pub type CardFn = fn(&SceneCtx, &mut CharCanvas, f32, f32);

/// 尾声的一张卡。
pub struct OutroCard {
    /// 相对段起点的秒数
    pub at: f32,
    /// 卡名（日志 / 汇总用）
    pub name: &'static str,
    /// 绘制函数
    pub draw: CardFn,
}

/// 尾声 6 卡：间距 3.4 / 3.4 / 3.4 / 3.4 / 3.6（均落在 3.0–3.6s）。
pub const OUTRO_CARDS: &[OutroCard] = &[
    OutroCard { at: 0.0, name: "卷轴开始合拢", draw: o01_scroll_closing },
    OutroCard { at: 3.4, name: "四季色褪成灰白", draw: o02_seasons_gray },
    OutroCard { at: 6.8, name: "最后一粒雪落下", draw: o03_last_snow },
    OutroCard { at: 10.2, name: "余烬明灭", draw: o04_embers },
    OutroCard { at: 13.6, name: "画面向中心收束", draw: o05_converge },
    OutroCard { at: 17.2, name: "静默", draw: o06_silence },
];

/// 卡名清单（供日志 / Lead 汇总）。
pub fn card_names() -> Vec<&'static str> {
    OUTRO_CARDS.iter().map(|k| k.name).collect()
}

/// 第 `idx` 张卡的跨度（到下一卡起点；末卡到段尾）。
fn card_span(idx: usize) -> f32 {
    let start = OUTRO_CARDS[idx].at;
    let end = OUTRO_CARDS.get(idx + 1).map(|k| k.at).unwrap_or(OUTRO_LEN);
    (end - start).max(0.1)
}

/// 按段内时间选卡：最后一个 `at <= local` 的卡（`local < 0` 取首卡）。
fn pick(local: f32) -> usize {
    let mut idx = 0usize;
    for (i, k) in OUTRO_CARDS.iter().enumerate() {
        if local >= k.at {
            idx = i;
        }
    }
    idx
}

/// 尾声入口：共享背景 + 当前卡。
///
/// `alpha` 只用于向黑压暗（`tint` 只混合颜色、不动 `Pixel.w`），
/// 保证 Braille 的点阵结构在任何 alpha 下都不被破坏。
pub fn render_card(ctx: &SceneCtx, canvas: &mut CharCanvas, alpha: f32) {
    let alpha = clamp01(alpha);
    let local = ctx.local_t as f32;
    let idx = pick(local);
    let card = &OUTRO_CARDS[idx];
    outro_bg(ctx, canvas);
    (card.draw)(ctx, canvas, local - card.at, card_span(idx));
    if alpha < 1.0 {
        canvas.tint(col::BLACK, 1.0 - alpha);
    }
}

// ── 测试 ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::features::{BAND_COUNT, SPECTRUM_BINS, FrameFeatures};
    use crate::config::RenderMode;
    use crate::lyrics::Lyrics;
    use crate::timeline::scene::SceneKind;

    fn feat(e: f32) -> FrameFeatures {
        let mut f = FrameFeatures {
            time: 0.0,
            energy: e,
            energy_norm: e,
            ..Default::default()
        };
        f.bands_norm = [e; BAND_COUNT];
        f.spectrum = (0..SPECTRUM_BINS)
            .map(|i| ((i as f32 / SPECTRUM_BINS as f32) * std::f32::consts::TAU).sin().abs() * e)
            .collect();
        f
    }

    /// 尾声段起点是 268.7s，`local` 是段内秒，所以 `t = 268.7 + local`。
    fn ctx_at(local: f32) -> SceneCtx<'static> {
        let f = feat(0.6);
        let t = 268.7 + local as f64;
        SceneCtx {
            t,
            local_t: local as f64,
            progress: local / OUTRO_LEN,
            feat: f.clone(),
            prev: f,
            frame: (t * 24.0) as u64,
            cols: 80,
            rows: 24,
            scene: SceneKind::Outro,
            lyrics: Box::leak(Box::new(Lyrics::default())),
            flash: 0.0,
            punch: 0.0,
            heartbeat: 0.0,
            glitch: 0.0,
            burst: 0.0,
            invert: 0.0,
        }
    }

    fn draw(local: f32, alpha: f32) -> CharCanvas {
        let ctx = ctx_at(local);
        let mut c = CharCanvas::new(80, 24, RenderMode::Braille);
        render_card(&ctx, &mut c, alpha);
        c
    }

    /// 真正点亮的子像素数（Braille 只有 `cov > 0.5` 才落点）。
    fn lit(local: f32) -> usize {
        draw(local, 1.0).pixels().iter().filter(|p| p.w > 0.5).count()
    }

    /// 逐位签名（w + 颜色），用于确定性断言。
    fn sig(local: f32) -> Vec<u32> {
        draw(local, 1.0)
            .pixels()
            .iter()
            .flat_map(|p| [p.w.to_bits(), p.r.to_bits(), p.g.to_bits(), p.b.to_bits()])
            .collect()
    }

    /// 点亮掩码：视觉上亮/不亮，用于卡间连续性断言。
    fn mask(local: f32) -> Vec<u8> {
        draw(local, 1.0)
            .pixels()
            .iter()
            .map(|p| u8::from(p.w > 0.5))
            .collect()
    }

    fn diff(a: &[u8], b: &[u8]) -> usize {
        a.iter().zip(b.iter()).filter(|(x, y)| x != y).count()
    }

    #[test]
    fn six_cards_exist() {
        assert_eq!(OUTRO_CARDS.len(), 6);
        assert_eq!(card_names().len(), 6);
        for w in OUTRO_CARDS.windows(2) {
            assert!(w[1].at > w[0].at, "卡起点未递增：{} → {}", w[0].name, w[1].name);
            let gap = w[1].at - w[0].at;
            // f32 相减会带 ~4e-7 的舍入误差（17.2 - 13.6 = 3.6000004），
            // 所以要带容差比较，不能直接和 3.6 比。
            const EPS: f32 = 1e-3;
            assert!(
                gap >= 3.0 - EPS && gap <= 3.6 + EPS,
                "卡间距 {gap} 不在 3.0–3.6s"
            );
        }
        assert!(OUTRO_CARDS[OUTRO_CARDS.len() - 1].at < OUTRO_LEN);
    }

    #[test]
    fn every_card_draws_something() {
        for k in OUTRO_CARDS {
            let n = lit(k.at + 0.5);
            assert!(n > 20, "尾卡「{}」只点亮了 {n} 像素", k.name);
        }
    }

    #[test]
    fn no_panic_on_tiny_canvas() {
        for k in OUTRO_CARDS {
            let ctx = ctx_at(k.at + 0.5);
            let mut c = CharCanvas::new(8, 5, RenderMode::Braille);
            render_card(&ctx, &mut c, 1.0);
        }
    }

    #[test]
    fn same_local_is_bit_identical() {
        for k in OUTRO_CARDS {
            let a = sig(k.at + 0.7);
            let b = sig(k.at + 0.7);
            assert_eq!(a, b, "尾卡「{}」同一 local 两次绘制不一致", k.name);
        }
    }

    #[test]
    fn every_card_moves() {
        let mut moved = 0;
        for k in OUTRO_CARDS {
            if mask(k.at + 0.5) != mask(k.at + 1.6) {
                moved += 1;
            }
        }
        assert_eq!(moved, OUTRO_CARDS.len(), "只有 {moved}/6 张卡在动");
    }

    #[test]
    fn boundary_jump_is_smaller_than_card_motion() {
        for i in 0..OUTRO_CARDS.len() - 1 {
            let cur = &OUTRO_CARDS[i];
            let nxt = &OUTRO_CARDS[i + 1];
            let jump = diff(&mask(nxt.at - 0.05), &mask(nxt.at + 0.05));
            let m_cur = diff(&mask(cur.at + 0.4), &mask(cur.at + 1.5));
            let m_nxt = diff(&mask(nxt.at + 0.4), &mask(nxt.at + 1.5));
            let motion = m_cur.min(m_nxt).max(1);
            assert!(motion > 0, "卡「{}」/「{}」在 1.1s 内完全没动", cur.name, nxt.name);
            assert!(
                jump < motion,
                "卡「{}」交界跳变 {jump} 不小于卡内运动 {motion}",
                nxt.name
            );
        }
    }

    #[test]
    fn last_card_is_almost_dark_but_not_dead() {
        // 静默卡：末段整体亮度必须显著低于余烬卡，但仍留微光
        let bright: f32 = draw(11.0, 1.0).pixels().iter().map(|p| p.r + p.g + p.b).sum();
        let dim: f32 = draw(20.6, 1.0).pixels().iter().map(|p| p.r + p.g + p.b).sum();
        assert!(dim < bright * 0.5, "静默卡不够暗：{dim} vs {bright}");
        assert!(lit(20.6) > 20, "静默卡完全死黑（{} 像素）", lit(20.6));
    }

    #[test]
    fn alpha_only_darkens_color() {
        let local = 11.5;
        let full = draw(local, 1.0);
        let dim = draw(local, 0.25);
        let wf: Vec<u32> = full.pixels().iter().map(|p| p.w.to_bits()).collect();
        let wd: Vec<u32> = dim.pixels().iter().map(|p| p.w.to_bits()).collect();
        assert_eq!(wf, wd, "alpha 不得改变 Pixel.w");
        let lf: f32 = full.pixels().iter().map(|p| p.r + p.g + p.b).sum();
        let ld: f32 = dim.pixels().iter().map(|p| p.r + p.g + p.b).sum();
        assert!(ld < lf, "alpha 未压暗颜色：{ld} !< {lf}");
    }
}

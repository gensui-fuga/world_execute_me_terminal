//! 昇華（175.55–224s）＋ 終章（224.9–268s）：绘卷独楽，记忆不锈。
//!
//! 昇華把四季织成一卷无声绘卷：雪道埋脚印、卷轴横流、四季色环如独楽
//! 旋转、毛笔落下写下「刹那」。終章花散月缺，全屏暗淡只剩一句词发亮，
//! 在「錆ビツキハシナイ！」处全画面金脉冲上扬，最后以褪色的春景重现收束。
//! 两个入口 [`render_ascension`] / [`render_finale`]，各 10 卡，共享卷轴底。

use ratatui::style::Color;

use crate::render::color as col;
use crate::render::CharCanvas;
use crate::render::scenes::SceneCtx;

/// 昇華／終章色系。
const GOLD: Color = Color::Rgb(255, 214, 130);
const GOLD_DIM: Color = Color::Rgb(210, 170, 90);
const PAPER: Color = Color::Rgb(236, 226, 200); // 绘卷纸底
const INK: Color = Color::Rgb(40, 36, 30); // 墨
const VERDURIS: Color = Color::Rgb(235, 84, 66);
const FADE_GREEN: Color = Color::Rgb(168, 196, 158); // 褪色春绿
const MEM_BLUE: Color = Color::Rgb(120, 150, 190);

/// 四季色环（昇華独楽、卷轴取色共用）。
const FOUR_SEASONS: [Color; 4] = [
    Color::Rgb(140, 214, 120), // 春
    Color::Rgb(70, 196, 210),  // 夏
    Color::Rgb(240, 180, 88),  // 秋
    Color::Rgb(196, 218, 240), // 冬
];

// ── 工具 ────────────────────────────────────────────────

fn hash_u32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^= x >> 16;
    x
}

fn hf(i: u32, seed: u32) -> f32 {
    (hash_u32(i.wrapping_mul(0x9E37_79B1).wrapping_add(seed)) >> 8) as f32 / 16_777_216.0
}

/// 昇華下的 0..1 归一（消除 f32 歧义）。
fn unit(x: f32) -> f32 {
    x.clamp(0.0, 1.0)
}

fn fade(x: f32) -> f32 {
    let v = x.clamp(0.0, 1.0);
    v * v * (3.0 - 2.0 * v)
}

/// 把季号映射为颜色（支持小数插值）。
fn season_color(u: f32) -> Color {
    let u = u.rem_euclid(4.0);
    let i = u.floor() as usize % 4;
    let j = (i + 1) % 4;
    let f = u - i as f32;
    let (r0, g0, b0) = col::rgb_of(FOUR_SEASONS[i]);
    let (r1, g1, b1) = col::rgb_of(FOUR_SEASONS[j]);
    Color::Rgb(
        (r0 as f32 * (1.0 - f) + r1 as f32 * f) as u8,
        (g0 as f32 * (1.0 - f) + g1 as f32 * f) as u8,
        (b0 as f32 * (1.0 - f) + b1 as f32 * f) as u8,
    )
}

// ── 共享底：卷轴纸 ─────────────────────────────────────

/// 绘卷纸底：上下两道卷轴杆 + 纸纹。
fn scroll_paper(c: &mut CharCanvas) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let rod_h = (sh * 0.06).max(1.2);
    // 上下卷杆
    c.fill_rect(0.0, 0.0, sw, rod_h, GOLD_DIM, 0.5);
    c.fill_rect(0.0, sh - rod_h, sw, rod_h, GOLD_DIM, 0.5);
    c.line(0.0, rod_h, sw, rod_h, INK, 0.35);
    c.line(0.0, sh - rod_h - 0.6, sw, sh - rod_h - 0.6, INK, 0.35);
    // 纸底
    c.tint_rect(0.0, rod_h, sw, sh - rod_h * 2.0, PAPER, 0.10);
    // 纸纹（细横丝）
    let rows = 8;
    for r in 0..rows {
        let y = rod_h + (sh - rod_h * 2.0) * (r as f32 + 0.5) / rows as f32;
        for i in 0..((sw / 6.0) as usize) {
            let x = i as f32 * 6.0 + hf(i as u32 + r * 17, 5) * 4.0;
            c.set(x, y, GOLD_DIM, 0.10);
        }
    }
}

/// 横向卷动偏移：`speed` 单位是画布宽/秒。
fn scroll_x(local: f32, speed: f32, sw: f32) -> f32 {
    (local * speed * sw).rem_euclid(sw * 2.0) - sw * 0.5
}

/// 四季微缩景（卷轴里的一景）：占位宽 sw*0.22，一景一色。
fn mini_season(c: &mut CharCanvas, cx: f32, cy: f32, w: f32, h: f32, season: f32, t: f32) {
    let color = season_color(season);
    match (season.floor() as i32) % 4 {
        0 => {
            // 春：丘 + 蕾点
            c.ellipse(cx, cy + h * 0.3, w * 0.5, h * 0.22, color, 0.5);
            c.set(cx - w * 0.2, cy - h * 0.1, Color::Rgb(248, 152, 178), 0.6);
        }
        1 => {
            // 夏：雨丝 + 叶
            c.ellipse(cx, cy, w * 0.32, h * 0.30, color, 0.4);
            for k in 0..3 {
                c.line(cx - w * 0.3 + k as f32 * w * 0.25, cy - h * 0.4, cx - w * 0.34 + k as f32 * w * 0.25, cy - h * 0.2, PAPER, 0.3);
            }
        }
        2 => {
            // 秋：月 + 枝
            c.disc(cx + w * 0.2, cy - h * 0.25, h * 0.14, color, 0.7);
            c.line(cx - w * 0.4, cy + h * 0.2, cx + w * 0.1, cy - h * 0.05, Color::Rgb(74, 104, 62), 0.5);
        }
        _ => {
            // 冬：雪原 + 枯枝
            c.line(cx - w * 0.5, cy + h * 0.25, cx + w * 0.5, cy + h * 0.25, color, 0.4);
            c.line(cx - w * 0.1, cy + h * 0.2, cx + w * 0.15, cy - h * 0.2, Color::Rgb(52, 44, 40), 0.5);
        }
    }
    let _ = t;
}

// ── 昇華 10 卡 ──────────────────────────────────────────

/// 01 来タレリ：来路——一串脚印自下而上。
fn s01_footprints(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, _dur: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    // 地平线：开段空镜也要有结构，不能只剩几个孤点
    c.line(0.0, sh * 0.86, sw, sh * 0.86, MEM_BLUE, 0.40);
    c.line(0.0, sh * 0.885, sw, sh * 0.885, MEM_BLUE, 0.34);
    let n = 9;
    for i in 0..n {
        let u = i as f32 / n as f32;
        let x = sw * (0.5 + (u - 0.5) * 0.5 + (i % 2) as f32 * 0.02 - 0.01);
        let y = sh * (0.85 - u * 0.6);
        // 脚印出现得更密（每 0.18s 一个），否则开场前几帧几乎是空屏
        let g = fade(local / 1.4 - i as f32 * 0.18);
        if g <= 0.02 {
            continue;
        }
        c.ellipse(x, y, 1.15, 0.62, MEM_BLUE, (0.62 * g).clamp(0.0, 1.0));
        c.ellipse(x, y - 0.9, 0.70, 0.40, SNOWY(), (0.40 * g).clamp(0.0, 1.0));
    }
    // 远端雪坡的微光，保证整幅画面不空
    for k in 0..20u32 {
        let kf = k as f32;
        let x = ((kf * 0.613) % 1.0) * sw;
        let y = sh * (0.88 + ((kf * 0.371) % 1.0) * 0.10);
        let tw = 0.5 + 0.5 * (ctx.t as f32 * 1.7 + kf).sin();
        c.set(x, y, SNOWY(), (0.5 + tw * 0.25).clamp(0.0, 1.0));
    }
}

/// 02 雪ニ消エ：脚印被雪逐个埋平（从最老的开始消失）。
fn s02_snow_bury(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, _dur: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let n = 9;
    let bury = local * 0.35; // 已埋掉的脚印数
    for i in 0..n {
        let u = i as f32 / n as f32;
        let x = sw * (0.5 + (u - 0.5) * 0.5);
        let y = sh * (0.85 - u * 0.6);
        let alive = (i as f32 - bury).max(0.0);
        let g = fade(alive / 1.5);
        if g <= 0.02 {
            // 埋平后留一圈雪痕
            c.circle(x, y, 1.4, SNOWY(), 0.25);
            continue;
        }
        c.ellipse(x, y, 0.9, 0.5, MEM_BLUE, (0.55 * g).clamp(0.0, 1.0));
    }
}

fn const_fn_snow() -> Color {
    Color::Rgb(214, 228, 244)
}

fn SNOWY() -> Color {
    const_fn_snow()
}

/// 03 行ク先：前路未知——地平线上一条模糊的路伸向雾中。
fn s03_unknown_path(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, _dur: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let mut prev: Option<(f32, f32)> = None;
    for i in 0..=20 {
        let u = i as f32 / 20.0;
        let x = sw * (0.5 + (u - 0.5) * 0.08 + (local * 0.1).sin() * 0.01 * u);
        let y = sh * (0.9 - u * 0.35);
        if let Some(q) = prev {
            c.line(q.0, q.1, x, y, GOLD_DIM, (0.4 * (1.0 - u) + 0.05).clamp(0.0, 1.0));
        }
        prev = Some((x, y));
    }
    // 雾端
    c.tint_rect(0.0, sh * 0.3, sw, sh * 0.3, PAPER, 0.12);
}

/// 04 絵巻物：横卷轴——四季四景从右向左流过（核心卡）。
fn s04_emaki_scroll(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, _dur: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    scroll_paper(c);
    let rod = (sh * 0.06).max(1.2);
    let cy = rod + (sh - rod * 2.0) * 0.5;
    let h = (sh - rod * 2.0) * 0.7;
    let w = sw * 0.24;
    let off = scroll_x(local, 0.05, sw);
    for k in -2..=4i32 {
        let cx = sw * 0.5 + k as f32 * w * 1.15 - off;
        if cx < -w || cx > sw + w {
            continue;
        }
        // 景框
        c.rect(cx - w * 0.5, cy - h * 0.5, w, h, GOLD_DIM, 0.30);
        mini_season(c, cx, cy, w * 0.8, h * 0.8, k.rem_euclid(4) as f32, local);
    }
}

/// 05 展巻：卷轴展开动画——纸面从中间向两侧铺开。
fn s05_unroll(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, _dur: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let open = fade(local / 3.0);
    let half = sw * 0.5 * open;
    // 左右两卷滚筒
    c.fill_rect(sw * 0.5 - half - 1.2, sh * 0.12, 1.4, sh * 0.76, GOLD, 0.7);
    c.fill_rect(sw * 0.5 + half - 0.2, sh * 0.12, 1.4, sh * 0.76, GOLD, 0.7);
    // 纸面
    if half > 2.0 {
        c.tint_rect(sw * 0.5 - half, sh * 0.14, half * 2.0, sh * 0.72, PAPER, 0.14);
        let cy = sh * 0.5;
        for k in 0..3 {
            let px = sw * 0.5 + (k as f32 - 1.0) * sw * 0.18 * open;
            mini_season(c, px, cy, sw * 0.14 * open, sh * 0.5, k as f32, local);
        }
    }
}

/// 06 色ノ匂イ：色彩雾——四季色带在纸面上漂染。
fn s06_color_mist(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, _dur: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    for k in 0..4 {
        let u = k as f32 + local * 0.08;
        let color = season_color(u * 0.25);
        let x = sw * ((hf(k, 201) + local * 0.012 * (1.0 + k as f32 * 0.3)) % 1.0);
        let y = sh * hf(k, 202);
        c.disc(x, y, sh * 0.10, color, 0.12);
    }
}

/// 07 速サ：速度线——画面边缘向中心的流动（快到看不见）。
fn s07_speed_lines(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, _dur: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let (cx, cy) = (sw * 0.5, sh * 0.5);
    for i in 0..24 {
        let a = i as f32 * std::f32::consts::TAU / 24.0 + local * 0.15;
        let r0 = sh * 0.55;
        let r1 = sh * (0.18 + 0.2 * hf(i, 211)) + (local * 2.0 + i as f32).sin() * 2.0;
        c.line(
            cx + a.cos() * r0,
            cy + a.sin() * r0 * 0.7,
            cx + a.cos() * r1,
            cy + a.sin() * r1 * 0.7,
            GOLD,
            unit(0.25 + 0.2 * ctx.feat.energy_norm),
        );
    }
}

/// 08 独楽ノ様：陀螺四季色环——转速随 energy_norm（核心卡）。
fn s08_season_top(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, _dur: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let (cx, cy) = (sw * 0.5, sh * 0.46);
    let spin = local * (1.2 + ctx.feat.energy_norm * 3.5);
    // 四色环
    let seg = 28;
    for i in 0..seg {
        let a0 = spin + i as f32 * std::f32::consts::TAU / seg as f32;
        let a1 = a0 + std::f32::consts::TAU / seg as f32 * 0.85;
        let u = (i as f32 / seg as f32 * 4.0) % 4.0;
        let colr = season_color(u);
        let r = sh * 0.30;
        c.line(cx + a0.cos() * r, cy + a0.sin() * r * 0.92, cx + a1.cos() * r, cy + a1.sin() * r * 0.92, colr, 0.75);
    }
    // 轴心 + 底尖
    c.disc(cx, cy, 1.4, GOLD, 0.9);
    c.line(cx, cy + sh * 0.30, cx, cy + sh * 0.38, GOLD_DIM, 0.5);
    // 陀螺的呼吸摆动
    let wobble = (local * 1.3).sin() * 0.04;
    c.line(cx, cy, cx + wobble * sh * 0.3, cy + sh * 0.30, GOLD_DIM, 0.3);
}

/// 09 筆ヲ執リ：毛笔落纸——笔杆悬于纸面。
fn s09_brush_descend(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, _dur: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let drop = fade(local / 2.5);
    let bx = sw * 0.5;
    let tip_y = sh * (0.30 + 0.22 * drop);
    // 笔杆
    c.line(bx + 2.5, sh * 0.08, bx + 1.2, tip_y - 1.2, GOLD_DIM, 0.6);
    // 笔锋
    c.line(bx + 1.2, tip_y - 1.2, bx, tip_y, INK, 0.8);
    // 落点墨晕
    if drop > 0.95 {
        c.disc(bx, tip_y + 0.4, 1.1, INK, 0.5);
    }
    // 纸面提示线
    c.line(sw * 0.3, sh * 0.56, sw * 0.7, sh * 0.56, GOLD_DIM, 0.25);
}

/// 10 書キ留メン：笔画动画——粗线段逐笔写下「刹那」二字。
fn s10_write_kanji(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, _dur: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    scroll_paper(c);
    // 「刹那」两字的骨架笔画（相对坐标，按笔顺逐笔显现）
    // 刹：立刀旁与刍的简化骨架；那：阝+那的简化骨架
    let strokes_cut: [[f32; 4]; 6] = [
        [0.86, 0.30, 0.86, 0.62], // 竖（刀旁）
        [0.94, 0.34, 0.94, 0.58], // 短竖
        [0.58, 0.34, 0.82, 0.30], // 刍首横
        [0.62, 0.30, 0.60, 0.52], // 刍左折
        [0.60, 0.44, 0.84, 0.42], // 中横
        [0.62, 0.52, 0.86, 0.52], // 底横
    ];
    let strokes_na: [[f32; 4]; 6] = [
        [0.10, 0.28, 0.10, 0.64], // 阝竖
        [0.06, 0.40, 0.14, 0.40], // 阝横折
        [0.06, 0.54, 0.14, 0.54], // 阝二横
        [0.18, 0.34, 0.40, 0.34], // 那横
        [0.42, 0.30, 0.42, 0.62], // 那竖钩
        [f32::NAN, 0.0, 0.0, 0.0], // 空笔占位
    ];
    let all: [&[[f32; 4]; 6]; 2] = [&strokes_na, &strokes_cut];
    let total: usize = 11;
    let shown = ((local * 1.4) as usize).min(total);
    let mut done = 0usize;
    for (which, group) in all.iter().enumerate() {
        let ox = sw * (0.30 + 0.28 * which as f32);

        let oy = sh * 0.18;
        let gw = sw * 0.16;
        let gh = sh * 0.44;
        for st in group.iter() {
            if st[0].is_nan() {
                continue;
            }
            if done >= shown {
                return;
            }
            let prog = ((local * 1.4) - done as f32).clamp(0.0, 1.0);
            let x0 = ox + st[0] * gw;
            let y0 = oy + st[1] * gh;
            let x1 = x0 + (st[2] - st[0]) * gw * prog;
            let y1 = y0 + (st[3] - st[1]) * gh * prog;
            c.line(x0, y0, x1, y1, INK, 1.1);
            if prog >= 1.0 {
                done += 1;
            } else {
                return; // 正在写这一笔
            }
        }
    }
}

// ── 昇華入口 ────────────────────────────────────────────

type CardFn = fn(&SceneCtx, &mut CharCanvas, f32, f32);

pub struct AscCard {
    pub at: f32,
    pub name: &'static str,
    pub draw: CardFn,
}

/// 昇華 10 卡。唱句锚点（相对 175.55）：0 / 3.8 / 9.31 / 13.6 / 17.95 / 21.65 / 26.73。
pub const ASC_CARDS: &[AscCard] = &[
    AscCard { at: 0.0, name: "来タレリ", draw: s01_footprints },
    AscCard { at: 2.2, name: "雪ニ消エ", draw: s02_snow_bury },
    AscCard { at: 5.2, name: "行ク先", draw: s03_unknown_path },
    AscCard { at: 9.31, name: "絵巻物", draw: s04_emaki_scroll },
    AscCard { at: 12.2, name: "展巻", draw: s05_unroll },
    AscCard { at: 15.0, name: "色ノ匂イ", draw: s06_color_mist },
    AscCard { at: 17.95, name: "速サ", draw: s07_speed_lines },
    AscCard { at: 21.65, name: "独楽", draw: s08_season_top },
    AscCard { at: 26.73, name: "筆ヲ執リ", draw: s09_brush_descend },
    AscCard { at: 29.6, name: "書キ留メン", draw: s10_write_kanji },
];

pub fn render_ascension(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, dur: f32) {
    let card = ASC_CARDS.iter().rev().find(|k| local >= k.at).unwrap_or(&ASC_CARDS[0]);
    (card.draw)(ctx, c, local - card.at, dur);
    let _ = dur;
}

// ── 終章 10 卡 ──────────────────────────────────────────

/// 11 花ハ散リ：花瓣四散（褪色春）。
fn f01_petals_fall(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, _dur: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    for i in 0..26 {
        let speed = 0.6 + hf(i, 221) * 0.9;
        let phase = (local * 0.06 * speed + hf(i, 222)) % 1.0;
        let x = sw * ((hf(i, 223) + (local * 0.01).sin() * 0.1) % 1.0);
        let y = sh * phase;
        let spin = local * (1.0 + hf(i, 224)) + i as f32;
        let dx = spin.cos() * 0.9;
        let dy = spin.sin() * 0.5;
        c.line(x - dx, y - dy, x + dx, y + dy, Color::Rgb(220, 160, 150), 0.5);
    }
}

/// 12 月ハ欠ケ：月缺动画——用背景色 disc 叠咬月亮。
fn f02_moon_wane(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, _dur: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let (mx, my) = (sw * 0.72, sh * 0.20);
    let r = (sh * 0.07).max(2.4);
    c.disc(mx, my, r, PAPER, 0.85);
    c.circle(mx, my, r, PAPER, 0.3);
    // 缺口：随进度从右侧咬掉
    let bite = (local * 0.12).clamp(0.0, 0.95);
    let bx = mx + r * 2.0 * (1.0 - bite * 1.1).max(0.05);
    c.disc(bx, my - r * 0.3, r * (0.35 + bite * 0.75), NIGHT_INK(), 0.9);
}

fn const_night_ink() -> Color {
    Color::Rgb(24, 24, 28)
}

fn NIGHT_INK() -> Color {
    const_night_ink()
}

/// 13 人ハ去リ行ク：人影远去——剪影缩小走向地平线。
fn f03_people_leave(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, _dur: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let ink = Color::Rgb(30, 30, 34);
    let horizon = sh * 0.78;
    // 地平线与远处雪原：单个人影太孤，撑不起整幅画面
    c.line(0.0, horizon, sw, horizon, FADE_GREEN, 0.42);
    for k in 0..16u32 {
        let kf = k as f32;
        let x = ((kf * 0.717) % 1.0) * sw;
        let y = horizon + 1.2 + ((kf * 0.413) % 1.0) * (sh - horizon) * 0.45;
        c.disc(x, y, 0.55, FADE_GREEN, 0.55);
    }
    // 一队远去的人影：越远越小越淡
    for i in 0..4u32 {
        let f = i as f32;
        let u = (local * 0.05 + f * 0.11).clamp(0.0, 1.0);
        let x = sw * (0.30 + u * 0.38 + f * 0.045);
        let y = sh * (0.82 - u * 0.20);
        let s = (1.0 - u * 0.55) * (1.0 - f * 0.16);
        let a = (0.85 - f * 0.16).clamp(0.25, 1.0);
        c.disc(x, y - 2.6 * s, 0.95 * s, ink, a);
        c.line(x, y - 1.7 * s, x, y, ink, a);
        c.line(x, y, x - 0.85 * s, y + 0.45, ink, a * 0.8);
        c.line(x, y, x + 0.85 * s, y + 0.45, ink, a * 0.8);
    }
}

/// 14 記憶ノ：全屏暗淡，只剩记忆微光。
fn f04_memory_dim(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, _dur: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    c.tint(Color::Rgb(14, 16, 22), (0.5 * fade(local / 2.0)).clamp(0.0, 1.0));
    // 记忆微光：一片缓慢明灭的点，否则「暗淡」会退化成空屏
    for k in 0..28u32 {
        let kf = k as f32;
        let x = ((kf * 0.613) % 1.0) * sw;
        let y = ((kf * 0.427) % 1.0) * sh;
        let tw = 0.5 + 0.5 * (ctx.t as f32 * 1.3 + kf * 1.7).sin();
        c.set(x, y, FADE_GREEN, (0.5 + tw * 0.3).clamp(0.0, 1.0));
    }
    // 一道横向的记忆流光
    let y = sh * (0.5 + (local * 0.3).sin() * 0.05);
    let x = sw * ((local * 0.08) % 1.0);
    c.line(x - 6.0, y, x, y, FADE_GREEN, 0.48);
    c.line(x - 6.0, y + 0.7, x - 1.5, y + 0.7, FADE_GREEN, 0.36);
}

/// 15 美シサダケハ：一句词发亮——中央光带托起唯一亮点。
fn f05_beauty_glows(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, _dur: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    c.tint(Color::Rgb(14, 16, 22), 0.55);
    let (cx, cy) = (sw * 0.5, sh * 0.48);
    let breathe = 0.6 + 0.4 * (local * 1.1).sin();
    c.disc(cx, cy, sh * 0.06, FADE_GREEN, (0.35 * breathe).clamp(0.0, 1.0));
    c.circle(cx, cy, sh * 0.09, FADE_GREEN, (0.25 * breathe).clamp(0.0, 1.0));
    // 底部一线微光
    c.line(sw * 0.2, sh * 0.78, sw * 0.8, sh * 0.78, FADE_GREEN, 0.15);
}

/// 16 錆ビナイ前奏：金色在暗场里凝聚。
fn f16_gather_gold(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, _dur: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let (cx, cy) = (sw * 0.5, sh * 0.5);
    let g = fade(local / 2.5);
    for i in 0..40 {
        let a = hf(i, 231) * std::f32::consts::TAU + local * 0.4;
        let r = sh * 0.42 * (1.0 - g) + hf(i, 232) * 2.0;
        let x = cx + a.cos() * r;
        let y = cy + a.sin() * r * 0.85;
        c.set(x, y, GOLD, (0.5 * g).clamp(0.0, 1.0));
    }
    c.disc(cx, cy, sh * 0.03 + g * 1.2, GOLD, (0.5 * g).clamp(0.0, 1.0));
}

/// 17 錆ビツキハシナイ！：全曲最高点——金色脉冲环 + 全画面上扬（flash 驱动）。
fn f17_never_rust(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, _dur: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let (cx, cy) = (sw * 0.5, sh * 0.5);
    // 三层脉冲环：flash/beat 驱动
    for k in 0..3 {
        let p = ((local * 0.8 + k as f32 * 0.33) % 1.0).abs();
        let r = sh * (0.08 + p * 0.5);
        let g = (1.0 - p).powf(1.6);
        c.circle(cx, cy, r, GOLD, (0.25 + 0.65 * g + 0.3 * ctx.flash).clamp(0.0, 1.0));
    }
    // 上扬光线
    for i in 0..12 {
        let a = -std::f32::consts::FRAC_PI_2 + (i as f32 - 5.5) * 0.16;
        let r0 = sh * 0.14;
        let r1 = sh * (0.5 + 0.15 * ctx.flash + 0.08 * (local * 2.0 + i as f32).sin());
        c.line(cx + a.cos() * r0, cy + a.sin() * r0, cx + a.cos() * r1, cy + a.sin() * r1, GOLD, 0.4);
    }
    // 核心亮盘
    c.disc(cx, cy, sh * 0.05, col::WHITE, (0.7 + 0.3 * ctx.flash).clamp(0.0, 1.0));
    // 整屏上扬：底边金雾
    c.tint_rect(0.0, sh * 0.8, sw, sh * 0.2, GOLD, (0.12 + 0.15 * ctx.flash).clamp(0.0, 1.0));
}

/// 18 副歌再现：褪色春景——丘、蕾、东风（记忆版）。
fn f18_spring_reprise(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, _dur: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let horizon = sh * 0.58;
    // 褪色春丘
    c.ellipse(sw * 0.35, horizon + sh * 0.12, sw * 0.42, sh * 0.20, FADE_GREEN, 0.4);
    c.ellipse(sw * 0.75, horizon + sh * 0.16, sw * 0.30, sh * 0.16, Color::Rgb(120, 148, 118), 0.35);
    // 淡樱点
    for i in 0..10 {
        let x = sw * hf(i, 241);
        let y = horizon - sh * 0.1 * hf(i, 242);
        let drift = (local * 0.4 + i as f32).sin() * 1.5;
        c.set(x + drift, y, Color::Rgb(220, 160, 150), 0.5);
    }
    // 远去的东风线
    for k in 0..3 {
        let y = sh * (0.2 + k as f32 * 0.08);
        let off = (local * 3.0 + k as f32 * 20.0) % (sw + 20.0) - 10.0;
        c.line(off, y, off + 6.0, y - 0.5, MEM_BLUE, 0.25);
    }
}

/// 19 二度ト無ク：此刻独一无二——画面里同景双影（今日/明日）。
fn f19_never_twice(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, _dur: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let horizon = sh * 0.6;
    // 同一座丘的两个投影，相位差半秒
    for (k, colr) in [(0, FADE_GREEN), (1, MEM_BLUE)].iter() {
        let dt = local - (*k as f32) * 0.5;
        let x = sw * 0.5 + (dt * 0.5).sin() * sw * 0.1;
        let y = horizon + sh * 0.14;
        c.ellipse(x, y, sw * 0.34, sh * 0.17, *colr, 0.30);
    }
    // 分界竖线（今日 | 明日）
    c.line(sw * 0.5, sh * 0.2, sw * 0.5, horizon, PAPER, 0.2);
}

/// 20 銘：收卷——卷轴合拢，只余一枚朱印。
fn f20_seal_close(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, _dur: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let close = fade(local / 3.0);
    let half = sw * 0.5 * (1.0 - close) + 1.0;
    // 合拢的卷杆向中间移动
    c.fill_rect(sw * 0.5 - half - 1.2, sh * 0.10, 1.4, sh * 0.80, GOLD_DIM, 0.6);
    c.fill_rect(sw * 0.5 + half - 0.2, sh * 0.10, 1.4, sh * 0.80, GOLD_DIM, 0.6);
    // 朱印：右下方一枚方印
    let seal = sw * 0.5 + half * 0.4;
    let sy = sh * 0.62;
    let ss = (sh * 0.07).max(2.0);
    c.rect(seal - ss * 0.5, sy - ss * 0.5, ss, ss, VERDURIS, (0.8 * close).clamp(0.0, 1.0));
    c.line(seal - ss * 0.25, sy - ss * 0.2, seal + ss * 0.2, sy + ss * 0.15, PAPER, (0.7 * close).clamp(0.0, 1.0));
}

/// 終章 10 卡。唱句锚点（相对 224.9）：0 / 3.94 / 7.55 / 13.3 / 17.39 / 21.29。
pub const FIN_CARDS: &[AscCard] = &[
    AscCard { at: 0.0, name: "花ハ散リ", draw: f01_petals_fall },
    AscCard { at: 1.6, name: "月ハ欠ケ", draw: f02_moon_wane },
    AscCard { at: 3.2, name: "人ハ去リ行ク", draw: f03_people_leave },
    AscCard { at: 5.4, name: "記憶ノ", draw: f04_memory_dim },
    AscCard { at: 7.55, name: "美シサダケ", draw: f05_beauty_glows },
    AscCard { at: 9.4, name: "錆前奏", draw: f16_gather_gold },
    AscCard { at: 11.2, name: "錆ビナイ！", draw: f17_never_rust },
    AscCard { at: 13.3, name: "副歌再现", draw: f18_spring_reprise },
    AscCard { at: 17.39, name: "二度ト無ク", draw: f19_never_twice },
    AscCard { at: 21.3, name: "銘", draw: f20_seal_close },
];

pub fn render_finale(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, dur: f32) {
    let card = FIN_CARDS.iter().rev().find(|k| local >= k.at).unwrap_or(&FIN_CARDS[0]);
    (card.draw)(ctx, c, local - card.at, dur);
}

// ── 测试 ────────────────────────────────────────────────

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

    fn ctx(scene: SceneKind, t: f64, local: f32) -> SceneCtx<'static> {
        let lyrics = Box::leak(Box::new(Lyrics::default()));
        let f = feat(0.6);
        SceneCtx {
            t,
            local_t: local as f64,
            progress: 0.5,
            feat: f.clone(),
            prev: f,
            frame: 300,
            cols: 80,
            rows: 30,
            scene,
            lyrics,
            flash: 0.4,
            punch: 0.3,
            heartbeat: 0.2,
            glitch: 0.0,
            burst: 0.0,
            invert: 0.0,
        }
    }

    fn sig_of(c: &CharCanvas) -> Vec<u32> {
        c.pixels()
            .iter()
            .flat_map(|p| [p.w.to_bits(), p.r.to_bits(), p.g.to_bits(), p.b.to_bits()])
            .collect()
    }

    fn draw_all(scene: SceneKind, local: f32) -> Vec<Vec<u32>> {
        let (cards, entry): (&[AscCard], CardFn) = match scene {
            SceneKind::Ascension => (ASC_CARDS, render_ascension as CardFn),
            _ => (FIN_CARDS, render_finale as CardFn),
        };
        let mut sigs = Vec::new();
        for k in cards {
            let c = ctx(scene, 200.0 + k.at as f64, k.at + 0.5);
            let mut canvas = CharCanvas::new(80, 30, RenderMode::Braille);
            entry(&c, &mut canvas, k.at + 0.5, 45.0);
            let n = canvas.pixels().iter().filter(|p| !p.is_empty()).count();
            assert!(n > 20, "卡「{}」只画了 {n} 像素", k.name);
            sigs.push(sig_of(&canvas));
        }
        sigs
    }

    #[test]
    fn ascension_cards_all_draw() {
        draw_all(SceneKind::Ascension, 0.0);
    }

    #[test]
    fn finale_cards_all_draw() {
        draw_all(SceneKind::Finale, 0.0);
    }

    #[test]
    fn no_panic_on_tiny_canvas() {
        for scene in [SceneKind::Ascension, SceneKind::Finale] {
            let (cards, entry) = match scene {
                SceneKind::Ascension => (ASC_CARDS, render_ascension as CardFn),
                _ => (FIN_CARDS, render_finale as CardFn),
            };
            let c = ctx(scene, 200.0, 1.0);
            for k in cards {
                let mut canvas = CharCanvas::new(8, 5, RenderMode::Braille);
                entry(&c, &mut canvas, k.at + 0.5, 45.0);
            }
        }
    }

    #[test]
    fn cards_move_over_time() {
        for scene in [SceneKind::Ascension, SceneKind::Finale] {
            let (cards, entry) = match scene {
                SceneKind::Ascension => (ASC_CARDS, render_ascension as CardFn),
                _ => (FIN_CARDS, render_finale as CardFn),
            };
            let mut moved = 0;
            for k in cards {
                let c = ctx(scene, 200.0, k.at + 0.2);
                let mut a = CharCanvas::new(80, 30, RenderMode::Braille);
                let mut b = CharCanvas::new(80, 30, RenderMode::Braille);
                entry(&c, &mut a, k.at + 0.2, 45.0);
                entry(&c, &mut b, k.at + 1.5, 45.0);
                if sig_of(&a) != sig_of(&b) {
                    moved += 1;
                }
            }
            assert!(moved >= cards.len() / 2, "{scene:?} 动的卡只有 {moved}");
        }
    }

    #[test]
    fn moon_wane_bites_grow() {
        // 月缺的咬口应随时间扩大：对比两个时刻的暗像素数
        let scene = SceneKind::Finale;
        let count = |local: f32| {
            let c = ctx(scene, 230.0, local);
            let mut canvas = CharCanvas::new(80, 30, RenderMode::Braille);
            f02_moon_wane(&c, &mut canvas, local, 45.0);
            canvas
                .pixels()
                .iter()
                .filter(|p| !p.is_empty() && p.r < 0.15 && p.g < 0.15)
                .count()
        };
        let early = count(0.5);
        let late = count(7.5);
        assert!(late > early, "月缺没有随时间加深：{early} → {late}");
    }

    #[test]
    fn season_color_cycles() {
        let c0 = season_color(0.0);
        let c2 = season_color(2.0);
        assert_ne!(c0, c2, "四季色环必须换色");
        let mid = season_color(0.5);
        assert_ne!(mid, c0, "插值色不应与端点重合");
    }

    #[test]
    fn kanji_writes_progressively() {
        // 写字动画：更晚的时刻笔画更多
        let scene = SceneKind::Ascension;
        let ink = |local: f32| {
            let c = ctx(scene, 210.0, local);
            let mut canvas = CharCanvas::new(80, 30, RenderMode::Braille);
            s10_write_kanji(&c, &mut canvas, local, 45.0);
            canvas
                .pixels()
                .iter()
                .filter(|p| !p.is_empty() && p.r < 0.25 && p.g < 0.25 && p.b < 0.25)
                .count()
        };
        assert!(ink(6.0) > ink(0.4), "「刹那」笔画没有随时间增加");
    }
}

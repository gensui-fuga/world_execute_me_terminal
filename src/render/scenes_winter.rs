//! 冬（157.9–175.0s）：枯野银花，霜枝南天。
//!
//! 一夜落雪的枯原。枝条上银花逐点结晶、六角霜晶次第显形、
//! 枯梢透出新绿、雪里埋着仅有的暖色南天红果。
//! 两层视差落雪，风向随 t 缓摆。二十张卡共享一块雪原背景。

use ratatui::style::Color;

use crate::render::color as col;
use crate::render::CharCanvas;
use crate::render::scenes::SceneCtx;

/// 冬段色系（对齐 SceneKind::Winter::palette）。
const SNOW_WHITE: Color = Color::Rgb(214, 228, 244);
const COLD_BLUE: Color = Color::Rgb(120, 150, 190);
const NIGHT_BLUE: Color = Color::Rgb(26, 34, 48);
const VERDURIS: Color = Color::Rgb(235, 84, 66); // 朱红：南天
const NEW_GREEN: Color = Color::Rgb(110, 168, 96); // 蘇ル的新绿
const BRANCH: Color = Color::Rgb(52, 44, 40); // 枯枝

// ── 工具 ────────────────────────────────────────────────

fn hash_u32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^= x >> 16;
    x
}

/// 冬背景下的 0..1 归一（消除 f32 歧义）。
fn unit(x: f32) -> f32 {
    x.clamp(0.0, 1.0)
}

fn hf(i: u32, seed: u32) -> f32 {
    (hash_u32(i.wrapping_mul(0x9E37_79B1).wrapping_add(seed)) >> 8) as f32 / 16_777_216.0
}

fn fade(x: f32) -> f32 {
    let v = x.clamp(0.0, 1.0);
    v * v * (3.0 - 2.0 * v)
}

/// 六角雪花晶：主轴 + 六分叉。
fn snowflake(c: &mut CharCanvas, cx: f32, cy: f32, r: f32, color: Color, w: f32) {
    if r <= 0.4 {
        c.set(cx, cy, color, w);
        return;
    }
    for k in 0..6 {
        let a = k as f32 * std::f32::consts::FRAC_PI_3;
        let (dx, dy) = (a.cos(), a.sin());
        c.line(cx, cy, cx + dx * r, cy + dy * r, color, w);
        // 分叉
        let bx = cx + dx * r * 0.55;
        let by = cy + dy * r * 0.55;
        let b = a + 0.6;
        c.line(bx, by, bx + b.cos() * r * 0.4, by + b.sin() * r * 0.4, color, w * 0.7);
        let b2 = a - 0.6;
        c.line(bx, by, bx + b2.cos() * r * 0.4, by + b2.sin() * r * 0.4, color, w * 0.7);
    }
}

/// 一根带分叉的枯枝（参数化：x0,y0 → x1,y1）。
fn branch(c: &mut CharCanvas, pts: &[(f32, f32)], color: Color, w: f32) {
    for w2 in pts.windows(2) {
        c.line(w2[0].0, w2[0].1, w2[1].0, w2[1].1, color, w);
    }
}

/// 生成主枝折线（确定性）。
fn branch_path(sw: f32, sh: f32, seed: u32, rise: f32) -> Vec<(f32, f32)> {
    let x0 = sw * (0.06 + 0.10 * hf(seed, 1));
    let y0 = sh * 0.72;
    let x1 = sw * (0.42 + 0.22 * hf(seed, 2));
    let y1 = sh * (0.30 - rise * 0.05);
    let mut pts = Vec::new();
    let n = 6;
    for i in 0..=n {
        let u = i as f32 / n as f32;
        let bend = (u * std::f32::consts::PI).sin() * 0.06 * (hf(seed, 3) - 0.5);
        pts.push((
            x0 + (x1 - x0) * u + bend * sw * 0.3,
            y0 + (y1 - y0) * u * (0.9 + 0.1 * rise),
        ));
    }
    pts
}

// ── 共享背景：雪原 + 枯野地平线 ────────────────────────

/// 冬背景：夜空渐暗、雪原起伏、远山一线。`local` 段内秒。
fn winter_bg(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, dur: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    if sw < 8.0 || sh < 6.0 {
        return;
    }
    let horizon = sh * 0.62;
    // 上：冬夜蓝
    c.tint_rect(0.0, 0.0, sw, horizon, NIGHT_BLUE, 0.5);
    // 下：雪原（提亮）
    c.tint_rect(0.0, horizon, sw, sh - horizon, SNOW_WHITE, 0.14);

    // 雪原地平线 + 起伏
    c.line(0.0, horizon, sw, horizon, COLD_BLUE, 0.3);
    for i in 0..5 {
        let x0 = sw * hf(i, 61);
        let w = sw * (0.1 + 0.15 * hf(i, 62));
        let yy = horizon + (sh - horizon) * (0.2 + 0.2 * hf(i, 63));
        c.ellipse(x0, yy, w * 0.5, (sh - horizon) * 0.10, SNOW_WHITE, 0.12);
    }

    // 远山一线（背景层）
    let mut prev: Option<(f32, f32)> = None;
    for i in 0..=24 {
        let u = i as f32 / 24.0;
        let x = sw * u;
        let y = horizon - sh * (0.05 + 0.04 * (u * 7.0 + local * 0.02).sin());
        if let Some(q) = prev {
            c.line(q.0, q.1, x, y, Color::Rgb(70, 88, 112), 0.22);
        }
        prev = Some((x, y));
    }
}

/// 两层视差落雪：近层快、远层慢，风向缓摆。
fn falling_snow(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, count: usize) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let wind = (local * 0.08).sin() * 0.9 + (local * 0.021).sin() * 0.5;
    for i in 0..count {
        let layer = (i % 2) as u32; // 0 远 1 近
        let iu = i as u32;
        let speed = if layer == 0 { 1.4 } else { 3.2 } + hf(iu, 71) * 0.8;
        let drift = if layer == 0 { 0.3 } else { 0.8 } + wind * (0.5 + hf(iu, 72));
        let phase = (local * 0.055 * speed + hf(iu, 73)) % 1.0;
        let x = sw * ((hf(iu, 74) + drift * phase * 0.25 + local * 0.004 * drift).rem_euclid(1.0));
        let y = sh * ((phase + hf(iu, 75) * 0.001).rem_euclid(1.0));
        let col = if layer == 0 { COLD_BLUE } else { SNOW_WHITE };
        let w = if layer == 0 { 0.30 } else { 0.62 };
        c.set(x, y, col, w);
        if layer == 1 {
            // 近层带一点尾迹
            c.set(x - drift * 0.5, y - 0.6, SNOW_WHITE, 0.22);
        }
    }
}

// ── 卡片绘制 ────────────────────────────────────────────

/// 01 枯レ野原：地平线 + 枯草（第一句前奏）。
fn w01_dead_field(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, _dur: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let horizon = sh * 0.62;
    let n = 40;
    for i in 0..n {
        let x = sw * hf(i, 81);
        let gy = horizon + (sh - horizon) * 0.5 * hf(i, 82);
        let h = 1.0 + 2.2 * hf(i, 83);
        let sway = (local * 0.6 + i as f32).sin() * 0.3;
        c.line(x, gy, x + sway, gy - h, BRANCH, 0.45);
    }
}

/// 02 一夜ニシテ：夜色压顶，雪云翻卷（上 1/3 暗带滚动）。
fn w02_one_night(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, _dur: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let band = sh * 0.22;
    c.tint_rect(0.0, 0.0, sw, band, NIGHT_BLUE, 0.30);
    for i in 0..7 {
        let x = sw * ((hf(i, 91) + local * 0.012) % 1.0);
        let y = sh * (0.04 + 0.14 * hf(i, 92));
        c.ellipse(x, y, sw * 0.10, band * 0.4, Color::Rgb(44, 56, 74), 0.25);
    }
}

/// 03 銀ノ花：枝条上银花逐点结晶生长（核心卡）。
fn w03_silver_bloom(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, _dur: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let rise = (local / 8.0).clamp(0.0, 1.0);
    // 主枝
    let pts = branch_path(sw, sh, 5, rise);
    branch(c, &pts, BRANCH, 0.6);
    // 侧枝
    for s in 0..3 {
        let (bx, by) = pts[2 + s];
        let dir = if s % 2 == 0 { -1.0 } else { 1.0 };
        c.line(bx, by, bx + dir * sw * 0.08, by - sh * 0.10, BRANCH, 0.45);
    }
    // 银花结晶：沿枝逐点显现
    let n = 46;
    for i in 0..n {
        let appear = (local * 0.5 - i as f32 * 0.045) / 0.9;
        let g = fade(appear);
        if g <= 0.02 {
            continue;
        }
        let u = (i as f32 / n as f32) * 0.96;
        let idx = (u * (pts.len() - 1) as f32) as usize;
        let (bx, by) = pts[idx.min(pts.len() - 1)];
        let off = (hf(i, 101) - 0.5) * 3.0;
        let r = 0.5 + 1.3 * g;
        let tw = 0.6 + 0.4 * (local * 2.0 + i as f32).sin();
        snowflake(c, bx + off, by - 0.8 - hf(i, 102) * 1.2, r, SNOW_WHITE, w03w(g, tw));
    }
}

fn w03w(g: f32, tw: f32) -> f32 {
    (g * tw).clamp(0.0, 1.0)
}

/// 04 雪ノ炭：雪堆里半埋的红点（炭の隐喻前置）。
fn w04_buried_coal(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, _dur: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let horizon = sh * 0.62;
    for i in 0..3 {
        let x = sw * (0.3 + 0.25 * i as f32 + 0.05 * (local * 0.3).sin());
        let y = horizon + (sh - horizon) * (0.3 + 0.1 * i as f32);
        let g = fade(local / 2.0 - i as f32 * 0.5);
        if g <= 0.0 {
            continue;
        }
        c.disc(x, y, 1.0, Color::Rgb(70, 58, 52), 0.6 * g);
        c.set(x - 0.4, y - 0.5, SNOW_WHITE, 0.5 * g); // 覆雪的边
    }
}

/// 05 霜化粧：六角霜晶在枝上逐个显形。
fn w05_frost_crystals(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, _dur: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let pts = branch_path(sw, sh, 5, 1.0);
    branch(c, &pts, BRANCH, 0.55);
    for i in 0..8 {
        let g = fade((local - i as f32 * 0.7) / 1.2);
        if g <= 0.02 {
            continue;
        }
        let idx = ((hf(i, 111) * 0.9 + 0.05) * (pts.len() - 1) as f32) as usize;
        let (bx, by) = pts[idx];
        let r = 0.8 + 1.6 * g;
        snowflake(c, bx, by - 1.0 - hf(i, 112) * 1.5, r, SNOW_WHITE, (0.65 * g).clamp(0.0, 1.0));
    }
}

/// 06 老イタル枝：老枝末梢泛出新绿点。
fn w06_old_branch_green(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, _dur: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let pts = branch_path(sw, sh, 5, 1.0);
    branch(c, &pts, BRANCH, 0.55);
    let tips = [pts[3], pts[4], pts[5]];
    for (i, &(tx, ty)) in tips.iter().enumerate() {
        let g = fade((local - 1.0 - i as f32 * 0.8) / 1.5);
        if g <= 0.02 {
            continue;
        }
        let pulse = 0.6 + 0.4 * (local * 1.2 + i as f32).sin();
        for k in 0..3 {
            c.set(tx + (k as f32 - 1.0) * 0.8, ty - 0.6 - k as f32 * 0.4, NEW_GREEN, (0.55 * g * pulse).clamp(0.0, 1.0));
        }
    }
}

/// 07 蘇ル：新绿蔓延——绿点沿枝回爬。
fn w07_revive_spread(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, _dur: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let pts = branch_path(sw, sh, 5, 1.0);
    branch(c, &pts, BRANCH, 0.5);
    let back = (local * 0.4) % 1.0; // 沿枝回爬的进度
    let n = pts.len();
    for (i, &(px, py)) in pts.iter().enumerate().rev() {
        let u = 1.0 - i as f32 / n as f32;
        if u < back {
            continue;
        }
        if u > back + 0.35 {
            continue;
        }
        let g = 1.0 - (u - back) / 0.35;
        c.set(px, py - 0.7, NEW_GREEN, (0.6 * g).clamp(0.0, 1.0));
    }
}

/// 08 南天ノ：南天丛——画面右侧一株，红果成串。
fn w08_nanten_bush(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, _dur: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let horizon = sh * 0.62;
    let bx = sw * 0.78;
    let by = horizon + (sh - horizon) * 0.35;
    let g = fade(local / 2.5);
    // 茎
    c.line(bx, by + 2.0, bx - 0.6, by - 4.5, BRANCH, 0.55);
    c.line(bx - 0.6, by - 3.4, bx - 3.0, by - 5.2, BRANCH, 0.4);
    c.line(bx - 0.6, by - 3.4, bx + 1.8, by - 5.6, BRANCH, 0.4);
    // 叶（深绿短笔）
    for k in 0..4 {
        let ex = bx - 3.0 + k as f32 * 1.4;
        let ey = by - 5.0 - (k % 2) as f32;
        c.line(ex, ey, ex + 1.2, ey - 0.8, Color::Rgb(40, 72, 44), 0.5);
    }
    // 红果成串
    for k in 0..6 {
        let gx = bx - 2.4 + hf(k, 121) * 4.2;
        let gy = by - 4.6 - hf(k, 122) * 1.8;
        let tw = 0.55 + 0.45 * (local * 1.5 + k as f32).sin();
        c.disc(gx, gy, 0.55 + 0.2 * hf(k, 123), VERDURIS, (g * tw).clamp(0.0, 1.0));
        c.set(gx - 0.2, gy - 0.2, Color::Rgb(255, 140, 130), 0.5 * g * tw); // 高光
    }
}

/// 09 赤キ実：红果特写——三枚大果占屏。
fn w09_red_berries(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, _dur: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let g = fade(local / 2.0);
    for i in 0..3 {
        let x = sw * (0.38 + 0.12 * i as f32);
        let y = sh * (0.44 + 0.06 * (i % 2) as f32) + (local * 0.4 + i as f32).sin() * 0.4;
        let r = 2.0 + 0.4 * (local * 1.1 + i as f32).sin();
        c.disc(x, y, r, VERDURIS, (0.8 * g).clamp(0.0, 1.0));
        c.disc(x - r * 0.3, y - r * 0.35, r * 0.25, Color::Rgb(255, 150, 140), 0.6 * g);
        c.set(x + r * 0.4, y + r * 0.5, SNOW_WHITE, 0.4 * g); // 落雪一点
    }
}

/// 10 雪ノ炭：红果埋半寸雪——果顶覆雪帽。
fn w10_berry_in_snow(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, _dur: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let horizon = sh * 0.62;
    for i in 0..4 {
        let x = sw * (0.28 + 0.16 * i as f32);
        let y = horizon + (sh - horizon) * (0.42 + 0.06 * i as f32);
        let g = fade(local / 1.8 - i as f32 * 0.3);
        if g <= 0.02 {
            continue;
        }
        c.disc(x, y, 1.1, VERDURIS, (0.75 * g).clamp(0.0, 1.0));
        // 雪帽：上半 disc
        c.disc(x, y - 0.7, 0.9, SNOW_WHITE, (0.8 * g).clamp(0.0, 1.0));
        c.line(x - 0.9, y - 0.5, x + 0.9, y - 0.5, SNOW_WHITE, 0.7 * g);
    }
}

/// 11 白キ夢：全屏雪雾——雾幕 opacity 随 high()。
fn w11_white_dream(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, _dur: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let fog = (0.18 + 0.25 * ctx.high() + 0.06 * (local * 0.5).sin()).clamp(0.0, 0.5);
    c.tint_rect(0.0, 0.0, sw, sh, Color::Rgb(190, 204, 224), fog);
    // 雾团滚动
    for i in 0..5 {
        let x = sw * ((hf(i, 131) + local * 0.01 * (1.0 + hf(i, 132))) % 1.0);
        let y = sh * hf(i, 133);
        c.ellipse(x, y, sw * 0.14, sh * 0.08, SNOW_WHITE, 0.10);
    }
}

/// 12 燃ユルが如キ：雪雾中透出的暖光（冷中一点暖）。
fn w12_burning_cold(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, _dur: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    w11_white_dream(ctx, c, local, _dur);
    let x = sw * 0.5 + (local * 0.3).sin() * sw * 0.04;
    let y = sh * 0.5;
    let breathe = 0.6 + 0.4 * ctx.low();
    c.disc(x, y, sh * 0.10, VERDURIS, unit(0.18 * breathe));
    c.circle(x, y, sh * 0.14, Color::Rgb(255, 160, 140), unit(0.15 * breathe));
}

/// 13 吹雪：暴雪——近层密度骤增（呼应 punch）。
fn w13_blizzard(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, _dur: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let gust = 0.5 + 0.5 * ctx.punch;
    let wind = (local * 0.15).sin() * 1.4;
    for i in 0..120 {
        let layer = i % 2;
        let speed = if layer == 0 { 2.0 } else { 4.5 } + hf(i, 141) * 1.2;
        let phase = (local * 0.075 * speed + hf(i, 142)) % 1.0;
        let x = sw * ((hf(i, 143) + phase * (0.2 + wind * 0.1)).rem_euclid(1.0));
        let y = sh * phase;
        let dx = wind * 1.5;
        c.line(x, y, x + dx, y - 0.8, SNOW_WHITE, (if layer == 0 { 0.3 } else { 0.55 } + gust * 0.2).clamp(0.0, 1.0));
    }
}

/// 14 雪暖：雪原上唯一暖色常驻（南天丛远景，静置）。
fn w14_warm_spot(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, dur: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let horizon = sh * 0.62;
    let bx = sw * (0.78 - 0.02 * (local / dur).sin());
    let by = horizon + (sh - horizon) * 0.4;
    for k in 0..4 {
        let gx = bx - 1.2 + hf(k + 9, 121) * 2.6;
        let gy = by - 3.0 - hf(k + 9, 122) * 1.4;
        let tw = 0.5 + 0.5 * (local * 1.2 + k as f32).sin();
        c.disc(gx, gy, 0.5, VERDURIS, (0.6 * tw).clamp(0.0, 1.0));
    }
}

/// 15 雪弧：风卷雪成弧线（雪的运动可视化）。
fn w15_snow_arcs(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, _dur: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    for i in 0..6 {
        let cy = sh * hf(i, 151);
        let r = sh * (0.12 + 0.10 * hf(i, 152));
        let a0 = local * (0.5 + hf(i, 153)) + i as f32;
        let mut prev: Option<(f32, f32)> = None;
        for s in 0..=10 {
            let a = a0 + s as f32 * 0.22;
            let px = sw * 0.5 + a.cos() * r * 2.0;
            let py = cy + a.sin() * r * 0.5;
            if let Some(q) = prev {
                c.line(q.0, q.1, px, py, SNOW_WHITE, 0.2);
            }
            prev = Some((px, py));
        }
    }
}

/// 16 霜縁：画面四角结霜（边缘霜纹逼近）。
fn w16_frost_edges(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, _dur: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let grow = fade(local / 4.0);
    let corners = [
        (0.0f32, 0.0f32, 1.0f32, 1.0f32),
        (sw, 0.0, -1.0, 1.0),
        (0.0, sh, 1.0, -1.0),
        (sw, sh, -1.0, -1.0),
    ];
    for (i, &(x0, y0, dx, dy)) in corners.iter().enumerate() {
        let reach = (sw.min(sh) * 0.28) * grow;
        for k in 0..5 {
            let a = std::f32::consts::FRAC_PI_4 * (k as f32 - 2.0) * 0.35;
            let len = reach * (0.6 + 0.4 * hf(i as u32 * 7 + k, 161));
            let ex = x0 + dx * len * a.cos();
            let ey = y0 + dy * len * a.sin();
            c.line(x0 + dx * 1.5, y0 + dy * 1.5, ex, ey, SNOW_WHITE, (0.35 * grow).clamp(0.0, 1.0));
        }
    }
}

/// 17 枝雪落：枝头积雪脱落成团（雪块下坠）。
fn w17_branch_dump(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, _dur: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let pts = branch_path(sw, sh, 5, 1.0);
    branch(c, &pts, BRANCH, 0.5);
    // 枝上积雪
    for (i, &(px, py)) in pts.iter().enumerate() {
        if i % 2 == 0 {
            c.line(px - 1.0, py - 0.7, px + 1.0, py - 0.7, SNOW_WHITE, 0.5);
        }
    }
    // 周期性脱落的雪团
    let p = (local * 0.45) % 1.0;
    if p < 0.7 {
        let (bx, by) = pts[4];
        let y = by + p * p * (sh - by);
        c.disc(bx + p * 0.8, y, 1.0 - p * 0.6, SNOW_WHITE, (0.6 * (1.0 - p)).clamp(0.0, 1.0));
    }
}

/// 18 月淡：冬月淡出（月被云吃掉的过程）。
fn w18_moon_fade(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, _dur: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let mx = sw * 0.8;
    let my = sh * 0.14;
    let mr = (sh * 0.045).max(1.8);
    let vis = 1.0 - fade(local / 4.0);
    if vis > 0.02 {
        c.disc(mx, my, mr, SNOW_WHITE, (0.5 * vis).clamp(0.0, 1.0));
    }
    // 云吃月
    let cx = mx - sw * (0.12 - 0.12 * fade(local / 4.0));
    c.ellipse(cx, my, sw * 0.08, sh * 0.045, NIGHT_BLUE, 0.4);
}

/// 19 万象白：画面被雪白吞没（为昇華的雪道过门）。
fn w19_all_white(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, _dur: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let white = fade(local / 3.5);
    c.tint(SNOW_WHITE, (0.55 * white).clamp(0.0, 1.0));
    // 雪点愈密
    let n = (60.0 * white) as usize + 20;
    for i in 0..n {
        let iu = i as u32;
        let x = sw * hf(iu, 171);
        let y = sh * hf(iu, 172);
        c.set(x, y, SNOW_WHITE, 0.5);
    }
}

/// 20 夜静：雪停，只剩一点红果微光（冬收束）。
fn w20_night_still(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, _dur: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    c.tint(NIGHT_BLUE, (0.4 * fade(local / 2.5)).clamp(0.0, 1.0));
    let x = sw * 0.7;
    let y = sh * 0.55;
    let tw = 0.5 + 0.5 * (local * 1.8).sin();
    c.set(x, y, VERDURIS, (0.5 + 0.4 * tw).clamp(0.0, 1.0));
    c.set(x + 0.8, y + 0.2, VERDURIS, 0.3 * tw);
    c.set(x - 0.7, y - 0.3, VERDURIS, 0.25 * tw);
    // 一粒雪悬停
    c.set(x + 0.2, y - 1.6, SNOW_WHITE, 0.4 * (1.0 - tw) + 0.2);
}

// ── 卡表 ────────────────────────────────────────────────

type CardFn = fn(&SceneCtx, &mut CharCanvas, f32, f32);

pub struct WinterCard {
    pub at: f32,
    pub name: &'static str,
    pub draw: CardFn,
}

/// 冬 20 卡。唱句锚点（相对 157.9）：0 / 3.05 / 7.92 / 11.48，句后延伸。
pub const WINTER_CARDS: &[WinterCard] = &[
    WinterCard { at: 0.0, name: "枯レ野原", draw: w01_dead_field },
    WinterCard { at: 1.2, name: "一夜ニシテ", draw: w02_one_night },
    WinterCard { at: 2.4, name: "銀ノ花", draw: w03_silver_bloom },
    WinterCard { at: 4.6, name: "雪ノ炭", draw: w04_buried_coal },
    WinterCard { at: 6.0, name: "霜化粧", draw: w05_frost_crystals },
    WinterCard { at: 7.92, name: "老イタル枝", draw: w06_old_branch_green },
    WinterCard { at: 9.2, name: "蘇ル", draw: w07_revive_spread },
    WinterCard { at: 10.6, name: "南天ノ", draw: w08_nanten_bush },
    WinterCard { at: 11.48, name: "赤キ実", draw: w09_red_berries },
    WinterCard { at: 12.8, name: "雪ノ炭II", draw: w10_berry_in_snow },
    WinterCard { at: 14.2, name: "白キ夢", draw: w11_white_dream },
    WinterCard { at: 15.8, name: "燃ユル如キ", draw: w12_burning_cold },
    WinterCard { at: 17.1, name: "吹雪", draw: w13_blizzard },
    WinterCard { at: 18.6, name: "雪暖", draw: w14_warm_spot },
    WinterCard { at: 20.0, name: "雪弧", draw: w15_snow_arcs },
    WinterCard { at: 21.4, name: "霜縁", draw: w16_frost_edges },
    WinterCard { at: 22.8, name: "枝雪落", draw: w17_branch_dump },
    WinterCard { at: 24.2, name: "月淡", draw: w18_moon_fade },
    WinterCard { at: 25.6, name: "万象白", draw: w19_all_white },
    WinterCard { at: 27.0, name: "夜静", draw: w20_night_still },
];

/// 冬段入口：背景 + 落雪常驻 + 当卡。
pub fn render_winter(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, dur: f32) {
    winter_bg(ctx, c, local, dur);
    let card = WINTER_CARDS.iter().rev().find(|k| local >= k.at).unwrap_or(&WINTER_CARDS[0]);
    (card.draw)(ctx, c, local - card.at, dur);
    // 落雪常驻：基础 60 点，吹雪卡时交给卡片自己加密
    let blizzard = card.name == "吹雪" || card.name == "万象白";
    if !blizzard {
        falling_snow(ctx, c, local, 60);
    }
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

    fn ctx() -> SceneCtx<'static> {
        let lyrics = Box::leak(Box::new(Lyrics::default()));
        let f = feat(0.6);
        SceneCtx {
            t: 165.0,
            local_t: 7.1,
            progress: 0.4,
            feat: f.clone(),
            prev: f,
            frame: 200,
            cols: 80,
            rows: 30,
            scene: SceneKind::Winter,
            lyrics,
            flash: 0.0,
            punch: 0.0,
            heartbeat: 0.0,
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

    #[test]
    fn every_card_draws_something() {
        let c = ctx();
        for k in WINTER_CARDS {
            let mut canvas = CharCanvas::new(80, 30, RenderMode::Braille);
            winter_bg(&c, &mut canvas, k.at + 0.5, 28.0);
            (k.draw)(&c, &mut canvas, k.at + 0.5, 28.0);
            let n = canvas.pixels().iter().filter(|p| !p.is_empty()).count();
            assert!(n > 20, "冬卡「{}」只画了 {n} 像素", k.name);
        }
    }

    #[test]
    fn no_panic_on_tiny_canvas() {
        let c = ctx();
        for k in WINTER_CARDS {
            let mut canvas = CharCanvas::new(8, 5, RenderMode::Braille);
            winter_bg(&c, &mut canvas, k.at + 0.5, 28.0);
            (k.draw)(&c, &mut canvas, k.at + 0.5, 28.0);
        }
    }

    #[test]
    fn cards_move_over_time() {
        let c = ctx();
        let mut moved = 0;
        for k in WINTER_CARDS {
            let mut a = CharCanvas::new(80, 30, RenderMode::Braille);
            let mut b = CharCanvas::new(80, 30, RenderMode::Braille);
            winter_bg(&c, &mut a, k.at + 0.2, 28.0);
            (k.draw)(&c, &mut a, k.at + 0.2, 28.0);
            winter_bg(&c, &mut b, k.at + 1.4, 28.0);
            (k.draw)(&c, &mut b, k.at + 1.4, 28.0);
            if sig_of(&a) != sig_of(&b) {
                moved += 1;
            }
        }
        assert!(moved >= WINTER_CARDS.len() / 2, "动的卡只有 {moved}");
    }

    #[test]
    fn snow_is_deterministic() {
        let c = ctx();
        let mut a = CharCanvas::new(80, 30, RenderMode::Braille);
        let mut b = CharCanvas::new(80, 30, RenderMode::Braille);
        falling_snow(&c, &mut a, 10.0, 60);
        falling_snow(&c, &mut b, 10.0, 60);
        assert_eq!(sig_of(&a), sig_of(&b), "同帧落雪必须逐位一致");
    }

    #[test]
    fn winter_palette_red_punctuates() {
        // 南天红必须比主色亮差明显，保住「雪中一点红」
        let pw = {
            let (r, g, b) = col::rgb_of(VERDURIS);
            (0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32) / 255.0
        };
        let pm = {
            let (r, g, b) = col::rgb_of(SNOW_WHITE);
            (0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32) / 255.0
        };
        assert!((pm - pw).abs() > 0.2, "红果与雪底亮度差不足");
    }
}

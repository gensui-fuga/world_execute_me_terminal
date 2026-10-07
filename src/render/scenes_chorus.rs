//! 副歌（84.9–118.6s，33.7s）：移ロフは 空の気配と 人の世 / 留マらぬ 川の流れを
//! 眺メラン / 昨日咲キ 今日ハ散リユク 物語 / 静ヤカニ 心ノ綾ヲ 染メテ行ク。
//!
//! 十五张卡共享同一层「世界」：四色色环（春绿 / 夏青 / 秋金 / 冬蓝白）、
//! 下半幅的川、漂流的四色尘埃 —— 它们只依赖段内时钟，所以卡间必然连续。
//! 每张卡再叠加自己的强调层（气旋 / 人世灯火 / 川的白沫 / 花开花散 /
//! 心之绫的经纬与染色），强调层带 [`env`] 包络，在卡首尾精确归零。
//!
//! 唱句锚点（相对 84.9s）：0.0 / 3.92 / 7.86 / 12.02 —— 句首卡与唱句对齐；
//! 12.02 之后是 100.8–118.6 的器乐段，用意象展开把段落填满。
//!
//! 位置 / 大小 / 亮度全是时间的连续函数；伪随机一律走整数坐标哈希。

use ratatui::style::Color;

use crate::render::color as col;
use crate::render::motifs::{cicada_rings, envelope, frame_wipe, ki_flow, silver_arcs};
use crate::render::scenes::SceneCtx;
use crate::render::CharCanvas;

/// 副歌长度（timeline.toml: 84.9 → 118.6）。
pub const CHORUS_LEN: f32 = 33.7;

/// 四季四色（与各季 palette 主色一致，本地副本避免反复解构）。
const SPRING: Color = Color::Rgb(140, 214, 120); // 春绿
const SUMMER: Color = Color::Rgb(70, 196, 210); // 夏青
const AUTUMN: Color = Color::Rgb(240, 180, 88); // 秋金
const WINTER: Color = Color::Rgb(196, 218, 240); // 冬蓝白
const SEASON: [Color; 4] = [SPRING, SUMMER, AUTUMN, WINTER];

/// 副歌主色（`SceneKind::Chorus::palette` 的本地副本）。
const LEAF: Color = Color::Rgb(190, 214, 120);
const WATER: Color = Color::Rgb(120, 190, 190);
const PALE: Color = Color::Rgb(238, 244, 236);

const TAU: f32 = std::f32::consts::TAU;
const HALF_PI: f32 = std::f32::consts::FRAC_PI_2;

// ── 小工具 ────────────────────────────────────────────────

/// 单个 u32 整数哈希（与 scenes.rs 同款，独立副本避免跨文件耦合）。
fn hash_u32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^= x >> 16;
    x
}

/// 哈希 → 0..1（纯函数，无状态）。
fn hf(i: u32, seed: u32) -> f32 {
    (hash_u32(i.wrapping_mul(0x9E37_79B1).wrapping_add(seed)) >> 8) as f32 / 16_777_216.0
}

/// smoothstep：缓入缓出。
fn sstep(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// 卡内包络：卡首尾各 0.55s 淡入淡出，交界处精确为 0。
fn env(phase: f32, span: f32) -> f32 {
    if span <= 0.0 {
        return 1.0;
    }
    sstep(phase / 0.55)
        .min(sstep((span - phase) / 0.55))
        .clamp(0.0, 1.0)
}

/// 共享的川：下半幅五行流水线 + 顺流飞沫。
///
/// 位置 / 波长 / 亮度全是段内时钟的连续函数（`l` 为段内秒）。
fn river(c: &mut CharCanvas, ctx: &SceneCtx, l: f32, gain: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    if sw < 12.0 || sh < 10.0 {
        return;
    }
    let lines = 5;
    for k in 0..lines {
        let kf = k as f32;
        let y0 = sh * (0.70 + 0.055 * kf);
        let speed = 1.1 + 0.35 * kf + 0.5 * ctx.low();
        let tone = col::lerp(WATER, PALE, (kf / lines as f32) * 0.8);
        let steps = 40;
        let mut prev: Option<(f32, f32)> = None;
        for i in 0..=steps {
            let x = sw * i as f32 / steps as f32;
            let y = y0 + (x * 0.045 - l * speed).sin() * (1.2 + 0.5 * kf)
                + (x * 0.017 + l * speed * 0.6).sin() * 0.8;
            if let Some(q) = prev {
                c.line(q.0, q.1, x, y, tone, 0.50 * gain);
            }
            prev = Some((x, y));
        }
    }
    // 飞沫：哈希点顺流向右滑。
    for k in 0..24u32 {
        let x = ((hf(k, 71) + l * (0.03 + 0.03 * hf(k, 72))).fract()) * sw;
        let y = sh * (0.70 + 0.28 * hf(k, 73));
        c.set(x, y, PALE, 0.40 * gain);
    }
}

/// 一族平行弦（被半径 `r` 的圆裁切）—— 心之绫的经纬基本单元。
fn hatch_family(
    c: &mut CharCanvas,
    cx: f32,
    cy: f32,
    r: f32,
    ang: f32,
    lines: i32,
    tone: Color,
    w: f32,
) {
    if r <= 0.5 || lines < 2 || w <= 0.0 {
        return;
    }
    let (nx, ny) = (ang.cos(), ang.sin());
    let (tx, ty) = (-ny, nx);
    for k in 0..lines {
        let u = k as f32 / (lines - 1) as f32 * 2.0 - 1.0;
        let d = u * r * 0.92;
        let half = (r * r - d * d).max(0.0).sqrt();
        if half <= 0.4 {
            continue;
        }
        let mx = cx + nx * d;
        let my = cy + ny * d;
        c.line(
            mx - tx * half,
            my - ty * half,
            mx + tx * half,
            my + ty * half,
            tone,
            w,
        );
    }
}

/// 一朵花：`open` 0..1 控制花瓣张开程度。
fn flower(c: &mut CharCanvas, x: f32, y: f32, r: f32, open: f32, tone: Color, w: f32) {
    if r <= 0.3 || open <= 0.02 || w <= 0.0 {
        return;
    }
    let petals = 5;
    for i in 0..petals {
        let a = i as f32 / petals as f32 * TAU + 0.3;
        let p0 = (x + a.cos() * r * 0.25 * open, y + a.sin() * r * 0.25 * open);
        let p1 = (x + a.cos() * r * open, y + a.sin() * r * 0.9 * open);
        c.line(p0.0, p0.1, p1.0, p1.1, tone, w);
    }
    c.set(x, y, col::lerp(tone, col::WHITE, 0.5), (w * 1.2).min(1.0));
}

// ── 共享的「世界」 ────────────────────────────────────────

/// 副歌背景：四色色环 + 内环（色相轮转）+ 川 + 四色尘埃 + 入场 wipe。
///
/// 全部只依赖段内时钟 → 卡与卡之间的背景逐帧完全一致。
fn chorus_bg(ctx: &SceneCtx, c: &mut CharCanvas, l: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    if sw < 12.0 || sh < 10.0 {
        return;
    }
    let (cx, cy) = ctx.center(c);
    let r0 = ctx.radius(c);
    let rot = ctx.t as f32 * 0.22 + l * 0.06;
    let rr = r0 * (0.80 + 0.02 * (ctx.t as f32 * 0.7).sin());

    // ① 四季色环：四段弧各自本色，整体缓慢旋转（四季同时在画面里流动）。
    for i in 0..4u32 {
        let a0 = rot + i as f32 * HALF_PI;
        let a1 = a0 + HALF_PI * 0.92;
        let tone = SEASON[i as usize];
        let mut prev: Option<(f32, f32)> = None;
        for s in 0..=26 {
            let a = a0 + (a1 - a0) * s as f32 / 26.0;
            let p = (cx + rr * a.cos(), cy + rr * a.sin() * 0.92);
            if let Some(q) = prev {
                c.line(q.0, q.1, p.0, p.1, tone, 0.62);
            }
            prev = Some(p);
        }
        // 环上的季节标记：小方块。
        let am = (a0 + a1) * 0.5;
        let mx = cx + rr * am.cos();
        let my = cy + rr * am.sin() * 0.92;
        c.fill_rect(
            mx - 1.0,
            my - 1.0,
            2.0,
            2.0,
            col::lerp(tone, PALE, 0.35),
            0.60,
        );
    }

    // ② 内环：色相随时间推移 —— 「巡リ」的底色。
    let mut prev: Option<(f32, f32)> = None;
    for s in 0..=64 {
        let a = s as f32 / 64.0 * TAU - rot * 0.6;
        let tone = col::hue_shift(SEASON[((s / 16) % 4) as usize], (l * 12.0) % 360.0);
        let p = (
            cx + rr * 0.66 * a.cos(),
            cy + rr * 0.66 * a.sin() * 0.92,
        );
        if let Some(q) = prev {
            c.line(q.0, q.1, p.0, p.1, tone, 0.50);
        }
        prev = Some(p);
    }

    // ③ 川（留マらぬ 川の流れ）。
    river(c, ctx, l, 1.0);

    // ④ 四色尘埃：四种颜色同时在画面里横向流动。
    for k in 0..80u32 {
        let tone = SEASON[(k % 4) as usize];
        let x = ((hf(k, 61) + l * (0.020 + 0.020 * hf(k, 62))).fract()) * sw;
        let y = hf(k, 63) * sh * 0.98;
        c.set(x, y, tone, 0.35 + 0.25 * hf(k, 64));
    }

    // ⑤ 段落入口 wipe（自带 0..1.6s 包络，这里再收一道尾，避免末端硬切）。
    let wipe = 1.0 - sstep((l - 1.15) / 0.40);
    if wipe > 0.0 {
        frame_wipe(c, ctx, 0.85 * wipe);
    }
    // ⑥ 复用母题：银色弧（川的跳沫，自带 2–14s 包络）、蝉鸣环（巡りの同心环）、气粒流光。
    silver_arcs(c, ctx, 0.80);
    cicada_rings(c, ctx, 0.45);
    let mut seed = 0x9E37_79B9u32;
    let mut rng = move || {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (seed >> 8) as f32 / 16_777_216.0
    };
    ki_flow(c, ctx, 0.45, &mut rng);
}

// ── 卡片绘制函数 ──────────────────────────────────────────

/// 01 移ロフ空気：三缕气旋绕着中心打转，地平线上浮出人世的雏形。
fn c01_air_world(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, dur: f32) {
    let e = env(local, dur);
    if e <= 0.0 {
        return;
    }
    let (cx, cy) = ctx.center(c);
    let r0 = ctx.radius(c);
    for k in 0..3u32 {
        let ph = local * 1.1 + k as f32 * 2.1;
        let mut prev: Option<(f32, f32)> = None;
        for i in 0..=30 {
            let p = i as f32 / 30.0;
            let a = ph + p * 2.6;
            let rr = r0 * (0.35 + 0.45 * p);
            let pt = (cx + rr * a.cos(), cy + rr * a.sin() * 0.90);
            if let Some(q) = prev {
                c.line(
                    q.0,
                    q.1,
                    pt.0,
                    pt.1,
                    col::lerp(WATER, PALE, p),
                    0.45 * e * (1.0 - p * 0.5),
                );
            }
            prev = Some(pt);
        }
    }
    // 人の世：地平线与小屋剪影（随卡内进度长出来）。
    let grow = sstep(local / (dur * 0.6));
    let hy = cy + r0 * 0.72;
    c.line(
        cx - r0 * 0.95,
        hy,
        cx + r0 * 0.95,
        hy,
        col::lerp(WATER, PALE, 0.30),
        0.42 * e,
    );
    for k in 0..7u32 {
        let x = cx + (k as f32 - 3.0) * r0 * 0.26;
        let h = r0 * (0.10 + 0.06 * hf(k, 41)) * grow;
        if h <= 0.2 {
            continue;
        }
        c.line(x, hy, x, hy - h, LEAF, 0.45 * e);
        c.line(x - h * 0.4, hy - h, x + h * 0.4, hy - h, LEAF, 0.42 * e);
    }
}

/// 02 人ノ世：灯火在屋影里闪，人影沿地平线起伏。
fn c02_human_world(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, dur: f32) {
    let e = env(local, dur);
    if e <= 0.0 {
        return;
    }
    let (cx, cy) = ctx.center(c);
    let r0 = ctx.radius(c);
    let hy = cy + r0 * 0.72;
    for k in 0..9u32 {
        let x = cx + (hf(k, 43) - 0.5) * r0 * 1.9;
        let y = hy - r0 * (0.06 + 0.10 * hf(k, 44));
        let tw = 0.55 + 0.45 * (local * 3.0 + k as f32 * 1.7).sin();
        c.set(x, y, col::lerp(AUTUMN, PALE, 0.35), 0.55 * e * tw);
    }
    for k in 0..7u32 {
        let x = cx + (k as f32 - 3.0) * r0 * 0.24;
        let bob = (local * 1.4 + k as f32).sin() * r0 * 0.012;
        c.line(x, hy, x, hy - r0 * 0.09 + bob, PALE, 0.45 * e);
    }
}

/// 03 留マらぬ川：川面加亮的流线 + 顺流白沫（共享川之上再压一层强调）。
fn c03_flowing_river(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, dur: f32) {
    let e = env(local, dur);
    if e <= 0.0 {
        return;
    }
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    if sw < 12.0 || sh < 10.0 {
        return;
    }
    for k in 0..4u32 {
        let kf = k as f32;
        let y0 = sh * (0.72 + 0.055 * kf);
        let mut prev: Option<(f32, f32)> = None;
        for i in 0..=48 {
            let x = sw * i as f32 / 48.0;
            let y = y0 + (x * 0.05 - local * (1.6 + 0.3 * kf)).sin() * (1.0 + 0.4 * kf);
            if let Some(q) = prev {
                c.line(q.0, q.1, x, y, col::lerp(WATER, PALE, 0.70), 0.50 * e);
            }
            prev = Some((x, y));
        }
    }
    for k in 0..16u32 {
        let x = ((hf(k, 51) + local * (0.08 + 0.05 * hf(k, 52))).fract()) * sw;
        let y = sh * (0.70 + 0.28 * hf(k, 53));
        c.set(x, y, PALE, 0.55 * e);
    }
}

/// 04 眺メラン：一只「眺望」的眼弧，三支顺流箭头向右滑走。
fn c04_gazing(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, dur: f32) {
    let e = env(local, dur);
    if e <= 0.0 {
        return;
    }
    let (cx, cy) = ctx.center(c);
    let r0 = ctx.radius(c);
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let ex = cx;
    let ey = cy - r0 * 0.25;
    for (dir, tone) in [(-1.0f32, PALE), (1.0f32, WATER)] {
        let mut prev: Option<(f32, f32)> = None;
        for i in 0..=28 {
            let a = std::f32::consts::PI * i as f32 / 28.0;
            let pt = (ex + r0 * 0.55 * a.cos(), ey + dir * r0 * 0.20 * a.sin());
            if let Some(q) = prev {
                c.line(q.0, q.1, pt.0, pt.1, tone, 0.50 * e);
            }
            prev = Some(pt);
        }
    }
    if sw < 12.0 || sh < 10.0 {
        return;
    }
    for k in 0..3u32 {
        let u = (local * 0.45 + k as f32 * 0.33).fract();
        let x = sw * (0.12 + 0.70 * u);
        let y = sh * (0.74 + 0.06 * k as f32);
        let a = 0.50 * e * (1.0 - u * 0.5);
        c.line(x, y, x + 3.0, y, PALE, a);
        c.line(x + 3.0, y, x + 1.6, y - 1.0, PALE, a * 0.9);
        c.line(x + 3.0, y, x + 1.6, y + 1.0, PALE, a * 0.9);
    }
}

/// 05 昨日咲キ：岸边五朵花次第张开（每朵有自己的错峰）。
fn c05_bloom(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, dur: f32) {
    let e = env(local, dur);
    if e <= 0.0 {
        return;
    }
    let (cx, cy) = ctx.center(c);
    let r0 = ctx.radius(c);
    for k in 0..5u32 {
        let open = sstep((local - k as f32 * 0.30) / 0.90);
        if open <= 0.02 {
            continue;
        }
        let x = cx + (k as f32 - 2.0) * r0 * 0.42 + (hf(k, 61) - 0.5) * r0 * 0.10;
        let y = cy + r0 * 0.60 + (hf(k, 62) - 0.5) * r0 * 0.12;
        let tone = SEASON[(k % 4) as usize];
        c.line(x, y, x, y + r0 * 0.16, LEAF, 0.40 * e * open);
        flower(c, x, y, r0 * 0.16, open, tone, 0.50 * e);
    }
}

/// 06 今日散リユク：花瓣散落 —— 旋转着往下、往右飘。
fn c06_scatter(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, dur: f32) {
    let e = env(local, dur);
    if e <= 0.0 {
        return;
    }
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let (cx, cy) = ctx.center(c);
    for k in 0..22u32 {
        let life = (local * 0.30 + hf(k, 71)).fract();
        let x = cx + (hf(k, 72) - 0.5) * sw * 0.80 + life * sw * 0.16;
        let y = cy + (hf(k, 73) - 0.35) * sh * 0.30 + life * sh * 0.34;
        let a = 0.55 * e * (1.0 - life);
        let spin = local * 2.2 + k as f32;
        let tone = SEASON[(k % 4) as usize];
        c.line(
            x - spin.cos() * 1.0,
            y - spin.sin() * 0.5,
            x + spin.cos() * 1.0,
            y + spin.sin() * 0.5,
            tone,
            a,
        );
    }
}

/// 07 静ヤカニ：一切放慢，只剩两圈极缓的静默涟漪与中心一点。
fn c07_stillness(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, dur: f32) {
    let e = env(local, dur);
    if e <= 0.0 {
        return;
    }
    let (cx, cy) = ctx.center(c);
    let r0 = ctx.radius(c);
    for k in 0..2u32 {
        let cyc = (local * 0.30 + k as f32 * 0.5).fract();
        let life = envelope(cyc, 0.0, 0.25, 0.80, 1.0);
        if life <= 0.0 {
            continue;
        }
        c.circle(
            cx,
            cy,
            r0 * (0.30 + 0.60 * cyc),
            col::lerp(WATER, PALE, cyc),
            0.50 * e * life,
        );
    }
    c.set(cx, cy, PALE, 0.60 * e);
}

/// 08 心ノ綾：中心的织物经纬浮现（两组互相垂直的平行弦）。
fn c08_weave(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, dur: f32) {
    let e = env(local, dur);
    if e <= 0.0 {
        return;
    }
    let (cx, cy) = ctx.center(c);
    let r0 = ctx.radius(c);
    let r = r0 * 0.62 * sstep(local / dur.max(0.1));
    hatch_family(c, cx, cy, r, 0.55, 9, col::lerp(WATER, PALE, 0.35), 0.45 * e);
    hatch_family(
        c,
        cx,
        cy,
        r,
        0.55 + HALF_PI,
        9,
        col::lerp(WATER, PALE, 0.50),
        0.45 * e,
    );
}

/// 09 染メテ行ク：染料沿经纬从中心漫开，每一道颜色都是四季之一。
fn c09_dye(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, dur: f32) {
    let e = env(local, dur);
    if e <= 0.0 {
        return;
    }
    let (cx, cy) = ctx.center(c);
    let r0 = ctx.radius(c);
    let u = sstep(local / (dur * 0.80));
    let r = r0 * 0.62;
    for fam in 0..2u32 {
        let ang = 0.55 + fam as f32 * HALF_PI;
        let (nx, ny) = (ang.cos(), ang.sin());
        let (tx, ty) = (-ny, nx);
        for k in 0..9u32 {
            let d = (k as f32 / 8.0 * 2.0 - 1.0) * r * 0.92;
            let half = (r * r - d * d).max(0.0).sqrt() * u;
            if half <= 0.4 {
                continue;
            }
            let tone = SEASON[((k + fam * 2) % 4) as usize];
            let mx = cx + nx * d;
            let my = cy + ny * d;
            c.line(
                mx - tx * half,
                my - ty * half,
                mx + tx * half,
                my + ty * half,
                tone,
                0.50 * e,
            );
        }
    }
    // 从中心荡开的染色波。
    for k in 0..2u32 {
        let cyc = (local * 0.6 + k as f32 * 0.5).fract();
        let life = envelope(cyc, 0.0, 0.15, 0.70, 1.0);
        if life <= 0.0 {
            continue;
        }
        c.circle(
            cx,
            cy,
            r * cyc,
            col::lerp(SUMMER, PALE, cyc),
            0.50 * e * life,
        );
    }
}

/// 10 四季ノ環：四色弧被强调，象限里落下季节记号。
fn c10_season_ring(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, dur: f32) {
    let e = env(local, dur);
    if e <= 0.0 {
        return;
    }
    let (cx, cy) = ctx.center(c);
    let r0 = ctx.radius(c);
    let rr = r0 * 0.80;
    for i in 0..4u32 {
        let a0 = local * 0.60 + i as f32 * HALF_PI;
        let a1 = a0 + HALF_PI * 0.90;
        let tone = SEASON[i as usize];
        let mut prev: Option<(f32, f32)> = None;
        for s in 0..=20 {
            let a = a0 + (a1 - a0) * s as f32 / 20.0;
            let pt = (cx + rr * a.cos(), cy + rr * a.sin() * 0.92);
            if let Some(q) = prev {
                c.line(q.0, q.1, pt.0, pt.1, col::lerp(tone, PALE, 0.30), 0.55 * e);
            }
            prev = Some(pt);
        }
        let am = (a0 + a1) * 0.5;
        let mx = cx + rr * 0.72 * am.cos();
        let my = cy + rr * 0.72 * am.sin() * 0.92;
        c.line(mx - 1.5, my, mx + 1.5, my, tone, 0.50 * e);
        c.line(mx, my - 1.5, mx, my + 1.5, tone, 0.50 * e);
    }
}

/// 11 巡リ：四个色点绕环奔跑，拖着同色的尾；中心被卷进去。
fn c11_cycle(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, dur: f32) {
    let e = env(local, dur);
    if e <= 0.0 {
        return;
    }
    let (cx, cy) = ctx.center(c);
    let r0 = ctx.radius(c);
    let rr = r0 * 0.80;
    for k in 0..4u32 {
        let a = local * 1.10 + k as f32 * HALF_PI;
        let tone = col::hue_shift(SEASON[k as usize], (local * 40.0) % 360.0);
        let x = cx + rr * a.cos();
        let y = cy + rr * a.sin() * 0.92;
        c.disc(x, y, 1.6, tone, 0.60 * e);
        for t in 1..=5 {
            let aa = a - t as f32 * 0.06;
            let xx = cx + rr * aa.cos();
            let yy = cy + rr * aa.sin() * 0.92;
            c.set(xx, yy, tone, 0.45 * e * (1.0 - t as f32 / 6.0));
        }
    }
    let shrink = 1.0 - 0.30 * sstep(local / dur.max(0.1));
    c.circle(cx, cy, r0 * 0.22 * shrink, PALE, 0.50 * e);
}

/// 12 花ト川：花瓣顺流而下，岸边还留着两朵。
fn c12_flower_river(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, dur: f32) {
    let e = env(local, dur);
    if e <= 0.0 {
        return;
    }
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let (cx, cy) = ctx.center(c);
    let r0 = ctx.radius(c);
    if sw >= 12.0 && sh >= 10.0 {
        for k in 0..14u32 {
            let x = ((hf(k, 81) + local * (0.10 + 0.04 * hf(k, 82))).fract()) * sw;
            let y = sh * (0.72 + 0.24 * hf(k, 83));
            let tone = SEASON[(k % 4) as usize];
            let spin = local * 1.6 + k as f32;
            c.line(
                x - spin.cos() * 1.2,
                y - spin.sin() * 0.4,
                x + spin.cos() * 1.2,
                y + spin.sin() * 0.4,
                tone,
                0.50 * e,
            );
        }
    }
    for k in 0..2u32 {
        let x = cx + (k as f32 * 2.0 - 1.0) * r0 * 0.60;
        let y = cy + r0 * 0.62;
        flower(
            c,
            x,
            y,
            r0 * 0.14,
            0.85,
            SEASON[((k + 2) as usize) % 4],
            0.50 * e,
        );
    }
}

/// 13 綾ニ色：四条季节色带斜穿织物，心之绫彻底被染上四季。
fn c13_dyed_weave(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, dur: f32) {
    let e = env(local, dur);
    if e <= 0.0 {
        return;
    }
    let (cx, cy) = ctx.center(c);
    let r0 = ctx.radius(c);
    let r = r0 * 0.62;
    for i in 0..4u32 {
        let ang = 0.55 + i as f32 * (HALF_PI / 2.0);
        hatch_family(
            c,
            cx,
            cy,
            r,
            ang,
            3,
            col::lerp(SEASON[i as usize], PALE, 0.25),
            0.50 * e,
        );
    }
    hatch_family(c, cx, cy, r * 0.98, 0.55, 7, col::lerp(WATER, PALE, 0.30), 0.40 * e);
}

/// 14 静ノ中ノ動：画面几乎静止，只有中心一圈极缓的脉动在证明它还活着。
fn c14_pulse(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, dur: f32) {
    let e = env(local, dur);
    if e <= 0.0 {
        return;
    }
    let (cx, cy) = ctx.center(c);
    let r0 = ctx.radius(c);
    for k in 0..2u32 {
        let cyc = (local * 0.35 + k as f32 * 0.5).fract();
        let life = envelope(cyc, 0.0, 0.30, 0.85, 1.0);
        if life <= 0.0 {
            continue;
        }
        c.circle(
            cx,
            cy,
            r0 * (0.10 + 0.75 * cyc),
            col::lerp(LEAF, PALE, cyc),
            0.45 * e * life,
        );
    }
    let br = 0.6 + 0.4 * (local * 1.2).sin();
    c.disc(cx, cy, 1.4 + br, PALE, 0.45 * e * br);
}

/// 15 還ル：四色螺线向心收束，一圈光环向外散开收卷。
fn c15_return(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, dur: f32) {
    let e = env(local, dur);
    if e <= 0.0 {
        return;
    }
    let (cx, cy) = ctx.center(c);
    let r0 = ctx.radius(c);
    let u = sstep(local / (dur * 0.85));
    for s in 0..4u32 {
        let tone = SEASON[s as usize];
        let mut prev: Option<(f32, f32)> = None;
        for i in 0..=60 {
            let p = i as f32 / 60.0;
            let a = s as f32 * HALF_PI + p * 3.4 + u * 2.2;
            let rr = r0 * 0.82 * (1.0 - 0.75 * p) * (1.0 - 0.45 * u);
            let pt = (cx + rr * a.cos(), cy + rr * a.sin() * 0.92);
            if let Some(q) = prev {
                c.line(q.0, q.1, pt.0, pt.1, tone, 0.50 * e);
            }
            prev = Some(pt);
        }
    }
    for k in 0..2u32 {
        let cyc = (local * 0.5 + k as f32 * 0.5).fract();
        let life = envelope(cyc, 0.0, 0.20, 0.75, 1.0);
        if life <= 0.0 {
            continue;
        }
        c.circle(
            cx,
            cy,
            r0 * (0.15 + 0.85 * cyc),
            col::lerp(PALE, SUMMER, cyc),
            0.55 * e * life,
        );
    }
}

// ── 卡片表 ────────────────────────────────────────────────

/// 卡：段内起始秒（相对副歌起点 84.9s）、卡名、绘制函数。
///
/// `draw` 收到 `(ctx, canvas, local, dur)`：`local` 是卡内相位、`dur` 是本卡跨度。
type CardFn = fn(&SceneCtx, &mut CharCanvas, f32, f32);

pub struct ChorusCard {
    /// 相对段起点的秒数
    pub at: f32,
    pub name: &'static str,
    pub draw: CardFn,
}

/// 副歌 15 卡。唱句锚点（相对 84.9s）：0.0 / 3.92 / 7.86 / 12.02 ——
/// 句首卡压在唱句上；12.02 之后 15.9–33.7s 是器乐段，用意象展开铺满。
pub const CHORUS_CARDS: &[ChorusCard] = &[
    ChorusCard {
        at: 0.0,
        name: "移ロフ空気",
        draw: c01_air_world,
    },
    ChorusCard {
        at: 2.0,
        name: "人ノ世",
        draw: c02_human_world,
    },
    ChorusCard {
        at: 3.92,
        name: "留マらぬ川",
        draw: c03_flowing_river,
    },
    ChorusCard {
        at: 5.9,
        name: "眺メラン",
        draw: c04_gazing,
    },
    ChorusCard {
        at: 7.86,
        name: "昨日咲キ",
        draw: c05_bloom,
    },
    ChorusCard {
        at: 9.9,
        name: "今日散リユク",
        draw: c06_scatter,
    },
    ChorusCard {
        at: 12.02,
        name: "静ヤカニ",
        draw: c07_stillness,
    },
    ChorusCard {
        at: 14.2,
        name: "心ノ綾",
        draw: c08_weave,
    },
    ChorusCard {
        at: 16.4,
        name: "染メテ行ク",
        draw: c09_dye,
    },
    ChorusCard {
        at: 18.6,
        name: "四季ノ環",
        draw: c10_season_ring,
    },
    ChorusCard {
        at: 20.8,
        name: "巡リ",
        draw: c11_cycle,
    },
    ChorusCard {
        at: 23.0,
        name: "花ト川",
        draw: c12_flower_river,
    },
    ChorusCard {
        at: 25.2,
        name: "綾ニ色",
        draw: c13_dyed_weave,
    },
    ChorusCard {
        at: 27.4,
        name: "静ノ中ノ動",
        draw: c14_pulse,
    },
    ChorusCard {
        at: 29.8,
        name: "還ル",
        draw: c15_return,
    },
];

/// 卡名导出（供日志 / Lead 汇总）。
pub fn card_names() -> Vec<&'static str> {
    CHORUS_CARDS.iter().map(|k| k.name).collect()
}

/// 按段内时间选卡：返回 `(下标, 卡内相位, 本卡跨度)`。
fn pick(l: f32) -> (usize, f32, f32) {
    let mut idx = 0usize;
    for (i, k) in CHORUS_CARDS.iter().enumerate() {
        if l >= k.at {
            idx = i;
        }
    }
    let at = CHORUS_CARDS[idx].at;
    let end = CHORUS_CARDS
        .get(idx + 1)
        .map(|k| k.at)
        .unwrap_or(CHORUS_LEN);
    (idx, (l - at).max(0.0), (end - at).max(0.1))
}

/// 副歌入口：共享世界 + 按 `ctx.local_t` 选卡绘制。
///
/// `alpha` 是整段不透明度：只做「向黑压暗」（tint 改色不改覆盖度），
/// 绝不乘 `Pixel.w`。
pub fn render_card(ctx: &SceneCtx, canvas: &mut CharCanvas, alpha: f32) {
    if alpha <= 0.02 {
        return;
    }
    let l = ctx.local_t as f32;
    let (idx, phase, span) = pick(l);
    chorus_bg(ctx, canvas, l);
    (CHORUS_CARDS[idx].draw)(ctx, canvas, phase, span);
    if alpha < 0.999 {
        canvas.tint(col::BLACK, (1.0 - alpha) * 0.85);
    }
}

// ── 测试 ──────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::features::{FrameFeatures, BAND_COUNT};
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
        f
    }

    fn ctx_at(local: f32) -> SceneCtx<'static> {
        let f = feat(0.6);
        SceneCtx {
            t: 84.9 + local as f64,
            local_t: local as f64,
            progress: local / CHORUS_LEN,
            feat: f.clone(),
            prev: f,
            frame: ((84.9 + local as f64) * 24.0) as u64,
            cols: 80,
            rows: 24,
            scene: SceneKind::Chorus,
            lyrics: Box::leak(Box::new(Lyrics::default())),
            flash: 0.0,
            punch: 0.0,
            heartbeat: 0.0,
            glitch: 0.0,
            burst: 0.0,
            invert: 0.0,
        }
    }

    fn sig(local: f32) -> Vec<u32> {
        let ctx = ctx_at(local);
        let mut canvas = CharCanvas::new(80, 24, RenderMode::Braille);
        render_card(&ctx, &mut canvas, 1.0);
        canvas
            .pixels()
            .iter()
            .flat_map(|p| [p.w.to_bits(), p.r.to_bits(), p.g.to_bits(), p.b.to_bits()])
            .collect()
    }

    fn lit(local: f32) -> usize {
        let ctx = ctx_at(local);
        let mut canvas = CharCanvas::new(80, 24, RenderMode::Braille);
        render_card(&ctx, &mut canvas, 1.0);
        canvas.pixels().iter().filter(|p| !p.is_empty()).count()
    }

    fn coverage(local: f32) -> f32 {
        let ctx = ctx_at(local);
        let mut canvas = CharCanvas::new(80, 24, RenderMode::Braille);
        render_card(&ctx, &mut canvas, 1.0);
        canvas.pixels().iter().map(|p| p.w).sum()
    }

    #[test]
    fn fifteen_cards_exist() {
        assert_eq!(CHORUS_CARDS.len(), 15, "副歌必须是 15 张卡");
        assert_eq!(card_names().len(), 15);
    }

    #[test]
    fn sentence_cards_align_with_lyrics() {
        // 唱句锚点（相对 84.9）：0.0 / 3.92 / 7.86 / 12.02
        for at in [0.0f32, 3.92, 7.86, 12.02] {
            assert!(
                CHORUS_CARDS.iter().any(|k| (k.at - at).abs() < 0.01),
                "缺少与唱句对齐的卡：{at}"
            );
        }
    }

    #[test]
    fn every_card_lights_up() {
        for k in CHORUS_CARDS {
            let n = lit(k.at + 0.6);
            assert!(n > 30, "副歌卡「{}」只点亮 {n} 个像素", k.name);
        }
    }

    #[test]
    fn every_card_moves() {
        for k in CHORUS_CARDS {
            let a = sig(k.at + 0.15);
            let b = sig(k.at + 0.95);
            assert!(a != b, "副歌卡「{}」在 0.8s 内完全静止", k.name);
        }
    }

    #[test]
    fn no_panic_on_tiny_canvas() {
        for (cols, rows) in [(4u16, 3u16), (1, 1), (8, 5)] {
            for k in CHORUS_CARDS {
                let ctx = ctx_at(k.at + 0.5);
                let mut canvas = CharCanvas::new(cols, rows, RenderMode::Braille);
                render_card(&ctx, &mut canvas, 1.0);
            }
        }
    }

    #[test]
    fn boundaries_are_seamless() {
        for k in CHORUS_CARDS.iter().skip(1) {
            let before = coverage(k.at - 0.05);
            let after = coverage(k.at + 0.05);
            let d = (after - before).abs() / after.max(1.0);
            assert!(d < 0.20, "副歌卡「{}」交界跳变 {:.1}%", k.name, d * 100.0);
        }
    }

    #[test]
    fn alpha_zero_draws_nothing() {
        let ctx = ctx_at(5.0);
        let mut canvas = CharCanvas::new(80, 24, RenderMode::Braille);
        render_card(&ctx, &mut canvas, 0.0);
        let n = canvas.pixels().iter().filter(|p| !p.is_empty()).count();
        assert_eq!(n, 0, "alpha=0 时不应有像素");
    }
}

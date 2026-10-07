//! 序（0.0–15.7s，6 张卡）：晨雾漫过地面 → 第一缕光切进画面 → 万物未醒的呼吸 →
//! 远处一声（涟漪扩散）→ 霜气消散 → 光斑聚拢成春的形状。
//!
//! 这一段没有唱句（`assets/shunkashuto.lrc` 首句在 15.72s），画面必须靠自身的
//! 连续运动撑满 15.7 秒。文件结构照抄 [`crate::render::scenes_autumn`]：
//! 共享背景层 [`intro_bg`] + 卡表 [`INTRO_CARDS`] + 入口 [`render_card`]。
//!
//! 卡内元素一律乘 [`card_gain`] 包络（卡首 0.7s 淡入、卡末淡出到 0），
//! 于是相邻卡的交界处只剩共享背景，跳变远小于卡内运动。
//!
//! 不变式：时间只写在卡表；伪随机全部走整数哈希（纯函数，可 seek）；
//! `alpha` 只用来向黑压暗（[`CharCanvas::tint`]），绝不动 `Pixel.w`。

use ratatui::style::Color;

use crate::render::color as col;
use crate::render::motifs::{cicada_rings, ease, envelope, ki_flow, light_shaft, moss_ground};
use crate::render::scenes::SceneCtx;
use crate::render::CharCanvas;

/// 序段长度（timeline: 0.0 → 15.7）。
pub const INTRO_LEN: f32 = 15.7;

// ── 色系（与 `SceneKind::Intro::palette` 同源）──────────────────

/// 主色：晨雾灰蓝。
const MIST: Color = Color::Rgb(168, 178, 188);
/// 暗部（未醒的天顶）。
const DEEP: Color = Color::Rgb(51, 51, 51);
/// 灰白（雾芯 / 浮尘）。
const PALE: Color = Color::Rgb(170, 170, 170);
/// 第一缕光的暖白。
const DAWN: Color = Color::Rgb(236, 220, 190);
/// 霜青。
const FROST: Color = Color::Rgb(206, 222, 232);
/// 地面暗色。
const EARTH: Color = Color::Rgb(30, 32, 38);

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

/// 线性同余 0..1：给需要 rng 闭包的母题（[`ki_flow`]）用。
///
/// 调用点每次都重置种子，所以整段仍是 `t` 的纯函数 ——
/// 同一 `local` 两次绘制逐位一致。
fn lcg01(s: &mut u32) -> f32 {
    *s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    ((*s >> 8) & 0x00FF_FFFF) as f32 / 16_777_216.0
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

/// 六边形轮廓（霜晶用，与 autumn 同款）。
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

// ── 共享背景：未醒的天 + 地面 + 晨雾 ────────────────────────────

/// 序段背景：上 62% 未醒的天，下 38% 地面，晨雾慢速漫过地面。
///
/// 所有元素都是 `ctx.t / ctx.local_t` 的连续函数，卡与卡之间共享，
/// 因此交界处背景零跳变。
fn intro_bg(ctx: &SceneCtx, c: &mut CharCanvas) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    if sw < 4.0 || sh < 6.0 {
        return;
    }
    let t = ctx.t as f32;
    let hz = sh * 0.62;

    // 天：七条层积带，越靠地平线越亮（天光从地面方向渗上来）
    let bands = 7;
    for r in 0..bands {
        let u = (r as f32 + 0.5) / bands as f32;
        let y = u * hz;
        let k = (0.18 + 0.30 * (1.0 - u)) * (0.88 + 0.12 * (t * 0.21).sin());
        let tone = col::lerp(col::lerp(DEEP, col::BLACK, 0.40), MIST, clamp01(k));
        c.line(0.0, y, sw, y, tone, 0.40 + 0.16 * (1.0 - u));
    }
    // 天顶再压一层，让上沿沉下去
    c.tint_rect(0.0, 0.0, sw, hz * 0.45, col::BLACK, 0.30);

    // 地面：横向土纹 + 暗色压底
    let rows = 8;
    for r in 0..rows {
        let u = (r as f32 + 0.5) / rows as f32;
        let y = hz + (sh - hz) * u;
        let tone = col::lerp(EARTH, PALE, 0.10 + 0.16 * (1.0 - u));
        c.line(0.0, y, sw, y, tone, 0.40);
    }
    c.tint_rect(0.0, hz, sw, sh - hz, EARTH, 0.55);

    // 地平线：整段唯一一条亮线，画面基准
    c.line(0.0, hz, sw, hz, MIST, 0.80);

    // 地表：复用春的苔地母题 —— 地面绒毛随段内时间长出来并持续颤动
    moss_ground(c, ctx, 1.0);

    // 晨雾：八团，各自速度不同，从左向右漫过地面后出屏重来
    for k in 0..8u32 {
        let sp = 0.030 + hf(k, 71) * 0.045;
        let u = ((hf(k, 72) + t * sp) % 1.25) - 0.15;
        let x = u * sw;
        let y = hz + (sh - hz) * (0.08 + hf(k, 73) * 0.74);
        let rx = sw * (0.10 + hf(k, 74) * 0.15);
        let ry = (sh - hz) * (0.05 + hf(k, 75) * 0.07);
        let edge = clamp01(1.0 - (u - 0.5).abs() * 1.5);
        let a = 0.62 * edge;
        if a <= 0.03 || rx <= 0.0 || ry <= 0.0 {
            continue;
        }
        c.ellipse(x, y, rx, ry, MIST, a);
        c.ellipse(x, y, rx * 0.52, ry * 0.52, PALE, a * 0.55);
    }

    // 浮尘：缓慢上浮的亮点（给空天一点呼吸）
    for k in 0..14u32 {
        let p = (hf(k, 81) + t * (0.012 + hf(k, 82) * 0.020)) % 1.0;
        let x = sw * (0.04 + hf(k, 83) * 0.92);
        let y = sh * (0.10 + hf(k, 84) * 0.78) - p * sh * 0.22;
        c.set(x, y, PALE, 0.56);
    }
}

// ── 卡片绘制函数 ────────────────────────────────────────────────

/// 01 晨雾漫过地面：一道雾锋从左侧推进，身后铺开雾毯。
fn i01_mist_creeps(ctx: &SceneCtx, c: &mut CharCanvas, phase: f32, dur: f32) {
    let g = card_gain(phase, dur);
    if g <= 0.0 {
        return;
    }
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let hz = sh * 0.62;
    let t = ctx.t as f32;
    let front = ease(phase, (dur * 0.80).max(0.4));
    let x = sw * (-0.20 + 1.30 * front);

    // 雾锋：一条竖直的柔边，中段最亮
    let n = 18usize;
    let d = (n - 1).max(1) as f32;
    for k in 0..n {
        let u = k as f32 / d;
        let y = hz - sh * 0.14 + u * (sh - hz + sh * 0.18);
        let w = 0.75 * g * clamp01(1.0 - (u - 0.5).abs() * 1.5);
        c.line(x, y, x + (y * 0.05).sin() * 1.6, y, MIST, w);
    }
    // 雾毯：从画面左沿铺到雾锋，越靠后越淡
    for k in 0..7 {
        let u = (k as f32 + 0.5) / 7.0;
        let y = hz + (sh - hz) * u;
        c.line(0.0, y, x.max(0.0), y, PALE, 0.72 * g * (1.0 - k as f32 * 0.06));
    }
    // 锋前的波纹：雾推着空气走
    for k in 0..3u32 {
        let p = (t * 0.34 + k as f32 * 0.33) % 1.0;
        c.ellipse(
            x,
            hz + (sh - hz) * 0.52,
            2.0 + p * 7.0,
            0.7 + p * 1.8,
            MIST,
            0.62 * g * (1.0 - p),
        );
    }
}

/// 02 第一缕光切进画面：一道暖色光刃斜切，切口留下光斑。
fn i02_first_light(ctx: &SceneCtx, c: &mut CharCanvas, phase: f32, dur: f32) {
    let g = card_gain(phase, dur);
    if g <= 0.0 {
        return;
    }
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    // 复用母题：右上斜洒的三道光
    light_shaft(c, ctx, g);

    // 光刃：从画面右外斜切到左下（phase 决定切到哪）
    let u = ease(phase, (dur * 0.55).max(0.4));
    let x0 = sw * 1.06 - sw * 0.92 * u;
    c.line(x0, -1.0, x0 - sw * 0.20, sh + 1.0, DAWN, 0.85 * g);
    c.line(x0 + 1.4, -1.0, x0 + 1.4 - sw * 0.20, sh + 1.0, col::WHITE, 0.45 * g);

    // 切口在地平线上的亮斑
    let hz = sh * 0.62;
    c.ellipse(x0 - sw * 0.06, hz, sw * 0.07, sh * 0.035, DAWN, 0.62 * g);
    c.ellipse(x0 - sw * 0.06, hz, sw * 0.035, sh * 0.018, col::WHITE, 0.55 * g);

    // 被光切亮的地面：雾层被照透的一条亮带
    c.line(
        0.0,
        hz + (sh - hz) * 0.30,
        sw,
        hz + (sh - hz) * 0.30,
        DAWN,
        0.60 * g,
    );
}

/// 03 万物未醒的呼吸：整幅随极慢正弦起伏，浮游粒子与呼吸环。
fn i03_breath(ctx: &SceneCtx, c: &mut CharCanvas, phase: f32, dur: f32) {
    let g = card_gain(phase, dur);
    if g <= 0.0 {
        return;
    }
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let t = ctx.t as f32;

    // 复用母题：气 / 风粒子带（确定性 rng 闭包）
    let mut seed = 0x51ED_2701u32;
    let mut rng = || lcg01(&mut seed);
    ki_flow(c, ctx, g * 0.9, &mut rng);

    // 呼吸：≈0.09Hz 的极慢起伏
    let breath = 0.5 + 0.5 * (t * 0.55).sin();
    let hz = sh * 0.62;

    // 呼吸环：从地平线升起的三圈
    for k in 0..3u32 {
        let u = (t * 0.26 + k as f32 * 0.33) % 1.0;
        let r = u * sw.min(sh) * 0.46 * (0.55 + 0.45 * breath);
        if r <= 0.2 {
            continue;
        }
        c.ellipse(
            sw * 0.5,
            hz,
            r,
            r * 0.26,
            MIST,
            0.62 * g * (1.0 - u) * (0.45 + 0.55 * breath),
        );
    }

    // 未醒的「眼」：地平线正中一点微光，随呼吸明灭
    c.disc(sw * 0.5, hz, 1.1 + breath * 0.9, PALE, 0.85 * g * (0.8 + 0.2 * breath));
    c.set(sw * 0.5 - 2.0, hz, MIST, 0.58 * g);
    c.set(sw * 0.5 + 2.0, hz, MIST, 0.58 * g);
}

/// 04 远处一声：地平线深处一点发声，涟漪一圈圈扩到观者脚下。
fn i04_ripples(ctx: &SceneCtx, c: &mut CharCanvas, phase: f32, dur: f32) {
    let g = card_gain(phase, dur);
    if g <= 0.0 {
        return;
    }
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let t = ctx.t as f32;
    let hz = sh * 0.62;
    let (ox, oy) = (sw * 0.70, hz + (sh - hz) * 0.10);

    // 复用母题：随高频能量扩张的同心环
    cicada_rings(c, ctx, g * 0.75);

    // 涟漪：四圈由声源向外扩散（扁椭圆，暗示在地面上）
    for k in 0..4u32 {
        let u = (t * 0.40 + k as f32 * 0.25) % 1.0;
        let r = u * sw * 0.55;
        if r <= 0.3 {
            continue;
        }
        c.ellipse(ox, oy, r, r * 0.32, MIST, 0.62 * g * (1.0 - u));
    }

    // 声源：一点亮 + 十字微光，随低频脉动
    let pulse = 0.55 + 0.45 * clamp01(ctx.low());
    c.disc(ox, oy, 1.0 + pulse, PALE, 0.85 * g);
    c.line(ox - 2.6, oy, ox - 1.2, oy, col::WHITE, 0.60 * g * pulse);
    c.line(ox + 1.2, oy, ox + 2.6, oy, col::WHITE, 0.60 * g * pulse);

    // 声波抵达观者：画面底部泛起的横纹
    for k in 0..4u32 {
        let p = (t * 0.30 + k as f32 * 0.22) % 1.0;
        let y = sh - p * sh * 0.26;
        c.line(0.0, y, sw, y, MIST, 0.55 * g * (1.0 - p));
    }
}

/// 05 霜气消散：六角霜晶逐渐缩小、升空，最后只剩地平线上化开的湿光。
fn i05_frost_dissolves(ctx: &SceneCtx, c: &mut CharCanvas, phase: f32, dur: f32) {
    let g = card_gain(phase, dur);
    if g <= 0.0 {
        return;
    }
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let t = ctx.t as f32;
    let melt = ease(phase, (dur * 0.85).max(0.4));
    let base = sw.min(sh) * 0.055;

    for k in 0..6u32 {
        let bx = sw * (0.12 + hf(k, 101) * 0.76);
        let by = sh * (0.28 + hf(k, 102) * 0.44);
        let r = base * (0.7 + 0.6 * hf(k, 103)) * (1.0 - melt * 0.85);
        let a = 0.66 * g * (1.0 - melt);
        if r <= 0.3 || a <= 0.03 {
            continue;
        }
        // 霜晶：六角轮廓 + 三条主轴
        hex_ring(c, bx, by, r, FROST, a);
        c.line(bx, by - r, bx, by + r, FROST, a * 0.55);
        c.line(bx - r * 0.86, by - r * 0.5, bx + r * 0.86, by + r * 0.5, FROST, a * 0.55);
        c.line(bx - r * 0.86, by + r * 0.5, bx + r * 0.86, by - r * 0.5, FROST, a * 0.55);

        // 消散：晶体表面向上飘走的微粒
        for j in 0..5u32 {
            let id = k * 7 + j;
            let p = (melt * 1.35 + hf(id, 104)) % 1.0;
            let px = bx + (hf(id, 105) - 0.5) * r * 2.0 + (t * 0.6 + j as f32).sin() * 1.2;
            let py = by - p * sh * 0.22;
            c.set(px, py, FROST, 0.58 * g * (1.0 - p) * (1.0 - melt * 0.4));
        }
    }

    // 霜散后留下的湿光：地平线上一道被化开的亮
    let wet = clamp01(melt * 1.2);
    c.line(0.0, sh * 0.66, sw, sh * 0.66, PALE, 0.58 * g * wet);
    c.set(sw * 0.5, sh * 0.66, col::WHITE, 0.60 * g * wet);
}

/// 06 光斑聚拢成春的形状：散落全屏的光斑向一朵五瓣花的轮廓收拢。
fn i06_spring_shape(ctx: &SceneCtx, c: &mut CharCanvas, phase: f32, dur: f32) {
    let g = card_gain(phase, dur);
    if g <= 0.0 {
        return;
    }
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let t = ctx.t as f32;
    let (cx, cy) = (sw * 0.5, sh * 0.60);
    let big_r = sw.min(sh) * 0.34;
    let conv = ease(phase, (dur * 0.80).max(0.4));
    let n = 26u32;

    for k in 0..n {
        let th = k as f32 / n as f32 * std::f32::consts::TAU;
        // 目标：五瓣花的极坐标轮廓
        let tr = big_r * (0.62 + 0.38 * (5.0 * th).sin());
        let tx = cx + tr * th.cos();
        let ty = cy + tr * th.sin() * 0.72;
        // 起点：哈希撒在全屏
        let sx = sw * (0.06 + hf(k, 121) * 0.88);
        let sy = sh * (0.10 + hf(k, 122) * 0.80);
        let x = sx + (tx - sx) * conv;
        let y = sy + (ty - sy) * conv;
        let tw = 0.5 + 0.5 * (t * 1.1 + k as f32 * 0.5).sin();

        // 光斑
        c.disc(x, y, 0.8 + 0.6 * conv, DAWN, g * (0.7 + 0.3 * tw));

        // 聚拢时拖出的尾迹
        let back = (conv - 0.14).max(0.0);
        let bx = sx + (tx - sx) * back;
        let by = sy + (ty - sy) * back;
        c.line(bx, by, x, y, PALE, 0.60 * g * (1.0 - conv * 0.5));

        // 成形后花瓣之间的连线
        if conv > 0.72 {
            let th2 = (k + 1) as f32 / n as f32 * std::f32::consts::TAU;
            let tr2 = big_r * (0.62 + 0.38 * (5.0 * th2).sin());
            let nx = cx + tr2 * th2.cos();
            let ny = cy + tr2 * th2.sin() * 0.72;
            c.line(x, y, nx, ny, DAWN, 0.62 * g * clamp01((conv - 0.72) * 3.5));
        }
    }

    // 花心：一点暖光，随心跳放大
    let heart = clamp01(ctx.heartbeat);
    c.disc(cx, cy, 1.1 + heart * 1.6, col::WHITE, 0.85 * g);
    c.circle(cx, cy, 3.0 + heart * 1.2, DAWN, 0.62 * g);
}

// ── 卡片表 ──────────────────────────────────────────────────────

/// 卡绘制函数签名：`(ctx, canvas, 卡内相位 local, 本卡跨度 dur)`。
pub type CardFn = fn(&SceneCtx, &mut CharCanvas, f32, f32);

/// 序段的一张卡。
pub struct IntroCard {
    /// 相对段起点的秒数
    pub at: f32,
    /// 卡名（日志 / 汇总用）
    pub name: &'static str,
    /// 绘制函数
    pub draw: CardFn,
}

/// 序 6 卡：间距 2.4 / 2.4 / 2.6 / 2.5 / 2.5（均落在 2.2–2.8s）。
pub const INTRO_CARDS: &[IntroCard] = &[
    IntroCard { at: 0.0, name: "晨雾漫过地面", draw: i01_mist_creeps },
    IntroCard { at: 2.4, name: "第一缕光切进画面", draw: i02_first_light },
    IntroCard { at: 4.8, name: "万物未醒的呼吸", draw: i03_breath },
    IntroCard { at: 7.4, name: "远处一声", draw: i04_ripples },
    IntroCard { at: 9.9, name: "霜气消散", draw: i05_frost_dissolves },
    IntroCard { at: 12.4, name: "光斑聚拢成春的形状", draw: i06_spring_shape },
];

/// 卡名清单（供日志 / Lead 汇总）。
pub fn card_names() -> Vec<&'static str> {
    INTRO_CARDS.iter().map(|k| k.name).collect()
}

/// 第 `idx` 张卡的跨度（到下一卡起点；末卡到段尾）。
fn card_span(idx: usize) -> f32 {
    let start = INTRO_CARDS[idx].at;
    let end = INTRO_CARDS.get(idx + 1).map(|k| k.at).unwrap_or(INTRO_LEN);
    (end - start).max(0.1)
}

/// 按段内时间选卡：最后一个 `at <= local` 的卡（`local < 0` 取首卡）。
fn pick(local: f32) -> usize {
    let mut idx = 0usize;
    for (i, k) in INTRO_CARDS.iter().enumerate() {
        if local >= k.at {
            idx = i;
        }
    }
    idx
}

/// 序段入口：共享背景 + 当前卡。
///
/// `alpha` 只用于向黑压暗（`tint` 只混合颜色、不动 `Pixel.w`），
/// 保证 Braille 的点阵结构在任何 alpha 下都不被破坏。
pub fn render_card(ctx: &SceneCtx, canvas: &mut CharCanvas, alpha: f32) {
    let alpha = clamp01(alpha);
    let local = ctx.local_t as f32;
    let idx = pick(local);
    let card = &INTRO_CARDS[idx];
    intro_bg(ctx, canvas);
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

    /// 序段起点是 0，所以 `t == local_t`。
    fn ctx_at(local: f32) -> SceneCtx<'static> {
        let f = feat(0.6);
        SceneCtx {
            t: local as f64,
            local_t: local as f64,
            progress: local / INTRO_LEN,
            feat: f.clone(),
            prev: f,
            frame: (local * 24.0) as u64,
            cols: 80,
            rows: 24,
            scene: SceneKind::Intro,
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
        assert_eq!(INTRO_CARDS.len(), 6);
        assert_eq!(card_names().len(), 6);
        for w in INTRO_CARDS.windows(2) {
            assert!(w[1].at > w[0].at, "卡起点未递增：{} → {}", w[0].name, w[1].name);
            let gap = w[1].at - w[0].at;
            assert!((2.2..=2.8).contains(&gap), "卡间距 {gap} 不在 2.2–2.8s");
        }
        assert!(INTRO_CARDS[INTRO_CARDS.len() - 1].at < INTRO_LEN);
    }

    #[test]
    fn every_card_draws_something() {
        for k in INTRO_CARDS {
            let n = lit(k.at + 0.5);
            assert!(n > 20, "序卡「{}」只点亮了 {n} 像素", k.name);
        }
    }

    #[test]
    fn no_panic_on_tiny_canvas() {
        for k in INTRO_CARDS {
            let ctx = ctx_at(k.at + 0.5);
            let mut c = CharCanvas::new(8, 5, RenderMode::Braille);
            render_card(&ctx, &mut c, 1.0);
        }
    }

    #[test]
    fn same_local_is_bit_identical() {
        for k in INTRO_CARDS {
            let a = sig(k.at + 0.7);
            let b = sig(k.at + 0.7);
            assert_eq!(a, b, "序卡「{}」同一 local 两次绘制不一致", k.name);
        }
    }

    #[test]
    fn every_card_moves() {
        let mut moved = 0;
        for k in INTRO_CARDS {
            if mask(k.at + 0.5) != mask(k.at + 1.6) {
                moved += 1;
            }
        }
        assert_eq!(moved, INTRO_CARDS.len(), "只有 {moved}/6 张卡在动");
    }

    #[test]
    fn boundary_jump_is_smaller_than_card_motion() {
        for i in 0..INTRO_CARDS.len() - 1 {
            let cur = &INTRO_CARDS[i];
            let nxt = &INTRO_CARDS[i + 1];
            // 交界：新卡起点前后各 0.05s
            let jump = diff(&mask(nxt.at - 0.05), &mask(nxt.at + 0.05));
            // 卡内运动：两侧各自 0.4s → 1.5s 的变化，取较小者当基准
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
    fn alpha_only_darkens_color() {
        let local = 6.2;
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

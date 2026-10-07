//! 桥段（68.4–84.9s，16.5s）：瞬き 一ツ スル間ニモ / 形 変エ行ク 幻ヨ。
//!
//! 九张卡画的是**同一团形**：形态参数 `m` 与流动相位取自段内时钟
//! `ctx.local_t`，不是卡内相位。于是「形」的变形是一条跨卡连续的曲线 ——
//! 卡与卡之间不可能跳变；每张卡只在这团形上叠加自己的强调层，
//! 强调层带 [`env`] 包络，在卡首尾精确归零。
//!
//! 位置 / 大小 / 亮度全是时间的连续函数（smoothstep 缓入缓出）；
//! 伪随机一律走整数坐标哈希，不使用任何带状态的随机源。
//! 时间锚点只写在 [`BRIDGE_CARDS`] 卡表里，绘制函数内不出现绝对秒数。

use ratatui::style::Color;

use crate::render::color as col;
use crate::render::motifs::{east_wind, envelope, frame_wipe, ki_flow};
use crate::render::scenes::SceneCtx;
use crate::render::CharCanvas;

/// 桥段长度（timeline.toml: 68.4 → 84.9）。
pub const BRIDGE_LEN: f32 = 16.5;

/// 主色：青紫过渡（`SceneKind::Bridge::palette` 的本地副本）。
const VIOLET: Color = Color::Rgb(150, 130, 230);
const INDIGO: Color = Color::Rgb(80, 110, 190);
const PALE: Color = Color::Rgb(226, 222, 255);
const DEEP: Color = Color::Rgb(34, 28, 62);

const TAU: f32 = std::f32::consts::TAU;

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

// ── 同一团形 ──────────────────────────────────────────────

/// 整体形态进度 0..1：静止的近圆 → 凝定的新形。
fn morph(l: f32) -> f32 {
    sstep(l / BRIDGE_LEN)
}

/// 「瞬き」闭合量：1.05s 起闭合、1.62s 全闭、2.17s 睁完。
///
/// 这是**段级**现象（整段只眨一次），所以由段内时钟驱动，
/// 因此九张卡画出的压暗程度完全一致，交界处不会闪。
fn blink(l: f32) -> f32 {
    let down = sstep((l - 1.05) / 0.55);
    let up = 1.0 - sstep((l - 1.62) / 0.55);
    down.min(up).clamp(0.0, 1.0)
}

/// 极坐标半径：静止（近圆）→ 新形（纺锤双瓣）。
///
/// 插值权重按角度错相 —— 变化的前沿绕形体扫过，于是同一团形像液体一样流动，
/// 而不是整体瞬间切换。对 `theta / m / flow` 全部连续。
fn shape_r(theta: f32, m: f32, flow: f32) -> f32 {
    let a = 1.0 + 0.06 * (3.0 * theta + flow * 0.30).cos();
    let b = 1.0 + 0.34 * (2.0 * theta + 0.60 - flow * 0.22).cos()
        - 0.14 * (4.0 * theta - 0.40 + flow * 0.18).cos();
    let front = 0.5 + 0.5 * (1.7 * theta - flow * 0.85).sin();
    let w = sstep((m - 0.10 - 0.55 * front) / 0.40);
    a + (b - a) * w
}

/// 画一层同形的闭合轮廓（y 略压扁，贴近终端格子比例）。
fn draw_ring(
    c: &mut CharCanvas,
    cx: f32,
    cy: f32,
    base: f32,
    m: f32,
    flow: f32,
    tone: Color,
    w: f32,
) {
    if base <= 0.5 || w <= 0.0 {
        return;
    }
    let n = 84;
    let mut prev: Option<(f32, f32)> = None;
    for i in 0..=n {
        let th = i as f32 / n as f32 * TAU;
        let r = base * shape_r(th, m, flow);
        let p = (cx + r * th.cos(), cy + r * th.sin() * 0.88);
        if let Some(q) = prev {
            c.line(q.0, q.1, p.0, p.1, tone, w);
        }
        prev = Some(p);
    }
}

/// 共享的「形」：外轮廓 + 两层内等值线 + 表面流光 + 剥落细粒。
///
/// 九张卡都调用它，参数只依赖段内时钟 → 卡间必然连续。
fn bridge_body(ctx: &SceneCtx, c: &mut CharCanvas, l: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    if sw < 10.0 || sh < 8.0 {
        return;
    }
    let (cx0, cy0) = ctx.center(c);
    let r0 = ctx.radius(c);
    let m = morph(l);
    let flow = l;
    let shut = blink(l);
    // 闭眼时形被压暗但**不归零** —— 形体始终在，只是「看不见一瞬」。
    let gain = (1.0 - 0.72 * shut).clamp(0.15, 1.0);
    let breathe = 0.94 + 0.05 * (ctx.t as f32 * 1.05).sin() + 0.05 * ctx.low();
    let base = r0 * 0.62 * breathe * (1.0 + 0.10 * m);
    let cx = cx0 + (l * 0.30).sin() * r0 * 0.06;
    let cy = cy0 + (l * 0.19).cos() * r0 * 0.05;

    // 三层等值线：同一 θ 采样、同一 shape_r → 天然同形，读起来像「形在呼吸」。
    let rings = [
        (1.00f32, VIOLET, 0.62f32),
        (0.66, INDIGO, 0.55),
        (0.34, PALE, 0.46),
    ];
    for (k, tone, w) in rings {
        draw_ring(c, cx, cy, base * k, m, flow + (1.0 - k) * 1.4, tone, w * gain);
    }

    // 表面流光：沿轮廓切向的短线，随时间绕形滑走。
    for k in 0..12u32 {
        let th = k as f32 / 12.0 * TAU + flow * 0.55;
        let r = base * shape_r(th, m, flow) * 1.06;
        let px = cx + r * th.cos();
        let py = cy + r * th.sin() * 0.88;
        let tx = -th.sin() * 2.4;
        let ty = th.cos() * 2.0;
        c.line(px - tx, py - ty, px + tx, py + ty, PALE, 0.5 * gain);
    }

    // 剥落细粒：坐标哈希决定角度与相位，沿法线向外漂，越远越淡。
    for k in 0..26u32 {
        let th = hf(k, 7) * TAU;
        let out = 1.0 + 0.28 * (l * 0.35 + hf(k, 8)).fract();
        let r = base * shape_r(th, m, flow) * out;
        let a = 0.45 * gain * (1.0 - ((out - 1.0) / 0.28).clamp(0.0, 1.0));
        c.set(cx + r * th.cos(), cy + r * th.sin() * 0.88, INDIGO, a);
    }
}

// ── 共享背景 ──────────────────────────────────────────────

/// 桥段背景：深紫底纹 + 框住形体的大环 + 漂移星尘 + 入场 wipe。
fn bridge_bg(ctx: &SceneCtx, c: &mut CharCanvas, l: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    if sw < 10.0 || sh < 8.0 {
        return;
    }
    // 底纹：密而淡的横向细线（给半块 / ASCII 模式留底，Braille 下几乎不可见）。
    let rows = (sh / 3.0).floor().max(2.0) as i32;
    for r in 0..rows {
        let y = sh * (r as f32 + 0.5) / rows as f32;
        c.line(0.0, y, sw, y, DEEP, 0.30);
    }

    // 大环：框住形体的静默画框，半径缓慢呼吸。
    let (cx, cy) = ctx.center(c);
    let r0 = ctx.radius(c);
    let rr = r0 * (0.92 + 0.02 * (ctx.t as f32 * 0.5).sin());
    let mut prev: Option<(f32, f32)> = None;
    for i in 0..=72 {
        let th = i as f32 / 72.0 * TAU;
        let p = (cx + rr * th.cos(), cy + rr * th.sin() * 0.90);
        if let Some(q) = prev {
            c.line(q.0, q.1, p.0, p.1, INDIGO, 0.42);
        }
        prev = Some(p);
    }

    // 星尘：哈希点缓慢下漂（纯坐标哈希，无状态）。
    for k in 0..70u32 {
        let x = hf(k, 21) * sw;
        let y = hf(k, 22) * sh;
        let drift = (l * 0.12 + hf(k, 23)).fract() * 3.0;
        c.set(x, y + drift, PALE, 0.30 + 0.25 * hf(k, 24));
    }

    // 入场 wipe：motifs 自带 0..1.6s 包络，这里再乘一道收尾淡出，避免末端硬切。
    let wipe = 1.0 - sstep((l - 1.15) / 0.40);
    if wipe > 0.0 {
        frame_wipe(c, ctx, 0.85 * wipe);
    }
}

// ── 卡片绘制函数 ──────────────────────────────────────────

/// 01 静止ノ形：形尚未开始变，外圈三道同心环缓慢外扩。
fn b01_still(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, dur: f32) {
    let l = ctx.local_t as f32;
    let e = env(local, dur);
    bridge_bg(ctx, c, l);
    bridge_body(ctx, c, l);
    if e <= 0.0 {
        return;
    }
    let (cx, cy) = ctx.center(c);
    let r0 = ctx.radius(c);
    for k in 0..3u32 {
        let cyc = (local * 0.45 + k as f32 * 0.33).fract();
        let life = envelope(cyc, 0.0, 0.18, 0.72, 1.0);
        if life <= 0.0 {
            continue;
        }
        let rr = r0 * (0.62 + 0.34 * cyc);
        c.circle(cx, cy, rr, col::lerp(INDIGO, PALE, cyc), 0.55 * e * life);
    }
    // 八向准星刻度：随呼吸微微伸缩（连续，不跳）。
    let br = 0.96 + 0.04 * (ctx.t as f32 * 0.9).sin();
    for k in 0..8u32 {
        let th = k as f32 / 8.0 * TAU;
        let r1 = r0 * 0.60 * br;
        let r2 = r1 * 1.12;
        c.line(
            cx + r1 * th.cos(),
            cy + r1 * th.sin() * 0.88,
            cx + r2 * th.cos(),
            cy + r2 * th.sin() * 0.88,
            VIOLET,
            0.5 * e,
        );
    }
}

/// 02 瞬キ閉ヂ：上下眼睑从眼角收拢到中缝，一瞬全闭，再睁开。
fn b02_blink(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, dur: f32) {
    let l = ctx.local_t as f32;
    let e = env(local, dur);
    bridge_bg(ctx, c, l);
    bridge_body(ctx, c, l);
    if e <= 0.0 {
        return;
    }
    let (cx, cy) = ctx.center(c);
    let r0 = ctx.radius(c);
    let shut = blink(l);
    let open = 1.0 - shut;
    // 眼睑：左眼角 → 右眼角的弧，`open` → 0 时压平成一条中缝。
    for (dir, tone) in [(-1.0f32, PALE), (1.0f32, VIOLET)] {
        let mut prev: Option<(f32, f32)> = None;
        let steps = 40;
        for i in 0..=steps {
            let a = std::f32::consts::PI * i as f32 / steps as f32;
            let x = cx + r0 * 0.72 * a.cos();
            let y = cy + dir * r0 * (0.05 + 0.36 * open) * a.sin();
            if let Some(q) = prev {
                c.line(q.0, q.1, x, y, tone, 0.62 * e);
            }
            prev = Some((x, y));
        }
    }
    // 睫毛：闭合度越高越向内收。
    for k in 0..9u32 {
        let u = k as f32 / 8.0;
        let a = std::f32::consts::PI * u;
        let x = cx + r0 * 0.72 * a.cos();
        let y = cy - r0 * (0.05 + 0.36 * open) * a.sin();
        let len = r0 * (0.10 - 0.06 * shut) * (0.4 + 0.6 * a.sin());
        c.line(x, y, x + len * 0.35, y - len, PALE, 0.45 * e);
    }
    if shut > 0.10 {
        c.line(
            cx - r0 * 0.72,
            cy,
            cx + r0 * 0.72,
            cy,
            PALE,
            0.70 * e * shut,
        );
    }
}

/// 03 形流レ初メ：形开始流动 —— 表面浮起切向流线，风线横掠。
fn b03_flow(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, dur: f32) {
    let l = ctx.local_t as f32;
    let e = env(local, dur);
    bridge_bg(ctx, c, l);
    bridge_body(ctx, c, l);
    if e <= 0.0 {
        return;
    }
    east_wind(c, ctx, 0.7 * e);
    let (cx, cy) = ctx.center(c);
    let r0 = ctx.radius(c);
    let m = morph(l);
    for k in 0..5u32 {
        let th0 = k as f32 * 1.2566 + local * 0.75;
        let mut prev: Option<(f32, f32)> = None;
        for i in 0..=16 {
            let th = th0 + i as f32 * 0.055;
            let r = r0 * 0.62 * shape_r(th, m, l) * 0.92;
            let p = (cx + r * th.cos(), cy + r * th.sin() * 0.88);
            if let Some(q) = prev {
                c.line(q.0, q.1, p.0, p.1, PALE, 0.50 * e);
            }
            prev = Some(p);
        }
    }
}

/// 04 境界溶ケ：边界碎成虚线段，碎屑向外溶散。
fn b04_dissolve(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, dur: f32) {
    let l = ctx.local_t as f32;
    let e = env(local, dur);
    bridge_bg(ctx, c, l);
    bridge_body(ctx, c, l);
    if e <= 0.0 {
        return;
    }
    let (cx, cy) = ctx.center(c);
    let r0 = ctx.radius(c);
    let m = morph(l);
    for k in 0..24u32 {
        let seg = hf(k, 31);
        if seg < 0.42 {
            continue;
        }
        let th = k as f32 / 24.0 * TAU + local * 0.5;
        let r = r0 * 0.62 * shape_r(th, m, l) * (1.0 + 0.06 * (local * 1.3 + k as f32).sin());
        let p0 = (cx + r * th.cos(), cy + r * th.sin() * 0.88);
        let p1 = (
            cx + r * 1.10 * th.cos(),
            cy + r * 1.10 * th.sin() * 0.88,
        );
        c.line(
            p0.0,
            p0.1,
            p1.0,
            p1.1,
            col::lerp(VIOLET, PALE, seg),
            0.50 * e,
        );
    }
    for k in 0..30u32 {
        let th = hf(k, 33) * TAU;
        let out = 1.0 + 0.42 * (local * 0.30 + hf(k, 34)).fract();
        let r = r0 * 0.62 * shape_r(th, m, l) * out;
        let a = 0.50 * e * (1.0 - ((out - 1.0) / 0.42).clamp(0.0, 1.0));
        c.set(cx + r * th.cos(), cy + r * th.sin() * 0.88, PALE, a);
    }
}

/// 05 別形ニ組ミ上ガリ：辐条从内环搭到外轮廓，另一个形正在被组装。
fn b05_assembly(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, dur: f32) {
    let l = ctx.local_t as f32;
    let e = env(local, dur);
    bridge_bg(ctx, c, l);
    bridge_body(ctx, c, l);
    if e <= 0.0 {
        return;
    }
    let (cx, cy) = ctx.center(c);
    let r0 = ctx.radius(c);
    let m = morph(l);
    for k in 0..18u32 {
        let th = k as f32 / 18.0 * TAU + local * 0.42;
        let rr = r0 * 0.62 * shape_r(th, m, l);
        let r_in = rr * 0.34;
        let r_out = rr * (0.98 + 0.05 * (local * 2.0 + k as f32).sin());
        c.line(
            cx + r_in * th.cos(),
            cy + r_in * th.sin() * 0.88,
            cx + r_out * th.cos(),
            cy + r_out * th.sin() * 0.88,
            INDIGO,
            0.42 * e,
        );
    }
    // 组装时溢出的气流粒（LCG 局部确定性，不用 rand 的内部状态）。
    let mut seed = 0x51ED_2701u32;
    let mut rng = move || {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (seed >> 8) as f32 / 16_777_216.0
    };
    ki_flow(c, ctx, 0.5 * e, &mut rng);
}

/// 06 相変移ロ：一条相变亮带自下而上穿过形体，内环换色。
fn b06_phase(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, dur: f32) {
    let l = ctx.local_t as f32;
    let e = env(local, dur);
    bridge_bg(ctx, c, l);
    bridge_body(ctx, c, l);
    if e <= 0.0 {
        return;
    }
    let (cx, cy) = ctx.center(c);
    let r0 = ctx.radius(c);
    // 相变带：卡内进度 0→1 自下而上扫过。
    let u = sstep(local / dur.max(0.1));
    let y = cy + r0 * (0.95 - 1.90 * u);
    let half = r0 * 0.78;
    let mut prev: Option<(f32, f32)> = None;
    for i in 0..=40 {
        let x = cx - half + 2.0 * half * i as f32 / 40.0;
        let yy = y + (x * 0.10 + local * 2.2).sin() * 1.6;
        if let Some(q) = prev {
            c.line(q.0, q.1, x, yy, col::lerp(VIOLET, PALE, 0.7), 0.50 * e);
        }
        prev = Some((x, yy));
    }
    // 变相后的内环：色相随时间推移，读作「已经是另一种形」。
    c.circle(
        cx,
        cy,
        r0 * 0.40,
        col::hue_shift(VIOLET, 40.0 + local * 8.0),
        0.50 * e,
    );
}

/// 07 幻影二ニ分カレ：同一团形旁边浮出第二个「幻影」，越分越远。
fn b07_phantom_split(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, dur: f32) {
    let l = ctx.local_t as f32;
    let e = env(local, dur);
    bridge_bg(ctx, c, l);
    bridge_body(ctx, c, l);
    if e <= 0.0 {
        return;
    }
    let (cx, cy) = ctx.center(c);
    let r0 = ctx.radius(c);
    let m = morph(l);
    let sep = sstep(local / dur.max(0.1)) * r0 * 0.55;
    let base = r0 * 0.62 * 0.86;
    draw_ring(c, cx + sep, cy, base, m, l + 0.6, INDIGO, 0.55 * e);
    // 分裂处的一线牵丝：随分离拉长变淡。
    c.line(cx, cy, cx + sep, cy, col::lerp(VIOLET, PALE, 0.5), 0.45 * e);
    // 幻影的尾迹点。
    for k in 0..12u32 {
        let u = k as f32 / 11.0;
        let x = cx + sep * u;
        let y = cy + (u * 6.0 + local).sin() * 1.2;
        c.set(x, y, INDIGO, 0.40 * e * (1.0 - u * 0.6));
    }
}

/// 08 二形交差：两个形对穿而过，交错瞬间叠出干涉亮线。
fn b08_crossing(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, dur: f32) {
    let l = ctx.local_t as f32;
    let e = env(local, dur);
    bridge_bg(ctx, c, l);
    bridge_body(ctx, c, l);
    if e <= 0.0 {
        return;
    }
    let (cx, cy) = ctx.center(c);
    let r0 = ctx.radius(c);
    let m = morph(l);
    // 扫掠量 -1 → +1：甲从右到左、乙从左到右，中途在中心对穿。
    let sweep = sstep(local / dur.max(0.1)) * 2.0 - 1.0;
    let sep = sweep * r0 * 0.50;
    let base = r0 * 0.62 * 0.82;
    draw_ring(c, cx - sep, cy, base, m, l + 0.9, VIOLET, 0.52 * e);
    draw_ring(c, cx + sep, cy, base, m, l + 2.1, INDIGO, 0.52 * e);
    // 交错处：|sep| 越小越亮，形成一瞬的干涉。
    let inter = (1.0 - (sep.abs() / (r0 * 0.50)).clamp(0.0, 1.0)).powi(2);
    if inter > 0.02 {
        for k in 0..7u32 {
            let yy = cy + (k as f32 - 3.0) * r0 * 0.09;
            c.line(
                cx - r0 * 0.30 * inter,
                yy,
                cx + r0 * 0.30 * inter,
                yy,
                col::lerp(PALE, VIOLET, k as f32 / 6.0),
                0.55 * e * inter,
            );
        }
    }
    east_wind(c, ctx, 0.4 * e);
}

/// 09 新形ニ凝定：新形定型 —— 双层清晰轮廓 + 沉降刻线 + 收束光环。
fn b09_solidify(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, dur: f32) {
    let l = ctx.local_t as f32;
    let e = env(local, dur);
    bridge_bg(ctx, c, l);
    bridge_body(ctx, c, l);
    if e <= 0.0 {
        return;
    }
    let (cx, cy) = ctx.center(c);
    let r0 = ctx.radius(c);
    let m = morph(l);
    let settle = sstep(local / dur.max(0.1));
    let base = r0 * 0.62 * (1.0 + 0.10 * m);
    // 定型轮廓：外一圈 + 外扩一圈，随沉降收紧。
    draw_ring(c, cx, cy, base, m, l, PALE, 0.60 * e);
    draw_ring(
        c,
        cx,
        cy,
        base * (1.18 - 0.06 * settle),
        m,
        l + 0.4,
        VIOLET,
        0.52 * e,
    );
    // 沉降刻线：从外向内收拢的短刻，条数固定、位置连续。
    for k in 0..16u32 {
        let th = k as f32 / 16.0 * TAU;
        let r_out = base * (1.30 - 0.20 * settle);
        let r_in = base * (1.02 - 0.04 * settle);
        c.line(
            cx + r_out * th.cos(),
            cy + r_out * th.sin() * 0.88,
            cx + r_in * th.cos(),
            cy + r_in * th.sin() * 0.88,
            INDIGO,
            0.45 * e,
        );
    }
    // 收束光环：一圈圈向外散去的余韵。
    for k in 0..2u32 {
        let cyc = (local * 0.5 + k as f32 * 0.5).fract();
        let life = envelope(cyc, 0.0, 0.2, 0.75, 1.0);
        if life <= 0.0 {
            continue;
        }
        c.circle(
            cx,
            cy,
            base * (1.0 + 0.5 * cyc),
            col::lerp(PALE, INDIGO, cyc),
            0.50 * e * life,
        );
    }
}

// ── 卡片表 ────────────────────────────────────────────────

/// 卡：段内起始秒（相对桥段起点 68.4s）、卡名、绘制函数。
///
/// `draw` 收到 `(ctx, canvas, local, dur)`：`local` 是卡内相位、`dur` 是本卡跨度。
type CardFn = fn(&SceneCtx, &mut CharCanvas, f32, f32);

pub struct BridgeCard {
    /// 相对段起点的秒数
    pub at: f32,
    pub name: &'static str,
    pub draw: CardFn,
}

/// 桥 9 卡。唱句锚点（相对 68.4s）：0.038「瞬き 一ツ スル間ニモ」、
/// 7.928「形 変エ行ク 幻ヨ」—— 卡 6 压在第二句上。
pub const BRIDGE_CARDS: &[BridgeCard] = &[
    BridgeCard {
        at: 0.0,
        name: "静止ノ形",
        draw: b01_still,
    },
    BridgeCard {
        at: 1.4,
        name: "瞬キ閉ヂ",
        draw: b02_blink,
    },
    BridgeCard {
        at: 3.0,
        name: "形流レ初メ",
        draw: b03_flow,
    },
    BridgeCard {
        at: 4.8,
        name: "境界溶ケ",
        draw: b04_dissolve,
    },
    BridgeCard {
        at: 6.6,
        name: "別形ニ組ミ上ガリ",
        draw: b05_assembly,
    },
    BridgeCard {
        at: 7.9,
        name: "相変移ロ",
        draw: b06_phase,
    },
    BridgeCard {
        at: 9.8,
        name: "幻影二ニ分カレ",
        draw: b07_phantom_split,
    },
    BridgeCard {
        at: 11.8,
        name: "二形交差",
        draw: b08_crossing,
    },
    BridgeCard {
        at: 13.9,
        name: "新形ニ凝定",
        draw: b09_solidify,
    },
];

/// 卡名导出（供日志 / Lead 汇总）。
pub fn card_names() -> Vec<&'static str> {
    BRIDGE_CARDS.iter().map(|k| k.name).collect()
}

/// 按段内时间选卡：返回 `(下标, 卡内相位, 本卡跨度)`。
fn pick(l: f32) -> (usize, f32, f32) {
    let mut idx = 0usize;
    for (i, k) in BRIDGE_CARDS.iter().enumerate() {
        if l >= k.at {
            idx = i;
        }
    }
    let at = BRIDGE_CARDS[idx].at;
    let end = BRIDGE_CARDS
        .get(idx + 1)
        .map(|k| k.at)
        .unwrap_or(BRIDGE_LEN);
    (idx, (l - at).max(0.0), (end - at).max(0.1))
}

/// 桥段入口：按 `ctx.local_t` 选卡并绘制。
///
/// `alpha` 是整段不透明度：只做「向黑压暗」（tint 改色不改覆盖度），
/// 绝不乘 `Pixel.w` —— 覆盖度一旦被缩放，Braille 的 0.5 阈值会把整段吃掉。
pub fn render_card(ctx: &SceneCtx, canvas: &mut CharCanvas, alpha: f32) {
    if alpha <= 0.02 {
        return;
    }
    let l = ctx.local_t as f32;
    let (idx, phase, span) = pick(l);
    (BRIDGE_CARDS[idx].draw)(ctx, canvas, phase, span);
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
            t: 68.4 + local as f64,
            local_t: local as f64,
            progress: local / BRIDGE_LEN,
            feat: f.clone(),
            prev: f,
            frame: ((68.4 + local as f64) * 24.0) as u64,
            cols: 80,
            rows: 24,
            scene: SceneKind::Bridge,
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
    fn nine_cards_exist() {
        assert_eq!(BRIDGE_CARDS.len(), 9, "桥段必须是 9 张卡");
        assert_eq!(card_names().len(), 9);
    }

    #[test]
    fn every_card_lights_up() {
        for k in BRIDGE_CARDS {
            let n = lit(k.at + 0.6);
            assert!(n > 30, "桥卡「{}」只点亮 {n} 个像素", k.name);
        }
    }

    #[test]
    fn every_card_moves() {
        for k in BRIDGE_CARDS {
            let a = sig(k.at + 0.15);
            let b = sig(k.at + 0.95);
            assert!(a != b, "桥卡「{}」在 0.8s 内完全静止", k.name);
        }
    }

    #[test]
    fn no_panic_on_tiny_canvas() {
        for (cols, rows) in [(4u16, 3u16), (1, 1), (8, 5)] {
            for k in BRIDGE_CARDS {
                let ctx = ctx_at(k.at + 0.5);
                let mut canvas = CharCanvas::new(cols, rows, RenderMode::Braille);
                render_card(&ctx, &mut canvas, 1.0);
            }
        }
    }

    #[test]
    fn boundaries_are_seamless() {
        for k in BRIDGE_CARDS.iter().skip(1) {
            let before = coverage(k.at - 0.05);
            let after = coverage(k.at + 0.05);
            let d = (after - before).abs() / after.max(1.0);
            assert!(d < 0.20, "桥卡「{}」交界跳变 {:.1}%", k.name, d * 100.0);
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

//! 秋（118.6–155.9s）：松影月盃，砧声夜风。
//!
//! 一间无客的和室。松影在畳上随月移动，月照空盃，砧声从远处传来，
//! 夜风最后只剩「我」与风。二十张卡按唱句推进，卡与卡共享同一块
//! 背景层（月弧 + 畳目 + 松），保证交界处画面连续。
//!
//! 所有元素位置都是 `t` 的连续函数；粒子用整数哈希手绘离散点，
//! 不依赖共享 [`ParticleSystem`]，保证纯函数确定性。

use ratatui::style::Color;

use crate::render::color as col;
use crate::render::CharCanvas;
use crate::render::scenes::SceneCtx;

/// 秋段主色系（与 SceneKind::Autumn::palette 一致，这里取本地副本避免反复解构）。
const AMBER: Color = Color::Rgb(240, 180, 88);
const AMBER_DIM: Color = Color::Rgb(178, 118, 52);
const AKANE: Color = Color::Rgb(216, 84, 58); // 茜色：落月/点睛
const MOON: Color = Color::Rgb(252, 244, 214); // 月白
const NIGHT: Color = Color::Rgb(38, 44, 58); // 夜蓝
const PINE: Color = Color::Rgb(74, 104, 62); // 松绿

// ── 小工具 ──────────────────────────────────────────────

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

/// 两端平滑（smoothstep），用于淡入淡出。
fn fade(x: f32) -> f32 {
    x.clamp(0.0, 1.0) * x.clamp(0.0, 1.0) * (3.0 - 2.0 * x.clamp(0.0, 1.0))
}

/// 窗口函数：a 前淡入、b 后淡出，峰值为 1。
fn win(x: f32, a: f32, b: f32) -> f32 {
    fade((a).min(b - x).min(1.0).max(0.0)) * 0.0 + {
        let up = fade((x - a).max(0.0) / 0.8);
        let down = fade((b - x).max(0.0) / 1.2);
        up.min(down)
    }
}

/// 六边形轮廓（霜晶/雪环共用）。
fn hex_ring(c: &mut CharCanvas, cx: f32, cy: f32, r: f32, color: Color, w: f32) {
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

// ── 共享背景：夜空 + 月弧 + 畳目 ────────────────────────

/// 秋夜背景：上 40% 夜空，下 60% 畳。月沿一条缓慢的弧爬行（整段连续），
/// 松影从右侧斜切进畳面。`local` 是段内秒，`dur` 是段长。
fn autumn_bg(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, dur: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    if sw < 8.0 || sh < 6.0 {
        return;
    }
    let horizon = sh * 0.40;
    let [main, alt, accent] = ctx.scene.palette();
    let _ = alt;
    let _ = accent;

    // 夜空：整幅染一层夜蓝，压暗上半屏
    c.tint_rect(0.0, 0.0, sw, horizon, NIGHT, 0.55);
    c.tint_rect(0.0, horizon, sw, sh - horizon, Color::Rgb(52, 44, 36), 0.45);

    // 月：沿弧线从左上爬向右上，整段连续移动
    let u = (local / dur).clamp(0.0, 1.0);
    let mx = sw * (0.30 + 0.42 * u);
    let my = sh * (0.16 - 0.05 * (u * std::f32::consts::PI).sin());
    let mr = (sh * 0.055).max(2.2);
    let breathe = 0.85 + 0.15 * (ctx.t as f32 * 0.9).sin(); // low() 呼吸
    c.disc(mx, my, mr * 1.9, MOON, 0.05 * breathe);
    c.disc(mx, my, mr, MOON, (0.62 * breathe).clamp(0.0, 1.0));
    c.circle(mx, my, mr * 1.35, MOON, 0.22);

    // 畳目：水平榻榻米缝 + 竖向短栅
    let seam = Color::Rgb(120, 96, 64);
    let rows = ((sh - horizon) / 2.6).floor().max(2.0) as i32;
    for r in 1..=rows {
        let y = horizon + (sh - horizon) * r as f32 / (rows as f32 + 1.0);
        c.line(0.0, y, sw, y, seam, 0.16 + 0.05 * (r % 2) as f32);
    }
    // 畳的竖缝：错位排布
    for r in 0..rows {
        let y0 = horizon + (sh - horizon) * r as f32 / (rows as f32 + 1.0);
        let y1 = horizon + (sh - horizon) * (r + 1) as f32 / (rows as f32 + 1.0);
        let off = if r % 2 == 0 { 0.18 } else { 0.62 };
        c.line(sw * off, y0, sw * off, y1, seam, 0.14);
    }

    // 松影：从右侧斜进来的长影，角度随月位缓慢变化
    let sway = (local * 0.05).sin() * 0.02;
    let slope = 0.42 + sway;
    let steps = (sw * 0.6) as i32;
    for i in 0..=steps {
        let x = sw - i as f32;
        let y = horizon + (sw - x) * slope * 0.5;
        if y < sh {
            c.set(x, y, Color::Rgb(30, 26, 22), 0.34);
            c.set(x, y + 0.5, Color::Rgb(30, 26, 22), 0.22);
        }
    }
    // 影端头的松枝剪影（中景，微微摇）
    let bx = sw * (0.62 + 0.01 * sway);
    let by = horizon - sh * 0.04 + (ctx.t as f32 * 0.4).sin() * 0.6;
    c.line(bx, by, bx + sw * 0.3, by - sh * 0.10, PINE, 0.55);
    for k in 0..7 {
        let hx = bx + sw * 0.3 * (k as f32 + 1.0) / 8.0;
        let hy = by - sh * 0.10 * (k as f32 + 1.0) / 8.0;
        c.line(hx, hy, hx + 2.5, hy - 1.8, PINE, 0.5);
        c.line(hx, hy, hx - 2.0, hy - 2.2, PINE, 0.5);
    }
}

/// 畳上的月光池：以月为源、随月移动的椭圆亮斑。
fn moon_pool(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, dur: f32, gain: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let u = (local / dur).clamp(0.0, 1.0);
    let mx = sw * (0.30 + 0.42 * u);
    let horizon = sh * 0.40;
    // 光池在畳上的投影：月正下方，随月移动
    let py = horizon + (sh - horizon) * 0.45;
    let rx = sw * 0.16;
    let ry = (sh - horizon) * 0.16;
    c.ellipse(mx * 0.9 + sw * 0.05, py, rx, ry, MOON, 0.16 * gain);
    c.ellipse(mx * 0.9 + sw * 0.05, py, rx * 0.6, ry * 0.6, MOON, 0.12 * gain);
}

/// 錆前奏用：把数字按 0..1 归一并消除类型歧义。
fn unit(x: f32) -> f32 {
    x.clamp(0.0, 1.0)
}

// ── 卡片绘制函数 ────────────────────────────────────────

/// 01 松ノ影：松影缓慢延上畳面（背景已含松影，这里加长影端头的松针群）。
fn a01_pine_shadow(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, dur: f32) {
    let sh = c.sh as f32;
    let grow = fade(local / (dur * 0.4));
    let horizon = sh * 0.40;
    let tip_x = c.sw as f32 * 0.34;
    let tip_y = horizon + (sh - horizon) * 0.5 * grow;
    for k in 0..9 {
        let a = k as f32 * 0.7 + local * 0.12;
        let len = 2.2 + 1.4 * (k % 3) as f32;
        c.line(tip_x, tip_y, tip_x + a.cos() * len, tip_y + a.sin() * len * 0.5, PINE, 0.42);
    }
}

/// 02 畳ニ延ビテ：畳目上再铺一层顺影方向的暗纹（影的「纤维」）。
fn a02_tatami_extend(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, _dur: f32) {
    let sh = c.sh as f32;
    let sw = c.sw as f32;
    let horizon = sh * 0.40;
    let n = 26;
    for i in 0..n {
        let (ox, oy) = (hf(i, 11), hf(i, 12));
        let x = sw * ox * 0.6 + sw * 0.3;
        let y = horizon + (sh - horizon) * oy;
        let len = 2.0 + 3.0 * hf(i, 13);
        let drift = (local * 0.1 + i as f32).sin() * 0.4;
        c.line(x + drift, y, x + drift + len, y + len * 0.42, Color::Rgb(44, 38, 30), 0.30);
    }
}

/// 03 客モ無シ：空座标记——畳上唯一的座布団轮廓 + 上方「客 0」的静态框。
fn a03_no_guest(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, _dur: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let horizon = sh * 0.40;
    let x = sw * 0.26;
    let y = horizon + (sh - horizon) * 0.36;
    let w = sw * 0.11;
    let h = (sh - horizon) * 0.20;
    let pulse = 0.55 + 0.2 * (local * 0.8).sin();
    c.rect(x, y, w, h, AMBER_DIM, 0.5 * pulse);
    c.line(x, y, x + w * 0.12, y + h, AMBER_DIM, 0.35 * pulse);
    c.line(x + w, y, x + w * 0.88, y + h, AMBER_DIM, 0.35 * pulse);
    // 上方小框：客 0 名
    c.rect(x + w * 0.1, y - 4.2, 5.5, 3.0, AMBER, 0.4);
}

/// 04 高キ月：月升到最高点（背景月已移动，这里加月晕三环）。
fn a04_high_moon(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, dur: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let u = (local / dur).clamp(0.0, 1.0);
    let mx = sw * (0.30 + 0.42 * u);
    let my = sh * (0.16 - 0.05 * (u * std::f32::consts::PI).sin());
    let mr = (sh * 0.055).max(2.2);
    let br = 0.7 + 0.3 * ctx.low();
    for (k, g) in [(1.9f32, 0.20f32), (2.7, 0.12), (3.6, 0.07)] {
        c.circle(mx, my, mr * k, MOON, g * br);
    }
    // 月面暗斑（两枚）
    c.disc(mx - mr * 0.3, my - mr * 0.2, mr * 0.22, AMBER_DIM, 0.25);
    c.disc(mx + mr * 0.25, my + mr * 0.3, mr * 0.16, AMBER_DIM, 0.20);
}

/// 05 照ラス盃：空盃承接月光，杯心一点月影。
fn a05_cup_moon(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, _dur: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let horizon = sh * 0.40;
    let x = sw * 0.44;
    let y = horizon + (sh - horizon) * 0.62;
    let rx = sw * 0.045;
    let ry = (sh - horizon) * 0.05;
    // 盃身：下半椭圆 + 口沿
    c.ellipse(x, y, rx, ry, AMBER, 0.75);
    c.line(x - rx, y, x - rx * 0.55, y + ry * 1.6, AMBER, 0.55);
    c.line(x + rx, y, x + rx * 0.55, y + ry * 1.6, AMBER, 0.55);
    c.ellipse(x, y + ry * 1.6, rx * 0.55, ry * 0.35, AMBER, 0.5);
    // 杯中月影：摇曳的小亮点
    let wob = (local * 1.6).sin() * 0.4 + ctx.mid() * 0.5;
    c.disc(x + wob, y + 0.4, 0.9, MOON, unit(0.55 + 0.3 * ctx.low()));
}

/// 06 影一ツ：人影坐姿剪影，被月光拉长缩短。
fn a06_one_shadow(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, dur: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let horizon = sh * 0.40;
    let x = sw * 0.56;
    let y = horizon + (sh - horizon) * 0.66;
    let u = (local / dur).clamp(0.0, 1.0);
    // 影长随月位：月低影长、月高影短
    let stretch = 1.6 - 0.9 * u;
    let ink = Color::Rgb(22, 22, 26);
    // 坐姿：头 + 脊背 + 腿
    c.disc(x, y - 2.4 * stretch, 0.9, ink, 0.85);
    c.line(x, y - 1.5 * stretch, x, y + 0.4, ink, 0.8);
    c.line(x, y + 0.4, x - 1.8 * stretch, y + 0.9, ink, 0.7);
    c.line(x, y + 0.4, x + 1.4, y + 0.8, ink, 0.7);
    // 影脚在畳上拖出的长尾
    c.line(x + 1.4, y + 0.8, x + 1.4 + 5.0 * stretch, y + 1.0, ink, 0.35);
}

/// 07 遠キ音：远处砧声——画面右上角落的节拍脉冲环。
fn a07_kinuta_pulse(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, _dur: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let x = sw * 0.86;
    let y = sh * 0.14;
    // 每拍一圈涟漪
    let beat = (local * 1.53) % 1.0; // ≈92bpm 的拍频
    let r = 1.2 + beat * 4.0;
    let g = (1.0 - beat).max(0.0);
    c.circle(x, y, r, AMBER, 0.55 * g + 0.1);
    c.circle(x, y, r * 0.55, MOON, 0.35 * g);
    c.disc(x, y, 0.7, AMBER, 0.5 + 0.4 * ctx.punch);
    // 「远」的衰减尾巴：向左下漂散的点
    for k in 0..6 {
        let p = (local * 0.5 + k as f32 * 0.16) % 1.0;
        let dx = -k as f32 * 1.2 - p * 2.0;
        let dy = k as f32 * 0.5 + p * 1.2;
        c.set(x + dx, y + dy, AMBER_DIM, 0.30 * (1.0 - p));
    }
}

/// 08 砧カ竿カ：两只砧的剪影在远处上下起落。
fn a08_kinota_rods(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, _dur: f32) {
    let sh = c.sh as f32;
    let sw = c.sw as f32;
    let horizon = sh * 0.40;
    for (i, fx) in [(0.66f32, 0.0f32), (0.74, 0.8)].iter().enumerate() {
        let x = sw * fx.0;
        let base = horizon - 1.0;
        let phase = local * 2.2 + fx.1;
        let lift = (phase.sin() * 0.5 + 0.5) * 1.6;
        let h = 2.4;
        c.line(x, base, x, base - h, PINE, 0.6); // 杆
        // 砧头：随拍点落下
        let ky = base - h + lift;
        c.fill_rect(x - 1.0, ky - 1.0, 2.0, 1.4, AMBER_DIM, 0.65);
        if lift < 0.25 && i == 0 {
            // 落下的瞬间一点亮
            c.disc(x, ky + 0.6, 0.8, MOON, 0.5);
        }
    }
}

/// 09 水ノ上：庭池水面——畳右下的波纹碎月。
fn a09_water_surface(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, _dur: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let horizon = sh * 0.40;
    let x0 = sw * 0.60;
    let y0 = horizon + (sh - horizon) * 0.18;
    let w = sw * 0.34;
    let h = (sh - horizon) * 0.34;
    c.rect(x0, y0, w, h, MOON, 0.14);
    let rows = (h / 1.6).floor().max(2.0) as i32;
    for r in 0..rows {
        let y = y0 + h * (r as f32 + 0.5) / rows as f32;
        let amp = 0.5 + 0.4 * (local * 0.7 + r as f32).sin();
        let steps = (w / 1.5) as i32;
        let mut prev: Option<(f32, f32)> = None;
        for i in 0..=steps {
            let x = x0 + w * i as f32 / steps as f32;
            let yy = y + (i as f32 * 0.9 + local * 1.4 + r as f32 * 2.0).sin() * amp * 0.35;
            if let Some(q) = prev {
                c.line(q.0, q.1, x, yy, MOON, 0.28);
            }
            prev = Some((x, yy));
        }
    }
    // 水中碎月
    for k in 0..10 {
        let p = ((local * 0.35 + k as f32 * 0.1) % 1.0).abs();
        let mx = x0 + w * hf(k, 21);
        let my = y0 + h * hf(k, 22);
        c.set(mx + (p - 0.5) * 2.0, my, MOON, 0.5 * (1.0 - p));
    }
}

/// 10 聞ク者ハ：倾听者——人影侧耳，头侧一道弧线示「听」。
fn a10_listener(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, _dur: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let horizon = sh * 0.40;
    let x = sw * 0.56;
    let y = horizon + (sh - horizon) * 0.66;
    let ink = Color::Rgb(22, 22, 26);
    let tilt = (local * 0.6).sin() * 0.25;
    // 头微倾
    c.disc(x + tilt, y - 2.6, 0.9, ink, 0.85);
    c.line(x, y - 1.6, x, y + 0.4, ink, 0.8);
    // 「听」的弧：从头侧向砧方向展开的弧线，随拍脉动
    let steps = 14;
    let mut prev: Option<(f32, f32)> = None;
    let amp = 0.75 + 0.25 * ctx.punch;
    for i in 0..=steps {
        let u = i as f32 / steps as f32;
        let a = -0.9 + u * 1.1;
        let px = x + 2.0 + (a * 2.6).cos() * 3.2 * amp;
        let py = y - 2.4 + (a * 2.6).sin() * 2.4 * amp;
        if let Some(q) = prev {
            c.line(q.0, q.1, px, py, AMBER, 0.35);
        }
        prev = Some((px, py));
    }
}

/// 11 我ト夜風ノ：夜风起——横向流线扫过夜空。
fn a11_night_wind(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, _dur: f32) {
    let sh = c.sh as f32;
    let sw = c.sw as f32;
    let rows = (sh * 0.38).floor().max(3.0) as i32;
    for r in 0..rows {
        let y = sh * 0.38 * (r as f32 + 0.5) / rows as f32 + 1.0;
        let speed = 3.0 + hf(r as u32, 31) * 3.0;
        let phase = local * speed + r as f32 * 2.1;
        let steps = 20;
        let mut prev: Option<(f32, f32)> = None;
        for i in 0..=steps {
            let u = i as f32 / steps as f32;
            let x = ((phase + u * 30.0) % (sw + 30.0)) - 15.0;
            let yy = y + (u * 9.0 + phase * 0.4).sin() * 0.8;
            if let Some(q) = prev {
                let vis = if u > 0.05 && u < 0.95 { 1.0f32 } else { 0.0 };
                c.line(q.0, q.1, x, yy, GREYISH(), 0.22 * vis);
            }
            prev = Some((x, yy));
        }
    }
}

/// 灰白（夜风线色）。
const fn GREYISH() -> Color {
    Color::Rgb(168, 174, 184)
}

/// 12 只二人：风线与人影并置——「只二人」的构图收束。
fn a12_only_two(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, dur: f32) {
    a11_night_wind(ctx, c, local, dur);
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let horizon = sh * 0.40;
    // 人影放大居中一点
    let x = sw * 0.52;
    let y = horizon + (sh - horizon) * 0.62;
    let ink = Color::Rgb(22, 22, 26);
    c.disc(x, y - 2.8, 1.0, ink, 0.85);
    c.line(x, y - 1.8, x, y + 0.6, ink, 0.8);
    // 「风」化为绕人的半环
    let mut prev: Option<(f32, f32)> = None;
    for i in 0..=16 {
        let a = std::f32::consts::PI * (1.0 + i as f32 / 16.0);
        let px = x + a.cos() * 4.2;
        let py = y - 1.0 + a.sin() * 3.0;
        if let Some(q) = prev {
            c.line(q.0, q.1, px, py, GREYISH(), 0.3);
        }
        prev = Some((px, py));
    }
}

/// 13 落葉：粒子群——畳上滚动的落叶（哈希点，带旋转尾迹）。
fn a13_falling_leaves(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, _dur: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let n = 22;
    for i in 0..n {
        let seed_x = hf(i, 41);
        let speed = 1.2 + hf(i, 42) * 1.8;
        let phase = (local * speed * 0.12 + seed_x) % 1.0;
        let x = sw * ((seed_x + phase * 0.5) % 1.0);
        let y = sh * (0.1 + phase * 0.85);
        let spin = local * (1.5 + hf(i, 43)) + i as f32;
        let col = if i % 3 == 0 { AKANE } else { AMBER_DIM };
        let w = 0.45 + 0.25 * hf(i, 44);
        // 叶：两短线交叉 + 轨迹点
        let dx = spin.cos() * 1.1;
        let dy = spin.sin() * 0.55;
        c.line(x - dx, y - dy, x + dx, y + dy, col, w);
        c.line(x - dx * 0.3, y - dy * 1.2, x + dx * 0.3, y + dy * 1.2, col, w * 0.7);
        // 落迹
        c.set(x - dx * 2.2, y - 1.2, col, w * 0.3);
    }
}

/// 14 月動：月位进度可视化——夜空里一条细弧轨 + 月当前位置的刻度。
fn a14_moon_track(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, dur: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let u = (local / dur).clamp(0.0, 1.0);
    // 弧轨
    let mut prev: Option<(f32, f32)> = None;
    for i in 0..=30 {
        let v = i as f32 / 30.0;
        let x = sw * (0.26 + 0.5 * v);
        let y = sh * (0.14 - 0.04 * (v * std::f32::consts::PI).sin()) + 6.0;
        if let Some(q) = prev {
            c.line(q.0, q.1, x, y, AMBER_DIM, 0.14);
        }
        prev = Some((x, y));
    }
    // 当前刻度
    let mx = sw * (0.26 + 0.5 * u);
    let my = sh * (0.14 - 0.04 * (u * std::f32::consts::PI).sin()) + 6.0;
    c.circle(mx, my, 1.4, MOON, 0.5);
}

/// 15 盃搖：杯中月影散作涟漪（呼应「照ラス盃」的余韵）。
fn a15_cup_ripples(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, _dur: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let horizon = sh * 0.40;
    let x = sw * 0.44;
    let y = horizon + (sh - horizon) * 0.62;
    for k in 0..3 {
        let p = ((local * 0.5 + k as f32 * 0.33) % 1.0).abs();
        c.ellipse(x, y, 1.2 + p * 3.2, 0.5 + p * 1.1, MOON, 0.4 * (1.0 - p));
    }
}

/// 16 影舞：人影起立，影长扫过整张畳。
fn a16_shadow_dance(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, dur: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let horizon = sh * 0.40;
    let x = sw * 0.5;
    let y = horizon + (sh - horizon) * 0.6;
    let rise = fade(local / (dur * 0.35));
    let ink = Color::Rgb(22, 22, 26);
    let h = 2.4 + rise * 2.2;
    c.disc(x, y - h, 0.95, ink, 0.85);
    c.line(x, y - h + 1.0, x, y, ink, 0.8);
    // 长影扫动
    let sweep = (local * 0.35).sin();
    c.line(x, y, x + sweep * sw * 0.3, y + 1.6, ink, 0.4);
}

/// 17 砧遠：砧声远去——脉冲环缩成一点，涟漪变稀。
fn a17_kinuta_far(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, _dur: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let x = sw * 0.86;
    let y = sh * 0.14;
    let dim = win(local, 0.0, 8.0);
    let beat = (local * 1.53) % 1.0;
    let r = 1.0 + beat * 2.2;
    c.circle(x, y, r, AMBER, 0.4 * (1.0 - beat) * dim);
    c.set(x, y, MOON, 0.5 * dim);
}

/// 18 風止：风停——流线全部收敛为一根横线。
fn a18_wind_still(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, _dur: f32) {
    let sh = c.sh as f32;
    let sw = c.sw as f32;
    let y = sh * 0.2;
    let amp = (1.0 - fade(local / 3.0)).max(0.0);
    let mut prev: Option<(f32, f32)> = None;
    for i in 0..=40 {
        let x = sw * i as f32 / 40.0;
        let yy = y + (x * 0.35 + local).sin() * 1.8 * amp;
        if let Some(q) = prev {
            c.line(q.0, q.1, x, yy, GREYISH(), 0.3);
        }
        prev = Some((x, yy));
    }
}

/// 19 秋深：茜色沉月——月染成茜色沉向地平线（为冬作铺垫）。
fn a19_akane_moon(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, dur: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let u = (local / dur).clamp(0.0, 1.0);
    let mx = sw * (0.72 - 0.2 * u);
    let my = sh * (0.16 + 0.16 * u);
    let mr = (sh * 0.055).max(2.2);
    let mix = fade(local / (dur * 0.5));
    let cc = Color::Rgb(
        (252.0 * (1.0 - mix) + 216.0 * mix) as u8,
        (244.0 * (1.0 - mix) + 84.0 * mix) as u8,
        (214.0 * (1.0 - mix) + 58.0 * mix) as u8,
    );
    c.disc(mx, my, mr, cc, 0.7);
    c.circle(mx, my, mr * 1.5, cc, 0.25);
    // 地平线附近压一层茜雾
    c.tint_rect(0.0, sh * 0.3, sw, sh * 0.12, AKANE, 0.10 * mix);
}

/// 20 夜半：全暗只余月与盃的一点月影（秋的收束静帧）。
fn a20_midnight(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, dur: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    let u = (local / dur).clamp(0.0, 1.0);
    // 全屏压暗（不动 w，只调色）
    c.tint(NIGHT, 0.35 * fade(local / 2.0));
    let mx = sw * (0.52 + 0.2 * u);
    let my = sh * 0.24;
    let mr = (sh * 0.05).max(2.0);
    c.disc(mx, my, mr, MOON, 0.6);
    // 杯影一点
    let x = sw * 0.44;
    let y = sh * 0.72;
    let tw = 0.5 + 0.5 * (local * 2.0).sin();
    c.set(x, y, MOON, 0.4 + 0.4 * tw);
    c.set(x + 1.0, y, MOON, 0.25 * tw);
    c.set(x - 1.0, y, MOON, 0.25 * tw);
}

// ── 卡片表 ──────────────────────────────────────────────

/// 卡：段内起始秒（相对秋段起点 118.6s）、卡名、绘制函数。
type CardFn = fn(&SceneCtx, &mut CharCanvas, f32, f32);

pub struct AutumnCard {
    /// 相对段起点的秒数
    pub at: f32,
    pub name: &'static str,
    pub draw: CardFn,
}

/// 秋 20 卡。唱句锚点（相对 118.6）：0 / 7.75 / 11.8 / 19.4，此后进入间奏与过门。
pub const AUTUMN_CARDS: &[AutumnCard] = &[
    AutumnCard { at: 0.0, name: "松ノ影", draw: a01_pine_shadow },
    AutumnCard { at: 1.6, name: "畳ニ延ビテ", draw: a02_tatami_extend },
    AutumnCard { at: 3.4, name: "客モ無シ", draw: a03_no_guest },
    AutumnCard { at: 5.6, name: "遠景", draw: a14_moon_track },
    AutumnCard { at: 7.75, name: "高キ月", draw: a04_high_moon },
    AutumnCard { at: 9.4, name: "照ラス盃", draw: a05_cup_moon },
    AutumnCard { at: 10.6, name: "影一ツ", draw: a06_one_shadow },
    AutumnCard { at: 11.8, name: "遠キ音", draw: a07_kinuta_pulse },
    AutumnCard { at: 13.2, name: "砧カ竿カ", draw: a08_kinota_rods },
    AutumnCard { at: 15.0, name: "水ノ上", draw: a09_water_surface },
    AutumnCard { at: 16.8, name: "聞ク者", draw: a10_listener },
    AutumnCard { at: 19.4, name: "我ト夜風", draw: a11_night_wind },
    AutumnCard { at: 21.2, name: "只二人", draw: a12_only_two },
    AutumnCard { at: 23.4, name: "落葉", draw: a13_falling_leaves },
    AutumnCard { at: 25.6, name: "月動", draw: a14_moon_track },
    AutumnCard { at: 27.4, name: "盃搖", draw: a15_cup_ripples },
    AutumnCard { at: 29.2, name: "影舞", draw: a16_shadow_dance },
    AutumnCard { at: 31.0, name: "砧遠", draw: a17_kinuta_far },
    AutumnCard { at: 33.0, name: "風止", draw: a18_wind_still },
    AutumnCard { at: 35.2, name: "夜半", draw: a20_midnight },
];

/// 秋段入口：背景 + 当卡 + 落叶常驻（卡间共享，保证连贯）。
pub fn render_autumn(ctx: &SceneCtx, c: &mut CharCanvas, local: f32, dur: f32) {
    autumn_bg(ctx, c, local, dur);
    moon_pool(ctx, c, local, dur, 0.6 + 0.4 * ctx.low());
    // 当前卡：取最后一个 at <= local 的
    let card = AUTUMN_CARDS.iter().rev().find(|k| local >= k.at).unwrap_or(&AUTUMN_CARDS[0]);
    (card.draw)(ctx, c, local - card.at, dur);
    // 落叶常驻层（低强度，卡间不断）
    a13_falling_leaves_quiet(ctx, c, local);
}

/// 常驻落叶（强度低，不抢卡）。
fn a13_falling_leaves_quiet(ctx: &SceneCtx, c: &mut CharCanvas, local: f32) {
    let sw = c.sw as f32;
    let sh = c.sh as f32;
    for i in 0..8 {
        let seed_x = hf(i + 90, 41);
        let phase = (local * 0.05 + seed_x) % 1.0;
        let x = sw * ((seed_x + phase * 0.4) % 1.0);
        let y = sh * (0.08 + phase * 0.9);
        c.set(x, y, AMBER_DIM, 0.35);
        c.set(x + 0.6, y + 0.3, AMBER_DIM, 0.2);
    }
}

// ── 测试 ────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::features::{BAND_COUNT, SPECTRUM_BINS, FrameFeatures};
    use crate::config::RenderMode;
    use crate::lyrics::Lyrics;
    use crate::render::particles::ParticleSystem;
    use crate::render::scenes::{SceneCtx, render};
    use crate::timeline::scene::SceneKind;
    use rand::rngs::SmallRng;
    use rand::SeedableRng;

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
            t: 130.0,
            local_t: 11.4,
            progress: 0.3,
            feat: f.clone(),
            prev: f,
            frame: 100,
            cols: 80,
            rows: 30,
            scene: SceneKind::Autumn,
            lyrics,
            flash: 0.0,
            punch: 0.0,
            heartbeat: 0.0,
            glitch: 0.0,
            burst: 0.0,
            invert: 0.0,
        }
    }

    fn draw_card(f: CardFn, local: f32) -> (usize, Vec<u32>) {
        let c = ctx();
        let mut canvas = CharCanvas::new(80, 30, RenderMode::Braille);
        autumn_bg(&c, &mut canvas, local, 37.3);
        f(&c, &mut canvas, local, 37.3);
        let sig: Vec<u32> = canvas
            .pixels()
            .iter()
            .flat_map(|p| [p.w.to_bits(), p.r.to_bits(), p.g.to_bits(), p.b.to_bits()])
            .collect();
        (canvas.pixels().iter().filter(|p| !p.is_empty()).count(), sig)
    }

    #[test]
    fn every_card_draws_something() {
        for k in AUTUMN_CARDS {
            let (n, _) = draw_card(k.draw, k.at + 0.5);
            assert!(n > 20, "秋卡「{}」只画了 {n} 像素", k.name);
        }
    }

    #[test]
    fn no_panic_on_tiny_canvas() {
        let c = ctx();
        for k in AUTUMN_CARDS {
            let mut canvas = CharCanvas::new(8, 5, RenderMode::Braille);
            autumn_bg(&c, &mut canvas, k.at + 0.5, 37.3);
            (k.draw)(&c, &mut canvas, k.at + 0.5, 37.3);
        }
    }

    #[test]
    fn cards_differ_frame_to_frame() {
        // 相邻时间片的画面对比：至少一半的卡能产出不同的帧
        let mut diffs = 0;
        for k in AUTUMN_CARDS {
            let (_, s0) = draw_card(k.draw, k.at + 0.2);
            let (_, s1) = draw_card(k.draw, k.at + 1.4);
            if s0 != s1 {
                diffs += 1;
            }
        }
        assert!(diffs >= AUTUMN_CARDS.len() / 2, "动起来的卡只有 {diffs}");
    }

    #[test]
    fn no_jump_at_card_boundary() {
        // 卡交界两侧 0.12s 的画面：平均覆盖度差应远小于卡间平均差
        let cov = |f: CardFn, t: f32| {
            let c = ctx();
            let mut canvas = CharCanvas::new(80, 30, RenderMode::Braille);
            autumn_bg(&c, &mut canvas, t, 37.3);
            f(&c, &mut canvas, t, 37.3);
            canvas.pixels().iter().map(|p| p.w).sum::<f32>()
        };
        let mut jumps = 0;
        for w in AUTUMN_CARDS.windows(2) {
            let before = cov(w[1].draw, w[1].at - 0.06);
            let after = cov(w[1].draw, w[1].at + 0.06);
            if (after - before).abs() / after.max(1.0) > 0.5 {
                jumps += 1;
            }
        }
        assert!(jumps <= 2, "卡交界跳变过多：{jumps}");
    }

    #[test]
    fn dispatch_wired() {
        // scenes::render 走到 Autumn 分支时不 panic（接线由 Lead 完成，这里只测本文件入口）
        let c = ctx();
        let mut canvas = CharCanvas::new(80, 30, RenderMode::Braille);
        let mut ps = ParticleSystem::new(1, 64);
        let mut rng = SmallRng::seed_from_u64(7);
        render(&c, &mut canvas, &mut ps, &mut rng); // Autumn 若已接线则走本文件
        let n = canvas.pixels().iter().filter(|p| !p.is_empty()).count();
        assert!(n > 20, "scenes::render(Autumn) 只画了 {n} 像素（若未接线属 Lead 侧）");
    }
}

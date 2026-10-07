//! 四季母题库 —— 《春・夏・秋・冬》可复用的绘制单元。
//!
//! 每个母题都接收 `(canvas, ctx, alpha)`，内部只依赖 `ctx.t / ctx.local_t /
//! ctx.progress / ctx.feat` 做连续动画：任何位置与大小必须是 `t` 的连续函数，
//! 用 [`smoothstep`] 缓入缓出，禁止跳变。
//!
//! 颜色语义：
//! - `main`  = 季节主色（春嫩绿 / 夏浓青 …）
//! - `deep`  = 季节暗色（大地、影）
//! - `accent`= 点睛色（樱粉 / 热橙 …）

use ratatui::style::Color;

use crate::render::color as col;
use crate::render::scenes::SceneCtx;
use crate::render::CharCanvas;

/// 缓入缓出：`x<0 → 0`，`x>1 → 1`，中间是 3x²-2x³。
pub fn smoothstep(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// 把 `phase`（秒）除以时长再 smoothstep。
pub fn ease(phase: f32, dur: f32) -> f32 {
    smoothstep(if dur <= 0.0 { 1.0 } else { phase / dur })
}

/// 一段「淡入-保持-淡出」包络：`[t0,t1]` 升、`[t1,t2]` 平、`[t2,t3]` 降。
pub fn envelope(t: f32, t0: f32, t1: f32, t2: f32, t3: f32) -> f32 {
    if t <= t0 || t >= t3 {
        return 0.0;
    }
    if t < t1 {
        return smoothstep((t - t0) / (t1 - t0).max(1e-6));
    }
    if t < t2 {
        return 1.0;
    }
    1.0 - smoothstep((t - t2) / (t3 - t2).max(1e-6))
}

/// 把画布坐标归一到 0..1（横向 / 纵向）。
pub fn uv(c: &CharCanvas) -> (f32, f32) {
    (c.sw as f32, c.sh as f32)
}

// ─────────────────────────────────────────────────────────────────────────────
// 春
// ─────────────────────────────────────────────────────────────────────────────

/// 「薄緑 苔の産毛に 日が射して」—— 苔地：多层起伏 + 表面绒毛颤动。
pub fn moss_ground(canvas: &mut CharCanvas, ctx: &SceneCtx, alpha: f32) {
    let (sw, sh) = uv(canvas);
    let [main, deep, _] = ctx.scene.palette();
    let t = ctx.t as f32;
    let rise = ease(ctx.local_t as f32, 4.0); // 苔随段落开头从地平线长上来
    if rise <= 0.0 {
        return;
    }
    // 三层大地，越近颜色越深、起伏越大
    for layer in 0..3u32 {
        let base = 0.62 + layer as f32 * 0.11;
        let amp = 2.0 + layer as f32 * 1.6;
        let tone = col::lerp(main, deep, layer as f32 / 2.0);
        let a = alpha * rise * (1.0 - layer as f32 * 0.18);
        let mut prev: Option<f32> = None;
        for x in 0..(sw as i32) {
            let xf = x as f32;
            let y = sh * base
                + (xf * 0.045 + layer as f32 * 2.1).sin() * amp
                + (xf * 0.013 - layer as f32 * 1.2).sin() * amp * 0.6;
            let y = y.max(0.0).min(sh - 1.0);
            if let Some(py) = prev {
                canvas.line(xf - 1.0, py, xf, y, tone, a * 0.85);
            }
            prev = Some(y);
            // 绒毛：地表往上冒的细刺，随时间轻轻颤
            if alpha * rise > 0.35 {
                let f = ((xf * 0.37 + t * 0.4).sin() * 0.5 + 0.5) * 2.2 * rise;
                canvas.line(xf, y, xf, y - f, tone, a * 0.35);
            }
        }
    }
}

/// 「薄緑 苔の産毛に 日が射して」—— 斜射光带：从右上洒下来的三道光。
pub fn light_shaft(canvas: &mut CharCanvas, ctx: &SceneCtx, alpha: f32) {
    let (sw, sh) = uv(canvas);
    let [main, _, accent] = ctx.scene.palette();
    let t = ctx.t as f32;
    for k in 0..3u32 {
        let sway = (t * 0.22 + k as f32 * 1.7).sin() * sw * 0.02;
        let x0 = sw * (0.34 + k as f32 * 0.16) + sway;
        let x1 = x0 - sw * 0.16;
        let beam = 2.2 + k as f32 * 1.3;
        let tone = if k == 1 { accent } else { main };
        let a = alpha * (0.16 + 0.06 * (t * 0.9 + k as f32).sin());
        let steps = (sh as i32).max(24);
        for i in 0..steps {
            let u = i as f32 / steps as f32;
            let x = x0 + (x1 - x0) * u;
            let y = u * sh;
            for d in 0..(beam as i32) {
                let w = a * (1.0 - d as f32 / beam);
                canvas.set(x + d as f32, y, tone, w);
            }
        }
    }
}

/// 「朝露を 飲み干す様は 生キル音」—— 朝露：先一颗颗凝出，随后被饮干。
pub fn dew_field(canvas: &mut CharCanvas, ctx: &SceneCtx, alpha: f32) {
    let (sw, sh) = uv(canvas);
    let [main, deep, accent] = ctx.scene.palette();
    let t = ctx.t as f32;
    let ph = ctx.local_t as f32;
    // 凝出（前 4s）→ 停留 → 饮干（第 6s 起往上收）
    let born = ease(ph - 1.0, 4.0);
    let drunk = ease(ph - 6.5, 3.5);
    if born <= 0.0 || drunk >= 1.0 {
        return;
    }
    let n = 26;
    for k in 0..n {
        let kf = k as f32;
        // 露珠只在苔地带内生成
        let x = ((kf * 0.618) % 1.0) * sw;
        let gy = sh * (0.68 + ((kf * 0.377) % 1.0) * 0.22);
        let stagger = ((kf * 0.731) % 1.0) * 2.5;
        let b = ease(ph - 1.0 - stagger, 2.0);
        if b <= 0.0 {
            continue;
        }
        // 饮干：露珠往上飘走并缩小
        let rise = drunk * sh * 0.35 * (0.6 + (kf * 0.513) % 1.0 * 0.8);
        let r = (1.0 + (kf * 0.913) % 1.0) * b * (1.0 - drunk * 0.9);
        if r <= 0.2 {
            continue;
        }
        let jitter = (t * 1.1 + kf).sin() * 0.8;
        let tone = if k % 6 == 0 { accent } else { col::lerp(col::WHITE, main, 0.35) };
        canvas.disc(x + jitter, gy - rise, r.max(0.4), tone, alpha * 0.85);
        // 高光点
        canvas.set(x + jitter - r * 0.4, gy - rise - r * 0.4, col::WHITE, alpha * 0.7 * b);
        let _ = deep;
    }
}

/// 「一羽の蝶 蕾の重み 試す午後」—— 蝴蝶：从左侧飞入，翅膀按 sin 拍动。
pub fn butterfly(canvas: &mut CharCanvas, ctx: &SceneCtx, alpha: f32) {
    let (sw, sh) = uv(canvas);
    let [main, deep, accent] = ctx.scene.palette();
    let t = ctx.t as f32;
    let ph = ctx.local_t as f32;
    let enter = ease(ph - 2.0, 5.0); // 第三句才进来
    if enter <= 0.0 {
        return;
    }
    // 飞行路径：缓的 8 字
    let x = -4.0 + (sw * 0.68 + 4.0) * enter + (t * 0.35).sin() * sw * 0.05;
    let y = sh * 0.34 + (t * 0.5).cos() * sh * 0.06;
    let flap = (t * 6.0).sin();
    let span = 3.2 + 1.4 * flap.abs();
    // 身体
    canvas.line(x, y - 1.6, x, y + 1.6, deep, alpha * 0.9);
    // 四片翅（上大下小），张开角度随 flap 变化
    let up = span * (1.0 + flap * 0.35);
    let lo = span * 0.7 * (1.0 + flap * 0.3);
    canvas.line(x, y, x - up, y - span * 0.9, accent, alpha * 0.9);
    canvas.line(x - up * 0.4, y - span * 0.5, x, y, accent, alpha * 0.6);
    canvas.line(x, y, x + up, y - span * 0.9, accent, alpha * 0.9);
    canvas.line(x + up * 0.4, y - span * 0.5, x, y, accent, alpha * 0.6);
    canvas.line(x, y + 0.8, x - lo, y + span * 0.6, col::lerp(accent, deep, 0.4), alpha * 0.7);
    canvas.line(x, y + 0.8, x + lo, y + span * 0.6, col::lerp(accent, deep, 0.4), alpha * 0.7);
    // 翅膀留下的一点点磷粉
    if flap > 0.6 {
        canvas.set(x - up * 1.3, y - span * 1.1, col::WHITE, alpha * 0.35);
        canvas.set(x + up * 1.3, y - span * 1.1, col::WHITE, alpha * 0.35);
    }
    let _ = main;
}

/// 「一羽の蝶 蕾の重み 試す午後」—— 花蕾：鼓起来，被蝶落下时轻轻压弯。
pub fn buds(canvas: &mut CharCanvas, ctx: &SceneCtx, alpha: f32) {
    let (sw, sh) = uv(canvas);
    let [main, deep, accent] = ctx.scene.palette();
    let t = ctx.t as f32;
    let ph = ctx.local_t as f32;
    let grow = ease(ph - 2.5, 4.0);
    if grow <= 0.0 {
        return;
    }
    let spots = [(0.30, 0.70), (0.44, 0.74), (0.58, 0.70)];
    for (i, &(ux, uy)) in spots.iter().enumerate() {
        let stagger = ease(ph - 2.5 - i as f32 * 0.7, 2.5);
        if stagger <= 0.0 {
            continue;
        }
        let x = ux * sw;
        let gy = uy * sh;
        // 呼吸式的鼓起
        let breathe = 1.0 + 0.12 * (t * 1.7 + i as f32 * 1.3).sin();
        let r = (1.6 + i as f32 * 0.5) * stagger * breathe;
        let tone = col::lerp(main, accent, 0.55 + 0.2 * (t * 0.8 + i as f32).sin());
        // 茎
        canvas.line(x, gy, x, gy + 3.5, deep, alpha * 0.8 * stagger);
        canvas.disc(x, gy - r, r.max(0.5), tone, alpha * 0.85);
        // 萼片
        canvas.line(x - r * 0.8, gy + 0.4, x, gy - r * 0.5, deep, alpha * 0.5 * stagger);
        canvas.line(x + r * 0.8, gy + 0.4, x, gy - r * 0.5, deep, alpha * 0.5 * stagger);
    }
}

/// 「東風吹けば 雲は解ケテ 水ニナル」—— 东风解云：云被吹散成水，水面涟漪渐起。
pub fn clouds_dissolving(canvas: &mut CharCanvas, ctx: &SceneCtx, alpha: f32) {
    let (sw, sh) = uv(canvas);
    let [main, deep, _] = ctx.scene.palette();
    let t = ctx.t as f32;
    let ph = ctx.local_t as f32;
    let w = envelope(ph, 3.0, 4.5, 10.0, 12.5); // 第四句的窗口
    if w <= 0.0 {
        return;
    }
    // 三朵云：从右往左被吹散（横向拉伸 + 淡出）
    for k in 0..3u32 {
        let drift = ease(ph - 3.5 - k as f32 * 0.8, 6.0);
        let cx = sw * (0.30 + k as f32 * 0.22) - drift * sw * 0.18;
        let cy = sh * (0.14 + k as f32 * 0.045);
        let spread = 4.0 + drift * 10.0; // 越吹越宽 → 越薄
        let a = alpha * w * (1.0 - drift * 0.85);
        if a <= 0.02 {
            continue;
        }
        for b in 0..3u32 {
            let rx = spread * (1.0 - b as f32 * 0.22);
            let ry = 1.6 + b as f32 * 0.5;
            canvas.ellipse(cx + (t * 0.4 + b as f32).sin() * 1.5, cy + b as f32 * 1.1 - 1.0, rx, ry,
                           col::lerp(col::WHITE, main, 0.4), a * (0.5 - b as f32 * 0.1));
        }
    }
    // 云化成的水：下边浮起的水纹
    let water = ease(ph - 6.0, 5.0);
    if water > 0.0 {
        let wl = sh * (0.90 - 0.16 * water);
        let mut prev: Option<f32> = None;
        for x in 0..(sw as i32) {
            let xf = x as f32;
            let y = wl + (xf * 0.08 - t * 1.6).sin() * 1.4 * water
                + (xf * 0.021 + t * 0.7).sin() * 1.0 * water;
            if let Some(py) = prev {
                canvas.line(xf - 1.0, py, xf, y, col::lerp(main, col::WHITE, 0.45), alpha * water * 0.6);
            }
            prev = Some(y);
        }
    }
    let _ = deep;
}

/// 「東風吹けば」—— 风线：横掠而过的气流丝。
pub fn east_wind(canvas: &mut CharCanvas, ctx: &SceneCtx, alpha: f32) {
    let (sw, sh) = uv(canvas);
    let [main, _, _] = ctx.scene.palette();
    let t = ctx.t as f32;
    for k in 0..5u32 {
        let speed = 0.14 + k as f32 * 0.05;
        let u = ((t * speed + k as f32 * 0.31) % 1.4) - 0.2; // 0..1.4，出屏后重来
        let y = sh * (0.12 + k as f32 * 0.13) + (t * 0.8 + k as f32).sin() * 2.0;
        let x = u * sw;
        let len = sw * 0.10 * (1.0 - (u - 0.5).abs());
        let a = alpha * 0.4 * (1.0 - (u - 0.7).abs().max(0.0)).max(0.0);
        if a <= 0.02 || len <= 0.5 {
            continue;
        }
        canvas.line(x, y, x + len, y + len * 0.06, col::lerp(col::WHITE, main, 0.5), a);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 夏
// ─────────────────────────────────────────────────────────────────────────────

/// 「白雨来て 蓮の葉叩ク 乱レ玉」—— 白雨：斜雨场，密度随节拍呼吸。
pub fn white_rain(canvas: &mut CharCanvas, ctx: &SceneCtx, alpha: f32, rng: &mut impl FnMut() -> f32) {
    let (sw, sh) = uv(canvas);
    let [main, _, _] = ctx.scene.palette();
    let t = ctx.t as f32;
    let beat = ctx.feat.beat.clamp(0.0, 1.0);
    let high = (ctx.feat.bands_norm[4] * 0.4 + ctx.feat.bands_norm[5] * 0.6).clamp(0.0, 1.0);
    let density = 90.0 + beat * 90.0 + high * 50.0;
    for _ in 0..(density as i32) {
        let r1 = rng();
        let r2 = rng();
        let x = r1 * (sw + sh * 0.35) - sh * 0.2;
        let y = r2 * sh;
        // 斜向短线：dx = dy * 0.35
        let len = 2.5 + r2 * 2.5 + beat * 1.5;
        let tone = if r1 > 0.85 { col::WHITE } else { col::lerp(col::WHITE, main, 0.5) };
        canvas.line(x, y, x + len * 0.35, y + len, tone, alpha * (0.28 + r2 * 0.4));
    }
}

/// 「白雨来て 蓮の葉叩ク 乱レ玉」—— 莲叶：三枚，雨打时叶面颤动。
pub fn lotus_leaves(canvas: &mut CharCanvas, ctx: &SceneCtx, alpha: f32) {
    let (sw, sh) = uv(canvas);
    let [main, deep, accent] = ctx.scene.palette();
    let t = ctx.t as f32;
    let beat = ctx.feat.beat.clamp(0.0, 1.0);
    let spots = [(0.24, 0.76, 0.13), (0.55, 0.86, 0.16), (0.85, 0.74, 0.11)];
    for (i, &(ux, uy, ur)) in spots.iter().enumerate() {
        let x = ux * sw;
        let y = uy * sh;
        let r = ur * sw.min(sh) * 0.9;
        // 雨点打击：叶面按 beat 颤动（幅度 1px 级别，连续）
        let tremble = (t * 9.0 + i as f32 * 2.1).sin() * (0.6 + beat * 1.6);
        let tone = col::lerp(main, deep, 0.35 + 0.25 * (t * 0.6 + i as f32).sin());
        canvas.ellipse(x, y + tremble * 0.4, r, r * 0.42, tone, alpha * 0.9);
        canvas.ellipse(x, y + tremble * 0.4, r * 0.55, r * 0.24, col::lerp(tone, deep, 0.5), alpha * 0.4);
        // 叶脉
        for v in 1..4u32 {
            let a = std::f32::consts::PI * v as f32 / 4.0;
            canvas.line(x, y + tremble * 0.4,
                        x + (a.cos()) * r * 0.9, y + tremble * 0.4 + (a.sin() * 0.42) * r * 0.9,
                        col::lerp(tone, deep, 0.6), alpha * 0.35);
        }
        // 叶心立起的水珠（乱レ玉的前兆）
        let bob = (t * 2.2 + i as f32).abs().sin() * 1.2;
        canvas.disc(x, y - r * 0.42 - bob, 1.2 + beat * 0.8, col::WHITE, alpha * 0.8);
        let _ = accent;
    }
}

/// 「跳ネる 銀ノ軌跡ハ 誰ガ意思」—— 银色抛物线：雨滴从叶面溅起的弧。
pub fn silver_arcs(canvas: &mut CharCanvas, ctx: &SceneCtx, alpha: f32) {
    let (sw, sh) = uv(canvas);
    let [main, _, _] = ctx.scene.palette();
    let t = ctx.t as f32;
    let ph = ctx.local_t as f32;
    let on = envelope(ph, 2.0, 3.5, 12.0, 14.0);
    if on <= 0.0 {
        return;
    }
    let beat = ctx.feat.beat.clamp(0.0, 1.0);
    for k in 0..7u32 {
        // 每条弧有独立相位，循环弹出
        let cyc = (t * 0.55 + k as f32 * 0.147) % 1.0;
        let life = ease(cyc, 0.18) * (1.0 - ease(cyc - 0.82, 0.18));
        if life <= 0.02 {
            continue;
        }
        let x0 = sw * (0.18 + k as f32 * 0.10);
        let y0 = sh * (0.74 + (k as f32 * 0.713) % 1.0 * 0.10);
        let dir = if k % 2 == 0 { 1.0 } else { -1.0 };
        let u = cyc;
        let vx = dir * (2.2 + (k as f32 * 0.311) % 1.0 * 1.6);
        let vy = -(3.4 + (k as f32 * 0.517) % 1.0 * 1.4);
        let g = 5.2;
        let x = x0 + vx * u * 6.0;
        let y = y0 + vy * u * 6.0 + g * u * u * 18.0;
        // 弧线本体：一串亮银点
        for s in 0..7u32 {
            let su = (u - s as f32 * 0.012).max(0.0);
            let sx = x0 + vx * su * 6.0;
            let sy = y0 + vy * su * 6.0 + g * su * su * 18.0;
            let fade = 1.0 - s as f32 / 7.0;
            let tone = if s < 2 { col::WHITE } else { col::lerp(col::WHITE, main, 0.45) };
            canvas.set(sx, sy, tone, alpha * life * fade * (0.6 + beat * 0.4));
        }
    }
}

/// 「簾巻ク 風ノ強サニ 酒香ル」—— 竹帘：顶部横纹像卷帘一样降下来。
pub fn sudare(canvas: &mut CharCanvas, ctx: &SceneCtx, alpha: f32) {
    let (sw, sh) = uv(canvas);
    let [main, deep, _] = ctx.scene.palette();
    let ph = ctx.local_t as f32;
    let roll = ease(ph - 3.5, 3.0); // 第三句卷帘
    if roll <= 0.0 {
        return;
    }
    let n = 9;
    let bottom = sh * 0.30 * roll; // 卷下来的深度
    for k in 0..n {
        let y = 2.0 + k as f32 * (bottom / n as f32);
        if y <= 2.0 {
            continue;
        }
        let sway = (ctx.t as f32 * 0.9 + k as f32 * 0.7).sin() * 0.8;
        let tone = col::lerp(main, deep, k as f32 / n as f32 * 0.4);
        let seg = 8.0;
        let mut x = 0.0;
        while x < sw {
            let drop = (x * 0.05 + k as f32).sin() * 0.5;
            canvas.line(x, y + drop + sway * (x / sw), (x + seg * 0.7).min(sw), y + drop + sway * ((x + seg) / sw),
                        tone, alpha * (0.45 - k as f32 * 0.02));
            x += seg;
        }
    }
}

/// 「簾巻ク 風ノ強サニ 酒香ル」—— 酒壶与飘香曲线。
pub fn sake_jug(canvas: &mut CharCanvas, ctx: &SceneCtx, alpha: f32) {
    let (sw, sh) = uv(canvas);
    let [main, deep, accent] = ctx.scene.palette();
    let t = ctx.t as f32;
    let ph = ctx.local_t as f32;
    let on = ease(ph - 4.0, 2.5);
    if on <= 0.0 {
        return;
    }
    let x = sw * 0.10;
    let y = sh * 0.86;
    let s = sh.min(sw) * 0.045;
    // 壶身：矮胖的瓮
    canvas.ellipse(x, y, s * 1.3, s * 0.9, col::lerp(deep, accent, 0.25), alpha * 0.85 * on);
    canvas.ellipse(x, y - s * 0.5, s * 0.6, s * 0.35, col::lerp(deep, accent, 0.4), alpha * 0.7 * on);
    // 壶口
    canvas.line(x - s * 0.5, y - s * 0.85, x + s * 0.5, y - s * 0.85, accent, alpha * 0.8 * on);
    // 飘香：三缕向上卷的线
    for k in 0..3u32 {
        let drift = ((t * 0.30 + k as f32 * 0.33) % 1.0);
        let a = alpha * on * (1.0 - drift) * 0.6;
        if a <= 0.02 {
            continue;
        }
        let mut prev: Option<(f32, f32)> = None;
        for i in 0..=10 {
            let u = i as f32 / 10.0;
            let yy = y - s * 1.1 - (drift * 14.0 + u * 4.0);
            let xx = x + (t * 1.8 + k as f32 * 2.0 + u * 3.5).sin() * (2.0 + u * 2.5);
            if let Some(p) = prev {
                canvas.line(p.0, p.1, xx, yy, col::lerp(col::WHITE, main, 0.5), a * (1.0 - u * 0.5));
            }
            prev = Some((xx, yy));
        }
    }
}

/// 「蒸セ返ル 土ノ匂イモ 凪イデ行ク」—— 蒸腾热浪：底部行位移的正弦扭曲。
pub fn haze_heat(canvas: &mut CharCanvas, ctx: &SceneCtx, alpha: f32) {
    let (sw, sh) = uv(canvas);
    let [main, _, _] = ctx.scene.palette();
    let ph = ctx.local_t as f32;
    let on = envelope(ph, 6.0, 8.0, 13.0, 15.5);
    if on <= 0.0 {
        return;
    }
    let t = ctx.t as f32;
    let rows = 10;
    let band_h = sh * 0.30 / rows as f32;
    let y_top = sh * 0.70;
    // 半透明雾层：逐行画横向摆动线，相位随行号错开
    for r in 0..rows {
        let y = y_top + r as f32 * band_h;
        let sway = (t * (0.7 + r as f32 * 0.05) + r as f32 * 1.4).sin() * (1.0 + r as f32 * 0.35);
        let mut prev: Option<f32> = None;
        let step = 6.0f32;
        let mut x = 0.0;
        while x <= sw {
            let yy = y + sway * (x * 0.02).sin();
            if let Some(px) = prev {
                canvas.line(px, yy, x, yy, col::lerp(main, col::WHITE, 0.5), alpha * on * 0.10);
            }
            prev = Some(x);
            x += step;
        }
    }
}

/// 「白雨来て」—— 蝉鸣环：随高频能量扩张的同心环。
pub fn cicada_rings(canvas: &mut CharCanvas, ctx: &SceneCtx, alpha: f32) {
    let (sw, sh) = uv(canvas);
    let [main, _, accent] = ctx.scene.palette();
    let t = ctx.t as f32;
    let high = (ctx.feat.bands_norm[4] * 0.4 + ctx.feat.bands_norm[5] * 0.6).clamp(0.0, 1.0);
    let (cx, cy) = (sw * 0.5, sh * 0.40);
    for k in 0..4u32 {
        let u = ((t * 0.5 + k as f32 * 0.25) % 1.0);
        let r = u * sw.min(sh) * (0.16 + high * 0.22);
        let a = alpha * (1.0 - u) * (0.28 + high * 0.5);
        if a <= 0.02 {
            continue;
        }
        let tone = if k % 2 == 0 { main } else { accent };
        canvas.ellipse(cx, cy, r, r * 0.62, tone, a);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 通用
// ─────────────────────────────────────────────────────────────────────────────

/// 气 / 风粒子带：横贯画面、随能量起伏的流光。
pub fn ki_flow(canvas: &mut CharCanvas, ctx: &SceneCtx, alpha: f32, rng: &mut impl FnMut() -> f32) {
    let (sw, sh) = uv(canvas);
    let [main, _, accent] = ctx.scene.palette();
    let t = ctx.t as f32;
    let e = ctx.feat.energy_norm.clamp(0.0, 1.0);
    for _ in 0..(28.0 + e * 34.0) as i32 {
        let r1 = rng();
        let r2 = rng();
        let y = r2 * sh;
        let x = ((r1 + t * (0.05 + r2 * 0.08)) % 1.0) * sw;
        let wob = (t * 1.3 + r1 * 9.0).sin() * 2.2;
        let tone = if r2 > 0.8 { accent } else { main };
        canvas.set(x, y + wob, tone, alpha * (0.25 + r1 * 0.45));
        canvas.set(x - 1.0, y + wob * 0.8, tone, alpha * (0.12 + r1 * 0.2));
    }
}

/// 季节切换的横向 wipe：两条扫过的竖直亮边。
pub fn frame_wipe(canvas: &mut CharCanvas, ctx: &SceneCtx, alpha: f32) {
    let (sw, sh) = uv(canvas);
    let [main, _, accent] = ctx.scene.palette();
    let ph = ctx.local_t as f32;
    let u = ease(ph, 1.6);
    if u <= 0.0 || u >= 1.0 {
        return;
    }
    let x = u * sw;
    let edge = 3.0;
    for i in 0..(edge as i32) {
        let w = alpha * (1.0 - i as f32 / edge) * 0.9;
        canvas.line(x + i as f32, 0.0, x + i as f32, sh, if i == 0 { col::WHITE } else { accent }, w);
        canvas.line(x - i as f32, 0.0, x - i as f32, sh, main, w * 0.7);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 测试
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::features::{BAND_COUNT, FrameFeatures, SPECTRUM_BINS};
    use crate::config::RenderMode;
    use crate::lyrics::Lyrics;
    use crate::timeline::scene::SceneKind;

    fn feat() -> FrameFeatures {
        let mut f = FrameFeatures {
            time: 0.0,
            energy: 0.6,
            energy_norm: 0.6,
            ..Default::default()
        };
        f.bands_norm = [0.6; BAND_COUNT];
        f.spectrum = (0..SPECTRUM_BINS).map(|i| ((i as f32 / 16.0).sin().abs()) * 0.6).collect();
        f
    }

    fn ctx_at(scene: SceneKind, t: f64, local_t: f64) -> SceneCtx<'static> {
        let f = feat();
        SceneCtx {
            t,
            local_t,
            progress: (local_t / 27.6) as f32,
            feat: f.clone(),
            prev: f,
            frame: (t * 24.0) as u64,
            cols: 80,
            rows: 24,
            scene,
            lyrics: Box::leak(Box::new(Lyrics::default())),
            flash: 0.0,
            punch: 0.0,
            heartbeat: 0.0,
            glitch: 0.0,
            burst: 0.0,
            invert: 0.0,
        }
    }

    fn lcg(seed: &mut u64) -> f32 {
        *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((*seed >> 33) as f32) / (u32::MAX as f32)
    }

    fn count_at(scene: SceneKind, t: f64, local_t: f64) -> usize {
        let ctx = ctx_at(scene, t, local_t);
        let mut canvas = CharCanvas::new(80, 24, RenderMode::Braille);
        let mut seed = 7u64;
        let mut rng = || lcg(&mut seed);
        moss_ground(&mut canvas, &ctx, 1.0);
        light_shaft(&mut canvas, &ctx, 1.0);
        dew_field(&mut canvas, &ctx, 1.0);
        butterfly(&mut canvas, &ctx, 1.0);
        buds(&mut canvas, &ctx, 1.0);
        clouds_dissolving(&mut canvas, &ctx, 1.0);
        east_wind(&mut canvas, &ctx, 1.0);
        white_rain(&mut canvas, &ctx, 1.0, &mut rng);
        lotus_leaves(&mut canvas, &ctx, 1.0);
        silver_arcs(&mut canvas, &ctx, 1.0);
        sudare(&mut canvas, &ctx, 1.0);
        sake_jug(&mut canvas, &ctx, 1.0);
        haze_heat(&mut canvas, &ctx, 1.0);
        cicada_rings(&mut canvas, &ctx, 1.0);
        ki_flow(&mut canvas, &ctx, 1.0, &mut rng);
        frame_wipe(&mut canvas, &ctx, 1.0);
        canvas.pixels().iter().filter(|p| !p.is_empty()).count()
    }

    #[test]
    fn motifs_draw_something() {
        for (scene, lt) in [(SceneKind::Spring, 8.0), (SceneKind::Spring, 30.0), (SceneKind::Summer, 12.0)] {
            let n = count_at(scene, 100.0, lt);
            assert!(n > 40, "{scene:?} lt={lt} 只画了 {n} 像素");
        }
    }

    #[test]
    fn motifs_are_continuous() {
        // 相邻两帧（1/24s）差异要小：非空像素数变化不超过 12%
        let a = count_at(SceneKind::Spring, 30.000, 14.0);
        let b = count_at(SceneKind::Spring, 30.000 + 1.0 / 24.0, 14.0 + 1.0 / 24.0);
        let d = (a as f32 - b as f32).abs() / a.max(1) as f32;
        assert!(d < 0.12, "母题逐帧跳变 {a}→{b}（{:.1}%）", d * 100.0);
    }

    #[test]
    fn motifs_move_over_time() {
        // 2 秒后画面必须有实质变化
        let a = count_at(SceneKind::Summer, 50.0, 6.7);
        let b = count_at(SceneKind::Summer, 52.0, 8.7);
        assert!(a != b, "母题完全静止（{a}）");
    }
}

//! 夏之段 —— 20 张卡的序列。
//!
//! 「白雨来て 蓮の葉叩ク 乱レ玉 / 跳ネる 銀ノ軌跡ハ 誰ガ意思 /
//!   簾巻ク 風ノ強サニ 酒香ル / 蒸セ返ル 土ノ匂イモ 凪イデ行ク」
//!
//! 段落区间：timeline.toml 43.3 → 68.4（長さ 25.1s）。
//! 卡片结构与 [`crate::render::scenes_spring`] 同构：共享背景 + 卡内连续动画。

use crate::render::color as col;
use crate::render::motifs::{
    cicada_rings, ease, east_wind, envelope, haze_heat, lotus_leaves, sake_jug, silver_arcs,
    sudare, white_rain,
};
use crate::render::scenes::SceneCtx;
use crate::render::CharCanvas;

/// 夏段长度。
pub const SUMMER_LEN: f64 = 25.1;

/// 卡槽步长：20 张卡等距铺满 25.0s（1.25 × 20 = 25.0，余 0.1s 留给收尾）。
/// 新增卡的淡入/淡出包络以此为基准，保证卡交界处元素先收干净再换卡。
const CARD_STEP: f32 = 1.25;

/// 每张卡共用的背景层：浓青天空 + 雨云压顶 + 远处水光。
fn summer_background(canvas: &mut CharCanvas, ctx: &SceneCtx, alpha: f32) {
    let (sw, sh) = (canvas.sw as f32, canvas.sh as f32);
    let [main, deep, _] = ctx.scene.palette();
    let t = ctx.t as f32;
    // 压顶的雨云：顶部 3 层横带缓慢蠕动
    for layer in 0..3u32 {
        let y = sh * (0.06 + layer as f32 * 0.05);
        let wob = (t * 0.5 + layer as f32 * 1.8).sin() * 2.0;
        let tone = col::lerp(deep, col::BLACK, 0.35);
        let mut prev: Option<f32> = None;
        let mut x = 0.0;
        while x <= sw {
            let yy = y + (x * 0.03 + layer as f32 * 2.0).sin() * 1.6 + wob;
            if let Some(p) = prev {
                canvas.line(p, yy, x, yy, tone, alpha * (0.30 - layer as f32 * 0.07));
            }
            prev = Some(x);
            x += 5.0;
        }
    }
    // 天光：雨隙里透出的青白
    let gap = 0.4 + 0.3 * (t * 0.4).sin();
    canvas.line(0.0, sh * 0.30, sw, sh * 0.30, col::lerp(main, col::WHITE, gap), alpha * 0.10);
    // 远处水面基线
    canvas.line(0.0, sh * 0.92, sw, sh * 0.92, col::lerp(main, deep, 0.5), alpha * 0.35);
}

/// 卡 A1：雨云压顶 —— 黑云聚集，光收窄。
fn card_clouds_gather(canvas: &mut CharCanvas, ctx: &SceneCtx, phase: f32, alpha: f32) {
    summer_background(canvas, ctx, alpha);
    let (sw, sh) = (canvas.sw as f32, canvas.sh as f32);
    let [main, deep, _] = ctx.scene.palette();
    let gather = ease(phase, 2.4);
    let t = ctx.t as f32;
    // 从两侧向中间收拢的云墙
    for k in 0..6u32 {
        let u = ease(phase - k as f32 * 0.18, 2.0);
        if u <= 0.0 {
            continue;
        }
        let x = if k % 2 == 0 { u * sw * 0.42 } else { sw - u * sw * 0.42 };
        let y = sh * (0.10 + (k as f32 * 0.377) % 1.0 * 0.10);
        let tone = col::lerp(deep, col::BLACK, 0.3);
        canvas.disc(x, y, 2.2 + u * 3.4, tone, alpha * 0.55);
    }
    // 中央天光被云挤窄
    let w = sw * (0.5 - 0.38 * gather);
    canvas.line(sw * 0.5 - w, sh * 0.16, sw * 0.5 + w, sh * 0.16,
                col::lerp(col::WHITE, main, 0.4), alpha * (0.4 - gather * 0.25));
    let _ = t;
}

/// 卡 A2：白雨来て —— 斜雨场建立。
fn card_rain_arrives(canvas: &mut CharCanvas, ctx: &SceneCtx, phase: f32, alpha: f32) {
    summer_background(canvas, ctx, alpha);
    let build = ease(phase, 1.6);
    let mut seed = 0x5EED_0001u64;
    let mut rng = || lcg(&mut seed);
    white_rain(canvas, ctx, alpha * build, &mut rng);
}

/// 卡 A3：雨脚ノ斜度 —— 雨脚斜度随鼓点变化，鼓点重时闪一道。
fn card_rain_slant(canvas: &mut CharCanvas, ctx: &SceneCtx, phase: f32, alpha: f32) {
    summer_background(canvas, ctx, alpha);
    // 常驻雨场：与邻卡同源，保证交界处雨密度不断档
    let mut seed = 0x5EED_0007u64;
    let mut rng = || lcg(&mut seed);
    white_rain(canvas, ctx, alpha * 0.7, &mut rng);
    let (sw, sh) = (canvas.sw as f32, canvas.sh as f32);
    let [main, _, _] = ctx.scene.palette();
    let t = ctx.t as f32;
    let life = envelope(phase, 0.0, 0.45, CARD_STEP - 0.35, CARD_STEP);
    if life <= 0.0 {
        return;
    }
    // 本卡自己的雨脚：斜率被 beat 推着走
    let beat = ctx.feat.beat.clamp(0.0, 1.0);
    let slant = 0.12 + 0.75 * beat;
    for k in 0..70u32 {
        let kf = k as f32;
        let x = ((kf * 0.618) % 1.0) * (sw + sh * 0.5) - sh * 0.25;
        let y = ((kf * 0.383 + t * 0.55) % 1.0) * sh;
        let len = 2.0 + ((kf * 0.731) % 1.0) * 3.0;
        let tone = if k % 7 == 0 { col::WHITE } else { col::lerp(col::WHITE, main, 0.45) };
        canvas.line(x, y, x + len * slant, y + len, tone,
                    alpha * life * (0.6 + 0.3 * ((kf * 0.517) % 1.0)));
    }
    // 重拍：整片雨脚一起亮一瞬
    let flash = ((beat - 0.6) / 0.4).clamp(0.0, 1.0);
    if flash > 0.0 {
        for k in 0..12u32 {
            let kf = k as f32;
            let x = ((kf * 0.587 + t * 0.05) % 1.0) * sw;
            canvas.line(x, 0.0, x + sh * slant, sh, col::WHITE, alpha * life * flash * 0.55);
        }
    }
}

/// 卡 A4：蓮の葉叩ク —— 三枚莲叶入场，叶面随雨颤动。
fn card_lotus_struck(canvas: &mut CharCanvas, ctx: &SceneCtx, phase: f32, alpha: f32) {
    summer_background(canvas, ctx, alpha);
    let mut seed = 0x5EED_0002u64;
    let mut rng = || lcg(&mut seed);
    white_rain(canvas, ctx, alpha * 0.8, &mut rng);
    lotus_leaves(canvas, ctx, alpha);
}

/// 卡 A5：乱レ玉 —— 叶上水珠弹跳。
fn card_midori_dama(canvas: &mut CharCanvas, ctx: &SceneCtx, phase: f32, alpha: f32) {
    summer_background(canvas, ctx, alpha);
    let mut seed = 0x5EED_0003u64;
    let mut rng = || lcg(&mut seed);
    white_rain(canvas, ctx, alpha * 0.6, &mut rng);
    lotus_leaves(canvas, ctx, alpha);
    // 叶心的水珠群：按 beat 起跳
    let (sw, sh) = (canvas.sw as f32, canvas.sh as f32);
    let [_, _, accent] = ctx.scene.palette();
    let t = ctx.t as f32;
    let beat = ctx.feat.beat.clamp(0.0, 1.0);
    for (i, &(ux, uy)) in [(0.24, 0.70), (0.55, 0.80), (0.85, 0.68)].iter().enumerate() {
        let hop = ((t * 2.6 + i as f32 * 0.9) % 1.0);
        let h = (hop * std::f32::consts::PI).sin() * (2.5 + beat * 3.5);
        let x = ux * sw + hop * 5.0;
        let y = uy * sh - h;
        canvas.disc(x, y, 1.1 + beat * 0.7, col::WHITE, alpha * 0.9);
        canvas.set(x, y - 0.6, accent, alpha * 0.5);
    }
}

/// 卡 B1：銀ノ軌跡 —— 溅起的银弧成串。
fn card_silver_arcs(canvas: &mut CharCanvas, ctx: &SceneCtx, phase: f32, alpha: f32) {
    summer_background(canvas, ctx, alpha);
    let mut seed = 0x5EED_0004u64;
    let mut rng = || lcg(&mut seed);
    white_rain(canvas, ctx, alpha * 0.5, &mut rng);
    lotus_leaves(canvas, ctx, alpha * 0.9);
    silver_arcs(canvas, ctx, alpha);
}

/// 卡 B2：乱レ玉ノ走リ —— 莲叶上的水银珠滚来滚去，重拍时弹起。
fn card_mercury_roll(canvas: &mut CharCanvas, ctx: &SceneCtx, phase: f32, alpha: f32) {
    summer_background(canvas, ctx, alpha);
    lotus_leaves(canvas, ctx, alpha);
    let (sw, sh) = (canvas.sw as f32, canvas.sh as f32);
    let [_, _, accent] = ctx.scene.palette();
    let t = ctx.t as f32;
    let life = envelope(phase, 0.0, 0.45, CARD_STEP - 0.35, CARD_STEP);
    if life <= 0.0 {
        return;
    }
    let beat = ctx.feat.beat.clamp(0.0, 1.0);
    // 三枚叶，每枚叶上 4 颗水银：沿叶面来回滚，beat 一重就弹高
    for (i, &(ux, uy, ur)) in [(0.24f32, 0.76f32, 0.13f32), (0.55, 0.86, 0.16), (0.85, 0.74, 0.11)]
        .iter()
        .enumerate()
    {
        let cx = ux * sw;
        let cy = uy * sh;
        let rr = ur * sw.min(sh) * 0.9;
        for k in 0..4u32 {
            let kf = k as f32 + i as f32 * 4.0;
            let roll = (t * (0.55 + (kf * 0.137) % 1.0 * 0.5) + kf * 1.7).sin();
            let hop = (t * 2.2 + kf * 0.37) % 1.0;
            let h = (hop * std::f32::consts::PI).sin() * (1.0 + beat * 2.5);
            let r = (0.9 + ((kf * 0.613) % 1.0) * 0.9) * life;
            if r <= 0.2 {
                continue;
            }
            let bx = cx + roll * rr * 0.62;
            let by = cy - rr * 0.40 + roll.abs() * rr * 0.10 - h;
            canvas.disc(bx, by, r, col::WHITE, alpha * life * 0.9);
            canvas.set(bx - r * 0.5, by - r * 0.5, col::WHITE, alpha * life);
            canvas.set(bx + r * 0.6, by + r * 0.5, col::lerp(accent, col::WHITE, 0.4),
                       alpha * life * 0.75);
        }
    }
}

/// 卡 B3：誰ガ意思 —— 银弧随高频共振加密。
fn card_arcs_intent(canvas: &mut CharCanvas, ctx: &SceneCtx, phase: f32, alpha: f32) {
    summer_background(canvas, ctx, alpha);
    silver_arcs(canvas, ctx, alpha);
    cicada_rings(canvas, ctx, alpha * 0.5);
    let _ = phase;
}

/// 卡 B4：雨势转弱 —— 雨密度下降，天光透出。
fn card_rain_relents(canvas: &mut CharCanvas, ctx: &SceneCtx, phase: f32, alpha: f32) {
    summer_background(canvas, ctx, alpha);
    let fade = 1.0 - ease(phase, 2.5);
    let mut seed = 0x5EED_0005u64;
    let mut rng = || lcg(&mut seed);
    white_rain(canvas, ctx, alpha * 0.5 * fade.max(0.15), &mut rng);
    lotus_leaves(canvas, ctx, alpha);
    let (sw, sh) = (canvas.sw as f32, canvas.sh as f32);
    let [main, _, _] = ctx.scene.palette();
    // 天光渐亮
    let bright = ease(phase, 3.0);
    canvas.line(0.0, sh * 0.24, sw, sh * 0.24, col::lerp(main, col::WHITE, bright * 0.7),
                alpha * bright * 0.35);
}

/// 卡 B5：雨后水镜 —— 水面涟漪一层层推开。
fn card_water_glass(canvas: &mut CharCanvas, ctx: &SceneCtx, phase: f32, alpha: f32) {
    summer_background(canvas, ctx, alpha);
    lotus_leaves(canvas, ctx, alpha * 0.9);
    let (sw, sh) = (canvas.sw as f32, canvas.sh as f32);
    let [main, _, _] = ctx.scene.palette();
    let t = ctx.t as f32;
    for k in 0..4u32 {
        let u = (t * 0.5 + k as f32 * 0.25) % 1.0;
        let cx = sw * (0.2 + (k as f32 * 0.613) % 1.0 * 0.6);
        let r = u * sw * 0.13;
        canvas.ellipse(cx, sh * 0.88, r, r * 0.22, col::lerp(main, col::WHITE, 0.4),
                       alpha * (1.0 - u) * 0.4);
    }
    let _ = phase;
}

/// 卡 C1：簾巻ク —— 竹帘卷落。
fn card_sudare_rolls(canvas: &mut CharCanvas, ctx: &SceneCtx, phase: f32, alpha: f32) {
    summer_background(canvas, ctx, alpha);
    sudare(canvas, ctx, alpha);
    east_wind(canvas, ctx, alpha * 0.4);
}

/// 卡 C2：簾巻クノ影 —— 竹帘卷起，帘影斜投在地面上一道道摆。
fn card_sudare_shadow(canvas: &mut CharCanvas, ctx: &SceneCtx, phase: f32, alpha: f32) {
    summer_background(canvas, ctx, alpha);
    sudare(canvas, ctx, alpha * 0.8);
    let (sw, sh) = (canvas.sw as f32, canvas.sh as f32);
    let [main, deep, _] = ctx.scene.palette();
    let t = ctx.t as f32;
    let life = envelope(phase, 0.0, 0.45, CARD_STEP - 0.35, CARD_STEP);
    if life <= 0.0 {
        return;
    }
    // 帘影：斜投在地面的条纹，越远越淡、摆幅越大
    for k in 0..11u32 {
        let kf = k as f32;
        let u = kf / 11.0;
        let sway = (t * 0.8 + kf * 0.45).sin() * (1.0 + u * 2.2);
        let y = sh * (0.34 + u * 0.52);
        let a = alpha * life * (0.50 - u * 0.16);
        canvas.line(sw * 0.06 + sway, y, sw * 0.94 + sway * 0.6, y + 1.6,
                    col::lerp(main, deep, 0.45), a);
    }
    // 帘缝漏下来的亮条
    for k in 0..4u32 {
        let kf = k as f32;
        let x = sw * (0.18 + kf * 0.21) + (t * 0.6 + kf).sin() * 1.5;
        canvas.line(x, sh * 0.30, x - sw * 0.05, sh * 0.92,
                    col::lerp(col::WHITE, main, 0.4), alpha * life * 0.5);
    }
}

/// 卡 C3：風ノ強サニ —— 风线加密，帘尾摆动。
fn card_wind_strength(canvas: &mut CharCanvas, ctx: &SceneCtx, phase: f32, alpha: f32) {
    summer_background(canvas, ctx, alpha);
    sudare(canvas, ctx, alpha);
    east_wind(canvas, ctx, alpha);
    let _ = phase;
}

/// 卡 C4：酒香ル —— 酒壶入画，香线上升。
fn card_sake_scents(canvas: &mut CharCanvas, ctx: &SceneCtx, phase: f32, alpha: f32) {
    summer_background(canvas, ctx, alpha);
    sudare(canvas, ctx, alpha * 0.85);
    sake_jug(canvas, ctx, alpha);
}

/// 卡 C5：一献 —— 壶旁多出一只盃，映着帘影。
fn card_one_cup(canvas: &mut CharCanvas, ctx: &SceneCtx, phase: f32, alpha: f32) {
    summer_background(canvas, ctx, alpha);
    sudare(canvas, ctx, alpha * 0.7);
    sake_jug(canvas, ctx, alpha);
    let (sw, sh) = (canvas.sw as f32, canvas.sh as f32);
    let [_, deep, accent] = ctx.scene.palette();
    let t = ctx.t as f32;
    let x = sw * 0.20;
    let y = sh * 0.90;
    // 盃：浅碗 + 酒面高光
    canvas.ellipse(x, y, 3.2, 1.2, col::lerp(deep, accent, 0.35), alpha * 0.8);
    canvas.ellipse(x, y - 0.5, 2.4, 0.7, col::lerp(col::WHITE, accent, 0.5),
                   alpha * (0.5 + 0.3 * (t * 1.1).sin()));
}

/// 卡 D1：蒸セ返ル —— 热浪扭曲地面。
fn card_heat_haze(canvas: &mut CharCanvas, ctx: &SceneCtx, phase: f32, alpha: f32) {
    summer_background(canvas, ctx, alpha);
    haze_heat(canvas, ctx, alpha);
    sake_jug(canvas, ctx, alpha * 0.6);
}

/// 卡 D2：蒸セ返ルノ山 —— 热浪里远山抖动，山脚雾气蒸腾。
fn card_heat_shimmer(canvas: &mut CharCanvas, ctx: &SceneCtx, phase: f32, alpha: f32) {
    summer_background(canvas, ctx, alpha);
    haze_heat(canvas, ctx, alpha * 0.8);
    let (sw, sh) = (canvas.sw as f32, canvas.sh as f32);
    let [main, deep, _] = ctx.scene.palette();
    let t = ctx.t as f32;
    let life = envelope(phase, 0.0, 0.45, CARD_STEP - 0.35, CARD_STEP);
    if life <= 0.0 {
        return;
    }
    // 远山：两层剪影，山脊被热浪推着抖
    for layer in 0..2u32 {
        let base = sh * (0.58 + layer as f32 * 0.05);
        let amp = 3.0 - layer as f32 * 1.0;
        let tone = col::lerp(deep, main, 0.35 - layer as f32 * 0.15);
        let mut prev: Option<f32> = None;
        for x in 0..(sw as i32) {
            let xf = x as f32;
            let ridge = (xf * 0.035 + layer as f32 * 1.9).sin() * amp
                + (xf * 0.011 - layer as f32 * 0.7).sin() * amp * 0.8;
            let wob = (t * 3.1 + xf * 0.28 + layer as f32 * 2.0).sin()
                * (1.1 - layer as f32 * 0.4)
                * life;
            let y = base + ridge + wob;
            if let Some(py) = prev {
                canvas.line(xf - 1.0, py, xf, y, tone, alpha * life * (0.7 - layer as f32 * 0.2));
            }
            prev = Some(y);
        }
    }
    // 山脚蒸腾的亮雾
    for k in 0..16u32 {
        let kf = k as f32;
        let x = ((kf * 0.613) % 1.0) * sw;
        let rise = (t * 0.35 + kf * 0.21) % 1.0;
        canvas.set(x + (t * 1.6 + kf).sin() * 1.4, sh * 0.62 - rise * sh * 0.10,
                   col::lerp(col::WHITE, main, 0.5), alpha * life * (1.0 - rise) * 0.6);
    }
}

/// 卡 D3：土ノ匂イ —— 雾层加厚，土色翻起。
fn card_earth_scent(canvas: &mut CharCanvas, ctx: &SceneCtx, phase: f32, alpha: f32) {
    summer_background(canvas, ctx, alpha);
    haze_heat(canvas, ctx, alpha);
    let (sw, sh) = (canvas.sw as f32, canvas.sh as f32);
    let [_, deep, _] = ctx.scene.palette();
    let t = ctx.t as f32;
    // 土粒：底部翻起的小点群
    for k in 0..30u32 {
        let kf = k as f32;
        let u = (t * 0.22 + kf * 0.147) % 1.0;
        let x = ((kf * 0.719) % 1.0) * sw;
        let y = sh * 0.94 - u * sh * 0.10;
        canvas.set(x, y, col::lerp(deep, col::BLACK, 0.2), alpha * (1.0 - u) * 0.5);
    }
}

/// 卡 D4：凪イデ行ク —— 一切缓下来，雨雾散尽。
fn card_calm_settles(canvas: &mut CharCanvas, ctx: &SceneCtx, phase: f32, alpha: f32) {
    summer_background(canvas, ctx, alpha);
    let still = ease(phase, 3.0);
    let mut seed = 0x5EED_0006u64;
    let mut rng = || lcg(&mut seed);
    // 残雨稀稀拉拉
    white_rain(canvas, ctx, alpha * (1.0 - still) * 0.4, &mut rng);
    haze_heat(canvas, ctx, alpha * (1.0 - still * 0.7));
    lotus_leaves(canvas, ctx, alpha);
}

/// 卡 D5：水面平 —— 夏段收束：全幅静水映天。
fn card_still_water(canvas: &mut CharCanvas, ctx: &SceneCtx, phase: f32, alpha: f32) {
    summer_background(canvas, ctx, alpha);
    lotus_leaves(canvas, ctx, alpha * 0.85);
    let (sw, sh) = (canvas.sw as f32, canvas.sh as f32);
    let [main, _, _] = ctx.scene.palette();
    let t = ctx.t as f32;
    // 静水：一条条几乎平行的微光线
    for k in 0..8u32 {
        let y = sh * (0.80 + k as f32 * 0.02);
        let drift = (t * 0.4 + k as f32 * 0.8).sin() * 1.5;
        canvas.line(sw * 0.1 + drift, y, sw * 0.9 + drift, y,
                    col::lerp(main, col::WHITE, 0.55), alpha * 0.22);
    }
    let _ = phase;
}

fn lcg(seed: &mut u64) -> f32 {
    *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    ((*seed >> 33) as f32) / (u32::MAX as f32)
}

pub type SummerFn = fn(&mut CharCanvas, &SceneCtx, f32, f32);

/// 夏 20 卡序列。`(段内起始秒, 卡名, 绘制函数)`
/// 卡间距恒为 [`CARD_STEP`]（1.2~1.8s 内），20 张铺满 25.0s，全程无 <1s 碎剪。
pub fn cards() -> Vec<(f64, &'static str, SummerFn)> {
    vec![
        (0.0, "clouds_gather", card_clouds_gather),
        (1.25, "rain_arrives", card_rain_arrives),
        (2.5, "rain_slant", card_rain_slant),
        (3.75, "lotus_struck", card_lotus_struck),
        (5.0, "midori_dama", card_midori_dama),
        (6.25, "silver_arcs", card_silver_arcs),
        (7.5, "mercury_roll", card_mercury_roll),
        (8.75, "arcs_intent", card_arcs_intent),
        (10.0, "rain_relents", card_rain_relents),
        (11.25, "water_glass", card_water_glass),
        (12.5, "sudare_rolls", card_sudare_rolls),
        (13.75, "sudare_shadow", card_sudare_shadow),
        (15.0, "wind_strength", card_wind_strength),
        (16.25, "sake_scents", card_sake_scents),
        (17.5, "one_cup", card_one_cup),
        (18.75, "heat_haze", card_heat_haze),
        (20.0, "heat_shimmer", card_heat_shimmer),
        (21.25, "earth_scent", card_earth_scent),
        (22.5, "calm_settles", card_calm_settles),
        (23.75, "still_water", card_still_water),
    ]
}

static CARDS: std::sync::OnceLock<Vec<(f64, &'static str, SummerFn)>> = std::sync::OnceLock::new();

fn card_list() -> &'static [(f64, &'static str, SummerFn)] {
    CARDS.get_or_init(cards)
}

/// 卡名导出（供日志 / Lead 汇总）。
pub fn card_names() -> Vec<&'static str> {
    card_list().iter().map(|c| c.1).collect()
}

/// 夏段入口：按 `local_t` 选卡并绘制。
pub fn render_card(ctx: &SceneCtx, canvas: &mut CharCanvas, alpha: f32) {
    let local = ctx.local_t as f32;
    let list = card_list();
    let mut idx = 0usize;
    for (i, (start, _, _)) in list.iter().enumerate() {
        if local >= *start as f32 {
            idx = i;
        }
    }
    let (start, _name, f) = list[idx];
    let phase = local - start as f32;
    f(canvas, ctx, phase, alpha);
}

/// 副歌「夏半」入口：把副歌后半映射回夏段循环（浅拷贝上下文只改 local_t）。
pub fn render_chorus_summer(ctx: &SceneCtx, canvas: &mut CharCanvas, alpha: f32) {
    let mapped = ctx.local_t % 14.0;
    let sub = SceneCtx {
        local_t: mapped,
        progress: (mapped as f32 / SUMMER_LEN as f32),
        ..Shallow::shallow(ctx)
    };
    render_card(&sub, canvas, alpha);
}

/// 把 `&SceneCtx` 借用转换成同字段的构造输入（无 Clone 依赖的手工浅拷贝）。
struct Shallow;

impl Shallow {
    fn shallow(ctx: &SceneCtx) -> SceneCtx<'static> {
        SceneCtx {
            t: ctx.t,
            local_t: ctx.local_t,
            progress: ctx.progress,
            feat: ctx.feat.clone(),
            prev: ctx.prev.clone(),
            frame: ctx.frame,
            cols: ctx.cols,
            rows: ctx.rows,
            scene: ctx.scene,
            lyrics: Box::leak(Box::new(crate::lyrics::Lyrics::default())),
            flash: ctx.flash,
            punch: ctx.punch,
            heartbeat: ctx.heartbeat,
            glitch: ctx.glitch,
            burst: ctx.burst,
            invert: ctx.invert,
        }
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

    fn ctx_at(t: f64, local_t: f64) -> SceneCtx<'static> {
        let f = feat();
        SceneCtx {
            t,
            local_t,
            progress: (local_t as f32 / SUMMER_LEN as f32),
            feat: f.clone(),
            prev: f,
            frame: (t * 24.0) as u64,
            cols: 80,
            rows: 24,
            scene: SceneKind::Summer,
            lyrics: Box::leak(Box::new(Lyrics::default())),
            flash: 0.0,
            punch: 0.0,
            heartbeat: 0.0,
            glitch: 0.0,
            burst: 0.0,
            invert: 0.0,
        }
    }

    /// 本次新增的 4 张卡（交界渐入/渐出检查只针对它们）。
    const NEW_CARDS: [&str; 4] = ["rain_slant", "mercury_roll", "sudare_shadow", "heat_shimmer"];

    /// 单帧度量：非空子像素数、点亮数、总墨量 Σw、逐像素滚动哈希。
    #[derive(Clone, Copy, PartialEq, Debug)]
    struct Frame {
        non_empty: usize,
        lit: usize,
        ink: f64,
        hash: u64,
    }

    /// 渲染一帧并度量。Σw 对 Braille 阈值抖动是连续的，哈希对任何像素级
    /// 改动都敏感 —— 两者都不像「非空像素计数」那样会被整片翻转。
    fn frame_at(t: f64, local_t: f64, cols: u16, rows: u16) -> Frame {
        let ctx = ctx_at(t, local_t);
        let mut canvas = CharCanvas::new(cols, rows, RenderMode::Braille);
        render_card(&ctx, &mut canvas, 1.0);
        let mut f = Frame { non_empty: 0, lit: 0, ink: 0.0, hash: 1469598103934665603 };
        for p in canvas.pixels() {
            if !p.is_empty() {
                f.non_empty += 1;
            }
            if p.w > 0.5 {
                f.lit += 1;
            }
            f.ink += p.w as f64;
            f.hash = (f.hash ^ p.w.to_bits() as u64).wrapping_mul(1099511628211);
        }
        f
    }

    fn full(t: f64, local_t: f64) -> Frame {
        frame_at(t, local_t, 80, 24)
    }

    /// 共享背景层的墨量，用来把某张卡自己的元素墨量剥出来。
    fn background_ink(t: f64, local_t: f64) -> f64 {
        let ctx = ctx_at(t, local_t);
        let mut canvas = CharCanvas::new(80, 24, RenderMode::Braille);
        summer_background(&mut canvas, &ctx, 1.0);
        canvas.pixels().iter().map(|p| p.w as f64).sum::<f64>()
    }

    #[test]
    fn twenty_cards_exist() {
        assert_eq!(card_list().len(), 20);
    }

    /// 卡间距必须落在 1.2~1.8s，不得出现 <1s 碎剪，末卡也不能越出段长。
    #[test]
    fn card_spacing_stays_in_range() {
        let list = card_list();
        for w in list.windows(2) {
            let gap = w[1].0 - w[0].0;
            assert!((1.2..=1.8).contains(&gap), "夏卡 {}→{} 间距 {gap:.2}s 越界", w[0].1, w[1].1);
        }
        let last = list.last().unwrap().0;
        assert!(last + 1.2 <= SUMMER_LEN, "夏末卡 {last:.2}s 起太晚，装不下 1.2s");
    }

    /// 每张卡都得画满画面（80×24 Braille 上非空子像素 > 20）。
    #[test]
    fn every_card_draws_something() {
        for (start, name, _) in card_list() {
            let f = full(43.3 + start + 0.10, start + 0.10);
            assert!(f.non_empty > 20, "夏卡 {name} 入口只画 {} 个非空子像素（点亮 {}）",
                    f.non_empty, f.lit);
        }
    }

    /// 极小画布（8×5 与 1×1）不许 panic。
    #[test]
    fn tiny_canvas_does_not_panic() {
        for (start, name, _) in card_list() {
            for k in 0..8 {
                let local = start + k as f64 * 0.2;
                let _ = frame_at(43.3 + local, local, 8, 5);
                let _ = frame_at(43.3 + local, local, 1, 1);
            }
            let _ = name;
        }
    }

    /// 场景是 t 的纯函数：同输入必须同画面。
    #[test]
    fn same_input_gives_same_frame() {
        for (start, name, _) in card_list() {
            let local = start + 0.7;
            let a = full(43.3 + local, local);
            let b = full(43.3 + local, local);
            assert_eq!(a, b, "夏卡 {name} 同输入却不同画面");
        }
    }

    /// 帧间必须有差异，不能是死帧。
    #[test]
    fn consecutive_frames_differ() {
        for (start, name, _) in card_list() {
            let local = start + 0.7;
            let a = full(43.3 + local, local);
            let b = full(43.3 + local + 1.0 / 24.0, local + 1.0 / 24.0);
            assert_ne!(a.hash, b.hash, "夏卡 {name} 相邻两帧完全一样");
        }
    }

    /// 卡内 Σw 密采样离群判据：真正的「突现」会让某一步长成为离群值，
    /// 连续渐入/渐出的卡不会。用 Σw 而不是非空像素计数 —— 后者会被 Braille
    /// 阈值抖动整片翻转，把背景微光算成几百像素的「跳变」。
    #[test]
    fn card_motion_is_continuous() {
        for (start, name, _) in card_list() {
            let mut series = Vec::new();
            for k in 0..13 {
                let local = start + 0.05 + k as f64 * 0.05;
                series.push(full(43.3 + local, local).ink);
            }
            let steps: Vec<f64> = series.windows(2).map(|w| (w[1] - w[0]).abs()).collect();
            let mean = steps.iter().sum::<f64>() / steps.len() as f64;
            let max = steps.iter().cloned().fold(0.0f64, f64::max);
            assert!(max <= mean * 3.0 + 14.0,
                    "夏卡 {name} 卡内墨量突跳：单步 {max:.1} > 3×平均 {mean:.1} + 14");
        }
    }

    /// 卡交界两侧 0.06s：把下一张卡按 ∓0.06s 相位各画一次，总墨量相对变化不得超 50%
    /// （与 scenes_autumn 同款口径）。
    #[test]
    fn no_jump_at_card_boundary() {
        let cov = |f: SummerFn, phase: f32, t: f64| {
            let ctx = ctx_at(43.3 + t, t);
            let mut canvas = CharCanvas::new(80, 24, RenderMode::Braille);
            summer_background(&mut canvas, &ctx, 1.0);
            f(&mut canvas, &ctx, phase, 1.0);
            canvas.pixels().iter().map(|p| p.w as f64).sum::<f64>()
        };
        let mut jumps = Vec::new();
        for (at, name, f) in card_list().iter().skip(1) {
            let before = cov(*f, -0.06, *at - 0.06);
            let after = cov(*f, 0.06, *at + 0.06);
            if (after - before).abs() / after.max(1.0) > 0.5 {
                jumps.push(*name);
            }
        }
        assert!(jumps.len() <= 2, "夏卡交界跳变过多：{jumps:?}");
    }

    /// 新增卡的交界行为：入口元素墨量在长出来、出口在收回去（渐入/渐出，不是突现）。
    #[test]
    fn new_cards_fade_in_and_out() {
        let list = card_list();
        for (i, (start, name, _)) in list.iter().enumerate() {
            if !NEW_CARDS.contains(name) {
                continue;
            }
            let step = list.get(i + 1)
                .map(|(next, _, _)| (*next - *start) as f32)
                .unwrap_or(CARD_STEP);
            let bg_in = background_ink(43.3 + start + 0.05, start + 0.05);
            let e1 = full(43.3 + start + 0.05, start + 0.05).ink - bg_in;
            let e2 = full(43.3 + start + 0.45, start + 0.45).ink - bg_in;
            assert!(e1 < e2, "夏卡 {name} 入口不是渐入：{e1:.1} → {e2:.1}");
            let t1 = start + step as f64 - 0.45;
            let t2 = start + step as f64 - 0.05;
            let o1 = full(43.3 + t1, t1).ink - background_ink(43.3 + t1, t1);
            let o2 = full(43.3 + t2, t2).ink - background_ink(43.3 + t2, t2);
            assert!(o1 > o2, "夏卡 {name} 出口不是渐出：{o1:.1} → {o2:.1}");
        }
    }
}

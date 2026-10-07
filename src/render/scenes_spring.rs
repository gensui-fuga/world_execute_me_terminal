//! 春之段 —— 20 张卡的序列。
//!
//! 「薄緑 苔の産毛に 日が射して / 朝露を 飲み干す様は 生キル音 /
//!   一羽の蝶 蕾の重み 試す午後 / 東風吹けば 雲は解ケテ 水ニナル」
//!
//! 每张卡 `(段内起始秒, 名, 绘制函数)`，绘制函数拿到 `phase = local_t - start`
//! 自己安排动作；卡与卡之间共享 [`spring_background`] 保证背景连贯。

use crate::render::color as col;
use crate::render::motifs::{
    buds, butterfly, clouds_dissolving, dew_field, ease, east_wind, envelope, light_shaft,
    moss_ground,
};
use crate::render::scenes::SceneCtx;
use crate::render::CharCanvas;

/// 春段长度（timeline.toml: 15.7 → 43.3）。
pub const SPRING_LEN: f64 = 27.6;

/// 卡槽步长：20 张卡等距铺满 [`SPRING_LEN`]（1.38 × 20 = 27.6）。
/// 新增卡的淡入/淡出包络以此为基准，保证卡交界处元素先收干净再换卡。
const CARD_STEP: f32 = 1.38;

/// 卡名导出（供日志 / Lead 汇总）。
pub fn card_names() -> Vec<&'static str> {
    card_list().iter().map(|c| c.1).collect()
}

/// 每张卡共用的背景层：天空渐晕 + 苔地 + 常驻微光。
fn spring_background(canvas: &mut CharCanvas, ctx: &SceneCtx, alpha: f32) {
    let (sw, sh) = (canvas.sw as f32, canvas.sh as f32);
    let [main, deep, _] = ctx.scene.palette();
    let t = ctx.t as f32;
    // 天空：从上往下由深到浅的呼吸渐变
    let breathe = 0.5 + 0.5 * (t * 0.23).sin();
    let rows = 14;
    for r in 0..rows {
        let u = r as f32 / rows as f32;
        let y = u * sh * 0.72;
        let tone = col::lerp(col::lerp(deep, col::BLACK, 0.55), main, (1.0 - u) * (0.30 + 0.14 * breathe));
        canvas.line(0.0, y, sw, y, tone, alpha * 0.16);
    }
    moss_ground(canvas, ctx, alpha * 0.9);
}

/// 卡 A1：空镜 —— 只有苔地苏醒，绒毛冒头。
fn card_moss_awakens(canvas: &mut CharCanvas, ctx: &SceneCtx, phase: f32, alpha: f32) {
    spring_background(canvas, ctx, alpha);
    let wake = ease(phase, 4.0);
    let (sw, sh) = (canvas.sw as f32, canvas.sh as f32);
    // 一圈从地平线漾开的呼吸环
    let [main, _, _] = ctx.scene.palette();
    let r = wake * sw.min(sh) * 0.4;
    if wake > 0.05 {
        canvas.ellipse(sw * 0.5, sh * 0.72, r, r * 0.30, col::lerp(main, col::WHITE, 0.4), alpha * (1.0 - wake) * 0.5);
    }
}

/// 卡 A2：日が射して —— 光带落下。
fn card_light_lands(canvas: &mut CharCanvas, ctx: &SceneCtx, phase: f32, alpha: f32) {
    spring_background(canvas, ctx, alpha);
    let a = alpha * ease(phase, 2.5);
    light_shaft(canvas, ctx, a);
}

/// 卡 A3：産毛 —— 特写绒毛随光闪。
fn card_fuzz_glint(canvas: &mut CharCanvas, ctx: &SceneCtx, phase: f32, alpha: f32) {
    spring_background(canvas, ctx, alpha);
    light_shaft(canvas, ctx, alpha * 0.7);
    let (sw, sh) = (canvas.sw as f32, canvas.sh as f32);
    let [main, _, accent] = ctx.scene.palette();
    let t = ctx.t as f32;
    // 中景绒毛带：密集的闪点
    for k in 0..46u32 {
        let kf = k as f32;
        let x = ((kf * 0.613) % 1.0) * sw;
        let y = sh * (0.64 + ((kf * 0.419) % 1.0) * 0.24);
        let tw = (t * 2.1 + kf * 1.7).sin();
        let tone = if tw > 0.7 { accent } else { main };
        canvas.line(x, y, x, y - 2.0 - tw.abs() * 1.6, tone, alpha * (0.3 + tw.abs() * 0.4));
    }
}

/// 卡 A4：薄緑满屏 —— 整幅绿色调呼吸到最亮。
fn card_green_breath(canvas: &mut CharCanvas, ctx: &SceneCtx, phase: f32, alpha: f32) {
    let (sw, sh) = (canvas.sw as f32, canvas.sh as f32);
    let [main, deep, _] = ctx.scene.palette();
    let t = ctx.t as f32;
    let pulse = 0.55 + 0.45 * (t * 0.9).sin();
    let rows = 18;
    for r in 0..rows {
        let u = r as f32 / rows as f32;
        let y = u * sh;
        let tone = col::lerp(deep, main, (1.0 - u) * pulse);
        canvas.line(0.0, y, sw, y, tone, alpha * 0.22);
    }
    spring_background(canvas, ctx, alpha * 0.5);
    light_shaft(canvas, ctx, alpha * 0.5);
}

/// 卡 A5：日が射して —— 光斑落在苔面上，一粒粒轻轻跳。
fn card_light_dapple(canvas: &mut CharCanvas, ctx: &SceneCtx, phase: f32, alpha: f32) {
    spring_background(canvas, ctx, alpha);
    light_shaft(canvas, ctx, alpha * 0.8);
    let (sw, sh) = (canvas.sw as f32, canvas.sh as f32);
    let [main, _, accent] = ctx.scene.palette();
    let t = ctx.t as f32;
    let life = envelope(phase, 0.0, 0.45, CARD_STEP - 0.35, CARD_STEP);
    if life <= 0.0 {
        return;
    }
    // 光斑：每粒自带相位，在苔面上起落；大小随包络长出来再收回去
    for k in 0..22u32 {
        let kf = k as f32;
        let x = ((kf * 0.618) % 1.0) * sw;
        let gy = sh * (0.70 + ((kf * 0.377) % 1.0) * 0.22);
        let hop = (t * 0.9 + kf * 0.37) % 1.0;
        let r = (0.9 + ((kf * 0.913) % 1.0) * 1.3) * life;
        if r <= 0.15 {
            continue;
        }
        let y = gy - hop * 3.0 * life;
        let tone = if k % 5 == 0 { accent } else { col::lerp(col::WHITE, main, 0.35) };
        canvas.disc(x, y, r, tone, alpha * life * (0.6 + 0.3 * hop));
        canvas.set(x - r * 0.5, y - r * 0.5, col::WHITE, alpha * life * 0.8);
    }
}

/// 卡 B1：朝露凝出。
fn card_dew_born(canvas: &mut CharCanvas, ctx: &SceneCtx, phase: f32, alpha: f32) {
    spring_background(canvas, ctx, alpha);
    let _ = phase;
    dew_field(canvas, ctx, alpha);
}

/// 卡 B2：生キル音 —— 露珠随低音脉动的涟漪。
fn card_dew_pulse(canvas: &mut CharCanvas, ctx: &SceneCtx, phase: f32, alpha: f32) {
    spring_background(canvas, ctx, alpha);
    dew_field(canvas, ctx, alpha * 0.85);
    let (sw, sh) = (canvas.sw as f32, canvas.sh as f32);
    let [main, _, _] = ctx.scene.palette();
    let low = (ctx.feat.bands_norm[0] * 0.5 + ctx.feat.bands_norm[1] * 0.5).clamp(0.0, 1.0);
    for k in 0..3u32 {
        let u = ((ctx.t as f32 * 0.7 + k as f32 * 0.33) % 1.0);
        let r = u * sw.min(sh) * 0.22 * (0.5 + low);
        canvas.ellipse(sw * 0.5, sh * 0.74, r, r * 0.32, col::lerp(main, col::WHITE, 0.5),
                       alpha * (1.0 - u) * (0.25 + low * 0.4));
    }
}

/// 卡 B3：飲み干す —— 露珠升空被饮干。
fn card_dew_drunk(canvas: &mut CharCanvas, ctx: &SceneCtx, phase: f32, alpha: f32) {
    spring_background(canvas, ctx, alpha);
    dew_field(canvas, ctx, alpha);
    east_wind(canvas, ctx, alpha * 0.5);
}

/// 卡 B4：余韵 —— 露尽后苔面只剩微光呼吸。
fn card_dew_afterglow(canvas: &mut CharCanvas, ctx: &SceneCtx, phase: f32, alpha: f32) {
    spring_background(canvas, ctx, alpha);
    let (sw, sh) = (canvas.sw as f32, canvas.sh as f32);
    let [main, _, _] = ctx.scene.palette();
    let t = ctx.t as f32;
    for k in 0..18u32 {
        let kf = k as f32;
        let x = ((kf * 0.587) % 1.0) * sw;
        let y = sh * (0.68 + ((kf * 0.313) % 1.0) * 0.20);
        let g = (t * 1.3 + kf * 0.9).sin().max(0.0);
        canvas.set(x, y, col::lerp(main, col::WHITE, 0.6), alpha * g * 0.5);
    }
}

/// 卡 B5：朝露余韵 —— 露珠坠地，砸出一圈圈扩散的环。
fn card_dew_ring(canvas: &mut CharCanvas, ctx: &SceneCtx, phase: f32, alpha: f32) {
    spring_background(canvas, ctx, alpha);
    dew_field(canvas, ctx, alpha * 0.9);
    let (sw, sh) = (canvas.sw as f32, canvas.sh as f32);
    let [main, _, _] = ctx.scene.palette();
    let t = ctx.t as f32;
    let life = envelope(phase, 0.0, 0.45, CARD_STEP - 0.35, CARD_STEP);
    if life <= 0.0 {
        return;
    }
    for k in 0..5u32 {
        let kf = k as f32;
        let cx = sw * (0.14 + ((kf * 0.719) % 1.0) * 0.72);
        let gy = sh * (0.74 + ((kf * 0.313) % 1.0) * 0.14);
        // 一颗露珠：先坠下，落地即漾开一圈环
        let u = (t * 0.42 + kf * 0.19) % 1.0;
        let fall = ease(u, 0.55);
        canvas.set(cx, gy - (1.0 - fall) * sh * 0.22, col::WHITE, alpha * life * 0.85);
        let ripple = ease(u - 0.55, 0.45);
        if ripple > 0.0 && ripple < 1.0 {
            let r = (0.6 + ripple * 5.5) * life;
            canvas.ellipse(cx, gy, r, r * 0.30, col::lerp(main, col::WHITE, 0.5),
                           alpha * life * (1.0 - ripple) * 0.8);
        }
    }
}

/// 卡 C1：一羽の蝶飞入。
fn card_butterfly_enters(canvas: &mut CharCanvas, ctx: &SceneCtx, phase: f32, alpha: f32) {
    spring_background(canvas, ctx, alpha);
    let _ = phase;
    butterfly(canvas, ctx, alpha);
}

/// 卡 C2：一羽の蝶 —— 蝶翅振频特写，翅面随快拍一张一合。
fn card_wingbeat_closeup(canvas: &mut CharCanvas, ctx: &SceneCtx, phase: f32, alpha: f32) {
    spring_background(canvas, ctx, alpha);
    let (sw, sh) = (canvas.sw as f32, canvas.sh as f32);
    let [_, deep, accent] = ctx.scene.palette();
    let t = ctx.t as f32;
    let life = envelope(phase, 0.0, 0.45, CARD_STEP - 0.35, CARD_STEP);
    if life <= 0.0 {
        return;
    }
    let cx = sw * 0.5;
    let cy = sh * 0.42;
    // 振频：快拍（翅面伸缩）叠一层慢呼吸（整只翅的大小）
    let flap = (t * 13.0).sin();
    let breathe = 0.6 + 0.4 * (t * 1.3).sin();
    let span = (sw * 0.30) * (0.72 + 0.28 * flap.abs()) * breathe * life;
    if span <= 0.3 {
        return;
    }
    let tone = col::lerp(accent, col::WHITE, 0.25);
    // 上翅两片 + 下翅两片
    canvas.line(cx, cy, cx - span, cy - span * 0.42, tone, alpha * 0.85);
    canvas.line(cx, cy, cx + span, cy - span * 0.42, tone, alpha * 0.85);
    canvas.line(cx, cy + span * 0.10, cx - span * 0.62, cy + span * 0.50,
                col::lerp(accent, deep, 0.35), alpha * 0.7);
    canvas.line(cx, cy + span * 0.10, cx + span * 0.62, cy + span * 0.50,
                col::lerp(accent, deep, 0.35), alpha * 0.7);
    // 翅脉：跟着振频闪
    for k in 0..5u32 {
        let u = (k as f32 + 1.0) / 6.0;
        let flick = 0.4 + 0.6 * ((t * 13.0 + k as f32 * 0.6).sin() * 0.5 + 0.5);
        canvas.line(cx - span * u, cy - span * 0.42 * u, cx - span * u * 0.9, cy + span * 0.30 * u,
                    col::lerp(tone, deep, 0.3), alpha * flick * 0.75);
        canvas.line(cx + span * u, cy - span * 0.42 * u, cx + span * u * 0.9, cy + span * 0.30 * u,
                    col::lerp(tone, deep, 0.3), alpha * flick * 0.75);
    }
    // 身体
    canvas.line(cx, cy - span * 0.16, cx, cy + span * 0.34, deep, alpha * 0.9);
    // 振频残影：翅尖摆到极限处的两点
    let ghost = (t * 13.0).cos();
    canvas.set(cx - span * 1.05, cy - span * 0.45 * ghost, col::WHITE, alpha * 0.75);
    canvas.set(cx + span * 1.05, cy - span * 0.45 * ghost, col::WHITE, alpha * 0.75);
}

/// 卡 C3：蕾の重み —— 花蕾鼓起。
fn card_buds_swell(canvas: &mut CharCanvas, ctx: &SceneCtx, phase: f32, alpha: f32) {
    spring_background(canvas, ctx, alpha);
    butterfly(canvas, ctx, alpha * 0.85);
    buds(canvas, ctx, alpha);
}

/// 卡 C4：試す午後 —— 蝶落蕾上，蕾被压弯回弹。
fn card_butterfly_lands(canvas: &mut CharCanvas, ctx: &SceneCtx, phase: f32, alpha: f32) {
    spring_background(canvas, ctx, alpha);
    buds(canvas, ctx, alpha);
    butterfly(canvas, ctx, alpha);
    let (sw, sh) = (canvas.sw as f32, canvas.sh as f32);
    let [main, deep, _] = ctx.scene.palette();
    let t = ctx.t as f32;
    // 蕾茎被压弯的弧：按慢正弦摆
    let bend = (t * 0.8).sin() * 2.2;
    canvas.line(sw * 0.44, sh * 0.74, sw * 0.44 + bend, sh * 0.70, deep, alpha * 0.7);
    canvas.disc(sw * 0.44 + bend, sh * 0.70 - 1.4, 1.8, col::lerp(main, col::WHITE, 0.3), alpha * 0.8);
}

/// 卡 C5：午後の光 —— 斜光变暖，尘埃浮游。
fn card_afternoon_dust(canvas: &mut CharCanvas, ctx: &SceneCtx, phase: f32, alpha: f32) {
    spring_background(canvas, ctx, alpha);
    light_shaft(canvas, ctx, alpha * 0.9);
    let (sw, sh) = (canvas.sw as f32, canvas.sh as f32);
    let [_, _, accent] = ctx.scene.palette();
    let t = ctx.t as f32;
    for k in 0..24u32 {
        let kf = k as f32;
        let x = ((kf * 0.677 + t * 0.021) % 1.0) * sw;
        let y = sh * (0.30 + ((kf * 0.449) % 1.0) * 0.45) + (t * 0.9 + kf).sin() * 2.0;
        canvas.set(x, y, col::lerp(col::WHITE, accent, 0.3), alpha * 0.35);
    }
}

/// 卡 D1：東風吹けば —— 风线掠过，草浪倒伏。
fn card_east_breeze(canvas: &mut CharCanvas, ctx: &SceneCtx, phase: f32, alpha: f32) {
    spring_background(canvas, ctx, alpha);
    east_wind(canvas, ctx, alpha);
    let (sw, sh) = (canvas.sw as f32, canvas.sh as f32);
    let [main, deep, _] = ctx.scene.palette();
    let t = ctx.t as f32;
    // 草浪：整排竖线按行波倒伏
    let mut x = 0.0;
    while x < sw {
        let lean = (x * 0.05 - t * 1.4).sin();
        let base = sh * 0.78 + (x * 0.021).sin() * 2.0;
        canvas.line(x, base, x + lean * 3.0, base - 4.0 - lean.abs() * 2.0,
                    col::lerp(main, deep, 0.3), alpha * 0.55);
        x += 3.0;
    }
}

/// 卡 D2：雲は解ケテ —— 云絮被东风抽成一丝丝，往左飘走。
fn card_cloud_threads(canvas: &mut CharCanvas, ctx: &SceneCtx, phase: f32, alpha: f32) {
    spring_background(canvas, ctx, alpha);
    east_wind(canvas, ctx, alpha * 0.9);
    let (sw, sh) = (canvas.sw as f32, canvas.sh as f32);
    let [main, _, _] = ctx.scene.palette();
    let t = ctx.t as f32;
    let life = envelope(phase, 0.0, 0.45, CARD_STEP - 0.35, CARD_STEP);
    if life <= 0.0 {
        return;
    }
    for k in 0..7u32 {
        let kf = k as f32;
        let cy = sh * (0.10 + ((kf * 0.317) % 1.0) * 0.26);
        // 云丝：自右向左被抽长；出入画两端先淡掉，免得瞬移成突跳
        let u = (t * 0.09 + kf * 0.13) % 1.0;
        let vis = (1.0 - (u * 2.0 - 1.0).abs()).clamp(0.0, 1.0);
        let x = sw * (0.86 - u * 1.15);
        let len = (sw * 0.22) * life * (0.5 + ((kf * 0.581) % 1.0));
        let thick = 1.0 + ((kf * 0.429) % 1.0) * 1.6;
        let a = alpha * life * vis * (0.55 + 0.35 * (t * 0.7 + kf).sin().abs());
        canvas.line(x, cy, x + len, cy + (kf * 0.7).sin() * 1.2,
                    col::lerp(col::WHITE, main, 0.35), a);
        canvas.line(x, cy + thick, x + len * 0.8, cy + thick,
                    col::lerp(col::WHITE, main, 0.55), a * 0.7);
    }
}

/// 卡 D2：雲は解ケテ —— 云开始被吹散。
fn card_clouds_unravel(canvas: &mut CharCanvas, ctx: &SceneCtx, phase: f32, alpha: f32) {
    spring_background(canvas, ctx, alpha);
    east_wind(canvas, ctx, alpha);
    clouds_dissolving(canvas, ctx, alpha);
}

/// 卡 D4：水ニナル —— 云化作水，水面初生。
fn card_cloud_becomes_water(canvas: &mut CharCanvas, ctx: &SceneCtx, phase: f32, alpha: f32) {
    spring_background(canvas, ctx, alpha);
    clouds_dissolving(canvas, ctx, alpha);
}

/// 卡 D5：水面映光 —— 全幅水纹托住光带，春段收束。
fn card_water_mirror(canvas: &mut CharCanvas, ctx: &SceneCtx, phase: f32, alpha: f32) {
    spring_background(canvas, ctx, alpha);
    light_shaft(canvas, ctx, alpha * 0.6);
    let (sw, sh) = (canvas.sw as f32, canvas.sh as f32);
    let [main, deep, accent] = ctx.scene.palette();
    let t = ctx.t as f32;
    // 全幅水纹：叠两列行波
    for layer in 0..2u32 {
        let mut prev: Option<f32> = None;
        for x in 0..(sw as i32) {
            let xf = x as f32;
            let y = sh * (0.80 + layer as f32 * 0.07)
                + (xf * 0.06 - t * (1.2 + layer as f32)).sin() * (1.6 - layer as f32 * 0.5)
                + (xf * 0.017 + t * 0.5).sin();
            if let Some(py) = prev {
                canvas.line(xf - 1.0, py, xf, y,
                            col::lerp(col::lerp(main, accent, layer as f32 * 0.4), col::WHITE, 0.2),
                            alpha * (0.5 - layer as f32 * 0.15));
            }
            prev = Some(y);
        }
    }
    let _ = deep;
}

/// 春 20 卡序列。`(段内起始秒, 卡名, 绘制函数)`
/// 卡间距恒为 [`CARD_STEP`]（1.2~1.8s 内），20 张正好铺满 [`SPRING_LEN`]，全程无 <1s 碎剪。
pub type SpringCard = (f64, &'static str, fn(&mut CharCanvas, &SceneCtx, f32, f32));

pub type SpringFn = fn(&mut CharCanvas, &SceneCtx, f32, f32);

pub fn cards() -> Vec<(f64, &'static str, SpringFn)> {
    vec![
        (0.0, "moss_awakens", card_moss_awakens),
        (1.38, "light_lands", card_light_lands),
        (2.76, "light_dapple", card_light_dapple),
        (4.14, "fuzz_glint", card_fuzz_glint),
        (5.52, "green_breath", card_green_breath),
        (6.9, "dew_born", card_dew_born),
        (8.28, "dew_ring", card_dew_ring),
        (9.66, "dew_pulse", card_dew_pulse),
        (11.04, "dew_drunk", card_dew_drunk),
        (12.42, "dew_afterglow", card_dew_afterglow),
        (13.8, "butterfly_enters", card_butterfly_enters),
        (15.18, "wingbeat_closeup", card_wingbeat_closeup),
        (16.56, "buds_swell", card_buds_swell),
        (17.94, "butterfly_lands", card_butterfly_lands),
        (19.32, "afternoon_dust", card_afternoon_dust),
        (20.7, "east_breeze", card_east_breeze),
        (22.08, "cloud_threads", card_cloud_threads),
        (23.46, "clouds_unravel", card_clouds_unravel),
        (24.84, "cloud_becomes_water", card_cloud_becomes_water),
        (26.22, "water_mirror", card_water_mirror),
    ]
}

/// 按表选卡并绘制。`list` 由调用方给出（春夏同构）。
fn pick_and_draw(
    list: &[(f64, &'static str, SpringFn)],
    local: f32,
    ctx: &SceneCtx,
    canvas: &mut CharCanvas,
    alpha: f32,
) -> &'static str {
    let mut idx = 0usize;
    for (i, (start, _, _)) in list.iter().enumerate() {
        if local >= *start as f32 {
            idx = i;
        }
    }
    let (start, name, f) = list[idx];
    let phase = local - start as f32;
    f(canvas, ctx, phase, alpha);
    name
}

/// 春段入口：按 `local_t` 选卡并绘制。
pub fn render_card(ctx: &SceneCtx, canvas: &mut CharCanvas, alpha: f32) {
    let local = ctx.local_t as f32;
    let _ = pick_and_draw(card_list(), local, ctx, canvas, alpha);
}

static CARDS: std::sync::OnceLock<Vec<(f64, &'static str, SpringFn)>> = std::sync::OnceLock::new();

fn card_list() -> &'static [(f64, &'static str, SpringFn)] {
    CARDS.get_or_init(cards)
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
            progress: (local_t as f32 / SPRING_LEN as f32),
            feat: f.clone(),
            prev: f,
            frame: (t * 24.0) as u64,
            cols: 80,
            rows: 24,
            scene: SceneKind::Spring,
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
    const NEW_CARDS: [&str; 4] = ["light_dapple", "dew_ring", "wingbeat_closeup", "cloud_threads"];

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
        spring_background(&mut canvas, &ctx, 1.0);
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
            assert!((1.2..=1.8).contains(&gap), "春卡 {}→{} 间距 {gap:.2}s 越界", w[0].1, w[1].1);
        }
        let last = list.last().unwrap().0;
        assert!(last + 1.2 <= SPRING_LEN, "春末卡 {last:.2}s 起太晚，装不下 1.2s");
    }

    /// 每张卡都得画满画面（80×24 Braille 上非空子像素 > 20）。
    #[test]
    fn every_card_draws_something() {
        for (start, name, _) in card_list() {
            let f = full(15.7 + start + 0.10, start + 0.10);
            assert!(f.non_empty > 20, "春卡 {name} 入口只画 {} 个非空子像素（点亮 {}）",
                    f.non_empty, f.lit);
        }
    }

    /// 极小画布（8×5 与 1×1）不许 panic。
    #[test]
    fn tiny_canvas_does_not_panic() {
        for (start, name, _) in card_list() {
            for k in 0..8 {
                let local = start + k as f64 * 0.2;
                let _ = frame_at(15.7 + local, local, 8, 5);
                let _ = frame_at(15.7 + local, local, 1, 1);
            }
            let _ = name;
        }
    }

    /// 场景是 t 的纯函数：同输入必须同画面。
    #[test]
    fn same_input_gives_same_frame() {
        for (start, name, _) in card_list() {
            let local = start + 0.7;
            let a = full(15.7 + local, local);
            let b = full(15.7 + local, local);
            assert_eq!(a, b, "春卡 {name} 同输入却不同画面");
        }
    }

    /// 帧间必须有差异，不能是死帧。
    #[test]
    fn consecutive_frames_differ() {
        for (start, name, _) in card_list() {
            let local = start + 0.7;
            let a = full(15.7 + local, local);
            let b = full(15.7 + local + 1.0 / 24.0, local + 1.0 / 24.0);
            assert_ne!(a.hash, b.hash, "春卡 {name} 相邻两帧完全一样");
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
                series.push(full(15.7 + local, local).ink);
            }
            let steps: Vec<f64> = series.windows(2).map(|w| (w[1] - w[0]).abs()).collect();
            let mean = steps.iter().sum::<f64>() / steps.len() as f64;
            let max = steps.iter().cloned().fold(0.0f64, f64::max);
            assert!(max <= mean * 3.0 + 14.0,
                    "春卡 {name} 卡内墨量突跳：单步 {max:.1} > 3×平均 {mean:.1} + 14");
        }
    }

    /// 卡交界两侧 0.06s：把下一张卡按 ∓0.06s 相位各画一次，总墨量相对变化不得超 50%
    /// （与 scenes_autumn 同款口径）。
    #[test]
    fn no_jump_at_card_boundary() {
        let cov = |f: SpringFn, phase: f32, t: f64| {
            let ctx = ctx_at(15.7 + t, t);
            let mut canvas = CharCanvas::new(80, 24, RenderMode::Braille);
            spring_background(&mut canvas, &ctx, 1.0);
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
        assert!(jumps.len() <= 2, "春卡交界跳变过多：{jumps:?}");
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
            let bg_in = background_ink(15.7 + start + 0.05, start + 0.05);
            let e1 = full(15.7 + start + 0.05, start + 0.05).ink - bg_in;
            let e2 = full(15.7 + start + 0.45, start + 0.45).ink - bg_in;
            assert!(e1 < e2, "春卡 {name} 入口不是渐入：{e1:.1} → {e2:.1}");
            let t1 = start + step as f64 - 0.45;
            let t2 = start + step as f64 - 0.05;
            let o1 = full(15.7 + t1, t1).ink - background_ink(15.7 + t1, t1);
            let o2 = full(15.7 + t2, t2).ink - background_ink(15.7 + t2, t2);
            assert!(o1 > o2, "春卡 {name} 出口不是渐出：{o1:.1} → {o2:.1}");
        }
    }
}

//! 全屏流动大气场 —— 消灭死黑、抬亮度、给景深。
//!
//! 成片被判「大片死黑、看不出画的是什么」，根因不是后处理，而是场景画的元素太少：
//! 九成以上的字符格是空的，全片平均亮度只有 2.88/255、中位 1.86/255。
//! 本模块铺一层覆盖整幅画布、随季节变色、持续流动的底纹，把整片抬到达标线。
//!
//! # 三层结构（景深来自空间频率与流速的落差）
//!
//! | 层 | 角色 | 空间波长（点阵单位） | 相位速度 |
//! |----|------|----------------------|----------|
//! | 慢 | 大块呼吸 | ≈279 × 370 点（≈140 × 92 字符） | ≈2.7 点/秒 |
//! | 中 | 斜向流动 | ≈60 × 84 点（≈30 × 21 字符） | ≈5.9 点/秒 |
//! | 快 | 细粒闪烁 | ≈10 点（≈5 字符） | ≈8.1 点/秒 |
//!
//! 三层叠加成一个平滑的「覆盖率场」，再用逐点固定的整数哈希做有序抖动，
//! 把覆盖率翻译成 `w > 0.5` 的点亮集合 —— 密度因此可控，
//! 且画面任意区域都有亮点，不会出现大片纯黑。最后补一道逐字符格保底：
//! 任何一格若一个亮点都没有，就把它内部场强最高的那个点强制点亮。
//!
//! # 配色
//!
//! 色带是一张 16 × 8 × 6 的三维表，三条轴各归一层：
//! **慢层**在 `deep → main` 之间取行（大块色区），**中层**把 `main` 往 `accent`
//! 推最多 12% 取列，**快层**再把强调色推进最多 25% 取第三轴。
//! 三层各推各的颜色，画面因此有色彩层次，而不是一整片单色。
//!
//! # 不变式
//!
//! - 颜色与 `w` 解耦：**绝不**把 `w` 乘进颜色再交给 `set`，那会双重衰减。
//! - `w` 是 `t` 的连续函数（抖动量由解析行波驱动），不存在帧间跳变。
//! - 无跨帧状态、无 `rand`：同一 `ctx` + 同一 `t` 必得逐位相同的画面。

use ratatui::style::Color;

use crate::config::RenderMode;
use crate::render::color as col;
use crate::render::scenes::SceneCtx;
use crate::render::CharCanvas;

// ─────────────────────────────────────────────────────────────────────────────
// 三层场的空间频率（弧度/点）与相位速度（弧度/秒）
// ─────────────────────────────────────────────────────────────────────────────

/// 慢层：大块呼吸，波长 ≈279 × 370 点。
const SLOW_KX: f32 = 0.0225;
const SLOW_KY: f32 = 0.0170;
const SLOW_WX: f32 = 0.060;
const SLOW_WY: f32 = 0.048;

/// 中层：斜向流动，波长 ≈60 × 84 点。
const MID_KX: f32 = 0.105;
const MID_KY: f32 = 0.075;
const MID_WX: f32 = 0.62;
const MID_WY: f32 = 0.44;

/// 快层：细粒闪烁，波长 ≈10 点。
const FAST_KX: f32 = 0.62;
const FAST_KY: f32 = 0.41;
const FAST_W: f32 = 5.0;

/// 覆盖率基线：保证最暗的区域也还有约 13% 的点在闪。
const COV_BASE: f32 = 0.13;
/// 覆盖率摆幅：`COV_BASE + COV_SPAN` 就是最亮区域的覆盖率。
const COV_SPAN: f32 = 0.32;
/// 抖动灵敏度：把「抖动值 − 覆盖率」的差值映射成 `w` 的偏离量。
const DITHER_GAIN: f32 = 2.6;
/// `w` 上限。压到 0.62 是为了让主体层（alpha 通常 ≥ 0.7）永远盖得住底纹。
const W_MAX: f32 = 0.62;
/// 主体避让：中心偏下最多压低 30% 的覆盖率。
const MASK_AMOUNT: f32 = 0.30;
/// 色带表尺寸：行 = 慢层，列 = 中层，第三轴 = 快层。
const LUT_ROWS: usize = 16;
const LUT_COLS: usize = 8;
const LUT_FAST: usize = 6;
/// 中层最多把强调色混进 12%。
const MID_ACCENT: f32 = 0.12;
/// 快层最多再把强调色推进 25%，合起来 ≈34%，再多就变成一片粉/橙，失去季节主色。
const FAST_ACCENT: f32 = 0.25;

/// 整数坐标哈希 → `[0, 1)`。
///
/// 纯函数，不含状态；用于有序抖动。`x`/`y` 是点阵坐标，恒为非负。
#[inline]
fn hash01(x: i32, y: i32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x9E37_79B9) ^ (y as u32).wrapping_mul(0x85EB_CA6B);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2545_F491);
    h ^= h >> 13;
    h = h.wrapping_mul(0x27D4_EB2D);
    h ^= h >> 16;
    // 取高 24 位 → [0, 1)
    (h >> 8) as f32 / 16_777_216.0
}

/// 一维正弦 / 余弦波表。
///
/// 三层场都是「x 波 × y 波」的可分离形式（斜向行波用 `sin(A+B)` 展开），
/// 所以每帧只需 `sw + sh` 次三角运算，而不是 `sw × sh` 次。
struct Wave {
    s: Vec<f32>,
    c: Vec<f32>,
}

impl Wave {
    /// `value(i) = i * k + phase`。
    fn new(n: usize, k: f32, phase: f32) -> Self {
        let mut s = Vec::with_capacity(n);
        let mut c = Vec::with_capacity(n);
        for i in 0..n {
            let a = i as f32 * k + phase;
            s.push(a.sin());
            c.push(a.cos());
        }
        Self { s, c }
    }

    #[inline]
    fn sin(&self, i: usize) -> f32 {
        self.s[i]
    }

    #[inline]
    fn cos(&self, i: usize) -> f32 {
        self.c[i]
    }
}

/// 逐字符格保底用的「一维高斯」表：`exp(-((i - center) / sigma)^2)`。
///
/// 二维高斯可分离，逐点只做一次乘法，省掉每点一次 `exp`。
fn gauss_axis(n: usize, center: f32, sigma: f32) -> Vec<f32> {
    let sigma = sigma.max(1e-3);
    (0..n)
        .map(|i| {
            let d = (i as f32 - center) / sigma;
            (-d * d).exp()
        })
        .collect()
}

/// 预计算 16 × 8 × 6 的色带表（行 → 列 → 快层，行优先）。
///
/// 三条轴各归一层，所以「不同层用不同色」是真的落在颜色上：
/// - 行 = **慢层**在 `lerp(main, deep, 0.65) → main` 之间取值 —— 大块色区呼吸；
/// - 列 = **中层**把 `main` 往 `accent` 推最多 12% —— 流动色偏移；
/// - 第三轴 = **快层**再把强调色推进最多 25% —— 细粒闪烁偏强调色。
fn tone_lut(main: Color, deep: Color, accent: Color, dim: f32) -> Vec<Color> {
    let dark = col::lerp(main, deep, 0.65);
    let mut table = Vec::with_capacity(LUT_ROWS * LUT_COLS * LUT_FAST);
    for r in 0..LUT_ROWS {
        let base = col::lerp(dark, main, r as f32 / (LUT_ROWS - 1) as f32);
        for c in 0..LUT_COLS {
            let mid = col::lerp(base, accent, MID_ACCENT * c as f32 / (LUT_COLS - 1) as f32);
            for f in 0..LUT_FAST {
                let mix = FAST_ACCENT * f as f32 / (LUT_FAST - 1) as f32;
                table.push(col::scale(col::lerp(mid, accent, mix), dim));
            }
        }
    }
    table
}

/// 铺满整幅画布的流动大气场。
///
/// `intensity` 0..=1 控制整体浓度（Lead 传 0.75~1.0）：
/// 它主要改颜色亮度，覆盖率只轻微跟随 —— 否则浓度调低时点亮率会掉出达标线。
pub fn render(canvas: &mut CharCanvas, ctx: &SceneCtx, intensity: f32) {
    let sw = canvas.sw as usize;
    let sh = canvas.sh as usize;
    if sw == 0 || sh == 0 {
        return;
    }
    let k = intensity.clamp(0.0, 1.0);
    // 写成 `!(k > 1e-3)`：NaN 也一并挡掉，避免把 NaN 的 w 写进画布。
    if !(k > 1e-3) {
        return;
    }

    let [main, deep, accent] = ctx.scene.palette();
    let t = ctx.t as f32;

    // 音乐只做轻微调制：密度必须稳定在 15%~30%，不能被鼓点推爆。
    let energy = ctx.feat.energy_norm.clamp(0.0, 1.0);
    let pump = 0.94 + 0.12 * energy;
    // 整体缓慢呼吸（周期 ≈30s），让最慢的一层也在动。
    let breathe = 1.0 + 0.05 * (t * 0.21).sin();
    let gain = (0.86 + 0.14 * k) * pump * breathe;
    // 浓度旋钮的亮度端。
    let dim = 0.62 + 0.38 * k;

    let slow_x = Wave::new(sw, SLOW_KX, t * SLOW_WX);
    let slow_y = Wave::new(sh, SLOW_KY, -t * SLOW_WY);
    let mid_x = Wave::new(sw, MID_KX, -t * MID_WX);
    let mid_y = Wave::new(sh, MID_KY, t * MID_WY);
    let fast_x = Wave::new(sw, FAST_KX, t * FAST_W);
    let fast_y = Wave::new(sh, FAST_KY, 0.0);

    let lut = tone_lut(main, deep, accent, dim);

    // 主体避让：中心偏下（主体所在）压低覆盖率，前景元素仍然读得出来。
    let mask_x = gauss_axis(sw, sw as f32 * 0.50, (sw as f32 * 0.30).max(8.0));
    let mask_y = gauss_axis(sh, sh as f32 * 0.62, (sh as f32 * 0.26).max(6.0));

    for y in 0..sh {
        let gy = mask_y[y];
        for x in 0..sw {
            // ── 三层场，全部落在 0..1 ──
            let a = 0.5 + 0.5 * slow_x.sin(x) * slow_y.sin(y);
            let b = 0.5 + 0.5 * (mid_x.sin(x) * mid_y.cos(y) + mid_x.cos(x) * mid_y.sin(y));
            let c = 0.5 + 0.5 * (fast_x.sin(x) * fast_y.cos(y) + fast_x.cos(x) * fast_y.sin(y));

            let mask = 1.0 - MASK_AMOUNT * mask_x[x] * gy;
            let cov = (COV_BASE + COV_SPAN * (0.55 * a + 0.45 * b)) * gain * mask;

            // 有序抖动：72% 的逐点固定哈希 + 28% 的流动细粒行波。
            // 两者都是 t 的连续函数，所以 w 连续，不会帧间跳变。
            let dither = 0.72 * hash01(x as i32, y as i32) + 0.28 * c;
            let w = (0.5 - DITHER_GAIN * (dither - cov)).clamp(0.0, W_MAX);
            if w <= 0.0 {
                continue;
            }

            // 颜色只由层的取值决定，与 w 无关（否则双重衰减）。
            let row = ((a * (LUT_ROWS - 1) as f32) as usize).min(LUT_ROWS - 1);
            let colm = ((b * (LUT_COLS - 1) as f32) as usize).min(LUT_COLS - 1);
            let fast = ((c * (LUT_FAST - 1) as f32) as usize).min(LUT_FAST - 1);
            let tone = lut[(row * LUT_COLS + colm) * LUT_FAST + fast];
            canvas.set(x as f32, y as f32, tone, w);
        }
    }

    // 逐字符格保底：任何区域都不许纯黑。
    light_empty_cells(canvas, col::scale(col::lerp(main, deep, 0.45), dim));
}

/// 每个字符格至少点亮一个点。
///
/// 抖动是逐点独立的，极暗区域仍可能整格无亮点（实测约三成格子）。
/// 这里对每个空格子取内部 `w` 最大的那个点强制点亮：位置由场强决定，
/// 不会形成规则网格，视觉上就是一层稀疏的氛围底噪。
fn light_empty_cells(canvas: &mut CharCanvas, tone: Color) {
    let sub_x = canvas.sub_x.max(1) as usize;
    let sub_y = canvas.sub_y.max(1) as usize;
    // ASCII 模式按 0.01 判定亮，其余模式走 Braille/半块/象限的 0.5 阈值。
    let thr = if canvas.mode == RenderMode::Ascii {
        0.02
    } else {
        0.5
    };

    let mut fixes: Vec<(f32, f32)> = Vec::new();
    for cy in 0..canvas.rows {
        for cx in 0..canvas.cols {
            let mut lit = false;
            let mut best = (0.0f32, 0.0f32, -1.0f32);
            for sy in 0..sub_y {
                for sx in 0..sub_x {
                    let x = (cx as usize * sub_x + sx) as i32;
                    let y = (cy as usize * sub_y + sy) as i32;
                    let p = canvas.pixel(x, y);
                    if p.w > thr {
                        lit = true;
                    }
                    if p.w > best.2 {
                        best = (x as f32, y as f32, p.w);
                    }
                }
            }
            if !lit {
                fixes.push((best.0, best.1));
            }
        }
    }

    for (x, y) in fixes {
        canvas.set(x, y, tone, 0.56);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 测试
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::features::{FrameFeatures, BAND_COUNT, SPECTRUM_BINS};
    use crate::lyrics::Lyrics;
    use crate::timeline::scene::SceneKind;

    fn feat(energy: f32) -> FrameFeatures {
        let mut f = FrameFeatures {
            time: 0.0,
            energy,
            energy_norm: energy,
            ..Default::default()
        };
        f.bands_norm = [energy; BAND_COUNT];
        f.spectrum = (0..SPECTRUM_BINS)
            .map(|i| ((i as f32 / 16.0).sin().abs()) * energy)
            .collect();
        f
    }

    fn ctx_at(scene: SceneKind, t: f64) -> SceneCtx<'static> {
        let f = feat(0.6);
        SceneCtx {
            t,
            local_t: t,
            progress: 0.5,
            feat: f.clone(),
            prev: f,
            frame: (t * 24.0) as u64,
            cols: 220,
            rows: 62,
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

    fn frame_at(scene: SceneKind, t: f64, cols: u16, rows: u16, k: f32) -> CharCanvas {
        let ctx = ctx_at(scene, t);
        let mut c = CharCanvas::new(cols, rows, RenderMode::Braille);
        render(&mut c, &ctx, k);
        c
    }

    /// `w > 0.5` 的点占比（Braille 只有这些点会亮）。
    fn lit_ratio(c: &CharCanvas) -> f32 {
        let n = c.pixels().len().max(1);
        let lit = c.pixels().iter().filter(|p| p.w > 0.5).count();
        lit as f32 / n as f32
    }

    /// 逐像素滚动哈希，对任何位级改动都敏感。
    fn canvas_hash(c: &CharCanvas) -> u64 {
        let mut h = 1469598103934665603u64;
        for p in c.pixels() {
            for v in [p.w, p.r, p.g, p.b] {
                h = (h ^ v.to_bits() as u64).wrapping_mul(1099511628211);
            }
        }
        h
    }

    /// 点亮点的平均颜色（0..1）。
    fn mean_lit_color(c: &CharCanvas) -> (f32, f32, f32) {
        let (mut n, mut r, mut g, mut b) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        for p in c.pixels() {
            if p.w > 0.5 {
                r += p.r;
                g += p.g;
                b += p.b;
                n += 1.0;
            }
        }
        if n <= 0.0 {
            return (0.0, 0.0, 0.0);
        }
        (r / n, g / n, b / n)
    }

    fn luma(c: (f32, f32, f32)) -> f32 {
        0.2126 * c.0 + 0.7152 * c.1 + 0.0722 * c.2
    }

    #[test]
    fn same_t_is_bit_identical() {
        let a = frame_at(SceneKind::Spring, 7.25, 120, 34, 1.0);
        let b = frame_at(SceneKind::Spring, 7.25, 120, 34, 1.0);
        assert_eq!(canvas_hash(&a), canvas_hash(&b), "同一 t 两次调用必须逐位一致");
        assert_eq!(a.pixels().len(), b.pixels().len());
    }

    #[test]
    fn adjacent_frames_differ() {
        let a = frame_at(SceneKind::Summer, 12.0, 220, 62, 1.0);
        let b = frame_at(SceneKind::Summer, 12.0 + 1.0 / 24.0, 220, 62, 1.0);
        assert_ne!(canvas_hash(&a), canvas_hash(&b), "相邻时刻画面必须不同");
        let flips = a
            .pixels()
            .iter()
            .zip(b.pixels())
            .filter(|(p, q)| (p.w > 0.5) != (q.w > 0.5))
            .count();
        assert!(flips > 200, "1/24 秒内点亮状态翻转的点太少：{flips}");
    }

    /// 禁止跳变：`t` 微小推进时任何一点的 `w` 变化都必须很小。
    #[test]
    fn motion_is_continuous() {
        let a = frame_at(SceneKind::Autumn, 20.0, 160, 45, 1.0);
        let b = frame_at(SceneKind::Autumn, 20.0 + 1.0 / 240.0, 160, 45, 1.0);
        let max_d = a
            .pixels()
            .iter()
            .zip(b.pixels())
            .map(|(p, q)| (p.w - q.w).abs())
            .fold(0.0f32, f32::max);
        assert!(max_d < 0.1, "1/240 秒内 w 跳变了 {max_d}");
    }

    #[test]
    fn density_at_220x62_is_in_range() {
        for t in [0.0, 3.7, 11.3, 42.0, 123.4] {
            let c = frame_at(SceneKind::Spring, t, 220, 62, 1.0);
            let r = lit_ratio(&c);
            assert!(
                (0.12..=0.35).contains(&r),
                "t={t} 点亮率 {:.1}% 越界（应在 12%~35%）",
                r * 100.0
            );
        }
    }

    /// 浓度旋钮 0.75~1.0 全程都得达标（Lead 可能传 0.75）。
    #[test]
    fn density_holds_across_intensity() {
        for k in [0.75, 0.85, 1.0] {
            let c = frame_at(SceneKind::Winter, 8.5, 220, 62, k);
            let r = lit_ratio(&c);
            assert!(
                (0.12..=0.35).contains(&r),
                "intensity={k} 点亮率 {:.1}% 越界",
                r * 100.0
            );
        }
    }

    /// 整幅不许有空格子 —— 「任何区域都不会是纯黑」。
    #[test]
    fn every_cell_has_a_lit_dot() {
        let c = frame_at(SceneKind::Outro, 30.0, 220, 62, 1.0);
        let mut blank = 0;
        for cy in 0..c.rows {
            for cx in 0..c.cols {
                let mut lit = false;
                for sy in 0..4u16 {
                    for sx in 0..2u16 {
                        if c.pixel((cx * 2 + sx) as i32, (cy * 4 + sy) as i32).w > 0.5 {
                            lit = true;
                        }
                    }
                }
                if !lit {
                    blank += 1;
                }
            }
        }
        assert_eq!(blank, 0, "有 {blank} 个字符格是纯黑的");
    }

    #[test]
    fn tiny_canvases_do_not_panic() {
        for (cols, rows) in [(8u16, 5u16), (1, 1), (2, 1), (1, 4)] {
            for k in [0.0, 0.5, 1.0] {
                let _ = frame_at(SceneKind::Bridge, 5.0, cols, rows, k);
            }
        }
        // 其它渲染模式也不能炸（保底阈值随模式变）。
        for mode in [
            RenderMode::Half,
            RenderMode::Ascii,
            RenderMode::Quadrant,
            RenderMode::Braille,
        ] {
            let ctx = ctx_at(SceneKind::Winter, 3.0);
            let mut c = CharCanvas::new(6, 4, mode);
            render(&mut c, &ctx, 1.0);
        }
    }

    #[test]
    fn intensity_zero_is_a_no_op() {
        let ctx = ctx_at(SceneKind::Spring, 2.0);
        let mut c = CharCanvas::new(40, 12, RenderMode::Braille);
        render(&mut c, &ctx, 0.0);
        assert!(
            c.pixels().iter().all(|p| p.is_empty()),
            "intensity=0 不该画任何东西"
        );
        // NaN 也必须被挡掉，不能把 NaN 的 w 写进画布。
        render(&mut c, &ctx, f32::NAN);
        assert!(
            c.pixels().iter().all(|p| p.is_empty()),
            "intensity=NaN 不该画任何东西"
        );
    }

    /// 四季不能画成同一个颜色，而且都得够亮。
    #[test]
    fn seasons_produce_different_average_color() {
        let seasons = [
            SceneKind::Spring,
            SceneKind::Summer,
            SceneKind::Autumn,
            SceneKind::Winter,
        ];
        let means: Vec<(f32, f32, f32)> = seasons
            .iter()
            .map(|s| mean_lit_color(&frame_at(*s, 3.7, 220, 62, 1.0)))
            .collect();
        for (i, m) in means.iter().enumerate() {
            assert!(luma(*m) > 0.35, "{:?} 的平均色太暗：{:?}", seasons[i], m);
        }
        for i in 0..seasons.len() {
            for j in (i + 1)..seasons.len() {
                let d = (means[i].0 - means[j].0)
                    .abs()
                    .max((means[i].1 - means[j].1).abs())
                    .max((means[i].2 - means[j].2).abs());
                assert!(
                    d > 0.08,
                    "{:?} 与 {:?} 的平均色几乎一样（最大通道差 {d:.3}）",
                    seasons[i],
                    seasons[j]
                );
            }
        }
    }

    /// 亮度量级：点亮率 × 点亮色亮度，目标 25~60/255。
    #[test]
    fn estimated_brightness_is_in_target_band() {
        let c = frame_at(SceneKind::Spring, 3.7, 220, 62, 1.0);
        let b = lit_ratio(&c) * luma(mean_lit_color(&c)) * 255.0;
        assert!(
            (25.0..=60.0).contains(&b),
            "估算平均亮度 {b:.1}/255 不在 25~60"
        );
    }
}

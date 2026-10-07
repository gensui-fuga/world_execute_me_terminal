//! 美术层：元素光晕 / 终端窗口框 / 走字带 / 逐格解码转场。
//!
//! 对标 kit.py 的 `haloed` / `tk.box` / 解码转场，补上两件「好看」的手艺：
//! 每个元素外面套一层同色的柔和辉光，以及「这是一个界面」的窗口结构。
//!
//! ## 和 postfx::bloom 的区别
//! `postfx::bloom` 是固定 3×3 的全屏模糊，元素本体依旧是硬边细线；
//! 这里做的是**元素级光晕**：把画布现有像素当 alpha 源，做可分离盒式模糊，
//! 再以指定颜色用 `canvas.add()` 加法叠回 —— 不是 `set`（`set` 取 max，看不出光）。
//!
//! ## Braille 的 0.5 阈值与「抖动辉光」
//! Braille 只点亮 `cov > 0.5` 的点，纯连续辉光会被 0.5 阈值整段吃掉。
//! 所以叠光分两路：
//! - **密度抖动**：把归一化后的辉光值当作点亮概率，用坐标哈希做有序抖动 ——
//!   近处点密、远处点疏，在 1bit 的 Braille 上这就是「柔和渐变」；
//! - **连续底光**：未点亮的点补一份很低的连续覆盖度，给半块 / ASCII 模式留层次。
//!
//! ## 复杂度与性能
//! 可分离盒式模糊：横向一趟 + 纵向一趟，各用一维**前缀和（积分图）**增量求和，
//! O(w·h) 与半径无关；跑 2 次迭代把盒式核近似成高斯核（单次盒式核的辉光是硬边
//! 方框，两次才叫柔和），共 4 趟一维扫描，仍是 O(w·h)。
//! 440×248 = 109k 点、半径 8 时全流程约 1~3ms（估算）。
//! 缓冲走 `thread_local` 复用，稳态下每帧零分配。
//!
//! ## 归一化
//! 叠光前按**模糊后的峰值**把强度分摊到 `strength * 0.9`：既保证看得见
//! （strength 0.6 时峰值覆盖度约 0.54 > Braille 的 0.5 阈值），又不会一叠一片白
//! （单点叠加上限 0.80，且只加在已有像素上，不乘任何 `Pixel.w`）。
//!
//! ## 字形来源
//! - **标题**走 `crate::render::text::draw`：有字体画真字，没字体该模块自己降级成
//!   2×3 小方块，两条路都不 panic。⚠️ 接线时 `mod.rs` 必须同时声明
//!   `pub mod chrome;` 与 `pub mod text;`，否则这个引用解析不了。
//! - **走字带 / 解码乱码**用本文件内的 3×3 点阵占位字形 [`stamp_char`]：
//!   形状由字符码位哈希决定，同一字符永远同一形状，确定性不受影响；
//!   而且走字带是逐字滚动、解码转场要的是「乱码块」的质感，本来就不该走字体。

use std::cell::RefCell;

use ratatui::style::Color;

use crate::render::color as col;
use crate::render::CharCanvas;

/// 光晕半径上限（点）。
const GLOW_RADIUS_MAX: f32 = 24.0;
/// 归一化后辉光的峰值覆盖度目标（× strength）。
const GLOW_PEAK: f32 = 0.90;
/// 抖动点亮时给的基础覆盖度。
const GLOW_LIT: f32 = 0.55;
/// 全屏 glow 的抖动种子（固定 → 纯函数）。
const GLOW_SEED: u32 = 0x5EED_1234;
/// 局部 glow 的抖动种子。
const GLOW_AT_SEED: u32 = 0x0A17_5EED;
/// 解码转场的字符池。
const GLYPHS: [char; 16] = [
    '#', '*', '+', '=', '-', ':', '.', '%', '@', '&', '$', '?', '!', '<', '>', '~',
];
/// 8 帧 spinner（kit.py 的 `|/-\` 序列）。
const SPINNER_FRAMES: [char; 8] = ['|', '/', '-', '\\', '|', '/', '-', '\\'];
/// 解码转场的格子数上限（防止 cell=1 时把 109k 格全画一遍）。
const MAX_CELLS: i32 = 40_000;

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

/// 坐标哈希 → 0..1 的抖动阈值（纯函数，无状态）。
fn dither01(x: i32, y: i32, seed: u32) -> f32 {
    let h = hash_u32(
        (x as u32)
            .wrapping_mul(0x9E37_79B1)
            ^ (y as u32).wrapping_mul(0x85EB_CA6B)
            ^ seed,
    );
    (h >> 8) as f32 / 16_777_216.0
}

/// 格坐标哈希 → 0..1 的切换阈值（解码转场用）。
fn cell_threshold(seed: u32, cx: i32, cy: i32) -> f32 {
    let h = hash_u32(
        (cx as u32)
            .wrapping_mul(0x27D4_EB2D)
            ^ (cy as u32).wrapping_mul(0x1656_67B1)
            ^ seed,
    );
    (h >> 8) as f32 / 16_777_216.0
}

/// 格坐标哈希 → 该格显示的字形（确定性）。
fn cell_glyph(seed: u32, cx: i32, cy: i32) -> char {
    let h = hash_u32(
        (cx as u32)
            .wrapping_mul(0x85EB_CA6B)
            ^ (cy as u32).wrapping_mul(0xC2B2_AE35)
            ^ seed
            ^ 0x51ED_2701,
    );
    GLYPHS[(h as usize) % GLYPHS.len()]
}

/// 3×3 点阵占位字形：形状由码位决定，同一字符永远同一形状。
///
/// `text` 模块就位前，标题 / 走字 / 解码字形都用它当替身。
fn stamp_char(canvas: &mut CharCanvas, x: f32, y: f32, ch: char, color: Color, w: f32) {
    if w <= 0.0 {
        return;
    }
    let code = ch as u32;
    let bits = code ^ (code >> 5) ^ (code >> 11);
    for row in 0..3u32 {
        for c in 0..3u32 {
            let k = row * 3 + c;
            if bits & (1u32 << (k % 12)) != 0 {
                canvas.set(x + c as f32, y + row as f32, color, w);
            }
        }
    }
    // 保底一点：某些码位可能让 3×3 全灭，字形就「消失」了。
    canvas.set(x + 1.0, y + 1.0, color, w);
}

/// 8 帧 spinner 的帧号（0..8）。
fn spinner_frame(t: f32) -> usize {
    let f = (t * 8.0).floor() as i64;
    f.rem_euclid(SPINNER_FRAMES.len() as i64) as usize
}

/// 把矩形两端规整成 `(x0<=x1, y0<=y1)` 并夹到画布内。
/// 返回 `None` 表示与画布完全不相交。
fn fit_rect(canvas: &CharCanvas, x0: f32, y0: f32, x1: f32, y1: f32) -> Option<(f32, f32, f32, f32)> {
    let (xa, xb) = if x1 >= x0 { (x0, x1) } else { (x1, x0) };
    let (ya, yb) = if y1 >= y0 { (y0, y1) } else { (y1, y0) };
    let sw = canvas.sw as f32;
    let sh = canvas.sh as f32;
    let xa = xa.max(0.0).min(sw - 1.0);
    let ya = ya.max(0.0).min(sh - 1.0);
    let xb = xb.max(0.0).min(sw - 1.0);
    let yb = yb.max(0.0).min(sh - 1.0);
    Some((xa, ya, xb, yb))
}

// ── 光晕：前缀和可分离盒式模糊 ────────────────────────────

/// 复用缓冲：避免每帧分配 109k 点的大 `Vec`。
struct Scratch {
    /// 源 / 模糊结果
    a: Vec<f32>,
    /// 中间缓冲
    b: Vec<f32>,
    /// 一维前缀和（积分图）
    pref: Vec<f32>,
}

impl Scratch {
    fn new() -> Self {
        Self {
            a: Vec::new(),
            b: Vec::new(),
            pref: Vec::new(),
        }
    }
}

thread_local! {
    static SCRATCH: RefCell<Scratch> = RefCell::new(Scratch::new());
}

/// 横向一维盒式模糊：逐行做前缀和，O(w)，核按**实际窗口长度**归一化。
fn blur_rows(a: &[f32], out: &mut [f32], w: usize, h: usize, r: usize, pref: &mut Vec<f32>) {
    if w == 0 || h == 0 {
        return;
    }
    for y in 0..h {
        let row = &a[y * w..y * w + w];
        pref.clear();
        pref.reserve(w + 1);
        pref.push(0.0);
        let mut acc = 0.0f32;
        for v in row {
            acc += *v;
            pref.push(acc);
        }
        let dst = &mut out[y * w..y * w + w];
        for x in 0..w {
            let lo = x.saturating_sub(r);
            let hi = (x + r + 1).min(w);
            dst[x] = (pref[hi] - pref[lo]) / (hi - lo) as f32;
        }
    }
}

/// 纵向一维盒式模糊：逐列做前缀和，O(h)，跨步读取。
fn blur_cols(a: &[f32], out: &mut [f32], w: usize, h: usize, r: usize, pref: &mut Vec<f32>) {
    if w == 0 || h == 0 {
        return;
    }
    for x in 0..w {
        pref.clear();
        pref.reserve(h + 1);
        pref.push(0.0);
        let mut acc = 0.0f32;
        for y in 0..h {
            acc += a[y * w + x];
            pref.push(acc);
        }
        for y in 0..h {
            let lo = y.saturating_sub(r);
            let hi = (y + r + 1).min(h);
            out[y * w + x] = (pref[hi] - pref[lo]) / (hi - lo) as f32;
        }
    }
}

/// 可分离盒式模糊：每轮「横 + 纵」两趟，跑 `iters` 轮。
///
/// 两次迭代 ≈ 三角核（近似高斯），辉光边缘才柔和；单次盒式核是硬边方框。
/// 每趟都是一维前缀和增量求和 → O(w·h)，与半径无关，无 O(r²) 二维卷积。
fn blur_sep(
    a: &mut [f32],
    b: &mut [f32],
    w: usize,
    h: usize,
    r: usize,
    iters: usize,
    pref: &mut Vec<f32>,
) {
    if w == 0 || h == 0 {
        return;
    }
    let r = r.max(1);
    for _ in 0..iters.max(1) {
        blur_rows(&a[..], &mut b[..], w, h, r, pref);
        blur_cols(&b[..], &mut a[..], w, h, r, pref);
    }
}

/// 光晕内核：对画布上 `(rx,ry)` 起、`rw×rh` 大小的区域做元素级辉光。
///
/// `disc` 为 `Some((cx,cy,rad))` 时只把光叠在以 `(cx,cy)` 为心、`rad` 为半径的圆内
/// （局部光晕，用于单个精灵）。
#[allow(clippy::too_many_arguments)]
fn glow_region(
    canvas: &mut CharCanvas,
    rx: i32,
    ry: i32,
    rw: usize,
    rh: usize,
    color: Color,
    r1: usize,
    strength: f32,
    disc: Option<(f32, f32, f32)>,
    seed: u32,
) {
    let n = rw * rh;
    if n == 0 || rw < 2 || rh < 2 {
        return;
    }
    SCRATCH.with(|cell| {
        let mut s = cell.borrow_mut();
        let Scratch { a, b, pref } = &mut *s;
        a.resize(n, 0.0);
        b.resize(n, 0.0);

        // 1) 取 alpha 源：覆盖度 × 感知亮度。
        for j in 0..rh {
            let py = ry + j as i32;
            for i in 0..rw {
                let p = canvas.pixel(rx + i as i32, py);
                let lum = 0.2126 * p.r + 0.7152 * p.g + 0.0722 * p.b;
                a[j * rw + i] = p.w * (0.4 + 0.6 * lum);
            }
        }

        // 2) 可分离盒式模糊（2 轮 = 4 趟一维前缀和扫描）。
        blur_sep(&mut a[..n], &mut b[..n], rw, rh, r1, 2, pref);

        // 3) 核归一化：按模糊后的峰值分摊强度，保证既亮又不糊成一片白。
        let mut peak = 0.0f32;
        for v in &a[..n] {
            if *v > peak {
                peak = *v;
            }
        }
        if peak <= 1e-5 {
            return;
        }
        let scale = strength.clamp(0.0, 1.5) * GLOW_PEAK / peak;

        // 4) 加法叠回：密度抖动（Braille 看得见）+ 连续底光（半块/ASCII 有层次）。
        for j in 0..rh {
            let py = ry + j as i32;
            for i in 0..rw {
                let px = rx + i as i32;
                let mut v = a[j * rw + i] * scale;
                if let Some((cx, cy, rad)) = disc {
                    let dx = px as f32 - cx;
                    let dy = py as f32 - cy;
                    let d = (dx * dx + dy * dy).sqrt();
                    let mut m = (1.0 - d / rad.max(1.0)).clamp(0.0, 1.0);
                    m = m * m * (3.0 - 2.0 * m);
                    v *= m;
                }
                if v <= 0.004 {
                    continue;
                }
                // 感知 gamma：辉光的能量按平方衰减，直接当概率用会在本体外 1~4 点
                // 只剩 20~30% 的点密度（实测），看着像脏点而不是光。开方把低值抬起来，
                // 峰值仍在 1 附近，于是近处密、远处疏，才是「柔和辉光」。
                let v = v.clamp(0.0, 1.0).sqrt();
                let th = dither01(px, py, seed);
                let w = if v > th {
                    GLOW_LIT + 0.25 * v.min(1.0)
                } else {
                    v * 0.25
                };
                canvas.add(px as f32, py as f32, color, w);
            }
        }
    });
}

/// 给「画布上已有的亮部」套一层柔和彩色光晕。
///
/// 做法：把画布当前像素按亮度取出来当作 alpha 源，做一次可分离的盒式模糊
/// （半径 `radius` 点），再用 `canvas.add()` 把模糊结果以 `color` 叠回去。
/// 必须在主体元素画完之后调用。
///
/// `radius` 会被夹到 1..=24 点；`radius <= 0.5` 或 `strength <= 0.001` 时是 no-op。
pub fn glow(canvas: &mut CharCanvas, color: Color, radius: f32, strength: f32) {
    if strength <= 0.001 || radius <= 0.5 {
        return;
    }
    let sw = canvas.sw as usize;
    let sh = canvas.sh as usize;
    if sw < 4 || sh < 4 {
        return;
    }
    // 2 轮迭代 → 支撑约 2×半径，所以单轮半径取一半。
    let r1 = ((radius.clamp(1.0, GLOW_RADIUS_MAX) * 0.5).round() as usize).max(1);
    glow_region(canvas, 0, 0, sw, sh, color, r1, strength, None, GLOW_SEED);
}

/// 局部光晕：只在以 `(cx,cy)` 为中心、半径 `radius` 的圆内做（用于单个精灵，比全屏快）。
///
/// 提取范围是外接矩形，叠光时按圆盘遮罩做软边；圆外的点逐位不变。
pub fn glow_at(canvas: &mut CharCanvas, cx: f32, cy: f32, radius: f32, color: Color, strength: f32) {
    if strength <= 0.001 || radius <= 0.5 {
        return;
    }
    let sw = canvas.sw as i32;
    let sh = canvas.sh as i32;
    let rad = radius.clamp(1.0, 96.0);
    let x0 = ((cx - rad).floor() as i32).max(0);
    let y0 = ((cy - rad).floor() as i32).max(0);
    let x1 = ((cx + rad).ceil() as i32).min(sw - 1);
    let y1 = ((cy + rad).ceil() as i32).min(sh - 1);
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    let rw = (x1 - x0 + 1) as usize;
    let rh = (y1 - y0 + 1) as usize;
    let r1 = ((rad * 0.35).clamp(1.0, 12.0)).round() as usize;
    glow_region(
        canvas,
        x0,
        y0,
        rw,
        rh,
        color,
        r1,
        strength,
        Some((cx, cy, rad)),
        GLOW_AT_SEED,
    );
}

// ── 窗口框 ────────────────────────────────────────────────

/// 标题字号（点阵单位）：标题栏占 3 个点行，所以取 3.0。
const TITLE_PX: f32 = 3.0;

/// 标题文字：走 `text::draw`（有字体画真字，无字体该模块自己降级成 2×3 方块）。
///
/// `y` 传的是标题栏顶行，内部换算成字体基线。
fn draw_title(canvas: &mut CharCanvas, x: f32, y: f32, title: &str, color: Color, alpha: f32) {
    if title.is_empty() || alpha <= 0.0 {
        return;
    }
    crate::render::text::draw(
        canvas,
        x,
        y + TITLE_PX * 0.85,
        title,
        TITLE_PX,
        color,
        alpha,
    );
}

/// 右上角 spinner：8 帧 `|/-\` 序列映射成绕小圆一圈的 8 个角度。
///
/// 终端里没有字体，所以「转圈」本身用点画出来；帧字符再淡淡地打一份做呼应。
fn draw_spinner(canvas: &mut CharCanvas, x: f32, y: f32, t: f32, color: Color, alpha: f32) {
    let frame = spinner_frame(t);
    let a = frame as f32 / SPINNER_FRAMES.len() as f32 * std::f32::consts::TAU;
    // 当前位置：亮点
    let px = x + a.cos() * 2.0;
    let py = y + a.sin() * 2.0;
    canvas.disc(px, py, 1.0, col::lerp(col::WHITE, color, 0.3), 0.85 * alpha);
    // 拖尾：前两帧
    for k in 1..=2u32 {
        let aa = a - k as f32 * (std::f32::consts::TAU / 8.0);
        let tx = x + aa.cos() * 2.0;
        let ty = y + aa.sin() * 2.0;
        canvas.set(tx, ty, color, 0.55 * alpha * (1.0 - k as f32 * 0.3));
    }
    // 帧字符：淡淡一份，保留 `|/-\` 的语义
    stamp_char(canvas, x + 4.0, y - 1.0, SPINNER_FRAMES[frame], color, 0.45 * alpha);
}

/// 画一个终端窗口框（带标题栏和右下角转圈 spinner）。
///
/// `(x0,y0,x1,y1)` 是点阵单位的外框（顺序颠倒会自动规整）；`title` 是标题文字；
/// `level` 0..=1 控制整体亮度；`t` 用于 spinner 转动与边框呼吸。标题栏占顶部 3 个点行。
#[allow(clippy::too_many_arguments)]
pub fn window_box(
    canvas: &mut CharCanvas,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    title: &str,
    level: f32,
    t: f32,
    color: Color,
) {
    let lv = level.clamp(0.0, 1.0);
    if lv <= 0.01 {
        return;
    }
    let Some((xa, ya, xb, yb)) = fit_rect(canvas, x0, y0, x1, y1) else {
        return;
    };
    if xb - xa < 2.0 || yb - ya < 2.0 {
        // 太小：退化成一条对角线，至少不 panic。
        canvas.line(xa, ya, xb, yb, color, 0.6 * lv);
        return;
    }

    // 边框呼吸：0.72..1.0，连续不跳。
    let breath = 0.72 + 0.28 * (t * 2.1).sin();
    let w = (0.55 + 0.35 * lv) * breath;

    // 外框
    canvas.line(xa, ya, xb, ya, color, w);
    canvas.line(xb, ya, xb, yb, color, w);
    canvas.line(xb, yb, xa, yb, color, w);
    canvas.line(xa, yb, xa, ya, color, w);

    // 四角加粗（「窗口」的手感）
    let cl = ((xb - xa) * 0.12).max(1.5).min((xb - xa) * 0.4);
    for (cx, cy, dx, dy) in [
        (xa, ya, 1.0f32, 1.0f32),
        (xb, ya, -1.0, 1.0),
        (xa, yb, 1.0, -1.0),
        (xb, yb, -1.0, -1.0),
    ] {
        canvas.line(cx, cy, cx + dx * cl, cy, color, w * 1.4);
        canvas.line(cx, cy, cx, cy + dy * cl, color, w * 1.4);
    }

    // 标题栏：顶部 3 个点行。用棋盘抖动填充 —— Braille 下是疏密有致的底纹，
    // 不是一堵实心墙（整块 fill_rect 会让 2×4 点全亮，看着像白条）。
    let bar_top = ya + 1.0;
    let bar_bot = (ya + 3.0).min(yb - 1.0);
    if bar_bot > bar_top {
        let bar_tone = col::lerp(col::BLACK, color, 0.35);
        let y0i = bar_top as i32;
        let y1i = bar_bot as i32;
        let x0i = (xa + 1.0) as i32;
        let x1i = (xb - 1.0) as i32;
        for yy in y0i..=y1i {
            for xx in x0i..=x1i {
                if ((xx + yy) & 1) == 0 {
                    canvas.set(xx as f32, yy as f32, bar_tone, 0.62 * lv);
                }
            }
        }
        // 标题栏下沿分隔线
        let sep = (bar_bot + 1.0).min(yb);
        canvas.line(xa, sep, xb, sep, color, 0.45 * lv);
        // 标题 + spinner
        draw_title(canvas, xa + 3.0, bar_top + 1.0, title, color, lv);
        draw_spinner(canvas, (xb - 6.0).max(xa + 2.0), bar_top + 1.0, t, color, lv);
    }
}

// ── 走字带 ────────────────────────────────────────────────

/// 画一条「走字带」：在给定矩形内水平滚动的一行小字，做出终端输出的观感。
///
/// `speed` 单位是 点/秒；`alpha` 是整体不透明度。矩形两端的字会自然淡出。
#[allow(clippy::too_many_arguments)]
pub fn ticker(
    canvas: &mut CharCanvas,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    text: &str,
    speed: f32,
    t: f32,
    color: Color,
    alpha: f32,
) {
    let a = alpha.clamp(0.0, 1.0);
    if a <= 0.02 || text.is_empty() {
        return;
    }
    let Some((xa, ya, xb, yb)) = fit_rect(canvas, x0, y0, x1, y1) else {
        return;
    };
    if xb - xa < 4.0 || yb - ya < 2.0 {
        return;
    }

    const ADV: f32 = 4.0; // 每字 3 点宽 + 1 点间距
    let span = xb - xa;
    let text_w = text.chars().count() as f32 * ADV;
    let total = text_w + span;
    if total <= 1.0 {
        return;
    }
    // + span 的相位偏移：t=0 时文字已经进场，不是空带。
    let off = (t * speed).rem_euclid(total) + span;
    let y = ((ya + yb) * 0.5).round() - 1.0;

    // 底部基准线：一条很淡的底线，让走字有「行」的感觉。
    canvas.line(xa, yb, xb, yb, col::lerp(col::BLACK, color, 0.5), 0.4 * a);

    let mut x = xa - text_w + off;
    for ch in text.chars() {
        if x + 3.0 >= xa && x <= xb {
            // 两端 4 点内线性淡出
            let edge = ((x - xa).min(xb - x - 3.0)).clamp(0.0, 4.0) / 4.0;
            stamp_char(canvas, x, y, ch, color, a * (0.35 + 0.65 * edge));
        }
        x += ADV;
    }
}

// ── 逐格解码转场 ──────────────────────────────────────────

/// 逐格解码转场：把 `rect` 区域按格子分成 `cell_x × cell_y` 的小块，
/// 每块按哈希阈值决定何时切换。`progress` 0..=1 是整体进度。
///
/// 每格显示 `GLYPHS` 里的一个随机字符（由 `seed + 格坐标` 哈希决定，确定性），
/// 阈值接近当前进度的格子额外压一块更亮的「扫描前沿」。
#[allow(clippy::too_many_arguments)]
pub fn decode_wipe(
    canvas: &mut CharCanvas,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    cell_x: f32,
    cell_y: f32,
    progress: f32,
    t: f32,
    color: Color,
    seed: u32,
) {
    let p = progress.clamp(0.0, 1.0);
    let Some((xa, ya, xb, yb)) = fit_rect(canvas, x0, y0, x1, y1) else {
        return;
    };
    let mut cw = cell_x.max(1.0);
    let mut chh = cell_y.max(1.0);
    let rect_w = xb - xa;
    let rect_h = yb - ya;
    if rect_w < 1.0 || rect_h < 1.0 {
        return;
    }

    // 格子数保护：cell 给得过小时按比例放大，宁可粗一点也不卡死。
    let mut cols;
    let mut rows;
    let mut guard = 0;
    loop {
        cols = ((rect_w / cw).ceil() as i32).max(1);
        rows = ((rect_h / chh).ceil() as i32).max(1);
        if cols * rows <= MAX_CELLS || guard >= 8 {
            break;
        }
        cw *= 1.5;
        chh *= 1.5;
        guard += 1;
    }

    // 扫描前沿的闪烁（用 t 但保持确定性）
    let flick = 0.85 + 0.15 * (t * 12.0).sin();

    for cy in 0..rows {
        for cx in 0..cols {
            let th = cell_threshold(seed, cx, cy);
            let gx = xa + cx as f32 * cw;
            let gy = ya + cy as f32 * chh;
            if p > th {
                // 已解码：密度随「刚切过」的程度从高到低
                let fresh = (1.0 - (p - th).min(1.0)).clamp(0.0, 1.0);
                let gl = cell_glyph(seed, cx, cy);
                stamp_char(
                    canvas,
                    gx + cw * 0.25,
                    gy + chh * 0.25,
                    gl,
                    color,
                    0.55 + 0.25 * fresh,
                );
            }
            // 扫描前沿：阈值就在当前进度附近的格子压一块更亮的方块
            let d = (th - p).abs();
            if p > 0.01 && d < 0.06 {
                let k = 1.0 - d / 0.06;
                canvas.fill_rect(
                    gx,
                    gy,
                    cw.max(1.0),
                    chh.max(1.0),
                    col::lerp(col::WHITE, color, 0.30),
                    (0.55 + 0.35 * k) * flick,
                );
            }
        }
    }
}

// ── 测试 ──────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RenderMode;

    const FRAME: Color = Color::Rgb(150, 200, 255);

    fn canvas() -> CharCanvas {
        CharCanvas::new(80, 24, RenderMode::Braille)
    }

    /// 有内容的小画面：一条斜线 + 一个圆 + 一个实心点。
    fn scene() -> CharCanvas {
        let mut c = canvas();
        c.line(10.0, 10.0, 120.0, 60.0, Color::Rgb(200, 220, 255), 0.8);
        c.circle(80.0, 48.0, 18.0, Color::Rgb(160, 200, 255), 0.7);
        c.disc(30.0, 20.0, 4.0, Color::Rgb(255, 180, 120), 0.9);
        c
    }

    /// 逐位签名：w/r/g/b 的原始位。
    fn sig(c: &CharCanvas) -> Vec<u32> {
        c.pixels()
            .iter()
            .flat_map(|p| [p.w.to_bits(), p.r.to_bits(), p.g.to_bits(), p.b.to_bits()])
            .collect()
    }

    fn lit(c: &CharCanvas) -> usize {
        c.pixels().iter().filter(|p| !p.is_empty()).count()
    }

    fn sum_w(c: &CharCanvas) -> f32 {
        c.pixels().iter().map(|p| p.w).sum()
    }

    // ── glow ──

    #[test]
    fn glow_raises_brightness_without_lowering_w() {
        let mut c = scene();
        let before: Vec<f32> = c.pixels().iter().map(|p| p.w).collect();
        let lit_before = lit(&c);
        let sum_before = sum_w(&c);
        glow(&mut c, FRAME, 8.0, 0.9);
        let after: Vec<f32> = c.pixels().iter().map(|p| p.w).collect();
        assert!(lit(&c) > lit_before * 2, "光晕没有扩散：{lit_before} → {}", lit(&c));
        assert!(sum_w(&c) > sum_before * 1.2, "总亮度没上去");
        for (a, b) in before.iter().zip(after.iter()) {
            assert!(*b >= *a - 1e-6, "光晕抹掉了本体：{a} → {b}");
        }
    }

    #[test]
    fn glow_does_not_overexpose() {
        let mut c = scene();
        glow(&mut c, FRAME, 8.0, 1.0);
        let n = c.pixels().len();
        let sat = c.pixels().iter().filter(|p| p.w >= 0.99).count();
        assert!(
            (sat as f32) / (n as f32) < 0.40,
            "过曝：{sat}/{n} 个点饱和"
        );
    }

    #[test]
    fn glow_is_noop_for_zero_radius_or_strength() {
        let base = scene();
        let want = sig(&base);

        let mut a = scene();
        glow(&mut a, FRAME, 0.0, 0.9);
        assert_eq!(sig(&a), want, "radius=0 应逐位不变");

        let mut b = scene();
        glow(&mut b, FRAME, 8.0, 0.0);
        assert_eq!(sig(&b), want, "strength=0 应逐位不变");

        let mut d = scene();
        glow(&mut d, FRAME, -4.0, 0.9);
        assert_eq!(sig(&d), want, "负半径应逐位不变");
    }

    #[test]
    fn glow_at_is_local_and_visible() {
        let mut c = scene();
        let far_before = c.pixel(4, 4);
        let center_before = sum_w(&c);
        glow_at(&mut c, 80.0, 48.0, 10.0, FRAME, 0.9);
        // 圆外逐位不变
        let far_after = c.pixel(4, 4);
        assert_eq!(
            (
                far_before.w.to_bits(),
                far_before.r.to_bits(),
                far_before.g.to_bits(),
                far_before.b.to_bits()
            ),
            (
                far_after.w.to_bits(),
                far_after.r.to_bits(),
                far_after.g.to_bits(),
                far_after.b.to_bits()
            ),
            "局部光晕漏到了圆外"
        );
        assert!(sum_w(&c) > center_before, "局部光晕没加上亮度");
    }

    #[test]
    fn glow_at_offcanvas_is_safe() {
        let mut c = scene();
        let want = sig(&c);
        glow_at(&mut c, -500.0, -500.0, 10.0, FRAME, 1.0);
        glow_at(&mut c, 10_000.0, 10_000.0, 10.0, FRAME, 1.0);
        assert_eq!(sig(&c), want, "画布外的局部光晕不应改动画面");
    }

    // ── window_box ──

    #[test]
    fn window_box_no_panic_on_tiny_and_inverted() {
        let mut tiny = CharCanvas::new(8, 5, RenderMode::Braille);
        window_box(&mut tiny, 2.0, 2.0, 14.0, 18.0, "TTY", 1.0, 0.0, FRAME);
        window_box(&mut tiny, 0.0, 0.0, 1.0, 1.0, "", 0.5, 1.0, FRAME);

        let mut c = canvas();
        // 顺序颠倒 / 零尺寸 / 画布外，都不许 panic
        window_box(&mut c, 100.0, 60.0, 20.0, 10.0, "INV", 1.0, 0.5, FRAME);
        window_box(&mut c, 40.0, 30.0, 40.0, 30.0, "PT", 1.0, 0.5, FRAME);
        window_box(&mut c, -500.0, -500.0, 500.0, 500.0, "HUGE", 1.0, 0.5, FRAME);
        window_box(&mut c, 10.0, 10.0, 100.0, 80.0, "X", 0.0, 0.0, FRAME);
    }

    #[test]
    fn window_box_draws_frame_title_and_spinner() {
        let mut c = canvas();
        window_box(&mut c, 10.0, 10.0, 150.0, 90.0, "SHUNKASHUTO", 1.0, 0.3, FRAME);
        assert!(lit(&c) > 60, "窗口框只画了 {} 个点", lit(&c));
        // 标题栏区域应该被底纹点亮
        let bar = (12..=18)
            .flat_map(|x| (11..=13).map(move |y| (x, y)))
            .filter(|(x, y)| !c.pixel(*x, *y).is_empty())
            .count();
        assert!(bar >= 8, "标题栏底纹没画出来（{bar} 点）");
    }

    #[test]
    fn window_box_is_deterministic() {
        let mut a = canvas();
        let mut b = canvas();
        window_box(&mut a, 5.0, 5.0, 120.0, 80.0, "DET", 0.9, 1.7, FRAME);
        window_box(&mut b, 5.0, 5.0, 120.0, 80.0, "DET", 0.9, 1.7, FRAME);
        assert_eq!(sig(&a), sig(&b));
    }

    // ── ticker ──

    #[test]
    fn ticker_draws_and_scrolls() {
        let mut a = canvas();
        ticker(&mut a, 4.0, 40.0, 150.0, 52.0, "loading model weights ...", 12.0, 0.0, FRAME, 1.0);
        assert!(lit(&a) > 10, "走字带没画出东西：{} 点", lit(&a));

        let mut b = canvas();
        ticker(&mut b, 4.0, 40.0, 150.0, 52.0, "loading model weights ...", 12.0, 1.5, FRAME, 1.0);
        assert_ne!(sig(&a), sig(&b), "走字带没有滚动");

        // 空文本 / alpha=0 → no-op
        let mut c = canvas();
        let want = sig(&c);
        ticker(&mut c, 4.0, 40.0, 150.0, 52.0, "", 12.0, 1.0, FRAME, 1.0);
        ticker(&mut c, 4.0, 40.0, 150.0, 52.0, "x", 12.0, 1.0, FRAME, 0.0);
        assert_eq!(sig(&c), want);
    }

    // ── decode_wipe ──

    #[test]
    fn decode_wipe_zero_progress_is_identity() {
        let mut c = scene();
        let want = sig(&c);
        decode_wipe(&mut c, 10.0, 10.0, 150.0, 90.0, 6.0, 8.0, 0.0, 0.0, FRAME, 7);
        assert_eq!(sig(&c), want, "progress=0 不应改动画面");
    }

    #[test]
    fn decode_wipe_full_progress_switches_everything() {
        let mut c = scene();
        let before = lit(&c);
        decode_wipe(&mut c, 10.0, 10.0, 150.0, 90.0, 6.0, 8.0, 1.0, 0.0, FRAME, 7);
        assert!(lit(&c) > before, "progress=1 没有铺开解码字形");
        // 所有格子的阈值都 < 1.0 → progress=1 必然全部切换
        let mut all = 0;
        let mut pass = 0;
        for cy in 0..24 {
            for cx in 0..40 {
                all += 1;
                if cell_threshold(7, cx, cy) < 1.0 {
                    pass += 1;
                }
            }
        }
        assert_eq!(pass, all, "有格子阈值 >= 1.0，progress=1 切不干净");
    }

    #[test]
    fn decode_wipe_is_deterministic_and_safe() {
        let mut a = scene();
        let mut b = scene();
        decode_wipe(&mut a, 0.0, 0.0, 150.0, 90.0, 5.0, 7.0, 0.45, 2.0, FRAME, 99);
        decode_wipe(&mut b, 0.0, 0.0, 150.0, 90.0, 5.0, 7.0, 0.45, 2.0, FRAME, 99);
        assert_eq!(sig(&a), sig(&b), "同一 t 两次调用必须逐位一致");

        // 颠倒矩形 / 超小格子 / 零区域，都不许 panic
        let mut c = canvas();
        decode_wipe(&mut c, 150.0, 90.0, 10.0, 10.0, 1.0, 1.0, 0.5, 0.0, FRAME, 1);
        decode_wipe(&mut c, 20.0, 20.0, 20.0, 20.0, 4.0, 4.0, 0.5, 0.0, FRAME, 1);
        decode_wipe(&mut c, 0.0, 0.0, 200.0, 100.0, 0.0, 0.0, 0.5, 0.0, FRAME, 1);
    }

    // ── 全局确定性 ──

    #[test]
    fn all_apis_are_pure_functions_of_their_inputs() {
        let run = || {
            let mut c = scene();
            glow(&mut c, FRAME, 10.0, 0.8);
            glow_at(&mut c, 60.0, 40.0, 12.0, Color::Rgb(255, 120, 200), 0.7);
            window_box(&mut c, 4.0, 4.0, 140.0, 88.0, "PURE", 0.9, 3.3, FRAME);
            ticker(&mut c, 8.0, 60.0, 150.0, 70.0, "pure fn", 9.0, 3.3, FRAME, 0.9);
            decode_wipe(&mut c, 20.0, 20.0, 140.0, 80.0, 8.0, 10.0, 0.6, 3.3, FRAME, 5);
            sig(&c)
        };
        assert_eq!(run(), run(), "同一输入必须逐位一致");
    }
}

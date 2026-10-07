//! 八个场景的具体绘制。
//!
//! 每个场景都是**纯函数**：给定时间与音乐特征，画出一帧。
//! 没有跨帧可变状态（粒子系统的状态除外，它由外部持有）——
//! 因此 seek 到任意位置都能得到同一画面，导出与实时播放也必然一致。
//!
//! 场景只往 [`CharCanvas`] 上画几何图形，具体是 Braille、半块还是 ASCII
//! 由画布自己决定，场景代码不关心。

use rand::rngs::SmallRng;
use rand::Rng;
use ratatui::style::Color;

use crate::audio::features::FrameFeatures;
use crate::lyrics::Lyrics;
use crate::render::color as col;
use crate::render::particles::ParticleSystem;
use crate::render::postfx::PostFx;
use crate::render::CharCanvas;
use crate::timeline::scene::SceneKind;

/// 场景绘制所需的全部输入。
pub struct SceneCtx<'a> {
    /// 全曲时间（秒）
    pub t: f64,
    /// 段内时间（秒）
    pub local_t: f64,
    /// 段内进度 0..1
    pub progress: f32,
    /// 当前帧特征（相邻帧插值后）
    pub feat: FrameFeatures,
    /// 上一帧特征
    pub prev: FrameFeatures,
    /// 帧号
    pub frame: u64,
    /// 字符列数
    pub cols: u16,
    /// 字符行数
    pub rows: u16,
    /// 当前场景
    pub scene: SceneKind,
    /// 歌词（可能为空）
    pub lyrics: &'a Lyrics,
    /// 当前事件强度
    pub flash: f32,
    pub punch: f32,
    pub heartbeat: f32,
    pub glitch: f32,
    pub burst: f32,
    pub invert: f32,
}

impl<'a> SceneCtx<'a> {
    /// 子像素宽高。
    fn sw(&self, c: &CharCanvas) -> f32 {
        c.sw as f32
    }
    fn sh(&self, c: &CharCanvas) -> f32 {
        c.sh as f32
    }

    /// 画布中心（子像素坐标）。
    pub fn center(&self, c: &CharCanvas) -> (f32, f32) {
        (c.sw as f32 / 2.0, c.sh as f32 / 2.0)
    }

    /// 画面短边的一半 —— 所有尺寸都按它缩放，保证任意分辨率下比例一致。
    pub fn radius(&self, c: &CharCanvas) -> f32 {
        (c.sw.min(c.sh)) as f32 * 0.5
    }

    /// 当前行歌词文本。
    pub fn lyric(&self) -> Option<&str> {
        self.lyrics.current(self.t).map(|l| l.text.as_str())
    }

    /// 低音能量（sub + bass）。
    pub fn low(&self) -> f32 {
        (self.feat.bands_norm[0] * 0.5 + self.feat.bands_norm[1] * 0.5).clamp(0.0, 1.0)
    }

    /// 中音能量。
    pub fn mid(&self) -> f32 {
        (self.feat.bands_norm[2] + self.feat.bands_norm[3]) * 0.5
    }

    /// 高音能量。
    pub fn high(&self) -> f32 {
        (self.feat.bands_norm[4] * 0.4 + self.feat.bands_norm[5] * 0.6).clamp(0.0, 1.0)
    }
}

/// 面板布局：把画布切成左右两块。
#[derive(Debug, Clone, Copy)]
pub struct Panes {
    /// 左面板（她）：角色区
    pub left: (f32, f32, f32, f32),
    /// 右面板（数据）：可视化区
    pub right: (f32, f32, f32, f32),
    /// 是否有足够空间分栏
    pub split: bool,
}

/// 画面板框架与右侧数据区。
///
/// 分栏阈值取 72 列：再窄就上下排而不是左右排，
/// 保证 80×24 的最小规格下也不会挤成一团。
fn draw_panes(ctx: &SceneCtx, canvas: &mut CharCanvas) -> Panes {
    let sw = canvas.sw as f32;
    let sh = canvas.sh as f32;
    let [main, alt, _accent] = ctx.scene.palette();

    // 顶部留 2 行给标题栏，底部留 1 行给刻度
    let top = 2.0;
    let bottom = sh - 1.0;
    let split = sw >= 72.0;

    let (left, right) = if split {
        // 她占左侧 62%（画面主角），数据栏退到右侧 38%
        let lw = (sw * 0.62).floor();
        (
            (1.0, top, lw - 1.5, bottom),
            (lw + 0.5, top, sw - lw - 2.0, bottom),
        )
    } else {
        // 窄屏不分栏，右面板退化为整幅
        ((1.0, top, sw - 2.0, bottom), (1.0, top, sw - 2.0, bottom))
    };

    let frame_w = 0.55 + ctx.feat.energy_norm * 0.45;
    let dim = col::lerp(col::BLACK, main, 0.45);

    // ── 标题栏 ──
    draw_title_bar(ctx, canvas);

    if split {
        // 两块面板的边框
        for r in [left, right] {
            draw_panel_frame(canvas, r, dim, frame_w);
        }
        // 分隔线
        canvas.line(
            left.0 + left.2 + 0.5,
            top,
            left.0 + left.2 + 0.5,
            bottom,
            col::lerp(col::BLACK, alt, 0.35),
            0.5,
        );
    }

    let p = Panes {
        left,
        right,
        split,
    };

    // ── 左面板底部：滚动指令流（给画面提供持续变化的文字内容）──
    draw_op_stream(ctx, canvas, p.left);

    // ── 右面板：数据可视化 ──
    draw_data_pane(ctx, canvas, p.right);

    p
}

/// 顶部标题栏：左侧标题、右侧章节与计时。
fn draw_title_bar(ctx: &SceneCtx, canvas: &mut CharCanvas) {
    let sw = canvas.sw as f32;
    let [main, alt, _] = ctx.scene.palette();
    let c = col::lerp(col::BLACK, main, 0.8);

    // 上下两条横线夹出标题栏
    canvas.line(1.0, 0.6, sw - 1.0, 0.6, c, 0.7);
    canvas.line(1.0, 1.8, sw - 1.0, 1.8, col::lerp(col::BLACK, alt, 0.4), 0.5);

    // 左上角的「系统标识」：几个短竖条，像终端的活动指示
    let bars = 4;
    for i in 0..bars {
        let x = 2.0 + i as f32 * 1.5;
        // 高度随频段起伏 —— 这是「系统在工作」的视觉信号
        let h = 0.2 + ctx.feat.bands_norm[i.min(5)] * 0.8;
        canvas.line(x, 1.5 - h * 0.9, x, 1.5, c, 0.9);
    }
    // 活动方块：随节拍闪烁
    let blink = ctx.feat.beat > 0.5;
    if blink {
        canvas.fill_rect(9.0, 0.9, 1.0, 0.8, col::WHITE, 1.0);
    }
}

/// 左面板底部的滚动指令流。
///
/// 参考成熟实现的做法：一列持续滚动的"系统操作"，中间一行反色高亮当光标，
/// 离光标越远越暗。它给画面提供了**持续变化的文字内容**，
/// 是「画面空」最直接的解药 —— 观众总能在某个位置读到正在发生的事。
fn draw_op_stream(ctx: &SceneCtx, canvas: &mut CharCanvas, r: (f32, f32, f32, f32)) {
    let (px, py, pw, ph) = r;
    // 只占面板下半部分
    let y0 = py + ph * 0.58;
    let height = ph * 0.42;
    if pw < 12.0 || height < 4.0 {
        return;
    }

    let [main, alt, _] = ctx.scene.palette();
    // 每帧都要有的「操作」：由时间与频段拼出来，保证确定性且不重复
    let ops = op_lines(ctx);
    if ops.is_empty() {
        return;
    }

    // 行高 1 字符，滚动速度随低频推进
    let speed = 1.6 + ctx.feat.bands_norm[0] * 4.0;
    let scroll = ctx.t as f32 * speed;
    let rows = height.floor().max(1.0) as usize;
    let base = scroll.floor() as usize;
    let cursor_row = rows / 2;

    let x = px + 1.5;
    let avail = (pw - 3.0).floor().max(4.0) as usize;

    for i in 0..rows {
        let y = y0 + i as f32;
        let idx = (base + i) % ops.len();
        let raw = &ops[idx];
        // 截断到面板宽度
        let text: String = raw.chars().take(avail).collect();

        if i == cursor_row {
            // 光标行：反色高亮，像终端里被选中的那条
            let c = col::lerp(col::BLACK, main, 0.9);
            canvas.fill_rect(x - 0.5, y - 0.35, avail as f32 + 1.0, 0.9, c, 0.82);
            // 反色文字：在色块上打暗点，形成「黑字」
            draw_faux_text(canvas, x, y, &text, col::BLACK, 0.95);
        } else {
            // 距离光标越远越暗
            let dist = (i as f32 - cursor_row as f32).abs() / rows.max(1) as f32;
            let a = (0.62 - dist * 0.85).max(0.14);
            let c = col::lerp(col::BLACK, alt, 0.55);
            draw_faux_text(canvas, x, y, &text, c, a);
        }
    }
}

/// 由时间与音乐特征合成的「系统操作」列表。
///
/// 不写死固定文本，而是按帧特征查表拼装 —— 这样它读起来像系统真的在活动，
/// 而不是循环播放一段常量。也刻意只用 ASCII 与符号，避免依赖任何字体。
fn op_lines(ctx: &SceneCtx) -> Vec<String> {
    let verbs = [
        "EXEC", "PUSH", "LOAD", "SYNC", "SCAN", "GRAD", "STEP", "EVAL", "CALL", "MOVE", "READ",
        "WRIT", "HALT", "JUMP", "TEST", "FORK",
    ];
    let objs = [
        "self.weights", "tensor[3]", "param.vec", "grad.buf", "loss.val", "optim.st", "input.x",
        "hidden.h", "output.y", "reward.r", "policy.pi", "memory.m",
    ];
    let kinds = ["ok", "ok", "ok", "warn", "err", "ok", "info"];
    let band_names = ["sub", "bass", "lowmid", "mid", "highmid", "high"];

    let n = 40usize;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let h = (i as u32).wrapping_mul(2654435761);
        let v = verbs[((h >> 8) & 0xFF) as usize % verbs.len()];
        let o = objs[((h >> 16) & 0xFF) as usize % objs.len()];
        let k = kinds[((h >> 24) & 0xFF) as usize % kinds.len()];
        // 让当前音乐特征影响一部分行，画面与声音就绑上了
        let b = (h >> 4) as usize % 6;
        let mark = if ctx.feat.bands_norm[b] > 0.45 { "!" } else { " " };
        out.push(format!(
            "[{k}] {v} {o} {}={:.2}{}",
            band_names[b], ctx.feat.bands_norm[b], mark
        ));
    }
    out
}

/// 用点阵绘制一段"伪文字"。
///
/// 不追求可读的字母（那需要内置字体），而是把字符的**形状密度**映射成
/// 一行有节奏的点 —— 远看像终端里的一行日志，近看是抽象纹理。
/// 每个字符按其 ASCII 码决定点亮几列，保证同一文本每次渲染一致。
fn draw_faux_text(canvas: &mut CharCanvas, x: f32, y: f32, text: &str, color: Color, alpha: f32) {
    let mut cx = x;
    for ch in text.chars() {
        if ch == ' ' {
            cx += 0.5;
            continue;
        }
        let code = ch as u32;
        // 按码位取出 4 位，决定这一列里哪几个子行被点亮
        let bits = (code ^ (code >> 5)) & 0xF;
        for k in 0..5 {
            if bits & (1 << (k % 4)) != 0 {
                canvas.set(cx, y - 0.3 + k as f32 * 0.16, color, alpha * 0.9);
            }
        }
        cx += 1.0;
    }
}

/// 画一块面板的边框（四角加粗，做出「窗口」的感觉）。
fn draw_panel_frame(canvas: &mut CharCanvas, r: (f32, f32, f32, f32), color: Color, w: f32) {
    let (x, y, ww, hh) = r;
    if ww < 4.0 || hh < 3.0 {
        return;
    }
    canvas.rect(x, y, ww, hh, color, w * 0.6);
    // 四角加重，像终端窗口的边角
    let cl = (ww * 0.12).max(1.5);
    for &(cxx, cyy, dx, dy) in &[
        (x, y, 1.0f32, 1.0f32),
        (x + ww, y, -1.0, 1.0),
        (x, y + hh, 1.0, -1.0),
        (x + ww, y + hh, -1.0, -1.0),
    ] {
        canvas.line(cxx, cyy, cxx + dx * cl, cyy, color, w);
        canvas.line(cxx, cyy, cxx, cyy + dy * cl, color, w);
    }
}

/// 右面板：实时数据可视化。
///
/// 三层叠在一起，让画面「有内容」：
/// 1. 上半：逐 bin 频谱柱（128 点降到面板宽度）
/// 2. 中部：六个频段的横向条形图 + 各自的峰值保持
/// 3. 下半：随时间滚动的能量曲线（自绘，确定性）
fn draw_data_pane(ctx: &SceneCtx, canvas: &mut CharCanvas, r: (f32, f32, f32, f32)) {
    let (px, py, pw, ph) = r;
    if pw < 16.0 || ph < 10.0 {
        return;
    }
    let [main, alt, accent] = ctx.scene.palette();
    let inner_x = px + 1.5;
    let inner_w = pw - 3.0;

    // ── 1) 频谱柱 ──
    let spec_h = (ph * 0.32).max(4.0);
    let spec_y0 = py + 1.5;
    let bars = inner_w.floor().max(4.0) as usize;
    let bins = ctx.feat.spectrum.len();
    if bins > 0 {
        for i in 0..bars {
            // 按对数分布取样，低频占更多柱
            let u = i as f32 / bars as f32;
            let idx = ((u.powf(1.6) * (bins - 1) as f32) as usize).min(bins - 1);
            // 重力平滑：上升跟瞬时值（反应快），下降用回溯峰值（不抖）。
            // 这是 cava 的做法 —— 原始频谱信号噪声很大，直接画会乱闪。
            // 这里用"回溯采样"而不是缓存上一帧，保持 frame(t) 是纯函数。
            let v = gravity_smoothed(ctx, idx, u);
            let h = v * spec_h;
            let x = inner_x + i as f32;
            if h < 0.4 {
                // 极小值也画一个地基点，避免柱子忽隐忽现
                canvas.set(x, spec_y0 + spec_h, col::lerp(col::BLACK, alt, 0.3), 0.55);
                continue;
            }
            // 从下往上填充
            let col_c = col::lerp(main, accent, v.powf(0.6));
            for k in 0..h.floor() as usize {
                let y = spec_y0 + spec_h - k as f32;
                if y < spec_y0 || y > py + ph {
                    continue;
                }
                canvas.set(x, y, col_c, 0.85);
            }
            // 峰值点
            canvas.add(x, spec_y0 + spec_h - h, col::WHITE, 0.9);
        }
    }

    // ── 2) 六频段条形图 ──
    let band_y = spec_y0 + spec_h + 1.5;
    let band_w = inner_w * 0.62;
    for b in 0..6 {
        let v = ctx.feat.bands_norm[b].clamp(0.0, 1.0);
        let y = band_y + b as f32;
        if y > py + ph - 4.0 {
            break;
        }
        // 轨道
        canvas.line(inner_x, y, inner_x + band_w, y, col::lerp(col::BLACK, alt, 0.35), 0.55);
        // 已填充部分
        if v > 0.02 {
            canvas.line(inner_x, y, inner_x + band_w * v, y, main, 0.9);
        }
        // 峰值刻度线
        canvas.line(
            inner_x + band_w * v,
            y - 0.6,
            inner_x + band_w * v,
            y + 0.6,
            accent,
            0.8,
        );
        // 右侧数值点阵（3 格）
        let lit = (v * 3.0).round() as usize;
        for k in 0..3 {
            let bx = inner_x + band_w + 1.0 + k as f32 * 1.2;
            let c = if k < lit { main } else { col::lerp(col::BLACK, alt, 0.25) };
            canvas.set(bx, y, c, if k < lit { 0.9 } else { 0.5 });
        }
    }

    // ── 3) 能量曲线 ──
    let curve_y = band_y + 6.5;
    let curve_h = (py + ph - curve_y - 1.5).max(3.0);
    if curve_h >= 3.0 {
        let n = inner_w.floor().max(8.0) as usize;
        let mut prev: Option<(f32, f32)> = None;
        for i in 0..n {
            // 回溯看过去的能量，形成滚动曲线
            let back = (n - i) as f64 / n as f64 * 6.0;
            let tq = (ctx.t - back).max(0.0);
            let e = ctx.prev.energy_norm * 0.0 + energy_at(ctx, tq);
            let x = inner_x + i as f32;
            let y = curve_y + curve_h * (1.0 - e.clamp(0.0, 1.0) * 0.92);
            if let Some((px0, py0)) = prev {
                canvas.line(px0, py0, x, y, col::lerp(main, col::WHITE, 0.3), 0.8);
            }
            prev = Some((x, y));
        }
    }
}

/// 把两个整数哈希成两个 [0,1) 的浮点（确定性、跨平台一致）。
///
/// 只用整数乘法与移位 —— 绝不用 `fract(sin(x) * 43758.5453)`，
/// 那种做法在不同引擎/编译器下结果不同，会破坏可复现性。
fn hash2(i: u32, seed: u32) -> (f32, f32) {
    let a = hash_u32(i.wrapping_mul(0x9E37_79B1).wrapping_add(seed));
    let b = hash_u32(a ^ 0x85EB_CA6B);
    let to_unit = |v: u32| (v >> 8) as f32 / 16_777_216.0;
    (to_unit(a), to_unit(b))
}

/// 单个 u32 的整数混合哈希（xxHash 风格的移位-乘法混合）。
fn hash_u32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^= x >> 16;
    x
}

/// 三层深度的视觉倍率（背景 / 中景 / 前景）。
///
/// 全部元素用同一个亮度，画面就是**扁的** —— 这是"看起来平"的根因。
/// 用亮度 + 密度 + 速度三层区分，才有纵深。
///
/// | 层   | 亮度 | 密度 | 速度 | 例子           |
/// |------|------|------|------|----------------|
/// | 背景 | 0.3  | 稀   | 慢   | 星野、远处雨   |
/// | 中景 | 0.7  | 中   | 中   | 几何、狐狸     |
/// | 前景 | 1.0  | 密   | 快   | 粒子、UI 框架  |
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Depth {
    /// 背景：最暗、最慢、最稀
    Back,
    /// 中景：主体所在
    Mid,
    /// 前景：最亮、最快
    Front,
}

impl Depth {
    /// 该层的亮度倍率。
    pub fn dim(self) -> f32 {
        match self {
            Depth::Back => 0.32,
            Depth::Mid => 0.72,
            Depth::Front => 1.0,
        }
    }

    /// 该层的运动速度倍率。
    pub fn speed(self) -> f32 {
        match self {
            Depth::Back => 0.45,
            Depth::Mid => 1.0,
            Depth::Front => 1.45,
        }
    }

    /// 把颜色按层级压暗（只碰 RGB，不动覆盖率 —— 否则细线会消失）。
    pub fn shade(self, c: Color) -> Color {
        let (r, g, b) = col::rgb_of(c);
        let k = self.dim();
        Color::Rgb(
            (r as f32 * k) as u8,
            (g as f32 * k) as u8,
            (b as f32 * k) as u8,
        )
    }
}

/// 频谱柱的重力平滑（借鉴 cava 的 integral + gravity 双滤波器）。
///
/// 思路：
/// - **上升**：直接跟当前值，保证打击感不迟钝
/// - **下降**：参考过去一小段时间内的最大值，按指数衰减落下，避免抖动
///
/// 这里用 `ctx.prev` 作为"上一帧特征"，但**不做帧间累积** ——
/// 衰减量由时间常量决定，所以 `frame(t)` 仍是纯函数。
fn gravity_smoothed(ctx: &SceneCtx, idx: usize, _u: f32) -> f32 {
    let cur = ctx.feat.spectrum.get(idx).copied().unwrap_or(0.0);
    let prev = ctx.prev.spectrum.get(idx).copied().unwrap_or(0.0);
    let cur = cur.clamp(0.0, 1.0);
    let prev = prev.clamp(0.0, 1.0);

    // 帧间隔（秒）。导出与实时都走同一条路径。
    let dt = 1.0 / 60.0_f32;
    // 重力：越大落得越快。3.5 约等于 0.29 秒落到底。
    let gravity = 3.5;
    // 积分：让上升有一点粘性，避免单帧尖刺
    let integral = 0.35;

    if cur >= prev {
        // 上升：快速跟进 + 一点惯性
        prev + (cur - prev) * (1.0 - (-integral * 12.0 * dt).exp())
    } else {
        // 下降：从峰值按指数下落
        prev * (-gravity * dt).exp()
    }
}

/// 用当前帧特征近似过去某时刻的能量。
///
/// 没有历史缓冲，所以用「当前能量 + 正弦扰动」合成一条有起伏的曲线 ——
/// 它表达的是「能量在波动」这件事，而不是精确的历史值。
fn energy_at(ctx: &SceneCtx, tq: f64) -> f32 {
    let base = ctx.feat.energy_norm;
    let phase = (tq * 2.4 + ctx.frame as f64 * 0.01).sin() as f32;
    let phase2 = (tq * 5.7).cos() as f32;
    (base * (0.75 + phase * 0.18 + phase2 * 0.12)).clamp(0.0, 1.0)
}

/// 把狐狸限制在左面板里。
fn draw_fox_in(ctx: &SceneCtx, canvas: &mut CharCanvas, alpha: f32, pane: (f32, f32, f32, f32)) {
    let (px, py, pw, ph) = pane;
    let cx = px + pw * 0.5;
    let cy = py + ph * 0.5;
    // 半径按面板的**高度与宽度双重约束**取小值，
    // 再乘 0.92 留一点边距 —— 让她真正填满面板，
    // 而不是缩在中间一小团（那样导出后根本认不出是狐狸）。
    let r = (pw * 0.42).min(ph * 0.5) * 0.92;
    draw_fox_at(ctx, canvas, alpha, cx, cy, r);
}

/// 狐狸的在场强度 0~1。
///
/// 这条曲线是整支片子的情感线：
/// - 前奏从 0 长到 1（她「出现」）
/// - 主歌维持中等（她在旁边看着）
/// - 副歌压到很低（情绪本身是主角，她退成背景）
/// - 桥段回升（对话感最强的一段）
/// - 尾声从 1 衰减到 0（她消散）
fn fox_presence(ctx: &SceneCtx) -> f32 {
    use crate::timeline::scene::SceneKind as K;
    let base: f32 = match ctx.scene {
        K::Intro => {
            // 从 0 淡入，中段到达满值
            crate::timeline::beat::smoothstep(ctx.progress, 0.05, 0.55)
        }
        K::Spring => 0.55,
        K::Summer => 0.42,
        K::Bridge => 0.72,
        K::Chorus => 0.26,
        K::Autumn => 0.30,
        // 冬：她化作雪原上的一个剪影
        K::Winter => 0.34,
        // 昇華：绘卷展开，她隐入卷轴
        K::Ascension => 0.18,
        // 終章：记忆里的她
        K::Finale => 0.60,
        K::Outro => {
            // 从满值衰减到几乎消失
            (1.0 - crate::timeline::beat::smoothstep(ctx.progress, 0.25, 0.95)) * 0.85
        }
    };
    // 低频让她随音乐呼吸（但幅度小，避免闪烁）
    let breath = 1.0 + ctx.low() * 0.12;
    (base * breath).clamp(0.0, 1.0)
}

/// 把狐狸画到画布上。
///
/// `alpha` 是在场强度：它同时控制**亮度**和**细节量** ——
/// 低 alpha 时只画轮廓，高 alpha 时才画眼睛与尾巴。
/// 这样退到背景时是一团若有若无的影子，而不是生硬的缩放。
fn draw_fox_at(
    ctx: &SceneCtx,
    canvas: &mut CharCanvas,
    alpha: f32,
    cx: f32,
    cy: f32,
    r: f32,
) {
    use crate::render::fox::{self, FoxPose};

    let t = ctx.local_t as f32;

    // 她住在左面板里，所以尺度直接按面板半径给 ——
    // 面板已经把她的活动范围框死了，不需要再留额外余量。
    let recessed = 1.0 - alpha;
    let scale = r * (0.78 + alpha * 0.10);
    let pose = FoxPose {
        scale,
        ..FoxPose::from_music(t, ctx.feat.energy_norm, ctx.feat.beat, ctx.feat.vocal)
    };
    // 让中心稍微下移，头部落在画面中上部
    let oy = cy + scale * (0.28 + recessed * 0.22);
    let ox = cx;

    let [main, _alt, accent] = ctx.scene.palette();
    // 低 alpha 时颜色往背景压，形成「影子」的感觉 ——
    // 但**必须有亮度地板**：Braille 的点要有足够亮度才会被看见，
    // 之前压到 lerp(BLACK, main, 0.7) * 0.2 ≈ 纯黑，副歌段她就整段消失了。
    // 「淡」应该表达为「更低饱和 + 更细的线」，不是「更黑」。
    let base = col::lerp(col::BLACK, main, 0.55 + alpha * 0.35);
    let body_col = col::lerp(base, col::WHITE, alpha * 0.22);
    let (br, bg, bb) = col::rgb_of(body_col);
    let floor = 90.0;
    let body_col = if (br as f32 + bg as f32 + bb as f32) / 3.0 < floor {
        let (sr, sg, sb) = col::rgb_of(col::lerp(col::BLACK, main, 0.75));
        let k = alpha.clamp(0.25, 1.0);
        Color::Rgb(
            ((floor * (1.0 - k) + sr as f32 * k) as u8).max(70),
            ((floor * (1.0 - k) + sg as f32 * k) as u8).max(70),
            ((floor * (1.0 - k) + sb as f32 * k) as u8).max(70),
        )
    } else {
        body_col
    };
    // 淡出时线变细（而不是变黑），最细不低于 0.8 保证能点亮
    let line_w = (0.8 + alpha * 1.3).max(0.8);

    // ── 轮廓 ──
    let steps = ((scale * 6.0).clamp(180.0, 1200.0)) as usize;
    let outline = fox::outline(&pose, steps);
    let pts: Vec<(f32, f32)> = outline.iter().map(|(x, y)| (ox + x, oy - y)).collect();
    canvas.polyline(&pts, body_col, line_w, true);

    // ── 尾巴 ──
    // 只有她「在场」时才画尾巴，衰减时尾巴先消失
    if alpha > 0.3 {
        let spine = fox::tail_spine(&pose, 64);
        let tp: Vec<(f32, f32)> = spine.iter().map(|(x, y)| (ox + x, oy - y)).collect();
        let tail_col = col::lerp(body_col, accent, 0.35);
        canvas.polyline(&tp, tail_col, line_w * 0.8, false);
        // 蓬松感：沿中心线两侧各加一层细线
        if alpha > 0.6 {
            for off in [-1.0f32, 1.0] {
                let t2: Vec<(f32, f32)> = spine
                    .iter()
                    .enumerate()
                    .map(|(i, (x, y))| {
                        let u = i as f32 / spine.len().max(1) as f32;
                        // 中段最粗，末端收细
                        let w = (u * std::f32::consts::PI).sin() * 2.2 * off;
                        (ox + x * 1.0, oy - y + w)
                    })
                    .collect();
                canvas.polyline(
                    &t2,
                    col::lerp(tail_col, col::BLACK, 0.35),
                    line_w * 0.4,
                    false,
                );
            }
        }
    }

    // ── 眼睛 ──
    // 眼睛是她「有神」的关键，只在足够亮时才画
    if alpha > 0.45 {
        let (le, re) = fox::eye_centers(&pose);
        let openness = fox::eye_openness(&pose);
        let pr = fox::pupil_radius(&pose, ctx.feat.vocal);
        // alpha 越高眼睛越实
        let eye_a = ((alpha - 0.45) / 0.55).clamp(0.0, 1.0);
        for (ex, ey) in [le, re] {
            let px = ox + ex;
            let py = oy - ey;
            if openness > 0.15 {
                // 眼白：横向椭圆
                canvas.ellipse(
                    px,
                    py,
                    scale * 0.115,
                    scale * 0.062 * openness.max(0.2),
                    col::WHITE,
                    0.6 + eye_a * 0.4,
                );
                // 瞳孔
                canvas.disc(px, py, scale * pr * 2.4, accent, 0.75 + eye_a * 0.25);
            } else {
                // 闭眼：一条横线
                let w = scale * 0.1;
                canvas.line(px - w, py, px + w, py, body_col, 0.9);
            }
        }
    }

    // ── 鼻子 ──
    if alpha > 0.6 {
        let nose = fox::nose_points(&pose);
        let np: Vec<(f32, f32)> = nose.iter().map(|(x, y)| (ox + x, oy - y)).collect();
        canvas.polyline(&np, accent, 0.8, false);
    }
}

/// 背景星野：由时间驱动的确定性点阵，所有场景共用。
///
/// 用哈希而非随机数，保证 `frame(t)` 是纯函数 —— seek 到任意位置都能复现。
/// 亮度刻意压得比主体低，只负责填掉大片死黑。
fn draw_starfield(ctx: &SceneCtx, canvas: &mut CharCanvas) {
    // 背景层：最暗最慢
    let sw = canvas.sw as f32;
    let sh = canvas.sh as f32;
    if sw < 8.0 || sh < 8.0 {
        return;
    }
    // 密度随能量变化：安静段稀疏、高潮段铺满。
    // 星野的职责是当底噪纹理，恒定密度会让「能量高低」在画面上看不出来。
    let density = 110.0 - ctx.feat.energy_norm.clamp(0.0, 1.0) * 46.0;
    let count = ((sw * sh) / density).clamp(40.0, 640.0) as usize;
    let [main, alt, _] = ctx.scene.palette();
    let t = ctx.t as f32;
    // 音乐驱动整体亮度：安静段淡、高潮段亮，但下限永远高于点亮阈值
    // bands_norm 下标：0=sub 1=bass 2=lowmid 3=mid 4=highmid 5=high
    let gain = 0.62 + ctx.feat.energy_norm * 0.28 + ctx.feat.bands_norm[5] * 0.14;
    let depth = Depth::Back;
    for i in 0..count {
        // 整数哈希，不用 fract(sin(x)*k) —— 后者跨平台会发散，
        // 而且 a3f1a3f1 这类规律输入会折叠成同一个值。
        let (hx, hy) = hash2(i as u32, 0x5A17_2E9D);
        // 缓慢横向漂移 + 呼吸式明灭
        let drift = (t * 0.05 + i as f32 * 0.7).sin() * 3.5;
        let twinkle = (t * 1.6 + i as f32 * 1.3).sin().abs();
        let base = if i % 3 == 0 { main } else { alt };
        // 覆盖率必须 > 0.5 才会点亮，且颜色别压太狠（0.55 → 0.35）
        let w = (gain * (0.78 + twinkle * 0.22)).clamp(0.52, 1.0);
        // 背景层：压暗到 0.32，形成纵深最远的一层
        canvas.add(
            hx * sw + drift,
            hy * sh,
            depth.shade(col::lerp(base, col::BLACK, 0.35)),
            w,
        );
    }
}

/// 场景的后处理参数（在场景基调上叠加音乐与事件驱动）。
pub fn postfx_for(ctx: &SceneCtx, out: &mut PostFx) {
    let base = ctx.scene.default_postfx();
    let energy = ctx.feat.energy_norm;
    let beat = ctx.feat.beat;
    out.bloom = (base[0] + energy * 0.25 + ctx.heartbeat * 0.2).min(1.0);
    out.aberration = (base[1] + beat * 0.25 + ctx.glitch * 0.4).min(1.5);
    out.scanline = base[2];
    out.shake = (base[3] + beat * 0.25 + ctx.punch * 0.6).min(1.5);
    out.vignette = base[4];
    out.glitch = (base[5] + ctx.glitch * 1.2).min(1.5);
}

/// 残影强度与衰减。
///
/// 残影是叙事工具，不是装饰：
/// - **主歌**几乎不留影 —— 观众需要看清"正在发生什么"
/// - **副歌**留长影 —— 情绪爆发，画面要"满"、要糊、要有速度感
/// - **尾声**留中等影且衰减快 —— 像信号消散
///
/// 返回值 `(强度, 衰减)`。衰减越小拖尾越短。
pub fn trail_for(ctx: &SceneCtx) -> (f32, f32) {
    use crate::timeline::scene::SceneKind as K;
    let energy = ctx.feat.energy_norm;
    let (base, decay) = match ctx.scene {
        K::Intro => (0.30, 0.80),
        K::Spring => (0.30, 0.80),
        K::Summer => (0.48, 0.74),
        K::Bridge => (0.52, 0.76),
        K::Chorus => (0.68, 0.72),
        K::Autumn => (0.42, 0.76),
        // 冬：雪要看得清，短影
        K::Winter => (0.36, 0.80),
        // 昇華：绘卷流动，长影
        K::Ascension => (0.62, 0.70),
        // 終章：记忆淡出，中短影
        K::Finale => (0.44, 0.74),
        K::Outro => (0.42, 0.68),
    };
    // 巨响瞬间再多留一点，让爆发有"余温"
    let t = (base + energy * 0.12 + ctx.punch * 0.15).clamp(0.0, 0.85);
    (t, decay)
}

// ───────────────────────── 共用绘制工具 ─────────────────────────

/// 背景网格。
fn draw_grid(canvas: &mut CharCanvas, step: f32, color: Color, w: f32) {
    let sw = canvas.sw as f32;
    let sh = canvas.sh as f32;
    let mut x = 0.0f32;
    while x <= sw {
        canvas.line(x, 0.0, x, sh, color, w);
        x += step;
    }
    let mut y = 0.0f32;
    while y <= sh {
        canvas.line(0.0, y, sw, y, color, w);
        y += step;
    }
}

/// 径向频谱：半径随各频点能量起伏的闭合曲线。
fn draw_radial_spectrum(
    canvas: &mut CharCanvas,
    cx: f32,
    cy: f32,
    r0: f32,
    feat: &FrameFeatures,
    color: Color,
    w: f32,
    gain: f32,
    rot: f32,
) {
    let n = (r0 * 6.0).clamp(120.0, 900.0) as usize;
    let mut pts = Vec::with_capacity(n + 1);
    for i in 0..=n {
        let a = i as f32 / n as f32 * std::f32::consts::TAU + rot;
        // 用 0..1 的角位置在频谱上取样（低频在前）
        let u = i as f32 / n as f32;
        let s = feat.spec_at(u);
        let r = r0 * (1.0 + s * gain);
        pts.push((cx + r * a.cos(), cy + r * a.sin()));
    }
    canvas.polyline(&pts, color, w, false);
}

/// 横向波形：把 128 个频点铺成一条对称波形。
fn draw_wave(
    canvas: &mut CharCanvas,
    cy: f32,
    feat: &FrameFeatures,
    color: Color,
    w: f32,
    amp: f32,
) {
    let sw = canvas.sw as f32;
    // 采样点数随画布宽度走，否则宽画布上波形会断成虚线
    let n = (sw * 1.5).clamp(80.0, 900.0) as usize;
    let mut pts = Vec::with_capacity(n + 1);
    for i in 0..=n {
        let u = i as f32 / n as f32;
        let x = u * sw;
        let s = feat.spec_at(u);
        pts.push((x, cy - (s - 0.5) * amp));
    }
    canvas.polyline(&pts, color, w, false);
    // 镜像一条，形成对称
    let mirror: Vec<(f32, f32)> = pts.iter().map(|(x, y)| (*x, 2.0 * cy - *y)).collect();
    canvas.polyline(&mirror, color, w * 0.5, false);
}

/// 心形曲线点集。
///
/// 经典参数方程 `x = 16sin³t`、`y = 13cos t − 5cos2t − 2cos3t − cos4t`，
/// 除以 17 归一化到 ±1，再按 `scale` 放大。
/// 屏幕 y 轴向下，因此对 y 取负。
fn heart_points(n: usize, cx: f32, cy: f32, scale: f32, rot: f32) -> Vec<(f32, f32)> {
    let (sr, cr) = rot.sin_cos();
    let mut v = Vec::with_capacity(n + 1);
    for i in 0..=n {
        let t = i as f32 / n as f32 * std::f32::consts::TAU;
        let s = t.sin();
        let x = 16.0 * s * s * s;
        let y = 13.0 * t.cos() - 5.0 * (2.0 * t).cos() - 2.0 * (3.0 * t).cos() - (4.0 * t).cos();
        let (x, y) = (x / 17.0, -y / 17.0);
        let rx = x * cr - y * sr;
        let ry = x * sr + y * cr;
        v.push((cx + rx * scale, cy + ry * scale));
    }
    v
}

/// 频谱条：竖直柱状图。
fn draw_spectrum_bars(
    canvas: &mut CharCanvas,
    x0: f32,
    y_base: f32,
    width: f32,
    height: f32,
    feat: &FrameFeatures,
    color_lo: Color,
    color_hi: Color,
    bars: usize,
) {
    if bars == 0 {
        return;
    }
    let bw = width / bars as f32;
    for i in 0..bars {
        let u = (i as f32 + 0.5) / bars as f32;
        let s = feat.spec_at(u);
        let h = (s * height).max(0.5);
        let x = x0 + i as f32 * bw;
        let c = col::lerp(color_lo, color_hi, s);
        canvas.fill_rect(x, y_base - h, (bw - 1.0).max(0.5), h, c, 0.9);
    }
}

/// 代码雨：每列一条向下流动的字符轨迹。
///
/// 列速由低音驱动；有人声时整体减速（避免盖住歌词）。
fn draw_code_rain(
    canvas: &mut CharCanvas,
    t: f64,
    low: f32,
    vocal: f32,
    charset: &[char],
    head: Color,
    tail: Color,
    columns: usize,
    rng: &mut SmallRng,
) {
    if charset.is_empty() || columns == 0 {
        return;
    }
    let t = t as f32;
    let sw = canvas.sw as f32;
    let sh = canvas.sh as f32;
    let speed = (1.0 + low * 3.0) * (1.0 - vocal * 0.5);
    let _ = rng; // 不再消耗随机数：用列索引哈希，保证 frame(t) 是纯函数
    for i in 0..columns {
        // 确定性哈希而不是每帧随机 —— 早先用 rng 的话，
        // 雨滴位置每帧都在乱跳，既形不成流动的雨，也破坏了 seek 可复现性。
        let h = (i as u32).wrapping_mul(2654435761);
        let seed_phase = ((h >> 8) & 0xFFFF) as f32 / 65535.0;
        let h2 = (i as u32).wrapping_mul(40503).wrapping_add(12345);
        let x = (i as f32 + 0.5) / columns as f32 * sw;
        let len = 8.0 + ((h2 >> 8) & 0xFF) as f32 / 255.0 * 14.0;
        let y_head = ((t * speed * 12.0 + seed_phase * sh * 2.0) % (sh + len * 4.0)) - len * 2.0;
        for k in 0..len as usize {
            let y = y_head - k as f32;
            if y < 0.0 || y >= sh {
                continue;
            }
            let f = 1.0 - k as f32 / len;
            // 背景层：压暗，形成最远的一层
            let c = Depth::Back.shade(col::lerp(tail, head, f.powf(0.5)));
            canvas.set(x, y, c, (f * 0.9).max(0.05));
        }
        // 头部亮点
        if y_head >= 0.0 && y_head < sh {
            canvas.add(x, y_head, Depth::Back.shade(col::WHITE), 0.9);
            canvas.set(x + 1.0, y_head, Depth::Back.shade(col::WHITE), 0.5);
        }
    }
}

/// 李萨如曲线（两个正交正弦的合成）。
fn draw_lissajous(
    canvas: &mut CharCanvas,
    cx: f32,
    cy: f32,
    r: f32,
    a: f32,
    b: f32,
    delta: f32,
    color: Color,
    w: f32,
) {
    let n = 400usize;
    let mut pts = Vec::with_capacity(n + 1);
    for i in 0..=n {
        let t = i as f32 / n as f32 * std::f32::consts::TAU;
        pts.push((cx + r * (a * t + delta).sin(), cy + r * (b * t).sin()));
    }
    canvas.polyline(&pts, color, w, false);
}

/// 眼睛：外轮廓 + 瞳孔 + 眼睑开合。
fn draw_eye(
    canvas: &mut CharCanvas,
    cx: f32,
    cy: f32,
    rx: f32,
    ry: f32,
    openness: f32,
    color: Color,
    pupil: Color,
    w: f32,
) {
    let open = openness.clamp(0.05, 1.0);
    canvas.ellipse(cx, cy, rx, ry * open, color, w);
    canvas.ellipse(cx, cy, rx * 0.85, ry * open * 0.85, color, w * 0.6);
    // 瞳孔随开合放大
    let pr = (rx * 0.22 * (1.4 - open)).max(1.0);
    canvas.disc(cx, cy, pr, pupil, 1.0);
    // 高光
    canvas.add(cx - pr * 0.35, cy - pr * 0.35, col::WHITE, 0.9);
}

/// 镜像轴：左右对称的装饰线。
fn draw_mirror_axis(canvas: &mut CharCanvas, x: f32, sh: f32, color: Color) {
    let mut y = 0.0f32;
    while y < sh {
        canvas.set(x, y, color, 0.7);
        canvas.set(x - 1.0, y + 1.0, color, 0.35);
        y += 3.0;
    }
}

/// 由时间与能量驱动的「脉冲环」。
fn draw_pulse_rings(
    canvas: &mut CharCanvas,
    cx: f32,
    cy: f32,
    t: f64,
    energy: f32,
    color: Color,
    rings: usize,
) {
    for i in 0..rings {
        let phase = (t * 0.6 + i as f64 * 0.25).fract() as f32;
        let r = phase * (canvas.sw.min(canvas.sh) as f32 * 0.45);
        let a = (1.0 - phase) * (0.35 + energy * 0.65);
        if a <= 0.02 {
            continue;
        }
        canvas.circle(cx, cy, r, color, a);
    }

}


/// 当前场景的卡片绘制入口（接线点：由 scenes_*.rs 提供）。
///
/// 顺序与 [`SceneKind::ALL`] 一致；缺省占位画「底色 + 季节大字」。
pub fn render(
    ctx: &SceneCtx,
    canvas: &mut CharCanvas,
    particles: &mut ParticleSystem,
    rng: &mut SmallRng,
) {
    let [main, _dim, accent] = ctx.scene.palette();

    // ── 1. 全屏流动大气场 ────────────────────────────────────────
    // 由 `Engine::render_frame` 画在 LAYER_BG 上（背景层，先于主体合成）。
    // 放在这一层而不是主体层，是因为主体层带 alpha，底纹会被一起压暗。
    // 没有这一层的话画面 92% 的字符格是纯黑的 —— 实测成片平均亮度只有 1.86/255。

    // ── 2. 季节汉字水印 ──────────────────────────────────────────
    // 一眼就知道现在播到哪一季，不用靠猜配色。
    draw_season_watermark(ctx, canvas);

    // ── 3. 星野（灰白点阵） ──────────────────────────────────────
    draw_starfield(ctx, canvas);

    // ── 4. 场景主体 ──────────────────────────────────────────────
    match ctx.scene {
        SceneKind::Intro => {
            crate::render::scenes_intro::render_card(ctx, canvas, 1.0);
        }
        SceneKind::Spring => {
            crate::render::scenes_spring::render_card(ctx, canvas, 1.0);
            season_frame_ui(ctx, canvas);
        }
        SceneKind::Summer => {
            crate::render::scenes_summer::render_card(ctx, canvas, 1.0);
            season_frame_ui(ctx, canvas);
        }
        // 橋段：同一形体连续变形（瞬き一ツの間に、形が変エ行ク）
        SceneKind::Bridge => {
            crate::render::scenes_bridge::render_card(ctx, canvas, 1.0);
            season_frame_ui(ctx, canvas);
        }
        // 副歌：四季色环 + 川流 + 昨日咲キ今日散ル花 + 心の綾
        SceneKind::Chorus => {
            crate::render::scenes_chorus::render_card(ctx, canvas, 1.0);
            season_frame_ui(ctx, canvas);
        }
        SceneKind::Autumn => {
            let dur = 39.3f32;
            crate::render::scenes_autumn::render_autumn(ctx, canvas, ctx.local_t as f32, dur);
        }
        SceneKind::Winter => {
            let dur = 17.65f32;
            crate::render::scenes_winter::render_winter(ctx, canvas, ctx.local_t as f32, dur);
        }
        // 昇華与終章的真身在 scenes_ascension：绘卷展开、「刹那」逐笔、金脉冲最高点。
        SceneKind::Ascension => {
            let dur = 49.35f32;
            crate::render::scenes_ascension::render_ascension(ctx, canvas, ctx.local_t as f32, dur);
            season_frame_ui(ctx, canvas);
        }
        SceneKind::Finale => {
            let dur = 43.8f32;
            crate::render::scenes_ascension::render_finale(ctx, canvas, ctx.local_t as f32, dur);
            season_frame_ui(ctx, canvas);
        }
        SceneKind::Outro => {
            crate::render::scenes_outro::render_card(ctx, canvas, 1.0);
        }
    }

    // ── 5. 不再做全屏光晕 ────────────────────────────────────────
    // 上一版在这里对整幅画面套了半径 7 点（= 1760 宽画面上 28px）的同色辉光，
    // 结果是「太模糊」。参考实现（`tuikit.py` 的 `post()`）在 1280 宽的画面上
    // 只做 blur 4px，而且那是**整幅图的一次后期**，不是逐元素大半径扩散。
    // 现在辉光由导出外壳在像素层按画面宽度等比换算后统一施加。

    // ── 6. 真字关键词 ────────────────────────────────────────────
    // 把当前唱句里最具体的那个词放大打在画面上。这是「看得懂」的最后一道保险：
    // 观众就算看不懂抽象图形，也一定能读出这两个字。
    draw_keyword(ctx, canvas, accent, main);
}

/// 季节汉字水印：画在画面右侧，用暗色但**满覆盖度**。
///
/// 注意：想让文字变淡只能压颜色，**不能压 alpha** ——
/// Braille 只在覆盖度 `w > 0.5` 时点亮，而文字覆盖度是 `(cov*1.7).min(1) * alpha`，
/// alpha 低于 0.55 的笔画会整段消失，画面上什么都不剩。
fn draw_season_watermark(ctx: &SceneCtx, canvas: &mut CharCanvas) {
    let kanji = ctx.scene.kanji();
    if kanji.is_empty() || canvas.sw < 40 || canvas.sh < 20 {
        return;
    }
    let px = canvas.sh as f32 * 0.40;
    let w = crate::render::text::measure(kanji, px);
    if w <= 0.0 || w > canvas.sw as f32 {
        return;
    }
    let x = canvas.sw as f32 - w - canvas.sw as f32 * 0.05;
    // 基线落在画面 55% 高度处
    let y = canvas.sh as f32 * 0.55;
    let c = col::lerp(col::BLACK, ctx.scene.palette()[1], 0.32);
    crate::render::text::draw(canvas, x, y, kanji, px, c, 1.0);
}

/// 当前唱句里「最具体的词」，放大打在画面上部。
fn draw_keyword(ctx: &SceneCtx, canvas: &mut CharCanvas, accent: Color, main: Color) {
    let Some(line) = ctx.lyric() else {
        return;
    };
    let Some(kw) = keyword_of(line) else {
        return;
    };
    if canvas.sw < 60 || canvas.sh < 30 {
        return;
    }
    let px = canvas.sh as f32 * 0.26;
    let w = crate::render::text::measure(&kw, px);
    if w <= 0.0 || w > canvas.sw as f32 * 0.9 {
        return;
    }
    let x = (canvas.sw as f32 - w) * 0.5;
    let y = canvas.sh as f32 * 0.34;

    // 描边：先在四周各偏 1 点打一圈暗色，让字从任何背景上都读得出来。
    let halo = col::lerp(col::BLACK, main, 0.42);
    for (dx, dy) in [(-1.0f32, 0.0f32), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] {
        crate::render::text::draw(canvas, x + dx, y + dy, &kw, px, halo, 1.0);
    }
    crate::render::text::draw(canvas, x, y, &kw, px, accent, 1.0);
}

/// 从一句歌词里挑一个「最具体」的词做画面大字。
///
/// 取最长的连续汉字串（上限 2 字）；整句没有汉字（全是假名）时退回最长的片假名串。
/// 例：「東風吹けば雲は解ケテ」→「東風」、「一羽の蝶」→「一羽」。
fn keyword_of(line: &str) -> Option<String> {
    let is_kanji = |c: char| matches!(c as u32, 0x4E00..=0x9FFF | 0x3400..=0x4DBF);
    let is_kata = |c: char| matches!(c as u32, 0x30A1..=0x30FA | 0x30FD..=0x30FF);

    let longest = |pred: &dyn Fn(char) -> bool| -> String {
        let mut best = String::new();
        let mut cur = String::new();
        for ch in line.chars() {
            if pred(ch) {
                cur.push(ch);
            } else {
                if cur.chars().count() > best.chars().count() {
                    best = std::mem::take(&mut cur);
                }
                cur.clear();
            }
        }
        if cur.chars().count() > best.chars().count() {
            best = cur;
        }
        best
    };

    let k = longest(&is_kanji);
    let pick = if k.is_empty() { longest(&is_kata) } else { k };
    if pick.is_empty() {
        return None;
    }
    Some(pick.chars().take(2).collect())
}

/// 各季共用框架：进度线（接在卡片绘制之后）。
///
/// 唱句不在这里画了：底部歌词条由导出外壳统一绘制（真字形 + 逐词高亮），
/// 画面内的歌词改用 `draw_keyword` 的大字关键词。原来这里打的是「假字」，
/// 既读不出来又和歌词条重复。
fn season_frame_ui(ctx: &SceneCtx, canvas: &mut CharCanvas) {
    let [_main, _dim, accent] = ctx.scene.palette();
    let w = canvas.sw as f32;
    canvas.line(
        0.0,
        canvas.sh as f32 - 4.0,
        w * ctx.progress as f32,
        canvas.sh as f32 - 4.0,
        accent,
        0.5,
    );
}

/// 占位场景通用体：底色呼吸 + 季节大字 + 当前唱句。
/// 等各季卡片接入后被真身替换；Intro/Outro 的真身就是它的变体。
fn placeholder_scene(ctx: &SceneCtx, canvas: &mut CharCanvas, label_kanji: bool) {
    let (cx, cy) = ctx.center(canvas);
    let [main, dim, accent] = ctx.scene.palette();
    let t = ctx.t as f32;

    // 底色呼吸：低音驱动整屏明暗
    canvas.tint(main, 0.10 + ctx.low() * 0.10);

    // 中央季节大字（点阵描边，随呼吸缩放）
    if label_kanji {
        let k = ctx.scene.kanji();
        let r = ctx.radius(canvas) * (0.30 + 0.015 * (t * 0.9).sin());
        // 用圆环+十字勾勒字的骨架感（Braille 下比真字形更干净）
        canvas.circle(cx, cy, r, main, 0.55);
        canvas.circle(cx, cy, r * 0.94, dim, 0.25);
        canvas.line(cx - r * 0.5, cy, cx + r * 0.5, cy, accent, 0.4);
        canvas.line(cx, cy - r * 0.5, cx, cy + r * 0.5, accent, 0.4);
    }

    // 唱句不在这里画：底部歌词条由导出外壳统一绘制（真字形 + 逐词高亮），
    // 画面内改用 `draw_keyword` 的大字关键词。

    // 进度细线
    let w = canvas.sw as f32;
    canvas.line(0.0, canvas.sh as f32 - 4.0, w * ctx.progress as f32, canvas.sh as f32 - 4.0, accent, 0.5);
}

fn intro_placeholder(ctx: &SceneCtx, canvas: &mut CharCanvas) {
    placeholder_scene(ctx, canvas, false);
    let [main, dim, _accent] = ctx.scene.palette();
    // 晨雾：几条横向流动的雾带
    let sh = canvas.sh as f32;
    let sw = canvas.sw as f32;
    for i in 0..5 {
        let y = sh * (0.2 + 0.15 * i as f32) + 6.0 * ((ctx.t as f32) * 0.3 + i as f32).sin();
        canvas.line(0.0, y, sw, y + 4.0 * ((ctx.t as f32) * 0.5 + i as f32 * 1.7).sin(), dim, 0.15);
    }
    // 中央呼吸圆：万物未醒的一口气
    let (cx, cy) = ctx.center(canvas);
    let r = ctx.radius(canvas) * (0.18 + 0.05 * ((ctx.t as f32) * 0.8).sin() + ctx.low() * 0.04);
    canvas.circle(cx, cy, r, main, 0.35 + ctx.low() * 0.2);
}

fn bridge_placeholder(ctx: &SceneCtx, canvas: &mut CharCanvas) {
    placeholder_scene(ctx, canvas, true);
    let [main, _dim, accent] = ctx.scene.palette();
    // 瞬き：眨眼的两条弧，开合由节拍驱动
    let (cx, cy) = ctx.center(canvas);
    let open = (ctx.feat.beat * 0.5 + 0.5) * 0.5 + 0.1;
    let r = ctx.radius(canvas) * 0.35;
    for s in [-1.0f32, 1.0] {
        for i in 0..12 {
            let u = i as f32 / 11.0;
            let x = cx + s * r * u;
            let y = cy + s * r * (1.0 - u * u) * open * 0.4;
            canvas.set(x, y, main, 0.8);
            canvas.set(x, y + s * 2.0 * open, accent, 0.5);
        }
    }
}

fn chorus_placeholder(ctx: &SceneCtx, canvas: &mut CharCanvas) {
    placeholder_scene(ctx, canvas, true);
    let [main, dim, accent] = ctx.scene.palette();
    // 四季色环：四段圆弧绕心旋转，色相随时间推
    let (cx, cy) = ctx.center(canvas);
    let r = ctx.radius(canvas) * (0.42 + 0.03 * ((ctx.t as f32) * 1.2).sin());
    let cols = [main, accent, dim, main];
    for i in 0..4 {
        let a0 = (ctx.t as f32) * 0.5 + i as f32 * std::f32::consts::FRAC_PI_2;
        let a1 = a0 + std::f32::consts::FRAC_PI_2 * 0.8;
        let steps = 24;
        for s in 0..=steps {
            let a = a0 + (a1 - a0) * s as f32 / steps as f32;
            canvas.set(cx + a.cos() * r, cy + a.sin() * r, cols[i], 0.85);
        }
    }
    // 川の流れ：横向流水线
    let sh = canvas.sh as f32;
    for i in 0..3 {
        let y = sh * (0.72 + 0.06 * i as f32);
        for x in (0..(canvas.sw as i32)).step_by(6) {
            let x = x as f32;
            let yy = y + 3.0 * ((x * 0.05 + (ctx.t as f32) * 2.0 + i as f32).sin());
            canvas.set(x, yy, accent, 0.4);
        }
    }
}

/// 昇華占位：金白底 + 四季色带向上流动（真身等 scenes_ascension 文件）。
fn ascension_placeholder(ctx: &SceneCtx, canvas: &mut CharCanvas) {
    placeholder_scene(ctx, canvas, true);
    let [main, dim, accent] = ctx.scene.palette();
    let (cx, cy) = ctx.center(canvas);
    // 四条季节色带向上升腾
    let cols = [
        Color::Rgb(140, 214, 120), // 春
        Color::Rgb(70, 196, 210),  // 夏
        Color::Rgb(240, 180, 88),  // 秋
        Color::Rgb(196, 218, 240), // 冬
    ];
    let sw = canvas.sw as f32;
    let sh = canvas.sh as f32;
    for (i, c) in cols.iter().enumerate() {
        let x0 = sw * (0.2 + 0.2 * i as f32);
        let speed = 18.0 + 4.0 * i as f32;
        let off = ((ctx.t as f32) * speed) % (sh + 40.0);
        let y = sh - off;
        for s in 0..14 {
            let yy = y + s as f32 * 3.0;
            if yy < 0.0 || yy > sh {
                continue;
            }
            let a = 0.5 * (1.0 - s as f32 / 14.0);
            canvas.set(x0 + 4.0 * (yy * 0.1 + ctx.t as f32).sin(), yy, *c, a);
        }
    }
    // 中央金环：全曲最高点的光
    let r = ctx.radius(canvas) * (0.3 + 0.04 * ((ctx.t as f32) * 1.5).sin() + ctx.feat.energy_norm * 0.06);
    canvas.circle(cx, cy, r, main, 0.6);
    canvas.circle(cx, cy, r * 0.86, accent, 0.35);
    let _ = dim;
}

fn finale_placeholder(ctx: &SceneCtx, canvas: &mut CharCanvas) {
    placeholder_scene(ctx, canvas, true);
    let [_main, dim, accent] = ctx.scene.palette();
    // 月欠：满月逐渐被咬掉一口
    let (cx, cy) = ctx.center(canvas);
    let r = ctx.radius(canvas) * 0.28;
    let bite = ctx.progress * r * 1.6;
    canvas.disc(cx, cy, r, dim, 0.9);
    canvas.disc(cx + bite, cy - r * 0.35, r * 0.92, Color::Rgb(8, 10, 14), 1.0);
    // 不锈的金线：一圈永不熄灭
    canvas.circle(cx, cy, r * 1.15, accent, 0.5 + ctx.feat.beat * 0.3);
}

fn outro_placeholder(ctx: &SceneCtx, canvas: &mut CharCanvas) {
    placeholder_scene(ctx, canvas, false);
    let [main, dim, _accent] = ctx.scene.palette();
    // 收卷：横线逐条淡出
    let sh = canvas.sh as f32;
    let sw = canvas.sw as f32;
    let k = ctx.progress;
    let lines = 8;
    for i in 0..lines {
        let y = sh * (0.25 + 0.5 * i as f32 / lines as f32);
        let half = sw * 0.5 * (1.0 - k * (i as f32 / lines as f32));
        canvas.line(sw * 0.5 - half, y, sw * 0.5 + half, y, main, 0.4 * (1.0 - k));
    }
    let _ = dim;
}

#[cfg(test)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::features::{BAND_COUNT, SPECTRUM_BINS};
    use crate::config::RenderMode;
    use crate::timeline::scene::SceneKind;
    use rand::SeedableRng;

    fn feat(energy: f32) -> FrameFeatures {
        let mut f = FrameFeatures {
            time: 0.0,
            energy,
            energy_norm: energy,
            ..Default::default()
        };
        f.bands_norm = [energy; BAND_COUNT];
        f.spectrum = (0..SPECTRUM_BINS)
            .map(|i| {
                (i as f32 / SPECTRUM_BINS as f32 * std::f32::consts::TAU)
                    .sin()
                    .abs()
                    * energy
            })
            .collect();
        f
    }

    fn ctx<'a>(scene: SceneKind, lyrics: &'a Lyrics, progress: f32) -> SceneCtx<'a> {
        let f = feat(0.6);
        SceneCtx {
            t: 30.0,
            local_t: 10.0,
            progress,
            feat: f.clone(),
            prev: f,
            frame: 100,
            cols: 80,
            rows: 30,
            scene,
            lyrics,
            flash: 0.0,
            punch: 0.0,
            heartbeat: 0.0,
            glitch: 0.0,
            burst: 0.0,
            invert: 0.0,
        }
    }

    fn run(scene: SceneKind, progress: f32) -> usize {
        let lyrics = Lyrics::default();
        let c = ctx(scene, &lyrics, progress);
        let mut canvas = CharCanvas::new(80, 30, RenderMode::Braille);
        let mut ps = ParticleSystem::new(1, 512);
        let mut rng = SmallRng::seed_from_u64(1);
        render(&c, &mut canvas, &mut ps, &mut rng);
        canvas.pixels().iter().filter(|p| !p.is_empty()).count()
    }

    /// 与 [`run`] 同款，但返回整幅画面的指纹。
    ///
    /// 关键在于**夹具必须让 local_t 随进度推进**：四季/桥段/副歌的卡系统读的是
    /// `ctx.local_t`（段内时钟），只改 `progress` 的话它们当然纹丝不动 ——
    /// 那测的是夹具没接上时钟，不是画面不动。
    fn run_sig(scene: SceneKind, progress: f32) -> Vec<u32> {
        let lyrics = Lyrics::default();
        let mut c = ctx(scene, &lyrics, progress);
        c.t = 30.0 + progress as f64 * 40.0;
        c.local_t = 10.0 + progress as f64 * 20.0;
        let mut canvas = CharCanvas::new(80, 30, RenderMode::Braille);
        let mut ps = ParticleSystem::new(1, 512);
        let mut rng = SmallRng::seed_from_u64(1);
        render(&c, &mut canvas, &mut ps, &mut rng);
        canvas
            .pixels()
            .iter()
            .flat_map(|p| [p.w.to_bits(), p.r.to_bits(), p.g.to_bits(), p.b.to_bits()])
            .collect()
    }

    #[test]
    fn every_scene_draws_something() {
        for scene in SceneKind::ALL {
            for p in [0.0f32, 0.25, 0.5, 0.75, 1.0] {
                let n = run(scene, p);
                assert!(n > 20, "{scene:?} 在进度 {p} 时只画了 {n} 个像素");
            }
        }
    }

    #[test]
    fn scenes_do_not_panic_on_tiny_canvas() {
        let lyrics = Lyrics::default();
        for scene in SceneKind::ALL {
            let mut c = ctx(scene, &lyrics, 0.5);
            c.cols = 4;
            c.rows = 3;
            let mut canvas = CharCanvas::new(4, 3, RenderMode::Braille);
            let mut ps = ParticleSystem::new(1, 64);
            let mut rng = SmallRng::seed_from_u64(1);
            render(&c, &mut canvas, &mut ps, &mut rng);
        }
    }

    #[test]
    fn scenes_are_deterministic_for_same_input() {
        let lyrics = Lyrics::default();
        for scene in SceneKind::ALL {
            let mut a = CharCanvas::new(40, 20, RenderMode::Braille);
            let mut b = CharCanvas::new(40, 20, RenderMode::Braille);
            let mut pa = ParticleSystem::new(7, 256);
            let mut pb = ParticleSystem::new(7, 256);
            let c = ctx(scene, &lyrics, 0.4);
            let mut r1 = SmallRng::seed_from_u64(9);
            let mut r2 = SmallRng::seed_from_u64(9);
            render(&c, &mut a, &mut pa, &mut r1);
            render(&c, &mut b, &mut pb, &mut r2);
            let sa: Vec<f32> = a.pixels().iter().map(|p| p.w).collect();
            let sb: Vec<f32> = b.pixels().iter().map(|p| p.w).collect();
            assert_eq!(sa, sb, "{scene:?} 不确定");
        }
    }

    #[test]
    fn postfx_for_all_scenes_is_in_range() {
        let lyrics = Lyrics::default();
        for scene in SceneKind::ALL {
            let mut c = ctx(scene, &lyrics, 0.5);
            // 最坏情况：心跳 + 故障 + 冲击全部拉满
            c.heartbeat = 1.0;
            c.glitch = 1.0;
            c.punch = 1.0;
            let mut fx = PostFx::off();
            postfx_for(&c, &mut fx);
            // 逐字段按 postfx_for 自己的钳位断言 —— 各字段上限不同，
            // 用一把尺子量会把合法的强故障（glitch 上限 1.5）误判成越界。
            let checks: [(&str, f32, f32); 6] = [
                ("bloom", fx.bloom, 1.0),
                ("aberration", fx.aberration, 1.5),
                ("scanline", fx.scanline, 1.0),
                ("shake", fx.shake, 1.5),
                ("vignette", fx.vignette, 1.0),
                ("glitch", fx.glitch, 1.5),
            ];
            for (name, v, max) in checks {
                assert!(v.is_finite(), "{scene:?} 后处理 {name} 不是有限数: {v}");
                assert!((0.0..=max).contains(&v), "{scene:?} 后处理 {name}={v} 越界（上限 {max}）");
            }
        }
    }

    #[test]
    fn scenes_animate_over_progress() {
        // 同一场景在不同进度上必须产出不同画面（「画面在动」的最低要求）。
        // 用整幅指纹比较，而不是像素计数 —— 计数相同不代表画面相同。
        let lyrics = Lyrics::default();
        for scene in SceneKind::ALL {
            let a = run_sig(scene, 0.2);
            let b = run_sig(scene, 0.5);
            let c = run_sig(scene, 0.8);
            assert_ne!(a, b, "{scene:?} 画面在 0.2→0.5 之间纹丝不动");
            assert_ne!(b, c, "{scene:?} 画面在 0.5→0.8 之间纹丝不动");
            assert_ne!(a, c, "{scene:?} 画面在 0.2→0.8 之间纹丝不动");
        }
        drop(lyrics);
    }
}

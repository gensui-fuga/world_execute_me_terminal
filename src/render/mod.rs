//! 渲染层。
//!
//! 核心是 [`CharCanvas`]：一块**子像素**画布，按模式聚合成终端字符单元。
//! 所有场景只往画布上画几何图形，不关心最终是 Braille、半块还是 ASCII ——
//! 切换模式不需要改一行场景代码。
//!
//! ```text
//!  Scene::render(&mut CharCanvas)
//!        ↓
//!  layers 合成（背景 / 主体 / 粒子 / 歌词 / UI）
//!        ↓
//!  postfx 后处理（bloom / 色差 / 扫描线 / 抖动 / 故障）
//!        ↓
//!  CharCanvas::to_buffer(&mut ratatui::Buffer)
//! ```

pub mod ascii;
pub mod braille;
pub mod color;
pub mod effects;
pub mod fox;
pub mod halfblock;
pub mod layers;
pub mod particles;
pub mod postfx;
pub mod scenes;
pub mod motifs;
pub mod scenes_intro;
pub mod scenes_spring;
pub mod scenes_summer;
pub mod scenes_bridge;
pub mod scenes_chorus;
pub mod scenes_autumn;
pub mod scenes_winter;
pub mod scenes_ascension;
pub mod scenes_outro;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};

use crate::config::RenderMode;

/// 一个子像素：线性 RGB + 覆盖度。
///
/// 颜色用 `f32` 而不是 `u8`，因为叠加/发光要反复混合，整数量化会累积误差。
#[derive(Clone, Copy, Debug)]
pub struct Pixel {
    /// 红 0..1
    pub r: f32,
    /// 绿 0..1
    pub g: f32,
    /// 蓝 0..1
    pub b: f32,
    /// 覆盖度 0..1（0 表示完全透明）
    pub w: f32,
}

impl Default for Pixel {
    fn default() -> Self {
        Self {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            w: 0.0,
        }
    }
}

impl Pixel {
    /// 是否为空（完全透明）。
    pub fn is_empty(&self) -> bool {
        self.w <= 0.0
    }

    /// 转成 ratatui 颜色。
    pub fn to_color(self) -> Color {
        Color::Rgb(
            (self.r.clamp(0.0, 1.0) * 255.0).round() as u8,
            (self.g.clamp(0.0, 1.0) * 255.0).round() as u8,
            (self.b.clamp(0.0, 1.0) * 255.0).round() as u8,
        )
    }
}

/// 聚合后的一个字符单元。
#[derive(Clone, Copy, Debug)]
pub struct CellOut {
    /// 字符
    pub ch: char,
    /// 前景色
    pub fg: Color,
    /// 背景色（半块模式会用）
    pub bg: Color,
}

/// 把 `RenderMode::Auto` 解析成具体模式。
///
/// 默认 Braille；`TERM=dumb` 或空终端类型时退化到 ASCII（最保险）。
pub fn resolve_mode(mode: RenderMode) -> RenderMode {
    match mode {
        RenderMode::Auto => match std::env::var("TERM") {
            Ok(t) if t == "dumb" || t.is_empty() => RenderMode::Ascii,
            _ => RenderMode::Braille,
        },
        m => m,
    }
}

/// 子像素画布。
pub struct CharCanvas {
    /// 字符列数
    pub cols: u16,
    /// 字符行数
    pub rows: u16,
    /// 实际渲染模式（已解析 Auto）
    pub mode: RenderMode,
    /// 每字符列的子像素数
    pub sub_x: u16,
    /// 每字符行的子像素数
    pub sub_y: u16,
    /// 子像素总宽
    pub sw: u16,
    /// 子像素总高
    pub sh: u16,
    pix: Vec<Pixel>,
    /// 位移/模糊用的交换缓冲（避免每帧堆分配）
    swap: Vec<Pixel>,
    /// 整块画布的背景色。默认 `Reset`（用终端自己的底色）。
    ///
    /// 有些场景需要一整块实心底（蓝屏段最典型）——
    /// 那种情况下**不能**用 `fill_rect` 去点亮每个子像素：
    /// Braille 转码后那是一堵实心墙，看起来像满屏噪声。
    /// 正确做法是把底色交给这个字段，字符只负责画字。
    pub bg: Color,
}

impl CharCanvas {
    /// 新建画布。
    pub fn new(cols: u16, rows: u16, mode: RenderMode) -> Self {
        let mode = resolve_mode(mode);
        let (sub_x, sub_y) = match mode {
            RenderMode::Braille => (braille::SUB_X, braille::SUB_Y),
            RenderMode::Half => (halfblock::SUB_X, halfblock::SUB_Y),
            RenderMode::Quadrant => (2, 2),
            _ => (ascii::SUB_X, ascii::SUB_Y),
        };
        let cols = cols.max(1);
        let rows = rows.max(1);
        let sw = cols * sub_x;
        let sh = rows * sub_y;
        Self {
            cols,
            rows,
            mode,
            sub_x,
            sub_y,
            sw,
            sh,
            pix: vec![Pixel::default(); sw as usize * sh as usize],
            swap: Vec::new(),
            bg: Color::Reset,
        }
    }

    /// 只读访问全部子像素（行优先，长度 `sw * sh`）。
    pub fn pixels(&self) -> &[Pixel] {
        &self.pix
    }

    /// 设置整块画布的背景色。传 `Color::Reset` 恢复终端默认底色。
    pub fn paint_bg(&mut self, c: Color) {
        self.bg = c;
    }

    /// 可变访问全部子像素。
    pub fn pixels_mut(&mut self) -> &mut [Pixel] {
        &mut self.pix
    }

    /// 把已有内容复制进交换缓冲，返回其切片。
    ///
    /// 注意：该缓冲与 [`CharCanvas::shift_rows`] / [`CharCanvas::translate`]
    /// 共用，调用位移操作后内容即失效 —— 需要留存请自行 `to_vec()`。
    pub fn snapshot(&mut self) -> &[Pixel] {
        self.swap.clear();
        self.swap.extend_from_slice(&self.pix);
        &self.swap
    }

    /// 清空为全透明。
    pub fn clear(&mut self) {
        for p in self.pix.iter_mut() {
            *p = Pixel::default();
        }
    }

    /// 用指定颜色填满整块画布。
    pub fn fill(&mut self, color: Color, w: f32) {
        let (r, g, b) = color::rgb_of(color);
        let (r, g, b) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
        let w = w.clamp(0.0, 1.0);
        for p in self.pix.iter_mut() {
            *p = Pixel { r, g, b, w };
        }
    }

    /// 子像素坐标 → 线性下标。越界返回 `None`。
    #[inline]
    fn index(&self, x: i32, y: i32) -> Option<usize> {
        if x < 0 || y < 0 || x >= self.sw as i32 || y >= self.sh as i32 {
            return None;
        }
        Some(y as usize * self.sw as usize + x as usize)
    }

    /// 读取子像素（越界返回空像素）。
    #[inline]
    pub fn pixel(&self, x: i32, y: i32) -> Pixel {
        self.index(x, y).map(|i| self.pix[i]).unwrap_or_default()
    }

    /// 写入子像素（覆盖式：更亮者胜，弱者在强像素上做半透明混合）。
    pub fn set(&mut self, x: f32, y: f32, color: Color, w: f32) {
        let w = w.clamp(0.0, 1.0);
        if w <= 0.0 {
            return;
        }
        let xi = x.round() as i32;
        let yi = y.round() as i32;
        let Some(i) = self.index(xi, yi) else {
            return;
        };
        let (r, g, b) = color::rgb_of(color);
        let (nr, ng, nb) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
        let p = &mut self.pix[i];
        if w >= p.w {
            p.r = nr;
            p.g = ng;
            p.b = nb;
            p.w = w;
        } else {
            // 弱像素叠加：按权重轻微混色，保留层次但不盖住主体
            let k = (w / p.w.max(1e-6)) * 0.5;
            p.r = p.r * (1.0 - k) + nr * k;
            p.g = p.g * (1.0 - k) + ng * k;
            p.b = p.b * (1.0 - k) + nb * k;
        }
    }

    /// 加法混合（用于发光、Bloom 的源）。
    pub fn add(&mut self, x: f32, y: f32, color: Color, w: f32) {
        let w = w.clamp(0.0, 1.0);
        if w <= 0.0 {
            return;
        }
        let xi = x.round() as i32;
        let yi = y.round() as i32;
        let Some(i) = self.index(xi, yi) else {
            return;
        };
        let (r, g, b) = color::rgb_of(color);
        let p = &mut self.pix[i];
        p.r = (p.r + r as f32 / 255.0 * w).min(1.0);
        p.g = (p.g + g as f32 / 255.0 * w).min(1.0);
        p.b = (p.b + b as f32 / 255.0 * w).min(1.0);
        p.w = (p.w + w).min(1.0);
    }

    /// 直线（DDA，浮点端点）。`w` 是线宽（子像素），> 1 时沿法线加粗。
    pub fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, color: Color, w: f32) {
        let dx = x1 - x0;
        let dy = y1 - y0;
        let steps = dx.abs().max(dy.abs()).ceil().max(1.0) as usize;
        let alpha = (w * 1.5).clamp(0.0, 1.0);
        // 线宽取整并保底 2 个子像素。
        //
        // Braille 单元只有 2×4 子像素，宽度 1 意味着整条线只占一列 ——
        // 一个字符格里最多点亮 4 个点，稀疏到看不出是线。
        // 保底 2 让轮廓在任何能量下都是连续实线。
        let lw = (w.max(1.0).round() as i32).clamp(2, 8);
        let half = (lw - 1) / 2;
        // 法线方向按主轴吸附到整数偏移：Braille 的 set() 会做浮点取整，
        // 斜线的亚像素偏移会被舍掉，结果粗细不均。
        let (nx, ny) = if dx.abs() >= dy.abs() {
            (0.0, 1.0)
        } else {
            (1.0, 0.0)
        };
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            let (px, py) = (x0 + dx * t, y0 + dy * t);
            for o in -half..=(lw - 1 - half) {
                self.set(px + nx * o as f32, py + ny * o as f32, color, alpha);
            }
        }
    }

    /// 折线。
    pub fn polyline(&mut self, pts: &[(f32, f32)], color: Color, w: f32, close: bool) {
        if pts.len() < 2 {
            return;
        }
        for seg in pts.windows(2) {
            self.line(seg[0].0, seg[0].1, seg[1].0, seg[1].1, color, w);
        }
        if close {
            let a = pts[pts.len() - 1];
            let b = pts[0];
            self.line(a.0, a.1, b.0, b.1, color, w);
        }
    }

    /// 圆（描边）。
    pub fn circle(&mut self, cx: f32, cy: f32, r: f32, color: Color, w: f32) {
        if r <= 0.0 {
            return;
        }
        let steps = ((r * 6.0).ceil() as usize).clamp(24, 720);
        let mut prev: Option<(f32, f32)> = None;
        for i in 0..=steps {
            let a = i as f32 / steps as f32 * std::f32::consts::TAU;
            let p = (cx + r * a.cos(), cy + r * a.sin());
            if let Some(q) = prev {
                self.line(q.0, q.1, p.0, p.1, color, w);
            }
            prev = Some(p);
        }
    }

    /// 圆盘（填充，扫描线）。
    pub fn disc(&mut self, cx: f32, cy: f32, r: f32, color: Color, w: f32) {
        if r <= 0.0 {
            return;
        }
        let y0 = (cy - r).floor().max(0.0) as i32;
        let y1 = (cy + r).ceil().min(self.sh as f32 - 1.0) as i32;
        for y in y0..=y1 {
            let dy = y as f32 - cy;
            let d2 = r * r - dy * dy;
            if d2 < 0.0 {
                continue;
            }
            let half = d2.sqrt();
            let x0 = (cx - half).ceil().max(0.0) as i32;
            let x1 = (cx + half).floor().min(self.sw as f32 - 1.0) as i32;
            for x in x0..=x1 {
                // 边缘 1 像素做 AA：离圆心越远覆盖度越低
                let dist = ((x as f32 - cx).powi(2) + dy * dy).sqrt();
                let aa = ((r - dist) / 1.0).clamp(0.0, 1.0);
                if aa > 0.0 {
                    self.set(x as f32, y as f32, color, w * aa);
                }
            }
        }
    }

    /// 椭圆（描边）。
    pub fn ellipse(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, color: Color, w: f32) {
        if rx <= 0.0 || ry <= 0.0 {
            return;
        }
        let steps = ((rx.max(ry) * 6.0).ceil() as usize).clamp(24, 720);
        let mut prev: Option<(f32, f32)> = None;
        for i in 0..=steps {
            let a = i as f32 / steps as f32 * std::f32::consts::TAU;
            let p = (cx + rx * a.cos(), cy + ry * a.sin());
            if let Some(q) = prev {
                self.line(q.0, q.1, p.0, p.1, color, w);
            }
            prev = Some(p);
        }
    }

    /// 矩形（描边）。
    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Color, weight: f32) {
        self.line(x, y, x + w, y, color, weight);
        self.line(x + w, y, x + w, y + h, color, weight);
        self.line(x + w, y + h, x, y + h, color, weight);
        self.line(x, y + h, x, y, color, weight);
    }

    /// 矩形（填充）。
    pub fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Color, weight: f32) {
        let x0 = x.floor().max(0.0) as i32;
        let y0 = y.floor().max(0.0) as i32;
        let x1 = (x + w).ceil().min(self.sw as f32 - 1.0) as i32;
        let y1 = (y + h).ceil().min(self.sh as f32 - 1.0) as i32;
        for yy in y0..=y1 {
            for xx in x0..=x1 {
                self.set(xx as f32, yy as f32, color, weight);
            }
        }
    }

    /// 把**已有内容**的颜色朝 `color` 混合，覆盖率不变。
    ///
    /// 这是背景压暗 / 色调偏移的正确做法。直接用 [`fill`](Self::fill)
    /// 会连覆盖率一起设满，在 Braille 画布上等于点亮每个单元的全部 8 个点，
    /// 整幅画面立刻变成一堵实心墙（外加扫描线 → 满屏条纹）。
    pub fn tint(&mut self, color: Color, amount: f32) {
        self.tint_rect(0.0, 0.0, self.sw as f32, self.sh as f32, color, amount);
    }

    /// [`tint`](Self::tint) 的矩形版本。
    pub fn tint_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Color, amount: f32) {
        let (cr, cg, cb) = color::rgb_of(color);
        let (cr, cg, cb) = (cr as f32 / 255.0, cg as f32 / 255.0, cb as f32 / 255.0);
        let a = amount.clamp(0.0, 1.0);
        let x0 = x.floor().max(0.0) as i32;
        let y0 = y.floor().max(0.0) as i32;
        let x1 = (x + w).ceil().min(self.sw as f32) as i32;
        let y1 = (y + h).ceil().min(self.sh as f32) as i32;
        for yy in y0..y1 {
            for xx in x0..x1 {
                let Some(i) = self.index(xx, yy) else {
                    continue;
                };
                let p = &mut self.pix[i];
                if p.w <= 0.0 {
                    continue;
                }
                p.r += (cr - p.r) * a;
                p.g += (cg - p.g) * a;
                p.b += (cb - p.b) * a;
            }
        }
    }

    /// 把另一块画布贴上来（`alpha` 为整体不透明度）。
    pub fn blit(&mut self, src: &CharCanvas, dx: i32, dy: i32, alpha: f32) {
        let alpha = alpha.clamp(0.0, 1.0);
        for y in 0..src.sh as i32 {
            let ty = y + dy;
            if ty < 0 || ty >= self.sh as i32 {
                continue;
            }
            for x in 0..src.sw as i32 {
                let tx = x + dx;
                if tx < 0 || tx >= self.sw as i32 {
                    continue;
                }
                let sp = src.pixel(x, y);
                if sp.is_empty() {
                    continue;
                }
                // 子像素尺寸不同时按最近邻取样（不常见，够用）
                let color = sp.to_color();
                self.set(tx as f32, ty as f32, color, sp.w * alpha);
            }
        }
    }

    /// 整块画布做「逐行水平位移」（故障效果的基础操作）。
    ///
    /// 使用内部交换缓冲，不产生每帧堆分配。
    pub fn shift_rows(&mut self, offsets: &[i32]) {
        if offsets.is_empty() {
            return;
        }
        let sw = self.sw as usize;
        let sh = self.sh as usize;
        self.swap.clear();
        self.swap.resize(sw * sh, Pixel::default());
        for y in 0..sh {
            let off = offsets[y % offsets.len()];
            for x in 0..sw {
                let sx = x as i32 - off;
                if sx >= 0 && (sx as usize) < sw {
                    self.swap[y * sw + x] = self.pix[y * sw + sx as usize];
                }
            }
        }
        std::mem::swap(&mut self.pix, &mut self.swap);
    }

    /// 整体平移（子像素单位）。移出边界的部分直接丢弃。
    pub fn translate(&mut self, dx: i32, dy: i32) {
        if dx == 0 && dy == 0 {
            return;
        }
        let sw = self.sw as usize;
        let sh = self.sh as usize;
        self.swap.clear();
        self.swap.resize(sw * sh, Pixel::default());
        for y in 0..sh {
            let sy = y as i32 - dy;
            if sy < 0 || sy >= sh as i32 {
                continue;
            }
            for x in 0..sw {
                let sx = x as i32 - dx;
                if sx < 0 || sx >= sw as i32 {
                    continue;
                }
                self.swap[y * sw + x] = self.pix[sy as usize * sw + sx as usize];
            }
        }
        std::mem::swap(&mut self.pix, &mut self.swap);
    }

    /// 取字符单元 `(cx, cy)` 的最终字符与颜色；`None` 表示该单元留空。
    pub fn cell(&self, cx: u16, cy: u16) -> Option<CellOut> {
        if cx >= self.cols || cy >= self.rows {
            return None;
        }
        match self.mode {
            RenderMode::Braille => self.cell_braille(cx, cy),
            RenderMode::Half => self.cell_half(cx, cy),
            RenderMode::Quadrant => self.cell_quadrant(cx, cy),
            _ => self.cell_ascii(cx, cy),
        }
    }

    fn cell_braille(&self, cx: u16, cy: u16) -> Option<CellOut> {
        let mut cov = [[0.0f32; 4]; 2];
        let (mut ar, mut ag, mut ab, mut aw) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        for sx in 0..2u16 {
            for sy in 0..4u16 {
                let p = self.pixel((cx * 2 + sx) as i32, (cy * 4 + sy) as i32);
                cov[sx as usize][sy as usize] = p.w;
                let w = p.w;
                ar += p.r * w;
                ag += p.g * w;
                ab += p.b * w;
                aw += w;
            }
        }
        let bits = braille::bits_from_coverage(&cov, 0.5);
        let ch = braille::braille_char(bits);
        if ch == ' ' || aw <= 0.0 {
            return None;
        }
        let fg = Color::Rgb(
            ((ar / aw).clamp(0.0, 1.0) * 255.0).round() as u8,
            ((ag / aw).clamp(0.0, 1.0) * 255.0).round() as u8,
            ((ab / aw).clamp(0.0, 1.0) * 255.0).round() as u8,
        );
        Some(CellOut {
            ch,
            fg,
            bg: self.bg,
        })
    }

    fn cell_half(&self, cx: u16, cy: u16) -> Option<CellOut> {
        let top = self.pixel(cx as i32, (cy * 2) as i32);
        let bot = self.pixel(cx as i32, (cy * 2 + 1) as i32);
        let ch = halfblock::half_char(top.w, bot.w, 0.5);
        if ch == ' ' {
            return None;
        }
        let fg = if top.w > 0.0 {
            top.to_color()
        } else {
            bot.to_color()
        };
        let bg = if bot.w > 0.0 {
            bot.to_color()
        } else {
            Color::Reset
        };
        Some(CellOut { ch, fg, bg })
    }

    fn cell_quadrant(&self, cx: u16, cy: u16) -> Option<CellOut> {
        // 2×2 象限块 U+2596..U+259F 的查表
        const QUAD: [char; 16] = [
            ' ', '▘', '▝', '▀', '▖', '▌', '▞', '▛', '▗', '▚', '▐', '▜', '▄', '▙', '▟', '█',
        ];
        let mut idx = 0usize;
        let mut cov = [0.0f32; 4];
        let mut aw = 0.0f32;
        let (mut ar, mut ag, mut ab) = (0.0f32, 0.0f32, 0.0f32);
        let order = [(0u16, 0u16), (1, 0), (0, 1), (1, 1)];
        for (i, (sx, sy)) in order.iter().enumerate() {
            let p = self.pixel((cx * 2 + sx) as i32, (cy * 2 + sy) as i32);
            cov[i] = p.w;
            if p.w > 0.5 {
                idx |= 1 << i;
            }
            ar += p.r * p.w;
            ag += p.g * p.w;
            ab += p.b * p.w;
            aw += p.w;
        }
        let ch = QUAD[idx];
        if ch == ' ' || aw <= 0.0 {
            return None;
        }
        let fg = Color::Rgb(
            ((ar / aw).clamp(0.0, 1.0) * 255.0).round() as u8,
            ((ag / aw).clamp(0.0, 1.0) * 255.0).round() as u8,
            ((ab / aw).clamp(0.0, 1.0) * 255.0).round() as u8,
        );
        Some(CellOut {
            ch,
            fg,
            bg: self.bg,
        })
    }

    fn cell_ascii(&self, cx: u16, cy: u16) -> Option<CellOut> {
        let p = self.pixel(cx as i32, cy as i32);
        if p.w <= 0.01 {
            return None;
        }
        let ch = ascii::dither_char(p.w, cx, cy, ascii::RAMP.len());
        if ch == ' ' {
            return None;
        }
        Some(CellOut {
            ch,
            fg: p.to_color(),
            bg: self.bg,
        })
    }

    /// 把画布写进 ratatui 的 `Buffer`。
    ///
    /// 只在 `area` 范围内绘制；画布比区域大时裁剪，比区域小时留空。
    pub fn to_buffer(&self, buf: &mut Buffer, area: Rect) {
        let cols = self.cols.min(area.width);
        let rows = self.rows.min(area.height);
        for cy in 0..rows {
            for cx in 0..cols {
                if let Some(c) = self.cell(cx, cy) {
                    let cell = &mut buf[(area.x + cx, area.y + cy)];
                    cell.set_char(c.ch);
                    cell.set_style(Style::default().fg(c.fg).bg(c.bg));
                }
            }
        }
    }

    /// 直接写一个字符单元（UI 层用，绕过子像素）。
    pub fn put_cell(buf: &mut Buffer, area: Rect, x: u16, y: u16, ch: char, fg: Color, bg: Color) {
        if x >= area.width || y >= area.height {
            return;
        }
        let cell = &mut buf[(area.x + x, area.y + y)];
        cell.set_char(ch);
        cell.set_style(Style::default().fg(fg).bg(bg));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn canvas(mode: RenderMode) -> CharCanvas {
        CharCanvas::new(8, 4, mode)
    }

    #[test]
    fn dimensions_follow_mode() {
        let b = CharCanvas::new(10, 5, RenderMode::Braille);
        assert_eq!((b.sw, b.sh), (20, 20));
        let h = CharCanvas::new(10, 5, RenderMode::Half);
        assert_eq!((h.sw, h.sh), (10, 10));
        let a = CharCanvas::new(10, 5, RenderMode::Ascii);
        assert_eq!((a.sw, a.sh), (10, 5));
    }

    #[test]
    fn auto_mode_resolves_to_concrete() {
        let m = resolve_mode(RenderMode::Auto);
        assert_ne!(m, RenderMode::Auto);
    }

    #[test]
    fn clear_makes_everything_transparent() {
        let mut c = canvas(RenderMode::Braille);
        c.fill(color::CYAN, 1.0);
        c.clear();
        for y in 0..c.sh as i32 {
            for x in 0..c.sw as i32 {
                assert!(c.pixel(x, y).is_empty());
            }
        }
    }

    #[test]
    fn set_out_of_bounds_is_ignored() {
        let mut c = canvas(RenderMode::Braille);
        c.set(-5.0, 0.0, color::CYAN, 1.0);
        c.set(0.0, -5.0, color::CYAN, 1.0);
        c.set(9999.0, 0.0, color::CYAN, 1.0);
        c.set(0.0, 9999.0, color::CYAN, 1.0);
        // 不该 panic，也不该写入任何像素
        assert_eq!(c.pix.iter().filter(|p| !p.is_empty()).count(), 0);
    }

    #[test]
    fn brighter_pixel_wins_on_overlap() {
        let mut c = canvas(RenderMode::Braille);
        c.set(1.0, 1.0, color::DARK_GREY, 0.3);
        c.set(1.0, 1.0, color::WHITE, 0.9);
        let p = c.pixel(1, 1);
        assert!((p.w - 0.9).abs() < 1e-6);
        assert!(p.r > 0.9 && p.g > 0.9 && p.b > 0.9);
    }

    #[test]
    fn add_accumulates_and_clamps() {
        let mut c = canvas(RenderMode::Braille);
        for _ in 0..10 {
            c.add(0.0, 0.0, color::WHITE, 0.3);
        }
        let p = c.pixel(0, 0);
        assert!(p.w <= 1.0);
        assert!(p.r <= 1.0);
    }

    #[test]
    fn line_draws_both_endpoints() {
        let mut c = canvas(RenderMode::Braille);
        c.line(0.0, 0.0, 10.0, 10.0, color::CYAN, 1.0);
        assert!(!c.pixel(0, 0).is_empty());
        assert!(!c.pixel(10, 10).is_empty());
        // 中点也应在线上
        assert!(!c.pixel(5, 5).is_empty());
    }

    #[test]
    fn horizontal_line_covers_full_span() {
        let mut c = canvas(RenderMode::Braille);
        c.line(2.0, 3.0, 12.0, 3.0, color::CYAN, 1.0);
        for x in 2..=12 {
            assert!(!c.pixel(x, 3).is_empty(), "x={x} 缺失");
        }
    }

    #[test]
    fn disc_fills_interior() {
        let mut c = CharCanvas::new(20, 20, RenderMode::Braille);
        c.disc(20.0, 20.0, 8.0, color::MAGENTA, 1.0);
        // 圆心
        assert!(!c.pixel(20, 20).is_empty());
        // 内部
        assert!(!c.pixel(20, 24).is_empty());
        // 外部
        assert!(c.pixel(20, 34).is_empty());
    }

    #[test]
    fn circle_is_hollow() {
        let mut c = CharCanvas::new(20, 20, RenderMode::Braille);
        c.circle(20.0, 20.0, 10.0, color::CYAN, 1.0);
        assert!(!c.pixel(30, 20).is_empty(), "右边缘缺失");
        assert!(c.pixel(20, 20).is_empty(), "圆心应为空");
    }

    #[test]
    fn rect_outline_is_hollow() {
        let mut c = CharCanvas::new(20, 20, RenderMode::Braille);
        c.rect(5.0, 5.0, 10.0, 10.0, color::WHITE, 1.0);
        assert!(!c.pixel(5, 5).is_empty());
        assert!(!c.pixel(15, 15).is_empty());
        assert!(c.pixel(10, 10).is_empty(), "矩形内部应为空");
    }

    #[test]
    fn fill_rect_covers_area() {
        let mut c = CharCanvas::new(20, 20, RenderMode::Braille);
        c.fill_rect(2.0, 2.0, 5.0, 5.0, color::WHITE, 1.0);
        assert!(!c.pixel(3, 3).is_empty());
        assert!(!c.pixel(6, 6).is_empty());
        assert!(c.pixel(10, 10).is_empty());
    }

    #[test]
    fn braille_cell_renders_a_character() {
        let mut c = canvas(RenderMode::Braille);
        c.set(0.0, 0.0, color::CYAN, 1.0);
        let out = c.cell(0, 0).expect("应有字符");
        assert!(braille::is_braille(out.ch), "得到 {:?}", out.ch);
        assert_eq!(out.ch, '\u{2801}');
    }

    #[test]
    fn empty_cell_returns_none() {
        let c = canvas(RenderMode::Braille);
        assert!(c.cell(0, 0).is_none());
    }

    #[test]
    fn ascii_cell_uses_ramp() {
        let mut c = canvas(RenderMode::Ascii);
        c.set(0.0, 0.0, color::WHITE, 1.0);
        let out = c.cell(0, 0).unwrap();
        assert!(ascii::RAMP.contains(&out.ch), "得到 {:?}", out.ch);
        assert_eq!(out.ch, '@');
    }

    #[test]
    fn half_cell_uses_block_chars() {
        let mut c = canvas(RenderMode::Half);
        c.set(0.0, 0.0, color::CYAN, 1.0); // 上
        let out = c.cell(0, 0).unwrap();
        assert_eq!(out.ch, halfblock::UPPER);
    }

    #[test]
    fn quadrant_cell_uses_quadrant_chars() {
        let mut c = canvas(RenderMode::Quadrant);
        c.set(0.0, 0.0, color::CYAN, 1.0);
        c.set(1.0, 0.0, color::CYAN, 1.0);
        c.set(0.0, 1.0, color::CYAN, 1.0);
        c.set(1.0, 1.0, color::CYAN, 1.0);
        let out = c.cell(0, 0).unwrap();
        assert_eq!(out.ch, '█');
    }

    #[test]
    fn shift_rows_moves_content() {
        let mut c = canvas(RenderMode::Ascii);
        c.set(3.0, 0.0, color::WHITE, 1.0);
        c.shift_rows(&[2]);
        assert!(c.pixel(3, 0).is_empty());
        assert!(!c.pixel(5, 0).is_empty());
    }

    #[test]
    fn shift_rows_clips_at_edges() {
        let mut c = canvas(RenderMode::Ascii);
        c.set(0.0, 0.0, color::WHITE, 1.0);
        c.shift_rows(&[5]);
        // 内容整体右移 5：原位置清空，新位置有内容
        assert!(c.pixel(0, 0).is_empty());
        assert!(!c.pixel(5, 0).is_empty());
        // 移出右边界的部分被丢弃，不环绕回左侧
        c.shift_rows(&[1000]);
        assert!(c.pixel(0, 0).is_empty());
        assert!(c.pixel(5, 0).is_empty());
    }

    #[test]
    fn blit_copies_region() {
        let mut src = CharCanvas::new(4, 4, RenderMode::Braille);
        src.set(1.0, 1.0, color::WHITE, 1.0);
        let mut dst = CharCanvas::new(8, 8, RenderMode::Braille);
        dst.blit(&src, 2, 2, 1.0);
        assert!(!dst.pixel(3, 3).is_empty());
        assert!(dst.pixel(1, 1).is_empty());
    }

    #[test]
    fn blit_with_zero_alpha_is_noop() {
        let mut src = CharCanvas::new(4, 4, RenderMode::Braille);
        src.set(1.0, 1.0, color::WHITE, 1.0);
        let mut dst = CharCanvas::new(8, 8, RenderMode::Braille);
        dst.blit(&src, 0, 0, 0.0);
        assert!(dst.pixel(1, 1).is_empty());
    }

    #[test]
    fn to_buffer_writes_within_area() {
        let mut c = CharCanvas::new(4, 2, RenderMode::Braille);
        c.set(0.0, 0.0, color::CYAN, 1.0);
        let area = Rect::new(0, 0, 4, 2);
        let mut buf = Buffer::empty(area);
        c.to_buffer(&mut buf, area);
        assert_eq!(buf[(0, 0)].symbol(), "\u{2801}");
    }

    #[test]
    fn to_buffer_clips_to_smaller_area() {
        let mut c = CharCanvas::new(10, 10, RenderMode::Braille);
        c.set(0.0, 0.0, color::CYAN, 1.0);
        c.set(18.0, 18.0, color::CYAN, 1.0); // 第 9 列第 4 行
        let area = Rect::new(0, 0, 4, 2);
        let mut buf = Buffer::empty(area);
        // 不应 panic
        c.to_buffer(&mut buf, area);
        assert_eq!(buf[(0, 0)].symbol(), "\u{2801}");
    }

    #[test]
    fn fill_sets_every_pixel() {
        let mut c = canvas(RenderMode::Ascii);
        c.fill(color::MAGENTA, 0.7);
        for y in 0..c.sh as i32 {
            for x in 0..c.sw as i32 {
                assert!((c.pixel(x, y).w - 0.7).abs() < 1e-6);
            }
        }
    }

    #[test]
    fn zero_weight_set_is_ignored() {
        let mut c = canvas(RenderMode::Braille);
        c.set(1.0, 1.0, color::WHITE, 0.0);
        assert!(c.pixel(1, 1).is_empty());
    }

    #[test]
    fn ellipse_touches_both_axes() {
        let mut c = CharCanvas::new(20, 20, RenderMode::Braille);
        c.ellipse(20.0, 20.0, 10.0, 5.0, color::CYAN, 1.0);
        assert!(!c.pixel(30, 20).is_empty(), "x 轴端点缺失");
        assert!(!c.pixel(20, 25).is_empty(), "y 轴端点缺失");
    }

    #[test]
    fn put_cell_respects_bounds() {
        let area = Rect::new(0, 0, 4, 2);
        let mut buf = Buffer::empty(area);
        CharCanvas::put_cell(&mut buf, area, 1, 1, 'X', color::WHITE, Color::Reset);
        CharCanvas::put_cell(&mut buf, area, 99, 99, 'Y', color::WHITE, Color::Reset);
        assert_eq!(buf[(1, 1)].symbol(), "X");
    }
}

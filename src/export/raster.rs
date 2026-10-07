//! 字符 → 像素位图。
//!
//! 两条路径：
//! 1. **Braille 字符**（U+2800..U+28FF）用几何方式画点阵 —— 位映射是已知的，
//!    直接画 8 个圆点比任何字体都准，而且**不依赖系统字体**。
//!    默认渲染模式是 Braille，因此即使机器上一个字体都没有，导出依然可用。
//! 2. **其余字符**用 `fontdue` 光栅化系统等宽字体；字体缺失时降级为方块。
//!
//! 字体探测顺序见 [`FONT_CANDIDATES`]，也可用环境变量 `WEM_FONT` 指定。

use anyhow::{Context, Result};
use fontdue::{Font, FontSettings};
use image::{Rgba, RgbaImage};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use crate::render::braille::BRAILLE_BITS;
use crate::render::color as col;

/// 常见等宽字体路径（按优先级）。含中日韩字体以便覆盖半角片假名。
pub const FONT_CANDIDATES: &[&str] = &[
    "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
    "/usr/share/fonts/dejavu/DejaVuSansMono.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
    "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/noto/NotoSansMono-Regular.ttf",
    "/usr/share/fonts/TTF/JetBrainsMono-Regular.ttf",
    "/usr/share/fonts/jetbrains-mono/JetBrainsMono-Regular.ttf",
    "/usr/share/fonts/liberation/LiberationMono-Regular.ttf",
    "/usr/share/fonts/TTF/LiberationMono-Regular.ttf",
    "/usr/share/fonts/gnu-free/FreeMono.ttf",
    "/usr/share/fonts/truetype/freefont/FreeMono.ttf",
    "/System/Library/Fonts/Menlo.ttc",
    "C:/Windows/Fonts/consola.ttf",
];

/// 找到并加载一个可用字体。返回 `None` 表示没有可用字体（Braille 仍可渲染）。
pub fn load_font() -> Option<(Font, String)> {
    let mut candidates: Vec<String> = Vec::new();
    if let Ok(p) = std::env::var("WEM_FONT") {
        if !p.is_empty() {
            candidates.push(p);
        }
    }
    candidates.extend(FONT_CANDIDATES.iter().map(|s| s.to_string()));

    for p in candidates {
        let Ok(data) = std::fs::read(&p) else {
            continue;
        };
        // fontdue 对损坏/不支持的字体返回 Err，此时继续尝试下一个
        if let Ok(font) = Font::from_bytes(data, FontSettings::default()) {
            return Some((font, p));
        }
    }
    None
}

/// 中日韩回退字体路径。
///
/// 主字体表里的 DejaVu / JetBrains / Liberation **都没有汉字**，
/// 而本片的歌词、标题、章节名全是日文与汉字。没有回退字体的话，
/// 所有文字都会渲染成豆腐块 —— 这是必须堵死的坑。
pub const CJK_FALLBACK: &[&str] = &[
    "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/opentype/noto/NotoSerifCJK-Regular.ttc",
    "/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf",
    "/usr/share/fonts/truetype/droid/DroidSansFallback.ttf",
    "/usr/share/fonts/truetype/fonts-japanese-gothic.ttf",
    "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc",
    "C:/Windows/Fonts/meiryo.ttc",
    "C:/Windows/Fonts/msgothic.ttc",
];

/// 加载一个中日韩字体。返回 `None` 表示系统上没有可用汉字字体。
pub fn load_cjk_font() -> Option<(Font, String)> {
    let mut candidates: Vec<String> = Vec::new();
    if let Ok(p) = std::env::var("WEM_CJK_FONT") {
        if !p.is_empty() {
            candidates.push(p);
        }
    }
    candidates.extend(CJK_FALLBACK.iter().map(|s| s.to_string()));

    for p in candidates {
        let Ok(data) = std::fs::read(&p) else {
            continue;
        };
        if let Ok(font) = Font::from_bytes(data, FontSettings::default()) {
            return Some((font, p));
        }
    }
    None
}

/// 字符光栅化器。
pub struct Rasterizer {
    font: Option<Font>,
    /// 缺字回退字体（中日韩）。
    fallback: Option<Font>,
    /// 字体文件路径（日志用）
    font_path: Option<String>,
    /// 回退字体路径（日志用）
    fallback_path: Option<String>,
    /// 单元宽（像素）
    pub cell_w: u32,
    /// 单元高（像素）
    pub cell_h: u32,
    /// 字号（像素）
    pub font_size: f32,
}

impl Rasterizer {
    /// 新建。`cell_w × cell_h` 是一个字符单元在输出图像里占的像素。
    pub fn new(cell_w: u32, cell_h: u32) -> Self {
        let (font, font_path) = match load_font() {
            Some((f, p)) => {
                tracing::info!(font = %p, "光栅化字体已加载");
                (Some(f), Some(p))
            }
            None => {
                tracing::warn!("未找到系统等宽字体，非 Braille 字符将降级为方块");
                (None, None)
            }
        };
        let cell_w = cell_w.max(2);
        let cell_h = cell_h.max(2);

        // 回退字体：只在主字体缺这个字形时才用（见 `draw_char`）。
        let (fallback, fallback_path) = match load_cjk_font() {
            Some((f, p)) => {
                tracing::info!(font = %p, "中日韩回退字体已加载");
                (Some(f), Some(p))
            }
            None => {
                tracing::warn!(
                    "未找到中日韩字体，汉字将渲染为方块；\
                     请安装 fonts-noto-cjk 或设置 WEM_CJK_FONT"
                );
                (None, None)
            }
        };

        Self {
            font,
            fallback,
            font_path,
            fallback_path,
            cell_w,
            cell_h,
            // 字号取单元高度的 85%，给上下留一点呼吸空间
            font_size: cell_h as f32 * 0.85,
        }
    }

    /// 实际使用的字体路径。
    pub fn font_path(&self) -> Option<&str> {
        self.font_path.as_deref()
    }

    /// 中日韩回退字体路径。
    pub fn fallback_path(&self) -> Option<&str> {
        self.fallback_path.as_deref()
    }

    /// 是否有可用字体。
    pub fn has_font(&self) -> bool {
        self.font.is_some()
    }

    /// 把一块 ratatui `Buffer` 渲染成 RGBA 图像。
    ///
    /// 性能要点：**先整图填一次默认背景色**，再只对背景非默认的单元单独填充。
    /// 逐单元铺背景的话，160×50 画布要写 100 万次像素，
    /// 而其中 99% 写的是同一个黑色 —— 实测这一步占了导出耗时的绝大部分。
    pub fn render(&self, buf: &Buffer, area: Rect) -> RgbaImage {
        let w = area.width as u32 * self.cell_w;
        let h = area.height as u32 * self.cell_h;
        // Color::Reset 即「跟随终端默认背景」，导出时按黑处理
        let mut img = RgbaImage::from_pixel(w.max(1), h.max(1), Rgba([0, 0, 0, 255]));

        for cy in 0..area.height {
            for cx in 0..area.width {
                let cell = &buf[(area.x + cx, area.y + cy)];
                let x0 = cx as u32 * self.cell_w;
                let y0 = cy as u32 * self.cell_h;

                // 只有非默认背景才需要单独铺 —— 整图已经是默认色了
                let bg = cell.bg;
                if !matches!(bg, Color::Reset) {
                    let bg_rgba = color_to_rgba(bg, 255);
                    if bg_rgba.0[0] != 0 || bg_rgba.0[1] != 0 || bg_rgba.0[2] != 0 {
                        fill_cell(&mut img, x0, y0, self.cell_w, self.cell_h, bg_rgba);
                    }
                }

                let sym = cell.symbol();
                let mut chars = sym.chars();
                let Some(ch) = chars.next() else {
                    continue;
                };
                if ch == ' ' {
                    continue;
                }
                // 双宽字符（理论上不该出现）只画第一个
                let fg = color_to_rgba(cell.fg, 255);
                self.draw_char(&mut img, x0, y0, ch, fg);
            }
        }
        img
    }

    /// 画一个字符到指定单元。
    pub fn draw_char(&self, img: &mut RgbaImage, x0: u32, y0: u32, ch: char, fg: Rgba<u8>) {
        // Braille 走几何路径
        let cp = ch as u32;
        if (0x2800..=0x28FF).contains(&cp) {
            let bits = (cp - 0x2800) as u8;
            self.draw_braille(img, x0, y0, bits, fg);
            return;
        }
        // 半块/方块族：直接用矩形填充，比字体更精确
        match ch {
            '█' => {
                fill_cell(img, x0, y0, self.cell_w, self.cell_h, fg);
                return;
            }
            '▀' => {
                fill_cell(img, x0, y0, self.cell_w, self.cell_h / 2, fg);
                return;
            }
            '▄' => {
                fill_cell(
                    img,
                    x0,
                    y0 + self.cell_h / 2,
                    self.cell_w,
                    self.cell_h - self.cell_h / 2,
                    fg,
                );
                return;
            }
            '▌' => {
                fill_cell(img, x0, y0, self.cell_w / 2, self.cell_h, fg);
                return;
            }
            '▐' => {
                fill_cell(
                    img,
                    x0 + self.cell_w / 2,
                    y0,
                    self.cell_w - self.cell_w / 2,
                    self.cell_h,
                    fg,
                );
                return;
            }
            _ => {}
        }

        // 字形回退：主字体没有这个字形时换用中日韩字体。
        // `lookup_glyph_index` 返回 0 即缺字 —— DejaVu 遇到汉字就是这种情况。
        let chosen = match &self.font {
            Some(f) if f.lookup_glyph_index(ch) != 0 => self.font.as_ref(),
            _ => self.fallback.as_ref().or(self.font.as_ref()),
        };
        match chosen {
            Some(font) => {
                let (metrics, bitmap) = font.rasterize(ch, self.font_size);
                if metrics.width == 0 || metrics.height == 0 {
                    return;
                }
                // 基线对齐：让字形在单元内垂直居中
                let ascent = font.horizontal_line_metrics(self.font_size);
                let baseline = match ascent {
                    Some(m) => (self.cell_h as f32 * 0.5 - (m.ascent + m.descent) * 0.5 + m.ascent)
                        .round() as i32,
                    None => (self.cell_h as f32 * 0.8).round() as i32,
                };
                // 水平居中
                let x_off = ((self.cell_w as i32 - metrics.width as i32) / 2)
                    .max(0)
                    .min((self.cell_w as i32 - metrics.width as i32).max(0));
                let px0 = x0 as i32 + x_off;
                let py0 = y0 as i32 + baseline - metrics.height as i32 - metrics.ymin;
                for gy in 0..metrics.height {
                    for gx in 0..metrics.width {
                        let a = bitmap[gy * metrics.width + gx];
                        if a == 0 {
                            continue;
                        }
                        blend_pixel(img, px0 + gx as i32, py0 + gy as i32, fg, a);
                    }
                }
            }
            None => {
                // 无字体：画一个「未知字符」方块，至少能看出这里有东西
                let m = (self.cell_w.min(self.cell_h) / 4).max(1);
                fill_cell(
                    img,
                    x0 + m,
                    y0 + m,
                    self.cell_w.saturating_sub(m * 2).max(1),
                    self.cell_h.saturating_sub(m * 2).max(1),
                    fg,
                );
            }
        }
    }

    /// 选字体：主字体缺字时回退到中日韩字体。
    pub fn pick(&self, ch: char) -> Option<&Font> {
        match &self.font {
            Some(f) if f.lookup_glyph_index(ch) != 0 => Some(f),
            _ => self.fallback.as_ref().or(self.font.as_ref()),
        }
    }

    /// 按**像素坐标**画真字形文字，字号任意。`y` 是基线，返回下一个字的左边缘。
    ///
    /// 这是参考实现（`MisakaZentai/world-execute-me-dsh-pv` 的 `tuikit.py`）的核心画法：
    /// 文字用真字体光栅化，而不是塞进字符格。字符格是 8×16 像素，
    /// 而汉字是方块字 —— 13 像素宽的汉字塞进 8 像素的格子，相邻两字重叠约 40%，
    /// 整行必然糊成一团。按像素画就完全没有这个问题。
    pub fn text(
        &self,
        img: &mut RgbaImage,
        x: i32,
        y: i32,
        s: &str,
        px: f32,
        color: [u8; 3],
        alpha: f32,
    ) -> i32 {
        let a = (alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
        if a == 0 || px <= 0.0 {
            return x;
        }
        let fg = Rgba([color[0], color[1], color[2], 255]);
        let mut cx = x;
        for ch in s.chars() {
            if ch.is_control() {
                continue;
            }
            let Some(font) = self.pick(ch) else {
                cx += (px * 0.6).round() as i32;
                continue;
            };
            let (m, bitmap) = font.rasterize(ch, px);
            if m.width > 0 && m.height > 0 {
                let py0 = y - m.height as i32 - m.ymin;
                for gy in 0..m.height {
                    for gx in 0..m.width {
                        let cov = bitmap[gy * m.width + gx];
                        if cov == 0 {
                            continue;
                        }
                        // 覆盖度 × 全局 alpha
                        let aa = ((cov as u32 * a as u32) / 255) as u8;
                        blend_pixel(img, cx + gx as i32, py0 + gy as i32, fg, aa);
                    }
                }
            }
            cx += m.advance_width.round() as i32;
        }
        cx
    }

    /// 量一段文字在 `px` 字号下的像素宽度（不画）。
    pub fn measure_text(&self, s: &str, px: f32) -> i32 {
        let mut w = 0i32;
        for ch in s.chars() {
            if ch.is_control() {
                continue;
            }
            match self.pick(ch) {
                Some(font) => w += font.metrics(ch, px).advance_width.round() as i32,
                None => w += (px * 0.6).round() as i32,
            }
        }
        w
    }

    /// 用几何方式画 Braille 点阵（2×4）。
    pub fn draw_braille(&self, img: &mut RgbaImage, x0: u32, y0: u32, bits: u8, fg: Rgba<u8>) {
        if bits == 0 {
            return;
        }
        let sub_w = self.cell_w as f32 / 2.0;
        let sub_h = self.cell_h as f32 / 4.0;
        // 点半径：取子格短边的 45%，保证点之间有空隙但不至于看不见
        let r = (sub_w.min(sub_h) * 0.45).max(0.6);
        for (sx, col_bits) in BRAILLE_BITS.iter().enumerate() {
            for (sy, bit) in col_bits.iter().enumerate() {
                if bits & bit == 0 {
                    continue;
                }
                let cx = x0 as f32 + (sx as f32 + 0.5) * sub_w;
                let cy = y0 as f32 + (sy as f32 + 0.5) * sub_h;
                draw_disc(img, cx, cy, r, fg);
            }
        }
    }
}

/// 颜色 → RGBA。
pub fn color_to_rgba(c: Color, alpha: u8) -> Rgba<u8> {
    let (r, g, b) = col::rgb_of(c);
    Rgba([r, g, b, alpha])
}

/// 填充一块矩形。
pub fn fill_cell(img: &mut RgbaImage, x: u32, y: u32, w: u32, h: u32, color: Rgba<u8>) {
    let (iw, ih) = (img.width(), img.height());
    let x1 = (x + w).min(iw);
    let y1 = (y + h).min(ih);
    for yy in y..y1 {
        for xx in x..x1 {
            img.put_pixel(xx, yy, color);
        }
    }
}

/// 画一个抗锯齿实心圆。
pub fn draw_disc(img: &mut RgbaImage, cx: f32, cy: f32, r: f32, color: Rgba<u8>) {
    if r <= 0.0 {
        return;
    }
    let (iw, ih) = (img.width() as i32, img.height() as i32);
    let x0 = (cx - r - 1.0).floor() as i32;
    let x1 = (cx + r + 1.0).ceil() as i32;
    let y0 = (cy - r - 1.0).floor() as i32;
    let y1 = (cy + r + 1.0).ceil() as i32;
    for y in y0..=y1 {
        if y < 0 || y >= ih {
            continue;
        }
        for x in x0..=x1 {
            if x < 0 || x >= iw {
                continue;
            }
            let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
            // 边缘 1 像素内线性过渡
            let a = ((r - d) / 1.0).clamp(0.0, 1.0);
            if a > 0.0 {
                blend_pixel(img, x, y, color, (a * 255.0) as u8);
            }
        }
    }
}

/// 按 alpha 混合一个像素。
pub fn blend_pixel(img: &mut RgbaImage, x: i32, y: i32, color: Rgba<u8>, alpha: u8) {
    if alpha == 0 {
        return;
    }
    if x < 0 || y < 0 || x >= img.width() as i32 || y >= img.height() as i32 {
        return;
    }
    let a = alpha as f32 / 255.0;
    let dst = img.get_pixel(x as u32, y as u32);
    let mix = |s: u8, d: u8| -> u8 { (s as f32 * a + d as f32 * (1.0 - a)).round() as u8 };
    img.put_pixel(
        x as u32,
        y as u32,
        Rgba([
            mix(color[0], dst[0]),
            mix(color[1], dst[1]),
            mix(color[2], dst[2]),
            255,
        ]),
    );
}

/// 计算导出图像尺寸。
///
/// `scale` 是每个字符单元的像素边长倍率（`cell_w = 8 * scale` 之类）。
pub fn image_size(cols: u16, rows: u16, cell_w: u32, cell_h: u32) -> (u32, u32) {
    (cols as u32 * cell_w, rows as u32 * cell_h)
}

/// 校验并保存 PNG。
pub fn save_png(img: &RgbaImage, path: &std::path::Path) -> Result<()> {
    img.save(path)
        .with_context(|| format!("写入 PNG 失败: {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::braille::dots_to_braille;

    fn raster() -> Rasterizer {
        Rasterizer::new(8, 16)
    }

    #[test]
    fn image_size_is_product_of_grid() {
        assert_eq!(image_size(80, 24, 8, 16), (640, 384));
    }

    #[test]
    fn braille_dot_renders_without_font() {
        // 即使字体缺失，Braille 也必须能画
        let r = Rasterizer {
            font: None,
            fallback: None,
            font_path: None,
            fallback_path: None,
            cell_w: 8,
            cell_h: 16,
            font_size: 13.0,
        };
        let mut img = RgbaImage::new(8, 16);
        let mut dots = [[false; 4]; 2];
        dots[0][0] = true;
        let bits = dots_to_braille(&dots);
        r.draw_braille(&mut img, 0, 0, bits, Rgba([255, 255, 255, 255]));
        // 左上子格中心附近应被点亮
        let p = img.get_pixel(2, 2);
        assert!(p[0] > 100, "左上点未绘制: {p:?}");
    }

    #[test]
    fn all_eight_braille_dots_render() {
        let r = raster();
        let mut img = RgbaImage::new(8, 16);
        r.draw_braille(&mut img, 0, 0, 0xFF, Rgba([255, 255, 255, 255]));
        let lit = img.pixels().filter(|p| p[0] > 100).count();
        assert!(lit >= 8, "8 个点应产生至少 8 个亮像素，实际 {lit}");
    }

    #[test]
    fn braille_with_zero_bits_draws_nothing() {
        let r = raster();
        let mut img = RgbaImage::new(8, 16);
        r.draw_braille(&mut img, 0, 0, 0, Rgba([255, 255, 255, 255]));
        assert!(img.pixels().all(|p| p[0] == 0));
    }

    #[test]
    fn block_chars_fill_expected_regions() {
        let r = raster();
        let mut img = RgbaImage::new(8, 16);
        r.draw_char(&mut img, 0, 0, '█', Rgba([255, 0, 0, 255]));
        assert!(img.pixels().all(|p| p[0] == 255), "全块应填满");

        let mut img2 = RgbaImage::new(8, 16);
        r.draw_char(&mut img2, 0, 0, '▀', Rgba([255, 0, 0, 255]));
        assert!(img2.get_pixel(4, 2)[0] > 200, "上半块应点亮上半部");
        assert!(img2.get_pixel(4, 12)[0] == 0, "下半块应留空");
    }

    #[test]
    fn render_produces_expected_dimensions() {
        let r = raster();
        let area = Rect::new(0, 0, 4, 2);
        let buf = Buffer::empty(area);
        let img = r.render(&buf, area);
        assert_eq!(img.width(), 32);
        assert_eq!(img.height(), 32);
    }

    #[test]
    fn render_fills_background() {
        let r = raster();
        let area = Rect::new(0, 0, 2, 1);
        let mut buf = Buffer::empty(area);
        buf[(0, 0)].set_bg(col::CYAN);
        let img = r.render(&buf, area);
        let p = img.get_pixel(1, 1);
        assert_eq!((p[0], p[1], p[2]), (0, 255, 255));
    }

    #[test]
    fn render_draws_braille_cells() {
        let r = raster();
        let area = Rect::new(0, 0, 1, 1);
        let mut buf = Buffer::empty(area);
        buf[(0, 0)].set_char('\u{28FF}');
        buf[(0, 0)].set_fg(col::WHITE);
        let img = r.render(&buf, area);
        let lit = img.pixels().filter(|p| p[0] > 100).count();
        assert!(lit > 4, "满点 Braille 应产生多个亮像素，实际 {lit}");
    }

    #[test]
    fn render_handles_space_cells() {
        let r = raster();
        let area = Rect::new(0, 0, 1, 1);
        let buf = Buffer::empty(area);
        let img = r.render(&buf, area);
        // 空格 + 默认背景 → 全黑
        assert!(img.pixels().all(|p| p[0] == 0 && p[1] == 0 && p[2] == 0));
    }

    #[test]
    fn blend_pixel_respects_bounds() {
        let mut img = RgbaImage::new(2, 2);
        blend_pixel(&mut img, -1, -1, Rgba([255, 255, 255, 255]), 255);
        blend_pixel(&mut img, 100, 100, Rgba([255, 255, 255, 255]), 255);
        // 不应 panic
    }

    #[test]
    fn blend_pixel_mixes_colors() {
        let mut img = RgbaImage::new(1, 1);
        img.put_pixel(0, 0, Rgba([0, 0, 0, 255]));
        blend_pixel(&mut img, 0, 0, Rgba([255, 255, 255, 255]), 128);
        let p = img.get_pixel(0, 0);
        assert!((100..160).contains(&p[0]), "混合结果 {} 不合理", p[0]);
    }

    #[test]
    fn draw_disc_is_round() {
        let mut img = RgbaImage::new(20, 20);
        draw_disc(&mut img, 10.0, 10.0, 5.0, Rgba([255, 255, 255, 255]));
        assert!(img.get_pixel(10, 10)[0] > 200, "圆心应实心");
        assert!(img.get_pixel(10, 0)[0] == 0, "远处应空白");
    }

    #[test]
    fn draw_disc_with_zero_radius_is_noop() {
        let mut img = RgbaImage::new(4, 4);
        draw_disc(&mut img, 2.0, 2.0, 0.0, Rgba([255, 255, 255, 255]));
        assert!(img.pixels().all(|p| p[0] == 0));
    }

    #[test]
    fn fill_cell_clips_to_image() {
        let mut img = RgbaImage::new(4, 4);
        fill_cell(&mut img, 2, 2, 100, 100, Rgba([1, 2, 3, 255]));
        assert_eq!(img.get_pixel(3, 3)[0], 1);
        assert_eq!(img.get_pixel(0, 0)[0], 0);
    }

    #[test]
    fn color_to_rgba_matches_palette() {
        assert_eq!(color_to_rgba(col::CYAN, 255), Rgba([0, 255, 255, 255]));
        assert_eq!(
            color_to_rgba(Color::Rgb(1, 2, 3), 128),
            Rgba([1, 2, 3, 128])
        );
    }

    #[test]
    fn save_png_writes_a_file() {
        let img = RgbaImage::new(4, 4);
        let p = std::env::temp_dir().join("wem_raster_test.png");
        save_png(&img, &p).unwrap();
        assert!(p.exists());
        let back = image::open(&p).unwrap();
        assert_eq!(back.width(), 4);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn font_probe_never_panics() {
        // 无论机器上有没有字体，探测都该安全返回
        let _ = load_font();
        let r = Rasterizer::new(8, 16);
        assert!(r.cell_w == 8 && r.cell_h == 16);
    }
}

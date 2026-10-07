//! 图层合成。
//!
//! 场景把不同内容画到不同图层，最后统一合成 ——
//! 这样「背景被暗角压暗、主体不受影响」这类需求不需要场景自己关心。
//!
//! 层序（下 → 上）：
//! `BG` 背景 → `SUBJECT` 主体 → `PARTICLES` 粒子 → `LYRICS` 歌词 → `UI` 状态栏

use ratatui::style::Color;

use crate::config::RenderMode;
use crate::render::{CharCanvas, Pixel};

/// 背景层
pub const LAYER_BG: usize = 0;
/// 主体层（几何图形 / 心形 / 频谱）
pub const LAYER_SUBJECT: usize = 1;
/// 粒子层
pub const LAYER_PARTICLES: usize = 2;
/// 歌词层
pub const LAYER_LYRICS: usize = 3;
/// UI 层（进度条 / 调试面板）
pub const LAYER_UI: usize = 4;
/// 图层总数
pub const LAYER_COUNT: usize = 5;

/// 混合模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendMode {
    /// 普通叠加：上层不透明处覆盖下层
    Normal,
    /// 加法：亮度相加（发光）
    Add,
    /// 正片叠底：相乘（压暗、染色）
    Multiply,
    /// 滤色：反相相乘（提亮、雾化）
    Screen,
}

/// 单个图层。
pub struct Layer {
    /// 子像素画布
    pub canvas: CharCanvas,
    /// 整体不透明度 0..1
    pub alpha: f32,
    /// 混合模式
    pub blend: BlendMode,
    /// 是否参与合成
    pub visible: bool,
}

impl Layer {
    /// 新建。
    pub fn new(cols: u16, rows: u16, mode: RenderMode) -> Self {
        Self {
            canvas: CharCanvas::new(cols, rows, mode),
            alpha: 1.0,
            blend: BlendMode::Normal,
            visible: true,
        }
    }

    /// 清空内容（保留设置）。
    pub fn clear(&mut self) {
        self.canvas.clear();
    }
}

/// 图层栈。
pub struct LayerStack {
    layers: Vec<Layer>,
    /// 合成用的临时缓冲（避免每帧分配）
    scratch: Vec<Pixel>,
}

impl LayerStack {
    /// 新建 `LAYER_COUNT` 个图层。
    pub fn new(cols: u16, rows: u16, mode: RenderMode) -> Self {
        let layers = (0..LAYER_COUNT)
            .map(|_| Layer::new(cols, rows, mode))
            .collect();
        Self {
            layers,
            scratch: Vec::new(),
        }
    }

    /// 图层数量。
    pub fn len(&self) -> usize {
        self.layers.len()
    }

    /// 是否为空（永远是 false，仅为 clippy 完整性）。
    pub fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }

    /// 按序号取图层。
    pub fn layer(&self, i: usize) -> &Layer {
        &self.layers[i.min(self.layers.len() - 1)]
    }

    /// 按序号取可变图层。
    pub fn layer_mut(&mut self, i: usize) -> &mut Layer {
        let n = self.layers.len();
        &mut self.layers[i.min(n - 1)]
    }

    /// 便捷：直接取某层的画布。
    pub fn canvas_mut(&mut self, i: usize) -> &mut CharCanvas {
        &mut self.layer_mut(i).canvas
    }

    /// 便捷：直接取某层的画布（只读）。
    pub fn canvas(&self, i: usize) -> &CharCanvas {
        &self.layer(i).canvas
    }

    /// 设定某层不透明度。
    pub fn set_alpha(&mut self, i: usize, alpha: f32) {
        self.layer_mut(i).alpha = alpha.clamp(0.0, 1.0);
    }

    /// 设定某层混合模式。
    pub fn set_blend(&mut self, i: usize, mode: BlendMode) {
        self.layer_mut(i).blend = mode;
    }

    /// 设定某层可见性。
    pub fn set_visible(&mut self, i: usize, visible: bool) {
        self.layer_mut(i).visible = visible;
    }

    /// 清空所有图层内容。
    pub fn clear_all(&mut self) {
        for l in self.layers.iter_mut() {
            l.clear();
        }
    }

    /// 用某个颜色铺满某一层。
    pub fn fill_layer(&mut self, i: usize, color: Color, w: f32) {
        self.canvas_mut(i).fill(color, w);
    }

    /// 合成到 `out`。`out` 会先被清空。
    pub fn composite(&mut self, out: &mut CharCanvas) {
        out.clear();
        // 目标缓冲：先在 scratch 里累积，最后一次性写回
        let n = out.sw as usize * out.sh as usize;
        self.scratch.clear();
        self.scratch.resize(n, Pixel::default());

        for l in self.layers.iter() {
            if !l.visible || l.alpha <= 0.0 {
                continue;
            }
            let src = l.canvas.pixels();
            let m = src.len().min(self.scratch.len());
            for i in 0..m {
                let s = src[i];
                if s.w <= 0.0 {
                    continue;
                }
                let d = &mut self.scratch[i];
                let a = (s.w * l.alpha).clamp(0.0, 1.0);
                match l.blend {
                    BlendMode::Normal => {
                        // 上层覆盖下层：按 alpha 混合
                        d.r = d.r * (1.0 - a) + s.r * a;
                        d.g = d.g * (1.0 - a) + s.g * a;
                        d.b = d.b * (1.0 - a) + s.b * a;
                        d.w = (d.w + a * (1.0 - d.w)).min(1.0);
                    }
                    BlendMode::Add => {
                        d.r = (d.r + s.r * a).min(1.0);
                        d.g = (d.g + s.g * a).min(1.0);
                        d.b = (d.b + s.b * a).min(1.0);
                        d.w = (d.w + a).min(1.0);
                    }
                    BlendMode::Multiply => {
                        // 下层为空的区域，乘法会把画面抹黑，所以用 alpha 做保护
                        d.r = d.r * (1.0 - a) + (d.r * s.r) * a;
                        d.g = d.g * (1.0 - a) + (d.g * s.g) * a;
                        d.b = d.b * (1.0 - a) + (d.b * s.b) * a;
                        d.w = (d.w + a * (1.0 - d.w)).min(1.0);
                    }
                    BlendMode::Screen => {
                        let sc = |x: f32, y: f32| 1.0 - (1.0 - x) * (1.0 - y);
                        d.r = d.r * (1.0 - a) + sc(d.r, s.r) * a;
                        d.g = d.g * (1.0 - a) + sc(d.g, s.g) * a;
                        d.b = d.b * (1.0 - a) + sc(d.b, s.b) * a;
                        d.w = (d.w + a * (1.0 - d.w)).min(1.0);
                    }
                }
            }
        }

        let dst = out.pixels_mut();
        dst.copy_from_slice(&self.scratch[..dst.len()]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::color as col;

    fn stack() -> LayerStack {
        LayerStack::new(10, 5, RenderMode::Braille)
    }

    #[test]
    fn new_stack_has_all_layers() {
        let s = stack();
        assert_eq!(s.len(), LAYER_COUNT);
        assert!(!s.is_empty());
    }

    #[test]
    fn composite_of_empty_stack_is_blank() {
        let mut s = stack();
        let mut out = CharCanvas::new(10, 5, RenderMode::Braille);
        s.composite(&mut out);
        assert!(out.pixels().iter().all(|p| p.is_empty()));
    }

    #[test]
    fn upper_layer_covers_lower() {
        let mut s = stack();
        s.canvas_mut(LAYER_BG).fill(col::BLACK, 1.0);
        s.canvas_mut(LAYER_SUBJECT).fill(col::WHITE, 1.0);
        let mut out = CharCanvas::new(10, 5, RenderMode::Braille);
        s.composite(&mut out);
        let p = out.pixel(5, 5);
        assert!(p.r > 0.9, "上层白色应覆盖下层黑色: {p:?}");
    }

    #[test]
    fn alpha_zero_layer_is_invisible() {
        let mut s = stack();
        s.canvas_mut(LAYER_BG).fill(col::BLACK, 1.0);
        s.canvas_mut(LAYER_SUBJECT).fill(col::WHITE, 1.0);
        s.set_alpha(LAYER_SUBJECT, 0.0);
        let mut out = CharCanvas::new(10, 5, RenderMode::Braille);
        s.composite(&mut out);
        assert!(out.pixel(5, 5).r < 0.1, "alpha=0 的层不该出现");
    }

    #[test]
    fn half_alpha_blends_two_layers() {
        let mut s = stack();
        s.canvas_mut(LAYER_BG).fill(col::BLACK, 1.0);
        s.canvas_mut(LAYER_SUBJECT).fill(col::WHITE, 1.0);
        s.set_alpha(LAYER_SUBJECT, 0.5);
        let mut out = CharCanvas::new(10, 5, RenderMode::Braille);
        s.composite(&mut out);
        let v = out.pixel(5, 5).r;
        assert!((0.3..0.7).contains(&v), "半透明混合结果 {v} 不在中间");
    }

    #[test]
    fn invisible_layer_is_skipped() {
        let mut s = stack();
        s.canvas_mut(LAYER_BG).fill(col::WHITE, 1.0);
        s.set_visible(LAYER_BG, false);
        let mut out = CharCanvas::new(10, 5, RenderMode::Braille);
        s.composite(&mut out);
        assert!(out.pixel(0, 0).is_empty());
    }

    #[test]
    fn add_blend_brightens() {
        let mut s = stack();
        s.canvas_mut(LAYER_BG).fill(Color::Rgb(100, 0, 0), 1.0);
        s.canvas_mut(LAYER_SUBJECT).fill(Color::Rgb(100, 0, 0), 1.0);
        s.set_blend(LAYER_SUBJECT, BlendMode::Add);
        let mut out = CharCanvas::new(10, 5, RenderMode::Braille);
        s.composite(&mut out);
        let r = out.pixel(0, 0).r;
        assert!(r > 0.6, "加法混合应变亮，实际 {r}");
    }

    #[test]
    fn screen_blend_is_lighter_than_multiply() {
        let mk = |mode: BlendMode| {
            let mut s = stack();
            s.canvas_mut(LAYER_BG).fill(Color::Rgb(128, 128, 128), 1.0);
            s.canvas_mut(LAYER_SUBJECT)
                .fill(Color::Rgb(128, 128, 128), 1.0);
            s.set_blend(LAYER_SUBJECT, mode);
            let mut out = CharCanvas::new(10, 5, RenderMode::Braille);
            s.composite(&mut out);
            out.pixel(0, 0).r
        };
        assert!(mk(BlendMode::Screen) > mk(BlendMode::Multiply));
    }

    #[test]
    fn clear_all_empties_every_layer() {
        let mut s = stack();
        for i in 0..LAYER_COUNT {
            s.canvas_mut(i).fill(col::WHITE, 1.0);
        }
        s.clear_all();
        for i in 0..LAYER_COUNT {
            assert!(s.canvas(i).pixels().iter().all(|p| p.is_empty()));
        }
    }

    #[test]
    fn layer_index_is_clamped() {
        let mut s = stack();
        // 越界索引不应 panic
        s.set_alpha(999, 0.5);
        assert_eq!(s.layer(999).alpha, 0.5);
    }

    #[test]
    fn composite_result_stays_in_unit_range() {
        let mut s = stack();
        for i in 0..LAYER_COUNT {
            s.canvas_mut(i).fill(col::WHITE, 1.0);
            s.set_blend(i, BlendMode::Add);
        }
        let mut out = CharCanvas::new(10, 5, RenderMode::Braille);
        s.composite(&mut out);
        for p in out.pixels() {
            assert!(p.r <= 1.0 && p.g <= 1.0 && p.b <= 1.0 && p.w <= 1.0);
        }
    }

    #[test]
    fn composite_is_repeatable() {
        let mut s = stack();
        s.canvas_mut(LAYER_BG).fill(Color::Rgb(10, 20, 30), 0.8);
        s.canvas_mut(LAYER_SUBJECT)
            .fill(Color::Rgb(200, 100, 50), 0.6);
        let mut a = CharCanvas::new(10, 5, RenderMode::Braille);
        let mut b = CharCanvas::new(10, 5, RenderMode::Braille);
        s.composite(&mut a);
        s.composite(&mut b);
        for (x, y) in a.pixels().iter().zip(b.pixels()) {
            assert_eq!(x.r, y.r);
            assert_eq!(x.w, y.w);
        }
    }

    #[test]
    fn layer_canvas_dimensions_match() {
        let s = stack();
        assert_eq!(s.canvas(LAYER_BG).cols, 10);
        assert_eq!(s.canvas(LAYER_BG).rows, 5);
    }
}

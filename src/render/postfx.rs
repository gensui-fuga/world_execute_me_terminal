//! 后处理栈：Bloom → 色差 → 扫描线 → 抖动 → 暗角 → 故障。
//!
//! **重要**：tachyonfx 没有内置 glitch / chromatic aberration / scanline / shake，
//! 这些效果全部在这里手写，直接操作 [`CharCanvas`] 的子像素缓冲。
//! tachyonfx 只用来做字符单元的淡入淡出/溶解这类它擅长的过渡（见 `effects`）。
//!
//! 顺序有讲究：
//! 1. Bloom 先做，让后续色差能吃到光晕；
//! 2. 色差在子像素级拆 RGB 通道；
//! 3. 扫描线压暗模拟 CRT；
//! 4. 抖动做整体位移（模拟相机震动）；
//! 5. 暗角压暗四周，把注意力收回画面中心；
//! 6. 故障最后做，因为它会破坏结构（逐行位移 + 块替换）。

use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};

use crate::render::{CharCanvas, Pixel};

/// 后处理参数。每个值都是「强度」，0 表示关闭。
#[derive(Debug, Clone, Copy, Default)]
pub struct PostFx {
    /// 泛光强度
    pub bloom: f32,
    /// 色差强度（RGB 通道横向分离）
    pub aberration: f32,
    /// 扫描线强度
    pub scanline: f32,
    /// 相机抖动强度
    pub shake: f32,
    /// 暗角强度
    pub vignette: f32,
    /// 故障强度
    pub glitch: f32,
}

impl PostFx {
    /// 全部关闭。
    pub fn off() -> Self {
        Self::default()
    }

    /// 是否完全没有效果（可以整体跳过）。
    pub fn is_off(&self) -> bool {
        self.bloom <= 1e-4
            && self.aberration <= 1e-4
            && self.scanline <= 1e-4
            && self.shake <= 1e-4
            && self.vignette <= 1e-4
            && self.glitch <= 1e-4
    }

    /// 按音乐特征缩放（让画面「跟着歌动」）。
    pub fn scaled(mut self, k: f32) -> Self {
        let k = k.clamp(0.0, 2.0);
        self.bloom *= k;
        self.aberration *= k;
        self.shake *= k;
        self.glitch *= k;
        self
    }
}

/// 后处理栈。持有 RNG，保证同一 `frame_index` 下结果可复现。
pub struct PostFxStack {
    /// 当前参数
    pub params: PostFx,
    rng: SmallRng,
    seed: u64,
    /// 复用的源缓冲
    src: Vec<Pixel>,
    /// 逐行位移表
    row_offsets: Vec<i32>,
    /// 残影缓冲：上一帧的最终画面
    prev: Vec<Pixel>,
    /// `prev` 实际覆盖的尺寸；与画布不符时重新分配
    prev_dims: (u16, u16),
    /// 残影强度（0 = 关闭）。由 `postfx_for` 每帧写入。
    pub trail: f32,
    /// 残影衰减：每帧对残影缓冲自身乘一次，形成有限长的拖尾
    pub trail_decay: f32,
}

impl PostFxStack {
    /// 新建。
    pub fn new(seed: u64) -> Self {
        Self {
            params: PostFx::off(),
            rng: SmallRng::seed_from_u64(seed),
            seed,
            src: Vec::new(),
            row_offsets: Vec::new(),
            prev: Vec::new(),
            prev_dims: (0, 0),
            trail: 0.0,
            trail_decay: 0.82,
        }
    }

    /// 重置随机状态（seek 或重播时调用，保证确定性）。
    pub fn reseed(&mut self, seed: u64) {
        self.seed = seed;
        self.rng = SmallRng::seed_from_u64(seed);
    }

    /// 随机种子。
    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// 施加整条后处理链。
    pub fn apply(&mut self, canvas: &mut CharCanvas) {
        // 残影在**所有效果之前**：它要叠的是「上一帧最终长什么样」，
        // 放在后面的话扫描线/暗角会被反复叠加，暗部越来越黑。
        if self.trail > 1e-4 {
            self.trail(canvas);
        } else {
            // 关闭时仍维护缓冲，避免重新打开时闪出陈旧内容
            let n = (canvas.sw as usize) * (canvas.sh as usize);
            if self.prev_dims != (canvas.sw, canvas.sh) || self.prev.len() != n {
                self.prev.clear();
                self.prev.resize(n, Pixel::default());
                self.prev_dims = (canvas.sw, canvas.sh);
            }
            self.prev.copy_from_slice(canvas.pixels());
        }

        if self.params.is_off() {
            return;
        }
        if self.params.bloom > 1e-4 {
            self.bloom(canvas);
        }
        if self.params.aberration > 1e-4 {
            self.aberration(canvas);
        }
        if self.params.scanline > 1e-4 {
            self.scanline(canvas);
        }
        if self.params.shake > 1e-4 {
            self.shake(canvas);
        }
        if self.params.vignette > 1e-4 {
            self.vignette(canvas);
        }
        if self.params.glitch > 1e-4 {
            self.glitch(canvas);
        }
    }

    /// 残影：把上一帧按 `trail` 衰减后叠加到当前帧。
    ///
    /// 这是让画面「信息量翻倍」最省力的手段 —— 运动物体自动拖出轨迹，
    /// 静止部分被反复加深，观感上像是满屏都在动，而成本只是一次线性扫描。
    ///
    /// 两个细节：
    /// - **取亮者（max）而不是相加**：相加会让重叠区域迅速饱和成白块，
    ///   取亮者则保持色彩，只延长残像。
    /// - 叠加后写回 `prev`，并**让 `prev` 自身缓慢衰减**，
    ///   否则残影会无限累积，几秒后整幅画面糊成一片亮斑。
    pub fn trail(&mut self, canvas: &mut CharCanvas) {
        let sw = canvas.sw as usize;
        let sh = canvas.sh as usize;
        if sw == 0 || sh == 0 {
            return;
        }
        let n = sw * sh;
        let dims = (canvas.sw, canvas.sh);

        // 尺寸变了（切终端大小 / 换导出分辨率）就丢弃旧残影
        if self.prev_dims != dims || self.prev.len() != n {
            self.prev.clear();
            self.prev.resize(n, Pixel::default());
            self.prev_dims = dims;
        }

        let amount = self.trail.clamp(0.0, 0.95);
        if amount <= 1e-4 {
            // 残影关闭时也要维护缓冲，否则重新打开会闪出旧内容
            self.prev.copy_from_slice(canvas.pixels());
            return;
        }

        let decay = self.trail_decay.clamp(0.0, 1.0);
        let pix = canvas.pixels_mut();
        for i in 0..n {
            let p = &mut pix[i];
            let prev = &mut self.prev[i];

            // 残影自身先衰减一格，形成有限长度的拖尾
            prev.r *= decay;
            prev.g *= decay;
            prev.b *= decay;
            prev.w *= decay;

            // 上一帧的残像按强度混入当前像素
            let pr = prev.r * amount;
            let pg = prev.g * amount;
            let pb = prev.b * amount;
            let pw = prev.w * amount;

            if pw > p.w {
                // 残像更强：让它接管覆盖率，同时保留当前帧的颜色倾向
                p.w = pw.min(1.0);
            }
            if pr > p.r {
                p.r = pr;
            }
            if pg > p.g {
                p.g = pg;
            }
            if pb > p.b {
                p.b = pb;
            }

            // 记录本帧结果作为下一帧的残影
            *prev = *p;
        }
    }

    /// 清空残影缓冲（seek / 重播 / 切场景时调用）。
    pub fn clear_trail(&mut self) {
        for p in self.prev.iter_mut() {
            *p = Pixel::default();
        }
    }

    /// 泛光：3×3 盒式模糊后加法叠加回原图。
    pub fn bloom(&mut self, canvas: &mut CharCanvas) {
        let amount = self.params.bloom.clamp(0.0, 1.0);
        let sw = canvas.sw as usize;
        let sh = canvas.sh as usize;
        if sw < 3 || sh < 3 {
            return;
        }
        // 源 = 当前内容（复制到内部缓冲）
        self.src.clear();
        self.src.extend_from_slice(canvas.pixels());

        let pix = canvas.pixels_mut();
        for y in 1..sh - 1 {
            for x in 1..sw - 1 {
                let mut r = 0.0f32;
                let mut g = 0.0f32;
                let mut b = 0.0f32;
                for dy in 0..3usize {
                    for dx in 0..3usize {
                        let p = self.src[(y + dy - 1) * sw + (x + dx - 1)];
                        r += p.r;
                        g += p.g;
                        b += p.b;
                    }
                }
                let inv = 1.0 / 9.0;
                let i = y * sw + x;
                pix[i].r = (pix[i].r + r * inv * amount).min(1.0);
                pix[i].g = (pix[i].g + g * inv * amount).min(1.0);
                pix[i].b = (pix[i].b + b * inv * amount).min(1.0);
                // 光晕也要有覆盖度，否则亮但没点的子像素依然不显示
                let halo = (r + g + b) * inv / 3.0;
                pix[i].w = (pix[i].w + halo * amount * 0.8).min(1.0);
            }
        }
    }

    /// 色差：R 通道右移、B 通道左移，G 保持。
    ///
    /// 偏移量必须限制在 **1~2 个子像素**：字符画布每单元只有 2×4 个子像素，
    /// 偏 3 个以上就会把 R/B 搬到完全不相干的图形上，颜色彻底错乱
    /// （青+红会混出黄绿，画面变成一锅粥）。
    pub fn aberration(&mut self, canvas: &mut CharCanvas) {
        let amount = self.params.aberration.clamp(0.0, 2.0);
        let shift = (amount * 1.5).round().clamp(0.0, 3.0) as i32;
        if shift == 0 {
            return;
        }
        let sw = canvas.sw as usize;
        let sh = canvas.sh as usize;
        self.src.clear();
        self.src.extend_from_slice(canvas.pixels());

        let pix = canvas.pixels_mut();
        for y in 0..sh {
            for x in 0..sw {
                let xr = (x as i32 - shift).clamp(0, sw as i32 - 1) as usize;
                let xb = (x as i32 + shift).clamp(0, sw as i32 - 1) as usize;
                let i = y * sw + x;
                let rp = self.src[y * sw + xr];
                let bp = self.src[y * sw + xb];
                // 保持总覆盖度不变，只搬通道
                pix[i].r = rp.r;
                pix[i].b = bp.b;
            }
        }
    }

    /// 扫描线：每 3 个字符行压暗中间那一行。
    ///
    /// **只压颜色，不动覆盖率** —— 降低 w 会让细线直接跌破点亮阈值而整行消失，
    /// 看起来像画面被撕掉几条。
    pub fn scanline(&mut self, canvas: &mut CharCanvas) {
        let amount = self.params.scanline.clamp(0.0, 1.0);
        let sub_y = canvas.sub_y.max(1) as usize;
        let sw = canvas.sw as usize;
        let sh = canvas.sh as usize;
        // 压暗系数保底 0.62：扫描线只是纹理，不该把整行拖黑
        let k = 1.0 - amount * 0.38;
        let pix = canvas.pixels_mut();
        for y in 0..sh {
            if (y / sub_y) % 3 != 1 {
                continue;
            }
            for x in 0..sw {
                let p = &mut pix[y * sw + x];
                p.r *= k;
                p.g *= k;
                p.b *= k;
            }
        }
    }

    /// 相机抖动：整体平移若干子像素。
    pub fn shake(&mut self, canvas: &mut CharCanvas) {
        let amount = self.params.shake.clamp(0.0, 2.0);
        let dx = self.rng.gen_range(-1.0f32..=1.0) * amount * 3.0;
        let dy = self.rng.gen_range(-1.0f32..=1.0) * amount * 2.0;
        canvas.translate(dx.round() as i32, dy.round() as i32);
    }

    /// 暗角：按到中心的归一化距离平方压暗。
    pub fn vignette(&mut self, canvas: &mut CharCanvas) {
        let amount = self.params.vignette.clamp(0.0, 1.0);
        let sw = canvas.sw as usize;
        let sh = canvas.sh as usize;
        if sw == 0 || sh == 0 {
            return;
        }
        let cx = sw as f32 / 2.0;
        let cy = sh as f32 / 2.0;
        let max_d = (cx * cx + cy * cy).sqrt().max(1.0);
        let pix = canvas.pixels_mut();
        for y in 0..sh {
            for x in 0..sw {
                let dx = x as f32 - cx;
                let dy = y as f32 - cy;
                let d = ((dx * dx + dy * dy).sqrt() / max_d).powi(2);
                // 压暗上限 55%（k 下限 0.45）：暗角是用来收拢视线的，
                // 压到底的话四角会变成纯黑死区，整幅画面视觉重心只剩中间一小块。
                let k = (1.0 - amount * d * 0.55).clamp(0.45, 1.0);
                let p = &mut pix[y * sw + x];
                // 同 scanline：只压颜色，压 w 会让边缘的细线整段消失
                p.r *= k;
                p.g *= k;
                p.b *= k;
            }
        }
    }

    /// 故障：逐行水平位移 + 随机块替换 + 通道错位。
    ///
    /// 只在节拍网格上调用（由场景决定），否则会一直抖，廉价感极强。
    pub fn glitch(&mut self, canvas: &mut CharCanvas) {
        let amount = self.params.glitch.clamp(0.0, 2.0);
        let sw = canvas.sw as usize;
        let sh = canvas.sh as usize;
        if sw < 4 || sh < 4 {
            return;
        }

        // 1) 逐行位移
        self.row_offsets.clear();
        self.row_offsets.resize(sh, 0);
        let mut any = false;
        let max_shift = (amount * 12.0).round().max(1.0) as i32;
        for y in 0..sh {
            if self.rng.gen::<f32>() < amount * 0.15 {
                let s = self.rng.gen_range(1..=max_shift);
                self.row_offsets[y] = if self.rng.gen_bool(0.5) { s } else { -s };
                any = true;
            }
        }
        if any {
            canvas.shift_rows(&self.row_offsets);
        }

        // 2) 随机块替换：清空 / 填亮 / 复制相邻行
        let blocks = (amount * 5.0).round() as usize;
        for _ in 0..blocks {
            let bw = self.rng.gen_range(1..=(sw / 5).max(2));
            let bh = self.rng.gen_range(1..=(sh / 10).max(2));
            let bx = self.rng.gen_range(0..sw.saturating_sub(bw).max(1));
            let by = self.rng.gen_range(0..sh.saturating_sub(bh).max(1));
            let mode = self.rng.gen_range(0..3u8);
            let bright = self.rng.gen_range(0.4f32..1.0);
            match mode {
                0 => {
                    // 清空
                    let pix = canvas.pixels_mut();
                    for y in by..(by + bh).min(sh) {
                        for x in bx..(bx + bw).min(sw) {
                            pix[y * sw + x] = Pixel::default();
                        }
                    }
                }
                1 => {
                    // 填成随机亮色（通道错位）
                    let rr = self.rng.gen_range(0.3f32..1.0);
                    let gg = self.rng.gen_range(0.0f32..0.6);
                    let bb = self.rng.gen_range(0.3f32..1.0);
                    let pix = canvas.pixels_mut();
                    for y in by..(by + bh).min(sh) {
                        for x in bx..(bx + bw).min(sw) {
                            pix[y * sw + x] = Pixel {
                                r: rr * bright,
                                g: gg * bright,
                                b: bb * bright,
                                w: bright,
                            };
                        }
                    }
                }
                _ => {
                    // 复制上方若干行（撕裂感）
                    let src_y = by.saturating_sub(self.rng.gen_range(1..8));
                    let pix = canvas.pixels_mut();
                    for y in by..(by + bh).min(sh) {
                        for x in bx..(bx + bw).min(sw) {
                            let v = pix[src_y * sw + x];
                            pix[y * sw + x] = v;
                        }
                    }
                }
            }
        }
    }

    /// 生成一段「撕裂」的逐行位移表（供场景在切场时使用）。
    pub fn tear_offsets(&mut self, sh: usize, amount: f32) -> Vec<i32> {
        let amount = amount.clamp(0.0, 2.0);
        let mut v = vec![0i32; sh];
        let max_shift = (amount * 16.0).round().max(1.0) as i32;
        for o in v.iter_mut() {
            if self.rng.gen::<f32>() < amount * 0.35 {
                let s = self.rng.gen_range(1..=max_shift);
                *o = if self.rng.gen_bool(0.5) { s } else { -s };
            }
        }
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RenderMode;
    use crate::render::color as col;

    fn canvas() -> CharCanvas {
        CharCanvas::new(20, 10, RenderMode::Braille)
    }

    fn lit(c: &CharCanvas) -> usize {
        c.pixels().iter().filter(|p| !p.is_empty()).count()
    }

    #[test]
    fn off_stack_is_noop() {
        let mut c = canvas();
        c.set(5.0, 5.0, col::WHITE, 1.0);
        let before = c.pixels().to_vec();
        let mut fx = PostFxStack::new(1);
        fx.params = PostFx::off();
        fx.apply(&mut c);
        for (a, b) in before.iter().zip(c.pixels()) {
            assert_eq!(a.w, b.w);
            assert_eq!(a.r, b.r);
        }
    }

    #[test]
    fn is_off_detects_all_zero() {
        assert!(PostFx::off().is_off());
        let p = PostFx {
            bloom: 0.5,
            ..Default::default()
        };
        assert!(!p.is_off());
    }

    #[test]
    fn scaled_multiplies_selected_fields() {
        let p = PostFx {
            bloom: 1.0,
            aberration: 1.0,
            scanline: 1.0,
            shake: 1.0,
            vignette: 1.0,
            glitch: 1.0,
        }
        .scaled(0.5);
        assert!((p.bloom - 0.5).abs() < 1e-6);
        assert!((p.aberration - 0.5).abs() < 1e-6);
        // scanline 与 vignette 不随音乐缩放
        assert!((p.scanline - 1.0).abs() < 1e-6);
        assert!((p.vignette - 1.0).abs() < 1e-6);
    }

    #[test]
    fn scaled_clamps_extremes() {
        let p = PostFx {
            bloom: 1.0,
            ..Default::default()
        }
        .scaled(100.0);
        assert!(p.bloom <= 2.0);
    }

    #[test]
    fn bloom_spreads_light_to_neighbors() {
        let mut c = canvas();
        c.set(10.0, 10.0, col::WHITE, 1.0);
        let mut fx = PostFxStack::new(1);
        fx.params.bloom = 1.0;
        fx.bloom(&mut c);
        // 邻居应该被点亮
        assert!(!c.pixel(11, 10).is_empty(), "Bloom 未扩散到邻像素");
        assert!(!c.pixel(10, 11).is_empty());
    }

    #[test]
    fn bloom_does_not_overflow_unit_range() {
        let mut c = canvas();
        c.fill(col::WHITE, 1.0);
        let mut fx = PostFxStack::new(1);
        fx.params.bloom = 1.0;
        for _ in 0..5 {
            fx.bloom(&mut c);
        }
        for p in c.pixels() {
            assert!(p.r <= 1.0 && p.g <= 1.0 && p.b <= 1.0 && p.w <= 1.0);
        }
    }

    #[test]
    fn aberration_separates_channels() {
        let mut c = canvas();
        // 一个纯红点 + 一个纯蓝点，隔开一段距离
        c.set(10.0, 10.0, col::Color::Rgb(255, 0, 0), 1.0);
        c.set(14.0, 10.0, col::Color::Rgb(0, 0, 255), 1.0);
        let mut fx = PostFxStack::new(1);
        fx.params.aberration = 1.0;
        fx.aberration(&mut c);
        // shift = round(1.0 * 1.5) = 2。R 从左边取 → 红点右移到 x=12；
        // B 从右边取 → 蓝点左移到 x=12，两者在中间相遇。
        let mid = c.pixel(12, 10);
        assert!(mid.r > 0.9, "R 通道未右移: {mid:?}");
        assert!(mid.b > 0.9, "B 通道未左移: {mid:?}");
    }

    #[test]
    fn aberration_keeps_green_in_place() {
        let mut c = canvas();
        c.set(10.0, 10.0, col::Color::Rgb(0, 255, 0), 1.0);
        let mut fx = PostFxStack::new(1);
        fx.params.aberration = 1.0;
        fx.aberration(&mut c);
        assert!(c.pixel(10, 10).g > 0.9, "G 通道不应移动");
    }

    #[test]
    fn scanline_darkens_every_third_row() {
        let mut c = canvas();
        c.fill(col::WHITE, 1.0);
        let mut fx = PostFxStack::new(1);
        fx.params.scanline = 1.0;
        fx.scanline(&mut c);
        // Braille: sub_y = 4，字符行 1 的中间子像素行被压暗。
        // 只压颜色、不动覆盖率 —— 压 w 会让细线整行跌破点亮阈值而消失。
        let dark = c.pixel(0, 4).r;
        let bright = c.pixel(0, 0).r;
        assert!(dark < bright, "扫描线未压暗: dark={dark} bright={bright}");
        assert!(c.pixel(0, 4).w > 0.9, "扫描线不应改动覆盖率");
    }

    #[test]
    fn shake_moves_content() {
        let mut c = canvas();
        c.set(10.0, 10.0, col::WHITE, 1.0);
        let mut fx = PostFxStack::new(1);
        fx.params.shake = 2.0;
        let mut moved = false;
        for _ in 0..10 {
            let mut cc = canvas();
            cc.set(10.0, 10.0, col::WHITE, 1.0);
            fx.shake(&mut cc);
            if cc.pixel(10, 10).is_empty() {
                moved = true;
                break;
            }
        }
        assert!(moved, "抖动未产生位移");
    }

    #[test]
    fn vignette_darkens_corners_more_than_center() {
        let mut c = canvas();
        c.fill(col::WHITE, 1.0);
        let mut fx = PostFxStack::new(1);
        fx.params.vignette = 1.0;
        fx.vignette(&mut c);
        let center = c.pixel(c.sw as i32 / 2, c.sh as i32 / 2).r;
        let corner = c.pixel(0, 0).r;
        assert!(
            corner < center,
            "暗角未生效: corner={corner} center={center}"
        );
        assert!(c.pixel(0, 0).w > 0.9, "暗角不应改动覆盖率");
    }

    #[test]
    fn glitch_perturbs_the_image() {
        let mut base = canvas();
        base.fill(col::WHITE, 1.0);
        let before = base.pixels().to_vec();
        let mut c = canvas();
        c.fill(col::WHITE, 1.0);
        let mut fx = PostFxStack::new(42);
        fx.params.glitch = 2.0;
        fx.glitch(&mut c);
        let changed = before
            .iter()
            .zip(c.pixels())
            .any(|(a, b)| (a.w - b.w).abs() > 1e-6 || (a.r - b.r).abs() > 1e-6);
        assert!(changed, "故障效果未改变画面");
    }

    #[test]
    fn glitch_is_deterministic_for_same_seed() {
        let mut a = canvas();
        a.fill(col::WHITE, 0.5);
        let mut b = canvas();
        b.fill(col::WHITE, 0.5);
        let mut fa = PostFxStack::new(7);
        let mut fb = PostFxStack::new(7);
        fa.params.glitch = 1.5;
        fb.params.glitch = 1.5;
        fa.glitch(&mut a);
        fb.glitch(&mut b);
        for (x, y) in a.pixels().iter().zip(b.pixels()) {
            assert_eq!(x.w, y.w);
            assert_eq!(x.r, y.r);
        }
    }

    #[test]
    fn reseed_changes_glitch_pattern() {
        let mut a = canvas();
        a.fill(col::WHITE, 0.5);
        let mut b = canvas();
        b.fill(col::WHITE, 0.5);
        let mut fa = PostFxStack::new(1);
        fa.params.glitch = 1.5;
        fa.glitch(&mut a);
        fa.reseed(999);
        fa.glitch(&mut b);
        let same = a
            .pixels()
            .iter()
            .zip(b.pixels())
            .all(|(x, y)| (x.w - y.w).abs() < 1e-9);
        assert!(!same, "重新播种后故障图案应改变");
    }

    #[test]
    fn full_chain_keeps_pixels_valid() {
        let mut c = canvas();
        c.fill(col::WHITE, 0.8);
        let mut fx = PostFxStack::new(3);
        fx.params = PostFx {
            bloom: 0.5,
            aberration: 1.0,
            scanline: 0.8,
            shake: 1.0,
            vignette: 0.6,
            glitch: 1.2,
        };
        for _ in 0..30 {
            fx.apply(&mut c);
            for p in c.pixels() {
                assert!(p.r.is_finite() && p.g.is_finite() && p.b.is_finite() && p.w.is_finite());
                assert!((0.0..=1.0).contains(&p.w));
            }
        }
    }

    #[test]
    fn tiny_canvas_does_not_panic() {
        let mut c = CharCanvas::new(1, 1, RenderMode::Braille);
        c.fill(col::WHITE, 1.0);
        let mut fx = PostFxStack::new(1);
        fx.params = PostFx {
            bloom: 1.0,
            aberration: 1.0,
            scanline: 1.0,
            shake: 1.0,
            vignette: 1.0,
            glitch: 1.0,
        };
        fx.apply(&mut c);
    }

    #[test]
    fn tear_offsets_length_matches() {
        let mut fx = PostFxStack::new(1);
        let v = fx.tear_offsets(50, 1.0);
        assert_eq!(v.len(), 50);
        assert!(v.iter().any(|&x| x != 0), "撕裂表应含非零位移");
    }

    #[test]
    fn lit_pixels_survive_full_chain() {
        let mut c = canvas();
        c.fill(col::WHITE, 1.0);
        let n0 = lit(&c);
        let mut fx = PostFxStack::new(5);
        fx.params = PostFx {
            bloom: 0.3,
            aberration: 0.5,
            scanline: 0.5,
            shake: 0.5,
            vignette: 0.3,
            glitch: 0.5,
        };
        fx.apply(&mut c);
        assert!(lit(&c) > n0 / 2, "后处理把画面吃光了");
    }
}

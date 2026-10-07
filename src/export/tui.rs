//! TUI 外壳：在光栅化完成的图像上直接画终端窗口框、标题栏与歌词条。
//!
//! 逐条对照参考实现（`MisakaZentai/world-execute-me-dsh-pv` 的 `tuikit.py`）：
//!
//! | 参考 | 这里 | 要点 |
//! |---|---|---|
//! | `box()` | [`window_box`] | **1px 描边** + 四角 7px 加粗角标 + 标题小标签（带旋转指示） |
//! | `haloed()` | [`haloed`] | 光晕只作用于**单个元素**，同色，半径极小 |
//! | `decode()` | [`decode_text`] | 逐字符在随机字形里抖动，再落到真字 |
//! | `scanlines()` / `vignette()` | [`scanlines`] / [`vignette`] | 每 3 行一条 alpha 55 的黑线；径向压暗 |
//! | `post()` | [`post`] | 残影 + **紧**辉光（1280 宽只 blur 4px）+ 扫描线 + 暗角 |
//!
//! 三个必须守住的数字，错了就是「模糊 / 杂乱 / 像 PPT」：
//!
//! 1. **辉光半径 ≈ 画面宽度的 0.3%**。参考是 1280 宽 blur 4px。上一版我在
//!    440 点宽的画布上用半径 7 点 = 1760 宽画面上 28px，**宽了 7 倍** —— 这就是「太模糊」。
//! 2. **底纹是规则网格、亮度 0.1**，不是随机噪点。参考 `dot_field()` 是 16px 一格的点阵。
//!    上一版铺了 20% 覆盖率的随机亮点场 —— 这就是「亮点太多、杂乱」。
//! 3. **文字用真字体按像素光栅化**，不塞进字符格。字符格 8×16 而汉字是方块字，
//!    13px 的汉字塞进 8px 的格子相邻两字重叠 40% —— 这就是「歌词糊」。

use image::{Rgba, RgbaImage};

use crate::export::raster::Rasterizer;

/// 底色。参考 `PALETTES["deepsea"]["BG"]`：深海军蓝黑，不是纯黑。
pub const BG: [u8; 3] = [4, 7, 15];
/// 系统色（文字 / 边框）。参考 `UI=(200,214,234)`：冷白钢色。
pub const UI: [u8; 3] = [200, 214, 234];

/// 朝底色混合：`mix(UI, 0.5)` 就是「半亮的系统色」。
pub fn mix(c: [u8; 3], level: f32) -> [u8; 3] {
    let l = level.clamp(0.0, 1.0);
    [
        (BG[0] as f32 + (c[0] as f32 - BG[0] as f32) * l).round() as u8,
        (BG[1] as f32 + (c[1] as f32 - BG[1] as f32) * l).round() as u8,
        (BG[2] as f32 + (c[2] as f32 - BG[2] as f32) * l).round() as u8,
    ]
}

/// 逐字符解码：新字符先在一串乱码字形里抖动，`settle` 秒后才落成真字。
///
/// 对照参考的 `decode()`。`age` 是这段文字已经显示了多久（秒）。
pub fn decode_text(s: &str, age: f64, seed: u64, rate: f64, settle: f64) -> String {
    const SCR: &[u8] = b"!<>-_\\/[]{}=+*^?#%$&@01|~:;";
    let n = ((age.max(0.0) * rate) as usize).min(s.chars().count());
    let mut rng = seed ^ 0x9E37_79B9_7F4A_7C15;
    let mut out = String::with_capacity(s.len());
    for (i, ch) in s.chars().enumerate() {
        if i >= n {
            break;
        }
        let a = age - i as f64 / rate;
        // xorshift：确定性，不需要 rand 依赖
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        if ch != ' ' && a < settle {
            out.push(SCR[(rng % SCR.len() as u64) as usize] as char);
        } else {
            out.push(ch);
        }
    }
    out
}

/// 窗口框：1px 描边 + 四角加粗角标 + 标题标签（可带旋转指示）。
///
/// 严格照抄参考的 `box()`：**不填充**，只描边。填充会立刻变成一堵色墙。
#[allow(clippy::too_many_arguments)]
pub fn window_box(
    r: &Rasterizer,
    img: &mut RgbaImage,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    title: &str,
    level: f32,
    spinner: Option<f64>,
    color: [u8; 3],
) {
    let line = mix(color, level);
    let corner = mix(color, (level + 0.4).min(1.0));
    rect_outline(img, x0, y0, x1, y1, line);

    // 四角加粗角标：长 7、粗 2。这是「终端窗口」质感的来源。
    const L: i32 = 7;
    for (px, py, sx, sy) in [
        (x0, y0, 1, 1),
        (x1, y0, -1, 1),
        (x0, y1, 1, -1),
        (x1, y1, -1, -1),
    ] {
        fill_rect(img, px, py - 1, L, 2, corner);
        fill_rect(img, px - 1, py, 2, L, corner);
        let _ = (sx, sy);
    }

    if title.is_empty() {
        return;
    }
    // 标题标签：在框线上挖一个底色小口，再把标题写上去
    let label = match spinner {
        Some(t) => format!("{} {}", "|/-\\".chars().nth((t * 8.0) as usize % 4).unwrap_or('|'), title),
        None => title.to_string(),
    };
    let f = label_with_spaces(&label);
    let tw = r.measure_text(&f, 13.0);
    fill_rect(img, x0 + 12, y0 - 9, tw, 19, BG);
    r.text(img, x0 + 12, y0 + 5, &f, 13.0, mix(color, (level + 0.35).min(1.0)), 1.0);
}

fn label_with_spaces(s: &str) -> String {
    format!(" {s} ")
}

/// 1px 矩形描边。
pub fn rect_outline(img: &mut RgbaImage, x0: i32, y0: i32, x1: i32, y1: i32, c: [u8; 3]) {
    let (xa, xb) = (x0.min(x1), x0.max(x1));
    let (ya, yb) = (y0.min(y1), y0.max(y1));
    fill_rect(img, xa, ya, xb - xa + 1, 1, c);
    fill_rect(img, xa, yb, xb - xa + 1, 1, c);
    fill_rect(img, xa, ya, 1, yb - ya + 1, c);
    fill_rect(img, xb, ya, 1, yb - ya + 1, c);
}

/// 实心矩形（带 alpha）。
pub fn fill_rect(img: &mut RgbaImage, x: i32, y: i32, w: i32, h: i32, c: [u8; 3]) {
    fill_rect_a(img, x, y, w, h, c, 1.0);
}

/// 实心矩形（指定不透明度）。
pub fn fill_rect_a(img: &mut RgbaImage, x: i32, y: i32, w: i32, h: i32, c: [u8; 3], a: f32) {
    let a = a.clamp(0.0, 1.0);
    if a <= 0.002 || w <= 0 || h <= 0 {
        return;
    }
    let (iw, ih) = (img.width() as i32, img.height() as i32);
    for yy in y.max(0)..(y + h).min(ih) {
        for xx in x.max(0)..(x + w).min(iw) {
            blend(img, xx, yy, c, a);
        }
    }
}

/// alpha 混合一个像素（`a` 是覆盖率，不做 premultiply）。
pub fn blend(img: &mut RgbaImage, x: i32, y: i32, c: [u8; 3], a: f32) {
    if a <= 0.002 || x < 0 || y < 0 || x >= img.width() as i32 || y >= img.height() as i32 {
        return;
    }
    let p = img.get_pixel_mut(x as u32, y as u32);
    let d = p.0;
    let k = a.clamp(0.0, 1.0);
    p.0 = [
        (d[0] as f32 + (c[0] as f32 - d[0] as f32) * k).round() as u8,
        (d[1] as f32 + (c[1] as f32 - d[1] as f32) * k).round() as u8,
        (d[2] as f32 + (c[2] as f32 - d[2] as f32) * k).round() as u8,
        255,
    ];
}

// ── 光晕 ────────────────────────────────────────────────────────

/// 给一块 RGBA 元素套同色光晕后合成到图上（对照参考的 `haloed()`）。
///
/// `radius` 单位是像素，**必须小**：参考在 1280 宽的画面上只用 4px。
/// 半径一大，锐利的边框和文字立刻糊成一团 —— 上一版就是栽在这里。
pub fn haloed(img: &mut RgbaImage, el: &RgbaImage, color: [u8; 3], k: f32, radius: u32) {
    if k <= 0.01 {
        composite(img, el);
        return;
    }
    let (w, h) = (el.width() as usize, el.height() as usize);
    let mut a = vec![0.0f32; w * h];
    for (i, p) in el.pixels().enumerate() {
        a[i] = p.0[3] as f32 / 255.0;
    }
    let b = box_blur2(&a, w, h, radius as usize);

    // 光晕层：颜色固定，alpha 来自模糊后的覆盖率
    let mut halo = RgbaImage::new(el.width(), el.height());
    for (i, p) in halo.pixels_mut().enumerate() {
        let v = (b[i] * k * 1.15).min(1.0);
        *p = Rgba([color[0], color[1], color[2], (v * 255.0).round() as u8]);
    }
    composite(img, &halo);
    composite(img, el);
}

/// 把一层 RGBA 用普通 alpha 混合叠到图上。
pub fn composite(img: &mut RgbaImage, el: &RgbaImage) {
    let (iw, ih) = (img.width(), img.height());
    for (x, y, p) in el.enumerate_pixels() {
        if x >= iw || y >= ih {
            continue;
        }
        let a = p.0[3] as f32 / 255.0;
        if a <= 0.002 {
            continue;
        }
        blend(img, x as i32, y as i32, [p.0[0], p.0[1], p.0[2]], a);
    }
}

/// 可分离盒式模糊，跑两遍近似高斯。返回模糊后的通道。
fn box_blur2(src: &[f32], w: usize, h: usize, radius: usize) -> Vec<f32> {
    if radius == 0 || w == 0 || h == 0 {
        return src.to_vec();
    }
    let r = radius.min(24);
    let mut a = box_blur1(src, w, h, r, true);
    a = box_blur1(&a, w, h, r, false);
    a = box_blur1(&a, w, h, r, true);
    box_blur1(&a, w, h, r, false)
}

/// 单向盒式模糊（`horizontal` = true 时按行）。
fn box_blur1(src: &[f32], w: usize, h: usize, r: usize, horizontal: bool) -> Vec<f32> {
    let mut out = vec![0.0f32; src.len()];
    let n = if horizontal { h } else { w };
    let m = if horizontal { w } else { h };
    let mut acc = vec![0.0f32; m + 1];
    for i in 0..n {
        acc[0] = 0.0;
        for j in 0..m {
            let v = if horizontal {
                src[i * w + j]
            } else {
                src[j * w + i]
            };
            acc[j + 1] = acc[j] + v;
        }
        for j in 0..m {
            let lo = j.saturating_sub(r);
            let hi = (j + r + 1).min(m);
            let s = acc[hi] - acc[lo];
            let v = s / (hi - lo) as f32;
            if horizontal {
                out[i * w + j] = v;
            } else {
                out[j * w + i] = v;
            }
        }
    }
    out
}

// ── CRT 后期（对照参考的 scanlines / vignette / post） ───────────

/// 每 3 行一条 alpha 55 的黑线。
pub fn scanlines(img: &mut RgbaImage) {
    let h = img.height() as i32;
    let w = img.width() as i32;
    let mut y = 0;
    while y < h {
        for x in 0..w {
            blend(img, x, y, [0, 0, 0], 55.0 / 255.0);
        }
        y += 3;
    }
}

/// 径向压暗（对照参考的 `vignette()`）：中心不动，越靠边越黑。
///
/// `lift_bottom` 为真时底部的歌词条不压暗 —— 歌词必须始终读得清。
pub fn vignette(img: &mut RgbaImage, lift_bottom: bool) {
    let (w, h) = (img.width() as i32, img.height() as i32);
    if w < 2 || h < 2 {
        return;
    }
    for y in 0..h {
        let dy = (y as f32 - h as f32 * 0.5) / (h as f32 * 0.5);
        for x in 0..w {
            let dx = (x as f32 - w as f32 * 0.5) / (w as f32 * 0.5);
            let d = dx * dx + dy * dy;
            let mut k = ((d - 0.35).max(0.0) * 0.55).min(1.0);
            // 底部歌词带：几乎不压
            if lift_bottom && y as f32 > h as f32 * 0.90 {
                k *= 0.25;
            }
            if k > 0.002 {
                blend(img, x, y, [0, 0, 0], k);
            }
        }
    }
}

/// 紧辉光：整幅图做一次小半径模糊后按 `bloom` 加回去。
///
/// 参考是 1280 宽 blur 4px，这里按画面宽度等比换算，绝不放大。
pub fn bloom(img: &mut RgbaImage, amount: f32) {
    if amount <= 0.01 {
        return;
    }
    let w = img.width() as usize;
    let h = img.height() as usize;
    let radius = ((w as f32 * 4.0 / 1280.0).round() as usize).clamp(2, 6);

    for ch in 0..3 {
        let src: Vec<f32> = img
            .pixels()
            .map(|p| p.0[ch] as f32 / 255.0)
            .collect();
        let b = box_blur2(&src, w, h, radius);
        for (i, p) in img.pixels_mut().enumerate() {
            let v = p.0[ch] as f32 / 255.0 + b[i] * amount;
            p.0[ch] = (v.min(1.0) * 255.0).round() as u8;
        }
    }
}

/// 底纹：16px 一格的**规则**点阵，亮度 0.1。
///
/// 对照参考的 `dot_field()`。关键是「规则」和「极暗」：
/// 随机亮点场会让画面看起来全是噪点（上一版的「杂乱 / 亮点太多」就是这么来的）。
pub fn dot_field(img: &mut RgbaImage, step: i32, level: f32) {
    let c = mix(UI, level);
    let (w, h) = (img.width() as i32, img.height() as i32);
    let mut y = step / 2;
    while y < h {
        let mut x = step / 2;
        while x < w {
            blend(img, x, y, c, 0.85);
            x += step;
        }
        y += step;
    }
}

// ── 外壳合成 ────────────────────────────────────────────────────

/// 一帧外壳需要的全部数据。
pub struct ChromeData<'a> {
    /// 场景主色（季节色）
    pub main: [u8; 3],
    /// 场景强调色
    pub accent: [u8; 3],
    /// 曲名 · 作者
    pub title: &'a str,
    /// 当前章节
    pub chapter: &'a str,
    /// 当前章节已进行的比例（0..=1），用于章节标题的逐字解码
    pub chapter_progress: f32,
    pub t: f64,
    pub progress: f32,
    /// 当前唱句全文
    pub line: Option<&'a str>,
    /// 当前唱句的逐词字节区间
    pub words: &'a [(usize, usize)],
    /// 正在唱的词序号（等于词数表示间奏）
    pub singing: usize,
    /// 下一句
    pub next: Option<&'a str>,
}

/// 画整帧外壳：顶栏窗口框、舞台上的两个数据框、底部歌词条、进度条。
///
/// 全部直接画在图像上（真字体、1px 描边），不做任何字符格映射。
pub fn draw_chrome(r: &Rasterizer, img: &mut RgbaImage, d: &ChromeData) {
    let (w, h) = (img.width() as i32, img.height() as i32);
    if w < 400 || h < 200 {
        return;
    }
    let amber = d.main;
    let pulse = 0.45 + 0.18 * ((d.t * 2.2).sin() as f32);

    // ── 顶栏 ──────────────────────────────────────────────────
    window_box(r, img, 20, 6, w - 20, 32, d.title, 0.55, Some(d.t), amber);
    let tc = format!(
        "{:02}:{:04.1} / 04:49.9",
        (d.t / 60.0) as u32,
        d.t % 60.0
    );
    let tcw = r.measure_text(&tc, 14.0);
    r.text(img, w - 40 - tcw, 26, &tc, 14.0, mix(UI, 0.62), 1.0);
    // 章节标题走参考实现的 `decode()`：新字符先在一串乱码字形里抖动，再落成真字
    let head = decode_text(
        &format!("▶ {}", d.chapter),
        d.chapter_progress as f64 * 24.0,
        11,
        14.0,
        0.06,
    );
    r.text(img, 40, 26, &head, 14.0, mix(amber, 0.85), 1.0);

    // ── 舞台上的两个数据框（TUI 质感；不压画面中心）──────────
    let stage_top = 44;
    let stage_bot = h - 108;
    if stage_bot - stage_top > 120 {
        let bw = 300;
        let bh = 96;
        // 左下：注意力读数
        let (ax0, ay0) = (24, stage_bot - bh);
        window_box(r, img, ax0, ay0, ax0 + bw, stage_bot, "attention · heads", 0.42, None, amber);
        for i in 0..4 {
            let v = (0.25 + 0.7 * (((d.t * 0.7 + i as f64 * 1.7).sin() * 0.5 + 0.5) as f32)).min(1.0);
            let bx = ax0 + 14 + i * 34;
            fill_rect_a(img, bx, stage_bot - 14 - (v * 44.0) as i32, 22, (v * 44.0) as i32, mix(amber, 0.75), 1.0);
            fill_rect_a(img, bx, stage_bot - 16, 22, 1, mix(UI, 0.35), 1.0);
        }
        let readout = format!("softmax  {:.4}", 0.5 + 0.5 * (d.t * 0.9).sin());
        r.text(img, ax0 + 150, ay0 + 44, &readout, 13.0, mix(UI, 0.5), 1.0);
        let pid = format!("pid {}", 4000 + (d.t as i64 % 900));
        r.text(img, ax0 + 150, ay0 + 70, &pid, 13.0, mix(UI, 0.42), 1.0);

        // 右下：时序波形
        let (bx0, by0) = (w - 24 - bw, stage_bot - bh);
        window_box(r, img, bx0, by0, bx0 + bw, stage_bot, "kv cache · t", 0.42, None, amber);
        let mid = by0 + 52;
        let mut prev = mid;
        for i in 0..(bw - 28) {
            let u = i as f64 * 0.16;
            let v = ((u + d.t).sin() * 0.6 + (u * 2.3 + d.t * 1.7).sin() * 0.4) as f32;
            let yy = mid - (v * 26.0) as i32;
            fill_rect_a(img, bx0 + 14 + i, yy.min(prev), 1, (prev - yy).abs().max(1), mix(amber, 0.7), 1.0);
            prev = yy;
        }
        let rows = format!("layers {}  ·  seq {}", 12 + (d.t as i64 % 4), 512);
        r.text(img, bx0 + 14, by0 + 82, &rows, 13.0, mix(UI, 0.45), 1.0);
    }

    // ── 底部歌词条 ────────────────────────────────────────────
    let (lx0, ly0, lx1, ly1) = (24, h - 100, w - 24, h - 26);
    window_box(r, img, lx0, ly0, lx1, ly1, "stdout · 歌詞", pulse, Some(d.t), amber);

    let base = ly1 - 16;
    match d.line {
        Some(text) if !text.is_empty() => {
            let px = 32.0;
            let total = r.measure_text(text, px);
            let mut x = lx0 + ((lx1 - lx0) - total) / 2;
            let space = r.measure_text(" ", px);
            for (i, (a, b)) in d.words.iter().enumerate() {
                if *a >= *b || *b > text.len() || !text.is_char_boundary(*a) || !text.is_char_boundary(*b) {
                    continue;
                }
                let piece = &text[*a..*b];
                let c = if i == d.singing {
                    // 正在唱：强调色打底 + 黑字（卡拉OK 走字块）
                    let pw = r.measure_text(piece, px);
                    fill_rect(img, x - 2, base - px as i32, pw + 4, (px * 1.25) as i32, d.accent);
                    [10, 10, 12]
                } else if i < d.singing {
                    mix(UI, 1.0)
                } else {
                    mix(UI, 0.42)
                };
                x = r.text(img, x, base, piece, px, c, 1.0);
                x += space;
            }
        }
        _ => {
            let dots = "♪  ·  ·  ·";
            let tw = r.measure_text(dots, 28.0);
            r.text(img, lx0 + ((lx1 - lx0) - tw) / 2, base, dots, 28.0, mix(UI, 0.3), 1.0);
        }
    }

    if let Some(nx) = d.next {
        let px = 14.0;
        let tw = r.measure_text(nx, px);
        r.text(img, lx0 + ((lx1 - lx0) - tw) / 2, ly1 - 8, nx, px, mix(UI, 0.35), 1.0);
    }

    // ── 进度条 ────────────────────────────────────────────────
    let py = h - 14;
    fill_rect_a(img, 24, py, w - 48, 3, mix(UI, 0.22), 1.0);
    let pw = ((w - 48) as f32 * d.progress.clamp(0.0, 1.0)) as i32;
    fill_rect(img, 24, py, pw.max(1), 3, d.accent);
    fill_rect(img, (24 + pw - 1).max(24), py - 4, 2, 11, mix(UI, 0.95));
    let pct = format!("{:>3}%", (d.progress.clamp(0.0, 1.0) * 100.0) as u32);
    r.text(img, w - 24 - r.measure_text(&pct, 13.0), py - 4, &pct, 13.0, mix(UI, 0.55), 1.0);
}

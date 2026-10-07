//! ASCII 模式：1×1 子像素，用字符梯度表达亮度。
//!
//! 分辨率最低，但**兼容性最好**（任何字体、任何终端都能显示），
//! 而且字符形状本身带纹理，做「代码雨」这类题材比 Braille 更有味道。

/// 每个字符列的子像素列数。
pub const SUB_X: u16 = 1;
/// 每个字符行的子像素行数。
pub const SUB_Y: u16 = 1;

/// 由疏到密的亮度梯度。
pub const RAMP: [char; 10] = [' ', '.', ':', '-', '=', '+', '*', '#', '%', '@'];

/// 只有 5 级的短梯度（小字号时更干净）。
pub const RAMP_SHORT: [char; 5] = [' ', '.', '+', '#', '@'];

/// 覆盖率 → 梯度字符（默认 10 级）。
pub fn ramp_char(t: f32) -> char {
    let t = t.clamp(0.0, 1.0);
    let i = (t * (RAMP.len() - 1) as f32).round() as usize;
    RAMP[i.min(RAMP.len() - 1)]
}

/// 覆盖率 → 指定梯度的字符。
pub fn ramp_char_with(ramp: &[char], t: f32) -> char {
    if ramp.is_empty() {
        return ' ';
    }
    let t = t.clamp(0.0, 1.0);
    let i = (t * (ramp.len() - 1) as f32).round() as usize;
    ramp[i.min(ramp.len() - 1)]
}

/// 字符 → 它在梯度里的亮度位置（0..1）。非梯度字符返回 `None`。
pub fn char_level(ramp: &[char], c: char) -> Option<f32> {
    if ramp.len() < 2 {
        return None;
    }
    ramp.iter()
        .position(|&r| r == c)
        .map(|i| i as f32 / (ramp.len() - 1) as f32)
}

/// 抖动：把连续覆盖率量化到梯度级别时加入有序抖动，避免大面积色带。
///
/// `x`、`y` 是单元坐标，用于取 4×4 Bayer 矩阵的值。
pub fn dither_char(t: f32, x: u16, y: u16, levels: usize) -> char {
    if levels < 2 {
        return ' ';
    }
    const BAYER4: [[f32; 4]; 4] = [
        [0.0, 8.0, 2.0, 10.0],
        [12.0, 4.0, 14.0, 6.0],
        [3.0, 11.0, 1.0, 9.0],
        [15.0, 7.0, 13.0, 5.0],
    ];
    let b = BAYER4[(y % 4) as usize][(x % 4) as usize] / 16.0 - 0.5;
    let step = 1.0 / (levels - 1) as f32;
    let v = (t.clamp(0.0, 1.0) + b * step).clamp(0.0, 1.0);
    let i = (v * (levels - 1) as f32).round() as usize;
    RAMP[i.min(RAMP.len() - 1).min(levels - 1)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ramp_endpoints() {
        assert_eq!(ramp_char(0.0), ' ');
        assert_eq!(ramp_char(1.0), '@');
        assert_eq!(ramp_char(-1.0), ' ');
        assert_eq!(ramp_char(2.0), '@');
    }

    #[test]
    fn ramp_is_monotonic_in_density() {
        let mut last = 0usize;
        for i in 0..=100 {
            let c = ramp_char(i as f32 / 100.0);
            let idx = RAMP.iter().position(|&r| r == c).unwrap();
            assert!(idx >= last, "梯度应单调不减");
            last = idx;
        }
    }

    #[test]
    fn ramp_char_with_handles_custom_and_empty() {
        assert_eq!(ramp_char_with(&[], 0.5), ' ');
        assert_eq!(ramp_char_with(&['a', 'b'], 0.0), 'a');
        assert_eq!(ramp_char_with(&['a', 'b'], 1.0), 'b');
        assert_eq!(ramp_char_with(&['a', 'b'], 0.5), 'b');
    }

    #[test]
    fn char_level_is_inverse_of_ramp() {
        for i in 0..RAMP.len() {
            let c = RAMP[i];
            let lv = char_level(&RAMP, c).unwrap();
            let expect = i as f32 / (RAMP.len() - 1) as f32;
            assert!((lv - expect).abs() < 1e-6);
        }
        assert_eq!(char_level(&RAMP, 'z'), None);
    }

    #[test]
    fn dither_preserves_extremes() {
        for x in 0..4u16 {
            for y in 0..4u16 {
                assert_eq!(dither_char(0.0, x, y, 10), ' ');
                assert_eq!(dither_char(1.0, x, y, 10), '@');
            }
        }
    }

    #[test]
    fn dither_breaks_flat_bands() {
        // 在量化边界附近，同一个值应被抖动摊成不止一种字符
        let mut seen = std::collections::HashSet::new();
        for y in 0..4u16 {
            for x in 0..4u16 {
                seen.insert(dither_char(0.49, x, y, 10));
            }
        }
        assert!(seen.len() >= 2, "抖动未产生层次");
    }

    #[test]
    fn dither_is_exact_at_extremes() {
        for y in 0..4u16 {
            for x in 0..4u16 {
                assert_eq!(dither_char(0.0, x, y, 10), ' ');
                assert_eq!(dither_char(1.0, x, y, 10), RAMP[9]);
            }
        }
    }

    #[test]
    fn geometry_is_1x1() {
        assert_eq!(SUB_X, 1);
        assert_eq!(SUB_Y, 1);
    }
}

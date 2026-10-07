//! Braille 点阵：每个字符 2×4 = 8 个子像素，是三种模式里密度最高的。
//!
//! Unicode Braille Patterns 区块 U+2800..U+28FF 的位序**不是**简单的行优先，
//! 而是历史上为 6 点盲文设计的 2×3 布局再加左下/右下两点：
//!
//! ```text
//!  (0,0)=0x01  (1,0)=0x08
//!  (0,1)=0x02  (1,1)=0x10
//!  (0,2)=0x04  (1,2)=0x20
//!  (0,3)=0x40  (1,3)=0x80
//! ```
//!
//! 搞错这张表就会得到上下颠倒或左右错位的画面。

/// 每个字符列包含的子像素列数。
pub const SUB_X: u16 = 2;
/// 每个字符行包含的子像素行数。
pub const SUB_Y: u16 = 4;

/// `BRAILLE_BITS[sx][sy]` → 位掩码。
pub const BRAILLE_BITS: [[u8; 4]; 2] = [[0x01, 0x02, 0x04, 0x40], [0x08, 0x10, 0x20, 0x80]];

/// 位掩码 → Braille 字符。0 返回空格（不显示点）。
pub fn braille_char(bits: u8) -> char {
    if bits == 0 {
        ' '
    } else {
        char::from_u32(0x2800 + bits as u32).unwrap_or(' ')
    }
}

/// 位掩码 → 2×4 布尔点阵。
pub fn braille_to_dots(bits: u8) -> [[bool; 4]; 2] {
    let mut out = [[false; 4]; 2];
    for sx in 0..2 {
        for sy in 0..4 {
            out[sx][sy] = bits & BRAILLE_BITS[sx][sy] != 0;
        }
    }
    out
}

/// 2×4 布尔点阵 → 位掩码。
pub fn dots_to_braille(dots: &[[bool; 4]; 2]) -> u8 {
    let mut bits = 0u8;
    for sx in 0..2 {
        for sy in 0..4 {
            if dots[sx][sy] {
                bits |= BRAILLE_BITS[sx][sy];
            }
        }
    }
    bits
}

/// 字符是否落在 Braille 区块内。
pub fn is_braille(c: char) -> bool {
    ('\u{2800}'..='\u{28FF}').contains(&c)
}

/// 从 8 个覆盖率（按 `(sx, sy)` 顺序传入）聚合出位掩码。
///
/// `threshold` 是置位阈值 —— 也就是「简单 AA」的抖动点：
/// 覆盖率高于阈值才点亮点，低于阈值则丢弃，避免半透明边缘糊成一团。
pub fn bits_from_coverage(cov: &[[f32; 4]; 2], threshold: f32) -> u8 {
    let mut bits = 0u8;
    for sx in 0..2 {
        for sy in 0..4 {
            if cov[sx][sy] > threshold {
                bits |= BRAILLE_BITS[sx][sy];
            }
        }
    }
    bits
}

/// 位掩码里的点亮数量（0..8）。
pub fn dot_count(bits: u8) -> u32 {
    bits.count_ones()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bit_table_matches_unicode_layout() {
        // (0,0) 是 0x01，(1,3) 是 0x80 —— 最容易写错的两个端点
        assert_eq!(BRAILLE_BITS[0][0], 0x01);
        assert_eq!(BRAILLE_BITS[1][3], 0x80);
        // 全 8 位应互不重复且覆盖 0xFF
        let mut all = 0u8;
        for sx in 0..2 {
            for sy in 0..4 {
                assert_eq!(all & BRAILLE_BITS[sx][sy], 0, "位重复");
                all |= BRAILLE_BITS[sx][sy];
            }
        }
        assert_eq!(all, 0xFF);
    }

    #[test]
    fn full_dots_gives_full_braille() {
        let bits = dots_to_braille(&[[true; 4]; 2]);
        assert_eq!(bits, 0xFF);
        assert_eq!(braille_char(bits), '\u{28FF}');
    }

    #[test]
    fn empty_dots_gives_space() {
        assert_eq!(dots_to_braille(&[[false; 4]; 2]), 0);
        assert_eq!(braille_char(0), ' ');
    }

    #[test]
    fn single_dot_roundtrip() {
        for sx in 0..2 {
            for sy in 0..4 {
                let mut d = [[false; 4]; 2];
                d[sx][sy] = true;
                let bits = dots_to_braille(&d);
                assert_eq!(bits, BRAILLE_BITS[sx][sy]);
                let back = braille_to_dots(bits);
                assert!(back[sx][sy]);
                assert_eq!(back.iter().flatten().filter(|x| **x).count(), 1);
            }
        }
    }

    #[test]
    fn char_is_in_braille_block() {
        for bits in 0..=255u8 {
            let c = braille_char(bits);
            if bits == 0 {
                assert_eq!(c, ' ');
            } else {
                assert!(is_braille(c), "bits={bits:#04x} 得到 {c:?}");
                assert_eq!(c as u32 - 0x2800, bits as u32);
            }
        }
    }

    #[test]
    fn coverage_threshold_controls_dots() {
        let mut cov = [[0.0f32; 4]; 2];
        cov[0][0] = 0.4;
        cov[1][3] = 0.9;
        // 阈值 0.5：只保留 0.9
        let bits = bits_from_coverage(&cov, 0.5);
        assert_eq!(bits, BRAILLE_BITS[1][3]);
        // 阈值 0.3：两个都保留
        let bits2 = bits_from_coverage(&cov, 0.3);
        assert_eq!(bits2, BRAILLE_BITS[0][0] | BRAILLE_BITS[1][3]);
    }

    #[test]
    fn dot_count_is_correct() {
        assert_eq!(dot_count(0), 0);
        assert_eq!(dot_count(0xFF), 8);
        assert_eq!(dot_count(BRAILLE_BITS[0][0] | BRAILLE_BITS[1][3]), 2);
    }

    #[test]
    fn subpixel_geometry_is_2x4() {
        assert_eq!(SUB_X, 2);
        assert_eq!(SUB_Y, 4);
        assert_eq!(BRAILLE_BITS.len(), SUB_X as usize);
        assert_eq!(BRAILLE_BITS[0].len(), SUB_Y as usize);
    }
}

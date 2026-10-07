//! 半块模式：每个字符上下 2 个子像素，用 `▀ ▄ █` 表达。
//!
//! 分辨率是 Braille 的一半（1×2），但**颜色**可以上下各一，
//! 因此在需要强烈双色对比（比如夕阳、霓虹）时反而比 Braille 好看。
//! 本项目的字符 MV 以 Braille 为主，半块用于需要双色渐变的段落。

/// 每个字符列的子像素列数。
pub const SUB_X: u16 = 1;
/// 每个字符行的子像素行数。
pub const SUB_Y: u16 = 2;

/// 上半块
pub const UPPER: char = '▀';
/// 下半块
pub const LOWER: char = '▄';
/// 全块
pub const FULL: char = '█';
/// 空格
pub const EMPTY: char = ' ';

/// 由上下两个覆盖率选出字符。
///
/// 两个子像素各自的颜色可以不同 —— 调用方应把上色填到 `fg`、下色填到 `bg`。
pub fn half_char(top: f32, bottom: f32, threshold: f32) -> char {
    let t = top > threshold;
    let b = bottom > threshold;
    match (t, b) {
        (true, true) => FULL,
        (true, false) => UPPER,
        (false, true) => LOWER,
        (false, false) => EMPTY,
    }
}

/// 半块字符 → 上下是否点亮。
pub fn half_dots(c: char) -> (bool, bool) {
    match c {
        FULL => (true, true),
        UPPER => (true, false),
        LOWER => (false, true),
        _ => (false, false),
    }
}

/// 字符是否为半块族。
pub fn is_half_block(c: char) -> bool {
    matches!(c, UPPER | LOWER | FULL)
}

/// 按覆盖率生成更细的块字符梯度（1/8 精度）。
///
/// 使用 U+2581..U+2588 的八分之一块，用于纯色柱状/条形图。
pub const EIGHTHS: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// 覆盖率 → 八分之一块字符。
pub fn eighth_char(cov: f32) -> char {
    let i = (cov.clamp(0.0, 1.0) * 8.0).round() as usize;
    EIGHTHS[i.min(8)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn threshold_decides_each_half() {
        assert_eq!(half_char(0.9, 0.9, 0.5), FULL);
        assert_eq!(half_char(0.9, 0.1, 0.5), UPPER);
        assert_eq!(half_char(0.1, 0.9, 0.5), LOWER);
        assert_eq!(half_char(0.1, 0.1, 0.5), EMPTY);
    }

    #[test]
    fn half_dots_roundtrips() {
        for c in [FULL, UPPER, LOWER, EMPTY] {
            let (t, b) = half_dots(c);
            let back = half_char(if t { 1.0 } else { 0.0 }, if b { 1.0 } else { 0.0 }, 0.5);
            assert_eq!(back, c, "往返失败: {c:?}");
        }
    }

    #[test]
    fn eighth_char_endpoints() {
        assert_eq!(eighth_char(0.0), ' ');
        assert_eq!(eighth_char(1.0), '█');
        assert_eq!(eighth_char(-1.0), ' ');
        assert_eq!(eighth_char(9.0), '█');
    }

    #[test]
    fn eighth_char_is_monotonic_in_density() {
        // 只检查索引递增（字符本身按设计递增）
        let mut last = 0usize;
        for i in 0..=40 {
            let cov = i as f32 / 40.0;
            let idx = EIGHTHS.iter().position(|&c| c == eighth_char(cov)).unwrap();
            assert!(idx >= last);
            last = idx;
        }
    }

    #[test]
    fn half_block_detection() {
        assert!(is_half_block(FULL));
        assert!(is_half_block(UPPER));
        assert!(!is_half_block('a'));
        assert!(!is_half_block(' '));
    }

    #[test]
    fn geometry_is_1x2() {
        assert_eq!(SUB_X, 1);
        assert_eq!(SUB_Y, 2);
    }
}

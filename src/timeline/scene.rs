//! 场景定义——《春・夏・秋・冬》。
//!
//! 十个场景按歌曲结构划分。**时间不写死在这里** ——
//! 具体起止来自 `timeline.toml`（见 [`crate::timeline`]），
//! 本模块只定义「有哪些场景」以及每个场景的视觉基调。
//!
//! 每段内部再按当前唱句切「卡」（见 scenes_*.rs 的 CARDS 表），
//! 卡与卡共享背景层，保证流动连贯。

use ratatui::style::Color;
use serde::{Deserialize, Serialize};

use crate::render::color as col;

/// 十个场景。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SceneKind {
    /// 前奏：万物未醒，灰白晨雾
    Intro,
    /// 春：苔露蝶蕾，东风解云
    Spring,
    /// 夏：白雨莲叶，银轨酒香
    Summer,
    /// 桥：瞬き、幻（形态流转）
    Bridge,
    /// 副歌：空の気配、人の世（四季流转）
    Chorus,
    /// 秋：松影月盃，砧声夜风
    Autumn,
    /// 冬：枯野银花，霜枝南天
    Winter,
    /// 昇華：雪道、绘卷、独楽、笔
    Ascension,
    /// 終章：花散月欠，记忆不锈
    Finale,
    /// 尾声：收卷归寂
    Outro,
}

impl SceneKind {
    /// 全部场景（按歌曲顺序）。
    pub const ALL: [SceneKind; 10] = [
        SceneKind::Intro,
        SceneKind::Spring,
        SceneKind::Summer,
        SceneKind::Bridge,
        SceneKind::Chorus,
        SceneKind::Autumn,
        SceneKind::Winter,
        SceneKind::Ascension,
        SceneKind::Finale,
        SceneKind::Outro,
    ];

    /// 短名（用于 UI 与日志）。
    pub fn name(&self) -> &'static str {
        match self {
            SceneKind::Intro => "intro",
            SceneKind::Spring => "spring",
            SceneKind::Summer => "summer",
            SceneKind::Bridge => "bridge",
            SceneKind::Chorus => "chorus",
            SceneKind::Autumn => "autumn",
            SceneKind::Winter => "winter",
            SceneKind::Ascension => "ascension",
            SceneKind::Finale => "finale",
            SceneKind::Outro => "outro",
        }
    }

    /// 中文显示名（UI 用）。
    pub fn label(&self) -> &'static str {
        match self {
            SceneKind::Intro => "序",
            SceneKind::Spring => "春",
            SceneKind::Summer => "夏",
            SceneKind::Bridge => "桥",
            SceneKind::Chorus => "副歌",
            SceneKind::Autumn => "秋",
            SceneKind::Winter => "冬",
            SceneKind::Ascension => "昇華",
            SceneKind::Finale => "終章",
            SceneKind::Outro => "尾声",
        }
    }

    /// 季节汉字（大字题写用）。
    pub fn kanji(&self) -> &'static str {
        match self {
            SceneKind::Intro => "序",
            SceneKind::Spring => "春",
            SceneKind::Summer => "夏",
            SceneKind::Bridge => "間",
            SceneKind::Chorus => "巡",
            SceneKind::Autumn => "秋",
            SceneKind::Winter => "冬",
            SceneKind::Ascension => "巻",
            SceneKind::Finale => "銘",
            SceneKind::Outro => "終",
        }
    }

    /// 序号。
    pub fn index(&self) -> usize {
        Self::ALL.iter().position(|s| s == self).unwrap_or(0)
    }

    /// 由序号取场景（越界时回到第一个）。
    pub fn from_index(i: usize) -> SceneKind {
        Self::ALL[i % Self::ALL.len()]
    }

    /// 该场景的三色基调：主色、副色、强调色。
    ///
    /// **主色必须是亮色** —— 它决定主体的实际亮度。
    /// 季节分色 + 各季专属强调色，保证四季一眼可辨且颜色丰富。
    pub fn palette(&self) -> [Color; 3] {
        match self {
            // 序：晨雾灰白
            SceneKind::Intro => [Color::Rgb(168, 178, 188), col::DARK_GREY, col::GREY_WHITE],
            // 春：嫩绿 + 樱粉点睛
            SceneKind::Spring => [Color::Rgb(140, 214, 120), Color::Rgb(88, 158, 84), Color::Rgb(248, 152, 178)],
            // 夏：浓青 + 热橙点睛（白雨用亮白）
            SceneKind::Summer => [Color::Rgb(70, 196, 210), Color::Rgb(34, 120, 150), Color::Rgb(255, 176, 96)],
            // 桥：青→紫的过渡色（形态流转）
            SceneKind::Bridge => [Color::Rgb(150, 130, 230), Color::Rgb(80, 110, 190), col::WHITE],
            // 副歌：四色环的基准（绿→青→琥珀的时序用 hue_shift 驱动）
            SceneKind::Chorus => [Color::Rgb(190, 214, 120), Color::Rgb(120, 190, 190), col::WHITE],
            // 秋：琥珀金 + 深红点睛
            SceneKind::Autumn => [Color::Rgb(240, 180, 88), Color::Rgb(178, 118, 52), Color::Rgb(216, 84, 58)],
            // 冬：蓝白 + 朱红点睛（南天果）
            SceneKind::Winter => [Color::Rgb(196, 218, 240), Color::Rgb(120, 150, 190), Color::Rgb(235, 84, 66)],
            // 昇華：金白（全曲最高点）
            SceneKind::Ascension => [Color::Rgb(255, 214, 130), Color::Rgb(210, 170, 90), col::WHITE],
            // 終章：褪色的春绿（记忆感，饱和度压低）
            SceneKind::Finale => [Color::Rgb(168, 196, 158), Color::Rgb(120, 148, 118), Color::Rgb(220, 160, 150)],
            // 尾声：归寂灰白
            SceneKind::Outro => [Color::Rgb(140, 146, 154), col::DARK_GREY, col::GREY_WHITE],
        }
    }

    /// 该场景默认的后处理强度（`bloom, aberration, scanline, shake, vignette, glitch`）。
    ///
    /// `glitch` 的基线刻意压得很低：glitch 会**整块清空/填亮**画面，
    /// 真正的故障效果留给 `[[cue]]` 事件。
    pub fn default_postfx(&self) -> [f32; 6] {
        match self {
            SceneKind::Intro => [0.35, 0.15, 0.25, 0.05, 0.35, 0.05],
            SceneKind::Spring => [0.45, 0.20, 0.22, 0.06, 0.35, 0.03],
            SceneKind::Summer => [0.55, 0.30, 0.38, 0.14, 0.40, 0.05],
            SceneKind::Bridge => [0.60, 0.45, 0.45, 0.25, 0.60, 0.08],
            SceneKind::Chorus => [0.80, 0.50, 0.40, 0.28, 0.45, 0.06],
            SceneKind::Autumn => [0.55, 0.25, 0.28, 0.08, 0.45, 0.04],
            SceneKind::Winter => [0.70, 0.18, 0.20, 0.05, 0.40, 0.03],
            SceneKind::Ascension => [0.95, 0.55, 0.35, 0.20, 0.35, 0.05],
            SceneKind::Finale => [0.60, 0.25, 0.30, 0.06, 0.50, 0.03],
            SceneKind::Outro => [0.40, 0.15, 0.25, 0.04, 0.45, 0.02],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_all_defined() {
        for k in SceneKind::ALL {
            assert!(!k.name().is_empty());
            assert!(!k.label().is_empty());
            assert!(!k.kanji().is_empty());
        }
    }

    #[test]
    fn palettes_are_all_defined() {
        for k in SceneKind::ALL {
            let p = k.palette();
            assert!(col::luma(p[0]) > 0.05, "{k:?} 主色过暗");
        }
    }

    #[test]
    fn postfx_values_are_in_range() {
        for k in SceneKind::ALL {
            for v in k.default_postfx() {
                assert!((0.0..=1.0).contains(&v), "{k:?} 后处理强度 {v} 越界");
            }
        }
    }

    #[test]
    fn round_trip_index() {
        for (i, k) in SceneKind::ALL.iter().enumerate() {
            assert_eq!(k.index(), i);
            assert_eq!(SceneKind::from_index(i), *k);
        }
    }
}

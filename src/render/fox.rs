//! 狐狸剪影：用参数曲线画一只可辨识的狐狸。
//!
//! 全部是**解析几何**，不依赖任何图片或字体资源，
//! 所以同一段代码在终端和导出位图里画出来完全一致。
//!
//! 形象要素（按辨识度排序）：
//! 1. 两只尖耳 —— 最能一眼认出是狐狸
//! 2. 狭长的眼 —— 雌小鬼的傲慢感来源
//! 3. 蓬松的尾 —— 区分狐狸与猫狗
//!
//! 所有函数返回**归一化坐标**（-1..1，原点在中心，y 轴向上），
//! 由调用方按画布尺寸和半径缩放，这样换分辨率不会变形。

/// 一只狐狸的姿态参数。
#[derive(Debug, Clone, Copy)]
pub struct FoxPose {
    /// 整体缩放（1.0 = 基准大小）
    pub scale: f32,
    /// 头部倾斜（弧度，正数向右歪）
    pub tilt: f32,
    /// 呼吸相位 0..1，驱动耳朵与胸腔起伏
    pub breath: f32,
    /// 眨眼 0..1，1 = 完全闭眼
    pub blink: f32,
    /// 尾巴摆动相位 0..1
    pub tail: f32,
    /// 视线方向 -1..1（负左正右），瞳孔随之偏移
    pub gaze: f32,
}

impl Default for FoxPose {
    fn default() -> Self {
        Self {
            scale: 1.0,
            tilt: 0.0,
            breath: 0.25,
            blink: 0.0,
            tail: 0.0,
            gaze: 0.0,
        }
    }
}

impl FoxPose {
    /// 由音乐与时间推出姿态。纯函数 —— 同一个 `t` 永远得到同一个姿态。
    pub fn from_music(t: f32, energy: f32, beat: f32, vocal: f32) -> Self {
        // 呼吸：0.28 Hz 的慢波，副歌时加快
        let rate = 0.28 + energy * 0.35;
        let breath = (t * rate * std::f32::consts::TAU).sin() * 0.5 + 0.5;

        // 眨眼：每 ~3.7 秒一次，闭合过程约 0.14 秒
        let cycle = ((t / 3.7).fract() * 3.7).rem_euclid(3.7);
        let blink = if cycle < 0.07 {
            cycle / 0.07
        } else if cycle < 0.14 {
            (0.14 - cycle) / 0.07
        } else {
            0.0
        };
        // 节拍瞬间眯眼（像被闪到）
        let blink = (blink + beat * 0.45).min(1.0);

        // 尾巴：低频驱动，慢摆
        let tail = (t * (0.5 + energy * 0.6)).sin() * 0.5 + 0.5;

        // 头部微倾：随人声起伏
        let tilt = (t * 0.37).sin() * 0.05 * (0.4 + vocal);

        Self {
            scale: 1.0,
            tilt,
            breath,
            blink,
            tail,
            gaze: (t * 0.23).sin() * 0.7,
        }
    }

    /// 把归一化坐标旋转 tilt 并缩放。
    fn place(&self, x: f32, y: f32) -> (f32, f32) {
        let (s, c) = self.tilt.sin_cos();
        let x = x * self.scale;
        let y = y * self.scale;
        (x * c - y * s, x * s + y * c)
    }
}

/// 狐狸的整体轮廓（头 + 双耳），返回按顺序连成闭合曲线的点。
///
/// **耳朵是独立的两块尖三角，不是把头部轮廓掰弯**。
/// 早先的实现把椭圆上的点朝耳尖方向插值，拉出来的是一团扭曲的曲线，
/// 导出后完全看不出是耳朵 —— 这是「画出来不像狐狸」的首要原因。
///
/// 现在按顺序拼：左耳根 → 左耳尖 → 左耳内根 → 头顶弧 → 右耳内根 →
/// 右耳尖 → 右耳根 → 右下颌 → 下巴 → 左下颌，闭合成一条曲线。
pub fn outline(pose: &FoxPose, steps: usize) -> Vec<(f32, f32)> {
    let steps = steps.clamp(32, 2048);
    let b = pose.breath;
    let mut pts: Vec<(f32, f32)> = Vec::with_capacity(steps + 16);

    // 头部椭圆参数
    let rx = 0.62;
    let ry = 0.52;
    let head_cy = -0.22;
    // 耳根在头顶两侧
    let ear_root_x = 0.34;
    let ear_root_y = 0.26;
    // 耳尖：向外上方伸出，是狐狸最标志性的特征
    let ear_tip_out = 0.72;
    let ear_tip_up = 1.02 + b * 0.05;
    // 耳内侧收窄点（让耳朵有厚度）
    let ear_in_x = 0.16;

    // ── 左耳 ──
    // 从耳根外侧起，到耳尖，再回内侧耳根
    pts.push((-ear_root_x, ear_root_y));
    pts.push((-ear_tip_out, ear_tip_up));
    pts.push((-ear_in_x, 0.30));

    // ── 头顶弧（从左耳内根跨到右耳内根）──
    // 参数从角 π 走到 0，y 取椭圆的上半部分
    // 两段弧（头顶 + 下颌）平分步数，另留 8 个点给耳朵与闭合。
    // 之前写成 steps/3，导致总点数只有请求值的 2/3 —— 采样精度
    // 在缩放时不够，耳尖和下巴会出现可见的棱角。
    let arc_budget = steps.saturating_sub(8).max(24);
    let top_n = (arc_budget / 2).max(12);
    for i in 0..=top_n {
        let a = std::f32::consts::PI - (i as f32 / top_n as f32) * std::f32::consts::PI;
        let x = a.cos() * rx * 0.92;
        let y = head_cy + a.sin() * ry + b * 0.012;
        // 额头上方压平一点，做出「眉骨」
        let y = if x.abs() < 0.3 { y - 0.03 } else { y };
        pts.push((x, y));
    }

    // ── 右耳 ──
    pts.push((ear_in_x, 0.30));
    pts.push((ear_tip_out, ear_tip_up));
    pts.push((ear_root_x, ear_root_y));

    // ── 右下颌 → 下巴 → 左下颌 ──
    // 参数从角 0 走到 -π（下半部分），并把 x 往内收做出尖下巴
    let bot_n = (arc_budget - top_n).max(16);
    for i in 0..=bot_n {
        let a = -(i as f32 / bot_n as f32) * std::f32::consts::PI;
        let x = a.cos() * rx;
        let mut y = head_cy + a.sin() * ry * 1.18;
        // 下巴收窄：越靠近底部越尖
        let k = ((-y) / ry).powf(0.8).min(1.0);
        let xx = x * (1.0 - k * 0.45);
        y += b * 0.008;
        pts.push((xx, y));
    }

    // 闭合
    if let Some(&first) = pts.first() {
        pts.push(first);
    }
    pts.into_iter().map(|(x, y)| pose.place(x, y)).collect()
}

/// 尾巴：一条从身体右后方伸出的蓬松曲线，返回**中心线**上的点。
///
/// 调用方沿线扫过并叠加宽度，就能得到有粗细变化的尾巴。
pub fn tail_spine(pose: &FoxPose, steps: usize) -> Vec<(f32, f32)> {
    let steps = steps.clamp(16, 512);
    let mut pts = Vec::with_capacity(steps + 1);
    let swing = (pose.tail - 0.5) * 2.0; // -1..1
    for i in 0..=steps {
        let u = i as f32 / steps as f32;
        // 从身体右后方向外向下弯
        let x = 0.55 + u * 0.85;
        // 基础下垂 + 摆动
        let y = -0.25 - u * 0.75 + swing * 0.28 * (u * std::f32::consts::PI).sin();
        // 末端上翘，做出蓬松的尖
        let y = y + u.powf(2.5) * 0.55;
        pts.push(pose.place(x, y));
    }
    pts
}

/// 眼睛：返回 (左眼中心, 右眼中心) 的归一化位置。
pub fn eye_centers(pose: &FoxPose) -> ((f32, f32), (f32, f32)) {
    let lx = -0.26 + pose.gaze * 0.045;
    let rx = 0.26 + pose.gaze * 0.045;
    let y = 0.10;
    (pose.place(lx, y), pose.place(rx, y))
}

/// 眼睛的闭合系数 0..1（1 = 完全闭上）。
pub fn eye_openness(pose: &FoxPose) -> f32 {
    (1.0 - pose.blink).clamp(0.0, 1.0)
}

/// 瞳孔半径（归一化）：人声越强瞳孔越大，眯眼时被压扁。
///
/// 眯眼压扁是刻意的 —— 竖直方向收缩、水平方向保持，
/// 做出猫科那种竖瞳被眼睑截断的样子。
pub fn pupil_radius(pose: &FoxPose, vocal: f32) -> f32 {
    let base = 0.055 + vocal.clamp(0.0, 1.0) * 0.045;
    base * (1.0 - pose.blink.clamp(0.0, 1.0) * 0.35)
}

/// 鼻子：一个小三角，位于吻部前端。
pub fn nose_points(pose: &FoxPose) -> Vec<(f32, f32)> {
    let base = [(0.0, -0.30), (-0.055, -0.38), (0.055, -0.38), (0.0, -0.30)];
    base.iter()
        .map(|&(x, y)| pose.place(x, y))
        .collect::<Vec<_>>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outline_is_closed_and_sized() {
        let p = FoxPose::default();
        let o = outline(&p, 128);
        assert!(o.len() >= 129, "点数 {} 太少", o.len());
        let first = o[0];
        let last = *o.last().unwrap();
        assert!(
            (first.0 - last.0).abs() < 1e-5 && (first.1 - last.1).abs() < 1e-5,
            "轮廓未闭合"
        );
    }

    #[test]
    fn outline_has_ears_higher_than_head() {
        let p = FoxPose::default();
        let o = outline(&p, 256);
        let max_y = o.iter().map(|q| q.1).fold(f32::MIN, f32::max);
        // 耳朵应该明显高出头部椭圆（0.60）
        assert!(max_y > 0.85, "耳朵不够高: max_y={max_y}");
    }

    #[test]
    fn outline_is_roughly_symmetric_when_not_tilted() {
        let p = FoxPose::default();
        let o = outline(&p, 256);
        let left = o.iter().filter(|q| q.0 < -0.1).count();
        let right = o.iter().filter(|q| q.0 > 0.1).count();
        let ratio = left as f32 / right.max(1) as f32;
        assert!((0.7..1.4).contains(&ratio), "左右不对称: {left} vs {right}");
    }

    #[test]
    fn pose_is_deterministic() {
        let a = FoxPose::from_music(12.34, 0.5, 0.2, 0.3);
        let b = FoxPose::from_music(12.34, 0.5, 0.2, 0.3);
        assert_eq!(a.breath, b.breath);
        assert_eq!(a.tail, b.tail);
        assert_eq!(a.blink, b.blink);
    }

    #[test]
    fn blink_is_bounded() {
        for i in 0..2000 {
            let t = i as f32 * 0.01;
            let p = FoxPose::from_music(t, 0.5, 0.0, 0.5);
            assert!((0.0..=1.0).contains(&p.blink), "t={t} blink={}", p.blink);
            assert!((0.0..=1.0).contains(&p.breath), "t={t} breath={}", p.breath);
            assert!((0.0..=1.0).contains(&p.tail), "t={t} tail={}", p.tail);
        }
    }

    #[test]
    fn blink_actually_closes_sometimes() {
        // 3.7 秒周期，采样 4 秒应该抓到至少一次闭眼
        let mut closed = false;
        for i in 0..400 {
            let p = FoxPose::from_music(i as f32 * 0.01, 0.3, 0.0, 0.3);
            if p.blink > 0.8 {
                closed = true;
                break;
            }
        }
        assert!(closed, "4 秒内没有眨眼");
    }

    #[test]
    fn beat_forces_a_squint() {
        let no_beat = FoxPose::from_music(1.234, 0.5, 0.0, 0.5);
        let with_beat = FoxPose::from_music(1.234, 0.5, 1.0, 0.5);
        assert!(
            with_beat.blink > no_beat.blink,
            "节拍应让眼睛眯起: {} vs {}",
            no_beat.blink,
            with_beat.blink
        );
    }

    #[test]
    fn tail_spine_extends_outward() {
        let p = FoxPose::default();
        let s = tail_spine(&p, 32);
        assert_eq!(s.len(), 33);
        assert!(s.last().unwrap().0 > s[0].0, "尾巴应向右侧伸出");
    }

    #[test]
    fn eyes_are_symmetric_and_apart() {
        let p = FoxPose::default();
        let (l, r) = eye_centers(&p);
        assert!(l.0 < 0.0 && r.0 > 0.0, "左右眼应在两侧");
        assert!((l.1 - r.1).abs() < 1e-5, "两眼应等高");
        assert!(r.0 - l.0 > 0.3, "两眼距离太近");
    }

    #[test]
    fn eye_openness_tracks_blink() {
        let mut p = FoxPose::default();
        p.blink = 0.0;
        assert!((eye_openness(&p) - 1.0).abs() < 1e-6);
        p.blink = 1.0;
        assert!(eye_openness(&p) < 1e-6);
    }

    #[test]
    fn pupil_grows_with_vocal() {
        let p = FoxPose::default();
        assert!(pupil_radius(&p, 1.0) > pupil_radius(&p, 0.0));
    }

    #[test]
    fn nose_is_below_eyes() {
        let p = FoxPose::default();
        let n = nose_points(&p);
        let (_, r) = eye_centers(&p);
        assert!(
            n.iter().all(|q| q.1 < r.1),
            "鼻子应在眼睛下方: nose_y={:?} eye_y={}",
            n.iter().map(|q| q.1).collect::<Vec<_>>(),
            r.1
        );
    }

    #[test]
    fn scale_actually_scales() {
        let small = FoxPose {
            scale: 0.5,
            ..Default::default()
        };
        let big = FoxPose::default();
        let a = outline(&small, 64);
        let b = outline(&big, 64);
        let ra = a.iter().map(|q| q.0.abs()).fold(0.0f32, f32::max);
        let rb = b.iter().map(|q| q.0.abs()).fold(0.0f32, f32::max);
        assert!((rb / ra - 2.0).abs() < 0.1, "缩放比 {:.2}", rb / ra);
    }

    #[test]
    fn all_points_are_finite() {
        for i in 0..50 {
            let p = FoxPose::from_music(i as f32 * 0.7, 0.5, 0.5, 0.5);
            for (x, y) in outline(&p, 64) {
                assert!(x.is_finite() && y.is_finite(), "轮廓出现 NaN/Inf");
            }
            for (x, y) in tail_spine(&p, 32) {
                assert!(x.is_finite() && y.is_finite(), "尾巴出现 NaN/Inf");
            }
        }
    }
}

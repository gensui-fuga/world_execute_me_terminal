//! 节拍网格与关键帧插值。
//!
//! 参考成熟实现的做法：把节拍位置算成**绝对时间**，而不是维护一个会漂移的累加器。
//! `beat_t(n) = first_beat + n * beat` 是纯函数，任意跳转都能立即对齐。
//!
//! 三个核心工具：
//! - [`BeatGrid::snap`] —— 把任意时刻吸附到最近的半拍，让视觉动作卡在拍上
//! - [`BeatGrid::pulse`] —— 距上一拍越近越接近 1，指数衰减，做出「一击即散」
//! - [`keyframes`] —— 分段线性插值，用于手工编排某个量的时间曲线

/// 节拍网格。
#[derive(Debug, Clone, Copy)]
pub struct BeatGrid {
    /// 一拍多少秒
    pub beat: f32,
    /// 第一拍落在哪一秒
    pub first: f32,
    /// 歌曲总长（秒）
    pub song_len: f32,
}

impl BeatGrid {
    /// 由 BPM 构建。
    pub fn from_bpm(bpm: f32, first: f32, song_len: f32) -> Self {
        let bpm = if bpm.is_finite() && bpm > 1.0 {
            bpm
        } else {
            120.0
        };
        Self {
            beat: 60.0 / bpm,
            first: if first.is_finite() { first } else { 0.0 },
            song_len: song_len.max(1.0),
        }
    }

    /// 默认网格（120 BPM，第一拍在 0）。
    pub fn default_grid(song_len: f32) -> Self {
        Self::from_bpm(120.0, 0.0, song_len)
    }

    /// 第 `n` 拍的绝对时间。
    pub fn beat_t(&self, n: f32) -> f32 {
        self.first + n * self.beat
    }

    /// `t` 时刻是第几拍（向下取整，可能为负）。
    pub fn beat_index(&self, t: f32) -> i64 {
        ((t - self.first) / self.beat + 1e-6).floor() as i64
    }

    /// 吸附到最近的**整拍**。
    pub fn snap(&self, t: f32) -> f32 {
        self.first + ((t - self.first) / self.beat).round() * self.beat
    }

    /// 吸附到最近的**半拍**。视觉动作比整拍更密时用这个。
    pub fn snap_half(&self, t: f32) -> f32 {
        let half = self.beat * 0.5;
        self.first + ((t - self.first) / half).round() * half
    }

    /// 吸附到 1/4 拍（十六分音符）。
    pub fn snap_quarter(&self, t: f32) -> f32 {
        let q = self.beat * 0.25;
        self.first + ((t - self.first) / q).round() * q
    }

    /// 距上一拍的时间差（永远 ≥ 0）。
    pub fn since_last_beat(&self, t: f32) -> f32 {
        let n = self.beat_index(t) as f32;
        (t - self.beat_t(n)).max(0.0)
    }

    /// 距下一拍的剩余时间。
    pub fn until_next_beat(&self, t: f32) -> f32 {
        (self.beat - self.since_last_beat(t)).max(0.0)
    }

    /// 节拍脉冲：刚过拍点为 1，之后指数衰减。
    pub fn pulse(&self, t: f32, decay: f32) -> f32 {
        let d = self.since_last_beat(t);
        let tau = if decay > 1e-4 { decay } else { 0.15 };
        (-d / tau).exp()
    }

    /// 拍内相位 0~1（0 = 正好在拍上）。
    pub fn phase(&self, t: f32) -> f32 {
        (self.since_last_beat(t) / self.beat).clamp(0.0, 1.0)
    }

    /// 是否正处在拍点附近（`window` 秒内）。
    pub fn on_beat(&self, t: f32, window: f32) -> bool {
        self.since_last_beat(t) <= window
    }

    /// 小节位置：返回 (第几小节, 小节内第几拍)。
    pub fn bar(&self, t: f32, beats_per_bar: i64) -> (i64, i64) {
        let bpb = beats_per_bar.max(1);
        let n = self.beat_index(t);
        let bar = n.div_euclid(bpb);
        let within = n.rem_euclid(bpb);
        (bar, within)
    }

    /// 每 4 拍一个重拍（强拍）的脉冲。
    pub fn downbeat_pulse(&self, t: f32, decay: f32) -> f32 {
        let (_, within) = self.bar(t, 4);
        if within != 0 {
            return 0.0;
        }
        self.pulse(t, decay)
    }

    /// 全曲节拍总数。
    pub fn beat_count(&self) -> i64 {
        self.beat_index(self.song_len) + 1
    }

    /// 把时间量化成「第几拍的第几分之几」，返回可用于确定性随机的整数。
    pub fn quantize_id(&self, t: f32, subdivisions: i64) -> i64 {
        let s = subdivisions.max(1) as f32;
        ((t - self.first) / (self.beat / s)).round() as i64
    }
}

/// 分段线性关键帧插值。
///
/// `pts` 必须按时间升序。超出范围时**夹到端点**（不外推），
/// 因为外推会让不存在的音域产生荒唐的数值。
pub fn keyframes(t: f32, pts: &[(f32, f32)]) -> f32 {
    if pts.is_empty() {
        return 0.0;
    }
    if pts.len() == 1 || t <= pts[0].0 {
        return pts[0].1;
    }
    for w in pts.windows(2) {
        let (t0, v0) = w[0];
        let (t1, v1) = w[1];
        if t <= t1 {
            let span = t1 - t0;
            if span.abs() < 1e-6 {
                return v1;
            }
            let u = (t - t0) / span;
            return v0 + (v1 - v0) * u;
        }
    }
    pts[pts.len() - 1].1
}

/// 平滑阶梯：在 `[t0, t1]` 之间从 0 平滑过渡到 1，之外夹紧。
pub fn smoothstep(t: f32, t0: f32, t1: f32) -> f32 {
    if (t1 - t0).abs() < 1e-6 {
        return if t >= t1 { 1.0 } else { 0.0 };
    }
    let u = ((t - t0) / (t1 - t0)).clamp(0.0, 1.0);
    u * u * (3.0 - 2.0 * u)
}

/// 缓出：起步快、收尾慢。
pub fn ease_out(u: f32) -> f32 {
    let u = u.clamp(0.0, 1.0);
    1.0 - (1.0 - u).powi(3)
}

/// 缓入：起步慢、收尾快。
pub fn ease_in(u: f32) -> f32 {
    let u = u.clamp(0.0, 1.0);
    u.powi(3)
}

/// 缓入缓出。
pub fn ease_in_out(u: f32) -> f32 {
    let u = u.clamp(0.0, 1.0);
    if u < 0.5 {
        4.0 * u * u * u
    } else {
        1.0 - (-2.0 * u + 2.0).powi(3) / 2.0
    }
}

/// 回弹：结尾处轻微过冲后回落，用于「落下」的动作。
pub fn ease_out_back(u: f32) -> f32 {
    let u = u.clamp(0.0, 1.0);
    let c1 = 1.70158f32;
    let c3 = c1 + 1.0;
    1.0 + c3 * (u - 1.0).powi(3) + c1 * (u - 1.0).powi(2)
}

/// 矩形窗口：在 `[t0, t1]` 内为 1，边缘 `fade` 秒内线性过渡。
pub fn window(t: f32, t0: f32, t1: f32, fade: f32) -> f32 {
    if t < t0 || t > t1 {
        return 0.0;
    }
    let f = fade.max(1e-3);
    let a = ((t - t0) / f).clamp(0.0, 1.0);
    let b = ((t1 - t) / f).clamp(0.0, 1.0);
    a.min(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beat_grid_from_bpm() {
        let g = BeatGrid::from_bpm(120.0, 0.0, 100.0);
        assert!((g.beat - 0.5).abs() < 1e-6);
        assert!((g.beat_t(4.0) - 2.0).abs() < 1e-6);
    }

    #[test]
    fn invalid_bpm_falls_back() {
        for bpm in [0.0, -5.0, f32::NAN, f32::INFINITY] {
            let g = BeatGrid::from_bpm(bpm, 0.0, 100.0);
            assert!(g.beat.is_finite() && g.beat > 0.0, "bpm={bpm}");
        }
    }

    #[test]
    fn beat_index_is_correct() {
        let g = BeatGrid::from_bpm(120.0, 0.0, 100.0);
        assert_eq!(g.beat_index(0.0), 0);
        assert_eq!(g.beat_index(0.49), 0);
        assert_eq!(g.beat_index(0.5), 1);
        assert_eq!(g.beat_index(1.0), 2);
        assert_eq!(g.beat_index(-0.1), -1);
    }

    #[test]
    fn snap_lands_on_beats() {
        let g = BeatGrid::from_bpm(120.0, 0.0, 100.0);
        assert!((g.snap(0.24) - 0.0).abs() < 1e-6);
        assert!((g.snap(0.26) - 0.5).abs() < 1e-6);
        // 半拍 = 0.25s，中点是 0.125：0.12 吸到 0，0.13 吸到 0.25
        assert!(
            (g.snap_half(0.12) - 0.0).abs() < 1e-6,
            "0.12 → {}",
            g.snap_half(0.12)
        );
        assert!(
            (g.snap_half(0.13) - 0.25).abs() < 1e-6,
            "0.13 → {}",
            g.snap_half(0.13)
        );
        assert!(
            (g.snap_half(0.37) - 0.25).abs() < 1e-6,
            "0.37 → {}",
            g.snap_half(0.37)
        );
        assert!(
            (g.snap_half(0.39) - 0.5).abs() < 1e-6,
            "0.39 → {}",
            g.snap_half(0.39)
        );
    }

    #[test]
    fn snap_half_is_finer_than_snap() {
        let g = BeatGrid::from_bpm(120.0, 0.0, 100.0);
        // 半拍吸附的误差不会超过整拍
        for i in 0..200 {
            let t = i as f32 * 0.017;
            let e1 = (g.snap(t) - t).abs();
            let e2 = (g.snap_half(t) - t).abs();
            assert!(e2 <= e1 + 1e-5, "t={t} half={e2} full={e1}");
        }
    }

    #[test]
    fn quarter_snap_is_even_finer() {
        let g = BeatGrid::from_bpm(120.0, 0.0, 100.0);
        for i in 0..200 {
            let t = i as f32 * 0.013;
            assert!((g.snap_quarter(t) - t).abs() <= 0.0625 + 1e-4, "t={t}");
        }
    }

    #[test]
    fn since_last_beat_is_nonnegative_and_bounded() {
        let g = BeatGrid::from_bpm(128.6, 0.1587, 211.9);
        for i in 0..5000 {
            let t = i as f32 * 0.05;
            let d = g.since_last_beat(t);
            assert!(d >= 0.0 && d < g.beat + 1e-4, "t={t} d={d}");
        }
    }

    #[test]
    fn since_and_until_sum_to_beat() {
        let g = BeatGrid::from_bpm(128.6, 0.1587, 211.9);
        for i in 0..2000 {
            let t = i as f32 * 0.07;
            let s = g.since_last_beat(t) + g.until_next_beat(t);
            assert!((s - g.beat).abs() < 1e-3, "t={t} sum={s} beat={}", g.beat);
        }
    }

    #[test]
    fn pulse_decays_from_one() {
        let g = BeatGrid::from_bpm(120.0, 0.0, 100.0);
        // 正好在拍上
        let p0 = g.pulse(0.0, 0.15);
        assert!((p0 - 1.0).abs() < 1e-4, "拍点应为 1: {p0}");
        // 半拍之后应该衰减很多
        let p1 = g.pulse(0.25, 0.15);
        assert!(p1 < p0, "应衰减: {p1}");
        assert!(p1 > 0.0);
    }

    #[test]
    fn pulse_is_bounded() {
        let g = BeatGrid::from_bpm(128.6, 0.0, 100.0);
        for i in 0..3000 {
            let t = i as f32 * 0.03;
            let p = g.pulse(t, 0.15);
            assert!((0.0..=1.0).contains(&p), "t={t} p={p}");
        }
    }

    #[test]
    fn phase_is_bounded() {
        let g = BeatGrid::from_bpm(128.6, 0.1, 100.0);
        for i in 0..3000 {
            let t = i as f32 * 0.03;
            let p = g.phase(t);
            assert!((0.0..=1.0).contains(&p), "t={t} p={p}");
        }
    }

    #[test]
    fn on_beat_detects_window() {
        let g = BeatGrid::from_bpm(120.0, 0.0, 100.0);
        assert!(g.on_beat(0.0, 0.05));
        assert!(g.on_beat(0.03, 0.05));
        assert!(!g.on_beat(0.3, 0.05));
    }

    #[test]
    fn bar_math_is_euclidean() {
        let g = BeatGrid::from_bpm(120.0, 0.0, 100.0);
        assert_eq!(g.bar(0.0, 4), (0, 0));
        assert_eq!(g.bar(0.5, 4), (0, 1));
        assert_eq!(g.bar(2.0, 4), (1, 0));
        // 负数不出错
        let (b, w) = g.bar(-1.0, 4);
        assert!(b <= 0 && (0..4).contains(&w));
    }

    #[test]
    fn downbeat_only_fires_on_bar_start() {
        let g = BeatGrid::from_bpm(120.0, 0.0, 100.0);
        assert!(g.downbeat_pulse(0.0, 0.15) > 0.5, "小节头应变强");
        assert_eq!(g.downbeat_pulse(0.5, 0.15), 0.0, "第二拍不该触发");
        assert_eq!(g.downbeat_pulse(1.5, 0.15), 0.0, "第三拍不该触发");
        assert!(g.downbeat_pulse(2.0, 0.15) > 0.5, "下一小节头应触发");
    }

    #[test]
    fn quantize_id_is_deterministic() {
        let g = BeatGrid::from_bpm(128.6, 0.1587, 211.9);
        assert_eq!(g.quantize_id(2.75, 4), g.quantize_id(2.75, 4));
        // 同一拍内不同时刻可能得到同一个 id
        let a = g.quantize_id(1.0, 4);
        let b = g.quantize_id(1.001, 4);
        assert_eq!(a, b);
    }

    #[test]
    fn beat_count_is_positive() {
        let g = BeatGrid::from_bpm(128.6, 0.0, 211.9);
        assert!(g.beat_count() > 400, "3 分半 128BPM 应有几百拍");
    }

    #[test]
    fn keyframes_interpolates() {
        let pts = [(0.0, 0.0), (1.0, 10.0), (2.0, 5.0)];
        assert!((keyframes(0.5, &pts) - 5.0).abs() < 1e-5);
        assert!((keyframes(1.5, &pts) - 7.5).abs() < 1e-5);
    }

    #[test]
    fn keyframes_clamps_outside_range() {
        let pts = [(1.0, 3.0), (2.0, 8.0)];
        assert!((keyframes(0.0, &pts) - 3.0).abs() < 1e-6, "不该外推");
        assert!((keyframes(99.0, &pts) - 8.0).abs() < 1e-6);
    }

    #[test]
    fn keyframes_handles_degenerate_input() {
        assert_eq!(keyframes(5.0, &[]), 0.0);
        assert_eq!(keyframes(5.0, &[(1.0, 7.0)]), 7.0);
        // 时间相同的两点不该除零
        let v = keyframes(1.0, &[(1.0, 2.0), (1.0, 9.0)]);
        assert!(v.is_finite());
    }

    #[test]
    fn smoothstep_is_smooth_at_ends() {
        assert!((smoothstep(-1.0, 0.0, 1.0) - 0.0).abs() < 1e-6);
        assert!((smoothstep(2.0, 0.0, 1.0) - 1.0).abs() < 1e-6);
        assert!((smoothstep(0.5, 0.0, 1.0) - 0.5).abs() < 1e-6);
        // 中点附近导数小，两端也小
        let d = smoothstep(0.05, 0.0, 1.0);
        assert!(d < 0.02, "起点应平滑: {d}");
    }

    #[test]
    fn smoothstep_handles_zero_width() {
        assert_eq!(smoothstep(0.0, 1.0, 1.0), 0.0);
        assert_eq!(smoothstep(1.0, 1.0, 1.0), 1.0);
        assert_eq!(smoothstep(2.0, 1.0, 1.0), 1.0);
    }

    #[test]
    fn easing_functions_are_bounded() {
        for i in 0..=100 {
            let u = i as f32 / 100.0;
            for v in [ease_out(u), ease_in(u), ease_in_out(u)] {
                assert!((0.0..=1.0).contains(&v), "u={u} v={v}");
            }
        }
    }

    #[test]
    fn ease_in_out_symmetric() {
        for i in 0..=50 {
            let u = i as f32 / 100.0;
            let a = ease_in_out(u);
            let b = 1.0 - ease_in_out(1.0 - u);
            assert!((a - b).abs() < 1e-4, "u={u} {a} vs {b}");
        }
    }

    #[test]
    fn ease_out_back_overshoots() {
        let mid = ease_out_back(0.7);
        assert!(mid > 1.0, "应有回弹过冲: {mid}");
        assert!((ease_out_back(1.0) - 1.0).abs() < 1e-4, "结尾应归位");
    }

    #[test]
    fn window_fades_at_edges() {
        assert_eq!(window(-1.0, 0.0, 2.0, 0.5), 0.0);
        assert_eq!(window(3.0, 0.0, 2.0, 0.5), 0.0);
        assert!((window(1.0, 0.0, 2.0, 0.5) - 1.0).abs() < 1e-6);
        assert!(window(0.0, 0.0, 2.0, 0.5) < 0.01);
        assert!(window(0.25, 0.0, 2.0, 0.5) > 0.4);
    }

    #[test]
    fn all_outputs_finite_across_song() {
        let g = BeatGrid::from_bpm(128.5714, 0.1587, 211.913);
        for i in 0..10000 {
            let t = i as f32 * 0.021;
            assert!(g.beat_t(i as f32).is_finite());
            assert!(g.snap(t).is_finite());
            assert!(g.snap_half(t).is_finite());
            assert!(g.pulse(t, 0.15).is_finite());
            assert!(g.phase(t).is_finite());
            assert!(g.since_last_beat(t).is_finite());
            assert!(g.until_next_beat(t).is_finite());
        }
    }
}

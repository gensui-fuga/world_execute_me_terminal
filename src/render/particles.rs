//! 粒子系统：位置 / 速度 / 加速度 / 寿命 / 字符 / 颜色，带 3D 透视投影。
//!
//! 容量固定 + 复用槽位，长时间播放不会增长内存 ——
//! 这是「200×60 稳定 60fps、无内存增长」这条验收标准的直接实现。
//! 需要新粒子时优先填死槽；全满则淘汰剩余寿命最短的那个。

use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};
use ratatui::style::Color;

use crate::render::color as col;
use crate::render::CharCanvas;

/// 单个粒子。
#[derive(Clone, Copy, Debug)]
pub struct Particle {
    /// 世界坐标（子像素单位）
    pub x: f32,
    pub y: f32,
    /// 深度：0 最近，越大越远
    pub z: f32,
    pub vx: f32,
    pub vy: f32,
    pub vz: f32,
    /// 剩余寿命（秒）
    pub life: f32,
    /// 初始寿命（秒），用于算归一化年龄
    pub max_life: f32,
    /// 显示字符
    pub ch: char,
    /// 基色
    pub color: Color,
    /// 尺寸倍率（影响亮度与字符选择）
    pub size: f32,
    /// 额外重力（子像素/秒²），叠加系统重力
    pub gravity: f32,
    /// 阻尼系数（每秒保留比例）
    pub drag: f32,
}

impl Default for Particle {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            vx: 0.0,
            vy: 0.0,
            vz: 0.0,
            life: 0.0,
            max_life: 1.0,
            ch: '*',
            color: col::CYAN,
            size: 1.0,
            gravity: 0.0,
            drag: 0.98,
        }
    }
}

impl Particle {
    /// 归一化年龄（0 刚生成，1 将消亡）。
    pub fn age(&self) -> f32 {
        if self.max_life <= 0.0 {
            return 1.0;
        }
        (1.0 - self.life / self.max_life).clamp(0.0, 1.0)
    }

    /// 是否还活着。
    pub fn alive(&self) -> bool {
        self.life > 0.0
    }

    /// 寿命剩余比例（1 → 0）。
    pub fn fade(&self) -> f32 {
        if self.max_life <= 0.0 {
            return 0.0;
        }
        (self.life / self.max_life).clamp(0.0, 1.0)
    }
}

/// 粒子池。
pub struct ParticleSystem {
    parts: Vec<Particle>,
    rng: SmallRng,
    /// 视距：z 每增加 `focal`，视觉尺寸减半
    pub focal: f32,
    /// 系统重力（子像素/秒²）
    pub gravity: f32,
    /// 边界（子像素）
    pub sw: f32,
    pub sh: f32,
    /// 撞边界是否反弹
    pub bounce: bool,
    /// 累计生成数（调试用）
    pub spawned: u64,
}

impl ParticleSystem {
    /// 新建粒子池。`capacity` 是硬上限。
    pub fn new(seed: u64, capacity: usize) -> Self {
        let capacity = capacity.clamp(16, 65_536);
        Self {
            parts: Vec::with_capacity(capacity),
            rng: SmallRng::seed_from_u64(seed),
            focal: 120.0,
            gravity: 0.0,
            sw: 0.0,
            sh: 0.0,
            bounce: false,
            spawned: 0,
        }
    }

    /// 设定边界。
    pub fn set_bounds(&mut self, sw: f32, sh: f32) {
        self.sw = sw;
        self.sh = sh;
    }

    /// 活着的粒子数。
    pub fn len(&self) -> usize {
        self.parts.iter().filter(|p| p.alive()).count()
    }

    /// 池是否为空。
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// 池容量。
    pub fn capacity(&self) -> usize {
        self.parts.capacity().max(16)
    }

    /// 清空。
    pub fn clear(&mut self) {
        self.parts.clear();
    }

    /// 加入一个粒子。池满时淘汰剩余寿命最短者。
    pub fn spawn(&mut self, p: Particle) {
        if self.parts.len() < self.capacity() {
            self.parts.push(p);
        } else if let Some(i) = self
            .parts
            .iter()
            .enumerate()
            .min_by(|a, b| {
                a.1.life
                    .partial_cmp(&b.1.life)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(i, _)| i)
        {
            self.parts[i] = p;
        }
        self.spawned = self.spawned.wrapping_add(1);
    }

    /// 随机数发生器（供场景自定义发射逻辑）。
    pub fn rng(&mut self) -> &mut SmallRng {
        &mut self.rng
    }

    /// 环形爆发：向四周均匀撒 `n` 个粒子。
    pub fn burst(
        &mut self,
        x: f32,
        y: f32,
        n: usize,
        speed: f32,
        color: Color,
        life: f32,
        ramp: &[char],
    ) {
        for _ in 0..n {
            let a: f32 = self.rng.gen_range(0.0..std::f32::consts::TAU);
            let s = speed * self.rng.gen_range(0.4..1.0);
            let ch = if ramp.is_empty() {
                '*'
            } else {
                ramp[self.rng.gen_range(0..ramp.len())]
            };
            self.spawn(Particle {
                x,
                y,
                vx: a.cos() * s,
                vy: a.sin() * s,
                life,
                max_life: life,
                ch,
                color,
                ..Default::default()
            });
        }
    }

    /// 定向喷射（角度 `dir` 弧度，张角 `spread`）。
    pub fn jet(
        &mut self,
        x: f32,
        y: f32,
        dir: f32,
        spread: f32,
        n: usize,
        speed: f32,
        color: Color,
        life: f32,
        ramp: &[char],
    ) {
        for _ in 0..n {
            // spread 为 0 时 gen_range(-0.0..0.0) 会 panic（空范围）
            let a = if spread.abs() < 1e-6 {
                dir
            } else {
                dir + self.rng.gen_range(-spread..spread)
            };
            let s = speed * self.rng.gen_range(0.5..1.2);
            let ch = if ramp.is_empty() {
                '*'
            } else {
                ramp[self.rng.gen_range(0..ramp.len())]
            };
            self.spawn(Particle {
                x,
                y,
                vx: a.cos() * s,
                vy: a.sin() * s,
                life,
                max_life: life,
                ch,
                color,
                ..Default::default()
            });
        }
    }

    /// 在矩形区域内随机撒点（星空/尘埃）。
    pub fn scatter(
        &mut self,
        x0: f32,
        y0: f32,
        w: f32,
        h: f32,
        n: usize,
        color: Color,
        life: f32,
        ramp: &[char],
    ) {
        for _ in 0..n {
            let ch = if ramp.is_empty() {
                '.'
            } else {
                ramp[self.rng.gen_range(0..ramp.len())]
            };
            let x = self.rng.gen_range(x0..x0 + w);
            let y = self.rng.gen_range(y0..y0 + h);
            let z = self.rng.gen_range(0.0..200.0);
            let vz = self.rng.gen_range(-8.0..8.0);
            let size = self.rng.gen_range(0.5..1.2);
            self.spawn(Particle {
                x,
                y,
                z,
                vz,
                life,
                max_life: life,
                ch,
                color,
                size,
                ..Default::default()
            });
        }
    }

    /// 推进一帧。`dt` 秒。
    pub fn update(&mut self, dt: f32) {
        let dt = dt.clamp(0.0, 0.1);
        for p in self.parts.iter_mut() {
            if !p.alive() {
                continue;
            }
            p.life -= dt;
            // 加速度
            p.vy += (self.gravity + p.gravity) * dt;
            // 阻尼按指数衰减，与帧率无关
            let d = p.drag.clamp(0.0, 1.0).powf(dt * 60.0);
            p.vx *= d;
            p.vy *= d;
            p.vz *= d;
            // 积分
            p.x += p.vx * dt;
            p.y += p.vy * dt;
            p.z += p.vz * dt;
            if p.z < 0.0 {
                p.z = 0.0;
                p.vz = p.vz.abs() * 0.5;
            }
            // 边界
            if self.bounce {
                if p.x < 0.0 {
                    p.x = 0.0;
                    p.vx = p.vx.abs();
                } else if p.x > self.sw {
                    p.x = self.sw;
                    p.vx = -p.vx.abs();
                }
                if p.y < 0.0 {
                    p.y = 0.0;
                    p.vy = p.vy.abs();
                } else if p.y > self.sh {
                    p.y = self.sh;
                    p.vy = -p.vy.abs();
                }
            }
        }
        // 回收死亡粒子（保持池大小有界）
        if self.parts.iter().any(|p| !p.alive()) {
            self.parts.retain(|p| p.alive());
        }
    }

    /// 画到画布。`cam_z` 是相机距离（越大越像长焦）。
    pub fn draw(&self, canvas: &mut CharCanvas) {
        let cx = canvas.sw as f32 / 2.0;
        let cy = canvas.sh as f32 / 2.0;
        for p in self.parts.iter() {
            if !p.alive() {
                continue;
            }
            // 透视投影：z 越大越靠近消失点
            let k = self.focal / (self.focal + p.z.max(0.0));
            let sx = cx + (p.x - cx) * k;
            let sy = cy + (p.y - cy) * k;
            if sx < 0.0 || sy < 0.0 || sx >= canvas.sw as f32 || sy >= canvas.sh as f32 {
                continue;
            }
            // 亮度随寿命与深度衰减
            let fade = p.fade();
            let w = (fade.powf(0.6) * k * p.size).clamp(0.0, 1.0);
            if w <= 0.01 {
                continue;
            }
            // 后段偏色：让消亡中的粒子更红/更暗
            let c = if fade > 0.5 {
                p.color
            } else {
                col::lerp(p.color, col::DARK_RED, (0.5 - fade) * 2.0 * 0.7)
            };
            if p.ch == ' ' {
                canvas.set(sx, sy, c, w);
            } else {
                // 字符粒子：先铺一层亮像素，字符由聚合阶段决定
                canvas.set(sx, sy, c, w);
            }
        }
    }

    /// 逐粒子的字符渲染（ASCII 模式下用字符表达粒子形状）。
    ///
    /// 与 `draw` 的区别：这里把粒子当成一个「字符单元」直接写入，
    /// 适合代码雨这类需要保留字符形状的场景。
    pub fn draw_glyphs(&self, canvas: &mut CharCanvas, glyphs: bool) {
        if !glyphs {
            self.draw(canvas);
            return;
        }
        for p in self.parts.iter() {
            if !p.alive() || p.ch == ' ' {
                continue;
            }
            let k = self.focal / (self.focal + p.z.max(0.0));
            let cx = canvas.sw as f32 / 2.0;
            let cy = canvas.sh as f32 / 2.0;
            let sx = cx + (p.x - cx) * k;
            let sy = cy + (p.y - cy) * k;
            // 用一个 2×2 的方块近似一个字符的占位
            let w = (p.fade().powf(0.6) * k).clamp(0.0, 1.0);
            canvas.set(sx, sy, p.color, w);
            canvas.set(sx + 1.0, sy, p.color, w * 0.6);
        }
    }

    /// 调试信息。
    pub fn stats(&self) -> (usize, u64) {
        (self.len(), self.spawned)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RenderMode;

    #[test]
    fn new_pool_is_empty() {
        let ps = ParticleSystem::new(1, 128);
        assert!(ps.is_empty());
        assert_eq!(ps.len(), 0);
    }

    #[test]
    fn spawn_increases_count() {
        let mut ps = ParticleSystem::new(1, 128);
        ps.spawn(Particle {
            life: 1.0,
            max_life: 1.0,
            ..Default::default()
        });
        assert_eq!(ps.len(), 1);
    }

    #[test]
    fn update_expires_particles() {
        let mut ps = ParticleSystem::new(1, 128);
        ps.spawn(Particle {
            life: 0.05,
            max_life: 0.05,
            ..Default::default()
        });
        ps.update(0.1);
        assert_eq!(ps.len(), 0);
    }

    #[test]
    fn capacity_is_never_exceeded() {
        let mut ps = ParticleSystem::new(1, 16);
        for _ in 0..500 {
            ps.spawn(Particle {
                life: 10.0,
                max_life: 10.0,
                ..Default::default()
            });
        }
        assert!(
            ps.len() <= ps.capacity(),
            "池溢出: {} > {}",
            ps.len(),
            ps.capacity()
        );
        assert!(ps.parts.len() <= ps.capacity());
    }

    #[test]
    fn burst_creates_requested_count() {
        let mut ps = ParticleSystem::new(7, 512);
        ps.burst(10.0, 10.0, 40, 20.0, col::CYAN, 1.0, &['*', '+']);
        assert_eq!(ps.len(), 40);
    }

    #[test]
    fn jet_is_directional() {
        let mut ps = ParticleSystem::new(3, 512);
        // 向正右方喷射，张角 0
        ps.jet(0.0, 0.0, 0.0, 0.0, 20, 50.0, col::CYAN, 5.0, &['*']);
        for p in ps.parts.iter() {
            assert!(p.vx > 0.0, "速度方向错误 vx={}", p.vx);
            assert!(p.vy.abs() < 1e-3);
        }
    }

    #[test]
    fn gravity_pulls_downward() {
        let mut ps = ParticleSystem::new(1, 32);
        ps.gravity = 100.0;
        ps.spawn(Particle {
            x: 0.0,
            y: 0.0,
            life: 10.0,
            max_life: 10.0,
            drag: 1.0,
            ..Default::default()
        });
        ps.update(0.1);
        assert!(ps.parts[0].vy > 0.0, "重力应让 vy 变正");
        assert!(ps.parts[0].y > 0.0, "粒子应向下移动");
    }

    #[test]
    fn bounce_keeps_particles_in_bounds() {
        let mut ps = ParticleSystem::new(1, 64);
        ps.set_bounds(100.0, 50.0);
        ps.bounce = true;
        ps.gravity = 200.0;
        for _ in 0..50 {
            ps.spawn(Particle {
                x: 50.0,
                y: 25.0,
                vx: 400.0,
                vy: 400.0,
                life: 10.0,
                max_life: 10.0,
                drag: 1.0,
                ..Default::default()
            });
        }
        for _ in 0..100 {
            ps.update(0.016);
        }
        for p in ps.parts.iter() {
            assert!((0.0..=100.0).contains(&p.x), "x 越界 {}", p.x);
            assert!((0.0..=50.0).contains(&p.y), "y 越界 {}", p.y);
        }
    }

    #[test]
    fn no_bounce_allows_escape() {
        let mut ps = ParticleSystem::new(1, 8);
        ps.set_bounds(100.0, 100.0);
        ps.bounce = false;
        ps.spawn(Particle {
            x: 0.0,
            y: 0.0,
            vx: -1000.0,
            life: 10.0,
            max_life: 10.0,
            drag: 1.0,
            ..Default::default()
        });
        ps.update(0.1);
        assert!(ps.parts[0].x < 0.0);
    }

    #[test]
    fn draw_marks_canvas_pixels() {
        let mut ps = ParticleSystem::new(1, 32);
        ps.set_bounds(20.0, 20.0);
        ps.spawn(Particle {
            x: 10.0,
            y: 10.0,
            life: 1.0,
            max_life: 1.0,
            z: 0.0,
            color: col::WHITE,
            ..Default::default()
        });
        let mut c = CharCanvas::new(10, 10, RenderMode::Braille);
        ps.draw(&mut c);
        assert!(!c.pixel(10, 10).is_empty());
    }

    #[test]
    fn far_particles_draw_dimmer() {
        let mut near = ParticleSystem::new(1, 8);
        near.spawn(Particle {
            x: 10.0,
            y: 10.0,
            z: 0.0,
            life: 1.0,
            max_life: 1.0,
            color: col::WHITE,
            ..Default::default()
        });
        let mut far = ParticleSystem::new(1, 8);
        far.spawn(Particle {
            x: 10.0,
            y: 10.0,
            z: 500.0,
            life: 1.0,
            max_life: 1.0,
            color: col::WHITE,
            ..Default::default()
        });
        let mut a = CharCanvas::new(10, 10, RenderMode::Braille);
        let mut b = CharCanvas::new(10, 10, RenderMode::Braille);
        near.draw(&mut a);
        far.draw(&mut b);
        assert!(a.pixel(10, 10).w > b.pixel(10, 10).w);
    }

    #[test]
    fn fade_goes_from_one_to_zero() {
        let p = Particle {
            life: 2.0,
            max_life: 4.0,
            ..Default::default()
        };
        assert!((p.fade() - 0.5).abs() < 1e-6);
        assert!((p.age() - 0.5).abs() < 1e-6);
    }

    #[test]
    fn scatter_fills_requested_region() {
        let mut ps = ParticleSystem::new(5, 512);
        ps.scatter(0.0, 0.0, 50.0, 30.0, 60, col::GREY_WHITE, 2.0, &['.']);
        assert_eq!(ps.len(), 60);
        for p in ps.parts.iter() {
            assert!((0.0..=50.0).contains(&p.x));
            assert!((0.0..=30.0).contains(&p.y));
        }
    }

    #[test]
    fn pool_does_not_grow_over_long_run() {
        let mut ps = ParticleSystem::new(9, 256);
        ps.set_bounds(100.0, 100.0);
        let cap = ps.capacity();
        for frame in 0..600 {
            ps.burst(50.0, 50.0, 20, 30.0, col::CYAN, 0.5, &['*']);
            ps.update(1.0 / 60.0);
            assert!(
                ps.parts.len() <= cap,
                "第 {frame} 帧池大小 {} 超过容量 {cap}",
                ps.parts.len()
            );
        }
    }

    #[test]
    fn stats_reports_spawned_total() {
        let mut ps = ParticleSystem::new(1, 64);
        ps.burst(0.0, 0.0, 5, 1.0, col::CYAN, 1.0, &['*']);
        ps.burst(0.0, 0.0, 7, 1.0, col::CYAN, 1.0, &['*']);
        let (alive, spawned) = ps.stats();
        assert_eq!(alive, 12);
        assert_eq!(spawned, 12);
    }

    #[test]
    fn clear_removes_everything() {
        let mut ps = ParticleSystem::new(1, 64);
        ps.burst(0.0, 0.0, 10, 1.0, col::CYAN, 1.0, &['*']);
        ps.clear();
        assert!(ps.is_empty());
        assert_eq!(ps.parts.len(), 0);
    }

    #[test]
    fn update_is_frame_rate_independent_for_drag() {
        // 两次 1/120 秒与一次 1/60 秒的阻尼结果应接近
        let mut a = ParticleSystem::new(1, 4);
        let mut b = ParticleSystem::new(1, 4);
        let mk = || Particle {
            vx: 100.0,
            life: 10.0,
            max_life: 10.0,
            drag: 0.9,
            ..Default::default()
        };
        a.spawn(mk());
        b.spawn(mk());
        a.update(1.0 / 120.0);
        a.update(1.0 / 120.0);
        b.update(1.0 / 60.0);
        let (va, vb) = (a.parts[0].vx, b.parts[0].vx);
        assert!((va - vb).abs() / vb < 0.05, "阻尼与帧率相关: {va} vs {vb}");
    }

    #[test]
    fn z_never_goes_negative() {
        let mut ps = ParticleSystem::new(1, 4);
        ps.spawn(Particle {
            z: 5.0,
            vz: -100.0,
            life: 10.0,
            max_life: 10.0,
            ..Default::default()
        });
        for _ in 0..50 {
            ps.update(0.05);
        }
        assert!(ps.parts[0].z >= 0.0);
    }
}

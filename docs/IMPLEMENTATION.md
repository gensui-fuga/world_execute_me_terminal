# 实现指南（技术细节）

本文是 `BRIEF.md`（为什么）和 `STORYBOARD.md`（做什么）之后的**怎么做**。
每条技术都标注了来源和具体参数。

---

## 一、渲染架构

### 1.1 三层结构

借鉴同人 PV 的三层架构（世界层 / 界面层 / 窗口外壳）：

```
世界层（Scene）     ← 几何、粒子、代码雨、狐狸
界面层（UI）        ← 标题栏、数据面板、指令流、token 条
窗口外壳（Frame）   ← 边框、四角、状态栏
```

**顺序**：世界 → 界面 → 外壳。外壳永远在最上，所以边框不会被内容盖住。

### 1.2 渲染管线

```
analyze(t) → SceneCtx
    ↓
Scene.render(ctx)       世界层
    ↓
Layers.composite()      合成
    ↓
PostFx.apply()          后处理
    ↓
to_buffer()             Braille → ratatui Buffer
```

### 1.3 后处理顺序（顺序不能换）

```
残影 → 泛光 → 色差 → 扫描线 → 抖动 → 暗角 → 故障
```

**为什么这个顺序**：
- 残影必须最先（后面每一步都会再叠一层，顺序错了暗部会越沉越深）
- 泛光要在色差前（否则色差把辉光拆开）
- 故障最后（它是整行平移，会破坏其他效果）

---

## 二、Braille 渲染

**来源**：MapSCII 渲染管线分析、dsh-mv-cli 场景指南

### 2.1 位映射

每个 Braille 字符是 **2×4 = 8 个点**：

```
位置      左列    右列
第1行     0x01    0x08
第2行     0x02    0x10
第3行     0x04    0x20
第4行     0x40    0x80
```

Rust 数组：
```rust
const BRAILLE: [[u8; 2]; 4] = [
    [0x01, 0x08],
    [0x02, 0x10],
    [0x04, 0x20],
    [0x40, 0x80],
];
```

### 2.2 覆盖率与点亮阈值

**本项目的关键决定**：点只在 `cov > 0.5` 时点亮。

**为什么**：如果按 `cov > 0` 点亮，任何淡入淡出都会让整屏变实心墙。
这是"画面变黑"和"画面糊成一片"两个 bug 的共同根因。

### 2.3 覆盖率语义（不可违反）

- `Pixel::is_empty()` 定义为 `w <= 0.0`
- **任何后处理都不能对 `w` 做乘法**——那会静默擦掉细线
- 颜色调整只能碰 RGB

```rust
// ❌ 错：会擦掉细线
p.w *= dim;

// ✅ 对：只压暗颜色
p.r *= dim; p.g *= dim; p.b *= dim;

// ✅ 对：用 tint
tint(&mut p, factor);
```

### 2.4 多边形填充（尚未实现，需要时按此做）

**来源**：MapSCII

```
1. 三角化（Earcut）
2. 对每个三角形求三条边（Bresenham）
3. 合并所有边点，按 y 再按 x 排序
4. 填充同一个 y 上相邻边点之间的横向跨度
```

简化版（凸多边形够用）：
```
对每一行 y：
    求扫描线与多边形所有边的交点 x
    排序后两两配对，填充区间
```

### 2.5 圆与极坐标（高宽比补偿）

字符格子**高约为宽的两倍**。所以画圆必须：

```rust
// x 方向乘 2 补偿
let px = cx + cos(a) * r * 2.0;
let py = cy + sin(a) * r;
```

**这是"圆看起来是椭圆"的唯一原因。**

---

## 三、确定性

**来源**：genart-skill `determinism.md`

### 3.1 三条铁律

1. **不用 `rand` 的全局熵**——用 seeded `SmallRng`
2. **不用浮点三角函数做哈希**——`fract(sin(x)*43758)` 跨平台发散
3. **PRNG 必须预热**——丢弃前 12 次输出（未稀释的种子前几次抽样会相关）

### 3.2 哈希的正确写法

```rust
// ✅ 整数混合（本项目用的大常数）
fn hash_u32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x
}

// ❌ 不要这样（跨平台发散）
let h = (x as f32 * 12.9898).sin() * 43758.5453;
```

### 3.3 具名子流

不同用途用不同种子前缀，**改一个不影响其他**：

```rust
let stars    = SmallRng::seed_from_u64(seed ^ 0x5TARS);
let rain     = SmallRng::seed_from_u64(seed ^ 0xRA1N);
let particles= SmallRng::seed_from_u64(seed ^ 0xPART);
```

### 3.4 seek 时的重置

```rust
pub fn seek(&mut self, t: f64) {
    // 重新播种，保证同一 t 总是同一画面
    self.rng = SmallRng::seed_from_u64(0x5EED_1077 ^ (t * 1000.0) as u64);
    self.particles.clear();
    self.postfx.clear_trail();
}
```

### 3.5 残影必须是纯函数

**❌ 错误做法**（缓存上一帧）：
```rust
// 违反纯函数：frame(t) 依赖 frame(t-1)
prev = current * decay + prev * (1-decay);
```

**✅ 正确做法**（重画更早时刻）：
```rust
// 把场景在 t-0.24 和 t-0.12 各画一遍，一遍比一遍暗
for (dt, alpha) in [(0.24, 0.15), (0.12, 0.35)] {
    draw_scene(ctx_at(t - dt), canvas, alpha);
}
draw_scene(ctx_at(t), canvas, 1.0);
```

**来源**：dsh-mv-cli `post-effects.scene.js` 的 `withTrails()`

---

## 四、频谱处理

**来源**：cava 的 `integral + gravity` 双滤波器

### 4.1 问题

> 原始可视化信号噪声很大，直接画出来会乱闪。

### 4.2 算法

```c
// cava 原版
gravity_mod  = pow(framerate_mod, 2.5) * 2 / noise_reduction;
integral_mod = pow(framerate_mod, 0.1);

if (out[n] < prev[n])
    out[n] = peak[n] * (1.0 - fall[n]*fall[n]*gravity_mod);  // 下降：指数衰减
out[n] = mem[n] * noise_reduction / integral_mod + out[n];   // 积分：混历史
```

**核心思想**：
- **上升跟瞬时值**（保证打击感不迟钝）
- **下降用指数衰减**（避免抖动）

### 4.3 本项目的实现

保持纯函数（用 `ctx.prev` 而非帧间累积状态）：

```rust
fn gravity_smoothed(ctx: &SceneCtx, idx: usize) -> f32 {
    let cur = ctx.feat.spectrum[idx].clamp(0.0, 1.0);
    let prev = ctx.prev.spectrum[idx].clamp(0.0, 1.0);
    let dt = 1.0 / 60.0;
    let gravity = 3.5;   // 约 0.29 秒落到底
    let integral = 0.35;

    if cur >= prev {
        prev + (cur - prev) * (1.0 - (-integral * 12.0 * dt).exp())
    } else {
        prev * (-gravity * dt).exp()
    }
}
```

---

## 五、频段映射

### 5.1 六个频段

| 索引 | 名称 | 范围 | 视觉职责 |
|---|---|---|---|
| 0 | sub | 20–60 Hz | **整体缩放、边框呼吸** |
| 1 | bass | 60–250 Hz | **粒子速度、旋涡半径** |
| 2 | lowmid | 250–500 Hz | 有机图形生长 |
| 3 | mid | 500–2000 Hz | 波形振幅、旋转速度 |
| 4 | highmid | 2000–4000 Hz | 碎片抖动、细碎粒子 |
| 5 | high | 4000–20000 Hz | 星野闪烁、数字跳动 |

**来源**：audio-reactive visualizer 的通用映射原则

### 5.2 节拍

- BPM 130，第一拍 0.1587s
- `pulse` 在拍点为 1 并迅速衰减
- **闪烁频率 ≤ 每拍一次**（超过就是乱闪，反而廉价）

---

## 六、动画

**来源**：12 原则动画

### 6.1 缓动函数（必须用，不能线性）

```rust
// 已实现于 timeline/beat.rs
smoothstep(x, a, b)     // S 形，最通用
ease_out(t)             // 快进慢出，UI 元素
ease_in(t)              // 慢进快出，消失
ease_in_out(t)          // 两头慢
ease_out_back(t)        // 回弹，强调

// 建议补充
power4_out(t)           // 系统启动：干脆利落
expo_in_out(t)          // 情感流露：有呼吸感
elastic_out(t, amp, p)  // 崩溃瞬间：震颤
```

### 6.2 入场/离场原则

**来源**：同人 PV `DESIGN.md`

> **入场不是出现，而是诞生**——从虚空中凝聚，不是 fade in
> **离场不是消失，而是消散**——碎裂、溶解、被吸入深渊
> **静态也要有生命**——微微的浮动、呼吸般的 scale pulse

### 6.3 预动画（关键技巧）

动画**不是在关键词出现时才开始**：

```
"If I'm a circle"      → 圆开始绘制（30% 弧）
"CIRCUMFERENCE"        → 圆闭合（高潮！）
```

实现方式：把"引导词时间"和"关键词时间"都从 `WordTimeline` 取出来，
`progress` 在两者之间插值。

---

## 七、布局

### 7.1 自适应规则

| 尺寸 | 布局 |
|---|---|
| < 72 列 | 单栏（不分左右面板） |
| ≥ 72 列 | 左 62%（狐狸）+ 右 38%（数据） |
| < 60 列 或 < 18 行 | 隐藏装饰边框，只留主体与歌词 |

### 7.2 元素密度（选方案 3）

**来源**：genart-skill `resolution.md`

> 5000 个点在 400px 画布上填满，在 4000px 上消失。

三种策略：
1. 固定数量
2. 固定数量 + 尺寸缩放
3. **数量随面积缩放** ← 本项目选这个

```rust
let particles = (cols * rows) / 40;      // 粒子
let stars     = (sw * sh) / density;     // 星野，density 随能量变化
```

**理由**：终端尺寸从 60×18 到 240×85 变化巨大，大终端就该显示更多内容。

---

## 八、性能预算

| 项目 | 预算 | 实测 |
|---|---|---|
| 每帧渲染 | < 10 ms | **0.34 ms** |
| 帧上限 | 40 ms | — |
| 导出（160×50） | — | 10.7 s / 600 帧 |

### 8.1 优化要点

- **避免每格内再套循环**（O(格数 × 对象数)）
- 字符串拼接先二维数组，最后 join 一次
- PNG 用 `Fast` + `Up` 滤波
- 光栅化：**整图一次填背景** + 仅非默认色单元单独填充

### 8.2 已修复的性能问题

- 背景预填（每格都填 1M 像素写入 → 一次填充）
- PNG 编码（默认压缩 → Fast）
- 三维性能剖析发现：**渲染只占 0.34ms，瓶颈在光栅化和编码**

---

## 九、调试方法

### 9.1 从 PNG 还原字符画

```python
from PIL import Image
import numpy as np
a = np.array(Image.open('frame.png').convert('L'))
H, W = 40, 120   # 字符行列
for cy in range(H):
    row = ''
    for cx in range(W):
        bits = 0
        for sy in range(4):
            for sx in range(2):
                v = a[cy*16+sy*4:cy*16+(sy+1)*4, cx*8+sx*4:cx*8+(sx+1)*4].mean()
                if v > 60: bits |= 1 << (sy*2+sx)
        row += chr(0x2800 + bits) if bits else ' '
    print(row.rstrip())
```

### 9.2 客观指标（比视觉模型可靠）

```python
# 非黑覆盖率
print(f'{(a>20).sum()/a.size*100:.2f}%')
# 峰值亮度
print(a.max())
```

**教训**：`vision_analyze` 和外部视觉模型在这次项目中给出过**错误的判断**
（说"没有心形、85% 是空的"，被像素统计推翻）。
**优先用数字，视觉模型只做参考。**

### 9.3 QA 必看时间点

- 0 秒
- 每个段落的中间
- **每次段落切换前后 0.3 秒**
- 最后 2 秒

---

## 十、已知的坑（本项目实战）

| 坑 | 根因 | 修法 |
|---|---|---|
| 细线消失 | postfx 对 `w` 做乘法 | 只碰 RGB |
| 画面一片黑 | 三层叠加变暗 + 暗色主色 | 暗角设下限、换亮主色 |
| 代码雨不流动 | `rng.gen_range()` 在循环里 | 确定性哈希 |
| 狐狸不像狐狸 | 耳朵用椭圆插值 | 独立尖三角 |
| 章节色调跳变 | 两侧用不同相对量 | 有符号距离统一公式 |
| 能量看不出 | 星野密度是常量 | 密度随能量 |
| 画面空 | 只有 2 个区域 | 扩到 4 个区域 + 指令流 |
| 拖影破坏纯函数 | 缓存 prev | 重画更早时刻 |

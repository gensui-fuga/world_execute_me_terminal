# 春・夏・秋・冬 — 终端音乐 PV

用 **纯字符** 在终端里逐帧渲染一支音乐 MV 的 Rust 程序。

曲子是 **《春・夏・秋・冬》**（Junia Brutus 词曲 / 重音テト 演唱，2025-11-28）。画面只画歌词所唱之物：苔の産毛に日が射す春、乱レ玉の夏、月に雁の秋、枯レ野原の冬——四季流转，直到「決シテ錆ビツキハシナイ」的昇華。

```
  ⠀⠀⠀⣀⣤⣶⣿⣿⣶⣤⣀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⣀⣤⣶⣿⣿⣶⣤⣀
  ⠀⣴⣿⣿⣿⣿⣿⣿⣿⣿⣿⣷⣄⠀⠀⠀⠀⣠⣾⣿⣿⣿⣿⣿⣿⣿⣿⣿⣦
  ⣾⣿⣿⠟⠋⠁⠀⠉⠙⠻⣿⣿⣿⣦⣀⣴⣿⣿⣿⠟⠋⠁⠀⠈⠙⠻⣿⣿⣷
  ⣿⣿⡇⠀⠀⠀⠀⠀⠀⠀⢹⣿⣿⣿⣿⣿⣿⡏⠀⠀⠀⠀⠀⠀⠀⢸⣿⣿
  ⠹⣿⣷⣄⠀⠀⠀⠀⠀⣠⣿⣿⡿⠋⠙⢿⣿⣿⣄⠀⠀⠀⠀⠀⣠⣾⣿⠏
  ⠀⠙⢿⣿⣿⣶⣤⣤⣶⣿⣿⡿⠋⠀⠀⠀⠙⢿⣿⣿⣶⣤⣤⣶⣿⣿⡿⠋
  ⠀⠀⠀⠈⠛⠿⣿⣿⣿⠿⠋⠀⠀⠀⠀⠀⠀⠀⠙⠿⣿⣿⣿⠿⠛⠁
```

---

## 它是什么

一支 MV 的每一帧都是 **歌曲时间的纯函数**：`frame(t) = f(t, 音乐特征)`。

- **没有时间轴状态机** —— seek 到任意位置都能立刻得到正确画面
- **没有预渲染缓存** —— 每一帧现算
- **没有图片素材** —— 全部由 Braille 点阵、半块字符与几何图元拼出来
- **实时与导出共用同一套场景代码** —— 终端里看到什么，导出的 MP4 就是什么

启动时一次性分析整首歌（STFT → 频段能量 → 频谱 → 节拍网格 → 分轨），播放时只做查表插值，因此实时渲染开销与音频复杂度无关。

---

## 结构

10 个段落，按 `assets/timeline.toml` 的锚点切换；每段有自己的调色板、后处理基调与卡片序列：

| 段落 | 时间 | 主题色 | 画面 |
|---|---|---|---|
| 序 Intro | 0:00–0:15 | 灰白 | 晨雾、未醒的万物 |
| 春 Spring | 0:15–0:43 | 嫩绿 + 樱粉 | 苔の産毛、朝露、一羽の蝶、東風解云 |
| 夏 Summer | 0:43–1:08 | 濃青 + 熱橙 | 節拍の斜雨、蓮葉の乱レ玉、簾、酒香 |
| 橋 Bridge | 1:08–1:24 | 青紫 | 瞬き一ツの間に、形が変エ行ク |
| 副歌 Chorus | 1:24–1:58 | 四色流转 | 移ロフ、川の流れ、昨日咲キ今日散ル花、心の綾 |
| 秋 Autumn | 1:58–2:37 | 琥珀 + 深紅 | 月に雁、松の影、常駐の落葉層 |
| 冬 Winter | 2:37–2:55 | 藍白 + 朱 | 枯レ野原、遠山、二層の視差雪 |
| 昇華 Ascension | 2:55–3:44 | 金白 | 絵巻が開き、「刹那」が筆で刻まれる |
| 終章 Finale | 3:44–4:28 | 褪せた春緑 | 四季の色が環になって閉じる |
| 尾奏 Outro | 4:28–4:49 | 灰白 | 巻を畳む、静寂 |

逐卡清单与说明见 [`docs/STORYBOARD.md`](docs/STORYBOARD.md)。

---

## 依赖

- Rust 1.75+（edition 2021）
- `ffmpeg`（仅视频封装需要；只导出 PNG 序列则不需要）
- Linux 下需要 ALSA 开发库：`libasound2-dev` / `alsa-lib`

## 构建

```bash
cargo build --release
# 产物：target/release/shunkashuto
```

## 使用

### 实时播放

```bash
./target/release/shunkashuto --audio "春・夏・秋・冬.flac"
```

歌词与时间轴默认从 `assets/` 读取，也可以显式指定：

```bash
./target/release/shunkashuto \
  --audio song.flac \
  --lrc assets/shunkashuto.lrc \
  --timeline assets/timeline.toml \
  --mode braille
```

### 导出视频

```bash
./target/release/shunkashuto \
  --audio song.flac \
  --lrc assets/shunkashuto.lrc \
  --timeline assets/timeline.toml \
  --mode braille \
  --fps 24 --width 220 --height 62 \
  --export-frames frames \
  --export-video out.mp4
```

`--export-video` 会自动调用 ffmpeg 把帧序列编码成 H.264 MP4 并混入音轨；加 `--no-audio` 则导出无声视频。

### 只看某一段

```bash
./target/release/shunkashuto --audio song.flac --offset 84.9 --duration 33 --fps 30 --width 120 --height 40
```

## 渲染模式

| 模式 | 每格点数 | 说明 |
|---|---|---|
| `braille` | 2×4 | 默认，细节最高 |
| `half` | 1×2 | 兼容性最好 |
| `quadrant` | 2×2 | 折中 |
| `ascii` | 1×1 | 无 Unicode 支持时的兜底 |
| `auto` | — | 按 `$TERM` 判断，**无 TTY 时会退化成 ascii** |

> 在 CI / 管道里渲染务必显式传 `--mode braille`，否则 `auto` 会给出字符画而不是点阵。

---

## 版权

- 词曲版权属于 **Junia Brutus**。本仓库是粉丝性质的视觉化程序，**不包含、也不会上传任何音频文件**。
- `assets/shunkashuto.lrc` 只包含用于画面同步的歌词时间轴。
- 想在本地出片，请自备你合法取得的音源，用上面的命令渲染。
- GitHub Actions 的 `Render PV` 工作流用 ffmpeg 现场合成**与歌曲等长的静音占位音轨**来驱动时间轴，因此分镜与真音轨渲染完全一致，只是音频特征（beat / energy）驱动的泛光与抖动会弱一些。拿到 artifact 后在本地 `ffmpeg -c:v copy` 混入真音轨即可，画质无损。

## 许可

代码以 MIT 发布，见 [`LICENSE`](LICENSE)。

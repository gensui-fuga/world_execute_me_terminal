//! `world_execute_me_terminal` —— 用代码逐帧渲染的终端 MV。
//!
//! 两种运行模式：
//! - **实时播放**：音频时钟驱动，终端里看，可交互（暂停/seek/调试面板）
//! - **离线导出**：`--export-frames` / `--export-video`，同一套场景代码逐帧渲染
//!
//! 运行期**绝不**向 stdout/stderr 打印日志（会撕碎 TUI 画面）；
//! 所有诊断信息经 `tracing` 写入文件。panic 时由 panic hook 负责把终端还原。

// 这是一个 bin crate，但内部模块按库的风格组织：
// 完整保留 API 表面（供扩展、测试与后续里程碑使用），
// 因此关闭 dead_code 检查，避免把「预留接口」当成错误。
#![allow(dead_code)]
// 渲染管线里大量函数需要 8~10 个几何/配色参数；拆成结构体反而更难读。
#![allow(clippy::too_many_arguments)]
// Braille 位表、调色板 ramp、频谱数组用索引循环比迭代器更直观。
#![allow(clippy::needless_range_loop)]
// 测试里「先 Default::default() 再逐字段赋值」是最清晰的构造方式。
#![allow(clippy::field_reassign_with_default)]

mod audio;
mod config;
mod export;
mod input;
mod lyrics;
mod render;
mod timeline;

use std::panic;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::Result;
use clap::Parser;
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use rand::rngs::SmallRng;
use rand::SeedableRng;
use ratatui::backend::CrosstermBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::{Frame, Terminal};

use crate::audio::features::{FeatureTrack, BAND_NAMES};
use crate::audio::loader::AudioSource;
use crate::config::{Cli, Config, RenderMode};
use crate::export::{find_ffmpeg, EncodeOptions, FrameExporter};
use crate::input::Action;
use crate::lyrics::words::WordTimeline;
use crate::lyrics::Lyrics;
use crate::render::color as col;
use crate::render::layers::{LayerStack, LAYER_BG, LAYER_LYRICS, LAYER_PARTICLES, LAYER_SUBJECT};
use crate::render::particles::ParticleSystem;
use crate::render::postfx::PostFxStack;
use crate::render::scenes::{self, SceneCtx};
use crate::render::CharCanvas;
use crate::timeline::chapters::Narrative;
use crate::timeline::cues::CueKind;
use crate::timeline::scene::SceneKind;
use crate::timeline::Timeline;

/// 终端是否处于 raw + alternate screen 状态。
static TERM_ACTIVE: AtomicBool = AtomicBool::new(false);

fn main() -> Result<()> {
    let cli = Cli::parse();
    let cfg = Config::load(&cli)?;

    config::init_logging(&cfg);
    install_panic_hook();

    let result = match &cfg.command {
        Some(cmd) => run_subcommand(&cfg, cmd),
        None => run(&cfg),
    };

    restore_terminal();
    if let Err(ref e) = result {
        eprintln!("shunkashuto: {e:#}");
    }
    result
}

/// 还原终端。可重复调用。
fn restore_terminal() {
    if TERM_ACTIVE.swap(false, Ordering::SeqCst) {
        let _ = disable_raw_mode();
        let mut stdout = std::io::stdout();
        let _ = execute!(stdout, LeaveAlternateScreen);
        let _ = execute!(stdout, crossterm::cursor::Show);
    }
}

/// panic 时先还原终端。
fn install_panic_hook() {
    let prev = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        restore_terminal();
        prev(info);
    }));
}

// ───────────────────────── 引擎 ─────────────────────────

/// 一帧的统计信息（供 UI 与调试面板使用）。
#[derive(Debug, Clone, Copy)]
pub struct FrameStats {
    /// 当前场景
    pub scene: SceneKind,
    /// 段内时间
    pub local_t: f64,
    /// 段内进度
    pub progress: f32,
    /// 归一化能量
    pub energy: f32,
    /// 节拍脉冲
    pub beat: f32,
    /// 粒子数
    pub particles: usize,
    /// 六个频段的归一化能量（调试面板用）
    pub bands: [f32; 6],
    /// 当前歌词行的行号（无歌词为 None）
    pub lyric_line: Option<usize>,
    /// 当前行已显示的字符数（逐词打字机）
    pub lyric_typed: usize,
    /// 当前行总字符数
    pub lyric_total: usize,
    /// 当前正在唱的词在行内的序号
    pub lyric_word: usize,
    /// 该行命中主题词的主主题（用于调试面板）
    pub lyric_motif: &'static str,
    /// 当前叙事章节
    pub chapter: &'static str,
    /// 章节内进度
    pub chapter_progress: f32,
    /// 系统色增益（随剧情排空）
    pub ui_gain: f32,
}

/// 渲染引擎：持有画布、图层、粒子与后处理栈。
pub struct Engine {
    track: FeatureTrack,
    timeline: Timeline,
    lyrics: Lyrics,
    /// 逐词时间轴（行内打字机与关键词高亮的数据源）
    pub words: WordTimeline,
    /// 叙事弧（章节标题、色调迁移、系统色排空）
    pub narrative: Narrative,
    /// 合成后的最终画布
    pub canvas: CharCanvas,
    layers: LayerStack,
    particles: ParticleSystem,
    postfx: PostFxStack,
    rng: SmallRng,
    /// 字符列数
    pub cols: u16,
    /// 字符行数
    pub rows: u16,
    /// 上一帧时间（用于 dt）
    prev_t: f64,
    /// 上一次实际渲染的时间（seek 时用于重置粒子）
    last_render_t: f64,
}

impl Engine {
    /// 新建引擎。
    pub fn new(
        track: FeatureTrack,
        timeline: Timeline,
        lyrics: Lyrics,
        cols: u16,
        rows: u16,
        mode: RenderMode,
        seed: u64,
    ) -> Self {
        // 逐词时间轴与叙事弧在构造时一次性算好，之后每帧只查表。
        let song_len = track.duration.max(1.0) as f32;
        let bpm = track.bpm;
        let beat = if bpm > 1.0 { 60.0 / bpm } else { 0.4666 };
        let words = WordTimeline::build(&lyrics.lines, song_len, beat);
        let narrative = Narrative::default_arc(song_len);
        Self {
            track,
            timeline,
            lyrics,
            words,
            narrative,
            canvas: CharCanvas::new(cols, rows, mode),
            layers: LayerStack::new(cols, rows, mode),
            particles: ParticleSystem::new(seed, 8192),
            postfx: PostFxStack::new(seed),
            rng: SmallRng::seed_from_u64(seed),
            cols,
            rows,
            prev_t: -1.0,
            last_render_t: -1.0,
        }
    }

    /// 改变画布尺寸（终端缩放时）。
    pub fn resize(&mut self, cols: u16, rows: u16, mode: RenderMode) {
        if cols == self.cols && rows == self.rows && self.canvas.mode == mode {
            return;
        }
        self.cols = cols.max(1);
        self.rows = rows.max(1);
        self.canvas = CharCanvas::new(self.cols, self.rows, mode);
        self.layers = LayerStack::new(self.cols, self.rows, mode);
    }

    /// 跳到某个时间点：重置粒子与随机状态，保证确定性。
    pub fn seek(&mut self, t: f64) {
        self.prev_t = t;
        self.last_render_t = t;
        self.particles.clear();
        // 残影缓冲必须一起清掉，否则跳转后的头几帧会叠着上一处的残像
        self.postfx.clear_trail();
        self.postfx.reseed(0x5EED_1077 ^ (t * 1000.0) as u64);
    }

    /// 渲染一帧。`t` 是音频时间（秒），`dt` 是距上一帧的秒数。
    pub fn render_frame(&mut self, t: f64, dt: f64) -> FrameStats {
        // 时间倒退（seek）时重置粒子与残影，避免拖尾穿越
        if t + 1e-6 < self.last_render_t {
            self.particles.clear();
            self.postfx.clear_trail();
        }
        self.last_render_t = t;

        let seg_idx = self.timeline.index_at(t);
        let seg = &self.timeline.segments()[seg_idx];
        let scene = seg.scene;
        let local_t = (t - seg.start).max(0.0);
        let progress = self.timeline.progress_in_segment(t);

        let feat = self.track.blend(t);
        let prev = if self.prev_t >= 0.0 {
            self.track.blend(self.prev_t)
        } else {
            feat.clone()
        };
        self.prev_t = t;

        let cues = &self.timeline.cues;
        let ctx = SceneCtx {
            t,
            local_t,
            progress,
            feat: feat.clone(),
            prev,
            frame: (t * self.track.fps).max(0.0) as u64,
            cols: self.cols,
            rows: self.rows,
            scene,
            lyrics: &self.lyrics,
            flash: cues.active(t, CueKind::Flash),
            punch: cues.active(t, CueKind::Punch),
            heartbeat: cues.active(t, CueKind::Heartbeat),
            glitch: cues.active(t, CueKind::Glitch),
            burst: cues.active(t, CueKind::Burst),
            invert: cues.active(t, CueKind::Invert),
        };

        // 清空所有图层（大气场必须画在 clear_all 之后，否则刚铺的底纹会被清掉）
        self.layers.clear_all();

        // 0) 全屏流动大气场 → 背景层
        //    必须画在主体之前、且单独占一层：主体层带 alpha，
        //    底纹若跟着主体一起被压暗就白铺了。
        //    这一层是「画面不再大片死黑」的硬保证 —— 它保证每个字符格至少有一个亮点。
        {
            let bg = self.layers.canvas_mut(LAYER_BG);
            crate::render::atmosphere::render(bg, &ctx, 0.95);
        }

        // 1) 场景 → 主体层
        {
            let subject = self.layers.canvas_mut(LAYER_SUBJECT);
            scenes::render(&ctx, subject, &mut self.particles, &mut self.rng);
        }

        // 2) 粒子推进并画到粒子层
        self.particles
            .set_bounds(self.canvas.sw as f32, self.canvas.sh as f32);
        self.particles.update(dt.clamp(0.0, 0.1) as f32);
        {
            let particles_canvas = self.layers.canvas_mut(LAYER_PARTICLES);
            self.particles.draw(particles_canvas);
        }

        // 3) 歌词 → 歌词层（用几何方式点缀，真正的文本在 UI 层）
        {
            let lc = self.layers.canvas_mut(LAYER_LYRICS);
            draw_lyric_ornament(&ctx, lc);
        }

        // 4) 合成
        let mode = self.canvas.mode;
        let mut out = std::mem::replace(&mut self.canvas, CharCanvas::new(1, 1, mode));
        self.layers.composite(&mut out);

        // 5) 后处理
        scenes::postfx_for(&ctx, &mut self.postfx.params);
        let (trail, trail_decay) = scenes::trail_for(&ctx);
        self.postfx.trail = trail;
        self.postfx.trail_decay = trail_decay;
        self.postfx.apply(&mut out);

        // 6) 事件驱动的整幅效果（反转 / 闪白）
        if ctx.invert > 0.5 {
            invert_canvas(&mut out, ctx.invert.min(1.0) * 0.5);
        }
        if ctx.flash > 0.1 {
            add_flash(&mut out, ctx.flash);
        }
        if ctx.burst > 0.2 {
            self.particles.burst(
                out.sw as f32 / 2.0,
                out.sh as f32 / 2.0,
                (12.0 * ctx.burst) as usize,
                60.0,
                scene.palette()[0],
                1.0,
                &['*', '+', '·'],
            );
        }

        self.canvas = out;

        // 歌词与章节状态：每帧查表，无分配
        let tf = t as f32;
        let (lyric_line, lyric_typed, lyric_total, lyric_word, lyric_motif) =
            match self.words.line_at(tf) {
                Some(l) => {
                    let total = l.text.chars().count();
                    let typed = l.typed_chars(tf);
                    let lm = crate::lyrics::motif::analyze_line(&l.text);
                    (
                        Some(l.index),
                        typed,
                        total,
                        l.current_word_index(tf),
                        lm.primary().name(),
                    )
                }
                None => (None, 0, 0, 0, "-"),
            };
        let mood = self.narrative.mood_at(tf);

        FrameStats {
            scene,
            local_t,
            progress,
            energy: feat.energy_norm,
            beat: feat.beat,
            particles: self.particles.len(),
            bands: feat.bands_norm,
            lyric_line,
            lyric_typed,
            lyric_total,
            lyric_word,
            lyric_motif,
            chapter: mood.label,
            chapter_progress: mood.progress,
            ui_gain: mood.ui_gain,
        }
    }

    /// 当前场景名。
    pub fn scene_at(&self, t: f64) -> SceneKind {
        self.timeline.scene_at(t)
    }

    /// 时间轴引用。
    pub fn timeline(&self) -> &Timeline {
        &self.timeline
    }

    /// 特征轨道引用。
    pub fn track(&self) -> &FeatureTrack {
        &self.track
    }

    /// 歌词引用。
    pub fn lyrics(&self) -> &Lyrics {
        &self.lyrics
    }

    /// 总时长。
    pub fn duration(&self) -> f64 {
        self.track.duration
    }
}

/// 歌词层的几何装饰：当前行出现时底部泛起光带。
fn draw_lyric_ornament(ctx: &SceneCtx, canvas: &mut CharCanvas) {
    let Some(line) = ctx.lyrics.current(ctx.t) else {
        return;
    };
    let age = ctx.t - line.time;
    let span = ctx
        .lyrics
        .next(ctx.t)
        .map(|n| n.time - line.time)
        .unwrap_or(4.0)
        .max(0.3);
    // 出现后 0.35 秒内从两侧扫入
    let k = (age / 0.35).clamp(0.0, 1.0);
    // 结束前 0.4 秒淡出
    let fade = ((span - age) / 0.4).clamp(0.0, 1.0) as f32;
    let a = (k as f32) * fade;
    if a <= 0.02 {
        return;
    }
    let sw = canvas.sw as f32;
    let y = canvas.sh as f32 * 0.82;
    let half = sw * 0.42 * (k as f32);
    let c = col::lerp(ctx.scene.palette()[0], col::WHITE, 0.35);
    canvas.line(sw / 2.0 - half, y, sw / 2.0 + half, y, c, a * 0.55);
    canvas.line(
        sw / 2.0 - half * 0.7,
        y + 1.0,
        sw / 2.0 + half * 0.7,
        y + 1.0,
        c,
        a * 0.3,
    );
}

/// 整幅反转（保持覆盖度）。
fn invert_canvas(canvas: &mut CharCanvas, amount: f32) {
    let a = amount.clamp(0.0, 1.0);
    for p in canvas.pixels_mut() {
        p.r = p.r * (1.0 - a) + (1.0 - p.r) * a;
        p.g = p.g * (1.0 - a) + (1.0 - p.g) * a;
        p.b = p.b * (1.0 - a) + (1.0 - p.b) * a;
    }
}

/// 全屏闪白。只提亮已有内容 —— 增加覆盖率会把空单元也点亮，画面变实心墙。
fn add_flash(canvas: &mut CharCanvas, amount: f32) {
    let a = (amount * 0.35).clamp(0.0, 1.0);
    for p in canvas.pixels_mut() {
        if p.w <= 0.0 {
            continue;
        }
        p.r = (p.r + a).min(1.0);
        p.g = (p.g + a).min(1.0);
        p.b = (p.b + a).min(1.0);
    }
}

// ───────────────────────── 运行模式 ─────────────────────────

fn run(cfg: &Config) -> Result<()> {
    // 音频与整曲分析
    let audio = AudioSource::open(cfg)?;
    let duration = audio.duration().as_secs_f64().max(0.1);
    let track = analyze(cfg, &audio)?;
    let lyrics = load_lyrics(cfg)?;
    let timeline = load_timeline(cfg, duration)?;

    tracing::info!(
        duration,
        bpm = track.bpm,
        scenes = timeline.len(),
        lyrics = lyrics.len(),
        "准备就绪"
    );

    if cfg.export_frames.is_some() || cfg.export_video.is_some() {
        export_all(cfg, track, timeline, lyrics, duration)
    } else {
        realtime(cfg, audio, track, timeline, lyrics)
    }
}

/// 整曲分析（导出与实时共用）。
fn analyze(cfg: &Config, audio: &AudioSource) -> Result<FeatureTrack> {
    let pcm = audio.pcm();
    audio::analyze(
        &pcm,
        audio.channels(),
        audio.sample_rate(),
        cfg.fps,
        cfg.stems_dir.as_deref(),
    )
}

/// 加载歌词：显式 `--lrc` > 与音频同名 > 占位。
fn load_lyrics(cfg: &Config) -> Result<Lyrics> {
    let path = cfg
        .lrc
        .clone()
        .or_else(|| cfg.audio.as_ref().and_then(|a| config::sibling_lrc(a)));

    match path {
        Some(p) => {
            let l = Lyrics::load(&p)?;
            tracing::info!(path = %p.display(), lines = l.len(), "歌词已加载");
            Ok(l)
        }
        None => {
            tracing::info!("未提供歌词文件，使用占位歌词");
            Ok(Lyrics::placeholder())
        }
    }
}

/// 加载时间轴。
fn load_timeline(cfg: &Config, duration: f64) -> Result<Timeline> {
    match &cfg.timeline {
        Some(p) => {
            let t = Timeline::load(p, duration)?;
            tracing::info!(path = %p.display(), segments = t.len(), "时间轴已加载");
            Ok(t)
        }
        None => {
            tracing::info!("未提供时间轴文件，使用内置默认结构");
            Ok(Timeline::default_for(duration))
        }
    }
}

/// 实时播放模式。
fn realtime(
    cfg: &Config,
    audio: AudioSource,
    track: FeatureTrack,
    timeline: Timeline,
    lyrics: Lyrics,
) -> Result<()> {
    // 终端初始化
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    execute!(stdout, crossterm::cursor::Hide)?;
    TERM_ACTIVE.store(true, Ordering::SeqCst);

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let size = terminal.size()?;
    let mut engine = Engine::new(
        track,
        timeline,
        lyrics,
        size.width,
        size.height.saturating_sub(2),
        cfg.mode,
        cfg.seed,
    );

    let total = audio.duration();
    if !cfg.no_audio {
        audio.play()?;
    }

    let wall_start = Instant::now();
    let mut paused = false;
    let mut quit = false;
    let mut show_debug = false;
    let mut offset = cfg.offset;
    let mut last = Instant::now();
    let mut fps_ema = cfg.fps;
    let mut clock_origin = Instant::now();
    let mut clock_base = 0.0f64;

    while !quit {
        // ── 输入 ──────────────────────────────────────────────
        for action in input::poll_actions()? {
            match action {
                Action::Quit => quit = true,
                Action::TogglePause => {
                    paused = !paused;
                    if paused {
                        audio.pause();
                    } else {
                        audio.resume();
                        clock_origin = Instant::now();
                        clock_base = audio.position().as_secs_f64();
                    }
                }
                Action::Seek(delta) => {
                    // 音频设备的 seek 需要重建 source，这里只调整画面时钟偏移
                    offset += delta;
                    engine.seek((audio.position().as_secs_f64() + offset).max(0.0));
                    tracing::info!(offset, "seek (画面偏移)");
                }
                Action::Offset(delta) => {
                    offset += delta;
                    tracing::info!(offset, "音画偏移调整");
                }
                Action::CycleMode => {
                    let next = match engine.canvas.mode {
                        RenderMode::Braille => RenderMode::Half,
                        RenderMode::Half => RenderMode::Quadrant,
                        RenderMode::Quadrant => RenderMode::Ascii,
                        _ => RenderMode::Braille,
                    };
                    engine.resize(engine.cols, engine.rows, next);
                    tracing::info!(mode = ?next, "切换渲染模式");
                }
                Action::ToggleDebug => show_debug = !show_debug,
                Action::Restart => {
                    offset = cfg.offset;
                    engine.seek(0.0);
                }
            }
        }

        // ── 时钟 ──────────────────────────────────────────────
        let audio_pos = audio.position().as_secs_f64();
        let t = if cfg.no_audio {
            wall_start.elapsed().as_secs_f64()
        } else if paused {
            clock_base
        } else {
            // 音频时钟为主，墙钟兜底（设备异常时 position 会卡住）
            let wall = clock_base + clock_origin.elapsed().as_secs_f64();
            if audio_pos > 0.0 {
                audio_pos
            } else {
                wall
            }
        } + offset
            - cfg.latency_comp / 1000.0;

        let dt = last.elapsed().as_secs_f64();
        last = Instant::now();
        let inst_fps = if dt > 0.0 { 1.0 / dt } else { cfg.fps };
        fps_ema = fps_ema * 0.9 + inst_fps * 0.1;

        // ── 尺寸变化 ──────────────────────────────────────────
        let size = terminal.size()?;
        let want_rows = size.height.saturating_sub(3).max(1);
        if size.width != engine.cols || want_rows != engine.rows {
            engine.resize(size.width, want_rows, engine.canvas.mode);
        }

        // ── 渲染 ──────────────────────────────────────────────
        let stats = engine.render_frame(t, dt);

        terminal.draw(|f| {
            let area = f.area();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(1),
                    Constraint::Length(1), // 歌词条
                    Constraint::Length(2), // 状态栏
                ])
                .split(area);
            draw_stage(f, &engine, chunks[0]);
            draw_lyric_band(f, &engine, &stats, chunks[1]);
            draw_status(f, &engine, &stats, chunks[2], paused, show_debug, fps_ema);
        })?;

        // 帧率节流
        let budget = Duration::from_secs_f64(1.0 / cfg.fps.max(1.0));
        let used = last.elapsed();
        if used < budget {
            std::thread::sleep(budget - used);
        }

        if let Some(dur) = cfg.duration {
            if t >= dur {
                break;
            }
        }
        if !cfg.no_audio && total > Duration::ZERO && t > total.as_secs_f64() + 0.2 {
            break;
        }
        if audio.finished() && !paused && total > Duration::ZERO {
            break;
        }
    }

    audio.stop();
    Ok(())
}

/// 把引擎画布写进终端，再叠加调试面板。
fn draw_stage(f: &mut Frame, engine: &Engine, area: Rect) {
    engine.canvas.to_buffer(f.buffer_mut(), area);
}

/// 歌词条：仿终端 `stdout · tokens` 面板。
///
/// 布局参考成熟实现，而不是光秃秃一行字：
/// - 外框 + 标题，让它是「系统的一块输出区」
/// - 开头 `>` 提示符
/// - 每个词自带底色块（深浅交替），正在唱的词反色高亮
/// - 命中关键词时换成语义色（execute→红、love→蓝、其余→主题色）
/// - 末尾一个闪烁方块光标，像终端在等输入
fn draw_lyric_band(f: &mut Frame, engine: &Engine, stats: &FrameStats, area: Rect) {
    if area.height < 3 || area.width < 20 {
        return;
    }
    let [main, _, _] = stats.scene.palette();

    // 外框：终端输出区的观感
    let box_col = col::lerp(col::BLACK, main, 0.62);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(box_col))
        .title(Span::styled(
            " stdout · tokens ",
            Style::default().fg(box_col).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height == 0 || inner.width < 6 {
        return;
    }

    // 提示符
    let mut spans: Vec<Span> = vec![Span::styled(
        "> ",
        Style::default().fg(col::lerp(col::BLACK, main, 0.9)),
    )];

    let Some(idx) = stats.lyric_line else {
        // 间奏：只留提示符 + 闪烁光标
        spans.push(Span::styled(
            "…",
            Style::default().fg(col::DARK_GREY),
        ));
        f.render_widget(Paragraph::new(Line::from(spans)), inner);
        return;
    };

    let Some(synced) = engine.words.lines().iter().find(|l| l.index == idx) else {
        return;
    };
    let motifs = crate::lyrics::motif::analyze_line(&synced.text);
    // 整行的底色由主主题决定，让「这一行在唱什么」一眼可辨
    let accent = motifs
        .primary()
        .accent()
        .unwrap_or_else(|| col::lerp(col::BLACK, main, 0.9));

    // 可用宽度：扣掉提示符与光标
    let avail = inner.width.saturating_sub(4) as usize;
    let mut used = 2usize;

    for (wi, w) in synced.words.iter().enumerate() {
        if wi >= synced.words.len() {
            break;
        }
        let slice = &synced.text[w.byte_start..w.byte_end];
        let singing = wi == stats.lyric_word;
        let sung = wi < stats.lyric_word;
        let wlen = slice.chars().count() + 1; // 含尾部空格
        if used + wlen > avail {
            // 放不下就省略，避免换行把布局顶开
            spans.push(Span::styled("…", Style::default().fg(col::DARK_GREY)));
            break;
        }
        used += wlen;

        // 每个词一个底色块：未唱的更暗，已唱的亮一些
        let bg = if singing {
            accent
        } else if sung {
            col::lerp(col::BLACK, main, 0.30)
        } else {
            // 未唱到的词也要有底，形成「待输出」的节奏感
            col::lerp(col::BLACK, main, 0.14)
        };
        let fg = if singing {
            col::BLACK
        } else if sung {
            col::WHITE
        } else {
            col::lerp(col::BLACK, col::GREY_WHITE, 0.55)
        };

        // 命中的关键词额外加粗，形成「系统识别出关键字」的暗示
        // 单独判定这个词是否命中主题词表
        let clean: String = slice
            .chars()
            .filter(|c| c.is_ascii_alphabetic() || *c == '-')
            .collect::<String>()
            .to_lowercase();
        let hot = !clean.is_empty()
            && crate::lyrics::motif::classify(&clean) != crate::lyrics::motif::Motif::None;
        let mut st = Style::default().fg(fg).bg(bg);
        if singing || hot {
            st = st.add_modifier(Modifier::BOLD);
        }
        spans.push(Span::styled(slice.to_string(), st));
        spans.push(Span::styled(" ", Style::default().bg(bg)));
    }

    // 光标：正在唱时实心，间奏时闪烁
    let singing_now = stats.lyric_word < synced.words.len();
    let blink_on = ((engine.duration() * 3.0) as i64) % 2 == 0;
    if singing_now || blink_on {
        spans.push(Span::styled(
            " ",
            Style::default().bg(if singing_now { accent } else { box_col }),
        ));
    }

    f.render_widget(Paragraph::new(Line::from(spans)), inner);
}

// ─────────────────────────────── 导出模式的 UI 外壳 ───────────────────────────────
//
// 实时播放用 `Frame` + widget 叠界面；离线导出只有一个 `Buffer`，
// 所以外壳直接写单元。这不是「另做一套」——是同一版式在导出侧的落地：
// 顶栏一行、底部三行（当前唱句 / 下一句 / 状态条），中间留给画面。
//
// 为什么要有外壳：只有画面的话，观众看到的是「一堆抽象图形在动」；
// 加上标题栏、时间码、歌词、频谱和进度条之后，它读起来是一个**正在运行的终端程序**，
// 有上下文、有进度、有「现在在唱哪句」。这是这支片子和幻灯片的区别。

/// 导出时上下各留多少行给外壳。
pub const CHROME_TOP: u16 = 2;
pub const CHROME_BOTTOM: u16 = 6;

/// 外壳的两块点阵画布（顶栏 / 底栏）。
///
/// **为什么不用 ratatui 的 `Buffer` 写字**：`Buffer` 的一个字符格是 8×16 像素，
/// 而中日文是方块字（宽高比 1:1）。把 13 像素宽的汉字塞进 8 像素宽的格子里，
/// 相邻两字会重叠约 40%，整行糊成一团 —— 这正是「歌词看不懂」的根因。
/// 点阵画布的一个点就是 4×4 像素的方块，汉字可以按真实比例光栅化，
/// 而且和画面里的字符画共享同一套光栅化路径，风格统一。
pub struct Chrome {
    top: CharCanvas,
    bottom: CharCanvas,
}

impl Chrome {
    pub fn new(cols: u16, mode: RenderMode) -> Self {
        Self {
            top: CharCanvas::new(cols, CHROME_TOP, mode),
            bottom: CharCanvas::new(cols, CHROME_BOTTOM, mode),
        }
    }

    /// 把外壳叠加到已经画好画面的 `buf` 上。
    pub fn draw(&mut self, buf: &mut Buffer, area: Rect, engine: &Engine, stats: &FrameStats, t: f64) {
        if area.width < 40 || area.height < CHROME_TOP + CHROME_BOTTOM + 4 {
            return;
        }
        self.top.clear();
        draw_chrome_top(&mut self.top, stats, t);
        self.top.to_buffer(
            buf,
            Rect::new(area.x, area.y, area.width, CHROME_TOP),
        );

        self.bottom.clear();
        draw_chrome_bottom(&mut self.bottom, engine, stats);
        self.bottom.to_buffer(
            buf,
            Rect::new(
                area.x,
                area.y + area.height - CHROME_BOTTOM,
                area.width,
                CHROME_BOTTOM,
            ),
        );
    }
}

/// 顶栏：曲名 · 作者 · 章节 · 时间码。
fn draw_chrome_top(c: &mut CharCanvas, stats: &FrameStats, t: f64) {
    let [main, _dim, _accent] = stats.scene.palette();
    let sw = c.sw as f32;
    let sh = c.sh as f32;

    c.fill_rect(0.0, 0.0, sw, sh, col::lerp(col::BLACK, main, 0.30), 1.0);
    // 底边一条亮线，把顶栏和画面切开
    c.fill_rect(0.0, sh - 1.0, sw, 1.0, col::lerp(main, col::WHITE, 0.35), 1.0);

    let px = sh * 0.62;
    let base = sh - 1.5;
    let hot = col::lerp(main, col::WHITE, 0.62);
    let title = format!(
        " 春・夏・秋・冬 · Junia Brutus × 重音テト · {} ",
        stats.scene.label()
    );
    let used = crate::render::text::draw(c, 2.0, base, &title, px, hot, 1.0);

    let tc = format!("{:02}:{:04.1} / 04:49.9 ", (t / 60.0) as u32, t % 60.0);
    let tw = crate::render::text::measure(&tc, px);
    if used + tw + 4.0 < sw {
        crate::render::text::draw(
            c,
            sw - tw - 2.0,
            base,
            &tc,
            px,
            col::lerp(col::BLACK, main, 0.95),
            1.0,
        );
    }
}

/// 底栏：当前唱句（逐词卡拉OK）+ 下一句 + 进度条。
fn draw_chrome_bottom(c: &mut CharCanvas, engine: &Engine, stats: &FrameStats) {
    let [main, dim, accent] = stats.scene.palette();
    let sw = c.sw as f32;
    let sh = c.sh as f32;

    c.fill_rect(0.0, 0.0, sw, sh, col::lerp(col::BLACK, main, 0.16), 1.0);
    c.fill_rect(0.0, 0.0, sw, 1.0, col::lerp(main, col::WHITE, 0.25), 1.0);

    // ── 当前唱句 ──────────────────────────────────────────────
    if let Some(idx) = stats.lyric_line {
        if let Some(synced) = engine.words.lines().iter().find(|l| l.index == idx) {
            let px = sh * 0.40;
            let base = sh * 0.48;
            let total = crate::render::text::measure(&synced.text, px);
            let mut x = ((sw - total) * 0.5).max(2.0);
            for (wi, wd) in synced.words.iter().enumerate() {
                if wd.byte_start > wd.byte_end || wd.byte_end > synced.text.len() {
                    continue;
                }
                let slice = &synced.text[wd.byte_start..wd.byte_end];
                let singing = wi == stats.lyric_word;
                let sung = wi < stats.lyric_word;
                if singing {
                    // 正在唱：先铺一块强调色，字画成黑色 —— 卡拉OK 的走字块
                    let w = crate::render::text::measure(slice, px);
                    c.fill_rect(x - 1.5, base - px * 0.92, w + 3.0, px * 1.15, accent, 1.0);
                    x = crate::render::text::draw(c, x, base, slice, px, col::BLACK, 1.0);
                } else {
                    let ink = if sung {
                        col::WHITE
                    } else {
                        col::lerp(col::BLACK, col::GREY_WHITE, 0.62)
                    };
                    x = crate::render::text::draw(c, x, base, slice, px, ink, 1.0);
                }
                x += crate::render::text::measure(" ", px);
            }
        }
    }

    // ── 下一句预告 ────────────────────────────────────────────
    let nxt = stats
        .lyric_line
        .and_then(|i| engine.words.lines().iter().find(|l| l.index == i + 1))
        .map(|l| l.text.as_str())
        .unwrap_or("");
    if !nxt.is_empty() {
        let px = sh * 0.21;
        let w = crate::render::text::measure(nxt, px);
        crate::render::text::draw(
            c,
            ((sw - w) * 0.5).max(2.0),
            sh - 4.0,
            nxt,
            px,
            col::lerp(col::BLACK, dim, 0.85),
            1.0,
        );
    }

    // ── 进度条 ────────────────────────────────────────────────
    let p = stats.progress.clamp(0.0, 1.0);
    let by = sh - 3.0;
    c.fill_rect(0.0, by, sw, 3.0, col::lerp(col::BLACK, main, 0.40), 1.0);
    c.fill_rect(0.0, by, sw * p, 3.0, accent, 1.0);
    // 进度游标：一条比进度条高的亮线，让「现在到哪了」一眼可见
    c.fill_rect((sw * p - 0.5).max(0.0), by - 1.0, 1.5, 4.0, col::WHITE, 1.0);
}

/// 底部状态栏。
fn draw_status(
    f: &mut Frame,
    engine: &Engine,
    stats: &FrameStats,
    area: Rect,
    paused: bool,
    debug: bool,
    fps: f64,
) {
    let t = engine.duration();
    let scene = stats.scene;
    let [main, alt, _] = scene.palette();

    // 系统色（标题、时间）随剧情排空：章节越靠后，「你」的存在感越弱。
    // 这是从成熟实现学到的叙事手法 —— 颜色本身在讲故事。
    let gain = stats.ui_gain.clamp(0.0, 1.0);
    let system = |c: Color| -> Color {
        let (r, g, b) = col::rgb_of(c);
        Color::Rgb(
            (r as f32 * gain) as u8,
            (g as f32 * gain) as u8,
            (b as f32 * gain) as u8,
        )
    };
    let chapter_col = system(col::CYAN);

    let title = Line::from(vec![
        Span::styled(
            "world.execute(me);",
            Style::default()
                .fg(chapter_col)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        // 叙事章节：让观众知道现在演到哪一段
        Span::styled(
            stats.chapter,
            Style::default().fg(main).add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(format!("[{}]", scene.label()), Style::default().fg(alt)),
        Span::raw("  "),
        Span::styled(
            format!("{:.1}s / {:.1}s", stats.local_t, t),
            Style::default().fg(system(col::GREY_WHITE)),
        ),
        if paused {
            Span::styled("  ⏸ PAUSED", Style::default().fg(col::BRIGHT_RED))
        } else {
            Span::raw("")
        },
    ]);

    let mut second = vec![
        Span::styled(" q", Style::default().fg(col::BLACK).bg(col::GREY_WHITE)),
        Span::raw(" 退出  "),
        Span::styled("Space", Style::default().fg(col::BLACK).bg(col::GREY_WHITE)),
        Span::raw(" 暂停  "),
        Span::styled("←/→", Style::default().fg(col::BLACK).bg(col::GREY_WHITE)),
        Span::raw(" 偏移  "),
        Span::styled("m", Style::default().fg(col::BLACK).bg(col::GREY_WHITE)),
        Span::raw(" 模式  "),
        Span::styled("d", Style::default().fg(col::BLACK).bg(col::GREY_WHITE)),
        Span::raw(" 调试  "),
        Span::styled(
            engine.canvas.mode_label().to_string(),
            Style::default().fg(alt),
        ),
    ];

    if debug {
        second.push(Span::raw("   "));
        second.push(Span::styled(
            format!(
                "fps {:.0} | 粒子 {} | 能量 {:.2} | 拍 {:.2}",
                fps, stats.particles, stats.energy, stats.beat
            ),
            Style::default().fg(col::GREY_WHITE),
        ));
        second.push(Span::raw("  "));
        for (i, name) in BAND_NAMES.iter().enumerate() {
            let v = stats.bands[i].clamp(0.0, 1.0);
            let filled = (v * 4.0).round() as usize;
            let bar = format!(
                "{}{}",
                "█".repeat(filled.min(4)),
                "·".repeat(4 - filled.min(4))
            );
            second.push(Span::styled(
                format!("{name}:{bar} "),
                Style::default().fg(col::lerp(col::DARK_GREY, col::CYAN, v)),
            ));
        }
    }

    let p = Paragraph::new(vec![title, Line::from(second)])
        .block(Block::default().borders(Borders::TOP));
    f.render_widget(p, area);
}

/// 画布模式的显示名。
trait ModeLabel {
    fn mode_label(&self) -> &'static str;
}

impl ModeLabel for CharCanvas {
    fn mode_label(&self) -> &'static str {
        match self.mode {
            RenderMode::Braille => "braille",
            RenderMode::Half => "half",
            RenderMode::Quadrant => "quadrant",
            RenderMode::Ascii => "ascii",
            RenderMode::Auto => "auto",
        }
    }
}

/// 离线导出模式。
fn export_all(
    cfg: &Config,
    track: FeatureTrack,
    timeline: Timeline,
    lyrics: Lyrics,
    duration: f64,
) -> Result<()> {
    let fps = cfg.fps.max(1.0);
    // --duration 限制导出长度，未指定时导出全曲
    let duration = cfg.duration.unwrap_or(duration);
    let cols = cfg.width.unwrap_or(160);
    let rows = cfg.height.unwrap_or(50);
    let cell_w = 8;
    let cell_h = 16;

    // 帧目录：显式指定，或为视频导出准备临时目录
    let frame_dir: PathBuf = match &cfg.export_frames {
        Some(d) => d.clone(),
        None => {
            let base = cfg
                .export_video
                .as_ref()
                .and_then(|p| p.parent().map(|q| q.to_path_buf()))
                .unwrap_or_else(|| PathBuf::from("."));
            base.join("shunkashuto_frames")
        }
    };

    // 画面只占中间：上下留给 UI 外壳（顶栏 + 歌词条 + 状态条）。
    // 外壳不是装饰 —— 有它才读得出「这是一个在跑的终端程序」，而不是一堆抽象图形。
    let stage_rows = rows.saturating_sub(CHROME_TOP + CHROME_BOTTOM).max(8);

    let mut exporter = FrameExporter::new(&frame_dir, cell_w, cell_h)?;
    tracing::info!(
        dir = %frame_dir.display(),
        cols,
        rows,
        cell = format!("{cell_w}x{cell_h}"),
        font = exporter.font_path().unwrap_or("<none: braille only>"),
        cjk = exporter.fallback_path().unwrap_or("<none: 汉字会变方块!>"),
        stage_rows,
        "开始导出帧"
    );

    let mut engine = Engine::new(track, timeline, lyrics, cols, stage_rows, cfg.mode, cfg.seed);

    let total_frames = (duration * fps).ceil() as u64;
    let dt = 1.0 / fps;
    let area = Rect::new(0, 0, cols, rows);
    let stage = Rect::new(0, CHROME_TOP, cols, stage_rows);
    let mut buf = ratatui::buffer::Buffer::empty(area);
    let mut chrome = Chrome::new(cols, cfg.mode);

    // 起始时间：--offset 指定从歌曲的哪一秒开始导。
    // 实时模式里 offset 是「画面相对音频的微调」，导出模式里它同时决定起点 ——
    // 想直接导副歌就 --offset 73.5 --duration 15。
    let start_t = cfg.offset.max(0.0);

    let started = Instant::now();
    for i in 0..total_frames {
        let t = start_t + i as f64 / fps;
        // 每帧重置 buffer，避免残留
        for cell in buf.content.iter_mut() {
            cell.reset();
        }
        let stats = engine.render_frame(t, dt);
        engine.canvas.to_buffer(&mut buf, stage);
        chrome.draw(&mut buf, area, &engine, &stats, t);
        exporter.write(i, &buf, area)?;

        // 进度写 stderr —— 导出模式不占用终端
        if i % 30 == 0 || i + 1 == total_frames {
            let pct = (i + 1) as f64 / total_frames as f64 * 100.0;
            let elapsed = started.elapsed().as_secs_f64();
            let eta = if i > 0 {
                elapsed / (i + 1) as f64 * (total_frames - i - 1) as f64
            } else {
                0.0
            };
            eprint!(
                "\r导出帧 {}/{} ({:.1}%)  已用 {:.0}s  ETA {:.0}s   ",
                i + 1,
                total_frames,
                pct,
                elapsed,
                eta
            );
        }
    }
    eprintln!();

    tracing::info!(frames = exporter.written(), "帧导出完成");

    // 视频封装
    if let Some(out) = &cfg.export_video {
        let is_gif = out
            .extension()
            .map(|e| e.eq_ignore_ascii_case("gif"))
            .unwrap_or(false);
        if is_gif {
            export::encode_gif(&exporter.pattern(), out, fps, cols as u32 * cell_w)?;
        } else {
            let opts = EncodeOptions {
                fps,
                width: cols as u32 * cell_w,
                height: rows as u32 * cell_h,
                audio: if cfg.no_audio {
                    None
                } else {
                    cfg.audio.clone()
                },
                ..Default::default()
            };
            export::encode_mp4(&exporter.pattern(), out, &opts)?;
        }
        eprintln!("已生成视频: {}", out.display());
    } else {
        eprintln!("帧序列已写入: {}", frame_dir.display());
    }
    Ok(())
}

/// 子命令。
fn run_subcommand(cfg: &Config, cmd: &config::Command) -> Result<()> {
    match cmd {
        config::Command::Info => {
            let audio = AudioSource::open(cfg)?;
            let duration = audio.duration().as_secs_f64();
            let track = analyze(cfg, &audio)?;
            let timeline = load_timeline(cfg, duration)?;
            let lyrics = load_lyrics(cfg)?;

            println!("音频      : {:?}", cfg.audio);
            println!("时长      : {duration:.3}s");
            println!(
                "采样率    : {} Hz / {} ch",
                audio.sample_rate(),
                audio.channels()
            );
            println!("帧率      : {} fps", cfg.fps);
            println!("BPM(估计) : {:.1}", track.bpm);
            println!("特征帧数  : {}", track.len());
            println!("歌词行数  : {}", lyrics.len());
            println!("渲染模式  : {:?}", cfg.mode);
            println!("随机种子  : {}", cfg.seed);
            println!();
            println!("场景分段:");
            for (i, s) in timeline.segments().iter().enumerate() {
                println!(
                    "  {:>2}. {:<10} {:>8.2}s → {:>8.2}s  ({} 段内 {:.1}s)",
                    i + 1,
                    s.scene.name(),
                    s.start,
                    timeline.end_of(i),
                    s.scene.label(),
                    timeline.duration_of(i)
                );
            }
            println!();
            println!("关键事件: {} 个", timeline.cues.len());
            if find_ffmpeg().is_some() {
                println!(
                    "ffmpeg    : {}",
                    export::ffmpeg_version().unwrap_or_default()
                );
            } else {
                println!("ffmpeg    : 未找到（--export-video 不可用）");
            }
            Ok(())
        }
        config::Command::Analyze { min_gap } => {
            let audio = AudioSource::open(cfg)?;
            let duration = audio.duration().as_secs_f64();
            let track = analyze(cfg, &audio)?;
            let cands = audio::novelty_candidates(&track, *min_gap);

            println!("# shunkashuto --analyze 生成的建议切分点（能量新颖度）");
            println!("# 音频时长 {duration:.2}s，BPM 估计 {:.1}", track.bpm);
            println!("# 把这些时间点填进 timeline.toml 的 [[cue]] 即可");
            println!();
            for (t, score) in &cands {
                println!("{t:8.2}s  新颖度 {score:.4}");
            }
            println!();
            println!("# 可直接使用的 TOML 片段：");
            for (t, _) in cands.iter().take(24) {
                println!("[[cue]]\ntime = {t:.3}\nkind = \"mark\"\nname = \"suggested\"\n");
            }
            Ok(())
        }
    }
}

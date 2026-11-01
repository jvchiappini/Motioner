//! The live preview window: layout, input, painting and hot reload.
//!
//! The composition is drawn in the engine's `Pure2D` pass through
//! [`motioner_render::LivePainter`]; the panels, outline, transport and status
//! bar are drawn in the engine's UI pass, which the renderer composites on top.
//! That ordering is why selected items can overflow the composition over the
//! pasteboard and still stay below the chrome.

use std::cell::RefCell;
use std::fmt;
use std::path::Path;
use std::rc::Rc;
use std::time::{Duration, Instant};

use ferrous_engine::{
    App, AppContext, AppMode, Color, DrawContext, FerrousApp, KeyCode, MouseButton, Vec2,
};
use ferrous_renderer::render_2d::ShapeInstance;
use motioner_core::{Color as CoreColor, FrameRate, NodeId};
use motioner_eval::{ItemShape, RenderPlan};
use motioner_render::{LivePainter, RenderError, ViewFit};

use crate::input::{action_for, apply, Action, Effect, Modifiers};
use crate::{
    frame_to_x, outline_rows, skipped_frames, tick_frames, tick_spacing, timecode, x_to_frame,
    OutlineRow, PreviewSession, SessionError, TickUnit, Transport, MIN_TICK_SPACING_PX,
};

/// Embedded UI font (also used by the engine's immediate text pass).
const UI_FONT: &[u8] = include_bytes!("../../../engine/ferrous/assets/fonts/Roboto-Regular.ttf");

/// Options accepted by [`run_preview`].
#[derive(Debug, Clone, Default)]
pub struct PreviewOptions {
    /// Initial frame (clamped into the composition).
    pub frame: Option<i64>,
    /// Start paused instead of playing immediately.
    pub paused: bool,
    /// Close the window after this many presented frames (smoke tests/CI).
    pub exit_after_frames: Option<u64>,
}

/// Errors that prevent the preview window from running.
#[derive(Debug)]
pub enum UiError {
    /// The file could not be read at startup.
    Load(SessionError),
    /// The embedded font could not be registered with the renderer.
    Painter(RenderError),
    /// An internal invariant failed without a better typed error.
    Internal(String),
}

impl fmt::Display for UiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Load(error) => write!(f, "{error}"),
            Self::Painter(error) => write!(f, "{error}"),
            Self::Internal(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for UiError {}

/// Opens the preview window for `path` and blocks until it closes.
///
/// A missing or unreadable file is a startup error. An MDF diagnostic opens
/// the window with no composition and shows the diagnostic in the status bar,
/// so the user can fix the file and let hot reload pick it up.
///
/// # Errors
///
/// Returns [`UiError::Load`] for an unreadable file and [`UiError::Painter`]
/// when the embedded font cannot be registered.
pub fn run_preview(path: &Path, options: PreviewOptions) -> Result<(), UiError> {
    let mut session = PreviewSession::new(path);
    match session.reload() {
        Ok(()) | Err(SessionError::Diagnostic(_)) => {}
        Err(error @ SessionError::Io { .. }) => return Err(UiError::Load(error)),
    }

    let (rate, duration) = session
        .composition()
        .map_or((default_rate()?, 1), |composition| {
            (composition.frame_rate(), composition.duration())
        });
    let mut transport = Transport::new(rate, duration);
    if let Some(frame) = options.frame {
        transport.set_frame(frame);
    }

    // `App::run` consumes the app and returns `()`, so a fatal error raised
    // inside the event loop is handed back through this shared slot.
    let fatal = Rc::new(RefCell::new(None));
    let app = PreviewApp::new(session, transport, options, Rc::clone(&fatal));
    let title = format!(
        "Motioner — {}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("untitled")
    );
    let background = Color::rgb(0.05, 0.05, 0.07);
    App::new(app)
        .with_title(&title)
        .with_size(1600, 900)
        .with_mode(AppMode::Flat2D)
        .with_background_color(background)
        .with_target_fps(Some(60))
        .with_vsync(true)
        .with_font_bytes(UI_FONT)
        .run();
    let error = fatal.borrow_mut().take();
    error.map_or(Ok(()), |error| Err(UiError::Painter(error)))
}

fn default_rate() -> Result<FrameRate, UiError> {
    FrameRate::new(60, 1).map_err(|error| UiError::Internal(error.to_string()))
}

// ---------------------------------------------------------------------------
// Layout
// ---------------------------------------------------------------------------

/// One transport button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Button {
    GoStart,
    StepBack10,
    StepBack,
    PlayPause,
    StepForward,
    StepForward10,
    GoEnd,
    Loop,
}

impl Button {
    fn action(self) -> Action {
        match self {
            Self::GoStart => Action::GoToStart,
            Self::StepBack10 => Action::Step(-10),
            Self::StepBack => Action::Step(-1),
            Self::PlayPause => Action::TogglePlay,
            Self::StepForward => Action::Step(1),
            Self::StepForward10 => Action::Step(10),
            Self::GoEnd => Action::GoToEnd,
            Self::Loop => Action::ToggleLoop,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct ButtonRect {
    button: Button,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

impl ButtonRect {
    fn contains(self, x: f32, y: f32) -> bool {
        x >= self.x && x <= self.x + self.w && y >= self.y && y <= self.y + self.h
    }
}

#[derive(Debug, Clone)]
struct Layout {
    window_w: f32,
    window_h: f32,
    scale: f32,
    top_h: f32,
    left_w: f32,
    main_bottom: f32,
    status_y: f32,
    viewport: [f64; 4],
    timeline: [f64; 4],
    outline_y: f32,
    row_h: f32,
    buttons: [ButtonRect; 8],
}

fn layout(window_w: f32, window_h: f32, scale: f32) -> Layout {
    let scale = scale.max(0.5);
    let top_h = 44.0 * scale;
    let left_w = (280.0 * scale).min(window_w * 0.4);
    let status_h = 28.0 * scale;
    let bottom_h = 104.0 * scale;
    let main_bottom = (window_h - status_h - bottom_h).max(top_h + 1.0);

    let viewport = [
        f64::from(left_w),
        f64::from(top_h),
        f64::from((window_w - left_w).max(0.0)),
        f64::from((main_bottom - top_h).max(0.0)),
    ];
    let timeline = [
        f64::from(left_w + 12.0 * scale),
        f64::from(main_bottom + 52.0 * scale),
        f64::from((window_w - left_w - 24.0 * scale).max(0.0)),
        f64::from(18.0 * scale),
    ];

    let mut buttons = [ButtonRect {
        button: Button::GoStart,
        x: 0.0,
        y: 0.0,
        w: 0.0,
        h: 0.0,
    }; 8];
    let mut x = left_w + 12.0 * scale;
    let y = main_bottom + 10.0 * scale;
    let h = 26.0 * scale;
    for (index, (button, width)) in [
        (Button::GoStart, 42.0),
        (Button::StepBack10, 46.0),
        (Button::StepBack, 36.0),
        (Button::PlayPause, 66.0),
        (Button::StepForward, 36.0),
        (Button::StepForward10, 46.0),
        (Button::GoEnd, 42.0),
        (Button::Loop, 58.0),
    ]
    .into_iter()
    .enumerate()
    {
        let w = width * scale;
        buttons[index] = ButtonRect { button, x, y, w, h };
        x += w + 6.0 * scale;
    }

    Layout {
        window_w,
        window_h,
        scale,
        top_h,
        left_w,
        main_bottom,
        status_y: window_h - status_h,
        viewport,
        timeline,
        outline_y: top_h + 28.0 * scale,
        row_h: 20.0 * scale,
        buttons,
    }
}

impl Layout {
    fn button_at(&self, x: f32, y: f32) -> Option<Button> {
        self.buttons
            .iter()
            .find(|rect| rect.contains(x, y))
            .map(|rect| rect.button)
    }

    fn in_timeline(&self, x: f32, y: f32) -> bool {
        contains(self.timeline, x, y)
    }

    fn in_outline(&self, x: f32, y: f32) -> bool {
        x >= 0.0 && x <= self.left_w && y >= self.outline_y && y < self.main_bottom
    }

    fn outline_row(&self, y: f32) -> Option<usize> {
        if y < self.outline_y {
            return None;
        }
        let index = ((y - self.outline_y) / self.row_h).floor();
        (index >= 0.0).then_some(index as usize)
    }
}

fn contains(rect: [f64; 4], x: f32, y: f32) -> bool {
    let x = f64::from(x);
    let y = f64::from(y);
    x >= rect[0] && x <= rect[0] + rect[2] && y >= rect[1] && y <= rect[1] + rect[3]
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

struct PreviewApp {
    session: PreviewSession,
    transport: Transport,
    watch: crate::FileWatch,
    options: PreviewOptions,
    outline: Vec<OutlineRow>,
    selected: Option<NodeId>,
    painter: Option<LivePainter>,
    /// Fatal painter error, read by [`run_preview`] after the loop exits.
    fatal: Rc<RefCell<Option<RenderError>>>,
    start_playing: bool,
    frames_drawn: u64,
    dropped: u64,
    last_dpr: f32,
}

impl PreviewApp {
    fn new(
        session: PreviewSession,
        transport: Transport,
        options: PreviewOptions,
        fatal: Rc<RefCell<Option<RenderError>>>,
    ) -> Self {
        let watch = crate::FileWatch::new(session.path());
        let outline = session.document().map(outline_rows).unwrap_or_default();
        let start_playing = !options.paused;
        Self {
            session,
            transport,
            watch,
            options,
            outline,
            selected: None,
            painter: None,
            fatal,
            start_playing,
            frames_drawn: 0,
            dropped: 0,
            last_dpr: 0.0,
        }
    }

    fn now(ctx: &AppContext<'_>) -> Duration {
        Duration::from_secs_f64(ctx.time.elapsed.max(0.0))
    }

    /// Reloads the document. A failed reload keeps the last good document and
    /// leaves the transport untouched; a good one re-times it without
    /// stopping playback.
    fn reload(&mut self, now: Duration) {
        let outcome = self.session.reload();
        if outcome.is_ok() {
            if let Some(painter) = self.painter.as_mut() {
                painter.clear_cache();
            }
            if let Some(composition) = self.session.composition() {
                self.transport
                    .retime(composition.frame_rate(), composition.duration(), now);
            }
            self.outline = self
                .session
                .document()
                .map(outline_rows)
                .unwrap_or_default();
            if let Some(selected) = self.selected {
                let still_exists = self
                    .session
                    .document()
                    .and_then(|document| document.node(selected))
                    .is_some();
                if !still_exists {
                    self.selected = None;
                }
            }
        }
    }

    fn handle_effect(&mut self, effect: Effect, ctx: &mut AppContext<'_>, now: Duration) {
        match effect {
            Effect::None => {}
            Effect::Reload => self.reload(now),
            Effect::Quit => ctx.request_exit(),
        }
    }

    fn scrub_to(&mut self, layout: &Layout, x: f32) {
        let frame = x_to_frame(
            f64::from(x),
            self.transport.duration(),
            layout.timeline[0],
            layout.timeline[2],
        );
        self.transport.set_frame(frame);
    }

    fn run_update(&mut self, ctx: &mut AppContext<'_>) {
        if let Some(limit) = self.options.exit_after_frames {
            if self.frames_drawn >= limit {
                ctx.request_exit();
                return;
            }
        }
        let now = Self::now(ctx);
        let dpr = ctx.window.scale_factor() as f32;
        if (dpr - self.last_dpr).abs() > 1e-4 {
            ctx.render.set_2d_pixel_ratio(dpr);
            self.last_dpr = dpr;
        }
        let layout = layout(ctx.window_size.0 as f32, ctx.window_size.1 as f32, dpr);

        let shift =
            ctx.input.is_key_down(KeyCode::ShiftLeft) || ctx.input.is_key_down(KeyCode::ShiftRight);
        let modifiers = Modifiers { shift };
        for key in [
            KeyCode::Space,
            KeyCode::ArrowLeft,
            KeyCode::ArrowRight,
            KeyCode::Home,
            KeyCode::End,
            KeyCode::KeyL,
            KeyCode::KeyR,
            KeyCode::Escape,
        ] {
            if ctx.input.just_pressed(key) {
                if let Some(action) = action_for(key, modifiers) {
                    let effect = apply(action, &mut self.transport, now);
                    self.handle_effect(effect, ctx, now);
                }
            }
        }

        if self.watch.check(Instant::now()) {
            self.reload(now);
        }

        if self.start_playing {
            self.start_playing = false;
            self.transport.play(now);
        }

        if self.transport.is_playing() {
            let before = self.transport.frame();
            self.transport.advance(now);
            let after = self.transport.frame();
            self.dropped = self.dropped.saturating_add(skipped_frames(
                before,
                after,
                self.transport.duration(),
            ));
        }

        let (mx, my) = ctx.input.mouse_pos_f32();
        if ctx.input.button_just_pressed(MouseButton::Left) {
            if let Some(button) = layout.button_at(mx, my) {
                let effect = apply(button.action(), &mut self.transport, now);
                self.handle_effect(effect, ctx, now);
            } else if layout.in_timeline(mx, my) {
                self.transport.begin_scrub();
                self.scrub_to(&layout, mx);
            } else if layout.in_outline(mx, my) {
                if let Some(index) = layout.outline_row(my) {
                    self.selected = self.outline.get(index).map(|row| row.node);
                }
            }
        }
        if ctx.input.is_button_down(MouseButton::Left) && self.transport.is_scrubbing() {
            self.scrub_to(&layout, mx);
        }
        if ctx.input.button_just_released(MouseButton::Left) {
            self.transport.end_scrub();
        }

        if layout.in_timeline(mx, my) {
            let scroll = ctx.input.scroll_y();
            if scroll.abs() > 0.01 {
                let direction = if scroll > 0.0 { -1 } else { 1 };
                let step = if shift { 10 } else { 1 };
                self.transport.step(direction * step);
            }
        }
    }

    // Immediate-mode chrome is intentionally one linear pass; splitting it
    // would only move the same `x`/`y`/`w`/`h` locals between functions.
    #[allow(clippy::many_single_char_names, clippy::too_many_lines)]
    fn run_draw_ui(&mut self, dc: &mut DrawContext<'_, '_>) {
        let font = dc.font;
        let gui = &mut *dc.gui;
        let ctx = &mut *dc.ctx;
        let dpr = ctx.window.scale_factor() as f32;
        let layout = layout(ctx.window_size.0 as f32, ctx.window_size.1 as f32, dpr);
        let s = layout.scale;

        // Frame selection is the only wall-clock use; the pixels depend on the
        // integer frame below. `update` already advanced the transport.
        let frame = self.transport.frame();
        if let Some(limit) = self.options.exit_after_frames {
            self.frames_drawn = self.frames_drawn.saturating_add(1);
            if self.frames_drawn >= limit {
                ctx.request_exit();
            }
        }
        let fit = match self.session.composition() {
            Some(composition) => ViewFit::contain(
                f64::from(composition.width()),
                f64::from(composition.height()),
                layout.viewport,
                f64::from(layout.window_w),
                f64::from(layout.window_h),
            ),
            None => ViewFit::contain(
                0.0,
                0.0,
                layout.viewport,
                f64::from(layout.window_w),
                f64::from(layout.window_h),
            ),
        };

        // ── Pure2D pass: pasteboard, composition background, plan, selection ──
        let renderer = ctx.render.renderer_mut();
        push_window_rect(
            renderer,
            layout.window_w,
            layout.window_h,
            layout.viewport[0] as f32,
            layout.viewport[1] as f32,
            (layout.viewport[0] + layout.viewport[2]) as f32,
            (layout.viewport[1] + layout.viewport[3]) as f32,
            linear_hex("#0b0b10"),
            -60.0,
        );

        if let Some((document, plan)) = self.session.evaluate(frame) {
            let composition = document.composition();
            let scale = fit.scale();
            let drawn_w = f64::from(composition.width()) * scale;
            let drawn_h = f64::from(composition.height()) * scale;
            let pad_x = layout.viewport[0] + (layout.viewport[2] - drawn_w) * 0.5;
            let pad_y = layout.viewport[1] + (layout.viewport[3] - drawn_h) * 0.5;
            if fit.is_drawable() {
                push_window_rect(
                    renderer,
                    layout.window_w,
                    layout.window_h,
                    pad_x as f32,
                    pad_y as f32,
                    (pad_x + drawn_w) as f32,
                    (pad_y + drawn_h) as f32,
                    linear_rgba(plan.background()),
                    -55.0,
                );
            }
            if let Some(painter) = self.painter.as_mut() {
                painter.paint(renderer, document, plan, fit);
            }
            if let Some(selected) = self.selected {
                draw_selection(
                    renderer,
                    plan,
                    selected,
                    fit,
                    (pad_x, pad_y),
                    (layout.window_w, layout.window_h),
                );
            }
        }

        // ── UI pass: chrome drawn on top of the composition ──────────────────
        let panel = linear_hex("#1b1b24");
        let bar = linear_hex("#23232f");
        let text = linear_hex("#d8d8e0");
        let dim = linear_hex("#8a8a99");
        let accent = linear_hex("#4fc3f7");
        let red = linear_hex("#ff5c6c");

        // Top bar.
        gui.rect(0.0, 0.0, layout.window_w, layout.top_h, bar);
        let title = self
            .session
            .path()
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("untitled");
        gui.draw_text(font, title, [12.0 * s, 28.0 * s], 15.0 * s, text);
        let info = match self.session.composition() {
            Some(composition) => format!(
                "{}x{} @ {:.3} fps",
                composition.width(),
                composition.height(),
                composition.frame_rate().as_f64()
            ),
            None => "no composition".to_owned(),
        };
        gui.draw_text(
            font,
            &info,
            [layout.window_w * 0.36, 28.0 * s],
            13.0 * s,
            dim,
        );
        let counter = format!("{} / {}", frame + 1, self.transport.duration().max(1));
        gui.draw_text(
            font,
            &counter,
            [layout.window_w * 0.62, 28.0 * s],
            13.0 * s,
            text,
        );
        let tc = timecode(frame, self.transport.frame_rate());
        gui.draw_text(
            font,
            &tc,
            [layout.window_w * 0.80, 28.0 * s],
            13.0 * s,
            accent,
        );

        // Left outline.
        gui.rect(
            0.0,
            layout.top_h,
            layout.left_w,
            layout.main_bottom - layout.top_h,
            panel,
        );
        gui.draw_text(
            font,
            "Scene",
            [12.0 * s, layout.top_h + 20.0 * s],
            14.0 * s,
            text,
        );
        if self.session.document().is_none() {
            gui.draw_text(
                font,
                "(no composition loaded)",
                [12.0 * s, layout.outline_y + 14.0 * s],
                12.0 * s,
                dim,
            );
        }
        for (index, row) in self.outline.iter().enumerate() {
            let y = layout.outline_y + index as f32 * layout.row_h;
            if y + layout.row_h > layout.main_bottom {
                break;
            }
            if self.selected == Some(row.node) {
                gui.rect(0.0, y, layout.left_w, layout.row_h, linear_hex("#2f3d4d"));
            }
            let x = 10.0 * s + row.depth as f32 * 14.0 * s;
            let color = if self.selected == Some(row.node) {
                accent
            } else {
                text
            };
            gui.draw_text(font, &row.label, [x, y + 15.0 * s], 12.0 * s, color);
        }

        // Transport buttons.
        gui.rect(
            0.0,
            layout.main_bottom,
            layout.window_w,
            layout.status_y - layout.main_bottom,
            bar,
        );
        let playing = self.transport.is_playing();
        let looping = self.transport.looping();
        for rect in &layout.buttons {
            let label = match rect.button {
                Button::GoStart => "|<",
                Button::StepBack10 => "-10",
                Button::StepBack => "<",
                Button::PlayPause => {
                    if playing {
                        "Pause"
                    } else {
                        "Play"
                    }
                }
                Button::StepForward => ">",
                Button::StepForward10 => "+10",
                Button::GoEnd => ">|",
                Button::Loop => {
                    if looping {
                        "Loop on"
                    } else {
                        "Loop off"
                    }
                }
            };
            let active = matches!(rect.button, Button::PlayPause | Button::Loop);
            let fill = if active && (playing || looping) {
                linear_hex("#2d4a5c")
            } else {
                linear_hex("#2c2c3a")
            };
            gui.rect(rect.x, rect.y, rect.w, rect.h, fill);
            gui.draw_text(
                font,
                label,
                [rect.x + 8.0 * s, rect.y + 18.0 * s],
                12.0 * s,
                text,
            );
        }

        // Timeline.
        gui.rect(
            0.0,
            layout.main_bottom,
            layout.left_w,
            layout.status_y - layout.main_bottom,
            panel,
        );
        {
            let [x, y, w, h] = layout.timeline;
            let duration = self.transport.duration();
            let spacing = tick_spacing(
                duration,
                self.transport.frame_rate(),
                w,
                MIN_TICK_SPACING_PX * f64::from(s),
            );
            gui.rect(
                x as f32,
                y as f32,
                w as f32,
                h as f32,
                linear_hex("#12121a"),
            );
            for tick in tick_frames(duration, spacing) {
                let tx = frame_to_x(tick, duration, x, w);
                gui.rect(tx as f32, y as f32, s, h as f32, linear_hex("#3a3a4a"));
                let label = match spacing.unit {
                    TickUnit::Frames => format!("{tick}"),
                    TickUnit::Seconds => {
                        let rate = self.transport.frame_rate();
                        let seconds = tick as f64 * f64::from(rate.denominator())
                            / f64::from(rate.numerator());
                        format!("{seconds:.0}s")
                    }
                };
                gui.draw_text(
                    font,
                    &label,
                    [tx as f32 + 3.0 * s, (y + h + 10.0 * f64::from(s)) as f32],
                    10.0 * s,
                    dim,
                );
            }
            let playhead = frame_to_x(frame, duration, x, w);
            gui.rect(
                playhead as f32 - s,
                y as f32,
                2.0 * s,
                h as f32 + 4.0 * s,
                accent,
            );
        }

        // Status bar.
        gui.rect(
            0.0,
            layout.status_y,
            layout.window_w,
            layout.window_h - layout.status_y,
            linear_hex("#101018"),
        );
        let status = match self.session.diagnostics().first() {
            Some(diagnostic) => (diagnostic.to_string(), red),
            None => ("ok".to_owned(), linear_hex("#7bd88f")),
        };
        gui.draw_text(
            font,
            &status.0,
            [12.0 * s, layout.status_y + 19.0 * s],
            12.0 * s,
            status.1,
        );
        gui.draw_text(
            font,
            "Space play  Left/Right step  L loop  R reload  Esc quit",
            [layout.window_w * 0.36, layout.status_y + 19.0 * s],
            11.0 * s,
            dim,
        );
        let metrics = format!("{:.0} fps   dropped {}", ctx.time.fps, self.dropped);
        gui.draw_text(
            font,
            &metrics,
            [layout.window_w * 0.80, layout.status_y + 19.0 * s],
            11.0 * s,
            dim,
        );
    }
}

impl FerrousApp for PreviewApp {
    fn setup(&mut self, ctx: &mut AppContext<'_>) {
        ctx.render.enable_pure_2d(Color::rgb(0.05, 0.05, 0.07));
        ctx.render
            .set_2d_pixel_ratio(ctx.window.scale_factor() as f32);
        match LivePainter::new(ctx.render.renderer_mut()) {
            Ok(painter) => self.painter = Some(painter),
            Err(error) => {
                *self.fatal.borrow_mut() = Some(error);
                ctx.request_exit();
            }
        }
    }

    fn update(&mut self, ctx: &mut AppContext<'_>) {
        self.run_update(ctx);
    }

    fn draw_ui(&mut self, dc: &mut DrawContext<'_, '_>) {
        self.run_draw_ui(dc);
    }
}

// ---------------------------------------------------------------------------
// Drawing helpers
// ---------------------------------------------------------------------------

fn draw_selection(
    renderer: &mut ferrous_renderer::Renderer,
    plan: &RenderPlan,
    selected: NodeId,
    fit: ViewFit,
    pad: (f64, f64),
    window: (f32, f32),
) {
    let scale = fit.scale();
    let (pad_x, pad_y) = pad;
    let (window_w, window_h) = window;
    let mut min = (f64::INFINITY, f64::INFINITY);
    let mut max = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    let mut found = false;
    for item in plan.items() {
        if item.node != selected {
            continue;
        }
        let Some((hw, hh)) = local_half_extents(item.shape) else {
            continue;
        };
        for (x, y) in [(-hw, -hh), (hw, -hh), (hw, hh), (-hw, hh)] {
            let point = item.transform.apply(motioner_core::Vec2::new(x, y));
            min.0 = min.0.min(point.x);
            min.1 = min.1.min(point.y);
            max.0 = max.0.max(point.x);
            max.1 = max.1.max(point.y);
            found = true;
        }
    }
    if !found {
        return;
    }
    let x0 = pad_x + min.0 * scale;
    let x1 = pad_x + max.0 * scale;
    let y0 = pad_y + min.1 * scale;
    let y1 = pad_y + max.1 * scale;
    let color = linear_hex("#4fc3f7");
    let t = 1.0_f32;
    push_window_rect(
        renderer,
        window_w,
        window_h,
        x0 as f32,
        y0 as f32,
        x1 as f32,
        y0 as f32 + t,
        color,
        60.0,
    );
    push_window_rect(
        renderer,
        window_w,
        window_h,
        x0 as f32,
        y1 as f32 - t,
        x1 as f32,
        y1 as f32,
        color,
        60.0,
    );
    push_window_rect(
        renderer,
        window_w,
        window_h,
        x0 as f32,
        y0 as f32,
        x0 as f32 + t,
        y1 as f32,
        color,
        60.0,
    );
    push_window_rect(
        renderer,
        window_w,
        window_h,
        x1 as f32 - t,
        y0 as f32,
        x1 as f32,
        y1 as f32,
        color,
        60.0,
    );
}

fn local_half_extents(shape: ItemShape) -> Option<(f64, f64)> {
    match shape {
        ItemShape::Rect { width, height, .. } => Some((width * 0.5, height * 0.5)),
        ItemShape::Circle { radius, .. } => Some((radius, radius)),
        // Text bounds need font metrics; the selection stays in the outline.
        ItemShape::Text { .. } => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn push_window_rect(
    renderer: &mut ferrous_renderer::Renderer,
    window_w: f32,
    window_h: f32,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    color: [f32; 4],
    z: f32,
) {
    let width = x1 - x0;
    let height = y1 - y0;
    if !width.is_finite() || width <= 0.0 || !height.is_finite() || height <= 0.0 {
        return;
    }
    let cx = (x0 + x1) * 0.5 - window_w * 0.5;
    let cy = window_h * 0.5 - (y0 + y1) * 0.5;
    renderer.draw_2d_shape(ShapeInstance::rect_z(
        Vec2::new(cx, cy),
        z,
        Vec2::new(width, height),
        color,
    ));
}

fn linear_hex(hex: &str) -> [f32; 4] {
    Color::hex(hex).to_linear_f32()
}

fn linear_rgba(color: CoreColor) -> [f32; 4] {
    [
        srgb_to_linear(color.r),
        srgb_to_linear(color.g),
        srgb_to_linear(color.b),
        f32::from(color.a) / 255.0,
    ]
}

fn srgb_to_linear(value: u8) -> f32 {
    let channel = f32::from(value) / 255.0;
    if channel <= 0.040_45 {
        channel / 12.92
    } else {
        ((channel + 0.055) / 1.055).powf(2.4)
    }
}

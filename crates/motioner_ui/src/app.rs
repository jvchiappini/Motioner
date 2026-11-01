//! The editor app: state, input handling and the per-frame draw order.
//!
//! `update` reads input against the [`Layout`] computed for this frame and
//! mutates state; `draw_ui` evaluates the plan for the transport frame,
//! paints the composition through [`LivePainter`] in the engine's `Pure2D`
//! pass, and draws the chrome in the UI pass on top. The wall clock only
//! drives the transport and chrome animation (hover fades, the reload pulse);
//! the composition pixels depend only on the integer frame.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use ferrous_engine::{
    AppContext, Color as EngineColor, CursorIcon, DrawContext, FerrousApp, KeyCode, MouseButton,
};
use ferrous_renderer::Renderer;
use motioner_core::{NodeId, Vec2};
use motioner_render::{LivePainter, RenderError, TextExtent, ViewFit};

use crate::bounds::{self, Bounds};
use crate::input::{action_for, apply, step_selection, Effect, Modifiers};
use crate::inspect::{inspect_comp, inspect_node};
use crate::layout::{
    resize_panels, scroll_into_view, Hit, Layout, PanelSizes, Rect, Splitter, TopBarToggle,
    TransportButton,
};
use crate::paint::Painter;
use crate::panels::{self, InspectorContent, Stats, TooltipRequest, View};
use crate::theme::{size, Theme};
use crate::{
    outline_rows, skipped_frames, x_to_frame, FileWatch, OutlineRow, PreviewOptions,
    PreviewSession, Transport,
};

/// Keys polled every frame for shortcuts.
const SHORTCUT_KEYS: [KeyCode; 11] = [
    KeyCode::Space,
    KeyCode::ArrowLeft,
    KeyCode::ArrowRight,
    KeyCode::ArrowUp,
    KeyCode::ArrowDown,
    KeyCode::Home,
    KeyCode::End,
    KeyCode::KeyL,
    KeyCode::KeyR,
    KeyCode::KeyG,
    KeyCode::Escape,
];

/// Hover time before a tooltip appears.
const TOOLTIP_DELAY: Duration = Duration::from_millis(450);
/// How long the reload pulse in the top bar lasts.
const RELOAD_PULSE: Duration = Duration::from_millis(1200);

/// An in-progress pointer drag.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Drag {
    /// Scrubbing the playhead.
    Scrub,
    /// Resizing a panel: where the drag started and the sizes at that time.
    Splitter {
        which: Splitter,
        origin: (f32, f32),
        start: PanelSizes,
    },
}

/// Pointer state carried between frames.
#[derive(Debug, Clone, Copy)]
struct Pointer {
    hover: Option<Hit>,
    pressed: Option<Hit>,
    drag: Option<Drag>,
    hover_since: Instant,
    cursor: CursorIcon,
    layer_scroll: f32,
    track_scroll: f32,
    inspector_scroll: f32,
}

/// The preview window's state.
pub(crate) struct PreviewApp {
    session: PreviewSession,
    transport: Transport,
    watch: FileWatch,
    options: PreviewOptions,
    outline: Vec<OutlineRow>,
    selected: Option<NodeId>,
    painter: Option<LivePainter>,
    fatal: Rc<RefCell<Option<RenderError>>>,
    theme: Theme,
    sizes: PanelSizes,
    pointer: Pointer,
    layout: Option<Layout>,
    safe_area: bool,
    start_playing: bool,
    frames_presented: u64,
    dropped: u64,
    eval_ms: f32,
    last_reload: Option<Instant>,
    started: Instant,
    pixel_ratio: f32,
    /// Selection seen by the last `update`, to reveal a new selection once.
    revealed: Option<NodeId>,
    /// Height of the inspector content drawn last frame, for scroll limits.
    inspector_content_h: f32,
}

impl PreviewApp {
    pub(crate) fn new(
        session: PreviewSession,
        transport: Transport,
        options: PreviewOptions,
        fatal: Rc<RefCell<Option<RenderError>>>,
    ) -> Self {
        let watch = FileWatch::new(session.path());
        let outline = session.document().map(outline_rows).unwrap_or_default();
        let selected = options
            .select
            .and_then(|index| outline.get(index))
            .map(|row| row.node);
        let start_playing = !options.paused;
        let now = Instant::now();
        Self {
            session,
            transport,
            watch,
            options,
            outline,
            selected,
            painter: None,
            fatal,
            theme: Theme::dark(),
            sizes: PanelSizes::default(),
            pointer: Pointer {
                hover: None,
                pressed: None,
                drag: None,
                hover_since: now,
                cursor: CursorIcon::Default,
                layer_scroll: 0.0,
                track_scroll: 0.0,
                inspector_scroll: 0.0,
            },
            layout: None,
            safe_area: false,
            start_playing,
            frames_presented: 0,
            dropped: 0,
            eval_ms: 0.0,
            last_reload: None,
            started: now,
            pixel_ratio: 0.0,
            revealed: None,
            inspector_content_h: 0.0,
        }
    }

    fn now(ctx: &AppContext<'_>) -> Duration {
        Duration::from_secs_f64(ctx.time.elapsed.max(0.0))
    }

    fn scale(&self, ctx: &AppContext<'_>) -> f32 {
        self.options
            .ui_scale
            .unwrap_or(ctx.window.scale_factor() as f32)
    }

    fn selected_row(&self) -> Option<usize> {
        let selected = self.selected?;
        self.outline.iter().position(|row| row.node == selected)
    }

    /// Reloads the document. A failed reload keeps the last good document and
    /// leaves the transport untouched; a good one re-times it without
    /// stopping playback and keeps the selection when the node still exists.
    fn reload(&mut self, now: Duration) {
        if self.session.reload().is_err() {
            return;
        }
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
        if self.selected_row().is_none() {
            self.selected = None;
        }
        self.last_reload = Some(Instant::now());
    }

    fn handle_effect(&mut self, effect: Effect, ctx: &mut AppContext<'_>, now: Duration) {
        match effect {
            Effect::None => {}
            Effect::Reload => self.reload(now),
            Effect::Quit => ctx.request_exit(),
            Effect::ToggleSafeArea => self.safe_area = !self.safe_area,
            Effect::SelectRelative(delta) => {
                let next = step_selection(self.selected_row(), self.outline.len(), delta);
                self.selected = next.and_then(|i| self.outline.get(i)).map(|row| row.node);
            }
        }
    }

    fn scrub_to(&mut self, layout: &Layout, x: f32) {
        let scrub = layout.timeline.scrub;
        let frame = x_to_frame(
            f64::from(x),
            self.transport.duration(),
            f64::from(scrub.x),
            f64::from(scrub.w),
        );
        self.transport.set_frame(frame);
    }

    fn activate(&mut self, hit: Hit, ctx: &mut AppContext<'_>, now: Duration) {
        let action = match hit {
            Hit::Transport(TransportButton::ToStart) => crate::Action::GoToStart,
            Hit::Transport(TransportButton::StepBack) => crate::Action::Step(-1),
            Hit::Transport(TransportButton::PlayPause) => crate::Action::TogglePlay,
            Hit::Transport(TransportButton::StepForward) => crate::Action::Step(1),
            Hit::Transport(TransportButton::ToEnd) => crate::Action::GoToEnd,
            Hit::Transport(TransportButton::Loop) => crate::Action::ToggleLoop,
            Hit::Toggle(TopBarToggle::SafeArea) => crate::Action::ToggleSafeArea,
            _ => return,
        };
        if self.session.document().is_none() && matches!(hit, Hit::Transport(_)) {
            return;
        }
        let effect = apply(action, &mut self.transport, now);
        self.handle_effect(effect, ctx, now);
    }

    /// Selects the top-most layer under a canvas point (window pixels).
    fn pick_canvas(&mut self, layout: &Layout, renderer: &Renderer, x: f32, y: f32) {
        let frame = self.transport.frame();
        let Some(comp_rect) = comp_rect(&self.session, layout) else {
            return;
        };
        let zoom = f64::from(comp_rect.w)
            / f64::from(
                self.session
                    .composition()
                    .map_or(1, motioner_core::Composition::width),
            );
        let point = Vec2::new(
            (f64::from(x) - f64::from(comp_rect.x)) / zoom,
            (f64::from(y) - f64::from(comp_rect.y)) / zoom,
        );
        let painter = self.painter.as_ref();
        let measure = |text: &str, size: f64| -> Option<TextExtent> {
            painter?.text_extent(renderer, text, size)
        };
        self.selected = self
            .session
            .evaluate(frame)
            .and_then(|(document, plan)| bounds::hit_node(document, plan, point, &measure));
    }

    fn handle_pointer(&mut self, ctx: &mut AppContext<'_>, layout: &Layout, now: Duration) {
        let (mx, my) = ctx.input.mouse_pos_f32();
        let hover = layout.hit(
            mx,
            my,
            self.outline.len(),
            self.pointer.layer_scroll,
            self.pointer.track_scroll,
        );
        if self.pointer.drag.is_none() && hover != self.pointer.hover {
            self.pointer.hover = hover;
            self.pointer.hover_since = Instant::now();
        }
        if ctx.input.button_just_pressed(MouseButton::Left) {
            self.on_press(ctx, layout, hover, (mx, my));
        }
        if ctx.input.is_button_down(MouseButton::Left) {
            self.on_drag(layout, (mx, my));
        }
        if ctx.input.button_just_released(MouseButton::Left) {
            if let (Some(pressed), Some(hover)) = (self.pointer.pressed, hover) {
                if pressed == hover {
                    self.activate(hover, ctx, now);
                }
            }
            self.transport.end_scrub();
            self.pointer.drag = None;
            self.pointer.pressed = None;
        }
        let scroll = ctx.input.scroll_y();
        if scroll.abs() > 0.01 {
            self.on_scroll(layout, hover, scroll, shift_held(ctx));
        }
        let cursor = cursor_for(self.pointer.drag, hover);
        if cursor != self.pointer.cursor {
            ctx.window.set_cursor(cursor);
            self.pointer.cursor = cursor;
        }
    }

    fn on_press(
        &mut self,
        ctx: &AppContext<'_>,
        layout: &Layout,
        hover: Option<Hit>,
        at: (f32, f32),
    ) {
        self.pointer.pressed = hover;
        match hover {
            Some(Hit::Scrub) if self.session.document().is_some() => {
                self.transport.begin_scrub();
                self.scrub_to(layout, at.0);
                self.pointer.drag = Some(Drag::Scrub);
            }
            Some(Hit::Splitter(which)) => {
                self.pointer.drag = Some(Drag::Splitter {
                    which,
                    origin: at,
                    start: self.sizes,
                });
            }
            Some(Hit::LayerRow(i) | Hit::Track(i)) => {
                self.selected = self.outline.get(i).map(|row| row.node);
            }
            Some(Hit::Canvas) => self.pick_canvas(layout, ctx.render.renderer(), at.0, at.1),
            _ => {}
        }
    }

    fn on_drag(&mut self, layout: &Layout, at: (f32, f32)) {
        match self.pointer.drag {
            Some(Drag::Scrub) => self.scrub_to(layout, at.0),
            Some(Drag::Splitter {
                which,
                origin,
                start,
            }) => {
                let delta = (
                    (at.0 - origin.0) / layout.metrics.scale,
                    (at.1 - origin.1) / layout.metrics.scale,
                );
                self.sizes = resize_panels(start, which, delta);
            }
            None => {}
        }
    }

    fn on_scroll(&mut self, layout: &Layout, hover: Option<Hit>, scroll: f32, shift: bool) {
        let rows = self.outline.len();
        match hover {
            Some(Hit::Scrub) => {
                let direction = if scroll > 0.0 { -1 } else { 1 };
                self.transport.step(direction * if shift { 10 } else { 1 });
            }
            Some(Hit::LayerRow(_) | Hit::LayerEye(_) | Hit::LayerLock(_)) => {
                let max = scroll_max(
                    rows,
                    layout.row_h,
                    layout.layers.body.h,
                    layout.metrics.px(size::GRID * 2.0),
                );
                self.pointer.layer_scroll =
                    (self.pointer.layer_scroll - scroll * layout.row_h).clamp(0.0, max);
            }
            Some(Hit::Track(_)) => {
                let max = scroll_max(rows, layout.row_h, layout.timeline.tracks.h, 0.0);
                self.pointer.track_scroll =
                    (self.pointer.track_scroll - scroll * layout.row_h).clamp(0.0, max);
            }
            Some(Hit::Inspector) => {
                let max = (self.inspector_content_h - layout.inspector.body.h).max(0.0);
                self.pointer.inspector_scroll =
                    (self.pointer.inspector_scroll - scroll * layout.row_h).clamp(0.0, max);
            }
            _ => {}
        }
    }

    fn run_update(&mut self, ctx: &mut AppContext<'_>) {
        if self
            .options
            .exit_after_frames
            .is_some_and(|limit| self.frames_presented >= limit)
        {
            ctx.request_exit();
            return;
        }
        let now = Self::now(ctx);
        let scale = self.scale(ctx);
        let dpr = ctx.window.scale_factor() as f32;
        if (dpr - self.pixel_ratio).abs() > 1e-4 {
            ctx.render.set_2d_pixel_ratio(dpr);
            self.pixel_ratio = dpr;
        }
        let layout = Layout::compute(
            ctx.window_size.0 as f32,
            ctx.window_size.1 as f32,
            scale,
            self.sizes,
        );

        let modifiers = Modifiers {
            shift: shift_held(ctx),
        };
        for key in SHORTCUT_KEYS {
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
            if self.session.document().is_some() {
                self.transport.play(now);
            }
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

        self.handle_pointer(ctx, &layout, now);
        if self.selected != self.revealed {
            self.reveal_selection(&layout);
        }
        // Recompute with the possibly changed panel sizes so draw matches.
        self.layout = Some(Layout::compute(
            ctx.window_size.0 as f32,
            ctx.window_size.1 as f32,
            scale,
            self.sizes,
        ));
    }

    /// Scrolls the layer list and the timeline so a newly selected row is
    /// visible, and resets the inspector to its top.
    fn reveal_selection(&mut self, layout: &Layout) {
        self.revealed = self.selected;
        self.pointer.inspector_scroll = 0.0;
        let Some(row) = self.selected_row() else {
            return;
        };
        let top = row as f32 * layout.row_h;
        let pad = layout.metrics.px(size::GRID);
        self.pointer.layer_scroll = scroll_into_view(
            self.pointer.layer_scroll,
            top,
            layout.row_h + pad * 2.0,
            layout.layers.body.h,
        );
        self.pointer.track_scroll = scroll_into_view(
            self.pointer.track_scroll,
            top,
            layout.row_h,
            layout.timeline.tracks.h,
        );
    }

    fn tooltip(&self, layout: &Layout) -> Option<TooltipRequest> {
        if self.pointer.pressed.is_some() || self.pointer.hover_since.elapsed() < TOOLTIP_DELAY {
            return None;
        }
        let (label, shortcut, anchor) = match self.pointer.hover? {
            Hit::Transport(button) => {
                let anchor = layout.transport.iter().find(|(b, _)| *b == button)?.1;
                let (label, key) = match button {
                    TransportButton::ToStart => ("Go to start", "Home"),
                    TransportButton::StepBack => ("Previous frame", "Left"),
                    TransportButton::PlayPause if self.transport.is_playing() => ("Pause", "Space"),
                    TransportButton::PlayPause => ("Play", "Space"),
                    TransportButton::StepForward => ("Next frame", "Right"),
                    TransportButton::ToEnd => ("Go to end", "End"),
                    TransportButton::Loop => ("Loop playback", "L"),
                };
                (label, Some(key), anchor)
            }
            Hit::Toggle(TopBarToggle::SafeArea) => {
                ("Safe areas", Some("G"), layout.safe_area_toggle)
            }
            Hit::LayerEye(i) => (
                "Visibility (read-only for now)",
                None,
                layout.layer_eye(layout.layer_row(i, self.pointer.layer_scroll)),
            ),
            Hit::LayerLock(i) => (
                "Lock (read-only for now)",
                None,
                layout.layer_lock(layout.layer_row(i, self.pointer.layer_scroll)),
            ),
            _ => return None,
        };
        Some(TooltipRequest {
            anchor,
            label,
            shortcut,
        })
    }

    /// Paints the backdrop and the composition in the `Pure2D` pass and
    /// returns the on-screen bounds of the selected and hovered layers.
    fn paint_canvas(
        &mut self,
        renderer: &mut Renderer,
        layout: &Layout,
        frame: i64,
    ) -> CanvasBounds {
        let comp_rect = comp_rect(&self.session, layout);
        let window = (layout.window.w, layout.window.h);
        let background = self
            .session
            .composition()
            .map(motioner_core::Composition::background);
        panels::canvas::paint_backdrop(
            renderer,
            &self.theme,
            window,
            layout.canvas,
            comp_rect.zip(background),
            layout.metrics.px(24.0),
        );
        let hovered_node = match self.pointer.hover {
            Some(Hit::LayerRow(i) | Hit::LayerEye(i) | Hit::LayerLock(i) | Hit::Track(i)) => {
                self.outline.get(i).map(|r| r.node)
            }
            _ => None,
        };
        let stale = self.session.plan_frame() != Some(frame);
        let started = Instant::now();
        let mut out = CanvasBounds::default();
        let (Some((document, plan)), Some(rect)) = (self.session.evaluate(frame), comp_rect) else {
            return out;
        };
        if stale {
            self.eval_ms = started.elapsed().as_secs_f32() * 1000.0;
        }
        let comp = document.composition();
        if let Some(painter) = self.painter.as_mut() {
            let fit = ViewFit::contain(
                f64::from(comp.width()),
                f64::from(comp.height()),
                rect.to_f64(),
                f64::from(window.0),
                f64::from(window.1),
            );
            painter.paint(renderer, document, plan, fit);
        }
        let zoom = f64::from(rect.w) / f64::from(comp.width());
        let painter = self.painter.as_ref();
        let renderer = &*renderer;
        let measure = |text: &str, size: f64| -> Option<TextExtent> {
            painter?.text_extent(renderer, text, size)
        };
        let to_screen = |b: Bounds| {
            let x0 = (f64::from(rect.x) + b.min_x * zoom).floor() as f32;
            let y0 = (f64::from(rect.y) + b.min_y * zoom).floor() as f32;
            let x1 = (f64::from(rect.x) + b.max_x * zoom).ceil() as f32;
            let y1 = (f64::from(rect.y) + b.max_y * zoom).ceil() as f32;
            Rect::new(x0, y0, x1 - x0, y1 - y0)
        };
        if let Some(b) = self
            .selected
            .and_then(|node| bounds::node_bounds(document, plan, node, &measure))
        {
            out.selection = Some(to_screen(b));
            out.selection_size = Some((b.max_x - b.min_x, b.max_y - b.min_y));
        }
        out.hover = hovered_node
            .and_then(|node| bounds::node_bounds(document, plan, node, &measure))
            .map(to_screen);
        out
    }

    /// What the inspector shows for the current selection at `frame`.
    fn inspector_content(&self, frame: i64) -> InspectorContent {
        let Some(document) = self.session.document() else {
            return InspectorContent::Empty;
        };
        let layer = self
            .selected
            .zip(self.selected_row())
            .and_then(|(node, row)| {
                let sections = inspect_node(document, node, frame)?;
                Some(InspectorContent::Layer {
                    name: self.outline[row].name.clone(),
                    kind: self.outline[row].kind,
                    sections,
                })
            });
        layer.unwrap_or_else(|| {
            InspectorContent::Composition(inspect_comp(document.composition(), self.outline.len()))
        })
    }

    fn run_draw_ui(&mut self, dc: &mut DrawContext<'_, '_>) {
        let ctx = &mut *dc.ctx;
        let layout = self.layout.unwrap_or_else(|| {
            Layout::compute(
                ctx.window_size.0 as f32,
                ctx.window_size.1 as f32,
                self.scale(ctx),
                self.sizes,
            )
        });
        let ui_fps = ctx.time.fps;
        if let Some(limit) = self.options.exit_after_frames {
            self.frames_presented = self.frames_presented.saturating_add(1);
            if self.frames_presented >= limit {
                ctx.request_exit();
            }
        }

        // Frame selection is the only wall-clock use; the pixels depend on the
        // integer frame below. `update` already advanced the transport.
        let frame = self.transport.frame();
        let canvas = self.paint_canvas(ctx.render.renderer_mut(), &layout, frame);
        let comp_rect = comp_rect(&self.session, &layout);
        let inspector = self.inspector_content(frame);
        let file_name = self
            .session
            .path()
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("untitled");
        let zoom = match (comp_rect, self.session.composition()) {
            (Some(rect), Some(comp)) => f64::from(rect.w) / f64::from(comp.width()),
            _ => 0.0,
        };
        let reload_pulse = self.last_reload.map_or(0.0, |at| {
            1.0 - (at.elapsed().as_secs_f32() / RELOAD_PULSE.as_secs_f32()).min(1.0)
        });
        let tooltip = self.tooltip(&layout);
        let view = View {
            layout: &layout,
            file_name,
            document: self.session.document(),
            outline: &self.outline,
            selected: self.selected_row(),
            hover: self.pointer.hover,
            pressed: self.pointer.pressed,
            frame,
            duration: self.transport.duration(),
            rate: self.transport.frame_rate(),
            playing: self.transport.is_playing(),
            looping: self.transport.looping(),
            diagnostics: self.session.diagnostics(),
            comp_rect,
            zoom,
            selection_bounds: canvas.selection,
            selection_size: canvas.selection_size,
            hover_bounds: canvas.hover,
            safe_area: self.safe_area,
            stats: Stats {
                ui_fps,
                eval_ms: self.eval_ms,
                dropped: self.dropped,
            },
            layer_scroll: self.pointer.layer_scroll,
            track_scroll: self.pointer.track_scroll,
            inspector_scroll: self.pointer.inspector_scroll,
            reload_pulse,
            inspector: &inspector,
            tooltip,
        };

        let mut p = Painter::new(dc.gui, dc.font, &self.theme, layout.metrics);
        panels::canvas::draw_overlays(&mut p, &view, self.started.elapsed().as_secs_f32());
        p.layer();
        panels::top_bar::draw(&mut p, &view);
        panels::layers::draw(&mut p, &view);
        self.inspector_content_h = panels::inspector::draw(&mut p, &view);
        panels::timeline::draw(&mut p, &view);
        panels::status::draw(&mut p, &view);
        if let Some(request) = view.tooltip {
            p.layer();
            crate::widgets::tooltip(
                &mut p,
                request.anchor,
                layout.window,
                request.label,
                request.shortcut,
            );
        }
    }
}

impl FerrousApp for PreviewApp {
    fn setup(&mut self, ctx: &mut AppContext<'_>) {
        let pasteboard = self.theme.pasteboard;
        ctx.render.enable_pure_2d(EngineColor::rgb(
            pasteboard[0],
            pasteboard[1],
            pasteboard[2],
        ));
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

/// On-screen bounds produced while painting the canvas.
#[derive(Debug, Clone, Copy, Default)]
struct CanvasBounds {
    selection: Option<Rect>,
    selection_size: Option<(f64, f64)>,
    hover: Option<Rect>,
}

/// The cursor for the current drag or hover target.
fn cursor_for(drag: Option<Drag>, hover: Option<Hit>) -> CursorIcon {
    match (drag, hover) {
        (Some(Drag::Splitter { which, .. }), _) | (None, Some(Hit::Splitter(which))) => match which
        {
            Splitter::Left | Splitter::Right => CursorIcon::ColResize,
            Splitter::Timeline => CursorIcon::RowResize,
        },
        (Some(Drag::Scrub), _) | (None, Some(Hit::Scrub)) => CursorIcon::EwResize,
        (
            None,
            Some(
                Hit::Transport(_)
                | Hit::Toggle(_)
                | Hit::LayerRow(_)
                | Hit::LayerEye(_)
                | Hit::LayerLock(_)
                | Hit::Track(_),
            ),
        ) => CursorIcon::Pointer,
        _ => CursorIcon::Default,
    }
}

fn shift_held(ctx: &AppContext<'_>) -> bool {
    ctx.input.is_key_down(KeyCode::ShiftLeft) || ctx.input.is_key_down(KeyCode::ShiftRight)
}

/// How far a list of `rows` rows can scroll inside `visible` pixels.
fn scroll_max(rows: usize, row_h: f32, visible: f32, padding: f32) -> f32 {
    (rows as f32 * row_h + padding - visible).max(0.0)
}

/// The composition's letterboxed rectangle inside the canvas, in whole
/// window pixels, or `None` when nothing is loaded or the canvas is empty.
fn comp_rect(session: &PreviewSession, layout: &Layout) -> Option<Rect> {
    let comp = session.composition()?;
    let short_side = layout.canvas.w.min(layout.canvas.h);
    let margin = layout
        .metrics
        .px(size::GRID * 8.0)
        .min((short_side * 0.06).round());
    let area = layout.canvas.inset(margin);
    let fit = ViewFit::contain(
        f64::from(comp.width()),
        f64::from(comp.height()),
        area.to_f64(),
        f64::from(layout.window.w),
        f64::from(layout.window.h),
    );
    if !fit.is_drawable() {
        return None;
    }
    let w = (f64::from(comp.width()) * fit.scale()) as f32;
    let h = (f64::from(comp.height()) * fit.scale()) as f32;
    let x = (area.center_x() - w * 0.5).round();
    let y = (area.center_y() - h * 0.5).round();
    Some(Rect::new(x, y, w.round(), h.round()))
}

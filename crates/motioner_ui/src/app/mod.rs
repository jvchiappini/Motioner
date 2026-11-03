//! The editor window: input, the per-frame draw order and the live control
//! endpoint, on top of the window-free [`Editor`].
//!
//! Every gesture, shortcut and field edit becomes a [`Command`] that
//! [`Editor::execute`] runs, the same path `motioner edit` and `motioner
//! ctl` use (D-0024). `run_update` reads input against the pure layouts and
//! polls the control endpoint; `run_draw_ui` paints the composition through
//! [`LivePainter`] and the chrome on top. The wall clock only drives the
//! transport, the chrome animation and the debounced overrides write; the
//! composition pixels depend only on the document and the integer frame.

mod canvas;
mod drag;
mod pointer;
mod press;

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use ferrous_engine::{
    AppContext, Color as EngineColor, DrawContext, FerrousApp, KeyCode, Screenshot,
};
use motioner_core::{Frame, NodeId};
use motioner_render::LivePainter;

use crate::capture::Capture;
use crate::command::{Command, Panel as PanelId};
use crate::control::{ControlServer, Pending, Request};
use crate::editor::{Editor, Effect};
use crate::gizmo::Guide;
use crate::input::{command_for, Modifiers, Toggles};
use crate::inspect::{self, Editing, FieldTarget};
use crate::layout::{Hit, Layout, Rect, TopBarToggle, TransportButton};
use crate::paint::Painter;
use crate::panels::{self, OrphanRow, Stats, TooltipRequest, View};
use crate::text_edit::{EditKey, TextEdit};
use crate::theme::Theme;
use crate::timeline_model;
use crate::{skipped_frames, FileWatch, InspectLayout, PreviewOptions, UiError};

use self::pointer::Pointer;

/// Keys polled every frame for shortcuts while no field is being edited.
/// Space is handled separately so a Space+drag can pan the canvas.
const SHORTCUT_KEYS: [KeyCode; 17] = [
    KeyCode::ArrowLeft,
    KeyCode::ArrowRight,
    KeyCode::ArrowUp,
    KeyCode::ArrowDown,
    KeyCode::Home,
    KeyCode::End,
    KeyCode::KeyL,
    KeyCode::KeyR,
    KeyCode::KeyG,
    KeyCode::KeyK,
    KeyCode::KeyU,
    KeyCode::KeyF,
    KeyCode::Digit1,
    KeyCode::Delete,
    KeyCode::KeyZ,
    KeyCode::KeyY,
    KeyCode::Escape,
];

/// Hover time before a tooltip appears.
const TOOLTIP_DELAY: Duration = Duration::from_millis(450);
/// How long the reload pulse in the top bar lasts.
const RELOAD_PULSE: Duration = Duration::from_millis(1200);
/// How long the "Copied" confirmation stays.
const COPIED_PULSE: Duration = Duration::from_millis(1200);

/// The preview window's state.
#[allow(clippy::struct_excessive_bools)] // independent UI flags, not a state machine
pub(crate) struct PreviewApp {
    editor: Editor,
    watch: FileWatch,
    options: PreviewOptions,
    /// Project faces for the live painter, when the app supplied them.
    fonts: Option<motioner_render::FontSet>,
    timeline_layout: timeline_model::RowsLayout,
    inspector_layout: InspectLayout,
    editing: Option<Editing>,
    field_error: Option<FieldTarget>,
    painter: Option<LivePainter>,
    /// Generation whose images the painter has uploaded.
    painter_generation: Option<u64>,
    fatal: Rc<RefCell<Option<UiError>>>,
    capture: Capture,
    /// Whether the `--screenshot` capture was queued.
    screenshot_queued: bool,
    control: Option<ControlServer>,
    /// Control requests waiting for a screenshot.
    waiting: Vec<Pending>,
    theme: Theme,
    pointer: Pointer,
    layout: Option<Layout>,
    guides: Vec<Guide>,
    marquee: Option<Rect>,
    pending_pick: Option<motioner_core::Vec2>,
    last_bounds: Option<crate::Bounds>,
    copied_at: Option<Instant>,
    start_playing: bool,
    quit: bool,
    frames_presented: u64,
    dropped: u64,
    eval_ms: f32,
    last_reload: Option<Instant>,
    started: Instant,
    /// The transport clock of this frame (time since the window opened).
    clock: Duration,
    pixel_ratio: f32,
    revealed: Option<NodeId>,
    revealed_keys: usize,
    /// Space is held: a canvas drag pans instead of picking.
    space_down: bool,
    /// The Space+drag moved, so releasing Space must not toggle playback.
    space_dragged: bool,
}

impl PreviewApp {
    pub(crate) fn new(
        editor: Editor,
        options: PreviewOptions,
        control: Option<ControlServer>,
        fatal: Rc<RefCell<Option<UiError>>>,
    ) -> Self {
        let watch = FileWatch::new(editor.session().path());
        let start_playing = !options.paused;
        let fonts = options.fonts.clone();
        Self {
            editor,
            watch,
            options,
            fonts,
            timeline_layout: timeline_model::RowsLayout {
                rows: Vec::new(),
                height: 0.0,
            },
            inspector_layout: InspectLayout {
                items: Vec::new(),
                height: 0.0,
            },
            editing: None,
            field_error: None,
            painter: None,
            painter_generation: None,
            fatal,
            capture: Capture::default(),
            screenshot_queued: false,
            control,
            waiting: Vec::new(),
            theme: Theme::dark(),
            pointer: Pointer::new(),
            layout: None,
            guides: Vec::new(),
            marquee: None,
            pending_pick: None,
            last_bounds: None,
            copied_at: None,
            start_playing,
            quit: false,
            frames_presented: 0,
            dropped: 0,
            eval_ms: 0.0,
            last_reload: None,
            started: Instant::now(),
            clock: Duration::ZERO,
            pixel_ratio: 0.0,
            revealed: None,
            revealed_keys: 0,
            space_down: false,
            space_dragged: false,
        }
    }

    fn scale(&self, ctx: &AppContext<'_>) -> f32 {
        self.options
            .ui_scale
            .unwrap_or(ctx.window.scale_factor() as f32)
    }

    fn frame(&self) -> Frame {
        self.editor.frame()
    }

    // ---- commands --------------------------------------------------------

    /// Runs a command for the user and applies its effect. Returns whether it
    /// succeeded; a failed gesture command simply does nothing.
    fn run(&mut self, command: &Command) -> bool {
        match self.editor.execute(command, self.clock) {
            Ok(outcome) => {
                self.apply_effect(outcome.effect);
                true
            }
            Err(_) => false,
        }
    }

    fn apply_effect(&mut self, effect: Effect) {
        match effect {
            Effect::None => {}
            Effect::Quit => self.quit = true,
            Effect::Reloaded => self.reloaded(),
            Effect::Copy(text) => {
                crate::clipboard::copy(&text);
                self.copied_at = Some(Instant::now());
            }
        }
    }

    /// Painter caches and canvas bounds are stale after a reload.
    fn reloaded(&mut self) {
        if let Some(painter) = self.painter.as_mut() {
            painter.clear_cache();
        }
        self.last_reload = Some(Instant::now());
        self.last_bounds = None;
    }

    /// The identity of an outline row's node.
    fn row_id(&self, row: usize) -> Option<String> {
        let node = self.editor.outline().get(row)?.node;
        Some(self.editor.node_identity(node))
    }

    // ---- layouts and scrolling -------------------------------------------

    fn scroll_px(&self, panel: PanelId) -> f32 {
        let scale = self.layout.map_or(1.0, |l| l.metrics.scale);
        self.editor.view().scroll(panel) as f32 * scale
    }

    fn set_scroll_px(&mut self, panel: PanelId, px: f32) {
        let scale = self.layout.map_or(1.0, |l| l.metrics.scale).max(1e-3);
        self.editor
            .view_mut()
            .set_scroll(panel, f64::from(px / scale));
    }

    fn layout_inspector(&mut self) {
        let Some(layout) = self.layout else {
            return;
        };
        let body = layout.inspector.body;
        let mut scroll = self.scroll_px(PanelId::Inspector);
        self.inspector_layout =
            inspect::layout(self.editor.inspector(), body, scroll, layout.metrics);
        // Content can shrink (another selection, fewer keys): keep the
        // scroll inside the new range.
        let max = (self.inspector_layout.height - body.h).max(0.0);
        if scroll > max {
            scroll = max;
            self.set_scroll_px(PanelId::Inspector, max);
            self.inspector_layout =
                inspect::layout(self.editor.inspector(), body, scroll, layout.metrics);
        }
    }

    fn layout_timeline(&mut self) {
        let Some(layout) = self.layout else {
            return;
        };
        self.timeline_layout = timeline_model::layout(
            self.editor.timeline(),
            &layout,
            self.scroll_px(PanelId::Timeline),
        );
    }

    fn relayout(&mut self, ctx: &AppContext<'_>) -> Layout {
        let layout = Layout::compute(
            ctx.window_size.0 as f32,
            ctx.window_size.1 as f32,
            self.scale(ctx),
            self.editor.view().sizes,
        );
        self.layout = Some(layout);
        self.layout_inspector();
        self.layout_timeline();
        layout
    }

    // ---- text fields -------------------------------------------------------

    fn begin_edit(&mut self, target: FieldTarget) {
        let text = self.editor.field_text(target);
        self.field_error = None;
        self.editing = Some(Editing {
            target,
            editor: TextEdit::select_all(text),
            invalid: false,
        });
    }

    fn commit_edit(&mut self) {
        let Some(editing) = self.editing.as_ref() else {
            return;
        };
        let target = editing.target;
        let command = self.editor.field_command(target, editing.editor.text());
        if command.is_some_and(|command| self.run(&command)) {
            self.editing = None;
            self.field_error = None;
        } else {
            if let Some(editing) = self.editing.as_mut() {
                editing.invalid = true;
            }
            self.field_error = Some(target);
        }
    }

    fn cancel_edit(&mut self) {
        self.editing = None;
        self.field_error = None;
    }

    // ---- keyboard ----------------------------------------------------------

    fn handle_keys(&mut self, ctx: &AppContext<'_>) {
        if self.editing.is_some() {
            self.space_down = false;
            self.space_dragged = false;
            self.handle_edit_keys(ctx);
            return;
        }
        // A Space tap toggles playback on release; Space+drag pans instead.
        if ctx.input.just_pressed(KeyCode::Space) {
            self.space_down = true;
            self.space_dragged = false;
        }
        if ctx.input.just_released(KeyCode::Space) {
            if self.space_down && !self.space_dragged {
                self.run(&Command::TogglePlay);
            }
            self.space_down = false;
            self.space_dragged = false;
        }
        let modifiers = modifiers(ctx);
        let toggles = Toggles {
            looping: self.editor.transport().looping(),
            safe_area: self.editor.view().safe_area,
            only_animated: self.editor.only_animated(),
        };
        for key in SHORTCUT_KEYS {
            if ctx.input.just_pressed(key) {
                if let Some(command) = command_for(key, modifiers, toggles) {
                    self.run(&command);
                    break;
                }
            }
        }
    }

    fn handle_edit_keys(&mut self, ctx: &AppContext<'_>) {
        let shift = modifiers(ctx).shift;
        let ctrl = modifiers(ctx).ctrl;
        let key = [
            (KeyCode::Backspace, EditKey::Backspace),
            (KeyCode::Delete, EditKey::Delete),
            (KeyCode::ArrowLeft, EditKey::Left),
            (KeyCode::ArrowRight, EditKey::Right),
            (KeyCode::Home, EditKey::Home),
            (KeyCode::End, EditKey::End),
            (KeyCode::Enter, EditKey::Commit),
            (KeyCode::Escape, EditKey::Cancel),
        ]
        .into_iter()
        .find(|(code, _)| ctx.input.just_pressed(*code))
        .map(|(_, key)| key)
        .or_else(|| (ctrl && ctx.input.just_pressed(KeyCode::KeyA)).then_some(EditKey::SelectAll));
        match key {
            Some(EditKey::Commit) => self.commit_edit(),
            Some(EditKey::Cancel) => self.cancel_edit(),
            Some(key) => {
                if let Some(editing) = self.editing.as_mut() {
                    editing.editor.key(key, shift);
                    editing.invalid = false;
                    self.field_error = None;
                }
            }
            None => {}
        }
        if let Some(editing) = self.editing.as_mut() {
            for ch in ctx.input.typed_chars() {
                if !ch.is_control() {
                    editing.editor.insert_char(*ch);
                    editing.invalid = false;
                    self.field_error = None;
                }
            }
        }
    }

    // ---- control endpoint ----------------------------------------------------

    /// Answers control requests without blocking the frame.
    fn poll_control(&mut self) {
        let Some(server) = self.control.as_ref() else {
            return;
        };
        for pending in server.poll() {
            match pending.request.clone() {
                Ok(Request::Command(command)) => {
                    let result = self.editor.execute(&command, self.clock);
                    match result {
                        Ok(outcome) => {
                            pending.reply_ok_summary(&outcome.summary);
                            self.apply_effect(outcome.effect);
                        }
                        Err(error) => pending.reply_error(&error),
                    }
                }
                Ok(Request::State) => pending.reply_ok_json(&self.editor.state_json()),
                Ok(Request::Query { node }) => match self.editor.query_json(node.as_deref()) {
                    Ok(json) => pending.reply_ok_json(&json),
                    Err(error) => pending.reply_error(&error),
                },
                Ok(Request::Screenshot(path)) => {
                    self.capture.request(path);
                    self.waiting.push(pending);
                }
                Err(error) => pending.reply_error(&error),
            }
        }
    }

    /// Consumes finished captures: replies to control requests and closes
    /// the window after the `--screenshot` capture. Returns `true` when the
    /// window should close.
    fn handle_captures(&mut self) -> bool {
        let mut close = false;
        for (path, result) in self.capture.take_done() {
            if self.options.screenshot.as_ref() == Some(&path) && self.screenshot_queued {
                if let Err(message) = &result {
                    *self.fatal.borrow_mut() = Some(UiError::Screenshot(message.clone()));
                }
                close = true;
                continue;
            }
            if let Some(index) = self
                .waiting
                .iter()
                .position(|p| p.screenshot_path() == Some(&path))
            {
                let pending = self.waiting.remove(index);
                match result {
                    Ok((path, width, height)) => pending.reply_screenshot(&path, width, height),
                    Err(message) => pending
                        .reply_error(&crate::CommandError::new(crate::ErrorKind::Io, message)),
                }
            }
        }
        close
    }

    // ---- frame -------------------------------------------------------------

    fn run_update(&mut self, ctx: &mut AppContext<'_>) {
        if self.handle_captures() || self.frame_limit_reached() || self.quit {
            ctx.request_exit();
            return;
        }
        self.clock = Duration::from_secs_f64(ctx.time.elapsed.max(0.0));
        let dpr = ctx.window.scale_factor() as f32;
        if (dpr - self.pixel_ratio).abs() > 1e-4 {
            ctx.render.set_2d_pixel_ratio(dpr);
            self.pixel_ratio = dpr;
        }
        let layout = self.relayout(ctx);
        self.editor.view_mut().fit_zoom = canvas::fit_zoom(self.editor.session(), &layout);

        self.handle_keys(ctx);
        let now = Instant::now();
        if (self.watch.check(now) || self.editor.session_mut().poll_changes(now))
            && self.editor.reload(self.clock)
        {
            self.reloaded();
        }
        if self.start_playing {
            self.start_playing = false;
            self.run(&Command::Play);
        }
        let transport = self.editor.transport_mut();
        if transport.is_playing() {
            let before = transport.frame();
            transport.advance(self.clock);
            let after = transport.frame();
            let duration = transport.duration();
            self.dropped = self
                .dropped
                .saturating_add(skipped_frames(before, after, duration));
        }
        self.poll_control();
        self.editor.sync_models();

        self.handle_pointer(ctx, &layout);
        self.reveal(&layout);
        self.editor.session_mut().flush_overrides(Instant::now());
        self.relayout(ctx);
        self.editor.sync_models();
        self.marquee = self.pointer.marquee_rect();
    }

    /// Whether `--exit-after-frames` closes the window now. With
    /// `--screenshot` the window closes after the capture instead.
    fn frame_limit_reached(&self) -> bool {
        self.options.screenshot.is_none()
            && self
                .options
                .exit_after_frames
                .is_some_and(|limit| self.frames_presented >= limit)
    }

    fn tooltip(&self, layout: &Layout) -> Option<TooltipRequest> {
        if self.pointer.pressed.is_some() || self.pointer.hover_since.elapsed() < TOOLTIP_DELAY {
            return None;
        }
        let layer_scroll = self.scroll_px(PanelId::Layers);
        let (label, shortcut, anchor) = match self.pointer.hover? {
            Hit::Transport(button) => {
                let anchor = layout.transport.iter().find(|(b, _)| *b == button)?.1;
                let playing = self.editor.transport().is_playing();
                let (label, key) = match button {
                    TransportButton::ToStart => ("Go to start", "Home"),
                    TransportButton::StepBack => ("Previous frame", "Left"),
                    TransportButton::PlayPause if playing => ("Pause", "Space"),
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
            Hit::SnapToggle => ("Snap to guides and layers", None, layout.snap_toggle),
            Hit::ZoomPill => (
                "Fit the canvas to the composition",
                Some("F"),
                layout.zoom_pill,
            ),
            Hit::OrphanRebind(_) => (
                "Re-bind this override to the suggested layer",
                None,
                Rect::default(),
            ),
            Hit::LayerEye(i) => (
                "Toggle visibility",
                None,
                layout.layer_eye(layout.layer_row(i, layer_scroll)),
            ),
            Hit::LayerLock(i) => (
                "Toggle lock",
                None,
                layout.layer_lock(layout.layer_row(i, layer_scroll)),
            ),
            Hit::InspectStopwatch(item) => (
                "Animate this property",
                None,
                self.inspector_item_rect(item),
            ),
            Hit::InspectSegment { item, segment } if self.is_ease_row(item) => (
                motioner_core::Ease::ALL
                    .get(segment)
                    .map_or("", |(_, name)| name),
                None,
                self.inspector_item_rect(item),
            ),
            Hit::TimelineStopwatch(row) => (
                "Animate this property",
                None,
                self.timeline_row_widget(row, 0),
            ),
            Hit::TimelineKeyToggle(row) => (
                "Add or remove a key at the playhead",
                Some("K"),
                self.timeline_row_widget(row, 1),
            ),
            Hit::InspectCopy => ("Copy the identity to the clipboard", None, Rect::default()),
            _ => return None,
        };
        Some(TooltipRequest {
            anchor,
            label,
            shortcut,
        })
    }

    fn is_ease_row(&self, item: usize) -> bool {
        matches!(
            self.inspector_row_value(item),
            Some(inspect::RowValue::Ease { .. })
        )
    }

    fn inspector_item_rect(&self, item: usize) -> Rect {
        match self.inspector_layout.items.get(item) {
            Some(inspect::Item::Row {
                stopwatch, rect, ..
            }) => stopwatch.unwrap_or(*rect),
            Some(inspect::Item::Section { rect, .. } | inspect::Item::Header { rect, .. }) => *rect,
            None => Rect::default(),
        }
    }

    fn timeline_row_widget(&self, row: usize, which: usize) -> Rect {
        self.timeline_layout
            .rows
            .iter()
            .find(|r| r.row == row)
            .and_then(|r| match which {
                0 => r.stopwatch,
                1 => r.key,
                _ => r.next,
            })
            .unwrap_or_default()
    }

    fn frame_facts(&self, layout: &Layout) -> FrameFacts {
        let view = canvas::viewport(self.editor.session(), self.editor.view(), layout);
        let comp_rect = view.map(|view| view.rect);
        let zoom = view.map_or(0.0, |view| view.scale);
        let reload_pulse = self.last_reload.map_or(0.0, |at| {
            1.0 - (at.elapsed().as_secs_f32() / RELOAD_PULSE.as_secs_f32()).min(1.0)
        });
        let orphans: Vec<OrphanRow> = self
            .editor
            .orphan_suggestions()
            .into_iter()
            .map(|(entry, suggestions)| OrphanRow {
                id: entry.id,
                property: entry.property,
                suggestion: suggestions.into_iter().next(),
            })
            .collect();
        let document = self.editor.session().document();
        let layer_states: Vec<(bool, bool)> = self
            .editor
            .outline()
            .iter()
            .map(|row| {
                let visible = document
                    .and_then(|document| document.node(row.node))
                    .is_some_and(motioner_core::Node::is_visible);
                (visible, self.editor.node_locked(row.node))
            })
            .collect();
        let copied = self.copied_at.is_some_and(|at| at.elapsed() < COPIED_PULSE);
        FrameFacts {
            comp_rect,
            zoom,
            reload_pulse,
            orphans,
            layer_states,
            copied,
        }
    }

    fn run_draw_ui(&mut self, dc: &mut DrawContext<'_, '_>) {
        let ctx = &mut *dc.ctx;
        let layout = self.layout.unwrap_or_else(|| {
            Layout::compute(
                ctx.window_size.0 as f32,
                ctx.window_size.1 as f32,
                self.scale(ctx),
                self.editor.view().sizes,
            )
        });
        let ui_fps = ctx.time.fps;
        self.frames_presented = self.frames_presented.saturating_add(1);
        if let Some(path) = self.options.screenshot.clone() {
            if !self.screenshot_queued
                && self.frames_presented >= self.options.exit_after_frames.unwrap_or(1)
            {
                self.screenshot_queued = true;
                self.capture.request(path);
            }
        }
        if self.capture.poll(self.frames_presented) {
            ctx.request_screenshot();
        }

        let frame = self.frame();
        let canvas = self.paint_canvas(ctx.render.renderer_mut(), &layout, frame);
        let file_name = self
            .editor
            .session()
            .path()
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("untitled")
            .to_owned();
        let facts = self.frame_facts(&layout);
        let tooltip = self.tooltip(&layout);
        let editor = &self.editor;
        let transport = editor.transport();
        let view = View {
            layout: &layout,
            file_name: &file_name,
            document: editor.session().document(),
            outline: editor.outline(),
            selected: editor.selected_row(),
            hover: self.pointer.hover,
            pressed: self.pointer.pressed,
            frame,
            duration: transport.duration(),
            rate: transport.frame_rate(),
            playing: transport.is_playing(),
            looping: transport.looping(),
            diagnostics: editor.session().diagnostics(),
            warnings: editor.session().warnings(),
            comp_rect: facts.comp_rect,
            zoom: facts.zoom,
            selection_bounds: canvas.selection,
            selection_size: canvas.size,
            hover_bounds: canvas.hover,
            safe_area: editor.view().safe_area,
            snap: editor.view().snap,
            guides: &self.guides,
            marquee: self.marquee,
            stats: Stats {
                ui_fps,
                eval_ms: self.eval_ms,
                dropped: self.dropped,
            },
            layer_scroll: self.scroll_px(PanelId::Layers),
            inspector_scroll: self.scroll_px(PanelId::Inspector),
            reload_pulse: facts.reload_pulse,
            inspector: editor.inspector(),
            inspector_layout: &self.inspector_layout,
            editing: self.editing.as_ref(),
            inspector_field_error: self.field_error,
            timeline: editor.timeline(),
            timeline_layout: &self.timeline_layout,
            selected_keys: editor.selected_keys(),
            expanded: editor.expanded(),
            orphans: &facts.orphans,
            layer_states: &facts.layer_states,
            copied: facts.copied,
            retime_delta: self.pointer.retime_delta(),
            tooltip,
        };

        let mut p = Painter::new(dc.gui, dc.font, &self.theme, layout.metrics);
        panels::canvas::draw_overlays(&mut p, &view, self.started.elapsed().as_secs_f32());
        p.layer();
        panels::top_bar::draw(&mut p, &view);
        panels::layers::draw(&mut p, &view);
        panels::inspector::draw(&mut p, &view);
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
        let painter = match self.fonts.clone() {
            Some(fonts) => LivePainter::with_fonts(ctx.render.renderer_mut(), fonts),
            None => LivePainter::new(ctx.render.renderer_mut()),
        };
        match painter {
            Ok(painter) => self.painter = Some(painter),
            Err(error) => {
                *self.fatal.borrow_mut() = Some(UiError::Painter(error));
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

    fn on_screenshot(&mut self, screenshot: Screenshot) {
        self.capture.deliver(
            self.options.png_writer,
            screenshot.width,
            screenshot.height,
            &screenshot.rgba,
        );
    }
}

/// Values the editor draws from that only depend on the session, not on the
/// frame painting.
struct FrameFacts {
    comp_rect: Option<Rect>,
    zoom: f64,
    reload_pulse: f32,
    orphans: Vec<OrphanRow>,
    layer_states: Vec<(bool, bool)>,
    copied: bool,
}

/// Modifier state for this frame.
fn modifiers(ctx: &AppContext<'_>) -> Modifiers {
    Modifiers {
        shift: ctx.input.is_key_down(KeyCode::ShiftLeft)
            || ctx.input.is_key_down(KeyCode::ShiftRight),
        ctrl: ctx.input.is_key_down(KeyCode::ControlLeft)
            || ctx.input.is_key_down(KeyCode::ControlRight),
        alt: ctx.input.is_key_down(KeyCode::AltLeft) || ctx.input.is_key_down(KeyCode::AltRight),
    }
}

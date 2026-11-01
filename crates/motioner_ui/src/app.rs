//! The editor app: state, input handling and the per-frame draw order.
//!
//! `run_update` reads input against the pure layouts (window, inspector,
//! timeline) and turns every gesture into an [`EditCommand`]; `run_draw_ui`
//! evaluates the plan for the transport frame, paints the composition through
//! [`LivePainter`] and draws the chrome on top. The wall clock only drives the
//! transport, the chrome animation and the debounced overrides write; the
//! composition pixels depend only on the document and the integer frame.
//!
//! Picking a layer is resolved while painting, because measuring text needs
//! the renderer: a press stores a point in `pending_pick` and the next
//! `paint_canvas` resolves it. Gizmo hit testing uses the selection bounds
//! computed by the previous frame (one frame of latency, invisible in use).

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;
use std::time::{Duration, Instant};

use ferrous_engine::{
    AppContext, Color as EngineColor, CursorIcon, DrawContext, FerrousApp, KeyCode, MouseButton,
};
use ferrous_renderer::Renderer;
use motioner_core::{
    split_property, Axis, ContentField, Ease, Frame, NodeContent, NodeId, OverrideAction,
    OverrideEntry, OverrideSet, OverrideValue, Property, TextAlign, Vec2,
};
use motioner_render::{LivePainter, RenderError, TextExtent, ViewFit};

use crate::bounds::{self, Bounds};
use crate::edit::{self, EditCommand, KeyRef, MoveKey, UndoStack};
use crate::gizmo::{self, Guide, Handle};
use crate::input::{action_for, apply, step_selection, Effect, Modifiers};
use crate::inspect::{self, Editing, FieldTarget};
use crate::layout::{
    resize_panels, scroll_into_view, Hit, Layout, PanelSizes, Rect, Splitter, TopBarToggle,
    TransportButton,
};
use crate::paint::Painter;
use crate::panels::{self, OrphanRow, Stats, TooltipRequest, View};
use crate::text_edit::{EditKey, TextEdit};
use crate::theme::{size, Theme};
use crate::timeline_model::{self, RowKind, TimelineRows};
use crate::{
    outline_rows, skipped_frames, x_to_frame, FileWatch, InspectLayout, InspectModel, OutlineRow,
    PreviewOptions, PreviewSession, Transport,
};

/// Keys polled every frame for shortcuts while no field is being edited.
const SHORTCUT_KEYS: [KeyCode; 16] = [
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
    KeyCode::KeyK,
    KeyCode::KeyU,
    KeyCode::Delete,
    KeyCode::KeyZ,
    KeyCode::KeyY,
    KeyCode::Escape,
];

/// Hover time before a tooltip appears.
const TOOLTIP_DELAY: Duration = Duration::from_millis(450);
/// How long the reload pulse in the top bar lasts.
const RELOAD_PULSE: Duration = Duration::from_millis(1200);
/// Two clicks on the same target within this window are a double click.
const DOUBLE_CLICK: Duration = Duration::from_millis(420);
/// Pointer travel below this is a click, not a scrub.
const CLICK_SLOP: f32 = 3.0;
/// Snap threshold in screen pixels.
const SNAP_PX: f64 = 6.0;

/// An in-progress pointer drag.
enum Drag {
    /// Scrubbing the playhead.
    Scrub,
    /// Resizing a panel.
    Splitter {
        which: Splitter,
        origin: (f32, f32),
        start: PanelSizes,
    },
    /// Dragging a canvas gizmo handle.
    Gizmo {
        handle: Handle,
        start_set: OverrideSet,
        start_pointer: Vec2,
        start_bounds: Bounds,
        start_position: Vec2,
        start_scale: Vec2,
        start_rotation: f64,
        center: Vec2,
        start_angle: f64,
        other_bounds: Vec<Bounds>,
    },
    /// Scrubbing a numeric field.
    Field {
        target: FieldTarget,
        start_set: OverrideSet,
        start_value: f64,
        start_x: f32,
        moved: bool,
    },
    /// Retiming the selected keys.
    Keys {
        start_set: OverrideSet,
        keys: Vec<MoveKey>,
        start_x: f32,
        delta: Frame,
    },
    /// Box-selecting keys in the track area.
    Marquee {
        start: (f32, f32),
        current: (f32, f32),
    },
}

/// Pointer state carried between frames.
struct Pointer {
    hover: Option<Hit>,
    pressed: Option<Hit>,
    drag: Option<Drag>,
    hover_since: Instant,
    cursor: CursorIcon,
    layer_scroll: f32,
    track_scroll: f32,
    inspector_scroll: f32,
    last_click: Option<(Instant, Option<Hit>)>,
}

/// The preview window's state.
#[allow(clippy::struct_excessive_bools)] // editor flags (safe area, snap, playing)
pub(crate) struct PreviewApp {
    session: PreviewSession,
    transport: Transport,
    watch: FileWatch,
    options: PreviewOptions,
    outline: Vec<OutlineRow>,
    timeline: TimelineRows,
    timeline_layout: timeline_model::RowsLayout,
    expanded: BTreeSet<usize>,
    pinned: BTreeSet<usize>,
    only_animated: bool,
    selected: Option<NodeId>,
    selected_keys: Vec<KeyRef>,
    inspector: InspectModel,
    inspector_layout: InspectLayout,
    editing: Option<Editing>,
    field_error: Option<FieldTarget>,
    undo: UndoStack,
    painter: Option<LivePainter>,
    fatal: Rc<RefCell<Option<RenderError>>>,
    theme: Theme,
    sizes: PanelSizes,
    pointer: Pointer,
    layout: Option<Layout>,
    safe_area: bool,
    snap: bool,
    guides: Vec<Guide>,
    marquee: Option<Rect>,
    pending_pick: Option<Vec2>,
    last_bounds: Option<Bounds>,
    copied_at: Option<Instant>,
    start_playing: bool,
    frames_presented: u64,
    dropped: u64,
    eval_ms: f32,
    last_reload: Option<Instant>,
    started: Instant,
    pixel_ratio: f32,
    revealed: Option<NodeId>,
    model_generation: u64,
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
        let mut app = Self {
            session,
            transport,
            watch,
            options,
            outline,
            timeline: TimelineRows::default(),
            timeline_layout: timeline_model::RowsLayout {
                rows: Vec::new(),
                height: 0.0,
            },
            expanded: BTreeSet::new(),
            pinned: BTreeSet::new(),
            only_animated: false,
            selected,
            selected_keys: Vec::new(),
            inspector: InspectModel {
                header: None,
                rows: Vec::new(),
            },
            inspector_layout: InspectLayout {
                items: Vec::new(),
                height: 0.0,
            },
            editing: None,
            field_error: None,
            undo: UndoStack::default_limit(),
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
                last_click: None,
            },
            layout: None,
            safe_area: false,
            snap: true,
            guides: Vec::new(),
            marquee: None,
            pending_pick: None,
            last_bounds: None,
            copied_at: None,
            start_playing,
            frames_presented: 0,
            dropped: 0,
            eval_ms: 0.0,
            last_reload: None,
            started: now,
            pixel_ratio: 0.0,
            revealed: None,
            model_generation: u64::MAX,
        };
        if let Some(row) = app.selected_row() {
            app.expanded.insert(row);
        }
        app.refresh_models();
        app
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

    fn frame(&self) -> Frame {
        self.transport.frame()
    }

    // ---- models ----------------------------------------------------------

    fn refresh_models(&mut self) {
        self.outline = self
            .session
            .document()
            .map(outline_rows)
            .unwrap_or_default();
        if self.selected_row().is_none() {
            self.selected = None;
            self.selected_keys.clear();
        }
        self.rebuild_timeline();
        self.rebuild_inspector();
        self.model_generation = self.session.generation();
    }

    fn rebuild_timeline(&mut self) {
        let Some(document) = self.session.document() else {
            self.timeline = TimelineRows::default();
            return;
        };
        self.timeline =
            TimelineRows::compute(document, &self.outline, &self.expanded, self.only_animated);
    }

    fn rebuild_inspector(&mut self) {
        let Some(document) = self.session.document() else {
            self.inspector = InspectModel {
                header: None,
                rows: Vec::new(),
            };
            return;
        };
        let frame = self.frame();
        let fallback = self
            .selected_row()
            .and_then(|row| self.outline.get(row))
            .map_or("", |row| row.name.as_str());
        let mut model = match self.selected {
            Some(node) => inspect::inspect_node(
                document,
                node,
                frame,
                Some(self.session.overrides()),
                fallback,
            ),
            None => Some(inspect::inspect_comp(
                document.composition(),
                self.outline.len(),
            )),
        }
        .unwrap_or(InspectModel {
            header: None,
            rows: Vec::new(),
        });
        if !self.selected_keys.is_empty() {
            let ease = self
                .selected_keys
                .first()
                .and_then(|key| self.key_ease(key))
                .unwrap_or(Ease::Linear);
            model.rows.push(inspect::Row {
                section: "Keys",
                label: "Ease",
                value: inspect::RowValue::Text {
                    target: FieldTarget::Ease,
                    text: ease.name().to_owned(),
                },
                channel: None,
            });
        }
        self.inspector = model;
        self.layout_inspector();
    }

    fn layout_inspector(&mut self) {
        let Some(layout) = self.layout else {
            return;
        };
        self.inspector_layout = inspect::layout(
            &self.inspector,
            layout.inspector.body,
            self.pointer.inspector_scroll,
            layout.metrics,
        );
    }

    fn layout_timeline(&mut self) {
        let Some(layout) = self.layout else {
            return;
        };
        self.timeline_layout =
            timeline_model::layout(&self.timeline, &layout, self.pointer.track_scroll);
    }

    /// The transform channel a key reference points at.
    fn key_channel(&self, key: &KeyRef) -> Option<(NodeId, Property, Option<Axis>)> {
        let document = self.session.document()?;
        let node = document.find(&key.id)?;
        let (base, axis) = split_property(&key.property);
        let property = Property::from_name(base)?;
        Some((node, property, axis))
    }

    fn key_ease(&self, key: &KeyRef) -> Option<Ease> {
        let (node, property, axis) = self.key_channel(key)?;
        let document = self.session.document()?;
        let n = document.node(node)?;
        let t = n.transform();
        let channel = |value: &motioner_core::Animatable<f64>| {
            value
                .keys()
                .iter()
                .find(|k| k.frame == key.frame)
                .map(|k| k.ease)
        };
        match (property, axis) {
            (Property::Rotation, _) => channel(&t.rotation),
            (Property::Opacity, _) => channel(&t.opacity),
            (Property::Position, Some(axis)) => channel(t.position.axis(axis)),
            (Property::Scale, Some(axis)) => channel(t.scale.axis(axis)),
            (Property::Anchor, Some(axis)) => channel(t.anchor.axis(axis)),
            (Property::Position, None) => find_pair_ease(&t.position, key.frame),
            (Property::Scale, None) => find_pair_ease(&t.scale, key.frame),
            (Property::Anchor, None) => find_pair_ease(&t.anchor, key.frame),
        }
    }

    fn key_value(&self, key: &KeyRef) -> Option<(OverrideValue, Ease)> {
        let (node, property, axis) = self.key_channel(key)?;
        let document = self.session.document()?;
        let n = document.node(node)?;
        let t = n.transform();
        let scalar = |value: &motioner_core::Animatable<f64>| {
            value
                .keys()
                .iter()
                .find(|k| k.frame == key.frame)
                .map(|k| (OverrideValue::Number(k.value), k.ease))
        };
        match (property, axis) {
            (Property::Rotation, _) => scalar(&t.rotation),
            (Property::Opacity, _) => scalar(&t.opacity),
            (Property::Position, Some(axis)) => scalar(t.position.axis(axis)),
            (Property::Scale, Some(axis)) => scalar(t.scale.axis(axis)),
            (Property::Anchor, Some(axis)) => scalar(t.anchor.axis(axis)),
            (Property::Position, None) => pair_value(&t.position, key.frame),
            (Property::Scale, None) => pair_value(&t.scale, key.frame),
            (Property::Anchor, None) => pair_value(&t.anchor, key.frame),
        }
    }
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
        self.refresh_models();
        self.last_reload = Some(Instant::now());
        self.last_bounds = None;
    }

    // ---- editing ---------------------------------------------------------

    fn apply_command(&mut self, command: &EditCommand, now: Instant) -> bool {
        let before = self.session.overrides().clone();
        self.undo.record(before.clone());
        let mut set = before;
        edit::apply(command, &mut set);
        self.commit_set(&set, now)
    }

    fn commit_set(&mut self, set: &OverrideSet, now: Instant) -> bool {
        if self.session.edit_overrides(set, now) {
            self.refresh_models();
            true
        } else {
            false
        }
    }

    fn apply_gesture(&mut self, base: &OverrideSet, command: &EditCommand, now: Instant) -> bool {
        self.undo.record(base.clone());
        let mut set = base.clone();
        edit::apply(command, &mut set);
        self.commit_set(&set, now)
    }

    fn node_identity(&self, node: NodeId) -> String {
        self.session
            .document()
            .and_then(|document| document.node(node))
            .map(|node| node.identity().as_str().to_owned())
            .unwrap_or_default()
    }

    fn node_locked(&self, node: NodeId) -> bool {
        let id = self.node_identity(node);
        self.session
            .overrides()
            .get(&id, motioner_core::LOCKED, None)
            .is_some_and(|entry| entry.action == OverrideAction::Locked(true))
    }

    fn channel_keys(&self, node: NodeId, property: Property, axis: Option<Axis>) -> Vec<Frame> {
        let Some(document) = self.session.document() else {
            return Vec::new();
        };
        let Some(n) = document.node(node) else {
            return Vec::new();
        };
        let t = n.transform();
        match (property, axis) {
            (Property::Position, Some(axis)) => t
                .position
                .axis(axis)
                .keys()
                .iter()
                .map(|k| k.frame)
                .collect(),
            (Property::Scale, Some(axis)) => {
                t.scale.axis(axis).keys().iter().map(|k| k.frame).collect()
            }
            (Property::Anchor, Some(axis)) => {
                t.anchor.axis(axis).keys().iter().map(|k| k.frame).collect()
            }
            (Property::Position, None) => t.position.key_frames(),
            (Property::Scale, None) => t.scale.key_frames(),
            (Property::Anchor, None) => t.anchor.key_frames(),
            (Property::Rotation, _) => t.rotation.keys().iter().map(|k| k.frame).collect(),
            (Property::Opacity, _) => t.opacity.keys().iter().map(|k| k.frame).collect(),
        }
    }

    /// The sampled value of a channel: a number for one axis, a vector for a
    /// whole vector property.
    fn sampled(
        &self,
        node: NodeId,
        property: Property,
        axis: Option<Axis>,
    ) -> Option<OverrideValue> {
        let document = self.session.document()?;
        let n = document.node(node)?;
        let t = n.transform();
        let frame = self.frame();
        Some(match (property, axis) {
            (Property::Position, Some(axis)) => {
                OverrideValue::Number(motioner_eval::sample(t.position.axis(axis), frame))
            }
            (Property::Scale, Some(axis)) => {
                OverrideValue::Number(motioner_eval::sample(t.scale.axis(axis), frame))
            }
            (Property::Anchor, Some(axis)) => {
                OverrideValue::Number(motioner_eval::sample(t.anchor.axis(axis), frame))
            }
            (Property::Position, None) => {
                OverrideValue::Vec2(motioner_eval::sample_vec2(&t.position, frame))
            }
            (Property::Scale, None) => {
                OverrideValue::Vec2(motioner_eval::sample_vec2(&t.scale, frame))
            }
            (Property::Anchor, None) => {
                OverrideValue::Vec2(motioner_eval::sample_vec2(&t.anchor, frame))
            }
            (Property::Rotation, _) => {
                OverrideValue::Number(motioner_eval::sample(&t.rotation, frame))
            }
            (Property::Opacity, _) => {
                OverrideValue::Number(motioner_eval::sample(&t.opacity, frame))
            }
        })
    }

    fn numeric_value(&self, target: FieldTarget) -> Option<f64> {
        let document = self.session.document()?;
        let node = document.node(self.selected?)?;
        let frame = self.frame();
        match target {
            FieldTarget::Transform(property, axis) => {
                let t = node.transform();
                let core_axis = match axis {
                    inspect::Axis::X => Some(Axis::X),
                    inspect::Axis::Y => Some(Axis::Y),
                    inspect::Axis::Single => None,
                };
                match self.sampled(node.id(), property, core_axis) {
                    Some(OverrideValue::Number(number)) => Some(number),
                    Some(OverrideValue::Vec2(vector)) => Some(vector.x),
                    _ => {
                        let _ = t;
                        let _ = frame;
                        None
                    }
                }
            }
            FieldTarget::Content(field) => content_number(node.content(), field),
            _ => None,
        }
    }

    fn display_factor(target: FieldTarget) -> f64 {
        match target {
            FieldTarget::Transform(Property::Scale | Property::Opacity, _) => 100.0,
            _ => 1.0,
        }
    }

    fn command_for_number(
        &self,
        target: FieldTarget,
        value: f64,
        base: &OverrideSet,
    ) -> Option<EditCommand> {
        let selected = self.selected?;
        let id = self.node_identity(selected);
        let frame = self.frame();
        match target {
            FieldTarget::Transform(property, axis) => {
                let core_axis = match axis {
                    inspect::Axis::X if property.is_vector() => Some(Axis::X),
                    inspect::Axis::Y if property.is_vector() => Some(Axis::Y),
                    _ => None,
                };
                let keyed = !self.channel_keys(selected, property, core_axis).is_empty()
                    || base.entries().iter().any(|entry| {
                        let (entry_base, entry_axis) = split_property(&entry.property);
                        entry.id == id && entry_base == property.name() && entry_axis == core_axis
                    });
                if let Some(axis) = core_axis {
                    if keyed {
                        Some(EditCommand::SetKey {
                            id,
                            property: property.name().to_owned(),
                            axis: Some(axis),
                            frame,
                            value: OverrideValue::Number(value),
                        })
                    } else {
                        let mut vector = match self.sampled(selected, property, None)? {
                            OverrideValue::Vec2(vector) => vector,
                            _ => Vec2::ZERO,
                        };
                        match axis {
                            Axis::X => vector.x = value,
                            Axis::Y => vector.y = value,
                        }
                        Some(EditCommand::SetValue {
                            id,
                            property: property.name().to_owned(),
                            value: OverrideValue::Vec2(vector),
                        })
                    }
                } else if keyed {
                    Some(EditCommand::SetKey {
                        id,
                        property: property.name().to_owned(),
                        axis: None,
                        frame,
                        value: OverrideValue::Number(value),
                    })
                } else {
                    Some(EditCommand::SetValue {
                        id,
                        property: property.name().to_owned(),
                        value: OverrideValue::Number(value),
                    })
                }
            }
            FieldTarget::Content(field) => Some(EditCommand::SetValue {
                id,
                property: field.name().to_owned(),
                value: OverrideValue::Number(value),
            }),
            _ => None,
        }
    }
    fn current_name(&self) -> String {
        let Some(document) = self.session.document() else {
            return String::new();
        };
        let Some(node) = self.selected.and_then(|node| document.node(node)) else {
            return String::new();
        };
        self.session
            .overrides()
            .get(node.identity().as_str(), motioner_core::NAME, None)
            .and_then(|entry| match &entry.action {
                OverrideAction::Set(OverrideValue::Text(text)) => Some(text.clone()),
                _ => None,
            })
            .unwrap_or_default()
    }

    fn begin_edit(&mut self, target: FieldTarget, text: String, select_all: bool) {
        self.field_error = None;
        self.editing = Some(Editing {
            target,
            editor: if select_all {
                TextEdit::select_all(text)
            } else {
                TextEdit::new(text)
            },
            invalid: false,
        });
    }

    fn field_text(&self, target: FieldTarget) -> String {
        if let Some(value) = self.numeric_value(target) {
            let factor = Self::display_factor(target);
            let shown = (value * factor * 10.0).round() / 10.0;
            return if shown.fract().abs() < 1e-9 {
                format!("{shown:.0}")
            } else {
                format!("{shown:.1}")
            };
        }
        let Some(node) = self
            .selected
            .and_then(|selected| self.session.document()?.node(selected))
        else {
            return String::new();
        };
        match target {
            FieldTarget::Content(ContentField::Fill) => match node.content() {
                NodeContent::Rect { paint, .. } | NodeContent::Circle { paint, .. } => {
                    paint.fill.map_or_else(|| "none".to_owned(), inspect::hex)
                }
                _ => "none".to_owned(),
            },
            FieldTarget::Content(ContentField::Stroke) => match node.content() {
                NodeContent::Rect { paint, .. } | NodeContent::Circle { paint, .. } => paint
                    .stroke
                    .map_or_else(|| "none".to_owned(), |stroke| inspect::hex(stroke.color)),
                _ => "none".to_owned(),
            },
            FieldTarget::Content(ContentField::Color) => match node.content() {
                NodeContent::Text(text) => inspect::hex(text.color),
                _ => String::new(),
            },
            FieldTarget::Content(ContentField::Text) | FieldTarget::Text => match node.content() {
                NodeContent::Text(text) => text.text.clone(),
                _ => String::new(),
            },
            FieldTarget::Align => match node.content() {
                NodeContent::Text(text) => match text.align {
                    TextAlign::Left => "left",
                    TextAlign::Center => "center",
                    TextAlign::Right => "right",
                }
                .to_owned(),
                _ => String::new(),
            },
            FieldTarget::Name => self.current_name(),
            FieldTarget::Ease => self
                .selected_keys
                .first()
                .and_then(|key| self.key_ease(key))
                .map_or_else(|| "linear".to_owned(), |ease| ease.name().to_owned()),
            _ => String::new(),
        }
    }

    fn reject_edit(&mut self) -> bool {
        if let Some(editing) = self.editing.as_mut() {
            editing.invalid = true;
        }
        self.field_error = self.editing.as_ref().map(|editing| editing.target);
        false
    }

    fn commit_edit(&mut self, now: Instant) -> bool {
        let Some(editing) = self.editing.clone() else {
            return false;
        };
        let text = editing.editor.text().to_owned();
        let Some(id) = self.selected.map(|node| self.node_identity(node)) else {
            self.editing = None;
            return false;
        };
        let command = match self.edit_command(&editing, &text, &id, now) {
            Ok(Some(command)) => command,
            Ok(None) => {
                self.editing = None;
                return true;
            }
            Err(()) => return self.reject_edit(),
        };
        let ok = self.apply_command(&command, now);
        if ok {
            self.editing = None;
            self.field_error = None;
        }
        ok
    }

    /// Turns the editor text into a command for `target`.
    ///
    /// `Ok(None)` means the text was handled without a command (an ease
    /// change); `Err(())` means it did not parse and the field should stay in
    /// the danger colour.
    fn edit_command(
        &mut self,
        editing: &Editing,
        text: &str,
        id: &str,
        now: Instant,
    ) -> Result<Option<EditCommand>, ()> {
        let target = editing.target;
        let command = match target {
            FieldTarget::Transform(..) | FieldTarget::Content(_) if is_numeric_target(target) => {
                let shown = text.trim().parse::<f64>().map_err(|_| ())?;
                if !shown.is_finite() {
                    return Err(());
                }
                let value = shown / Self::display_factor(target);
                let base = self.session.overrides().clone();
                self.command_for_number(target, value, &base)
            }
            FieldTarget::Content(ContentField::Fill | ContentField::Stroke) => {
                let trimmed = text.trim();
                let value = if trimmed.eq_ignore_ascii_case("none") {
                    OverrideValue::Text("none".to_owned())
                } else if motioner_core::Color::parse_hex(trimmed).is_ok() {
                    OverrideValue::Text(trimmed.to_owned())
                } else {
                    return Err(());
                };
                Some(EditCommand::SetValue {
                    id: id.to_owned(),
                    property: match target {
                        FieldTarget::Content(field) => field.name().to_owned(),
                        _ => String::new(),
                    },
                    value,
                })
            }
            FieldTarget::Content(ContentField::Color) => {
                let trimmed = text.trim();
                if motioner_core::Color::parse_hex(trimmed).is_err() {
                    return Err(());
                }
                Some(EditCommand::SetValue {
                    id: id.to_owned(),
                    property: "color".to_owned(),
                    value: OverrideValue::Text(trimmed.to_owned()),
                })
            }
            FieldTarget::Content(ContentField::Text) | FieldTarget::Text | FieldTarget::Name => {
                Some(EditCommand::SetValue {
                    id: id.to_owned(),
                    property: if target == FieldTarget::Name {
                        motioner_core::NAME.to_owned()
                    } else {
                        "text".to_owned()
                    },
                    value: OverrideValue::Text(text.to_owned()),
                })
            }
            FieldTarget::Align => {
                let align = TextAlign::from_name(text.trim()).ok_or(())?;
                let name = match align {
                    TextAlign::Left => "left",
                    TextAlign::Center => "center",
                    TextAlign::Right => "right",
                };
                Some(EditCommand::SetValue {
                    id: id.to_owned(),
                    property: "align".to_owned(),
                    value: OverrideValue::Text(name.to_owned()),
                })
            }
            FieldTarget::Ease => {
                let ease = Ease::from_name(text.trim()).ok_or(())?;
                self.set_ease(ease, now);
                return Ok(None);
            }
            FieldTarget::Content(_) | FieldTarget::Transform(..) | FieldTarget::None => None,
        };
        Ok(command)
    }
    fn cancel_edit(&mut self) {
        self.editing = None;
        self.field_error = None;
    }

    fn set_ease(&mut self, ease: Ease, now: Instant) {
        let keys = self.selected_keys.clone();
        if keys.is_empty() {
            return;
        }
        let mut moves = Vec::new();
        for key in &keys {
            if let Some((value, _)) = self.key_value(key) {
                moves.push(MoveKey {
                    key: key.clone(),
                    to: key.frame,
                    value,
                    ease,
                });
            }
        }
        if moves.is_empty() {
            return;
        }
        let before = self.session.overrides().clone();
        let mut set = before.clone();
        edit::apply(&EditCommand::MoveKeys(moves), &mut set);
        self.undo.record(before);
        if self.commit_set(&set, now) {
            self.selected_keys.clear();
        }
    }

    fn cycle_ease(&mut self, now: Instant) {
        let Some(current) = self
            .selected_keys
            .first()
            .and_then(|key| self.key_ease(key))
        else {
            return;
        };
        let index = Ease::ALL
            .iter()
            .position(|(ease, _)| *ease == current)
            .unwrap_or(0);
        let next = Ease::ALL[(index + 1) % Ease::ALL.len()].0;
        self.set_ease(next, now);
    }

    // ---- toggles ---------------------------------------------------------

    fn toggle_visible(&mut self, node: NodeId) {
        let Some(document) = self.session.document() else {
            return;
        };
        let Some(n) = document.node(node) else {
            return;
        };
        let visible = !n.is_visible();
        let id = self.node_identity(node);
        self.apply_command(&EditCommand::SetVisible { id, visible }, Instant::now());
    }

    fn toggle_lock(&mut self, node: NodeId) {
        let locked = !self.node_locked(node);
        let id = self.node_identity(node);
        self.apply_command(&EditCommand::SetLocked { id, locked }, Instant::now());
    }

    fn toggle_stopwatch(&mut self, node: NodeId, property: Property, axis: Option<Axis>) {
        let value = self
            .sampled(node, property, axis)
            .unwrap_or(OverrideValue::Number(0.0));
        let existing = self.channel_keys(node, property, axis);
        let id = self.node_identity(node);
        self.apply_command(
            &EditCommand::ToggleStopwatch {
                id,
                property,
                axis,
                frame: self.frame(),
                value,
                existing_keys: existing,
            },
            Instant::now(),
        );
    }

    fn toggle_key(&mut self, node: NodeId, property: Property, axis: Option<Axis>) {
        let frame = self.frame();
        let id = self.node_identity(node);
        let command = if self.channel_keys(node, property, axis).contains(&frame) {
            EditCommand::RemoveKey {
                id,
                property: property.name().to_owned(),
                axis,
                frame,
            }
        } else {
            EditCommand::SetKey {
                id,
                property: property.name().to_owned(),
                axis,
                frame,
                value: self
                    .sampled(node, property, axis)
                    .unwrap_or(OverrideValue::Number(0.0)),
            }
        };
        self.apply_command(&command, Instant::now());
    }

    fn jump_key(&mut self, node: NodeId, property: Property, axis: Option<Axis>, forward: bool) {
        let frame = self.frame();
        let keys = self.channel_keys(node, property, axis);
        let target = if forward {
            keys.iter().copied().find(|&key| key > frame)
        } else {
            keys.iter().copied().rev().find(|&key| key < frame)
        };
        if let Some(target) = target {
            self.transport.set_frame(target);
        }
    }

    fn add_keys(&mut self) {
        let Some(node) = self.selected else {
            return;
        };
        let mut channels: Vec<(Property, Option<Axis>)> = Vec::new();
        for (property, _) in Property::ALL {
            let axes: Vec<Option<Axis>> = if property.is_vector() {
                vec![Some(Axis::X), Some(Axis::Y)]
            } else {
                vec![None]
            };
            for axis in axes {
                channels.push((property, axis));
            }
        }
        let animated: Vec<(Property, Option<Axis>)> = channels
            .into_iter()
            .filter(|(property, axis)| !self.channel_keys(node, *property, *axis).is_empty())
            .collect();
        let targets = if animated.is_empty() {
            vec![
                (Property::Position, Some(Axis::X)),
                (Property::Position, Some(Axis::Y)),
            ]
        } else {
            animated
        };
        for (property, axis) in targets {
            self.toggle_key(node, property, axis);
        }
    }
    fn delete_keys(&mut self) {
        let keys = self.selected_keys.clone();
        if keys.is_empty() {
            return;
        }
        let before = self.session.overrides().clone();
        let mut set = before.clone();
        for key in &keys {
            edit::apply(
                &EditCommand::RemoveKey {
                    id: key.id.clone(),
                    property: split_property(&key.property).0.to_owned(),
                    axis: split_property(&key.property).1,
                    frame: key.frame,
                },
                &mut set,
            );
        }
        self.undo.record(before);
        if self.commit_set(&set, Instant::now()) {
            self.selected_keys.clear();
        }
    }

    fn undo(&mut self) {
        let current = self.session.overrides().clone();
        if let Some(set) = self.undo.undo(&current) {
            self.commit_set(&set, Instant::now());
        }
    }

    fn redo(&mut self) {
        let current = self.session.overrides().clone();
        if let Some(set) = self.undo.redo(&current) {
            self.commit_set(&set, Instant::now());
        }
    }

    fn select_layer(&mut self, row: usize) {
        let previous = self.selected_row();
        self.selected = self.outline.get(row).map(|r| r.node);
        self.selected_keys.clear();
        if let Some(previous) = previous {
            if Some(previous) != self.selected_row() && !self.pinned.contains(&previous) {
                self.expanded.remove(&previous);
            }
        }
        if let Some(current) = self.selected_row() {
            if !self.pinned.contains(&current) {
                self.expanded.insert(current);
            }
        }
        self.last_bounds = None;
        self.rebuild_timeline();
        self.rebuild_inspector();
    }

    fn toggle_expanded(&mut self, row: usize, alt: bool) {
        if alt {
            if self.expanded.len() >= self.outline.len() {
                self.expanded.clear();
                self.pinned.clear();
            } else {
                self.expanded = (0..self.outline.len()).collect();
                self.pinned = self.expanded.clone();
            }
        } else if self.expanded.remove(&row) {
            self.pinned.remove(&row);
        } else {
            self.expanded.insert(row);
            self.pinned.insert(row);
        }
        self.rebuild_timeline();
    }

    // ---- effects ---------------------------------------------------------

    fn handle_effect(&mut self, effect: Effect, ctx: &mut AppContext<'_>, now: Duration) -> bool {
        match effect {
            Effect::None => false,
            Effect::Reload => {
                self.reload(now);
                true
            }
            Effect::Quit => {
                ctx.request_exit();
                true
            }
            Effect::ToggleSafeArea => {
                self.safe_area = !self.safe_area;
                true
            }
            Effect::SelectRelative(delta) => {
                if let Some(row) = step_selection(self.selected_row(), self.outline.len(), delta) {
                    self.select_layer(row);
                }
                true
            }
            Effect::Edit(action) => {
                match action {
                    crate::Action::AddKeys => self.add_keys(),
                    crate::Action::DeleteKeys => self.delete_keys(),
                    crate::Action::ToggleOnlyAnimated => {
                        self.only_animated = !self.only_animated;
                        self.rebuild_timeline();
                    }
                    crate::Action::Undo => self.undo(),
                    crate::Action::Redo => self.redo(),
                    _ => {}
                }
                true
            }
        }
    }

    // ---- pointer ---------------------------------------------------------

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

    fn handle_pointer(&mut self, ctx: &mut AppContext<'_>, layout: &Layout) {
        let now_i = Instant::now();
        let (mx, my) = ctx.input.mouse_pos_f32();
        let base_hit = layout.hit(
            mx,
            my,
            self.outline.len(),
            self.pointer.layer_scroll,
            self.pointer.track_scroll,
        );
        let hover = self
            .hit_inspector(layout, mx, my)
            .or_else(|| self.hit_orphans(layout, mx, my))
            .or_else(|| self.hit_timeline(layout, mx, my))
            .or_else(|| self.hit_gizmo(layout, mx, my))
            .or(base_hit);
        if self.pointer.drag.is_none() && hover != self.pointer.hover {
            self.pointer.hover = hover;
            self.pointer.hover_since = now_i;
        }
        let modifiers = modifiers(ctx);
        if ctx.input.button_just_pressed(MouseButton::Left) {
            self.on_press(ctx, layout, hover, (mx, my), modifiers, now_i);
        }
        if ctx.input.is_button_down(MouseButton::Left) {
            self.on_drag(layout, (mx, my), modifiers, now_i);
        }
        if ctx.input.button_just_released(MouseButton::Left) {
            self.on_release(layout, hover);
        }
        let scroll = ctx.input.scroll_y();
        if scroll.abs() > 0.01 {
            self.on_scroll(layout, hover, scroll, modifiers.shift);
        }
        let cursor = self.cursor_for(hover);
        if cursor != self.pointer.cursor {
            ctx.window.set_cursor(cursor);
            self.pointer.cursor = cursor;
        }
    }

    fn hit_inspector(&self, layout: &Layout, mx: f32, my: f32) -> Option<Hit> {
        if !layout.inspector.body.contains(mx, my) {
            return None;
        }
        for (index, item) in self.inspector_layout.items.iter().enumerate() {
            match item {
                inspect::Item::Header { copy, .. } => {
                    if copy.contains(mx, my) {
                        return Some(Hit::InspectCopy);
                    }
                }
                inspect::Item::Row {
                    fields,
                    stopwatch,
                    diamond,
                    segments,
                    ..
                } => {
                    if stopwatch.is_some_and(|r| r.contains(mx, my)) {
                        return Some(Hit::InspectStopwatch(index));
                    }
                    if diamond.is_some_and(|r| r.contains(mx, my)) {
                        return Some(Hit::InspectDiamond(index));
                    }
                    if let Some(segments) = segments {
                        for (segment, rect) in segments.iter().enumerate() {
                            if rect.contains(mx, my) {
                                return Some(Hit::InspectSegment {
                                    item: index,
                                    segment,
                                });
                            }
                        }
                    }
                    for (field, (rect, _)) in fields.iter().enumerate() {
                        if rect.contains(mx, my) {
                            return Some(Hit::InspectField { item: index, field });
                        }
                    }
                }
                inspect::Item::Section { .. } => {}
            }
        }
        Some(Hit::Inspector)
    }

    fn hit_orphans(&self, layout: &Layout, mx: f32, my: f32) -> Option<Hit> {
        if !layout.layers.body.contains(mx, my) {
            return None;
        }
        let count = self.orphans().len();
        if count == 0 {
            return None;
        }
        for index in 0..count {
            let row = layout.orphan_row(index, self.outline.len(), self.pointer.layer_scroll);
            if !row.contains(mx, my) {
                continue;
            }
            if layout.orphan_delete(row).contains(mx, my) {
                return Some(Hit::OrphanDelete(index));
            }
            return Some(Hit::OrphanRow(index));
        }
        None
    }

    fn hit_timeline(&self, layout: &Layout, mx: f32, my: f32) -> Option<Hit> {
        let in_names = layout.timeline.names.contains(mx, my);
        let in_tracks = layout.timeline.tracks.contains(mx, my);
        if !(in_names || in_tracks) {
            return None;
        }
        for row in &self.timeline_layout.rows {
            if !row.rect.contains(mx, my) {
                continue;
            }
            if row.chevron.is_some_and(|r| r.contains(mx, my)) {
                return Some(Hit::LayerChevron(row.row));
            }
            if row.stopwatch.is_some_and(|r| r.contains(mx, my)) {
                return Some(Hit::TimelineStopwatch(row.row));
            }
            if row.prev.is_some_and(|r| r.contains(mx, my)) {
                return Some(Hit::TimelinePrevKey(row.row));
            }
            if row.key.is_some_and(|r| r.contains(mx, my)) {
                return Some(Hit::TimelineKeyToggle(row.row));
            }
            if row.next.is_some_and(|r| r.contains(mx, my)) {
                return Some(Hit::TimelineNextKey(row.row));
            }
            if row.field.is_some_and(|r| r.contains(mx, my)) {
                return Some(Hit::TimelineField(row.row));
            }
            if in_tracks {
                if let Some(frame) = self.diamond_at(row.row, mx, layout) {
                    return Some(Hit::TimelineDiamond {
                        row: row.row,
                        frame,
                    });
                }
            }
            return Some(Hit::TimelineRow(row.row));
        }
        None
    }

    fn hit_gizmo(&self, layout: &Layout, mx: f32, my: f32) -> Option<Hit> {
        if !layout.canvas.contains(mx, my) {
            return None;
        }
        let bounds = self.last_bounds?;
        let point = to_composition(&self.session, layout, mx, my)?;
        gizmo::hit_handle(bounds, point, self.zoom(layout)).map(Hit::Gizmo)
    }

    fn cursor_for(&self, hover: Option<Hit>) -> CursorIcon {
        match (self.pointer.drag.as_ref(), hover) {
            (Some(Drag::Splitter { which, .. }), _) => match which {
                Splitter::Left | Splitter::Right => CursorIcon::ColResize,
                Splitter::Timeline => CursorIcon::RowResize,
            },
            (None, Some(Hit::Splitter(which))) => match which {
                Splitter::Left | Splitter::Right => CursorIcon::ColResize,
                Splitter::Timeline => CursorIcon::RowResize,
            },
            (Some(Drag::Scrub | Drag::Field { .. }), _) | (None, Some(Hit::Scrub)) => {
                CursorIcon::EwResize
            }
            (Some(Drag::Keys { .. }), _) => CursorIcon::EwResize,
            (Some(Drag::Gizmo { handle, .. }), _) => match handle.cursor() {
                gizmo::GizmoCursor::Move => CursorIcon::Move,
                gizmo::GizmoCursor::Grab => CursorIcon::Grabbing,
                gizmo::GizmoCursor::NwseResize => CursorIcon::NwseResize,
                gizmo::GizmoCursor::NeswResize => CursorIcon::NeswResize,
            },
            (None, Some(Hit::Gizmo(handle))) => match handle.cursor() {
                gizmo::GizmoCursor::Move => CursorIcon::Move,
                gizmo::GizmoCursor::Grab => CursorIcon::Grab,
                gizmo::GizmoCursor::NwseResize => CursorIcon::NwseResize,
                gizmo::GizmoCursor::NeswResize => CursorIcon::NeswResize,
            },
            (
                None,
                Some(
                    Hit::Transport(_)
                    | Hit::Toggle(_)
                    | Hit::SnapToggle
                    | Hit::LayerRow(_)
                    | Hit::LayerEye(_)
                    | Hit::LayerLock(_)
                    | Hit::Track(_)
                    | Hit::TimelineRow(_)
                    | Hit::TimelineDiamond { .. }
                    | Hit::TimelineStopwatch(_)
                    | Hit::TimelinePrevKey(_)
                    | Hit::TimelineKeyToggle(_)
                    | Hit::TimelineNextKey(_)
                    | Hit::LayerChevron(_)
                    | Hit::OrphanRow(_)
                    | Hit::OrphanDelete(_)
                    | Hit::InspectStopwatch(_)
                    | Hit::InspectDiamond(_)
                    | Hit::InspectSegment { .. }
                    | Hit::InspectCopy
                    | Hit::InspectField { .. },
                ),
            ) => CursorIcon::Pointer,
            _ => CursorIcon::Default,
        }
    }

    fn on_press(
        &mut self,
        ctx: &mut AppContext<'_>,
        layout: &Layout,
        hover: Option<Hit>,
        at: (f32, f32),
        modifiers: Modifiers,
        now_i: Instant,
    ) {
        let double = self
            .pointer
            .last_click
            .is_some_and(|(when, hit)| hit == hover && when.elapsed() < DOUBLE_CLICK);
        self.pointer.last_click = Some((now_i, hover));
        self.pointer.pressed = hover;
        let Some(hit) = hover else {
            return;
        };
        if self.press_panel(hit, layout, at, modifiers, now_i, double) {
            return;
        }
        let now = Self::now(ctx);
        self.press_transport(ctx, hit, now);
    }

    /// Panel and canvas presses. Returns `true` when the press was handled.
    fn press_panel(
        &mut self,
        hit: Hit,
        layout: &Layout,
        at: (f32, f32),
        modifiers: Modifiers,
        now_i: Instant,
        double: bool,
    ) -> bool {
        match hit {
            Hit::Scrub if self.session.document().is_some() => {
                self.transport.begin_scrub();
                self.scrub_to(layout, at.0);
                self.pointer.drag = Some(Drag::Scrub);
            }
            Hit::Splitter(which) => {
                self.pointer.drag = Some(Drag::Splitter {
                    which,
                    origin: at,
                    start: self.sizes,
                });
            }
            Hit::LayerRow(i) => {
                self.select_layer(i);
                if double {
                    let text = self.current_name();
                    self.begin_edit(FieldTarget::Name, text, true);
                }
            }
            Hit::LayerEye(i) => {
                if let Some(node) = self.outline.get(i).map(|row| row.node) {
                    self.toggle_visible(node);
                }
            }
            Hit::LayerLock(i) => {
                if let Some(node) = self.outline.get(i).map(|row| row.node) {
                    self.toggle_lock(node);
                }
            }
            Hit::LayerChevron(i) => self.toggle_expanded(i, modifiers.alt),
            Hit::Track(i) => self.select_layer(i),
            Hit::Toggle(TopBarToggle::SafeArea) => self.safe_area = !self.safe_area,
            Hit::SnapToggle => self.snap = !self.snap,
            Hit::Canvas => {
                if let Some(point) = to_composition(&self.session, layout, at.0, at.1) {
                    self.pending_pick = Some(point);
                }
            }
            Hit::Gizmo(handle) => self.begin_gizmo(layout, handle, at),
            Hit::InspectCopy => {
                if let Some(header) = self.inspector.header.as_ref() {
                    crate::clipboard::copy(&header.identity);
                    self.copied_at = Some(now_i);
                }
            }
            Hit::InspectStopwatch(item) => {
                if let Some((node, property, axis)) = self.inspector_channel(item) {
                    self.toggle_stopwatch(node, property, axis);
                }
            }
            Hit::InspectDiamond(item) => {
                if let Some((node, property, axis)) = self.inspector_channel(item) {
                    self.toggle_key(node, property, axis);
                }
            }
            Hit::InspectSegment { item, segment } => {
                self.press_align_segment(item, segment, now_i);
            }
            Hit::InspectField { item, field } => self.on_inspector_field_press(item, field, at.0),
            Hit::TimelineRow(row) => {
                self.on_timeline_row_press(row, modifiers);
                if at.0 >= layout.timeline.tracks.x {
                    self.pointer.drag = Some(Drag::Marquee {
                        start: at,
                        current: at,
                    });
                }
            }
            Hit::TimelineStopwatch(row) => {
                if let Some((node, property, axis)) = self.timeline_channel(row) {
                    self.toggle_stopwatch(node, property, axis);
                }
            }
            Hit::TimelinePrevKey(row) => {
                if let Some((node, property, axis)) = self.timeline_channel(row) {
                    self.jump_key(node, property, axis, false);
                }
            }
            Hit::TimelineNextKey(row) => {
                if let Some((node, property, axis)) = self.timeline_channel(row) {
                    self.jump_key(node, property, axis, true);
                }
            }
            Hit::TimelineKeyToggle(row) => {
                if let Some((node, property, axis)) = self.timeline_channel(row) {
                    self.toggle_key(node, property, axis);
                }
            }
            Hit::TimelineField(row) => self.on_timeline_field_press(row, at),
            Hit::TimelineDiamond { row, frame } => {
                self.on_diamond_press(row, frame, at.0, modifiers);
            }
            Hit::OrphanDelete(index) => self.delete_orphan(index, now_i),
            Hit::OrphanRow(_) | Hit::Inspector | Hit::Scrub | Hit::Transport(_) => return false,
        }
        true
    }

    fn press_align_segment(&mut self, item: usize, segment: usize, now_i: Instant) {
        if self.inspector_field_target_at(item, 0) != Some(FieldTarget::Align) {
            return;
        }
        let Some(id) = self.selected.map(|node| self.node_identity(node)) else {
            return;
        };
        let name = match segment {
            0 => "left",
            1 => "center",
            _ => "right",
        };
        let command = EditCommand::SetValue {
            id,
            property: "align".to_owned(),
            value: OverrideValue::Text(name.to_owned()),
        };
        self.apply_command(&command, now_i);
    }

    fn press_transport(&mut self, ctx: &mut AppContext<'_>, hit: Hit, now: Duration) {
        let Hit::Transport(button) = hit else {
            return;
        };
        let action = match button {
            TransportButton::ToStart => crate::Action::GoToStart,
            TransportButton::StepBack => crate::Action::Step(-1),
            TransportButton::PlayPause => crate::Action::TogglePlay,
            TransportButton::StepForward => crate::Action::Step(1),
            TransportButton::ToEnd => crate::Action::GoToEnd,
            TransportButton::Loop => crate::Action::ToggleLoop,
        };
        if self.session.document().is_some() {
            let effect = apply(action, &mut self.transport, now);
            self.handle_effect(effect, ctx, now);
        }
    }
    fn begin_gizmo(&mut self, layout: &Layout, handle: Handle, at: (f32, f32)) {
        let (Some(bounds), Some(point)) = (
            self.last_bounds,
            to_composition(&self.session, layout, at.0, at.1),
        ) else {
            return;
        };
        let Some(document) = self.session.document() else {
            return;
        };
        let Some(selected) = self.selected else {
            return;
        };
        let Some(node) = document.node(selected) else {
            return;
        };
        if self.node_locked(selected) {
            return;
        }
        let t = node.transform();
        let frame = self.frame();
        let start_position = motioner_eval::sample_vec2(&t.position, frame);
        let start_scale = motioner_eval::sample_vec2(&t.scale, frame);
        let start_rotation = motioner_eval::sample(&t.rotation, frame);
        let center = Vec2::new(
            (bounds.min_x + bounds.max_x) * 0.5,
            (bounds.min_y + bounds.max_y) * 0.5,
        );
        let other_bounds = self.other_bounds(selected);
        self.undo.begin_gesture();
        self.pointer.drag = Some(Drag::Gizmo {
            handle,
            start_set: self.session.overrides().clone(),
            start_pointer: point,
            start_bounds: bounds,
            start_position,
            start_scale,
            start_rotation,
            center,
            start_angle: gizmo::angle_of(center, point),
            other_bounds,
        });
    }

    /// Bounds of the other layers, for snapping (shape bounds only; text
    /// layers need the renderer and are skipped).
    fn other_bounds(&self, selected: NodeId) -> Vec<Bounds> {
        let Some(document) = self.session.document() else {
            return Vec::new();
        };
        let plan = self.session.plan();
        self.outline
            .iter()
            .filter(|row| row.node != selected)
            .filter_map(|row| {
                bounds::node_bounds(
                    document,
                    plan,
                    row.node,
                    &|_: &str, _: f64| -> Option<TextExtent> { None },
                )
            })
            .collect()
    }

    fn on_inspector_field_press(&mut self, item: usize, field: usize, x: f32) {
        let Some(target) = self.inspector_field_target_at(item, field) else {
            return;
        };
        if target == FieldTarget::Ease {
            self.cycle_ease(Instant::now());
            return;
        }
        let numeric = matches!(
            self.inspector_row_value(item),
            Some(inspect::RowValue::Pair { .. } | inspect::RowValue::Single(_))
        );
        if numeric {
            if let Some(value) = self.numeric_value(target) {
                self.undo.begin_gesture();
                self.pointer.drag = Some(Drag::Field {
                    target,
                    start_set: self.session.overrides().clone(),
                    start_value: value,
                    start_x: x,
                    moved: false,
                });
                return;
            }
        }
        let text = self.field_text(target);
        self.begin_edit(target, text, true);
    }

    fn on_timeline_field_press(&mut self, row: usize, at: (f32, f32)) {
        let Some((node, property, axis)) = self.timeline_channel(row) else {
            return;
        };
        let Some(OverrideValue::Number(number)) = self.sampled(node, property, axis) else {
            return;
        };
        let field_axis = match axis {
            Some(Axis::X) => inspect::Axis::X,
            Some(Axis::Y) => inspect::Axis::Y,
            None => inspect::Axis::Single,
        };
        self.undo.begin_gesture();
        self.pointer.drag = Some(Drag::Field {
            target: FieldTarget::Transform(property, field_axis),
            start_set: self.session.overrides().clone(),
            start_value: number,
            start_x: at.0,
            moved: false,
        });
    }

    fn inspector_row_value(&self, item: usize) -> Option<&inspect::RowValue> {
        let inspect::Item::Row { index, .. } = self.inspector_layout.items.get(item)? else {
            return None;
        };
        self.inspector.rows.get(*index).map(|row| &row.value)
    }

    fn inspector_field_target_at(&self, item: usize, field: usize) -> Option<FieldTarget> {
        let inspect::Item::Row { fields, .. } = self.inspector_layout.items.get(item)? else {
            return None;
        };
        fields.get(field).map(|(_, target)| *target)
    }

    fn inspector_channel(&self, item: usize) -> Option<(NodeId, Property, Option<Axis>)> {
        let inspect::Item::Row { index, .. } = self.inspector_layout.items.get(item)? else {
            return None;
        };
        let row = self.inspector.rows.get(*index)?;
        let channel = row.channel.as_ref()?;
        Some((self.selected?, channel.property, channel.axis))
    }

    fn timeline_channel(&self, row: usize) -> Option<(NodeId, Property, Option<Axis>)> {
        let row = self.timeline.rows().get(row)?;
        let RowKind::Transform {
            outline,
            property,
            axis,
        } = row.kind
        else {
            return None;
        };
        let node = self.outline.get(outline)?.node;
        Some((node, property, axis))
    }

    fn on_timeline_row_press(&mut self, row: usize, modifiers: Modifiers) {
        let Some(entry) = self.timeline.rows().get(row) else {
            return;
        };
        let outline = entry.kind.outline();
        if entry.kind.is_layer() {
            self.select_layer(outline);
            if modifiers.alt {
                self.toggle_expanded(outline, true);
            }
        } else {
            self.selected = self.outline.get(outline).map(|r| r.node);
            self.selected_keys.clear();
            self.rebuild_inspector();
        }
    }

    fn diamond_at(&self, row: usize, mx: f32, layout: &Layout) -> Option<Frame> {
        let entry = self.timeline.rows().get(row)?;
        let frames: Vec<Frame> = if entry.kind.is_layer() {
            entry.summary.iter().map(|k| k.frame).collect()
        } else {
            entry.keys.clone()
        };
        let duration = self.transport.duration().max(1);
        let scrub = layout.timeline.scrub;
        let reach = layout.metrics.px(6.0);
        frames.into_iter().find(|frame| {
            let x = crate::timeline::frame_to_x(
                *frame,
                duration,
                f64::from(scrub.x),
                f64::from(scrub.w),
            ) as f32;
            (x - mx).abs() <= reach
        })
    }

    fn on_diamond_press(&mut self, row: usize, frame: Frame, mx: f32, modifiers: Modifiers) {
        let Some(entry) = self.timeline.rows().get(row).cloned() else {
            return;
        };
        let mut keys: Vec<KeyRef> = Vec::new();
        match entry.kind {
            RowKind::Layer { outline } => {
                let Some(node) = self.outline.get(outline).map(|r| r.node) else {
                    return;
                };
                let id = self.node_identity(node);
                for (property, _) in Property::ALL {
                    let axes: Vec<Option<Axis>> = if property.is_vector() {
                        vec![Some(Axis::X), Some(Axis::Y)]
                    } else {
                        vec![None]
                    };
                    for axis in axes {
                        if self.channel_keys(node, property, axis).contains(&frame) {
                            keys.push(KeyRef {
                                id: id.clone(),
                                property: channel_property_name(property, axis),
                                frame,
                            });
                        }
                    }
                }
            }
            RowKind::Transform {
                outline,
                property,
                axis,
            } => {
                let Some(node) = self.outline.get(outline).map(|r| r.node) else {
                    return;
                };
                keys.push(KeyRef {
                    id: self.node_identity(node),
                    property: channel_property_name(property, axis),
                    frame,
                });
            }
            RowKind::Content { .. } => return,
        }
        if modifiers.shift {
            for key in keys {
                if !self.selected_keys.contains(&key) {
                    self.selected_keys.push(key);
                }
            }
        } else {
            self.selected_keys = keys;
        }
        self.rebuild_inspector();
        let moves: Vec<MoveKey> = self
            .selected_keys
            .iter()
            .filter_map(|key| {
                let (value, ease) = self.key_value(key)?;
                Some(MoveKey {
                    key: key.clone(),
                    to: key.frame,
                    value,
                    ease,
                })
            })
            .collect();
        if !moves.is_empty() {
            self.undo.begin_gesture();
            self.pointer.drag = Some(Drag::Keys {
                start_set: self.session.overrides().clone(),
                keys: moves,
                start_x: mx,
                delta: 0,
            });
        }
    }

    fn delete_orphan(&mut self, index: usize, now: Instant) {
        let orphans = self.orphans();
        let Some(orphan) = orphans.get(index) else {
            return;
        };
        let before = self.session.overrides().clone();
        let mut set = before.clone();
        set.remove_entry(&orphan.entry);
        self.undo.record(before);
        self.commit_set(&set, now);
    }

    fn on_drag(&mut self, layout: &Layout, at: (f32, f32), modifiers: Modifiers, now_i: Instant) {
        let Some(mut drag) = self.pointer.drag.take() else {
            return;
        };
        match &mut drag {
            Drag::Scrub => self.scrub_to(layout, at.0),
            Drag::Splitter {
                which,
                origin,
                start,
            } => {
                let delta = (
                    (at.0 - origin.0) / layout.metrics.scale,
                    (at.1 - origin.1) / layout.metrics.scale,
                );
                self.sizes = resize_panels(*start, *which, delta);
            }
            Drag::Field {
                target,
                start_set,
                start_value,
                start_x,
                moved,
            } => {
                let dx = at.0 - *start_x;
                if dx.abs() > CLICK_SLOP {
                    *moved = true;
                }
                if *moved {
                    let target = *target;
                    let base = start_set.clone();
                    let step = scrub_step(target, modifiers);
                    let value = *start_value + f64::from(dx) * step;
                    if let Some(command) = self.command_for_number(target, value, &base) {
                        self.apply_gesture(&base, &command, now_i);
                    }
                }
            }
            Drag::Gizmo { .. } => self.drag_gizmo(&mut drag, layout, at, modifiers, now_i),
            Drag::Keys { .. } => self.drag_keys(&mut drag, layout, at, now_i),
            Drag::Marquee { current, .. } => *current = at,
        }
        self.pointer.drag = Some(drag);
    }

    /// Applies a gizmo drag from its captured start state.
    fn drag_gizmo(
        &mut self,
        drag: &mut Drag,
        layout: &Layout,
        at: (f32, f32),
        modifiers: Modifiers,
        now_i: Instant,
    ) {
        let Drag::Gizmo {
            handle,
            start_set,
            start_pointer,
            start_bounds,
            start_position,
            start_scale,
            start_rotation,
            center,
            start_angle,
            other_bounds,
        } = drag
        else {
            return;
        };
        let Some(point) = to_composition(&self.session, layout, at.0, at.1) else {
            return;
        };
        let base = start_set.clone();
        let command = match *handle {
            Handle::Move => {
                let delta = Vec2::new(point.x - start_pointer.x, point.y - start_pointer.y);
                let (delta, guides) = if self.snap {
                    let (width, height) = self.composition_size();
                    gizmo::snap_delta(
                        *start_bounds,
                        delta,
                        (width, height),
                        other_bounds,
                        SNAP_PX / self.zoom(layout).max(1e-9),
                    )
                } else {
                    (delta, Vec::new())
                };
                self.guides = guides;
                let position = gizmo::move_position(*start_position, delta, modifiers.shift);
                self.number_command(Property::Position, position, &base)
            }
            Handle::Rotate => {
                let angle = gizmo::angle_of(*center, point);
                let delta = gizmo::rotation_delta(*start_angle, angle, modifiers.shift);
                self.number_command(
                    Property::Rotation,
                    Vec2::new(*start_rotation + delta, 0.0),
                    &base,
                )
            }
            _ => {
                let corner = match *handle {
                    Handle::TopLeft => gizmo::Corner::TopLeft,
                    Handle::TopRight => gizmo::Corner::TopRight,
                    Handle::BottomRight => gizmo::Corner::BottomRight,
                    _ => gizmo::Corner::BottomLeft,
                };
                let anchor = if modifiers.alt {
                    *center
                } else {
                    gizmo::scale_anchor(*start_bounds, corner, None)
                };
                let factors =
                    gizmo::scale_factors(*start_bounds, corner, point, anchor, modifiers.shift);
                let scale = Vec2::new(
                    (start_scale.x * factors.x).clamp(-1000.0, 1000.0),
                    (start_scale.y * factors.y).clamp(-1000.0, 1000.0),
                );
                self.guides.clear();
                self.number_command(Property::Scale, scale, &base)
            }
        };
        if let Some(command) = command {
            self.apply_gesture(&base, &command, now_i);
        }
    }

    /// Retimes the dragged keys, snapping to whole frames.
    fn drag_keys(&mut self, drag: &mut Drag, layout: &Layout, at: (f32, f32), now_i: Instant) {
        let Drag::Keys {
            start_set,
            keys,
            start_x,
            delta,
        } = drag
        else {
            return;
        };
        let duration = self.transport.duration().max(1);
        let scrub = layout.timeline.scrub;
        let frames = x_to_frame(
            f64::from(at.0),
            duration,
            f64::from(scrub.x),
            f64::from(scrub.w),
        ) - x_to_frame(
            f64::from(*start_x),
            duration,
            f64::from(scrub.x),
            f64::from(scrub.w),
        );
        if frames == *delta {
            return;
        }
        *delta = frames;
        let moves: Vec<MoveKey> = keys
            .iter()
            .map(|key| {
                let mut moved = key.clone();
                moved.to = key.key.frame + frames;
                moved
            })
            .collect();
        let base = start_set.clone();
        self.apply_gesture(&base, &EditCommand::MoveKeys(moves), now_i);
    }
    fn composition_size(&self) -> (f64, f64) {
        self.session.composition().map_or((1920.0, 1080.0), |c| {
            (f64::from(c.width()), f64::from(c.height()))
        })
    }

    fn number_command(
        &self,
        property: Property,
        value: Vec2,
        base: &OverrideSet,
    ) -> Option<EditCommand> {
        let selected = self.selected?;
        let id = self.node_identity(selected);
        let keyed = !self.channel_keys(selected, property, None).is_empty()
            || base
                .entries()
                .iter()
                .any(|e| e.id == id && e.property == property.name());
        let value = match property {
            Property::Rotation | Property::Opacity => OverrideValue::Number(value.x),
            _ => OverrideValue::Vec2(value),
        };
        if keyed {
            Some(EditCommand::SetKey {
                id,
                property: property.name().to_owned(),
                axis: None,
                frame: self.frame(),
                value,
            })
        } else {
            Some(EditCommand::SetValue {
                id,
                property: property.name().to_owned(),
                value,
            })
        }
    }

    fn on_release(&mut self, layout: &Layout, hover: Option<Hit>) {
        if let Some(drag) = self.pointer.drag.take() {
            match drag {
                Drag::Field {
                    target,
                    moved: false,
                    ..
                } => {
                    let text = self.field_text(target);
                    self.begin_edit(target, text, true);
                }
                Drag::Marquee { start, current } => {
                    self.select_in_marquee(layout, start, current);
                    if start == current {
                        if let Some(Hit::TimelineRow(row)) = hover {
                            self.on_timeline_row_press(row, Modifiers::default());
                        }
                    }
                }
                Drag::Keys { delta, keys, .. } if delta != 0 => {
                    // The selection follows the retimed keys, so the Ease
                    // control and the markers refer to the new frames.
                    let moved: Vec<KeyRef> = keys
                        .iter()
                        .map(|key| KeyRef {
                            id: key.key.id.clone(),
                            property: key.key.property.clone(),
                            frame: key.key.frame + delta,
                        })
                        .collect();
                    self.selected_keys = moved;
                    self.rebuild_inspector();
                }
                _ => {}
            }
        }
        self.transport.end_scrub();
        self.undo.end_gesture();
        self.pointer.pressed = None;
        self.guides.clear();
        self.marquee = None;
        self.session.flush_overrides(Instant::now());
    }

    fn select_in_marquee(&mut self, layout: &Layout, start: (f32, f32), current: (f32, f32)) {
        let rect = Rect::new(
            start.0.min(current.0),
            start.1.min(current.1),
            (current.0 - start.0).abs(),
            (current.1 - start.1).abs(),
        );
        if rect.w < 2.0 && rect.h < 2.0 {
            return;
        }
        let duration = self.transport.duration().max(1);
        let scrub = layout.timeline.scrub;
        let mut selected = Vec::new();
        for row in &self.timeline_layout.rows {
            let Some(entry) = self.timeline.rows().get(row.row) else {
                continue;
            };
            let RowKind::Transform {
                outline,
                property,
                axis,
            } = entry.kind
            else {
                continue;
            };
            let Some(node) = self.outline.get(outline).map(|r| r.node) else {
                continue;
            };
            for frame in &entry.keys {
                let x = crate::timeline::frame_to_x(
                    *frame,
                    duration,
                    f64::from(scrub.x),
                    f64::from(scrub.w),
                ) as f32;
                let diamond = Rect::new(x - 5.0, row.rect.center_y() - 5.0, 10.0, 10.0);
                if diamond.overlaps(rect) {
                    selected.push(KeyRef {
                        id: self.node_identity(node),
                        property: channel_property_name(property, axis),
                        frame: *frame,
                    });
                }
            }
        }
        self.selected_keys = selected;
        self.rebuild_inspector();
    }

    fn on_scroll(&mut self, layout: &Layout, hover: Option<Hit>, scroll: f32, shift: bool) {
        match hover {
            Some(Hit::Scrub) => {
                let direction = if scroll > 0.0 { -1 } else { 1 };
                self.transport.step(direction * if shift { 10 } else { 1 });
            }
            Some(
                Hit::LayerRow(_) | Hit::LayerEye(_) | Hit::LayerLock(_) | Hit::LayerChevron(_),
            ) => {
                let max = scroll_max(
                    self.outline.len(),
                    layout.row_h,
                    layout.layers.body.h,
                    layout.metrics.px(size::GRID * 2.0),
                );
                self.pointer.layer_scroll =
                    (self.pointer.layer_scroll - scroll * layout.row_h).clamp(0.0, max);
            }
            Some(
                Hit::Track(_)
                | Hit::TimelineRow(_)
                | Hit::TimelineDiamond { .. }
                | Hit::TimelineStopwatch(_)
                | Hit::TimelinePrevKey(_)
                | Hit::TimelineKeyToggle(_)
                | Hit::TimelineNextKey(_)
                | Hit::TimelineField(_),
            ) => {
                let max = (self.timeline_layout.height - layout.timeline.tracks.h).max(0.0);
                self.pointer.track_scroll =
                    (self.pointer.track_scroll - scroll * layout.row_h).clamp(0.0, max);
            }
            Some(Hit::Inspector | Hit::InspectField { .. } | Hit::InspectStopwatch(_)) => {
                let max = (self.inspector_layout.height - layout.inspector.body.h).max(0.0);
                self.pointer.inspector_scroll =
                    (self.pointer.inspector_scroll - scroll * layout.row_h).clamp(0.0, max);
            }
            _ => {}
        }
    }

    // ---- keyboard --------------------------------------------------------

    fn handle_keys(&mut self, ctx: &mut AppContext<'_>, now: Duration) {
        if self.editing.is_some() {
            self.handle_edit_keys(ctx);
            return;
        }
        let modifiers = modifiers(ctx);
        for key in SHORTCUT_KEYS {
            if ctx.input.just_pressed(key) {
                if let Some(action) = action_for(key, modifiers) {
                    let effect = apply(action, &mut self.transport, now);
                    if self.handle_effect(effect, ctx, now) {
                        break;
                    }
                }
            }
        }
    }

    fn handle_edit_keys(&mut self, ctx: &mut AppContext<'_>) {
        let shift = modifiers(ctx).shift;
        let ctrl = modifiers(ctx).ctrl;
        let key = if ctx.input.just_pressed(KeyCode::Backspace) {
            Some(EditKey::Backspace)
        } else if ctx.input.just_pressed(KeyCode::Delete) {
            Some(EditKey::Delete)
        } else if ctx.input.just_pressed(KeyCode::ArrowLeft) {
            Some(EditKey::Left)
        } else if ctx.input.just_pressed(KeyCode::ArrowRight) {
            Some(EditKey::Right)
        } else if ctx.input.just_pressed(KeyCode::Home) {
            Some(EditKey::Home)
        } else if ctx.input.just_pressed(KeyCode::End) {
            Some(EditKey::End)
        } else if ctx.input.just_pressed(KeyCode::Enter) {
            Some(EditKey::Commit)
        } else if ctx.input.just_pressed(KeyCode::Escape) {
            Some(EditKey::Cancel)
        } else if ctrl && ctx.input.just_pressed(KeyCode::KeyA) {
            Some(EditKey::SelectAll)
        } else {
            None
        };
        match key {
            Some(EditKey::Commit) => {
                self.commit_edit(Instant::now());
            }
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
        if self.editing.is_some() {
            for ch in ctx.input.typed_chars() {
                if !ch.is_control() {
                    if let Some(editing) = self.editing.as_mut() {
                        editing.editor.insert_char(*ch);
                        editing.invalid = false;
                        self.field_error = None;
                    }
                }
            }
        }
    }

    // ---- frame -----------------------------------------------------------

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
        let mut layout = Layout::compute(
            ctx.window_size.0 as f32,
            ctx.window_size.1 as f32,
            scale,
            self.sizes,
        );
        self.layout = Some(layout);
        self.layout_inspector();
        self.layout_timeline();

        self.handle_keys(ctx, now);
        if self.watch.check(Instant::now()) || self.session.poll_changes(Instant::now()) {
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
        if self.model_generation != self.session.generation() {
            self.refresh_models();
        }

        self.handle_pointer(ctx, &layout);
        if self.selected != self.revealed {
            self.reveal_selection(&layout);
        }
        self.session.flush_overrides(Instant::now());
        layout = Layout::compute(
            ctx.window_size.0 as f32,
            ctx.window_size.1 as f32,
            scale,
            self.sizes,
        );
        self.layout = Some(layout);
        self.layout_inspector();
        self.layout_timeline();
        self.marquee = match self.pointer.drag {
            Some(Drag::Marquee { start, current }) => Some(Rect::new(
                start.0.min(current.0),
                start.1.min(current.1),
                (current.0 - start.0).abs(),
                (current.1 - start.1).abs(),
            )),
            _ => None,
        };
    }

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
        if let Some(index) = self.timeline.layer_row(row) {
            let y = self
                .timeline_layout
                .rows
                .iter()
                .find(|r| r.row == index)
                .map_or(0.0, |r| {
                    r.rect.y + self.pointer.track_scroll - layout.timeline.tracks.y
                });
            self.pointer.track_scroll = scroll_into_view(
                self.pointer.track_scroll,
                y,
                layout.row_h,
                layout.timeline.tracks.h,
            );
        }
        self.rebuild_inspector();
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
            Hit::SnapToggle => ("Snap to guides and layers", None, layout.snap_toggle),
            Hit::LayerEye(i) => (
                "Toggle visibility",
                None,
                layout.layer_eye(layout.layer_row(i, self.pointer.layer_scroll)),
            ),
            Hit::LayerLock(i) => (
                "Toggle lock",
                None,
                layout.layer_lock(layout.layer_row(i, self.pointer.layer_scroll)),
            ),
            Hit::InspectStopwatch(item) => (
                "Animate this property",
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
            .get(row)
            .and_then(|r| match which {
                0 => r.stopwatch,
                1 => r.key,
                _ => r.next,
            })
            .unwrap_or_default()
    }

    fn zoom(&self, layout: &Layout) -> f64 {
        match (comp_rect(&self.session, layout), self.session.composition()) {
            (Some(rect), Some(comp)) => f64::from(rect.w) / f64::from(comp.width()),
            _ => 1.0,
        }
    }

    fn orphans(&self) -> Vec<Orphan> {
        let Some(document) = self.session.document() else {
            return Vec::new();
        };
        self.session
            .overrides()
            .entries()
            .iter()
            .filter(|entry| document.find(&entry.id).is_none())
            .map(|entry| Orphan {
                entry: entry.clone(),
            })
            .collect()
    }

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
        let (out, picked) = self.canvas_overlays(renderer, layout, frame, comp_rect);
        if self.pending_pick.is_some() || picked.is_some() {
            self.apply_pick(picked);
        }
        self.last_bounds = out.bounds;
        out
    }

    /// Paints the composition and computes the selection/hover bounds, and
    /// resolves a pending canvas pick (which needs text measurement).
    fn canvas_overlays(
        &mut self,
        renderer: &mut Renderer,
        layout: &Layout,
        frame: i64,
        comp_rect: Option<Rect>,
    ) -> (CanvasBounds, Option<NodeId>) {
        let mut out = CanvasBounds::default();
        let pick = self.pending_pick.take();
        let overrides = self.session.overrides().clone();
        let hovered_node = self.hovered_node();
        let stale = self.session.plan_frame() != Some(frame);
        let started = Instant::now();
        let Some(rect) = comp_rect else {
            return (out, None);
        };
        let window = (layout.window.w, layout.window.h);
        let (Some((document, plan)), true) = (self.session.evaluate(frame), true) else {
            return (out, None);
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
        let renderer_ref = &*renderer;
        let measure = |text: &str, size: f64| -> Option<TextExtent> {
            painter?.text_extent(renderer_ref, text, size)
        };
        if let Some(b) = self
            .selected
            .and_then(|node| bounds::node_bounds(document, plan, node, &measure))
        {
            out.selection = Some(to_screen(rect, zoom, b));
            out.size = Some((b.max_x - b.min_x, b.max_y - b.min_y));
            out.bounds = Some(b);
        }
        out.hover = hovered_node
            .and_then(|node| bounds::node_bounds(document, plan, node, &measure))
            .map(|b| to_screen(rect, zoom, b));
        let picked = pick.and_then(|point| {
            let locked = |node: NodeId| {
                document.node(node).is_some_and(|n| {
                    overrides
                        .get(n.identity().as_str(), motioner_core::LOCKED, None)
                        .is_some_and(|entry| entry.action == OverrideAction::Locked(true))
                })
            };
            bounds::hit_node_where(document, plan, point, &measure, |node| !locked(node))
        });
        (out, picked)
    }

    fn hovered_node(&self) -> Option<NodeId> {
        let (Hit::LayerRow(index)
        | Hit::LayerEye(index)
        | Hit::LayerLock(index)
        | Hit::Track(index)
        | Hit::TimelineRow(index)
        | Hit::TimelineDiamond { row: index, .. }) = self.pointer.hover?
        else {
            return None;
        };
        if let Some(row) = self.timeline.rows().get(index) {
            return self.outline.get(row.kind.outline()).map(|row| row.node);
        }
        self.outline.get(index).map(|row| row.node)
    }

    /// Applies the result of a canvas pick: a node, or nothing under the
    /// pointer (which clears the selection).
    fn apply_pick(&mut self, picked: Option<NodeId>) {
        let changed = self.selected != picked;
        if let Some(node) = picked {
            self.selected = Some(node);
            self.selected_keys.clear();
            if let Some(row) = self.selected_row() {
                self.select_layer(row);
            }
        } else if self.selected.is_some() {
            self.selected = None;
            self.selected_keys.clear();
        }
        if changed {
            self.refresh_models();
        }
    }
    fn frame_facts(&self, layout: &Layout) -> FrameFacts {
        let comp_rect = comp_rect(&self.session, layout);
        let zoom = match (comp_rect, self.session.composition()) {
            (Some(rect), Some(comp)) => f64::from(rect.w) / f64::from(comp.width()),
            _ => 0.0,
        };
        let reload_pulse = self.last_reload.map_or(0.0, |at| {
            1.0 - (at.elapsed().as_secs_f32() / RELOAD_PULSE.as_secs_f32()).min(1.0)
        });
        let orphans: Vec<OrphanRow> = self
            .orphans()
            .iter()
            .map(|orphan| OrphanRow {
                id: orphan.entry.id.clone(),
                property: orphan.entry.property.clone(),
            })
            .collect();
        let layer_states: Vec<(bool, bool)> = self
            .outline
            .iter()
            .map(|row| {
                let visible = self
                    .session
                    .document()
                    .and_then(|document| document.node(row.node))
                    .is_some_and(motioner_core::Node::is_visible);
                (visible, self.node_locked(row.node))
            })
            .collect();
        let copied = self
            .copied_at
            .is_some_and(|at| at.elapsed() < Duration::from_millis(1200));
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

        let frame = self.transport.frame();
        let canvas = self.paint_canvas(ctx.render.renderer_mut(), &layout, frame);
        let file_name = self
            .session
            .path()
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("untitled");
        let facts = self.frame_facts(&layout);
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
            warnings: self.session.warnings(),
            comp_rect: facts.comp_rect,
            zoom: facts.zoom,
            selection_bounds: canvas.selection,
            selection_size: canvas.size,
            hover_bounds: canvas.hover,
            safe_area: self.safe_area,
            snap: self.snap,
            guides: &self.guides,
            marquee: self.marquee,
            stats: Stats {
                ui_fps,
                eval_ms: self.eval_ms,
                dropped: self.dropped,
            },
            layer_scroll: self.pointer.layer_scroll,
            inspector_scroll: self.pointer.inspector_scroll,
            reload_pulse: facts.reload_pulse,
            inspector: &self.inspector,
            inspector_layout: &self.inspector_layout,
            editing: self.editing.as_ref(),
            inspector_field_error: self.field_error,
            timeline: &self.timeline,
            timeline_layout: &self.timeline_layout,
            selected_keys: &self.selected_keys,
            expanded: &self.expanded,
            orphans: &facts.orphans,
            layer_states: &facts.layer_states,
            copied: facts.copied,
            retime_delta: match self.pointer.drag {
                Some(Drag::Keys { delta, .. }) if delta != 0 => Some(delta),
                _ => None,
            },
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

/// An override whose node no longer exists.
struct Orphan {
    entry: OverrideEntry,
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

/// On-screen bounds produced while painting the canvas.
#[derive(Debug, Clone, Copy, Default)]
struct CanvasBounds {
    selection: Option<Rect>,
    size: Option<(f64, f64)>,
    hover: Option<Rect>,
    bounds: Option<Bounds>,
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

/// The override/KeyRef property name of a channel.
fn channel_property_name(property: Property, axis: Option<Axis>) -> String {
    match axis {
        Some(axis) => format!("{}.{}", property.name(), axis.name()),
        None => property.name().to_owned(),
    }
}

/// The ease of a whole-vector key: both axes are keyed together by
/// `key("pos", ...)`, so the X key is representative.
fn find_pair_ease(value: &motioner_core::AnimatableVec2, frame: Frame) -> Option<Ease> {
    value
        .axis(Axis::X)
        .keys()
        .iter()
        .chain(value.axis(Axis::Y).keys().iter())
        .find(|key| key.frame == frame)
        .map(|key| key.ease)
}

/// A whole-vector key as an override value.
fn pair_value(
    value: &motioner_core::AnimatableVec2,
    frame: Frame,
) -> Option<(OverrideValue, Ease)> {
    let x = value
        .axis(Axis::X)
        .keys()
        .iter()
        .find(|key| key.frame == frame)?;
    let y = value
        .axis(Axis::Y)
        .keys()
        .iter()
        .find(|key| key.frame == frame)?;
    Some((OverrideValue::Vec2(Vec2::new(x.value, y.value)), x.ease))
}
/// The numeric value a content field holds, when it is a number.
fn content_number(content: &NodeContent, field: ContentField) -> Option<f64> {
    match (content, field) {
        (NodeContent::Rect { width, .. }, ContentField::Width) => Some(*width),
        (NodeContent::Rect { height, .. }, ContentField::Height) => Some(*height),
        (
            NodeContent::Rect { radius, .. } | NodeContent::Circle { radius, .. },
            ContentField::Radius,
        ) => Some(*radius),
        (
            NodeContent::Rect { paint, .. } | NodeContent::Circle { paint, .. },
            ContentField::StrokeWidth,
        ) => Some(paint.stroke.map_or(0.0, |stroke| stroke.width)),
        (NodeContent::Text(text), ContentField::Size) => Some(text.size),
        _ => None,
    }
}

/// Whether a field target holds a number (and can be parsed as one).
fn is_numeric_target(target: FieldTarget) -> bool {
    match target {
        FieldTarget::Transform(..) => true,
        FieldTarget::Content(field) => matches!(
            field,
            ContentField::Width
                | ContentField::Height
                | ContentField::Radius
                | ContentField::StrokeWidth
                | ContentField::Size
        ),
        _ => false,
    }
}

/// How much one pixel of a scrub moves a numeric field.
fn scrub_step(target: FieldTarget, modifiers: Modifiers) -> f64 {
    let base = match target {
        FieldTarget::Transform(Property::Scale | Property::Opacity, _) => 0.01,
        _ => 1.0,
    };
    if modifiers.shift {
        base * 10.0
    } else if modifiers.alt {
        base * 0.1
    } else {
        base
    }
}

/// How far a list of `rows` rows can scroll inside `visible` pixels.
fn scroll_max(rows: usize, row_h: f32, visible: f32, padding: f32) -> f32 {
    (rows as f32 * row_h + padding - visible).max(0.0)
}

/// Window pixels to composition pixels, through the letterboxed rect.
fn to_composition(session: &PreviewSession, layout: &Layout, x: f32, y: f32) -> Option<Vec2> {
    let rect = comp_rect(session, layout)?;
    let zoom = match session.composition() {
        Some(comp) => f64::from(rect.w) / f64::from(comp.width()),
        None => return None,
    };
    Some(Vec2::new(
        (f64::from(x) - f64::from(rect.x)) / zoom,
        (f64::from(y) - f64::from(rect.y)) / zoom,
    ))
}

/// A composition-space box converted to window pixels through the canvas
/// rect and zoom.
fn to_screen(rect: Rect, zoom: f64, b: Bounds) -> Rect {
    let x0 = (f64::from(rect.x) + b.min_x * zoom).floor() as f32;
    let y0 = (f64::from(rect.y) + b.min_y * zoom).floor() as f32;
    let x1 = (f64::from(rect.x) + b.max_x * zoom).ceil() as f32;
    let y1 = (f64::from(rect.y) + b.max_y * zoom).ceil() as f32;
    Rect::new(x0, y0, x1 - x0, y1 - y0)
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

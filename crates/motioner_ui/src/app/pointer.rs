//! Pointer input: hit testing against the pure layouts, and presses, drags,
//! releases and wheel turns turned into [`Command`]s.
//!
//! A drag opens an editor gesture: every intermediate command resolves
//! against the state at the press and the whole drag is one undo step.

use std::time::Instant;

use ferrous_engine::{AppContext, CursorIcon, MouseButton};
use motioner_core::{Frame, Vec2};

use super::{modifiers, PreviewApp};
use crate::bounds::Bounds;
use crate::command::Panel as PanelId;
use crate::gizmo::{self, Handle};
use crate::inspect::{self, FieldTarget};
use crate::layout::{Hit, Layout, PanelSizes, Rect, Splitter};
use crate::x_to_frame;

/// Two clicks on the same target within this window are a double click.
pub(super) const DOUBLE_CLICK: std::time::Duration = std::time::Duration::from_millis(420);
/// Pointer travel below this is a click, not a scrub.
pub(super) const CLICK_SLOP: f32 = 3.0;
/// Snap threshold in screen pixels.
pub(super) const SNAP_PX: f64 = 6.0;

/// An in-progress pointer drag.
pub(super) enum Drag {
    /// Scrubbing the playhead.
    Scrub,
    /// Resizing a panel.
    Splitter {
        which: Splitter,
        origin: (f32, f32),
        start: PanelSizes,
    },
    /// Dragging a canvas gizmo handle.
    Gizmo(GizmoDrag),
    /// Scrubbing a numeric field.
    Field {
        target: FieldTarget,
        start_value: f64,
        start_x: f32,
        moved: bool,
    },
    /// Retiming the selected keys.
    Keys { start_x: f32, delta: Frame },
    /// Box-selecting keys in the track area.
    Marquee {
        start: (f32, f32),
        current: (f32, f32),
    },
    /// Panning the canvas (middle drag or Space+drag).
    Pan {
        /// Previous pointer position in window pixels.
        last: (f32, f32),
    },
}

/// What a gizmo drag needs from its press.
pub(super) struct GizmoDrag {
    pub(super) handle: Handle,
    pub(super) id: String,
    pub(super) start_pointer: Vec2,
    pub(super) start_bounds: Bounds,
    pub(super) start_position: Vec2,
    pub(super) start_scale: Vec2,
    pub(super) start_rotation: f64,
    pub(super) center: Vec2,
    pub(super) start_angle: f64,
    pub(super) other_bounds: Vec<Bounds>,
}

/// Pointer state carried between frames.
pub(super) struct Pointer {
    pub(super) hover: Option<Hit>,
    pub(super) pressed: Option<Hit>,
    pub(super) drag: Option<Drag>,
    pub(super) hover_since: Instant,
    cursor: CursorIcon,
    pub(super) last_click: Option<(Instant, Option<Hit>)>,
}

impl Pointer {
    pub(super) fn new() -> Self {
        Self {
            hover: None,
            pressed: None,
            drag: None,
            hover_since: Instant::now(),
            cursor: CursorIcon::Default,
            last_click: None,
        }
    }

    /// The marquee rectangle while box-selecting.
    pub(super) fn marquee_rect(&self) -> Option<Rect> {
        match self.drag {
            Some(Drag::Marquee { start, current }) => Some(span(start, current)),
            _ => None,
        }
    }

    /// The retime offset while dragging keys.
    pub(super) fn retime_delta(&self) -> Option<Frame> {
        match self.drag {
            Some(Drag::Keys { delta, .. }) if delta != 0 => Some(delta),
            _ => None,
        }
    }
}

pub(super) fn span(a: (f32, f32), b: (f32, f32)) -> Rect {
    Rect::new(
        a.0.min(b.0),
        a.1.min(b.1),
        (b.0 - a.0).abs(),
        (b.1 - a.1).abs(),
    )
}

impl PreviewApp {
    pub(super) fn handle_pointer(&mut self, ctx: &mut AppContext<'_>, layout: &Layout) {
        let now = Instant::now();
        let (mx, my) = ctx.input.mouse_pos_f32();
        let base_hit = layout.hit(
            mx,
            my,
            self.editor.outline().len(),
            self.scroll_px(PanelId::Layers),
            self.scroll_px(PanelId::Timeline),
        );
        let hover = self
            .hit_inspector(layout, mx, my)
            .or_else(|| self.hit_orphans(layout, mx, my))
            .or_else(|| self.hit_timeline(layout, mx, my))
            .or_else(|| self.hit_gizmo(layout, mx, my))
            .or(base_hit);
        if self.pointer.drag.is_none() && hover != self.pointer.hover {
            self.pointer.hover = hover;
            self.pointer.hover_since = now;
        }
        let modifiers = modifiers(ctx);
        if ctx.input.button_just_pressed(MouseButton::Left) {
            self.on_press(layout, hover, (mx, my), modifiers, now);
        }
        if ctx.input.button_just_pressed(MouseButton::Middle) && layout.canvas.contains(mx, my) {
            self.pointer.drag = Some(Drag::Pan { last: (mx, my) });
        }
        if ctx.input.is_button_down(MouseButton::Left)
            || ctx.input.is_button_down(MouseButton::Middle)
        {
            self.on_drag(layout, (mx, my), modifiers);
        }
        if ctx.input.button_just_released(MouseButton::Left) {
            self.on_release(layout, hover);
        }
        if ctx.input.button_just_released(MouseButton::Middle) {
            self.end_pan();
        }
        let scroll = ctx.input.scroll_y();
        if scroll.abs() > 0.01 {
            self.on_scroll(layout, hover, scroll, (mx, my), modifiers);
        }
        let cursor = self.cursor_for(hover);
        if cursor != self.pointer.cursor {
            ctx.window.set_cursor(cursor);
            self.pointer.cursor = cursor;
        }
    }

    // ---- hit testing -------------------------------------------------------

    pub(super) fn hit_inspector(&self, layout: &Layout, mx: f32, my: f32) -> Option<Hit> {
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
                    if let Some(segment) = segments.iter().position(|r| r.contains(mx, my)) {
                        return Some(Hit::InspectSegment {
                            item: index,
                            segment,
                        });
                    }
                    if let Some(field) = fields.iter().position(|(r, _)| r.contains(mx, my)) {
                        return Some(Hit::InspectField { item: index, field });
                    }
                }
                inspect::Item::Section { .. } => {}
            }
        }
        Some(Hit::Inspector)
    }

    pub(super) fn hit_orphans(&self, layout: &Layout, mx: f32, my: f32) -> Option<Hit> {
        if !layout.layers.body.contains(mx, my) {
            return None;
        }
        let rows = self.editor.outline().len();
        let scroll = self.scroll_px(PanelId::Layers);
        let suggestions = self.editor.orphan_suggestions();
        (0..self.editor.orphans().len()).find_map(|index| {
            let row = layout.orphan_row(index, rows, scroll);
            if !row.contains(mx, my) {
                return None;
            }
            let rebindable = suggestions
                .get(index)
                .is_some_and(|(_, best)| !best.is_empty());
            Some(
                if rebindable && layout.orphan_rebind(row).contains(mx, my) {
                    Hit::OrphanRebind(index)
                } else if layout.orphan_delete(row).contains(mx, my) {
                    Hit::OrphanDelete(index)
                } else {
                    Hit::OrphanRow(index)
                },
            )
        })
    }

    pub(super) fn hit_timeline(&self, layout: &Layout, mx: f32, my: f32) -> Option<Hit> {
        let in_names = layout.timeline.names.contains(mx, my);
        let in_tracks = layout.timeline.tracks.contains(mx, my);
        if !(in_names || in_tracks) {
            return None;
        }
        let row = self
            .timeline_layout
            .rows
            .iter()
            .find(|r| r.rect.contains(mx, my))?;
        let inside = |rect: Option<Rect>| rect.is_some_and(|r| r.contains(mx, my));
        Some(if inside(row.chevron) {
            Hit::LayerChevron(row.row)
        } else if inside(row.stopwatch) {
            Hit::TimelineStopwatch(row.row)
        } else if inside(row.prev) {
            Hit::TimelinePrevKey(row.row)
        } else if inside(row.key) {
            Hit::TimelineKeyToggle(row.row)
        } else if inside(row.next) {
            Hit::TimelineNextKey(row.row)
        } else if inside(row.field) {
            Hit::TimelineField(row.row)
        } else if let Some(frame) = in_tracks
            .then(|| self.diamond_at(row.row, mx, layout))
            .flatten()
        {
            Hit::TimelineDiamond {
                row: row.row,
                frame,
            }
        } else {
            Hit::TimelineRow(row.row)
        })
    }

    pub(super) fn hit_gizmo(&self, layout: &Layout, mx: f32, my: f32) -> Option<Hit> {
        if !layout.canvas.contains(mx, my) {
            return None;
        }
        let bounds = self.last_bounds?;
        let view = super::canvas::viewport(self.editor.session(), self.editor.view(), layout)?;
        let point = view.to_composition(mx, my);
        gizmo::hit_handle(bounds, point, view.scale).map(Hit::Gizmo)
    }

    pub(super) fn diamond_at(&self, row: usize, mx: f32, layout: &Layout) -> Option<Frame> {
        let entry = self.editor.timeline().rows().get(row)?;
        let frames: Vec<Frame> = if entry.kind.is_layer() {
            entry.summary.iter().map(|k| k.frame).collect()
        } else {
            entry.keys.clone()
        };
        let reach = layout.metrics.px(6.0);
        frames
            .into_iter()
            .find(|frame| (self.frame_x(layout, *frame) - mx).abs() <= reach)
    }

    pub(super) fn frame_x(&self, layout: &Layout, frame: Frame) -> f32 {
        let scrub = layout.timeline.scrub;
        crate::timeline::frame_to_x(
            frame,
            self.editor.transport().duration().max(1),
            f64::from(scrub.x),
            f64::from(scrub.w),
        ) as f32
    }

    pub(super) fn x_frame(&self, layout: &Layout, x: f32) -> Frame {
        let scrub = layout.timeline.scrub;
        x_to_frame(
            f64::from(x),
            self.editor.transport().duration().max(1),
            f64::from(scrub.x),
            f64::from(scrub.w),
        )
    }

    pub(super) fn cursor_for(&self, hover: Option<Hit>) -> CursorIcon {
        let resize = |which: Splitter| match which {
            Splitter::Left | Splitter::Right => CursorIcon::ColResize,
            Splitter::Timeline => CursorIcon::RowResize,
        };
        let gizmo_cursor = |handle: Handle, grabbing: bool| match handle.cursor() {
            gizmo::GizmoCursor::Move => CursorIcon::Move,
            gizmo::GizmoCursor::Grab if grabbing => CursorIcon::Grabbing,
            gizmo::GizmoCursor::Grab => CursorIcon::Grab,
            gizmo::GizmoCursor::NwseResize => CursorIcon::NwseResize,
            gizmo::GizmoCursor::NeswResize => CursorIcon::NeswResize,
        };
        match (self.pointer.drag.as_ref(), hover) {
            (Some(Drag::Pan { .. }), _) => CursorIcon::Move,
            (Some(Drag::Splitter { which, .. }), _) => resize(*which),
            (None, Some(Hit::Splitter(which))) => resize(which),
            (Some(Drag::Scrub | Drag::Field { .. } | Drag::Keys { .. }), _)
            | (None, Some(Hit::Scrub)) => CursorIcon::EwResize,
            (Some(Drag::Gizmo(drag)), _) => gizmo_cursor(drag.handle, true),
            (None, Some(Hit::Gizmo(handle))) => gizmo_cursor(handle, false),
            (
                None,
                Some(
                    Hit::Transport(_)
                    | Hit::Toggle(_)
                    | Hit::SnapToggle
                    | Hit::ZoomPill
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
                    | Hit::OrphanRebind(_)
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
}

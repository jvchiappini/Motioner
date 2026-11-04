//! Drags, releases, the mouse wheel and scrolling a new selection into
//! view. A drag runs one command per move inside an editor gesture.

use std::time::Instant;

use motioner_core::{OverrideValue, Property, Vec2};

use super::pointer::{span, Drag, GizmoDrag, CLICK_SLOP, SNAP_PX};
use super::PreviewApp;
use crate::command::{Command, KeySpec, Panel as PanelId, Time};
use crate::editor::channel_name;
use crate::gizmo::{self, Handle};
use crate::input::Modifiers;
use crate::inspect::{self, FieldTarget};
use crate::layout::{resize_panels, scroll_into_view, Hit, Layout, Rect, Splitter};
use crate::theme::size;
use crate::timeline_model::RowKind;

impl PreviewApp {
    #[allow(clippy::too_many_lines)] // one arm per drag kind
    pub(super) fn on_drag(&mut self, layout: &Layout, at: (f32, f32), modifiers: Modifiers) {
        let Some(mut drag) = self.pointer.drag.take() else {
            return;
        };
        match &mut drag {
            Drag::Scrub => {
                let frame = self.x_frame(layout, at.0);
                self.run(&Command::Seek(frame));
            }
            Drag::Splitter {
                which,
                origin,
                start,
            } => {
                let delta = (
                    (at.0 - origin.0) / layout.metrics.scale,
                    (at.1 - origin.1) / layout.metrics.scale,
                );
                let sizes = resize_panels(*start, *which, delta);
                let (panel, size) = match which {
                    Splitter::Left => (PanelId::Layers, sizes.left),
                    Splitter::Right => (PanelId::Inspector, sizes.right),
                    Splitter::Timeline => (PanelId::Timeline, sizes.timeline),
                };
                self.run(&Command::PanelSize {
                    panel,
                    size: f64::from(size),
                });
            }
            Drag::Field {
                target,
                start_value,
                start_x,
                moved,
            } => {
                let dx = at.0 - *start_x;
                if dx.abs() > CLICK_SLOP {
                    *moved = true;
                }
                if *moved {
                    let value = *start_value + f64::from(dx) * scrub_step(*target, modifiers);
                    if let Some(command) = self.field_value_command(*target, value) {
                        self.run(&command);
                    }
                }
            }
            Drag::Gizmo(gizmo) => {
                if let Some(command) = self.gizmo_command(gizmo, layout, at, modifiers) {
                    self.run(&command);
                }
            }
            Drag::Keys { start_x, delta } => {
                let frames = self.x_frame(layout, at.0) - self.x_frame(layout, *start_x);
                if frames != *delta {
                    *delta = frames;
                    self.run(&Command::RetimeKeys(frames));
                }
            }
            Drag::GraphKey {
                id, property, from, ..
            } => {
                if let Some(graph) = self.editor.graph() {
                    let (frame, value) = graph.value_at(layout.graph_plot(), at.0, at.1);
                    if frame != *from || value.is_finite() {
                        self.run(&Command::MoveKeyValue {
                            id: id.clone(),
                            property: property.clone(),
                            from: Time::Frame(*from),
                            to: Time::Frame(frame),
                            value: OverrideValue::Number(value),
                        });
                    }
                }
            }
            Drag::GraphHandle {
                model,
                segment,
                kind,
            } => {
                let plot = layout.graph_plot();
                let (frame, value) = model.value_at(plot, at.0, at.1);
                if let Some(curve) = model.drag_handle(*segment, *kind, frame, value) {
                    let Some(arriving) = model.keys.get(*segment + 1) else {
                        return;
                    };
                    self.run(&Command::Ease {
                        id: model.id.clone(),
                        property: model.property.clone(),
                        time: Time::Frame(arriving.frame),
                        ease: motioner_core::Ease::Bezier(curve),
                    });
                }
            }
            Drag::GraphPan { last } => {
                if let Some(graph) = self.editor.graph() {
                    let plot = layout.graph_plot();
                    if plot.w > 0.0 && plot.h > 0.0 {
                        let dx = f64::from(at.0 - last.0) / f64::from(plot.w);
                        let dy = f64::from(at.1 - last.1) / f64::from(plot.h);
                        if dx != 0.0 || dy != 0.0 {
                            self.run(&Command::GraphView {
                                zoom: graph.zoom_x,
                                pan_x: graph.pan_x - dx,
                                pan_y: graph.pan_y + dy,
                                zoom_y: Some(graph.zoom_y),
                            });
                        }
                    }
                }
                *last = at;
            }
            Drag::Asset { last, moved, .. } => {
                if (at.0 - last.0).abs() > CLICK_SLOP || (at.1 - last.1).abs() > CLICK_SLOP {
                    *moved = true;
                }
                *last = at;
            }
            Drag::Marquee { current, .. } => *current = at,
            Drag::Pan { last } => {
                if let Some(view) =
                    super::canvas::viewport(self.editor.session(), self.editor.view(), layout)
                {
                    if view.scale > 0.0 {
                        let dx = f64::from(at.0 - last.0) / view.scale;
                        let dy = f64::from(at.1 - last.1) / view.scale;
                        if dx != 0.0 || dy != 0.0 {
                            self.run(&Command::Pan { dx, dy });
                        }
                    }
                }
                *last = at;
            }
        }
        self.pointer.drag = Some(drag);
    }

    /// Ends a middle-button pan; panning is view state, not an undo step.
    pub(super) fn end_pan(&mut self) {
        if matches!(self.pointer.drag, Some(Drag::Pan { .. })) {
            self.pointer.drag = None;
        }
    }

    /// The `edit` command a numeric field scrub sends.
    pub(super) fn field_value_command(&self, target: FieldTarget, value: f64) -> Option<Command> {
        let id = self.editor.node_identity(self.editor.selected()?);
        let property = match target {
            FieldTarget::Transform(property, axis) => {
                let axis = match axis {
                    inspect::Axis::X if property.is_vector() => Some(motioner_core::Axis::X),
                    inspect::Axis::Y if property.is_vector() => Some(motioner_core::Axis::Y),
                    _ => None,
                };
                channel_name(property, axis)
            }
            FieldTarget::Content(field) => field.name().clone(),
            FieldTarget::Effect(channel) => channel.name(),
            _ => return None,
        };
        Some(Command::Edit {
            id,
            property,
            value: OverrideValue::Number(value),
        })
    }

    /// The `edit` command a gizmo drag sends, from its captured start.
    pub(super) fn gizmo_command(
        &mut self,
        drag: &GizmoDrag,
        layout: &Layout,
        at: (f32, f32),
        modifiers: Modifiers,
    ) -> Option<Command> {
        let point = super::canvas::viewport(self.editor.session(), self.editor.view(), layout)
            .map(|view| view.to_composition(at.0, at.1))?;
        let (property, value) = match drag.handle {
            Handle::Move => {
                let delta = Vec2::new(
                    point.x - drag.start_pointer.x,
                    point.y - drag.start_pointer.y,
                );
                let (delta, guides) = if self.editor.view().snap {
                    gizmo::snap_delta(
                        drag.start_bounds,
                        delta,
                        self.composition_size(),
                        &drag.other_bounds,
                        SNAP_PX / self.zoom(layout).max(1e-9),
                    )
                } else {
                    (delta, Vec::new())
                };
                self.guides = guides;
                let position = gizmo::move_position(drag.start_position, delta, modifiers.shift);
                (Property::Position, OverrideValue::Vec2(position))
            }
            Handle::Rotate => {
                let angle = gizmo::angle_of(drag.center, point);
                let delta = gizmo::rotation_delta(drag.start_angle, angle, modifiers.shift);
                (
                    Property::Rotation,
                    OverrideValue::Number(drag.start_rotation + delta),
                )
            }
            handle => {
                let corner = match handle {
                    Handle::TopLeft => gizmo::Corner::TopLeft,
                    Handle::TopRight => gizmo::Corner::TopRight,
                    Handle::BottomRight => gizmo::Corner::BottomRight,
                    _ => gizmo::Corner::BottomLeft,
                };
                let anchor = if modifiers.alt {
                    drag.center
                } else {
                    gizmo::scale_anchor(drag.start_bounds, corner, None)
                };
                let factors =
                    gizmo::scale_factors(drag.start_bounds, corner, point, anchor, modifiers.shift);
                self.guides.clear();
                (
                    Property::Scale,
                    OverrideValue::Vec2(Vec2::new(
                        (drag.start_scale.x * factors.x).clamp(-1000.0, 1000.0),
                        (drag.start_scale.y * factors.y).clamp(-1000.0, 1000.0),
                    )),
                )
            }
        };
        Some(Command::Edit {
            id: drag.id.clone(),
            property: property.name().to_owned(),
            value,
        })
    }

    pub(super) fn on_release(&mut self, layout: &Layout, hover: Option<Hit>) {
        match self.pointer.drag.take() {
            Some(Drag::Field {
                target,
                moved: false,
                ..
            }) => {
                self.editor.end_gesture();
                self.begin_edit(target);
            }
            Some(Drag::Marquee { start, current }) => {
                if start == current {
                    if let Some(Hit::TimelineRow(row)) = hover {
                        self.press_timeline_row(row, Modifiers::default());
                    }
                } else {
                    self.select_in_marquee(layout, span(start, current));
                }
            }
            Some(Drag::Scrub) => self.editor.transport_mut().end_scrub(),
            Some(Drag::Asset { key, last, moved }) => {
                if moved && layout.canvas.contains(last.0, last.1) {
                    if let Some(viewport) = crate::app::canvas::viewport(
                        self.editor.session(),
                        self.editor.view(),
                        layout,
                    ) {
                        let point = viewport.to_composition(last.0, last.1);
                        self.run(&Command::PlaceAsset {
                            key,
                            frame: None,
                            x: Some(point.x),
                            y: Some(point.y),
                        });
                    }
                }
            }
            Some(
                Drag::Gizmo(_)
                | Drag::Keys { .. }
                | Drag::Field { .. }
                | Drag::GraphKey { .. }
                | Drag::GraphHandle { .. },
            ) => {
                self.editor.end_gesture();
            }
            Some(Drag::Splitter { .. } | Drag::Pan { .. } | Drag::GraphPan { .. }) | None => {}
        }
        self.pointer.pressed = None;
        self.guides.clear();
        self.marquee = None;
        self.editor.session_mut().flush_overrides(Instant::now());
    }

    pub(super) fn select_in_marquee(&mut self, layout: &Layout, rect: Rect) {
        if rect.w < 2.0 && rect.h < 2.0 {
            return;
        }
        let half = layout.metrics.px(5.0);
        let mut keys = Vec::new();
        for row in &self.timeline_layout.rows {
            let Some(entry) = self.editor.timeline().rows().get(row.row) else {
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
            let Some(id) = self.row_id(outline) else {
                continue;
            };
            for frame in &entry.keys {
                let x = self.frame_x(layout, *frame);
                let diamond =
                    Rect::new(x - half, row.rect.center_y() - half, half * 2.0, half * 2.0);
                if diamond.overlaps(rect) {
                    keys.push(KeySpec {
                        id: id.clone(),
                        property: channel_name(property, axis),
                        time: Time::Frame(*frame),
                    });
                }
            }
        }
        self.run(&Command::SelectKeys { keys, add: false });
    }

    pub(super) fn on_scroll(
        &mut self,
        layout: &Layout,
        hover: Option<Hit>,
        scroll: f32,
        at: (f32, f32),
        modifiers: Modifiers,
    ) {
        if modifiers.ctrl && matches!(hover, Some(Hit::Canvas | Hit::Gizmo(_))) {
            if let Some(view) =
                super::canvas::viewport(self.editor.session(), self.editor.view(), layout)
            {
                let point = view.to_composition(at.0, at.1);
                let steps = if scroll > 0.0 { 1 } else { -1 };
                self.run(&Command::ZoomBy {
                    steps,
                    x: Some(point.x),
                    y: Some(point.y),
                });
            }
            return;
        }
        if matches!(hover, Some(Hit::GraphPlot)) {
            if let Some(graph) = self.editor.graph() {
                let factor = if scroll > 0.0 { 1.25 } else { 0.8 };
                self.run(&Command::GraphView {
                    zoom: graph.zoom_x * factor,
                    pan_x: graph.pan_x,
                    pan_y: graph.pan_y,
                    zoom_y: Some(graph.zoom_y * factor),
                });
            }
            return;
        }
        let shift = modifiers.shift;
        let (panel, content, visible) = match hover {
            Some(Hit::Scrub) => {
                let direction = if scroll > 0.0 { -1 } else { 1 };
                self.run(&Command::Step(direction * if shift { 10 } else { 1 }));
                return;
            }
            Some(
                Hit::LayerRow(_)
                | Hit::LayerEye(_)
                | Hit::LayerLock(_)
                | Hit::LayerSolo(_)
                | Hit::LayerShy(_)
                | Hit::LayerFilter
                | Hit::OrphanRow(_)
                | Hit::OrphanDelete(_)
                | Hit::OrphanRebind(_),
            ) => (
                PanelId::Layers,
                self.editor.visible_outline().len() as f32 * layout.row_h
                    + layout.metrics.px(size::GRID * 2.0),
                layout.layers.body.h,
            ),
            Some(
                Hit::Track(_)
                | Hit::LayerChevron(_)
                | Hit::TimelineRow(_)
                | Hit::TimelineDiamond { .. }
                | Hit::TimelineStopwatch(_)
                | Hit::TimelinePrevKey(_)
                | Hit::TimelineKeyToggle(_)
                | Hit::TimelineNextKey(_)
                | Hit::TimelineField(_),
            ) => (
                PanelId::Timeline,
                self.timeline_layout.height,
                layout.timeline.tracks.h,
            ),
            Some(
                Hit::Inspector
                | Hit::InspectField { .. }
                | Hit::InspectStopwatch(_)
                | Hit::InspectDiamond(_)
                | Hit::InspectSegment { .. }
                | Hit::InspectCopy,
            ) => (
                PanelId::Inspector,
                self.inspector_layout.height,
                layout.inspector.body.h,
            ),
            _ => return,
        };
        let max = (content - visible).max(0.0);
        let px = (self.scroll_px(panel) - scroll * layout.row_h).clamp(0.0, max);
        let offset = f64::from(px / layout.metrics.scale.max(1e-3));
        self.run(&Command::Scroll { panel, offset });
    }

    /// Scrolls the layers and timeline to a new selection, and the timeline
    /// to the first newly selected key.
    pub(super) fn reveal(&mut self, layout: &Layout) {
        if self.editor.selected() != self.revealed {
            self.revealed = self.editor.selected();
            self.set_scroll_px(PanelId::Inspector, 0.0);
            if let Some(row) = self.editor.selected_row() {
                let top = row as f32 * layout.row_h;
                let pad = layout.metrics.px(size::GRID);
                let scroll = scroll_into_view(
                    self.scroll_px(PanelId::Layers),
                    top,
                    layout.row_h + pad * 2.0,
                    layout.layers.body.h,
                );
                self.set_scroll_px(PanelId::Layers, scroll);
                if let Some(index) = self.editor.timeline().layer_row(row) {
                    self.reveal_timeline_row(layout, index);
                }
            }
        }
        let keys = self.editor.selected_keys().len();
        if keys != self.revealed_keys {
            self.revealed_keys = keys;
            let first = self.editor.selected_keys().first().cloned();
            if let Some(key) = first {
                let index = self.editor.timeline().rows().iter().position(|row| {
                    matches!(row.kind, RowKind::Transform { outline, property, axis }
                        if channel_name(property, axis) == key.property
                            && self.row_id(outline).as_deref() == Some(key.id.as_str()))
                });
                if let Some(index) = index {
                    self.reveal_timeline_row(layout, index);
                }
            }
        }
    }

    pub(super) fn reveal_timeline_row(&mut self, layout: &Layout, index: usize) {
        let scroll = self.scroll_px(PanelId::Timeline);
        let Some(rect) = self.timeline_layout.rows.iter().find(|r| r.row == index) else {
            return;
        };
        let top = rect.rect.y + scroll - layout.timeline.tracks.y;
        let scroll = scroll_into_view(scroll, top, rect.rect.h, layout.timeline.tracks.h);
        self.set_scroll_px(PanelId::Timeline, scroll);
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

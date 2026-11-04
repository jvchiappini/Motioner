//! Presses: what a click on each element of the window does, as a
//! [`Command`].

use std::time::Instant;

use motioner_core::{Frame, OverrideValue, Vec2};

use super::pointer::{Drag, GizmoDrag, DOUBLE_CLICK};
use super::PreviewApp;
use crate::command::{Command, KeySpec, Time};
use crate::editor::channel_name;
use crate::gizmo::{self, Handle};
use crate::input::Modifiers;
use crate::inspect::{self, FieldTarget};
use crate::layout::{Hit, Layout, TopBarToggle, TransportButton};
use crate::timeline_model::RowKind;

impl PreviewApp {
    pub(super) fn on_press(
        &mut self,
        layout: &Layout,
        hover: Option<Hit>,
        at: (f32, f32),
        modifiers: Modifiers,
        now: Instant,
    ) {
        let double = self
            .pointer
            .last_click
            .is_some_and(|(when, hit)| hit == hover && when.elapsed() < DOUBLE_CLICK);
        self.pointer.last_click = Some((now, hover));
        self.pointer.pressed = hover;
        if self.editing.is_some() {
            // Clicking elsewhere commits the field being typed in.
            self.commit_edit();
        }
        let Some(hit) = hover else {
            return;
        };
        self.press(hit, layout, at, modifiers, double);
    }

    #[allow(clippy::too_many_lines)] // one arm per clickable element
    pub(super) fn press(
        &mut self,
        hit: Hit,
        layout: &Layout,
        at: (f32, f32),
        modifiers: Modifiers,
        double: bool,
    ) {
        match hit {
            Hit::LeftTab(tab) => self.left_tab = tab,
            Hit::AssetRow(index) | Hit::AssetCell(index) => {
                self.assets.selected = Some(index);
                if let Some(asset) = self.assets.entries.get(index) {
                    self.pointer.drag = Some(Drag::Asset {
                        key: asset.key.clone(),
                        last: at,
                        moved: false,
                    });
                }
                if double {
                    if let Some(asset) = self.assets.entries.get(index) {
                        let key = asset.key.clone();
                        self.run(&Command::PlaceAsset {
                            key,
                            frame: None,
                            x: None,
                            y: None,
                        });
                    }
                }
            }
            Hit::AssetAdd => {
                self.run(&Command::ImportAsset {
                    path: String::new(),
                });
            }
            Hit::AssetView => self.assets.toggle_view(),
            Hit::AssetRemoveUnused => {
                self.run(&Command::RemoveUnusedAssets);
            }
            Hit::RecoverRestore => {
                self.run(&Command::RestoreBackup);
            }
            Hit::RecoverDismiss => {
                self.run(&Command::DismissRecovery);
            }
            Hit::Scrub if self.editor.session().document().is_some() => {
                self.editor.transport_mut().begin_scrub();
                let frame = self.x_frame(layout, at.0);
                self.run(&Command::Seek(frame));
                self.pointer.drag = Some(Drag::Scrub);
            }
            Hit::Splitter(which) => {
                self.pointer.drag = Some(Drag::Splitter {
                    which,
                    origin: at,
                    start: self.editor.view().sizes,
                });
            }
            Hit::LayerRow(row) | Hit::Track(row) => {
                if let Some(id) = self.row_id(row) {
                    self.run(&Command::Select(id));
                    if double && matches!(hit, Hit::LayerRow(_)) {
                        self.begin_edit(FieldTarget::Name);
                    }
                }
            }
            Hit::LayerEye(row) => {
                if let (Some(id), Some(node)) = (self.row_id(row), self.editor.outline().get(row)) {
                    let visible = self
                        .editor
                        .session()
                        .document()
                        .and_then(|d| d.node(node.node))
                        .is_some_and(motioner_core::Node::is_visible);
                    self.run(&Command::Visible {
                        id,
                        visible: !visible,
                    });
                }
            }
            Hit::LayerLock(row) => {
                if let (Some(id), Some(node)) = (self.row_id(row), self.editor.outline().get(row)) {
                    let locked = self.editor.node_locked(node.node);
                    self.run(&Command::Lock {
                        id,
                        locked: !locked,
                    });
                }
            }
            Hit::LayerSolo(row) => {
                if let (Some(id), Some(node)) = (self.row_id(row), self.editor.outline().get(row)) {
                    let on = !self.editor.is_solo(node.node);
                    self.run(&Command::Solo { id, on });
                }
            }
            Hit::LayerShy(row) => {
                if let (Some(id), Some(node)) = (self.row_id(row), self.editor.outline().get(row)) {
                    let on = !self.editor.is_shy(node.node);
                    self.run(&Command::Shy { id, on });
                }
            }
            Hit::LayerFilter => self.begin_edit(FieldTarget::Filter),
            Hit::ShowShy => {
                let on = !self.editor.show_shy();
                self.run(&Command::ShowShy(on));
            }
            Hit::LayerChevron(row) => self.press_chevron(row, modifiers.alt),
            Hit::Toggle(TopBarToggle::SafeArea) => {
                let on = !self.editor.view().safe_area;
                self.run(&Command::SafeArea(on));
            }
            Hit::SnapToggle => {
                let on = !self.editor.view().snap;
                self.run(&Command::Snap(on));
            }
            Hit::ZoomPill => {
                self.run(&Command::ZoomFit);
            }
            Hit::Canvas => {
                if self.space_down {
                    // Space+drag pans the view; a Space tap without a drag
                    // still toggles playback when the key is released.
                    self.space_dragged = true;
                    self.pointer.drag = Some(Drag::Pan { last: at });
                } else if let Some(view) =
                    super::canvas::viewport(self.editor.session(), self.editor.view(), layout)
                {
                    self.pending_pick = Some(view.to_composition(at.0, at.1));
                }
            }
            Hit::Gizmo(handle) => self.begin_gizmo(layout, handle, at),
            Hit::InspectCopy => {
                self.run(&Command::CopyId);
            }
            Hit::InspectStopwatch(item) => {
                if let Some((id, property)) = self.inspector_channel(item) {
                    self.run(&Command::Stopwatch { id, property });
                }
            }
            Hit::InspectDiamond(item) => {
                if let Some((id, property)) = self.inspector_channel(item) {
                    self.run(&Command::ToggleKey { id, property });
                }
            }
            Hit::InspectSegment { item, segment } => self.press_segment(item, segment),
            Hit::InspectField { item, field } => self.press_inspector_field(item, field, at.0),
            Hit::TimelineRow(row) => {
                self.press_timeline_row(row, modifiers);
                if double {
                    if let Some((id, property)) = self.timeline_channel(row) {
                        self.run(&Command::GraphOpen { id, property });
                    }
                } else if at.0 >= layout.timeline.tracks.x {
                    self.pointer.drag = Some(Drag::Marquee {
                        start: at,
                        current: at,
                    });
                }
            }
            Hit::TimelineStopwatch(row) => {
                if let Some((id, property)) = self.timeline_channel(row) {
                    self.run(&Command::Stopwatch { id, property });
                }
            }
            Hit::TimelinePrevKey(row) => {
                if let Some((id, property)) = self.timeline_channel(row) {
                    self.run(&Command::PrevKey { id, property });
                }
            }
            Hit::TimelineNextKey(row) => {
                if let Some((id, property)) = self.timeline_channel(row) {
                    self.run(&Command::NextKey { id, property });
                }
            }
            Hit::TimelineKeyToggle(row) => {
                if let Some((id, property)) = self.timeline_channel(row) {
                    self.run(&Command::ToggleKey { id, property });
                }
            }
            Hit::TimelineField(row) => self.press_timeline_field(row, at.0),
            Hit::TimelineDiamond { row, frame } => {
                self.press_diamond(row, frame, at.0, modifiers.shift);
            }
            Hit::OrphanDelete(index) => {
                if let Some(entry) = self.editor.orphans().get(index) {
                    self.run(&Command::DeleteOrphan {
                        id: entry.id.clone(),
                        property: entry.property.clone(),
                        time: entry.frame().map(Time::Frame),
                    });
                }
            }
            Hit::OrphanRebind(index) => {
                if let Some((entry, best)) =
                    self.editor
                        .orphan_suggestions()
                        .get(index)
                        .and_then(|(entry, list)| {
                            list.first().map(|best| (entry.clone(), best.clone()))
                        })
                {
                    self.run(&Command::AssignId {
                        from: entry.id,
                        to: best,
                    });
                }
            }
            Hit::GraphPlot => {
                self.pointer.drag = Some(Drag::GraphPan { last: at });
            }
            Hit::GraphKey(key) => {
                let point = self.editor.graph().and_then(|graph| {
                    graph.keys.get(key).map(|point| {
                        (
                            graph.id.clone(),
                            graph.property.clone(),
                            point.frame,
                            point.ease,
                        )
                    })
                });
                if let Some((id, property, from, ease)) = point {
                    if double {
                        // Double click converts a named ease to an editable
                        // bezier; a bezier key is already editable.
                        if let Some(curve) = crate::graph::bezier_for(ease) {
                            self.run(&Command::Ease {
                                id,
                                property,
                                time: Time::Frame(from),
                                ease: motioner_core::Ease::Bezier(curve),
                            });
                        }
                    } else {
                        self.editor.begin_gesture();
                        self.pointer.drag = Some(Drag::GraphKey { id, property, from });
                    }
                }
            }
            Hit::GraphHandle { segment, kind } => {
                let model = self.editor.graph().cloned();
                if let Some(model) = model {
                    self.editor.begin_gesture();
                    self.pointer.drag = Some(Drag::GraphHandle {
                        model,
                        segment,
                        kind,
                    });
                }
            }
            Hit::Transport(button) => self.press_transport(button),
            Hit::OrphanRow(_) | Hit::Inspector | Hit::Scrub | Hit::FileMenu => {}
        }
    }

    pub(super) fn press_transport(&mut self, button: TransportButton) {
        if self.editor.session().document().is_none() {
            return;
        }
        let command = match button {
            TransportButton::ToStart => Command::GoToStart,
            TransportButton::StepBack => Command::Step(-1),
            TransportButton::PlayPause => Command::TogglePlay,
            TransportButton::StepForward => Command::Step(1),
            TransportButton::ToEnd => Command::GoToEnd,
            TransportButton::Loop => Command::Loop(!self.editor.transport().looping()),
        };
        self.run(&command);
    }

    pub(super) fn press_chevron(&mut self, row: usize, alt: bool) {
        if alt {
            let all = self.editor.expanded().len() >= self.editor.outline().len();
            self.run(&Command::ExpandAll(!all));
        } else if let Some(id) = self.row_id(row) {
            let open = !self.editor.expanded().contains(&row);
            self.run(&Command::Expand { id, open });
        }
    }

    #[allow(clippy::too_many_lines)] // one arm per segmented control
    pub(super) fn press_segment(&mut self, item: usize, segment: usize) {
        match self.inspector_row_value(item) {
            Some(inspect::RowValue::Ease { .. }) => {
                if let Some((ease, _)) = motioner_core::Ease::ALL.get(segment) {
                    self.run(&Command::SetEase(*ease));
                }
            }
            Some(inspect::RowValue::Blend { .. }) => {
                let Some(id) = self.editor.selected().map(|n| self.editor.node_identity(n)) else {
                    return;
                };
                let Some(mode) = motioner_core::BlendMode::ALL.get(segment) else {
                    return;
                };
                self.run(&Command::Edit {
                    id,
                    property: "blend".to_owned(),
                    value: OverrideValue::Text(mode.name().to_owned()),
                });
            }
            Some(inspect::RowValue::EffectAdd) => {
                let Some(id) = self.editor.selected().map(|n| self.editor.node_identity(n)) else {
                    return;
                };
                let kind = [
                    motioner_core::EffectKind::DropShadow,
                    motioner_core::EffectKind::Blur,
                    motioner_core::EffectKind::Glow,
                ][segment.min(2)];
                self.run(&Command::EffectAdd { id, kind });
            }
            Some(inspect::RowValue::TextAnimAdd) => {
                let Some(id) = self.editor.selected().map(|n| self.editor.node_identity(n)) else {
                    return;
                };
                let (property, from, to) = match segment.min(2) {
                    0 => (
                        motioner_core::CharProperty::Opacity,
                        OverrideValue::Number(0.0),
                        OverrideValue::Number(1.0),
                    ),
                    1 => (
                        motioner_core::CharProperty::Offset,
                        OverrideValue::Vec2(motioner_core::Vec2::new(0.0, 24.0)),
                        OverrideValue::Vec2(motioner_core::Vec2::new(0.0, 0.0)),
                    ),
                    _ => (
                        motioner_core::CharProperty::Color,
                        OverrideValue::Text("#FFFFFF".to_owned()),
                        OverrideValue::Text("#5AB0FF".to_owned()),
                    ),
                };
                self.run(&Command::TextAnimAdd {
                    id,
                    property,
                    from,
                    to,
                    start: crate::command::Time::Frame(self.editor.frame()),
                    stagger: 2.0,
                    duration: 12.0,
                    ease: motioner_core::Ease::QuadOut,
                    order: motioner_core::CharOrder::Forward,
                });
            }
            Some(&inspect::RowValue::TextAnimRemove { index }) => {
                let Some(id) = self.editor.selected().map(|n| self.editor.node_identity(n)) else {
                    return;
                };
                self.run(&Command::TextAnimRemove { id, index });
            }
            Some(&inspect::RowValue::EffectOps { kind }) => {
                let Some(node) = self.editor.selected() else {
                    return;
                };
                let id = self.editor.node_identity(node);
                let stack = self
                    .editor
                    .session()
                    .document()
                    .and_then(|document| document.node(node))
                    .map(|node| {
                        node.effects()
                            .iter()
                            .map(motioner_core::Effect::kind)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let Some(index) = stack.iter().position(|candidate| candidate == &kind) else {
                    return;
                };
                match segment {
                    0 if index > 0 => {
                        self.run(&Command::EffectMove {
                            id,
                            kind,
                            index: index - 1,
                        });
                    }
                    1 if index + 1 < stack.len() => {
                        self.run(&Command::EffectMove {
                            id,
                            kind,
                            index: index + 1,
                        });
                    }
                    2 => {
                        self.run(&Command::EffectRemove { id, kind });
                    }
                    _ => {}
                }
            }
            Some(inspect::RowValue::MaskSource {
                current,
                candidates,
                mode,
                invert,
            }) => {
                let Some(id) = self.editor.selected().map(|n| self.editor.node_identity(n)) else {
                    return;
                };
                // Cycle: the next candidate after the current source, or the
                // first one when the node is not masked yet.
                let next = match current {
                    Some(current) => {
                        let index = candidates
                            .iter()
                            .position(|candidate| candidate == current)
                            .map_or(0, |index| index + 1);
                        candidates.get(index % candidates.len().max(1))
                    }
                    None => candidates.first(),
                };
                let Some(source) = next else {
                    return;
                };
                self.run(&Command::MaskSet {
                    id,
                    source: source.clone(),
                    mode: *mode,
                    invert: *invert,
                });
            }
            Some(inspect::RowValue::MaskMode {
                source,
                mode,
                invert,
            }) => {
                let Some(id) = self.editor.selected().map(|n| self.editor.node_identity(n)) else {
                    return;
                };
                let Some(next) = motioner_core::MaskMode::ALL.get(segment) else {
                    return;
                };
                if next == mode {
                    return;
                }
                self.run(&Command::MaskSet {
                    id,
                    source: source.clone(),
                    mode: *next,
                    invert: *invert,
                });
            }
            Some(inspect::RowValue::MaskInvert {
                source,
                mode,
                invert,
            }) => {
                let Some(id) = self.editor.selected().map(|n| self.editor.node_identity(n)) else {
                    return;
                };
                if segment >= 2 {
                    self.run(&Command::MaskClear { id });
                    return;
                }
                let next = segment == 1;
                if next == *invert {
                    return;
                }
                self.run(&Command::MaskSet {
                    id,
                    source: source.clone(),
                    mode: *mode,
                    invert: next,
                });
            }
            Some(inspect::RowValue::Align { .. }) => {
                let Some(id) = self.editor.selected().map(|n| self.editor.node_identity(n)) else {
                    return;
                };
                let name = ["left", "center", "right"][segment.min(2)];
                self.run(&Command::Edit {
                    id,
                    property: "align".to_owned(),
                    value: OverrideValue::Text(name.to_owned()),
                });
            }
            Some(inspect::RowValue::Cycle {
                target,
                current,
                options,
            }) => {
                let Some(id) = self.editor.selected().map(|n| self.editor.node_identity(n)) else {
                    return;
                };
                if options.is_empty() {
                    return;
                }
                let index = options
                    .iter()
                    .position(|option| option == current)
                    .unwrap_or(0);
                let next = &options[(index + 1) % options.len()];
                let (property, value) = match target {
                    inspect::FieldTarget::Font => {
                        ("font".to_owned(), OverrideValue::Text(next.clone()))
                    }
                    inspect::FieldTarget::Weight => {
                        let Ok(weight) = next.parse::<f64>() else {
                            return;
                        };
                        ("weight".to_owned(), OverrideValue::Number(weight))
                    }
                    _ => return,
                };
                self.run(&Command::Edit {
                    id,
                    property,
                    value,
                });
            }
            _ => {}
        }
    }

    pub(super) fn press_timeline_row(&mut self, row: usize, modifiers: Modifiers) {
        let Some(entry) = self.editor.timeline().rows().get(row) else {
            return;
        };
        let outline = entry.kind.outline();
        let is_layer = entry.kind.is_layer();
        let Some(id) = self.row_id(outline) else {
            return;
        };
        if self.editor.selected_row() != Some(outline) {
            self.run(&Command::Select(id));
        }
        if is_layer && modifiers.alt {
            self.press_chevron(outline, true);
        }
    }

    pub(super) fn begin_gizmo(&mut self, layout: &Layout, handle: Handle, at: (f32, f32)) {
        let view = super::canvas::viewport(self.editor.session(), self.editor.view(), layout);
        let (Some(bounds), Some(point)) = (
            self.last_bounds,
            view.map(|view| view.to_composition(at.0, at.1)),
        ) else {
            return;
        };
        let Some(selected) = self.editor.selected() else {
            return;
        };
        if self.editor.node_locked(selected) {
            return;
        }
        let Some(node) = self
            .editor
            .session()
            .document()
            .and_then(|d| d.node(selected))
        else {
            return;
        };
        let t = node.transform();
        let frame = self.frame();
        let center = Vec2::new(
            (bounds.min_x + bounds.max_x) * 0.5,
            (bounds.min_y + bounds.max_y) * 0.5,
        );
        let drag = GizmoDrag {
            handle,
            id: node.identity().as_str().to_owned(),
            start_pointer: point,
            start_bounds: bounds,
            start_position: motioner_eval::sample_vec2(&t.position, frame),
            start_scale: motioner_eval::sample_vec2(&t.scale, frame),
            start_rotation: motioner_eval::sample(&t.rotation, frame),
            center,
            start_angle: gizmo::angle_of(center, point),
            other_bounds: self.other_bounds(selected),
        };
        self.editor.begin_gesture();
        self.pointer.drag = Some(Drag::Gizmo(drag));
    }

    pub(super) fn press_inspector_field(&mut self, item: usize, field: usize, x: f32) {
        let Some(target) = self.inspector_field_target(item, field) else {
            return;
        };
        let numeric = matches!(
            self.inspector_row_value(item),
            Some(inspect::RowValue::Pair { .. } | inspect::RowValue::Single(_))
        ) && !matches!(target, FieldTarget::Bezier(_) | FieldTarget::Filter);
        match self.editor.numeric_value(target).filter(|_| numeric) {
            Some(value) => self.begin_field_drag(target, value, x),
            None => self.begin_edit(target),
        }
    }

    pub(super) fn press_timeline_field(&mut self, row: usize, x: f32) {
        let Some(RowKind::Transform { property, axis, .. }) =
            self.editor.timeline().rows().get(row).map(|r| r.kind)
        else {
            return;
        };
        if let Some(outline) = self
            .editor
            .timeline()
            .rows()
            .get(row)
            .map(|r| r.kind.outline())
        {
            if self.editor.selected_row() != Some(outline) {
                if let Some(id) = self.row_id(outline) {
                    self.run(&Command::Select(id));
                }
            }
        }
        let field_axis = match axis {
            Some(motioner_core::Axis::X) => inspect::Axis::X,
            Some(motioner_core::Axis::Y) => inspect::Axis::Y,
            None => inspect::Axis::Single,
        };
        let target = FieldTarget::Transform(property, field_axis);
        if let Some(value) = self.editor.numeric_value(target) {
            self.begin_field_drag(target, value, x);
        }
    }

    pub(super) fn begin_field_drag(&mut self, target: FieldTarget, start_value: f64, x: f32) {
        self.editor.begin_gesture();
        self.pointer.drag = Some(Drag::Field {
            target,
            start_value,
            start_x: x,
            moved: false,
        });
    }

    pub(super) fn inspector_row_value(&self, item: usize) -> Option<&inspect::RowValue> {
        let inspect::Item::Row { index, .. } = self.inspector_layout.items.get(item)? else {
            return None;
        };
        self.editor
            .inspector()
            .rows
            .get(*index)
            .map(|row| &row.value)
    }

    pub(super) fn inspector_field_target(&self, item: usize, field: usize) -> Option<FieldTarget> {
        let inspect::Item::Row { fields, .. } = self.inspector_layout.items.get(item)? else {
            return None;
        };
        fields.get(field).map(|(_, target)| *target)
    }

    /// The selected node's identity and the channel of an inspector row.
    pub(super) fn inspector_channel(&self, item: usize) -> Option<(String, String)> {
        let inspect::Item::Row { index, .. } = self.inspector_layout.items.get(item)? else {
            return None;
        };
        let channel = self.editor.inspector().rows.get(*index)?.channel.as_ref()?;
        let id = self.editor.node_identity(self.editor.selected()?);
        Some((id, channel.target.name()))
    }

    /// The identity and channel of a timeline property or content row.
    pub(super) fn timeline_channel(&self, row: usize) -> Option<(String, String)> {
        match self.editor.timeline().rows().get(row)?.kind {
            RowKind::Transform {
                outline,
                property,
                axis,
            } => Some((self.row_id(outline)?, channel_name(property, axis))),
            RowKind::Content { outline, field } => {
                Some((self.row_id(outline)?, field.name().clone()))
            }
            RowKind::Layer { .. } => None,
        }
    }

    pub(super) fn press_diamond(&mut self, row: usize, frame: Frame, x: f32, add: bool) {
        let Some(entry) = self.editor.timeline().rows().get(row).cloned() else {
            return;
        };
        let Some(id) = self.row_id(entry.kind.outline()) else {
            return;
        };
        let keys: Vec<KeySpec> = match entry.kind {
            RowKind::Layer { outline } => {
                let Some(node) = self.editor.outline().get(outline).map(|r| r.node) else {
                    return;
                };
                let Some(document) = self.editor.session().document() else {
                    return;
                };
                crate::editor::channels::node_channels(document, node)
                    .into_iter()
                    .filter(|channel| {
                        crate::editor::channel_has_key(document, node, *channel, frame)
                    })
                    .map(|channel| KeySpec {
                        id: id.clone(),
                        property: channel.name(),
                        time: Time::Frame(frame),
                    })
                    .collect()
            }
            RowKind::Transform { property, axis, .. } => vec![KeySpec {
                id,
                property: channel_name(property, axis),
                time: Time::Frame(frame),
            }],
            RowKind::Content { field, .. } => vec![KeySpec {
                id,
                property: field.name().clone(),
                time: Time::Frame(frame),
            }],
        };
        if self.run(&Command::SelectKeys { keys, add }) {
            self.editor.begin_gesture();
            self.pointer.drag = Some(Drag::Keys {
                start_x: x,
                delta: 0,
            });
        }
    }
}

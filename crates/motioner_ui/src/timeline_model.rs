//! The timeline's row model and layout.
//!
//! Row 0 of the outline is one layer row; an expanded layer adds one
//! transform sub-row per property and one static sub-row per content field.
//! Layer rows summarise every channel, show **when the layer is actually
//! visible** (sampled from the document) and draw inherited parent keys on
//! child rows, so motion that reaches a layer is visible on its own row.
//!
//! Everything is pure: [`TimelineRows::compute`] reads the document,
//! [`layout`] turns the rows into rectangles for drawing and hit testing.

use std::collections::{BTreeMap, BTreeSet};

use motioner_core::{Axis, ContentField, Document, Ease, Frame, NodeContent, NodeId, Property};
use motioner_eval::sample;

use crate::editor::{channels, CHANNELS};
use crate::layout::{Layout, Rect};
use crate::outline::OutlineRow;
use crate::theme::size;

/// Height of a layer row, in logical pixels.
pub const LAYER_ROW: f32 = 28.0;
/// Height of a property sub-row, in logical pixels.
pub const SUB_ROW: f32 = 22.0;

/// What a timeline row represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowKind {
    /// A layer (one per outline row).
    Layer {
        /// Index into the outline.
        outline: usize,
    },
    /// One animatable channel of an expanded layer (an axis for vector
    /// properties).
    Transform {
        /// Index into the outline.
        outline: usize,
        /// The property.
        property: Property,
        /// The axis for vector properties.
        axis: Option<Axis>,
    },
    /// A static content field of an expanded layer.
    Content {
        /// Index into the outline.
        outline: usize,
        /// The field.
        field: ContentField,
    },
}

impl RowKind {
    /// The outline row this row belongs to.
    #[must_use]
    pub fn outline(self) -> usize {
        match self {
            Self::Layer { outline }
            | Self::Transform { outline, .. }
            | Self::Content { outline, .. } => outline,
        }
    }

    /// Whether this is a layer row.
    #[must_use]
    pub fn is_layer(self) -> bool {
        matches!(self, Self::Layer { .. })
    }
}

/// One summary diamond of a layer row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SummaryKey {
    /// Composition frame.
    pub frame: Frame,
    /// Number of channels that have a key at this frame.
    pub channels: u8,
}

/// A frame range (inclusive) where a layer is visible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VisibleRun {
    /// First visible frame.
    pub from: Frame,
    /// Last visible frame.
    pub to: Frame,
}

/// One timeline row.
#[derive(Debug, Clone, PartialEq)]
pub struct TimelineRow {
    /// What the row represents.
    pub kind: RowKind,
    /// Visible frame ranges (layer rows only).
    pub visible: Vec<VisibleRun>,
    /// Summary keys of all channels (layer rows only).
    pub summary: Vec<SummaryKey>,
    /// Parent-group keys drawn hollow on this row.
    pub inherited: Vec<Frame>,
    /// This channel's keys (transform rows only).
    pub keys: Vec<Frame>,
    /// Ease of each key's incoming segment, parallel to keys.
    pub eases: Vec<Ease>,
    /// Whether the channel has keys (transform rows only).
    pub animated: bool,
    /// Whether the row's channel is on a key at the playhead (transform rows).
    pub on_key: bool,
    /// Indentation depth for drawing.
    pub depth: u32,
}

/// The timeline rows of a document and expansion state.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TimelineRows {
    rows: Vec<TimelineRow>,
}

impl TimelineRows {
    /// The rows.
    #[must_use]
    pub fn rows(&self) -> &[TimelineRow] {
        &self.rows
    }

    /// Number of rows.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Whether there are no rows.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// The index of the layer row for an outline row.
    #[must_use]
    pub fn layer_row(&self, outline: usize) -> Option<usize> {
        self.rows
            .iter()
            .position(|row| row.kind == RowKind::Layer { outline })
    }

    /// Builds the rows.
    ///
    /// `expanded` contains the outline indices whose property rows are shown;
    /// `only_animated` hides transform rows without keys (the `U` shortcut).
    #[must_use]
    pub fn compute(
        document: &Document,
        outline: &[OutlineRow],
        expanded: &BTreeSet<usize>,
        only_animated: bool,
    ) -> Self {
        let duration = document.composition().duration();
        let mut rows = Vec::with_capacity(outline.len());
        for (index, outline_row) in outline.iter().enumerate() {
            let node = outline_row.node;
            rows.push(TimelineRow {
                kind: RowKind::Layer { outline: index },
                visible: visible_runs(document, node, duration),
                summary: summary_keys(document, node),
                inherited: inherited_keys(document, node),
                keys: Vec::new(),
                eases: Vec::new(),
                animated: false,
                on_key: false,
                depth: outline_row.depth,
            });
            if !expanded.contains(&index) {
                continue;
            }
            for (property, axis) in CHANNELS {
                let keys = channels::keys(document, node, property, axis);
                if only_animated && keys.is_empty() {
                    continue;
                }
                let eases = channels::eases(document, node, property, axis);
                rows.push(TimelineRow {
                    kind: RowKind::Transform {
                        outline: index,
                        property,
                        axis,
                    },
                    visible: Vec::new(),
                    summary: Vec::new(),
                    inherited: Vec::new(),
                    animated: !keys.is_empty(),
                    keys,
                    eases,
                    on_key: false,
                    depth: outline_row.depth + 1,
                });
            }
            if let Some(content) = document.node(node).map(motioner_core::Node::content) {
                for field in content_fields(content) {
                    rows.push(TimelineRow {
                        kind: RowKind::Content {
                            outline: index,
                            field,
                        },
                        visible: Vec::new(),
                        summary: Vec::new(),
                        inherited: Vec::new(),
                        keys: Vec::new(),
                        eases: Vec::new(),
                        animated: false,
                        on_key: false,
                        depth: outline_row.depth + 1,
                    });
                }
            }
        }
        Self { rows }
    }
}

/// The content rows a node of this kind shows.
#[must_use]
pub fn content_fields(content: &NodeContent) -> Vec<ContentField> {
    match content {
        NodeContent::Group => Vec::new(),
        NodeContent::Rect { .. } => vec![
            ContentField::Width,
            ContentField::Height,
            ContentField::Radius,
            ContentField::Fill,
            ContentField::Stroke,
            ContentField::StrokeWidth,
        ],
        NodeContent::Circle { .. } => vec![
            ContentField::Radius,
            ContentField::Fill,
            ContentField::Stroke,
            ContentField::StrokeWidth,
        ],
        NodeContent::Text(_) => vec![
            ContentField::Text,
            ContentField::Size,
            ContentField::Color,
            ContentField::Align,
        ],
    }
}

fn summary_keys(document: &Document, node: NodeId) -> Vec<SummaryKey> {
    let mut counts: BTreeMap<Frame, u8> = BTreeMap::new();
    for (property, axis) in CHANNELS {
        for frame in channels::keys(document, node, property, axis) {
            *counts.entry(frame).or_insert(0) += 1;
        }
    }
    counts
        .into_iter()
        .map(|(frame, channels)| SummaryKey { frame, channels })
        .collect()
}

fn inherited_keys(document: &Document, node: NodeId) -> Vec<Frame> {
    let mut frames: BTreeSet<Frame> = BTreeSet::new();
    let mut current = document.node(node).and_then(motioner_core::Node::parent);
    while let Some(parent) = current {
        for (property, axis) in CHANNELS {
            frames.extend(channels::keys(document, parent, property, axis));
        }
        current = document.node(parent).and_then(motioner_core::Node::parent);
    }
    frames.into_iter().collect()
}

/// Whether `node` (and its ancestors) are drawn at `frame`.
#[must_use]
pub fn is_visible_at(document: &Document, node: NodeId, frame: Frame) -> bool {
    let mut current = Some(node);
    let mut opacity = 1.0;
    let mut scale = 1.0_f64;
    while let Some(id) = current {
        let Some(n) = document.node(id) else {
            return false;
        };
        if !n.is_visible() {
            return false;
        }
        let t = n.transform();
        opacity *= sample(&t.opacity, frame).clamp(0.0, 1.0);
        let s = motioner_eval::sample_vec2(&t.scale, frame);
        scale *= s.x.abs().max(s.y.abs());
        current = n.parent();
    }
    opacity > 0.0 && scale > 1e-9
}

fn visible_runs(document: &Document, node: NodeId, duration: Frame) -> Vec<VisibleRun> {
    let mut runs: Vec<VisibleRun> = Vec::new();
    for frame in 0..duration.max(0) {
        if !is_visible_at(document, node, frame) {
            continue;
        }
        match runs.last_mut() {
            Some(run) if run.to + 1 == frame => run.to = frame,
            _ => runs.push(VisibleRun {
                from: frame,
                to: frame,
            }),
        }
    }
    runs
}

/// One row's rectangles, shared by drawing and hit testing.
#[derive(Debug, Clone, PartialEq)]
pub struct RowRect {
    /// Row index in [`TimelineRows`].
    pub row: usize,
    /// The whole row (names column and track area).
    pub rect: Rect,
    /// Name/label area inside the names column.
    pub name: Rect,
    /// Stopwatch button (transform sub-rows).
    pub stopwatch: Option<Rect>,
    /// Value field (transform sub-rows and content sub-rows).
    pub field: Option<Rect>,
    /// Previous-key button.
    pub prev: Option<Rect>,
    /// Add/remove-key button.
    pub key: Option<Rect>,
    /// Next-key button.
    pub next: Option<Rect>,
    /// Chevron that toggles expansion (layer rows with properties).
    pub chevron: Option<Rect>,
}

/// The laid-out rows for one frame.
#[derive(Debug, Clone, PartialEq)]
pub struct RowsLayout {
    /// Rows top to bottom.
    pub rows: Vec<RowRect>,
    /// Total height in pixels.
    pub height: f32,
}

/// Lays the rows out inside the timeline's names/tracks area.
#[must_use]
pub fn layout(rows: &TimelineRows, layout: &Layout, scroll: f32) -> RowsLayout {
    let m = layout.metrics;
    let names = layout.timeline.names;
    let tracks = layout.timeline.tracks;
    let full = Rect::new(
        names.x,
        tracks.y,
        layout.timeline.rect.right() - names.x,
        tracks.h,
    );
    let mut y = tracks.y - scroll;
    let mut out = Vec::with_capacity(rows.len());
    for (index, row) in rows.rows().iter().enumerate() {
        let h = if row.kind.is_layer() {
            m.px(LAYER_ROW)
        } else {
            m.px(SUB_ROW)
        };
        let rect = Rect::new(full.x, y, full.w, h);
        let chevron_side = m.px(12.0);
        let indent = m.px(size::INDENT) * row.depth as f32;
        let chevron = row.kind.is_layer().then(|| {
            Rect::new(
                names.x + m.px(size::PAD) + indent,
                (rect.center_y() - chevron_side * 0.5).round(),
                chevron_side,
                chevron_side,
            )
        });
        // Layer rows leave room for the kind icon; sub-rows start right
        // after the chevron/stopwatch column.
        let name_x = if row.kind.is_layer() {
            names.x + m.px(size::PAD) + indent + chevron_side + m.px(16.0) + m.px(size::GRID * 2.0)
        } else {
            names.x + m.px(size::PAD) + indent + m.px(14.0) + m.px(size::GRID * 2.0)
        };
        let name = Rect::new(
            name_x,
            rect.y,
            (names.right() - m.px(size::PAD) - name_x).max(0.0),
            rect.h,
        );
        let (stopwatch, field, prev, key, next) = if row.kind.is_layer() {
            (None, None, None, None, None)
        } else {
            let side = m.px(14.0);
            let stopwatch = Rect::new(
                names.x + m.px(size::PAD) + indent,
                (rect.center_y() - side * 0.5).round(),
                side,
                side,
            );
            // Buttons sit at the right edge of the names column.
            let right = names.right() - m.px(size::PAD);
            let gap = m.px(size::GRID);
            let next = Rect::new(right - side, rect.y + (h - side) * 0.5, side, side);
            let key = Rect::new(next.x - side - gap, next.y, side, side);
            let prev = Rect::new(key.x - side - gap, next.y, side, side);
            // The label gets a fixed column, then the scrub field fills what
            // is left before the key buttons.
            let label_w = m.px(56.0);
            let field_x = name_x + label_w;
            let field_w = (prev.x - gap - field_x).max(0.0);
            let field = Rect::new(
                field_x,
                (rect.center_y() - m.px(18.0) * 0.5).round(),
                field_w,
                m.px(18.0),
            );
            (
                Some(stopwatch),
                Some(field),
                Some(prev),
                Some(key),
                Some(next),
            )
        };
        out.push(RowRect {
            row: index,
            rect,
            name,
            stopwatch,
            field,
            prev,
            key,
            next,
            chevron,
        });
        y += h;
    }
    let height = (y - (tracks.y - scroll)).max(0.0);
    RowsLayout { rows: out, height }
}

#[cfg(test)]
mod tests {
    use super::*;
    use motioner_core::{OverrideAction, OverrideEntry, OverrideSet};

    const HELLO: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/hello.motion"
    ));

    fn hello() -> Document {
        motioner_dsl::run(HELLO, &motioner_dsl::Limits::default()).unwrap()
    }

    fn rows_at(frame: Frame) -> Vec<(String, Vec<Frame>, Vec<VisibleRun>)> {
        let document = hello();
        let outline = crate::outline_rows(&document);
        let rows = TimelineRows::compute(&document, &outline, &BTreeSet::new(), false);
        rows.rows()
            .iter()
            .map(|row| {
                let name = outline[row.kind.outline()].name.clone();
                let keys = if row.kind.is_layer() {
                    row.summary.iter().map(|k| k.frame).collect()
                } else {
                    row.keys.clone()
                };
                let _ = frame;
                (name, keys, row.visible.clone())
            })
            .collect()
    }

    #[test]
    fn collapsed_timeline_shows_the_documented_visibility() {
        let document = hello();
        let outline = crate::outline_rows(&document);
        let rows = TimelineRows::compute(&document, &outline, &BTreeSet::new(), false);
        // Circle 1 keys scale at 0.2 s with value 0, so it is invisible on
        // frames 0..=12 and appears at 13.
        let circle1 = rows.layer_row(1).unwrap();
        let visible = &rows.rows()[circle1].visible;
        assert_eq!(visible.first().unwrap().from, 13, "{visible:?}");
        // Group 1 (the first spinner) fades in from 1.0 s: invisible 0..=60.
        let rect_row = outline.iter().position(|r| r.name == "Rect 1").unwrap();
        let group_row = outline.iter().position(|r| r.name == "Group 1").unwrap();
        let group = rows.layer_row(group_row).unwrap();
        assert_eq!(rows.rows()[group].visible.first().unwrap().from, 61);
        let rect_layer = rows.layer_row(rect_row).unwrap();
        assert_eq!(
            rows.rows()[rect_layer].visible.first().unwrap().from,
            61,
            "the child inherits the parent fade-in"
        );
    }

    #[test]
    fn summary_keys_mark_shared_frames() {
        let document = hello();
        let rows = rows_at(0);
        let group_row = rows.iter().find(|(name, _, _)| name == "Group 1").unwrap();
        // Rotation at 1.0/3.5 s and opacity at 1.0/1.5 s share frame 60.
        let document_rows = {
            let outline = crate::outline_rows(&document);
            let model = TimelineRows::compute(&document, &outline, &BTreeSet::new(), false);
            let index = outline.iter().position(|r| r.name == "Group 1").unwrap();
            model.rows()[model.layer_row(index).unwrap()]
                .summary
                .clone()
        };
        assert_eq!(
            document_rows,
            vec![
                SummaryKey {
                    frame: 60,
                    channels: 2
                },
                SummaryKey {
                    frame: 90,
                    channels: 1
                },
                SummaryKey {
                    frame: 210,
                    channels: 1
                },
            ]
        );
        assert!(!group_row.1.is_empty());
    }

    #[test]
    fn children_show_inherited_parent_keys() {
        let document = hello();
        let outline = crate::outline_rows(&document);
        let rows = TimelineRows::compute(&document, &outline, &BTreeSet::new(), false);
        let rect_row = outline.iter().position(|r| r.name == "Rect 1").unwrap();
        let rect = rows.layer_row(rect_row).unwrap();
        assert_eq!(
            rows.rows()[rect].inherited,
            vec![60, 90, 210],
            "the parent's keys reach the child row"
        );
        // A root text layer inherits nothing.
        assert!(rows.rows()[rows.layer_row(0).unwrap()].inherited.is_empty());
    }

    #[test]
    fn expanding_adds_a_sub_row_per_property_and_content_field() {
        let document = hello();
        let outline = crate::outline_rows(&document);
        let group = outline.iter().position(|r| r.name == "Group 1").unwrap();
        let mut expanded = BTreeSet::new();
        expanded.insert(group);
        let rows = TimelineRows::compute(&document, &outline, &expanded, false);
        let start = rows.layer_row(group).unwrap();
        // One sub-row per channel: X and Y are separate for vector properties.
        let sub: Vec<_> = rows.rows()[start + 1..start + 9]
            .iter()
            .map(|r| r.kind)
            .collect();
        let x = Some(Axis::X);
        let y = Some(Axis::Y);
        assert_eq!(
            sub,
            vec![
                RowKind::Transform {
                    outline: group,
                    property: Property::Position,
                    axis: x
                },
                RowKind::Transform {
                    outline: group,
                    property: Property::Position,
                    axis: y
                },
                RowKind::Transform {
                    outline: group,
                    property: Property::Scale,
                    axis: x
                },
                RowKind::Transform {
                    outline: group,
                    property: Property::Scale,
                    axis: y
                },
                RowKind::Transform {
                    outline: group,
                    property: Property::Rotation,
                    axis: None
                },
                RowKind::Transform {
                    outline: group,
                    property: Property::Anchor,
                    axis: x
                },
                RowKind::Transform {
                    outline: group,
                    property: Property::Anchor,
                    axis: y
                },
                RowKind::Transform {
                    outline: group,
                    property: Property::Opacity,
                    axis: None
                },
            ]
        );
        // The next row is the child layer (a group has no content rows).
        assert_eq!(
            rows.rows()[start + 9].kind,
            RowKind::Layer { outline: group + 1 }
        );

        let rect = outline.iter().position(|r| r.name == "Rect 1").unwrap();
        expanded.insert(rect);
        let rows = TimelineRows::compute(&document, &outline, &expanded, false);
        let start = rows.layer_row(rect).unwrap();
        let content: Vec<_> = rows.rows()[start + 9..start + 15]
            .iter()
            .filter_map(|r| match r.kind {
                RowKind::Content { field, .. } => Some(field),
                _ => None,
            })
            .collect();
        assert_eq!(
            content.len(),
            6,
            "width, height, radius, fill, stroke, width"
        );
    }

    #[test]
    fn only_animated_hides_resting_channels() {
        let document = hello();
        let outline = crate::outline_rows(&document);
        let title = 0;
        let mut expanded = BTreeSet::new();
        expanded.insert(title);
        let all = TimelineRows::compute(&document, &outline, &expanded, false);
        let animated = TimelineRows::compute(&document, &outline, &expanded, true);
        assert!(all.len() > animated.len());
        assert!(animated.rows().iter().any(|r| matches!(
            r.kind,
            RowKind::Transform {
                property: Property::Opacity,
                ..
            }
        )));
        assert!(!animated.rows().iter().any(|r| matches!(
            r.kind,
            RowKind::Transform {
                property: Property::Anchor,
                ..
            }
        )));
    }

    #[test]
    fn explicit_visibility_and_overrides_change_the_strips() {
        let document = hello();
        let outline = crate::outline_rows(&document);
        let title = document.roots()[0];
        let mut edited = document.clone();
        edited.set_visible(title, false).unwrap();
        let mut set = OverrideSet::new();
        set.insert(OverrideEntry::new(
            document.node(title).unwrap().identity().as_str(),
            motioner_core::VISIBLE,
            OverrideAction::Visible(false),
        ));
        set.apply(&mut edited);
        let rows = TimelineRows::compute(&edited, &outline, &BTreeSet::new(), false);
        assert!(
            rows.rows()[0].visible.is_empty(),
            "hidden layers have no run"
        );
    }

    #[test]
    fn layout_maps_rows_to_rectangles_and_hits() {
        let document = hello();
        let outline = crate::outline_rows(&document);
        let group = outline.iter().position(|r| r.name == "Group 1").unwrap();
        let mut expanded = BTreeSet::new();
        expanded.insert(group);
        let model = TimelineRows::compute(&document, &outline, &expanded, false);
        let window = Layout::compute(1600.0, 900.0, 1.0, crate::layout::PanelSizes::default());
        let rows = layout(&model, &window, 0.0);
        assert_eq!(rows.rows.len(), model.len());
        for pair in rows.rows.windows(2) {
            assert!((pair[0].rect.bottom() - pair[1].rect.y).abs() < 1e-3);
        }
        let sub = &rows.rows[rows
            .rows
            .iter()
            .position(|r| !model.rows()[r.row].kind.is_layer())
            .unwrap()];
        assert!(sub.stopwatch.is_some() && sub.field.is_some());
        assert!(sub.prev.unwrap().right() <= sub.key.unwrap().x + 1e-3);
        assert!(sub.key.unwrap().right() <= sub.next.unwrap().x + 1e-3);
        // Scrolling shifts every row by the same amount.
        let scrolled = layout(&model, &window, 24.0);
        assert!((scrolled.rows[0].rect.y - (rows.rows[0].rect.y - 24.0)).abs() < 1e-3);
    }
}

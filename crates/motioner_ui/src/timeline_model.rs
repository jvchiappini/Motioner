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

use motioner_core::{
    AudioContent, Axis, ContentField, Document, Ease, Frame, FrameRate, NodeContent, NodeId,
    Property,
};
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
    /// Cached display label (transform and content rows only).
    ///
    /// Labels never change with the frame, so they are built when the rows are
    /// rebuilt instead of formatting a `String` on every draw.
    pub label: String,
    /// Cached override/selection channel name (transform and content rows).
    pub channel: String,
    /// The node this row belongs to (cached so value formatting needs no
    /// outline lookup).
    pub node: NodeId,
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
        let mut rows = Vec::with_capacity(outline.len());
        for (index, outline_row) in outline.iter().enumerate() {
            if !outline_row.visible {
                continue;
            }
            let node = outline_row.node;
            rows.push(TimelineRow {
                kind: RowKind::Layer { outline: index },
                visible: outline_row.visible_runs.clone(),
                summary: summary_keys(document, node),
                inherited: inherited_keys(document, node),
                keys: Vec::new(),
                eases: Vec::new(),
                animated: false,
                on_key: false,
                depth: outline_row.depth,
                label: String::new(),
                channel: String::new(),
                node,
            });
            if !expanded.contains(&index) {
                continue;
            }
            let properties = document
                .node(node)
                .map(|node| crate::properties::PropertySet::of(node.content()))
                .unwrap_or_default();
            if properties.has_visual() {
                for (property, axis) in CHANNELS {
                    let channel = channels::Channel::Transform(property, axis);
                    let keys = channels::keys(document, node, channel);
                    if only_animated && keys.is_empty() {
                        continue;
                    }
                    let eases = channels::eases(document, node, channel);
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
                        label: crate::labels::transform_label(property, axis),
                        channel: channels::channel_name(property, axis),
                        node,
                    });
                }
            }
            if let Some(content) = document.node(node).map(motioner_core::Node::content) {
                for field in content_fields(content) {
                    let channel = channels::Channel::Content(field);
                    let keys = if field.is_animatable() {
                        channels::keys(document, node, channel)
                    } else {
                        Vec::new()
                    };
                    if only_animated && keys.is_empty() {
                        continue;
                    }
                    let eases = if field.is_animatable() {
                        channels::eases(document, node, channel)
                    } else {
                        Vec::new()
                    };
                    rows.push(TimelineRow {
                        kind: RowKind::Content {
                            outline: index,
                            field,
                        },
                        visible: Vec::new(),
                        summary: Vec::new(),
                        inherited: Vec::new(),
                        animated: !keys.is_empty(),
                        keys,
                        eases,
                        on_key: false,
                        depth: outline_row.depth + 1,
                        label: crate::labels::content_label(field),
                        channel: crate::labels::content_label(field),
                        node,
                    });
                }
            }
        }
        Self { rows }
    }
}

impl TimelineRows {
    /// Writes every row's value text into `out`, reusing its `String`s.
    ///
    /// The draw loop reads the cached strings, so a scrub or a playback frame
    /// does not format a `String` per visible sub-row. `out` only grows once;
    /// after that every write reuses the existing capacity.
    pub fn write_values(&self, document: &Document, frame: Frame, out: &mut Vec<String>) {
        if out.len() < self.rows.len() {
            out.resize_with(self.rows.len(), String::new);
        }
        for (index, row) in self.rows.iter().enumerate() {
            let text = &mut out[index];
            text.clear();
            match row.kind {
                RowKind::Layer { .. } => {}
                RowKind::Transform { property, axis, .. } => {
                    write_value_text(text, document, row.node, property, axis, frame);
                }
                RowKind::Content { field, .. } => {
                    write_content_text(text, document, row.node, field, frame);
                }
            }
        }
    }
}

/// Writes the value text of one transform channel.
pub fn write_value_text(
    out: &mut String,
    document: &Document,
    node: NodeId,
    property: Property,
    axis: Option<Axis>,
    frame: Frame,
) {
    let Some(node) = document.node(node) else {
        return;
    };
    let t = node.transform();
    let scalar = |value: &motioner_core::Animatable<f64>| sample(value, frame);
    match (property, axis) {
        (Property::Position, Some(axis)) => write_round(out, scalar(t.position.axis(axis))),
        (Property::Scale, Some(axis)) => {
            let value = scalar(t.scale.axis(axis)) * 100.0;
            write_round(out, value);
            out.push('%');
        }
        (Property::Anchor, Some(axis)) => write_round(out, scalar(t.anchor.axis(axis))),
        (Property::Position, None) => {
            let v = motioner_eval::sample_vec2(&t.position, frame);
            write_round(out, v.x);
            out.push_str(", ");
            write_round(out, v.y);
        }
        (Property::Scale, None) => {
            let v = motioner_eval::sample_vec2(&t.scale, frame);
            write_round(out, v.x * 100.0);
            out.push_str("%, ");
            write_round(out, v.y * 100.0);
            out.push('%');
        }
        (Property::Anchor, None) => {
            let v = motioner_eval::sample_vec2(&t.anchor, frame);
            write_round(out, v.x);
            out.push_str(", ");
            write_round(out, v.y);
        }
        (Property::Rotation, _) => {
            write_round(out, scalar(&t.rotation));
            out.push('\u{00b0}');
        }
        (Property::Opacity, _) => {
            write_round(out, sample(&t.opacity, frame) * 100.0);
            out.push('%');
        }
    }
}

/// Writes the value text of one content field.
pub fn write_content_text(
    out: &mut String,
    document: &Document,
    node: NodeId,
    field: ContentField,
    frame: Frame,
) {
    use crate::editor::channels::{sampled_colour, sampled_number, SampledColour};
    if let Some(motioner_core::ContentValue::Gradient(Some(gradient))) = document
        .node(node)
        .and_then(|node| node.content().static_value(field))
    {
        out.push_str(gradient.kind().name());
        out.push_str(" gradient");
        return;
    }
    match field {
        ContentField::Fill | ContentField::Stroke => {
            match sampled_colour(document, node, field, frame) {
                SampledColour::Colour(colour) => write_hex(out, colour),
                SampledColour::Absent => out.push_str("none"),
                SampledColour::NotColour => out.push_str("n/a"),
            }
        }
        ContentField::FillStopColor(_) | ContentField::StrokeStopColor(_) => {
            match sampled_colour(document, node, field, frame) {
                SampledColour::Colour(colour) => write_hex(out, colour),
                _ => out.push_str("n/a"),
            }
        }
        ContentField::FillStopOffset(_)
        | ContentField::StrokeStopOffset(_)
        | ContentField::StrokeWidth
        | ContentField::Width
        | ContentField::Height
        | ContentField::Radius
        | ContentField::Size
        | ContentField::Weight
        | ContentField::Tracking
        | ContentField::LineHeight
        | ContentField::TrimStart
        | ContentField::TrimEnd
        | ContentField::TrimOffset
        | ContentField::Dash
        | ContentField::DashOffset
        | ContentField::InPoint
        | ContentField::OutPoint
        | ContentField::Gain
        | ContentField::Pan
        | ContentField::Start
        | ContentField::FadeIn
        | ContentField::FadeOut
        | ContentField::Speed => {
            let value = sampled_number(document, node, field, frame).or_else(|| {
                match document
                    .node(node)
                    .and_then(|node| node.content().static_value(field))
                {
                    Some(motioner_core::ContentValue::Number(value)) => Some(value),
                    _ => None,
                }
            });
            if let Some(value) = value {
                write_round(out, value);
            } else {
                out.push_str("n/a");
            }
        }
        ContentField::Color => {
            if let SampledColour::Colour(colour) = sampled_colour(document, node, field, frame) {
                write_hex(out, colour);
            }
        }
        ContentField::Text
        | ContentField::Align
        | ContentField::Cap
        | ContentField::Join
        | ContentField::Source
        | ContentField::Fit => {
            match document
                .node(node)
                .and_then(|node| node.content().static_value(field))
            {
                Some(motioner_core::ContentValue::Text(text)) => out.push_str(&text),
                Some(motioner_core::ContentValue::Align(align)) => {
                    out.push_str(match align {
                        motioner_core::TextAlign::Left => "left",
                        motioner_core::TextAlign::Center => "center",
                        motioner_core::TextAlign::Right => "right",
                    });
                }
                Some(motioner_core::ContentValue::Cap(cap)) => out.push_str(cap.name()),
                Some(motioner_core::ContentValue::Join(join)) => out.push_str(join.name()),
                Some(motioner_core::ContentValue::Fit(fit)) => out.push_str(fit.name()),
                _ => out.push_str("n/a"),
            }
        }
    }
}

/// Writes a rounded number exactly like the inspector (`0`, `1.5`).
pub fn write_round(out: &mut String, value: f64) {
    use std::fmt::Write as _;
    let value = (value * 10.0).round() / 10.0;
    let value = if value == 0.0 { 0.0 } else { value };
    if value.fract().abs() < 1e-9 {
        let _ = write!(out, "{value:.0}");
    } else {
        let _ = write!(out, "{value:.1}");
    }
}

/// Writes `#RRGGBB` or `#RRGGBBAA`, matching `Color::to_hex`.
pub fn write_hex(out: &mut String, color: motioner_core::Color) {
    use std::fmt::Write as _;
    if color.a == 255 {
        let _ = write!(out, "#{:02X}{:02X}{:02X}", color.r, color.g, color.b);
    } else {
        let _ = write!(
            out,
            "#{:02X}{:02X}{:02X}{:02X}",
            color.r, color.g, color.b, color.a
        );
    }
}

/// The content rows a node of this kind shows.
#[must_use]
pub fn content_fields(content: &NodeContent) -> Vec<ContentField> {
    let mut out = Vec::new();
    for field in base_fields(content) {
        match field {
            ContentField::Fill | ContentField::Stroke => {
                let slot = if field == ContentField::Fill {
                    motioner_core::PaintSlot::Fill
                } else {
                    motioner_core::PaintSlot::Stroke
                };
                if content.gradient_stop_count(slot) > 0 {
                    push_gradient_rows(&mut out, content, slot);
                } else {
                    out.push(field);
                }
            }
            ContentField::Color => {
                out.push(field);
                if content.gradient_stop_count(motioner_core::PaintSlot::Fill) > 0 {
                    push_gradient_rows(&mut out, content, motioner_core::PaintSlot::Fill);
                }
            }
            _ => out.push(field),
        }
    }
    out
}

/// Adds `fill_stop<n>_offset` / `fill_stop<n>_color` rows for one gradient.
fn push_gradient_rows(
    fields: &mut Vec<ContentField>,
    content: &NodeContent,
    slot: motioner_core::PaintSlot,
) {
    for index in 0..content.gradient_stop_count(slot) {
        if let Ok(index) = u8::try_from(index) {
            fields.push(ContentField::stop_field(
                slot,
                motioner_core::StopPart::Offset,
                index,
            ));
            fields.push(ContentField::stop_field(
                slot,
                motioner_core::StopPart::Color,
                index,
            ));
        }
    }
}

fn base_fields(content: &NodeContent) -> Vec<ContentField> {
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
        NodeContent::Path { .. } => vec![
            ContentField::Fill,
            ContentField::Stroke,
            ContentField::StrokeWidth,
            ContentField::TrimStart,
            ContentField::TrimEnd,
            ContentField::TrimOffset,
            ContentField::Dash,
            ContentField::DashOffset,
            ContentField::Cap,
            ContentField::Join,
        ],
        NodeContent::Text(_) => vec![
            ContentField::Text,
            ContentField::Size,
            ContentField::Color,
            ContentField::Align,
        ],
        NodeContent::Image(_) => vec![
            ContentField::Width,
            ContentField::Height,
            ContentField::Source,
            ContentField::Fit,
        ],
        NodeContent::Audio(_) => vec![
            ContentField::Gain,
            ContentField::Pan,
            ContentField::FadeIn,
            ContentField::FadeOut,
        ],
        NodeContent::Video(_) => vec![
            ContentField::Width,
            ContentField::Height,
            ContentField::Source,
            ContentField::Fit,
            ContentField::Speed,
            ContentField::Start,
            ContentField::InPoint,
            ContentField::OutPoint,
            ContentField::Gain,
        ],
    }
}

fn summary_keys(document: &Document, node: NodeId) -> Vec<SummaryKey> {
    let mut counts: BTreeMap<Frame, u8> = BTreeMap::new();
    for channel in channels::node_channels(document, node) {
        for frame in channels::keys(document, node, channel) {
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
            frames.extend(channels::keys(
                document,
                parent,
                channels::Channel::Transform(property, axis),
            ));
        }
        current = document.node(parent).and_then(motioner_core::Node::parent);
    }
    frames.into_iter().collect()
}

/// The clip range of an audio layer, as half-open composition frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioClipRange {
    /// First frame of the clip.
    pub start: Frame,
    /// One past the last frame of the clip.
    pub end: Frame,
}

/// The clip range of an audio layer.
///
/// `asset_duration` is the decoded file length; when it is `None` the
/// composition length is used for a clip without `out_point`, so a timeline
/// still shows a bounded bar.
#[must_use]
pub fn audio_clip_range(
    audio: &AudioContent,
    rate: FrameRate,
    asset_duration: Option<f64>,
    composition_duration: Frame,
) -> AudioClipRange {
    let fps = rate.as_f64();
    let seconds = composition_duration as f64 / fps;
    let in_point = audio.in_point.value().max(0.0);
    let out_point = audio.out_point.value();
    let length = if out_point > in_point {
        out_point - in_point
    } else {
        (asset_duration.unwrap_or(seconds) - in_point).max(0.0)
    };
    let start = audio.start.max(0);
    #[allow(clippy::cast_possible_truncation)]
    let frames = (length * fps).round() as i64;
    let end = start.saturating_add(frames).min(composition_duration);
    AudioClipRange {
        start,
        end: end.max(start.saturating_add(1)),
    }
}

/// The clip range of a video layer, as half-open composition frames.
#[must_use]
pub fn video_clip_range(
    video: &motioner_core::VideoContent,
    rate: FrameRate,
    asset_duration: Option<f64>,
    composition_duration: Frame,
) -> AudioClipRange {
    let fps = rate.as_f64();
    let seconds = composition_duration as f64 / fps;
    let in_point = video.in_point.value().max(0.0);
    let out_point = video.out_point.value();
    let source_length = if out_point > in_point {
        out_point - in_point
    } else {
        (asset_duration.unwrap_or(seconds) - in_point).max(0.0)
    };
    let length = source_length / video.speed.max(0.001);
    let start = video.start.max(0);
    #[allow(clippy::cast_possible_truncation)]
    let frames = (length * fps).round() as i64;
    let end = start.saturating_add(frames).min(composition_duration);
    AudioClipRange {
        start,
        end: end.max(start.saturating_add(1)),
    }
}

/// Whether `node` (and its ancestors) are drawn at `frame`.
#[must_use]
pub fn is_visible_at(document: &Document, node: NodeId, frame: Frame) -> bool {
    if let Some(content) = document.node(node).map(motioner_core::Node::content) {
        match content {
            NodeContent::Audio(audio) => {
                let range = audio_clip_range(
                    audio,
                    document.composition().frame_rate(),
                    None,
                    document.composition().duration(),
                );
                if frame < range.start || frame >= range.end {
                    return false;
                }
            }
            NodeContent::Video(video)
                if video
                    .source_frame(frame, document.composition().frame_rate())
                    .is_none() =>
            {
                return false;
            }
            _ => {}
        }
    }
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

/// The frame runs where `node` and its ancestors are drawn.
#[must_use]
pub fn visible_runs(document: &Document, node: NodeId, duration: Frame) -> Vec<VisibleRun> {
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

impl RowsLayout {
    /// Empties the layout, keeping its allocation (the per-frame reuse that
    /// makes scrolling 1000 layers allocation-free).
    pub fn clear(&mut self) {
        self.rows.clear();
        self.height = 0.0;
    }
}

/// Lays the rows out inside the timeline's names/tracks area.
#[cfg(test)]
#[must_use]
pub fn layout(rows: &TimelineRows, layout: &Layout, scroll: f32) -> RowsLayout {
    let mut out = RowsLayout {
        rows: Vec::with_capacity(rows.len().min(64)),
        height: 0.0,
    };
    layout_into(rows, layout, scroll, &mut out);
    out
}

/// [`layout`] into a reusable buffer, emitting only the rows that intersect
/// the visible tracks (virtualised rows).
pub fn layout_into(rows: &TimelineRows, layout: &Layout, scroll: f32, out: &mut RowsLayout) {
    out.clear();
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
    for (index, row) in rows.rows().iter().enumerate() {
        let h = if row.kind.is_layer() {
            m.px(LAYER_ROW)
        } else {
            m.px(SUB_ROW)
        };
        let rect = Rect::new(full.x, y, full.w, h);
        y += h;
        if rect.bottom() < tracks.y || rect.y > tracks.bottom() {
            continue;
        }
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
        out.rows.push(RowRect {
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
    }
    out.height = (y - (tracks.y - scroll)).max(0.0);
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
        let rect_row = outline
            .iter()
            .position(|r| r.kind == crate::outline::NodeKind::Rect)
            .unwrap();
        let group_row = outline
            .iter()
            .position(|r| r.kind == crate::outline::NodeKind::Group)
            .unwrap();
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
        let group_row = rows.iter().find(|(name, _, _)| name == "holder 1").unwrap();
        // Rotation at 1.0/3.5 s and opacity at 1.0/1.5 s share frame 60.
        let document_rows = {
            let outline = crate::outline_rows(&document);
            let model = TimelineRows::compute(&document, &outline, &BTreeSet::new(), false);
            let index = outline
                .iter()
                .position(|r| r.kind == crate::outline::NodeKind::Group)
                .unwrap();
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
        let rect_row = outline
            .iter()
            .position(|r| r.kind == crate::outline::NodeKind::Rect)
            .unwrap();
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
        let group = outline
            .iter()
            .position(|r| r.kind == crate::outline::NodeKind::Group)
            .unwrap();
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

        let rect = outline
            .iter()
            .position(|r| r.kind == crate::outline::NodeKind::Rect)
            .unwrap();
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
        let outline = crate::outline_rows(&edited);
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
        let group = outline
            .iter()
            .position(|r| r.kind == crate::outline::NodeKind::Group)
            .unwrap();
        let mut expanded = BTreeSet::new();
        expanded.insert(group);
        let model = TimelineRows::compute(&document, &outline, &expanded, false);
        let window = Layout::compute(1600.0, 900.0, 1.0, crate::layout::PanelSizes::default());
        let rows = layout(&model, &window, 0.0);
        // Rows are virtualised: only what intersects the track area is laid out.
        assert!(!rows.rows.is_empty());
        assert!(rows.rows.len() <= model.len());
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

    #[test]
    fn content_rows_carry_keys_and_layer_summary() {
        let source = "comp(name=\"C\", width=100, height=100, duration=2.0)\n\
                      r = rect(10, 10, 0, fill=\"#FF0000\")\n\
                      r.key(\"fill\", 0.0, \"#FF0000\")\n\
                      r.key(\"fill\", 1.0, \"#00FF00\")\n\
                      r.key(\"width\", 1.5, 40)\n";
        let document = motioner_dsl::run(source, &motioner_dsl::Limits::default()).unwrap();
        let outline = crate::outline_rows(&document);
        let mut expanded = BTreeSet::new();
        expanded.insert(0);
        let rows = TimelineRows::compute(&document, &outline, &expanded, false);
        let fill = rows
            .rows()
            .iter()
            .find(|row| {
                row.kind
                    == RowKind::Content {
                        outline: 0,
                        field: ContentField::Fill,
                    }
            })
            .expect("a fill row");
        assert!(fill.animated);
        assert_eq!(fill.keys, vec![0, 60]);
        assert_eq!(fill.eases.len(), 2);
        let width = rows
            .rows()
            .iter()
            .find(|row| {
                row.kind
                    == RowKind::Content {
                        outline: 0,
                        field: ContentField::Width,
                    }
            })
            .expect("a width row");
        assert_eq!(width.keys, vec![90]);
        // The layer row summarises content keys too; frame 60 has two keys
        // (fill.y is one channel, and width keys at 90).
        let layer = &rows.rows()[0];
        assert_eq!(
            layer.summary,
            vec![
                SummaryKey {
                    frame: 0,
                    channels: 1
                },
                SummaryKey {
                    frame: 60,
                    channels: 1
                },
                SummaryKey {
                    frame: 90,
                    channels: 1
                },
            ]
        );
    }

    #[test]
    fn an_audio_clip_range_uses_start_and_the_trimmed_length() {
        let rate = FrameRate::new(60, 1).unwrap();
        let mut audio = AudioContent::new("tone.wav", 30);
        audio.in_point = motioner_core::Animatable::new(0.5);
        audio.out_point = motioner_core::Animatable::new(1.5);
        let range = audio_clip_range(&audio, rate, Some(3.0), 120);
        assert_eq!(range, AudioClipRange { start: 30, end: 90 });
        // No out point: the asset duration bounds the bar.
        audio.out_point = motioner_core::Animatable::new(0.0);
        let range = audio_clip_range(&audio, rate, Some(2.5), 120);
        assert_eq!(
            range,
            AudioClipRange {
                start: 30,
                end: 120
            }
        );
        // The bar is clamped to the composition.
        let range = audio_clip_range(&audio, rate, Some(5.0), 60);
        assert_eq!(range, AudioClipRange { start: 30, end: 60 });
    }

    #[test]
    fn expanded_audio_rows_are_gain_pan_and_fades_only() {
        let document = motioner_dsl::run(
            r#"
comp("A", 640, 360, 60, 1.0)
music = audio("tone.wav", in_point=0.5, out_point=1.5, gain=0.5, start=12,
              pan=-0.5, fade_in=0.2, fade_out=0.3)
music.id("music")
"#,
            &motioner_dsl::Limits::default(),
        )
        .unwrap();
        let outline = crate::outline_rows(&document);
        let mut expanded = BTreeSet::new();
        expanded.insert(0);
        let rows = TimelineRows::compute(&document, &outline, &expanded, false);
        let audio_rows: Vec<(&str, bool)> = rows
            .rows()
            .iter()
            .filter(|row| matches!(row.kind, RowKind::Content { .. }))
            .map(|row| (row.label.as_str(), row.animated))
            .collect();
        assert_eq!(
            audio_rows,
            vec![
                ("Gain", false),
                ("Pan", false),
                ("Fade in", false),
                ("Fade out", false)
            ]
        );
        assert!(
            !rows
                .rows()
                .iter()
                .any(|row| matches!(row.kind, RowKind::Transform { .. })),
            "audio has no transform rows"
        );
    }
}

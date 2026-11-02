//! Editable inspector model and layout.
//!
//! [`inspect_node`] samples the selected node's transform and content at the
//! current frame and turns them into [`Row`]s: one row per property, with the
//! fields the user can edit, the stopwatch state of animatable channels and
//! the keyframes of the channel at the playhead.
//!
//! [`layout`] turns a model into rectangles for drawing **and** hit testing, so
//! what is drawn is exactly what is clickable. Everything here is pure: no
//! input, no window, no GPU.

use motioner_core::{
    Color, Composition, ContentField, ContentValue, Document, Frame, NodeContent, NodeId,
    OverrideSet, Property, TextAlign, Transform,
};
use motioner_eval::sample;

use crate::layout::Rect;
use crate::text_edit::TextEdit;
use crate::theme::{size, Metrics};

/// Which axis of a vector property a field edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    /// The only component (rotation, opacity).
    Single,
    /// X of a pair.
    X,
    /// Y of a pair.
    Y,
}

impl Axis {
    /// The core axis of a component, for calls that take one.
    #[must_use]
    pub const fn core(self) -> motioner_core::Axis {
        match self {
            Self::X | Self::Single => motioner_core::Axis::X,
            Self::Y => motioner_core::Axis::Y,
        }
    }
}

/// What a field writes when it is edited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldTarget {
    /// A transform property (with its axis for pairs).
    Transform(Property, Axis),
    /// A content field.
    Content(ContentField),
    /// The text of a text layer.
    Text,
    /// The horizontal alignment of a text layer.
    Align,
    /// The display name of the layer.
    Name,
    /// Not editable (composition summary rows).
    None,
}

/// One numeric field of a row.
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    /// What an edit writes.
    pub target: FieldTarget,
    /// Short label inside the field ("X", "W", "").
    pub label: String,
    /// The property value (scale 1.0, opacity 0.0..=1.0).
    pub value: f64,
    /// The text shown and edited (percent for scale and opacity).
    pub text: String,
    /// Multiplier from value to display number (100 for percentages).
    pub factor: f64,
    /// Unit suffix shown in the field ("%", "°").
    pub suffix: &'static str,
}

impl Field {
    /// The value a parsed display number means.
    #[must_use]
    pub fn value_of(&self, shown: f64) -> f64 {
        shown / self.factor
    }
}

/// What an animatable channel drives: a transform property or a content
/// field (D-0028).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelTarget {
    /// A transform property, with its axis for vector properties.
    Transform(Property, Option<motioner_core::Axis>),
    /// A content field.
    Content(ContentField),
}

impl ChannelTarget {
    /// The override name of the channel (`pos.x`, `fill`).
    #[must_use]
    pub fn name(self) -> String {
        match self {
            Self::Transform(property, axis) => crate::editor::channel_name(property, axis),
            Self::Content(field) => field.name().clone(),
        }
    }
}

/// The state of one animatable channel.
///
/// A vector property has one channel per axis: X and Y are keyed, sampled and
/// shown independently, like After Effects' "separate dimensions".
#[derive(Debug, Clone, PartialEq)]
pub struct Channel {
    /// What the channel drives.
    pub target: ChannelTarget,
    /// Whether the channel has keyframes.
    pub keyed: bool,
    /// Whether the playhead sits exactly on a key.
    pub on_key: bool,
    /// Frames of the channel's keys, sorted.
    pub frames: Vec<Frame>,
}

/// What one row shows.
#[derive(Debug, Clone, PartialEq)]
pub enum RowValue {
    /// Two numeric fields side by side.
    Pair {
        /// Left field.
        left: Field,
        /// Right field.
        right: Field,
    },
    /// One numeric field.
    Single(Field),
    /// A colour swatch and its hex code.
    Color {
        /// What an edit writes.
        target: FieldTarget,
        /// The colour.
        color: Color,
        /// `#RRGGBB[AA]`.
        text: String,
    },
    /// A free text field.
    Text {
        /// What an edit writes.
        target: FieldTarget,
        /// The text.
        text: String,
    },
    /// The node's blend mode as a seven-tile segmented grid.
    Blend {
        /// What an edit writes (`blend`).
        target: FieldTarget,
        /// The current mode.
        mode: motioner_core::BlendMode,
    },
    /// A three-state segmented control.
    Align {
        /// What an edit writes.
        target: FieldTarget,
        /// The current alignment.
        align: TextAlign,
    },
    /// The ease of the selected keys: one tile per ease with a curve preview.
    Ease {
        /// The ease shared by every selected key, `None` when they differ.
        current: Option<motioner_core::Ease>,
    },
    /// Read-only text.
    ReadOnly(String),
}

/// One inspector row.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    /// Section this row belongs to; the panel draws a header when it changes.
    pub section: &'static str,
    /// Row label.
    pub label: String,
    /// The value(s).
    pub value: RowValue,
    /// Animatable channel, when the row has a stopwatch and diamonds.
    pub channel: Option<Channel>,
}

/// Everything the inspector shows for a selection.
#[derive(Debug, Clone, PartialEq)]
pub struct Model {
    /// Header data (a selected layer) or `None` for the composition.
    pub header: Option<Header>,
    /// Rows in display order.
    pub rows: Vec<Row>,
}

/// The selected layer's header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    /// Display name (the `name` override when present).
    pub name: String,
    /// "Text layer", "Group layer", ...
    pub kind: &'static str,
    /// Stable identity key.
    pub identity: String,
    /// Whether the identity is explicit (`.id()`) or a provenance key.
    pub explicit: bool,
}

/// Formats a colour as `#RRGGBB`, plus alpha when not opaque.
#[must_use]
pub fn hex(color: Color) -> String {
    color.to_hex()
}

fn display(value: f64, factor: f64) -> String {
    let shown = (value * factor * 10.0).round() / 10.0;
    let shown = if shown == 0.0 { 0.0 } else { shown };
    if shown.fract().abs() < 1e-9 {
        format!("{shown:.0}")
    } else {
        format!("{shown:.1}")
    }
}

fn field(
    target: FieldTarget,
    label: &'static str,
    value: f64,
    factor: f64,
    suffix: &'static str,
) -> Field {
    Field {
        target,
        label: label.to_owned(),
        value,
        text: display(value, factor),
        factor,
        suffix,
    }
}

fn channel(
    t: &Transform,
    property: Property,
    axis: Option<motioner_core::Axis>,
    frame: Frame,
) -> Channel {
    let frames = crate::editor::channels::transform_keys(t, property, axis);
    Channel {
        target: ChannelTarget::Transform(property, axis),
        keyed: !frames.is_empty(),
        on_key: frames.contains(&frame),
        frames,
    }
}

/// The inspector channel of a content field.
fn content_channel(
    document: &Document,
    node: NodeId,
    field: ContentField,
    frame: Frame,
) -> Channel {
    let frames =
        crate::editor::channels::keys(document, node, crate::editor::Channel::Content(field));
    Channel {
        target: ChannelTarget::Content(field),
        keyed: !frames.is_empty(),
        on_key: frames.contains(&frame),
        frames,
    }
}
fn row(
    section: &'static str,
    label: impl Into<String>,
    value: RowValue,
    channel: Option<Channel>,
) -> Row {
    Row {
        section,
        label: label.into(),
        value,
        channel,
    }
}

fn transform_rows(t: &Transform, frame: Frame) -> Vec<Row> {
    let mut rows = position_scale_rows(t, frame);
    let anchor = motioner_eval::sample_vec2(&t.anchor, frame);
    let rotation = sample(&t.rotation, frame);
    let opacity = sample(&t.opacity, frame).clamp(0.0, 1.0);
    rows.push(row(
        "Transform",
        "Rotation",
        RowValue::Single(field(
            FieldTarget::Transform(Property::Rotation, Axis::Single),
            "",
            rotation,
            1.0,
            "\u{00b0}",
        )),
        Some(channel(t, Property::Rotation, None, frame)),
    ));
    rows.push(row(
        "Transform",
        "Anchor X",
        RowValue::Single(field(
            FieldTarget::Transform(Property::Anchor, Axis::X),
            "X",
            anchor.x,
            1.0,
            "",
        )),
        Some(channel(t, Property::Anchor, Some(Axis::X.core()), frame)),
    ));
    rows.push(row(
        "Transform",
        "Anchor Y",
        RowValue::Single(field(
            FieldTarget::Transform(Property::Anchor, Axis::Y),
            "Y",
            anchor.y,
            1.0,
            "",
        )),
        Some(channel(t, Property::Anchor, Some(Axis::Y.core()), frame)),
    ));
    rows.push(row(
        "Transform",
        "Opacity",
        RowValue::Single(field(
            FieldTarget::Transform(Property::Opacity, Axis::Single),
            "",
            opacity,
            100.0,
            "%",
        )),
        Some(channel(t, Property::Opacity, None, frame)),
    ));
    rows
}

/// Position and scale, one row per axis.
fn position_scale_rows(t: &Transform, frame: Frame) -> Vec<Row> {
    let position = motioner_eval::sample_vec2(&t.position, frame);
    let scale = motioner_eval::sample_vec2(&t.scale, frame);
    vec![
        row(
            "Transform",
            "Pos X",
            RowValue::Single(field(
                FieldTarget::Transform(Property::Position, Axis::X),
                "X",
                position.x,
                1.0,
                "",
            )),
            Some(channel(t, Property::Position, Some(Axis::X.core()), frame)),
        ),
        row(
            "Transform",
            "Pos Y",
            RowValue::Single(field(
                FieldTarget::Transform(Property::Position, Axis::Y),
                "Y",
                position.y,
                1.0,
                "",
            )),
            Some(channel(t, Property::Position, Some(Axis::Y.core()), frame)),
        ),
        row(
            "Transform",
            "Scale X",
            RowValue::Single(field(
                FieldTarget::Transform(Property::Scale, Axis::X),
                "W",
                scale.x,
                100.0,
                "%",
            )),
            Some(channel(t, Property::Scale, Some(Axis::X.core()), frame)),
        ),
        row(
            "Transform",
            "Scale Y",
            RowValue::Single(field(
                FieldTarget::Transform(Property::Scale, Axis::Y),
                "H",
                scale.y,
                100.0,
                "%",
            )),
            Some(channel(t, Property::Scale, Some(Axis::Y.core()), frame)),
        ),
    ]
}
fn content_rows(
    document: &Document,
    node: NodeId,
    content: &NodeContent,
    frame: Frame,
) -> Vec<Row> {
    match content {
        NodeContent::Group => Vec::new(),
        NodeContent::Rect { .. } => rect_rows(document, node, frame),
        NodeContent::Circle { .. } => circle_rows(document, node, frame),
        NodeContent::Path { geometry, .. } => path_rows(document, node, geometry, frame),
        NodeContent::Text(text) => text_rows(document, node, text, frame),
    }
}

fn path_rows(
    document: &Document,
    node: NodeId,
    geometry: &motioner_core::PathGeometry,
    frame: Frame,
) -> Vec<Row> {
    let mut rows = vec![row(
        "Path",
        "Points",
        RowValue::ReadOnly(format!(
            "{} pts{}",
            geometry.points().len(),
            if geometry.closed() { ", closed" } else { "" }
        )),
        None,
    )];
    for (content_field, label) in [
        (ContentField::TrimStart, "Trim Start"),
        (ContentField::TrimEnd, "Trim End"),
        (ContentField::TrimOffset, "Trim Offset"),
        (ContentField::Dash, "Dash"),
        (ContentField::DashOffset, "Dash Offset"),
    ] {
        rows.push(row(
            "Path",
            label,
            RowValue::Single(field(
                FieldTarget::Content(content_field),
                "",
                number_or_zero(document, node, content_field, frame),
                1.0,
                "",
            )),
            Some(content_channel(document, node, content_field, frame)),
        ));
    }
    rows.push(row(
        "Path",
        "Cap",
        RowValue::Text {
            target: FieldTarget::Content(ContentField::Cap),
            text: geometry.cap().name().to_owned(),
        },
        None,
    ));
    rows.push(row(
        "Path",
        "Join",
        RowValue::Text {
            target: FieldTarget::Content(ContentField::Join),
            text: geometry.join().name().to_owned(),
        },
        None,
    ));
    rows.extend(paint_rows(document, node, frame, true));
    rows
}

fn number_or_zero(document: &Document, node: NodeId, field: ContentField, frame: Frame) -> f64 {
    crate::editor::channels::sampled_number(document, node, field, frame).unwrap_or(0.0)
}

fn colour_of(
    document: &Document,
    node: NodeId,
    field: ContentField,
    frame: Frame,
) -> Option<Color> {
    match crate::editor::channels::sampled_colour(document, node, field, frame) {
        crate::editor::channels::SampledColour::Colour(color) => Some(color),
        _ => None,
    }
}

fn rect_rows(document: &Document, node: NodeId, frame: Frame) -> Vec<Row> {
    let width = number_or_zero(document, node, ContentField::Width, frame);
    let height = number_or_zero(document, node, ContentField::Height, frame);
    let radius = number_or_zero(document, node, ContentField::Radius, frame);
    let mut rows = vec![
        row(
            "Shape",
            "Size",
            RowValue::Pair {
                left: field(
                    FieldTarget::Content(ContentField::Width),
                    "W",
                    width,
                    1.0,
                    "",
                ),
                right: field(
                    FieldTarget::Content(ContentField::Height),
                    "H",
                    height,
                    1.0,
                    "",
                ),
            },
            None,
        ),
        row(
            "Shape",
            "Radius",
            RowValue::Single(field(
                FieldTarget::Content(ContentField::Radius),
                "",
                radius,
                1.0,
                "",
            )),
            Some(content_channel(document, node, ContentField::Radius, frame)),
        ),
    ];
    rows.extend(paint_rows(document, node, frame, true));
    rows
}

fn circle_rows(document: &Document, node: NodeId, frame: Frame) -> Vec<Row> {
    let radius = number_or_zero(document, node, ContentField::Radius, frame);
    let mut rows = vec![row(
        "Shape",
        "Radius",
        RowValue::Single(field(
            FieldTarget::Content(ContentField::Radius),
            "",
            radius,
            1.0,
            "",
        )),
        Some(content_channel(document, node, ContentField::Radius, frame)),
    )];
    rows.extend(paint_rows(document, node, frame, false));
    rows
}

fn text_rows(
    document: &Document,
    node: NodeId,
    text: &motioner_core::TextContent,
    frame: Frame,
) -> Vec<Row> {
    let mut rows = vec![
        row(
            "Text",
            "Text",
            RowValue::Text {
                target: FieldTarget::Text,
                text: text.text.clone(),
            },
            None,
        ),
        row(
            "Text",
            "Size",
            RowValue::Single(field(
                FieldTarget::Content(ContentField::Size),
                "",
                number_or_zero(document, node, ContentField::Size, frame),
                1.0,
                "",
            )),
            Some(content_channel(document, node, ContentField::Size, frame)),
        ),
        row(
            "Text",
            "Colour",
            RowValue::Color {
                target: FieldTarget::Content(ContentField::Color),
                color: colour_of(document, node, ContentField::Color, frame)
                    .unwrap_or(Color::WHITE),
                text: colour_of(document, node, ContentField::Color, frame)
                    .map_or_else(String::new, hex),
            },
            Some(content_channel(document, node, ContentField::Color, frame)),
        ),
        row(
            "Text",
            "Align",
            RowValue::Align {
                target: FieldTarget::Align,
                align: text.align,
            },
            None,
        ),
    ];
    if let Some(gradient) = &text.gradient {
        rows.extend(gradient_rows(
            document,
            node,
            frame,
            "Text",
            motioner_core::PaintSlot::Fill,
            gradient.len(),
        ));
    }
    rows
}

fn paint_rows(document: &Document, node: NodeId, frame: Frame, fill: bool) -> Vec<Row> {
    let mut rows = Vec::new();
    let content = document.node(node).map(motioner_core::Node::content);
    if fill {
        let stops = content.map_or(0, |content| {
            content.gradient_stop_count(motioner_core::PaintSlot::Fill)
        });
        if stops > 0 {
            rows.extend(gradient_rows(
                document,
                node,
                frame,
                "Fill",
                motioner_core::PaintSlot::Fill,
                stops,
            ));
        } else {
            rows.push(match colour_of(document, node, ContentField::Fill, frame) {
                Some(color) => row(
                    "Fill",
                    "Fill",
                    RowValue::Color {
                        target: FieldTarget::Content(ContentField::Fill),
                        color,
                        text: hex(color),
                    },
                    Some(content_channel(document, node, ContentField::Fill, frame)),
                ),
                None => row(
                    "Fill",
                    "Fill",
                    RowValue::ReadOnly("None".to_owned()),
                    Some(content_channel(document, node, ContentField::Fill, frame)),
                ),
            });
        }
    }
    let stroke_stops = content.map_or(0, |content| {
        content.gradient_stop_count(motioner_core::PaintSlot::Stroke)
    });
    if stroke_stops > 0 {
        rows.extend(gradient_rows(
            document,
            node,
            frame,
            "Stroke",
            motioner_core::PaintSlot::Stroke,
            stroke_stops,
        ));
    } else {
        rows.push(
            match colour_of(document, node, ContentField::Stroke, frame) {
                Some(color) => row(
                    "Fill",
                    "Stroke",
                    RowValue::Color {
                        target: FieldTarget::Content(ContentField::Stroke),
                        color,
                        text: hex(color),
                    },
                    Some(content_channel(document, node, ContentField::Stroke, frame)),
                ),
                None => row(
                    "Fill",
                    "Stroke",
                    RowValue::ReadOnly("None".to_owned()),
                    Some(content_channel(document, node, ContentField::Stroke, frame)),
                ),
            },
        );
    }
    rows.push(row(
        "Fill",
        "Width",
        RowValue::Single(field(
            FieldTarget::Content(ContentField::StrokeWidth),
            "",
            number_or_zero(document, node, ContentField::StrokeWidth, frame),
            1.0,
            "",
        )),
        Some(content_channel(
            document,
            node,
            ContentField::StrokeWidth,
            frame,
        )),
    ));
    rows
}

/// Rows for one gradient: a summary line and an offset/colour pair per stop
/// (D-0030). Every stop part is an ordinary content channel, so its row has
/// the same stopwatch and diamonds as `fill`.
fn gradient_rows(
    document: &Document,
    node: NodeId,
    frame: Frame,
    section: &'static str,
    slot: motioner_core::PaintSlot,
    stops: usize,
) -> Vec<Row> {
    let kind = document
        .node(node)
        .and_then(|node| {
            node.content().static_value(match slot {
                motioner_core::PaintSlot::Fill => ContentField::Fill,
                motioner_core::PaintSlot::Stroke => ContentField::Stroke,
            })
        })
        .and_then(|value| match value {
            ContentValue::Gradient(Some(gradient)) => Some(gradient.kind().name()),
            _ => None,
        })
        .unwrap_or("linear");
    let mut rows = vec![row(
        section,
        "Gradient",
        RowValue::ReadOnly(format!("{kind}, {stops} stops")),
        None,
    )];
    for index in 0..stops {
        let Ok(index) = u8::try_from(index) else {
            break;
        };
        let offset = ContentField::stop_field(slot, motioner_core::StopPart::Offset, index);
        let color = ContentField::stop_field(slot, motioner_core::StopPart::Color, index);
        rows.push(row(
            section,
            format!("Stop {index} Offset"),
            RowValue::Single(field(
                FieldTarget::Content(offset),
                "",
                number_or_zero(document, node, offset, frame),
                1.0,
                "",
            )),
            Some(content_channel(document, node, offset, frame)),
        ));
        let sampled = colour_of(document, node, color, frame);
        rows.push(row(
            section,
            format!("Stop {index} Colour"),
            RowValue::Color {
                target: FieldTarget::Content(color),
                color: sampled.unwrap_or(Color::WHITE),
                text: sampled.map_or_else(String::new, hex),
            },
            Some(content_channel(document, node, color, frame)),
        ));
    }
    rows
}

/// A one-line kind label for the header.
#[must_use]
pub const fn kind_label(content: &NodeContent) -> &'static str {
    match content {
        NodeContent::Group => "Group layer",
        NodeContent::Rect { .. } => "Rectangle layer",
        NodeContent::Circle { .. } => "Circle layer",
        NodeContent::Path { .. } => "Path layer",
        NodeContent::Text(_) => "Text layer",
    }
}

/// The display name of a node: its `name` override when present, otherwise a
/// kind label with a running ordinal computed by `fallback`.
fn display_name(
    document: &Document,
    node: NodeId,
    overrides: Option<&OverrideSet>,
) -> Option<String> {
    let n = document.node(node)?;
    if let Some(set) = overrides {
        if let Some(entry) = set.get(n.identity().as_str(), motioner_core::NAME, None) {
            if let motioner_core::OverrideAction::Set(motioner_core::OverrideValue::Text(text)) =
                &entry.action
            {
                return Some(text.clone());
            }
        }
    }
    Some(String::new())
}

/// The rows shown for `node` at `frame`, or `None` if the node is gone.
///
/// `fallback_name` is the outline's generated name ("Circle 3"); a `name`
/// override replaces it.
#[must_use]
pub fn inspect_node(
    document: &Document,
    node: NodeId,
    frame: Frame,
    overrides: Option<&OverrideSet>,
    fallback_name: &str,
) -> Option<Model> {
    let n = document.node(node)?;
    // Blend sits at the top: it is a whole-node property, and the segmented
    // grid needs the first screenful.
    let mut rows = vec![row(
        "Composite",
        "Blend",
        RowValue::Blend {
            target: FieldTarget::None,
            mode: n.blend(),
        },
        None,
    )];
    rows.extend(transform_rows(n.transform(), frame));
    rows.extend(content_rows(document, node, n.content(), frame));
    let name = display_name(document, node, overrides)
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| fallback_name.to_owned());
    let header = Header {
        name: name.clone(),
        kind: kind_label(n.content()),
        identity: n.identity().as_str().to_owned(),
        explicit: n.identity().is_explicit(),
    };
    rows.push(row(
        "Layer",
        "Name",
        RowValue::Text {
            target: FieldTarget::Name,
            text: name,
        },
        None,
    ));
    Some(Model {
        header: Some(header),
        rows,
    })
}

/// The rows shown for the composition when nothing is selected.
#[must_use]
pub fn inspect_comp(composition: &Composition, layers: usize) -> Model {
    let rate = composition.frame_rate();
    let frames = composition.duration();
    let fps = if rate.denominator() == 1 {
        rate.numerator().to_string()
    } else {
        format!("{:.3}", rate.as_f64())
    };
    let seconds = frames as f64 * f64::from(rate.denominator()) / f64::from(rate.numerator());
    let rows = vec![
        row(
            "Composition",
            "Name",
            RowValue::ReadOnly(composition.name().to_owned()),
            None,
        ),
        row(
            "Composition",
            "Size",
            RowValue::ReadOnly(format!(
                "{} × {}",
                composition.width(),
                composition.height()
            )),
            None,
        ),
        row(
            "Composition",
            "Timing",
            RowValue::ReadOnly(format!("{fps} fps · {seconds:.2} s")),
            None,
        ),
        row(
            "Composition",
            "Frames",
            RowValue::ReadOnly(frames.to_string()),
            None,
        ),
        row(
            "Composition",
            "Background",
            RowValue::Color {
                target: FieldTarget::None,
                color: composition.background(),
                text: hex(composition.background()),
            },
            None,
        ),
        row(
            "Scene",
            "Layers",
            RowValue::ReadOnly(layers.to_string()),
            None,
        ),
    ];
    Model { header: None, rows }
}

/// The field currently being typed into.
#[derive(Debug, Clone, PartialEq)]
pub struct Editing {
    /// Field being edited.
    pub target: FieldTarget,
    /// The editor model.
    pub editor: TextEdit,
    /// Whether the current text does not parse.
    pub invalid: bool,
}

/// One drawable/hit-testable item of the inspector body.
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    /// A section title.
    Section {
        /// Title.
        title: &'static str,
        /// Its rectangle.
        rect: Rect,
    },
    /// One property row.
    Row {
        /// Index into [`Model::rows`].
        index: usize,
        /// The whole row.
        rect: Rect,
        /// The label (also the numeric scrub handle).
        label: Rect,
        /// Stopwatch button, when the row has a channel.
        stopwatch: Option<Rect>,
        /// Filled/outline key diamond, when the row has a channel.
        diamond: Option<Rect>,
        /// Editable fields with their targets.
        fields: Vec<(Rect, FieldTarget)>,
        /// Segments of a segmented control: left, centre, right for Align,
        /// one tile per [`motioner_core::Ease::ALL`] entry for Ease; empty
        /// for other rows.
        segments: Vec<Rect>,
    },
    /// The selected layer's header (name, kind, identity, copy button).
    Header {
        /// The whole header block.
        rect: Rect,
        /// The "Copy" button next to the identity.
        copy: Rect,
    },
}

/// Rows and headers laid out for drawing and hit testing.
#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    /// Items top to bottom.
    pub items: Vec<Item>,
    /// Total content height, for scroll limits.
    pub height: f32,
}

/// Lays the selected layer's header out and returns the next `y`.
fn layout_header(
    model: &Model,
    left: f32,
    right: f32,
    y: f32,
    m: Metrics,
    items: &mut Vec<Item>,
) -> f32 {
    if model.header.is_none() {
        return y;
    }
    let h = m.px(56.0);
    let rect = Rect::new(left, y, (right - left).max(0.0), h);
    let copy = Rect::new(right - m.px(44.0), y + m.px(36.0), m.px(44.0), m.px(16.0));
    items.push(Item::Header { rect, copy });
    y + h + m.px(size::GRID * 3.0)
}

/// Lays `model` out inside `body`, scrolled by `scroll`.
#[must_use]
#[allow(clippy::too_many_lines)] // one branch per row value kind
pub fn layout(model: &Model, body: Rect, scroll: f32, m: Metrics) -> Layout {
    let pad = m.px(size::PAD);
    let row_h = m.px(28.0);
    let field_h = m.px(24.0);
    // Channel rows start with a fixed key column (stopwatch, then the key
    // diamond) so the diamond never touches the label; the label column
    // after it is wide enough for "Anchor X".
    let key_column = m.px(36.0);
    let label_column = m.px(104.0);
    let gap = m.px(size::GRID * 2.0);
    let left = body.x + pad;
    let right = body.right() - pad;
    let value_x = left + label_column;
    let value_w = (right - value_x).max(0.0);
    let mut y = body.y - scroll + m.px(size::GRID * 3.0);
    let top = y;
    let mut items = Vec::new();

    y = layout_header(model, left, right, y, m, &mut items);

    let mut section = "";
    for (index, row) in model.rows.iter().enumerate() {
        if row.section != section {
            section = row.section;
            let rect = Rect::new(left, y, (right - left).max(0.0), m.px(24.0));
            items.push(Item::Section {
                title: section,
                rect,
            });
            y += m.px(24.0);
        }
        let height = match row.value {
            RowValue::Ease { .. } => ease_grid_height(m) + row_h,
            RowValue::Blend { .. } => blend_grid_height(m) + row_h,
            _ => row_h,
        };
        let rect = Rect::new(left, y, (right - left).max(0.0), height);
        let line = Rect::new(left, y, rect.w, row_h);
        let stopwatch = row.channel.as_ref().map(|_| {
            Rect::new(
                left,
                (line.center_y() - m.px(8.0)).round(),
                m.px(16.0),
                m.px(16.0),
            )
        });
        let diamond = row.channel.as_ref().map(|_| {
            let side = m.px(10.0);
            Rect::new(
                left + m.px(21.0),
                (line.center_y() - side * 0.5).round(),
                side,
                side,
            )
        });
        let label_x = left
            + if row.channel.is_some() {
                key_column
            } else {
                0.0
            };
        let label = Rect::new(label_x, y, (value_x - gap - label_x).max(0.0), row_h);
        let field_y = (line.center_y() - field_h * 0.5).round();
        let mut fields = Vec::new();
        let mut segments = Vec::new();
        match &row.value {
            RowValue::Pair { left: a, right: b } => {
                let half = ((value_w - gap) * 0.5).floor();
                fields.push((Rect::new(value_x, field_y, half, field_h), a.target));
                fields.push((
                    Rect::new(value_x + half + gap, field_y, half, field_h),
                    b.target,
                ));
            }
            RowValue::Single(f) => {
                fields.push((Rect::new(value_x, field_y, value_w, field_h), f.target));
            }
            RowValue::Color { target, .. } | RowValue::Text { target, .. } => {
                fields.push((Rect::new(value_x, field_y, value_w, field_h), *target));
            }
            RowValue::Align { target, .. } => {
                let seg_w = (value_w / 3.0).floor();
                segments = (0..3)
                    .map(|i| Rect::new(value_x + seg_w * i as f32, field_y, seg_w, field_h))
                    .collect();
                fields.push((Rect::new(value_x, field_y, seg_w * 3.0, field_h), *target));
            }
            RowValue::Ease { .. } => {
                segments = ease_tiles(Rect::new(left, y + row_h, rect.w, ease_grid_height(m)), m);
            }
            RowValue::Blend { .. } => {
                segments = blend_tiles(Rect::new(left, y + row_h, rect.w, blend_grid_height(m)), m);
            }
            RowValue::ReadOnly(_) => {
                fields.push((
                    Rect::new(value_x, field_y, value_w, field_h),
                    FieldTarget::None,
                ));
            }
        }
        items.push(Item::Row {
            index,
            rect,
            label,
            stopwatch,
            diamond,
            fields,
            segments,
        });
        y += height;
    }
    let height = (y - top).max(0.0) + pad;
    Layout { items, height }
}

/// Columns of the ease grid.
const EASE_COLUMNS: usize = 3;
/// Height of one ease tile (curve preview over its name).
const EASE_TILE: f32 = 52.0;

/// Height of the ease grid below the "Ease" label.
fn ease_grid_height(m: Metrics) -> f32 {
    let rows = motioner_core::Ease::ALL.len().div_ceil(EASE_COLUMNS);
    let gap = m.px(size::GRID);
    m.px(EASE_TILE) * rows as f32 + gap * (rows as f32 - 1.0) + m.px(size::GRID)
}

/// Short labels for the blend tiles, in [`motioner_core::BlendMode::ALL`] order.
pub const BLEND_LABELS: [&str; 7] = [
    "Normal", "Add", "Mult", "Screen", "Overlay", "Darken", "Lighten",
];

/// Height of the blend grid below the "Blend" label.
fn blend_grid_height(m: Metrics) -> f32 {
    let rows = motioner_core::BlendMode::ALL.len().div_ceil(4);
    let gap = m.px(size::GRID);
    m.px(24.0) * rows as f32 + gap * (rows as f32 - 1.0) + m.px(size::GRID)
}

/// One tile per blend mode, four per row.
fn blend_tiles(area: Rect, m: Metrics) -> Vec<Rect> {
    const COLUMNS: usize = 4;
    let gap = m.px(size::GRID);
    let w = ((area.w - gap * (COLUMNS as f32 - 1.0)) / COLUMNS as f32).floor();
    let h = m.px(24.0);
    (0..motioner_core::BlendMode::ALL.len())
        .map(|index| {
            let column = (index % COLUMNS) as f32;
            let row = (index / COLUMNS) as f32;
            Rect::new(area.x + (w + gap) * column, area.y + (h + gap) * row, w, h)
        })
        .collect()
}

/// One tile per ease, in [`motioner_core::Ease::ALL`] order.
fn ease_tiles(area: Rect, m: Metrics) -> Vec<Rect> {
    let gap = m.px(size::GRID);
    let w = ((area.w - gap * (EASE_COLUMNS as f32 - 1.0)) / EASE_COLUMNS as f32).floor();
    let h = m.px(EASE_TILE);
    (0..motioner_core::Ease::ALL.len())
        .map(|index| {
            let column = (index % EASE_COLUMNS) as f32;
            let row = (index / EASE_COLUMNS) as f32;
            Rect::new(area.x + (w + gap) * column, area.y + (h + gap) * row, w, h)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Metrics;
    use motioner_core::{OverrideAction, OverrideEntry, OverrideValue};

    const HELLO: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/hello.motion"
    ));

    fn hello() -> Document {
        motioner_dsl::run(HELLO, &motioner_dsl::Limits::default()).unwrap()
    }

    fn body() -> Rect {
        Rect::new(0.0, 0.0, 320.0, 2000.0)
    }

    fn field_of<'a>(model: &'a Model, label: &str) -> &'a Row {
        model.rows.iter().find(|r| r.label == label).unwrap()
    }

    #[test]
    fn transform_values_follow_the_frame() {
        let document = hello();
        let title = document.roots()[0];
        let start = inspect_node(&document, title, 0, None, "Title").unwrap();
        let settled = inspect_node(&document, title, 120, None, "Title").unwrap();
        let position_x = field_of(&start, "Pos X");
        match &position_x.value {
            RowValue::Single(field) => {
                assert_eq!(field.text, "960");
                assert_eq!(
                    field.target,
                    FieldTarget::Transform(Property::Position, Axis::X)
                );
            }
            other => panic!("{other:?}"),
        }
        match &field_of(&start, "Pos Y").value {
            RowValue::Single(field) => assert_eq!(field.text, "520"),
            other => panic!("{other:?}"),
        }
        // X and Y have their own channels and keys.
        assert!(position_x.channel.as_ref().unwrap().keyed);
        match &field_of(&settled, "Pos Y").value {
            RowValue::Single(field) => assert_eq!(field.text, "480"),
            other => panic!("{other:?}"),
        }
        let opacity = field_of(&settled, "Opacity");
        assert!(opacity.channel.as_ref().unwrap().keyed);
        match &opacity.value {
            RowValue::Single(f) => assert_eq!(f.text, "100"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn stopwatch_state_reports_keys_and_the_playhead() {
        let document = hello();
        let title = document.roots()[0];
        let model = inspect_node(&document, title, 48, None, "Title").unwrap();
        let opacity = field_of(&model, "Opacity").channel.clone().unwrap();
        assert!(opacity.keyed);
        assert!(opacity.on_key, "frame 48 is a key (0.8 s at 60 fps)");
        assert_eq!(opacity.frames, vec![0, 48]);
        let between = inspect_node(&document, title, 20, None, "Title").unwrap();
        assert!(
            !field_of(&between, "Opacity")
                .channel
                .as_ref()
                .unwrap()
                .on_key
        );
        let rotation = field_of(&between, "Rotation").channel.clone().unwrap();
        assert!(!rotation.keyed);
    }

    #[test]
    fn the_name_override_replaces_the_display_name() {
        let document = hello();
        let title = document.roots()[0];
        let key = document.node(title).unwrap().identity().as_str().to_owned();
        let mut set = OverrideSet::new();
        set.insert(OverrideEntry::new(
            key,
            motioner_core::NAME,
            OverrideAction::Set(OverrideValue::Text("Headline".to_owned())),
        ));
        let model = inspect_node(&document, title, 0, Some(&set), "Title").unwrap();
        match &field_of(&model, "Name").value {
            RowValue::Text { text, .. } => assert_eq!(text, "Headline"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn layout_matches_the_model_and_has_no_overlaps() {
        let document = hello();
        let title = document.roots()[0];
        let model = inspect_node(&document, title, 0, None, "Title").unwrap();
        let m = Metrics::new(1.0);
        let out = layout(&model, body(), 0.0, m);
        let rows: Vec<&Item> = out
            .items
            .iter()
            .filter(|item| matches!(item, Item::Row { .. }))
            .collect();
        assert_eq!(rows.len(), model.rows.len());
        for pair in out.items.windows(2) {
            let a = item_rect(&pair[0]);
            let b = item_rect(&pair[1]);
            assert!(
                a.bottom() <= b.y + 1e-3 || b.bottom() <= a.y + 1e-3,
                "items overlap: {a:?} {b:?}"
            );
        }
        for item in &out.items {
            if let Item::Row {
                fields, diamond, ..
            } = item
            {
                for (rect, _) in fields {
                    assert!(rect.w > 0.0 && rect.h > 0.0);
                }
                if let Some(diamond) = diamond {
                    assert!(diamond.right() <= fields.first().map_or(f32::INFINITY, |(r, _)| r.x));
                }
            }
        }
        // Scrolling moves everything up by the same amount.
        let scrolled = layout(&model, body(), 40.0, m);
        if let (Some(Item::Row { rect: a, .. }), Some(Item::Row { rect: b, .. })) =
            (out.items.get(1), scrolled.items.get(1))
        {
            assert!((a.y - b.y - 40.0).abs() < 1e-3);
        }
    }

    fn item_rect(item: &Item) -> Rect {
        match item {
            Item::Section { rect, .. } | Item::Header { rect, .. } | Item::Row { rect, .. } => {
                *rect
            }
        }
    }

    #[test]
    fn content_rows_match_the_node_kind() {
        let document = hello();
        let rows = crate::outline_rows(&document);
        let rect = rows
            .iter()
            .find(|r| r.kind == crate::NodeKind::Rect)
            .unwrap();
        let model = inspect_node(&document, rect.node, 0, None, "Rect 1").unwrap();
        let labels: Vec<&str> = model.rows.iter().map(|r| r.label.as_str()).collect();
        assert!(labels.contains(&"Size"));
        assert!(labels.contains(&"Radius"));
        assert!(labels.contains(&"Fill"));
        assert!(labels.contains(&"Width"));
        let text = inspect_node(&document, document.roots()[0], 0, None, "Title").unwrap();
        assert!(text.rows.iter().any(|r| r.label == "Text"));
        assert!(text.rows.iter().any(|r| r.label == "Align"));
    }

    #[test]
    fn composition_rows_are_read_only() {
        let document = hello();
        let model = inspect_comp(document.composition(), 10);
        assert!(model.header.is_none());
        let m = Metrics::new(1.0);
        let out = layout(&model, body(), 0.0, m);
        // Only the background swatch is clickable, and it writes nothing.
        assert!(out.items.iter().all(|item| match item {
            Item::Row {
                fields, segments, ..
            } => {
                segments.is_empty()
                    && fields
                        .iter()
                        .all(|(_, target)| *target == FieldTarget::None)
            }
            _ => true,
        }));
    }

    #[test]
    fn a_missing_node_inspects_to_none() {
        let other = motioner_dsl::run(
            "comp(name=\"A\", duration=1.0)\n",
            &motioner_dsl::Limits::default(),
        )
        .unwrap();
        let document = hello();
        assert!(inspect_node(&other, document.roots()[0], 0, None, "Title").is_none());
    }
}

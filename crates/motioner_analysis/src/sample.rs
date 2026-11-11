//! `sample`: exact channel values at frames (D-0043).

use motioner_core::{
    split_property, Axis, ContentChannel, ContentField, Document, Node, NodeId, Property,
};
use motioner_eval::{sample, sample_vec2};

fn axis_value(vector: motioner_core::Vec2, axis: Option<Axis>) -> f64 {
    match axis {
        Some(Axis::Y) => vector.y,
        Some(Axis::X) | None => vector.x,
    }
}

fn channel_number(channel: ContentChannel<'_>, frame: i64) -> Option<f64> {
    match channel {
        ContentChannel::Number(value) => Some(sample(value, frame)),
        ContentChannel::Color(_) | ContentChannel::OptionalColor(_) => None,
    }
}

/// One sampled value of `node`'s `property` at `frame`.
///
/// Transform channels (`pos`, `pos.x`, `scale.y`, `rotation`, `opacity`,
/// `anchor.x`) and numeric content channels (`width`, `radius`, `size`,
/// `tracking`, `line_height`, `stroke_width`, ...) are supported. Colour
/// channels return `None`; `describe_frame`/`get_node` report those.
#[must_use]
pub fn sample_property(document: &Document, id: NodeId, property: &str, frame: i64) -> Option<f64> {
    let node: &Node = document.node(id)?;
    let transform = node.transform();
    let (base, axis) = split_property(property);
    if let Some(kind) = Property::from_name(base) {
        return match kind {
            Property::Position => Some(axis_value(sample_vec2(&transform.position, frame), axis)),
            Property::Scale => Some(axis_value(sample_vec2(&transform.scale, frame), axis)),
            Property::Anchor => Some(axis_value(sample_vec2(&transform.anchor, frame), axis)),
            Property::Rotation => Some(sample(&transform.rotation, frame)),
            Property::Opacity => Some(sample(&transform.opacity, frame)),
        };
    }
    let field = ContentField::from_name(base)?;
    let channel = node.content().channel(field)?;
    channel_number(channel, frame)
}

/// `sample_property` over an inclusive frame range, in order.
#[must_use]
pub fn sample_series(
    document: &Document,
    id: NodeId,
    property: &str,
    from: i64,
    to: i64,
    step: i64,
) -> Vec<(i64, f64)> {
    let step = step.max(1);
    let mut out = Vec::new();
    let mut frame = from;
    while frame <= to {
        if let Some(value) = sample_property(document, id, property, frame) {
            out.push((frame, value));
        }
        frame += step;
    }
    out
}

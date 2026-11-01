//! Reading animatable channels: keys, eases and sampled values. One channel
//! is a scalar property (`rotation`, `opacity`) or one axis of a vector
//! property (`pos.x`); `pos` without an axis means both axes together, as
//! `key("pos", ...)` does. This is the one place that maps a channel to the
//! transform's fields; the inspector, the timeline and the editor all read
//! channels through it.

use motioner_core::{
    split_property, Animatable, AnimatableVec2, Axis, Document, Ease, Frame, NodeId, OverrideValue,
    Property, Transform, Vec2,
};

use crate::command::{CommandError, ErrorKind};

/// Every channel the editor shows, in display order (one per axis).
pub const CHANNELS: [(Property, Option<Axis>); 8] = [
    (Property::Position, Some(Axis::X)),
    (Property::Position, Some(Axis::Y)),
    (Property::Scale, Some(Axis::X)),
    (Property::Scale, Some(Axis::Y)),
    (Property::Rotation, None),
    (Property::Anchor, Some(Axis::X)),
    (Property::Anchor, Some(Axis::Y)),
    (Property::Opacity, None),
];

/// The override name of a channel: `pos.x`, `opacity`.
#[must_use]
pub fn channel_name(property: Property, axis: Option<Axis>) -> String {
    match axis {
        Some(axis) => format!("{}.{}", property.name(), axis.name()),
        None => property.name().to_owned(),
    }
}

/// Resolves `id` and a channel name to a node and channel.
///
/// # Errors
///
/// [`ErrorKind::UnknownNode`](crate::ErrorKind::UnknownNode) when no node has the identity and
/// [`ErrorKind::UnknownProperty`](crate::ErrorKind::UnknownProperty) when the name is not an animatable channel.
pub fn resolve(
    document: &Document,
    id: &str,
    property: &str,
) -> Result<(NodeId, Property, Option<Axis>), CommandError> {
    let node = node_of(document, id)?;
    let (base, axis) = split_property(property);
    let Some(parsed) = Property::from_name(base) else {
        return Err(CommandError::new(
            ErrorKind::UnknownProperty,
            format!(
                "'{property}' is not an animatable channel; use pos, pos.x, pos.y, scale, \
                 scale.x, scale.y, rotation, anchor, anchor.x, anchor.y or opacity"
            ),
        ));
    };
    if axis.is_some() && !parsed.is_vector() {
        return Err(CommandError::new(
            ErrorKind::UnknownProperty,
            format!("'{base}' has no axes"),
        ));
    }
    Ok((node, parsed, axis))
}

/// The node with identity `id`.
///
/// # Errors
///
/// [`ErrorKind::UnknownNode`](crate::ErrorKind::UnknownNode) when no node has it.
pub fn node_of(document: &Document, id: &str) -> Result<NodeId, CommandError> {
    document.find(id).ok_or_else(|| {
        CommandError::new(
            ErrorKind::UnknownNode,
            format!("no node has the identity {id:?} (see `query` for the ids)"),
        )
    })
}

fn vector(t: &Transform, property: Property) -> Option<&AnimatableVec2> {
    match property {
        Property::Position => Some(&t.position),
        Property::Scale => Some(&t.scale),
        Property::Anchor => Some(&t.anchor),
        Property::Rotation | Property::Opacity => None,
    }
}

fn scalar(t: &Transform, property: Property, axis: Option<Axis>) -> Option<&Animatable<f64>> {
    match (property, axis) {
        (Property::Rotation, _) => Some(&t.rotation),
        (Property::Opacity, _) => Some(&t.opacity),
        (_, Some(axis)) => vector(t, property).map(|v| v.axis(axis)),
        (_, None) => None,
    }
}

/// Frames that have a key on a transform's channel, sorted. A whole vector
/// property reports the union of its axes.
#[must_use]
pub fn transform_keys(t: &Transform, property: Property, axis: Option<Axis>) -> Vec<Frame> {
    if let Some(value) = scalar(t, property, axis) {
        return value.keys().iter().map(|k| k.frame).collect();
    }
    vector(t, property).map_or_else(Vec::new, AnimatableVec2::key_frames)
}

/// Frames that have a key on the node's channel (see [`transform_keys`]).
#[must_use]
pub fn keys(
    document: &Document,
    node: NodeId,
    property: Property,
    axis: Option<Axis>,
) -> Vec<Frame> {
    document
        .node(node)
        .map_or_else(Vec::new, |n| transform_keys(n.transform(), property, axis))
}

/// Eases of the channel's keys, parallel to [`keys`] (empty for a whole
/// vector property).
#[must_use]
pub fn eases(
    document: &Document,
    node: NodeId,
    property: Property,
    axis: Option<Axis>,
) -> Vec<Ease> {
    document
        .node(node)
        .and_then(|n| scalar(n.transform(), property, axis))
        .map_or_else(Vec::new, |value| {
            value.keys().iter().map(|k| k.ease).collect()
        })
}

/// The channel's value at `frame`: a number for a scalar channel, a pair
/// for a whole vector property.
#[must_use]
pub fn sampled(
    document: &Document,
    node: NodeId,
    property: Property,
    axis: Option<Axis>,
    frame: Frame,
) -> Option<OverrideValue> {
    let t = document.node(node)?.transform();
    if let Some(value) = scalar(t, property, axis) {
        return Some(OverrideValue::Number(motioner_eval::sample(value, frame)));
    }
    vector(t, property).map(|value| OverrideValue::Vec2(motioner_eval::sample_vec2(value, frame)))
}

/// The value and ease of the key at `frame`, if there is one.
#[must_use]
pub fn key_at(
    document: &Document,
    node: NodeId,
    property: Property,
    axis: Option<Axis>,
    frame: Frame,
) -> Option<(OverrideValue, Ease)> {
    let t = document.node(node)?.transform();
    if let Some(value) = scalar(t, property, axis) {
        return value
            .keys()
            .iter()
            .find(|k| k.frame == frame)
            .map(|k| (OverrideValue::Number(k.value), k.ease));
    }
    let pair = vector(t, property)?;
    let x = pair
        .axis(Axis::X)
        .keys()
        .iter()
        .find(|k| k.frame == frame)?;
    let y = pair
        .axis(Axis::Y)
        .keys()
        .iter()
        .find(|k| k.frame == frame)?;
    Some((OverrideValue::Vec2(Vec2::new(x.value, y.value)), x.ease))
}

#[cfg(test)]
mod tests {
    use super::*;

    const HELLO: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/hello.motion"
    ));

    #[test]
    fn channels_read_keys_eases_and_values() {
        let document = motioner_dsl::run(HELLO, &motioner_dsl::Limits::default()).unwrap();
        let title = document.roots()[0];
        let id = document.node(title).unwrap().identity().as_str().to_owned();
        let (node, property, axis) = resolve(&document, &id, "pos.y").unwrap();
        assert_eq!(
            (node, property, axis),
            (title, Property::Position, Some(Axis::Y))
        );
        assert_eq!(keys(&document, node, property, axis), vec![0, 48]);
        assert_eq!(eases(&document, node, property, axis)[1], Ease::CubicOut);
        assert_eq!(
            sampled(&document, node, property, axis, 48),
            Some(OverrideValue::Number(480.0))
        );
        assert_eq!(
            key_at(&document, node, Property::Position, None, 48),
            Some((OverrideValue::Vec2(Vec2::new(960.0, 480.0)), Ease::CubicOut))
        );
        assert_eq!(
            resolve(&document, "nope", "pos").unwrap_err().kind,
            ErrorKind::UnknownNode
        );
        assert_eq!(
            resolve(&document, &id, "rotation.x").unwrap_err().kind,
            ErrorKind::UnknownProperty
        );
        assert_eq!(channel_name(Property::Scale, Some(Axis::Y)), "scale.y");
    }
}

//! Reading animatable channels: keys, eases and sampled values.
//!
//! One channel is either a transform channel (a scalar property such as
//! `rotation`, or one axis of a vector property such as `pos.x`) or a content
//! field (`fill`, `width`, `size`, ...; D-0028). `pos` without an axis means
//! both axes together, as `key("pos", ...)` does. This is the one place that
//! maps a channel name to the document's fields; the inspector, the timeline,
//! `query` and the editor all read channels through it.

use motioner_core::{
    split_property, Animatable, AnimatableVec2, Axis, Color, ContentChannel, ContentField,
    Document, Ease, EffectChannel, EffectParam, Frame, NodeId, OverrideValue, Property, Transform,
};

use crate::command::{CommandError, ErrorKind};

/// Every transform channel the editor shows, in display order (one per axis).
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

/// One animatable channel of a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Channel {
    /// A transform property, optionally one axis of a vector property.
    Transform(Property, Option<Axis>),
    /// A content field (no axes).
    Content(ContentField),
    /// An effect parameter, with its axis for the offset (D-0028 section 3).
    Effect(EffectChannel),
}

impl Channel {
    /// The override name of the channel: `pos.x`, `opacity`, `fill`.
    #[must_use]
    pub fn name(self) -> String {
        match self {
            Self::Transform(property, axis) => channel_name(property, axis),
            Self::Content(field) => field.name().clone(),
            Self::Effect(channel) => channel.name(),
        }
    }

    /// The property name written in edit commands (`pos`, `fill`).
    #[must_use]
    pub fn base(self) -> String {
        match self {
            Self::Transform(property, _) => property.name().to_owned(),
            Self::Content(field) => field.name(),
            Self::Effect(channel) => channel.base_name(),
        }
    }

    /// The axis of a vector transform channel.
    #[must_use]
    pub fn axis(self) -> Option<Axis> {
        match self {
            Self::Transform(_, axis) => axis,
            Self::Content(_) => None,
            Self::Effect(channel) => channel.axis,
        }
    }

    /// Whether this is a content channel.
    #[must_use]
    pub const fn is_content(self) -> bool {
        matches!(self, Self::Content(_))
    }

    /// Whether this is an effect parameter channel.
    #[must_use]
    pub const fn is_effect(self) -> bool {
        matches!(self, Self::Effect(_))
    }
}

/// The override name of a transform channel: `pos.x`, `opacity`.
#[must_use]
pub fn channel_name(property: Property, axis: Option<Axis>) -> String {
    match axis {
        Some(axis) => format!("{}.{}", property.name(), axis.name()),
        None => property.name().to_owned(),
    }
}

/// Every channel of a node, in display order: the transform channels, then
/// the content fields its kind accepts and can animate.
#[must_use]
pub fn node_channels(document: &Document, node: NodeId) -> Vec<Channel> {
    let mut channels: Vec<Channel> = CHANNELS
        .into_iter()
        .map(|(property, axis)| Channel::Transform(property, axis))
        .collect();
    if let Some(content) = document.node(node).map(motioner_core::Node::content) {
        for field in crate::timeline_model::content_fields(content) {
            if field.is_animatable() {
                channels.push(Channel::Content(field));
            }
        }
    }
    if let Some(node) = document.node(node) {
        for effect in node.effects() {
            for &param in effect.kind().params() {
                if param.is_vector() {
                    for axis in [Axis::X, Axis::Y] {
                        if let Ok(channel) = EffectChannel::new(effect.kind(), param, Some(axis)) {
                            channels.push(Channel::Effect(channel));
                        }
                    }
                } else if let Ok(channel) = EffectChannel::new(effect.kind(), param, None) {
                    channels.push(Channel::Effect(channel));
                }
            }
        }
    }
    channels
}

/// Resolves `id` and a channel name to a node and channel.
///
/// # Errors
///
/// [`ErrorKind::UnknownNode`](crate::ErrorKind::UnknownNode) when no node has the identity and
/// [`ErrorKind::UnknownProperty`](crate::ErrorKind::UnknownProperty) when the name is not an animatable channel
/// of that node.
pub fn resolve(
    document: &Document,
    id: &str,
    property: &str,
) -> Result<(NodeId, Channel), CommandError> {
    let node = node_of(document, id)?;
    if let Some(channel) = EffectChannel::from_name(property) {
        let has = document
            .node(node)
            .is_some_and(|node| node.effect(channel.kind).is_some());
        if !has {
            return Err(CommandError::new(
                ErrorKind::UnknownProperty,
                format!(
                    "'{property}' needs a '{}' effect on this layer (use effect.add)",
                    channel.kind.name()
                ),
            ));
        }
        return Ok((node, Channel::Effect(channel)));
    }
    if let Some((kind, _)) = property.split_once('.') {
        if let Some(kind) = motioner_core::EffectKind::from_name(kind) {
            if kind.accepts(EffectParam::Offset) && property.ends_with(".offset") {
                return Err(CommandError::new(
                    ErrorKind::UnknownProperty,
                    format!("'{property}' needs an axis ('{property}.x' or '{property}.y')"),
                ));
            }
        }
    }
    let (base, axis) = split_property(property);
    if let Some(field) = ContentField::from_name(base) {
        if axis.is_some() {
            return Err(CommandError::new(
                ErrorKind::UnknownProperty,
                format!("'{base}' has no axes"),
            ));
        }
        let content = document
            .node(node)
            .map(motioner_core::Node::content)
            .ok_or_else(|| unknown_property(property))?;
        if !field.is_animatable() || !content.field_exists(field) {
            return Err(CommandError::new(
                ErrorKind::UnknownProperty,
                format!(
                    "'{property}' is not an animatable channel of this {} layer",
                    content.kind_name()
                ),
            ));
        }
        return Ok((node, Channel::Content(field)));
    }
    let Some(parsed) = Property::from_name(base) else {
        let names: Vec<&str> = Property::ALL.iter().map(|(_, name)| *name).collect();
        let fields: Vec<&str> = ContentField::ALL
            .iter()
            .filter(|(field, _)| field.is_animatable())
            .map(|(_, name)| *name)
            .collect();
        return Err(CommandError::new(
            ErrorKind::UnknownProperty,
            format!(
                "'{property}' is not an animatable channel; transform channels are {} and \
                 content channels are {}",
                names.join(", "),
                fields.join(", ")
            ),
        ));
    };
    if axis.is_some() && !parsed.is_vector() {
        return Err(CommandError::new(
            ErrorKind::UnknownProperty,
            format!("'{base}' has no axes"),
        ));
    }
    Ok((node, Channel::Transform(parsed, axis)))
}

fn unknown_property(property: &str) -> CommandError {
    CommandError::new(
        ErrorKind::UnknownProperty,
        format!("'{property}' is not an animatable channel"),
    )
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

/// Frames that have a key on a transform channel, sorted. A whole vector
/// property reports the union of its axes.
#[must_use]
pub fn transform_keys(t: &Transform, property: Property, axis: Option<Axis>) -> Vec<Frame> {
    if let Some(value) = scalar(t, property, axis) {
        return value.keys().iter().map(|k| k.frame).collect();
    }
    vector(t, property).map_or_else(Vec::new, AnimatableVec2::key_frames)
}

/// Frames that have a key on the channel, sorted.
#[must_use]
pub fn keys(document: &Document, node: NodeId, channel: Channel) -> Vec<Frame> {
    let Some(node) = document.node(node) else {
        return Vec::new();
    };
    match channel {
        Channel::Transform(property, axis) => transform_keys(node.transform(), property, axis),
        Channel::Content(field) => node
            .content()
            .channel(field)
            .map(|value| value.key_frames())
            .unwrap_or_default(),
        Channel::Effect(channel) => effect_channel(node, channel)
            .map(|value| value.key_frames())
            .unwrap_or_default(),
    }
}

/// The channel of an effect parameter, as a content-shaped read.
fn effect_channel(
    node: &motioner_core::Node,
    channel: EffectChannel,
) -> Option<ContentChannel<'_>> {
    match node
        .effect(channel.kind)?
        .channel_ref(channel.param, channel.axis)?
    {
        motioner_core::EffectChannelRef::Number(value) => Some(ContentChannel::Number(value)),
        motioner_core::EffectChannelRef::Color(value) => Some(ContentChannel::Color(value)),
    }
}

/// Eases of the channel's keys, parallel to [`keys`] (empty for a whole
/// vector property and for content channels).
#[must_use]
pub fn eases(document: &Document, node: NodeId, channel: Channel) -> Vec<Ease> {
    let Some(node) = document.node(node) else {
        return Vec::new();
    };
    match channel {
        Channel::Transform(property, axis) => scalar(node.transform(), property, axis)
            .map_or_else(Vec::new, |value| {
                value.keys().iter().map(|k| k.ease).collect()
            }),
        Channel::Content(field) => node
            .content()
            .channel(field)
            .map_or_else(Vec::new, |value| match value {
                ContentChannel::Number(value) => value.keys().iter().map(|k| k.ease).collect(),
                ContentChannel::Color(value) => value.keys().iter().map(|k| k.ease).collect(),
                ContentChannel::OptionalColor(value) => {
                    value.keys().iter().map(|k| k.ease).collect()
                }
            }),
        Channel::Effect(channel) => {
            effect_channel(node, channel).map_or_else(Vec::new, |value| match value {
                ContentChannel::Number(value) => value.keys().iter().map(|k| k.ease).collect(),
                ContentChannel::Color(value) => value.keys().iter().map(|k| k.ease).collect(),
                ContentChannel::OptionalColor(value) => {
                    value.keys().iter().map(|k| k.ease).collect()
                }
            })
        }
    }
}

/// The channel's value at `frame`: a number for a scalar channel, a pair for
/// a whole vector property and a colour string (or `"none"`) for a colour
/// field.
#[must_use]
pub fn sampled(
    document: &Document,
    node: NodeId,
    channel: Channel,
    frame: Frame,
) -> Option<OverrideValue> {
    let node = document.node(node)?;
    match channel {
        Channel::Transform(property, axis) => {
            let t = node.transform();
            if let Some(value) = scalar(t, property, axis) {
                return Some(OverrideValue::Number(motioner_eval::sample(value, frame)));
            }
            vector(t, property)
                .map(|value| OverrideValue::Vec2(motioner_eval::sample_vec2(value, frame)))
        }
        Channel::Content(field) => {
            let channel = node.content().channel(field)?;
            Some(match channel {
                ContentChannel::Number(value) => {
                    OverrideValue::Number(motioner_eval::sample(value, frame))
                }
                ContentChannel::Color(value) => {
                    OverrideValue::Text(motioner_eval::sample(value, frame).to_hex())
                }
                ContentChannel::OptionalColor(value) => {
                    colour_value(motioner_eval::sample(value, frame))
                }
            })
        }
        Channel::Effect(channel) => {
            let channel = effect_channel(node, channel)?;
            Some(match channel {
                ContentChannel::Number(value) => {
                    OverrideValue::Number(motioner_eval::sample(value, frame))
                }
                ContentChannel::Color(value) => {
                    OverrideValue::Text(motioner_eval::sample(value, frame).to_hex())
                }
                ContentChannel::OptionalColor(value) => {
                    colour_value(motioner_eval::sample(value, frame))
                }
            })
        }
    }
}

/// A colour override value, `"none"` when the channel is absent.
fn colour_value(color: Option<Color>) -> OverrideValue {
    OverrideValue::Text(color.map_or_else(|| "none".to_owned(), Color::to_hex))
}

/// The sampled numeric value of a content field (inspector, timeline).
///
/// Returns `None` when the field is not a numeric channel of this node.
#[must_use]
pub fn sampled_number(
    document: &Document,
    node: NodeId,
    field: ContentField,
    frame: Frame,
) -> Option<f64> {
    match document.node(node)?.content().channel(field)? {
        ContentChannel::Number(value) => Some(motioner_eval::sample(value, frame)),
        ContentChannel::Color(_) | ContentChannel::OptionalColor(_) => None,
    }
}

/// The sampled colour of a content field at a frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampledColour {
    /// The field is not a colour channel of this node.
    NotColour,
    /// The channel is present but has no colour (`none`).
    Absent,
    /// The channel holds a colour.
    Colour(Color),
}

/// The sampled colour of a content field.
#[must_use]
pub fn sampled_colour(
    document: &Document,
    node: NodeId,
    field: ContentField,
    frame: Frame,
) -> SampledColour {
    let Some(channel) = document
        .node(node)
        .and_then(|node| node.content().channel(field))
    else {
        return SampledColour::NotColour;
    };
    match channel {
        ContentChannel::Color(value) => SampledColour::Colour(motioner_eval::sample(value, frame)),
        ContentChannel::OptionalColor(value) => match motioner_eval::sample(value, frame) {
            Some(color) => SampledColour::Colour(color),
            None => SampledColour::Absent,
        },
        ContentChannel::Number(_) => SampledColour::NotColour,
    }
}

/// The value and ease of the key at `frame`, if there is one.
#[must_use]
pub fn key_at(
    document: &Document,
    node: NodeId,
    channel: Channel,
    frame: Frame,
) -> Option<(OverrideValue, Ease)> {
    let node = document.node(node)?;
    match channel {
        Channel::Transform(property, axis) => {
            let t = node.transform();
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
            Some((
                OverrideValue::Vec2(motioner_core::Vec2::new(x.value, y.value)),
                x.ease,
            ))
        }
        Channel::Content(field) => match node.content().channel(field)? {
            ContentChannel::Number(value) => value
                .keys()
                .iter()
                .find(|k| k.frame == frame)
                .map(|k| (OverrideValue::Number(k.value), k.ease)),
            ContentChannel::Color(value) => value
                .keys()
                .iter()
                .find(|k| k.frame == frame)
                .map(|k| (OverrideValue::Text(k.value.to_hex()), k.ease)),
            ContentChannel::OptionalColor(value) => value
                .keys()
                .iter()
                .find(|k| k.frame == frame)
                .map(|k| (colour_value(k.value), k.ease)),
        },
        Channel::Effect(channel) => match effect_channel(node, channel)? {
            ContentChannel::Number(value) => value
                .keys()
                .iter()
                .find(|k| k.frame == frame)
                .map(|k| (OverrideValue::Number(k.value), k.ease)),
            ContentChannel::Color(value) => value
                .keys()
                .iter()
                .find(|k| k.frame == frame)
                .map(|k| (OverrideValue::Text(k.value.to_hex()), k.ease)),
            ContentChannel::OptionalColor(value) => value
                .keys()
                .iter()
                .find(|k| k.frame == frame)
                .map(|k| (colour_value(k.value), k.ease)),
        },
    }
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
        let (node, channel) = resolve(&document, &id, "pos.y").unwrap();
        assert_eq!(
            channel,
            Channel::Transform(Property::Position, Some(Axis::Y))
        );
        assert_eq!(keys(&document, node, channel), vec![0, 48]);
        assert_eq!(eases(&document, node, channel)[1], Ease::CubicOut);
        assert_eq!(
            sampled(&document, node, channel, 48),
            Some(OverrideValue::Number(480.0))
        );
        let whole = Channel::Transform(Property::Position, None);
        assert_eq!(
            key_at(&document, node, whole, 48),
            Some((
                OverrideValue::Vec2(motioner_core::Vec2::new(960.0, 480.0)),
                Ease::CubicOut
            ))
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

    #[test]
    fn content_channels_resolve_and_sample() {
        let document = motioner_dsl::run(HELLO, &motioner_dsl::Limits::default()).unwrap();
        let title = document.roots()[0];
        let id = document.node(title).unwrap().identity().as_str().to_owned();
        let (node, channel) = resolve(&document, &id, "color").unwrap();
        assert_eq!(channel, Channel::Content(ContentField::Color));
        assert_eq!(channel.name(), "color");
        assert_eq!(
            sampled(&document, node, channel, 0),
            Some(OverrideValue::Text("#FFFFFF".to_owned()))
        );
        // A group accepts no content field.
        let group = document
            .nodes()
            .find(|node| matches!(node.content(), motioner_core::NodeContent::Group))
            .unwrap();
        assert_eq!(
            resolve(&document, group.identity().as_str(), "fill")
                .unwrap_err()
                .kind,
            ErrorKind::UnknownProperty
        );
    }
}

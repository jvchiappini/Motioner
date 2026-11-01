//! The composition and its node tree, plus the validated mutation API.

use std::collections::HashMap;

use crate::animation::{Animatable, AnimatableVec2, Axis, Ease, Keyframe};
use crate::bounds;
use crate::color::Color;
use crate::error::CoreError;
use crate::math::Vec2;
use crate::node::{
    BlendMode, ContentField, ContentValue, Node, NodeContent, NodeId, NodeIdentity, Property,
    PropertyValue, Transform,
};
use crate::time::{frame_within_bounds, Frame, FrameRate};

/// Largest accepted canvas side in pixels.
pub const MAX_CANVAS_SIZE: u32 = 8192;

/// Canvas, time base and background of a composition.
#[derive(Debug, Clone, PartialEq)]
pub struct Composition {
    name: String,
    width: u32,
    height: u32,
    frame_rate: FrameRate,
    duration: Frame,
    background: Color,
}

impl Composition {
    /// Creates validated composition settings.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidCanvasSize`] when a side is zero or above
    /// [`MAX_CANVAS_SIZE`], and [`CoreError::InvalidDuration`] when
    /// `duration` is not positive.
    pub fn new(
        name: impl Into<String>,
        width: u32,
        height: u32,
        frame_rate: FrameRate,
        duration: Frame,
        background: Color,
    ) -> Result<Self, CoreError> {
        let side_ok = |side: u32| (1..=MAX_CANVAS_SIZE).contains(&side);
        if !side_ok(width) || !side_ok(height) {
            return Err(CoreError::InvalidCanvasSize { width, height });
        }
        if duration <= 0 {
            return Err(CoreError::InvalidDuration { frames: duration });
        }
        Ok(Self {
            name: name.into(),
            width,
            height,
            frame_rate,
            duration,
            background,
        })
    }

    /// Composition name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Canvas width in pixels.
    #[must_use]
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Canvas height in pixels.
    #[must_use]
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Frame rate.
    #[must_use]
    pub fn frame_rate(&self) -> FrameRate {
        self.frame_rate
    }

    /// Number of frames; valid frames are `0..duration`.
    #[must_use]
    pub fn duration(&self) -> Frame {
        self.duration
    }

    /// Background color painted under every layer.
    #[must_use]
    pub fn background(&self) -> Color {
        self.background
    }
}

/// A compiled composition: settings plus an ordered node tree.
///
/// Top-level nodes and each group's children are painted in insertion
/// order. Layer order is explicit and never inferred from IDs.
#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    composition: Composition,
    nodes: Vec<Node>,
    roots: Vec<NodeId>,
    /// Stable identity to node, for O(1) lookups from override files and the
    /// editor. Inserts and removes only; never iterated, so evaluation stays
    /// deterministic.
    identities: HashMap<String, NodeId>,
}

/// Whether a property write sets the static value or adds a keyframe.
#[derive(Clone, Copy)]
enum Write {
    Static,
    Key(Frame, Ease),
}

impl Document {
    /// Creates an empty document.
    #[must_use]
    pub fn new(composition: Composition) -> Self {
        Self {
            composition,
            nodes: Vec::new(),
            roots: Vec::new(),
            identities: HashMap::new(),
        }
    }

    /// The composition settings.
    #[must_use]
    pub fn composition(&self) -> &Composition {
        &self.composition
    }

    /// Top-level nodes in paint order.
    #[must_use]
    pub fn roots(&self) -> &[NodeId] {
        &self.roots
    }

    /// Number of nodes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether the document has no nodes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Looks up a node.
    #[must_use]
    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(id.index())
    }

    /// All nodes in creation order.
    pub fn nodes(&self) -> impl Iterator<Item = &Node> {
        self.nodes.iter()
    }

    /// Adds a top-level node with a default transform.
    ///
    /// The node gets a synthetic provenance identity `#[index]`, which is
    /// replaced by [`Self::add_node_with_identity`] or [`Self::assign_id`].
    /// The DSL always supplies a real identity.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidGeometry`] for negative or non-finite
    /// sizes and [`CoreError::TooManyNodes`] when IDs are exhausted.
    pub fn add_node(&mut self, content: NodeContent) -> Result<NodeId, CoreError> {
        let raw = u32::try_from(self.nodes.len()).map_err(|_| CoreError::TooManyNodes)?;
        self.add_node_with_identity(content, NodeIdentity::Provenance(format!("#{raw}")))
    }

    /// Adds a top-level node with an explicit stable identity.
    ///
    /// # Errors
    ///
    /// Same as [`Self::add_node`], plus [`CoreError::DuplicateId`] when an
    /// explicit identity is already used.
    pub fn add_node_with_identity(
        &mut self,
        content: NodeContent,
        identity: NodeIdentity,
    ) -> Result<NodeId, CoreError> {
        if let Some(field) = content.invalid_field() {
            return Err(CoreError::InvalidGeometry { field });
        }
        if identity.is_explicit() && self.identities.contains_key(identity.as_str()) {
            return Err(CoreError::DuplicateId {
                name: identity.as_str().to_owned(),
            });
        }
        let raw = u32::try_from(self.nodes.len()).map_err(|_| CoreError::TooManyNodes)?;
        let id = NodeId::from_raw(raw);
        self.nodes.push(Node {
            id,
            parent: None,
            children: Vec::new(),
            content,
            transform: Transform::default(),
            blend: BlendMode::Normal,
            identity,
            visible: true,
        });
        self.roots.push(id);
        self.index_identity(id);
        Ok(id)
    }

    /// Replaces a node's identity with a validated explicit `.id()` name.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::UnknownNode`] for an unknown ID,
    /// [`CoreError::InvalidIdentity`] for a malformed name and
    /// [`CoreError::DuplicateId`] when another node already uses it.
    pub fn assign_id(&mut self, id: NodeId, name: &str) -> Result<(), CoreError> {
        self.require(id)?;
        let identity = NodeIdentity::explicit(name)?;
        if self
            .identities
            .get(identity.as_str())
            .is_some_and(|&existing| existing != id)
        {
            return Err(CoreError::DuplicateId {
                name: name.to_owned(),
            });
        }
        self.identities
            .remove(self.nodes[id.index()].identity.as_str());
        self.nodes[id.index()].identity = identity;
        self.index_identity(id);
        Ok(())
    }

    /// The node with the given stable identity, if any.
    #[must_use]
    pub fn find(&self, identity: &str) -> Option<NodeId> {
        self.identities.get(identity).copied()
    }

    /// Sets whether a node and its subtree are rendered.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::UnknownNode`] for an unknown ID.
    pub fn set_visible(&mut self, id: NodeId, visible: bool) -> Result<(), CoreError> {
        self.require(id)?;
        self.nodes[id.index()].visible = visible;
        Ok(())
    }

    /// Writes a validated content field.
    ///
    /// # Errors
    ///
    /// Fails for an unknown node, a field the node's kind does not have
    /// ([`CoreError::WrongContentField`]) and a value of the wrong shape or
    /// out of range ([`CoreError::InvalidContentValue`]).
    pub fn set_content(
        &mut self,
        id: NodeId,
        field: ContentField,
        value: ContentValue,
    ) -> Result<(), CoreError> {
        let (kind, accepts) = {
            let node = self.require(id)?;
            (node.content.kind_name(), node.content.accepts(field))
        };
        if !accepts {
            return Err(CoreError::WrongContentField {
                kind,
                field: field.name(),
            });
        }
        let content = &mut self.nodes[id.index()].content;
        content_static_write(content, field, value)
    }

    /// Adds (or replaces) a keyframe on a content field (D-0028).
    ///
    /// `text` and `align` are discrete and cannot be keyed; keying them
    /// returns [`CoreError::WrongContentField`].
    ///
    /// # Errors
    ///
    /// Fails for an unknown node, a field the node's kind does not have, a
    /// value of the wrong shape or out of range, and
    /// [`CoreError::FrameOutOfRange`] when `frame` is outside the allowed
    /// range.
    pub fn add_content_keyframe(
        &mut self,
        id: NodeId,
        field: ContentField,
        frame: Frame,
        value: ContentValue,
        ease: Ease,
    ) -> Result<(), CoreError> {
        if !frame_within_bounds(frame) {
            return Err(CoreError::FrameOutOfRange { frame });
        }
        let content = self.require(id)?.content.clone();
        if !content.accepts(field) {
            return Err(CoreError::WrongContentField {
                kind: content.kind_name(),
                field: field.name(),
            });
        }
        if !field.is_animatable() {
            return Err(CoreError::WrongContentField {
                kind: content.kind_name(),
                field: field.name(),
            });
        }
        content_key_write(
            &mut self.nodes[id.index()].content,
            field,
            frame,
            value,
            ease,
        )
    }

    /// Removes the keyframe at `frame` from a content field, if there is one.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::UnknownNode`] for an unknown ID,
    /// [`CoreError::WrongContentField`] for a field the kind does not have or
    /// cannot animate, and [`CoreError::FrameOutOfRange`] for a frame outside
    /// the allowed range.
    pub fn remove_content_keyframe(
        &mut self,
        id: NodeId,
        field: ContentField,
        frame: Frame,
    ) -> Result<(), CoreError> {
        if !frame_within_bounds(frame) {
            return Err(CoreError::FrameOutOfRange { frame });
        }
        let content = self.require(id)?.content.clone();
        if !content.accepts(field) || !field.is_animatable() {
            return Err(CoreError::WrongContentField {
                kind: content.kind_name(),
                field: field.name(),
            });
        }
        content_key_remove(&mut self.nodes[id.index()].content, field, frame);
        Ok(())
    }

    /// Moves `child` to the end of `parent`'s children.
    ///
    /// # Errors
    ///
    /// Fails for unknown IDs, a non-group parent, or a move that would make
    /// a node its own ancestor.
    pub fn reparent(&mut self, child: NodeId, parent: NodeId) -> Result<(), CoreError> {
        self.require(child)?;
        let parent_node = self.require(parent)?;
        if !matches!(parent_node.content, NodeContent::Group) {
            return Err(CoreError::NotAGroup(parent));
        }
        if self.is_ancestor_or_self(child, parent) {
            return Err(CoreError::CyclicParent { child, parent });
        }
        match self.nodes[child.index()].parent {
            Some(old) => self.nodes[old.index()].children.retain(|&c| c != child),
            None => self.roots.retain(|&c| c != child),
        }
        self.nodes[parent.index()].children.push(child);
        self.nodes[child.index()].parent = Some(parent);
        Ok(())
    }

    /// Sets the static value of a property.
    ///
    /// `Scale` accepts a scalar (uniform) or a vector; `Position` and
    /// `Anchor` require vectors; `Rotation` and `Opacity` require scalars.
    ///
    /// # Errors
    ///
    /// Fails for an unknown node, a value of the wrong shape, a non-finite
    /// number, or a finite value outside the allowed range of the property
    /// (see [`crate::bounds`]).
    pub fn set_property(
        &mut self,
        id: NodeId,
        property: Property,
        value: PropertyValue,
    ) -> Result<(), CoreError> {
        self.write_property(id, property, value, Write::Static)
    }

    /// Adds (or replaces) a keyframe on a property. Value rules are the same
    /// as [`Document::set_property`].
    ///
    /// # Errors
    ///
    /// Same as [`Document::set_property`], plus
    /// [`CoreError::FrameOutOfRange`] when `frame` falls outside
    /// `[-MAX_ABS_FRAME, MAX_ABS_FRAME]`.
    pub fn add_keyframe(
        &mut self,
        id: NodeId,
        property: Property,
        frame: Frame,
        value: PropertyValue,
        ease: Ease,
    ) -> Result<(), CoreError> {
        if !frame_within_bounds(frame) {
            return Err(CoreError::FrameOutOfRange { frame });
        }
        self.write_property(id, property, value, Write::Key(frame, ease))
    }

    /// Sets one axis of a vector property's static value.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::UnknownNode`] for an unknown ID,
    /// [`CoreError::WrongAxis`] for a scalar property, and the same value
    /// errors as [`Self::set_property`].
    pub fn set_property_axis(
        &mut self,
        id: NodeId,
        property: Property,
        axis: Axis,
        value: f64,
    ) -> Result<(), CoreError> {
        self.require(id)?;
        if !property.is_vector() {
            return Err(CoreError::WrongAxis {
                property: property.name(),
                axis: Some(axis.name()),
            });
        }
        let value = bounds::check_value(property, value)?;
        self.vector_mut(id, property)?.set_axis_value(axis, value);
        Ok(())
    }

    /// Adds or replaces one axis's keyframe.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::UnknownNode`] for an unknown ID,
    /// [`CoreError::WrongAxis`] for a scalar property, and the same value and
    /// frame errors as [`Self::add_keyframe`].
    pub fn add_axis_keyframe(
        &mut self,
        id: NodeId,
        property: Property,
        axis: Axis,
        frame: Frame,
        value: f64,
        ease: Ease,
    ) -> Result<(), CoreError> {
        if !frame_within_bounds(frame) {
            return Err(CoreError::FrameOutOfRange { frame });
        }
        self.require(id)?;
        if !property.is_vector() {
            return Err(CoreError::WrongAxis {
                property: property.name(),
                axis: Some(axis.name()),
            });
        }
        let value = bounds::check_value(property, value)?;
        self.vector_mut(id, property)?
            .insert_axis_key(axis, frame, value, ease);
        Ok(())
    }

    /// Removes one axis's keyframe at `frame`, if there is one.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::UnknownNode`] for an unknown ID,
    /// [`CoreError::WrongAxis`] for a scalar property, and
    /// [`CoreError::FrameOutOfRange`] for a frame outside the allowed range.
    pub fn remove_axis_keyframe(
        &mut self,
        id: NodeId,
        property: Property,
        axis: Axis,
        frame: Frame,
    ) -> Result<(), CoreError> {
        if !frame_within_bounds(frame) {
            return Err(CoreError::FrameOutOfRange { frame });
        }
        self.require(id)?;
        if !property.is_vector() {
            return Err(CoreError::WrongAxis {
                property: property.name(),
                axis: Some(axis.name()),
            });
        }
        self.vector_mut(id, property)?.remove_axis_key(axis, frame);
        Ok(())
    }

    fn vector_mut(
        &mut self,
        id: NodeId,
        property: Property,
    ) -> Result<&mut AnimatableVec2, CoreError> {
        let transform = &mut self.nodes[id.index()].transform;
        Ok(match property {
            Property::Position => &mut transform.position,
            Property::Scale => &mut transform.scale,
            Property::Anchor => &mut transform.anchor,
            Property::Rotation | Property::Opacity => {
                return Err(CoreError::WrongAxis {
                    property: property.name(),
                    axis: None,
                })
            }
        })
    }

    /// Removes the keyframe at `frame` from a property, if one exists.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::UnknownNode`] for an unknown ID and
    /// [`CoreError::FrameOutOfRange`] for a frame outside the allowed range.
    pub fn remove_keyframe(
        &mut self,
        id: NodeId,
        property: Property,
        frame: Frame,
    ) -> Result<(), CoreError> {
        if !frame_within_bounds(frame) {
            return Err(CoreError::FrameOutOfRange { frame });
        }
        self.require(id)?;
        let transform = &mut self.nodes[id.index()].transform;
        match property {
            Property::Position => transform.position.remove_key(frame),
            Property::Scale => transform.scale.remove_key(frame),
            Property::Rotation => drop(transform.rotation.remove_key(frame)),
            Property::Anchor => transform.anchor.remove_key(frame),
            Property::Opacity => drop(transform.opacity.remove_key(frame)),
        }
        Ok(())
    }

    /// Sets a node's blend mode.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::UnknownNode`] for an unknown ID.
    pub fn set_blend(&mut self, id: NodeId, blend: BlendMode) -> Result<(), CoreError> {
        self.require(id)?;
        self.nodes[id.index()].blend = blend;
        Ok(())
    }

    fn require(&self, id: NodeId) -> Result<&Node, CoreError> {
        self.node(id).ok_or(CoreError::UnknownNode(id))
    }

    fn index_identity(&mut self, id: NodeId) {
        let key = self.nodes[id.index()].identity.as_str().to_owned();
        self.identities.insert(key, id);
    }

    fn is_ancestor_or_self(&self, ancestor: NodeId, mut node: NodeId) -> bool {
        loop {
            if node == ancestor {
                return true;
            }
            match self.nodes[node.index()].parent {
                Some(parent) => node = parent,
                None => return false,
            }
        }
    }

    fn write_property(
        &mut self,
        id: NodeId,
        property: Property,
        value: PropertyValue,
        write: Write,
    ) -> Result<(), CoreError> {
        self.require(id)?;
        let transform = &mut self.nodes[id.index()].transform;
        match property {
            Property::Position => {
                apply_vec(&mut transform.position, vector(property, value)?, write);
            }
            Property::Anchor => apply_vec(&mut transform.anchor, vector(property, value)?, write),
            Property::Scale => apply_vec(&mut transform.scale, scale(value)?, write),
            Property::Rotation => apply(&mut transform.rotation, scalar(property, value)?, write),
            Property::Opacity => apply(&mut transform.opacity, scalar(property, value)?, write),
        }
        Ok(())
    }
}

fn apply_vec(target: &mut crate::animation::AnimatableVec2, value: Vec2, write: Write) {
    match write {
        Write::Static => target.set_value(value),
        Write::Key(frame, ease) => target.insert_key(frame, value, ease),
    }
}

fn apply<T: Copy>(target: &mut Animatable<T>, value: T, write: Write) {
    match write {
        Write::Static => target.set_value(value),
        Write::Key(frame, ease) => target.insert_key(Keyframe { frame, value, ease }),
    }
}

/// Writes a static content value into the matching animatable channel.
#[allow(clippy::too_many_lines)] // one arm per content field, kept together
fn content_static_write(
    content: &mut NodeContent,
    field: ContentField,
    value: ContentValue,
) -> Result<(), CoreError> {
    match (field, value) {
        (ContentField::Width, ContentValue::Number(v)) => match content {
            NodeContent::Rect { width, .. } => set_number(width, field, v, false),
            _ => Err(wrong_kind(content, field)),
        },
        (ContentField::Height, ContentValue::Number(v)) => match content {
            NodeContent::Rect { height, .. } => set_number(height, field, v, false),
            _ => Err(wrong_kind(content, field)),
        },
        (ContentField::Radius, ContentValue::Number(v)) => match content {
            NodeContent::Rect { radius, .. } | NodeContent::Circle { radius, .. } => {
                set_number(radius, field, v, false)
            }
            _ => Err(wrong_kind(content, field)),
        },
        (ContentField::StrokeWidth, ContentValue::Number(v)) => match content {
            NodeContent::Rect { paint, .. }
            | NodeContent::Circle { paint, .. }
            | NodeContent::Path { paint, .. } => {
                set_number(&mut paint.stroke_width, field, v, false)
            }
            _ => Err(wrong_kind(content, field)),
        },
        (ContentField::Size, ContentValue::Number(v)) => match content {
            NodeContent::Text(text) => set_number(&mut text.size, field, v, true),
            _ => Err(wrong_kind(content, field)),
        },
        (
            ContentField::TrimStart
            | ContentField::TrimEnd
            | ContentField::TrimOffset
            | ContentField::Dash
            | ContentField::DashOffset,
            ContentValue::Number(v),
        ) => match content {
            NodeContent::Path {
                trim_start,
                trim_end,
                trim_offset,
                dash,
                dash_offset,
                ..
            } => {
                let channel = match field {
                    ContentField::TrimStart => trim_start,
                    ContentField::TrimEnd => trim_end,
                    ContentField::TrimOffset => trim_offset,
                    ContentField::Dash => dash,
                    _ => dash_offset,
                };
                set_number(channel, field, v, false)
            }
            _ => Err(wrong_kind(content, field)),
        },
        (ContentField::Cap, ContentValue::Cap(cap)) => match content {
            NodeContent::Path { geometry, .. } => {
                geometry.set_cap(cap);
                Ok(())
            }
            _ => Err(wrong_kind(content, field)),
        },
        (ContentField::Join, ContentValue::Join(join)) => match content {
            NodeContent::Path { geometry, .. } => {
                geometry.set_join(join);
                Ok(())
            }
            _ => Err(wrong_kind(content, field)),
        },
        (ContentField::Fill, ContentValue::Color(color)) => match content {
            NodeContent::Rect { paint, .. }
            | NodeContent::Circle { paint, .. }
            | NodeContent::Path { paint, .. } => {
                paint.fill.set_value(color);
                Ok(())
            }
            _ => Err(wrong_kind(content, field)),
        },
        (ContentField::Stroke, ContentValue::Color(color)) => match content {
            NodeContent::Rect { paint, .. }
            | NodeContent::Circle { paint, .. }
            | NodeContent::Path { paint, .. } => {
                paint.stroke.set_value(color);
                Ok(())
            }
            _ => Err(wrong_kind(content, field)),
        },
        (ContentField::Color, ContentValue::Color(Some(color))) => match content {
            NodeContent::Text(text) => {
                text.color.set_value(color);
                Ok(())
            }
            _ => Err(wrong_kind(content, field)),
        },
        (ContentField::Text, ContentValue::Text(text_value)) => match content {
            NodeContent::Text(text) => {
                text.text = text_value;
                Ok(())
            }
            _ => Err(wrong_kind(content, field)),
        },
        (ContentField::Align, ContentValue::Align(align)) => match content {
            NodeContent::Text(text) => {
                text.align = align;
                Ok(())
            }
            _ => Err(wrong_kind(content, field)),
        },
        _ => Err(CoreError::InvalidContentValue {
            field: field.name(),
            reason: "wrong value type",
        }),
    }
}

/// Writes one content keyframe into the matching animatable channel.
fn content_key_write(
    content: &mut NodeContent,
    field: ContentField,
    frame: Frame,
    value: ContentValue,
    ease: Ease,
) -> Result<(), CoreError> {
    let key = |value: f64, positive: bool| -> Result<Keyframe<f64>, CoreError> {
        check_number(field, value, positive)?;
        Ok(Keyframe { frame, value, ease })
    };
    match (field, value) {
        (ContentField::Width, ContentValue::Number(v)) => match content {
            NodeContent::Rect { width, .. } => width.insert_key(key(v, false)?),
            _ => return Err(wrong_kind(content, field)),
        },
        (ContentField::Height, ContentValue::Number(v)) => match content {
            NodeContent::Rect { height, .. } => height.insert_key(key(v, false)?),
            _ => return Err(wrong_kind(content, field)),
        },
        (ContentField::Radius, ContentValue::Number(v)) => match content {
            NodeContent::Rect { radius, .. } | NodeContent::Circle { radius, .. } => {
                radius.insert_key(key(v, false)?);
            }
            _ => return Err(wrong_kind(content, field)),
        },
        (ContentField::StrokeWidth, ContentValue::Number(v)) => match content {
            NodeContent::Rect { paint, .. }
            | NodeContent::Circle { paint, .. }
            | NodeContent::Path { paint, .. } => {
                paint.stroke_width.insert_key(key(v, false)?);
            }
            _ => return Err(wrong_kind(content, field)),
        },
        (ContentField::Size, ContentValue::Number(v)) => match content {
            NodeContent::Text(text) => text.size.insert_key(key(v, true)?),
            _ => return Err(wrong_kind(content, field)),
        },
        (
            ContentField::TrimStart
            | ContentField::TrimEnd
            | ContentField::TrimOffset
            | ContentField::Dash
            | ContentField::DashOffset,
            ContentValue::Number(v),
        ) => match content {
            NodeContent::Path {
                trim_start,
                trim_end,
                trim_offset,
                dash,
                dash_offset,
                ..
            } => {
                let channel = match field {
                    ContentField::TrimStart => trim_start,
                    ContentField::TrimEnd => trim_end,
                    ContentField::TrimOffset => trim_offset,
                    ContentField::Dash => dash,
                    _ => dash_offset,
                };
                channel.insert_key(key(v, false)?);
            }
            _ => return Err(wrong_kind(content, field)),
        },
        (ContentField::Fill, ContentValue::Color(color)) => match content {
            NodeContent::Rect { paint, .. }
            | NodeContent::Circle { paint, .. }
            | NodeContent::Path { paint, .. } => {
                paint.fill.insert_key(Keyframe {
                    frame,
                    value: color,
                    ease,
                });
            }
            _ => return Err(wrong_kind(content, field)),
        },
        (ContentField::Stroke, ContentValue::Color(color)) => match content {
            NodeContent::Rect { paint, .. }
            | NodeContent::Circle { paint, .. }
            | NodeContent::Path { paint, .. } => {
                paint.stroke.insert_key(Keyframe {
                    frame,
                    value: color,
                    ease,
                });
            }
            _ => return Err(wrong_kind(content, field)),
        },
        (ContentField::Color, ContentValue::Color(Some(color))) => match content {
            NodeContent::Text(text) => text.color.insert_key(Keyframe {
                frame,
                value: color,
                ease,
            }),
            _ => return Err(wrong_kind(content, field)),
        },
        _ => {
            return Err(CoreError::InvalidContentValue {
                field: field.name(),
                reason: "wrong value type",
            })
        }
    }
    Ok(())
}

/// Removes one content keyframe.
fn content_key_remove(content: &mut NodeContent, field: ContentField, frame: Frame) {
    let remove = |value: &mut Animatable<f64>| {
        value.remove_key(frame);
    };
    match (content, field) {
        (NodeContent::Rect { width, .. }, ContentField::Width) => remove(width),
        (NodeContent::Rect { height, .. }, ContentField::Height) => remove(height),
        (
            NodeContent::Rect { radius, .. } | NodeContent::Circle { radius, .. },
            ContentField::Radius,
        ) => remove(radius),
        (
            NodeContent::Rect { paint, .. }
            | NodeContent::Circle { paint, .. }
            | NodeContent::Path { paint, .. },
            ContentField::StrokeWidth,
        ) => remove(&mut paint.stroke_width),
        (NodeContent::Text(text), ContentField::Size) => remove(&mut text.size),
        (
            NodeContent::Path {
                trim_start,
                trim_end,
                trim_offset,
                dash,
                dash_offset,
                ..
            },
            ContentField::TrimStart
            | ContentField::TrimEnd
            | ContentField::TrimOffset
            | ContentField::Dash
            | ContentField::DashOffset,
        ) => {
            let channel = match field {
                ContentField::TrimStart => trim_start,
                ContentField::TrimEnd => trim_end,
                ContentField::TrimOffset => trim_offset,
                ContentField::Dash => dash,
                _ => dash_offset,
            };
            remove(channel);
        }
        (
            NodeContent::Rect { paint, .. }
            | NodeContent::Circle { paint, .. }
            | NodeContent::Path { paint, .. },
            ContentField::Fill,
        ) => {
            paint.fill.remove_key(frame);
        }
        (
            NodeContent::Rect { paint, .. }
            | NodeContent::Circle { paint, .. }
            | NodeContent::Path { paint, .. },
            ContentField::Stroke,
        ) => {
            paint.stroke.remove_key(frame);
        }
        (NodeContent::Text(text), ContentField::Color) => {
            text.color.remove_key(frame);
        }
        _ => {}
    }
}

/// Writes a finite, optionally positive number to a content channel.
fn set_number(
    target: &mut Animatable<f64>,
    field: ContentField,
    value: f64,
    positive: bool,
) -> Result<(), CoreError> {
    check_number(field, value, positive)?;
    target.set_value(value);
    Ok(())
}

fn check_number(field: ContentField, value: f64, positive: bool) -> Result<(), CoreError> {
    let (ok, reason) = if matches!(
        field,
        ContentField::TrimStart | ContentField::TrimEnd | ContentField::TrimOffset
    ) {
        (
            value.is_finite() && (0.0..=1.0).contains(&value),
            "must be a finite number from 0 to 1",
        )
    } else if positive {
        (
            value.is_finite() && value > 0.0,
            "must be a finite number above zero",
        )
    } else {
        (
            value.is_finite() && value >= 0.0,
            "must be a finite number zero or above",
        )
    };
    if !ok {
        return Err(CoreError::InvalidContentValue {
            field: field.name(),
            reason,
        });
    }
    Ok(())
}

fn wrong_kind(content: &NodeContent, field: ContentField) -> CoreError {
    CoreError::WrongContentField {
        kind: content.kind_name(),
        field: field.name(),
    }
}

fn scalar(property: Property, value: PropertyValue) -> Result<f64, CoreError> {
    match value {
        PropertyValue::Scalar(v) => bounds::check_value(property, v),
        PropertyValue::Vec2(_) => Err(CoreError::PropertyType {
            property,
            expected: "a number",
        }),
    }
}

fn vector(property: Property, value: PropertyValue) -> Result<Vec2, CoreError> {
    match value {
        PropertyValue::Vec2(v) => Ok(Vec2::new(
            bounds::check_value(property, v.x)?,
            bounds::check_value(property, v.y)?,
        )),
        PropertyValue::Scalar(_) => Err(CoreError::PropertyType {
            property,
            expected: "an (x, y) pair",
        }),
    }
}

fn scale(value: PropertyValue) -> Result<Vec2, CoreError> {
    match value {
        PropertyValue::Scalar(v) => Ok(Vec2::splat(bounds::check_value(Property::Scale, v)?)),
        PropertyValue::Vec2(_) => vector(Property::Scale, value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::Paint;
    use crate::MAX_ABS_FRAME;

    fn document() -> Document {
        let fps = FrameRate::new(60, 1).unwrap();
        Document::new(Composition::new("Test", 320, 240, fps, 60, Color::BLACK).unwrap())
    }

    fn dot() -> NodeContent {
        NodeContent::circle(
            4.0,
            Paint {
                fill: Some(Color::WHITE),
                stroke: None,
            },
        )
    }

    #[test]
    fn rejects_invalid_composition() {
        let fps = FrameRate::new(60, 1).unwrap();
        assert!(Composition::new("x", 0, 10, fps, 1, Color::BLACK).is_err());
        assert!(Composition::new("x", 10, 10, fps, 0, Color::BLACK).is_err());
        assert!(Composition::new("x", MAX_CANVAS_SIZE + 1, 10, fps, 1, Color::BLACK).is_err());
    }

    #[test]
    fn nodes_get_sequential_ids_and_root_order() {
        let mut doc = document();
        let a = doc.add_node(dot()).unwrap();
        let b = doc.add_node(dot()).unwrap();
        assert_eq!((a.raw(), b.raw()), (0, 1));
        assert_eq!(doc.roots(), &[a, b]);
    }

    #[test]
    fn rejects_negative_geometry() {
        let mut doc = document();
        let bad = NodeContent::circle(
            -1.0,
            Paint {
                fill: None,
                stroke: None,
            },
        );
        assert_eq!(
            doc.add_node(bad),
            Err(CoreError::InvalidGeometry { field: "radius" })
        );
    }

    #[test]
    fn reparent_moves_node_and_rejects_cycles() {
        let mut doc = document();
        let outer = doc.add_node(NodeContent::Group).unwrap();
        let inner = doc.add_node(NodeContent::Group).unwrap();
        let leaf = doc.add_node(dot()).unwrap();
        doc.reparent(inner, outer).unwrap();
        doc.reparent(leaf, inner).unwrap();
        assert_eq!(doc.roots(), &[outer]);
        assert_eq!(doc.node(inner).unwrap().children(), &[leaf]);
        assert!(matches!(
            doc.reparent(outer, inner),
            Err(CoreError::CyclicParent { .. })
        ));
        assert_eq!(doc.reparent(outer, leaf), Err(CoreError::NotAGroup(leaf)));
    }

    #[test]
    fn property_writes_check_shapes() {
        let mut doc = document();
        let id = doc.add_node(dot()).unwrap();
        doc.set_property(id, Property::Scale, PropertyValue::Scalar(2.0))
            .unwrap();
        assert_eq!(
            doc.node(id).unwrap().transform().scale.value(),
            Vec2::splat(2.0)
        );
        assert!(doc
            .set_property(id, Property::Position, PropertyValue::Scalar(1.0))
            .is_err());
        assert!(doc
            .set_property(id, Property::Opacity, PropertyValue::Scalar(f64::NAN))
            .is_err());
        doc.add_keyframe(
            id,
            Property::Opacity,
            30,
            PropertyValue::Scalar(1.0),
            Ease::QuadOut,
        )
        .unwrap();
        assert_eq!(doc.node(id).unwrap().transform().opacity.keys().len(), 1);
    }

    #[test]
    fn add_keyframe_rejects_out_of_range_frames() {
        let mut doc = document();
        let id = doc.add_node(dot()).unwrap();
        for frame in [
            MAX_ABS_FRAME + 1,
            -(MAX_ABS_FRAME + 1),
            Frame::MAX,
            Frame::MIN,
        ] {
            assert_eq!(
                doc.add_keyframe(
                    id,
                    Property::Rotation,
                    frame,
                    PropertyValue::Scalar(0.0),
                    Ease::Linear,
                ),
                Err(CoreError::FrameOutOfRange { frame })
            );
        }
        doc.add_keyframe(
            id,
            Property::Rotation,
            MAX_ABS_FRAME,
            PropertyValue::Scalar(0.0),
            Ease::Linear,
        )
        .unwrap();
        doc.add_keyframe(
            id,
            Property::Rotation,
            -MAX_ABS_FRAME,
            PropertyValue::Scalar(0.0),
            Ease::Linear,
        )
        .unwrap();
        assert_eq!(doc.node(id).unwrap().transform().rotation.keys().len(), 2);
    }

    #[test]
    fn write_property_accepts_exact_bounds() {
        let mut doc = document();
        let id = doc.add_node(dot()).unwrap();
        let pos = Vec2::new(bounds::MAX_POSITION, -bounds::MAX_POSITION);
        doc.set_property(id, Property::Position, PropertyValue::Vec2(pos))
            .unwrap();
        doc.set_property(id, Property::Anchor, PropertyValue::Vec2(pos))
            .unwrap();
        doc.set_property(
            id,
            Property::Rotation,
            PropertyValue::Scalar(bounds::MAX_ROTATION),
        )
        .unwrap();
        let scale = Vec2::new(bounds::MAX_SCALE, -bounds::MAX_SCALE);
        doc.set_property(id, Property::Scale, PropertyValue::Vec2(scale))
            .unwrap();
        doc.set_property(
            id,
            Property::Opacity,
            PropertyValue::Scalar(bounds::MIN_OPACITY),
        )
        .unwrap();
        doc.set_property(
            id,
            Property::Opacity,
            PropertyValue::Scalar(bounds::MAX_OPACITY),
        )
        .unwrap();
    }

    #[test]
    fn write_property_rejects_out_of_bounds_values() {
        let mut doc = document();
        let id = doc.add_node(dot()).unwrap();
        let too_far = bounds::MAX_POSITION + 1.0;
        let cases = [
            (
                Property::Position,
                PropertyValue::Vec2(Vec2::new(too_far, 0.0)),
            ),
            (
                Property::Anchor,
                PropertyValue::Vec2(Vec2::new(too_far, 0.0)),
            ),
            (
                Property::Rotation,
                PropertyValue::Scalar(bounds::MAX_ROTATION + 1.0),
            ),
            (
                Property::Scale,
                PropertyValue::Vec2(Vec2::new(bounds::MAX_SCALE + 1.0, 0.0)),
            ),
            (
                Property::Opacity,
                PropertyValue::Scalar(bounds::MAX_OPACITY + 0.5),
            ),
            (
                Property::Opacity,
                PropertyValue::Scalar(bounds::MIN_OPACITY - 0.5),
            ),
        ];
        for (property, value) in cases {
            assert!(
                matches!(
                    doc.set_property(id, property, value),
                    Err(CoreError::ValueOutOfRange { property: p, .. }) if p == property
                ),
                "{property:?} should reject {value:?}"
            );
        }
    }

    #[test]
    fn write_property_rejects_out_of_bounds_keyframes() {
        let mut doc = document();
        let id = doc.add_node(dot()).unwrap();
        assert!(matches!(
            doc.add_keyframe(
                id,
                Property::Scale,
                0,
                PropertyValue::Scalar(bounds::MAX_SCALE + 1.0),
                Ease::Linear,
            ),
            Err(CoreError::ValueOutOfRange {
                property: Property::Scale,
                ..
            })
        ));
    }

    fn rect_doc() -> (Document, crate::node::NodeId) {
        let mut doc = document();
        let id = doc
            .add_node(NodeContent::rect(
                10.0,
                10.0,
                0.0,
                Paint {
                    fill: Some(Color::WHITE),
                    stroke: None,
                },
            ))
            .unwrap();
        (doc, id)
    }

    #[test]
    fn content_channels_key_and_unkey() {
        let (mut doc, id) = rect_doc();
        let red = Color::parse_hex("#FF0000").unwrap();
        let green = Color::parse_hex("#00FF00").unwrap();
        doc.add_content_keyframe(
            id,
            ContentField::Fill,
            0,
            ContentValue::Color(Some(red)),
            Ease::Linear,
        )
        .unwrap();
        doc.add_content_keyframe(
            id,
            ContentField::Fill,
            30,
            ContentValue::Color(Some(green)),
            Ease::QuadOut,
        )
        .unwrap();
        doc.add_content_keyframe(
            id,
            ContentField::Width,
            30,
            ContentValue::Number(40.0),
            Ease::Linear,
        )
        .unwrap();
        let content = doc.node(id).unwrap().content();
        assert_eq!(
            content.channel(ContentField::Fill).unwrap().key_frames(),
            vec![0, 30]
        );
        assert_eq!(
            content.channel(ContentField::Width).unwrap().key_frames(),
            vec![30]
        );
        assert!(content.channel(ContentField::Text).is_none());
        doc.remove_content_keyframe(id, ContentField::Fill, 30)
            .unwrap();
        assert_eq!(
            doc.node(id)
                .unwrap()
                .content()
                .channel(ContentField::Fill)
                .unwrap()
                .key_frames(),
            vec![0]
        );
    }

    #[test]
    fn content_keyframes_reject_invalid_values_and_discrete_fields() {
        let (mut doc, id) = rect_doc();
        assert!(matches!(
            doc.add_content_keyframe(
                id,
                ContentField::Width,
                0,
                ContentValue::Number(-1.0),
                Ease::Linear
            ),
            Err(CoreError::InvalidContentValue { field: "width", .. })
        ));
        assert!(matches!(
            doc.add_content_keyframe(
                id,
                ContentField::Text,
                0,
                ContentValue::Text("x".to_owned()),
                Ease::Linear
            ),
            Err(CoreError::WrongContentField { .. })
        ));
        assert!(matches!(
            doc.add_content_keyframe(
                id,
                ContentField::Width,
                MAX_ABS_FRAME + 1,
                ContentValue::Number(1.0),
                Ease::Linear
            ),
            Err(CoreError::FrameOutOfRange { .. })
        ));
    }
}

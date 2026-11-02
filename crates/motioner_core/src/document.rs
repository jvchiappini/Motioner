//! The composition and its node tree, plus the validated mutation API.

use std::collections::HashMap;

use crate::animation::{Animatable, AnimatableVec2, Axis, Ease, Keyframe};
use crate::bounds;
use crate::color::Color;
use crate::effect::{Effect, EffectChannel, EffectChannelMut, EffectKind, EffectValueInput};
use crate::error::CoreError;
use crate::gradient::{PaintSlot, StopPart};
use crate::math::Vec2;
use crate::node::{
    BlendMode, ContentField, ContentValue, Node, NodeContent, NodeId, NodeIdentity, PaintSpec,
    Property, PropertyValue, Transform,
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
            effects: Vec::new(),
            mask: None,
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
            (node.content.kind_name(), node.content.field_exists(field))
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
        if !content.field_exists(field) {
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
        if !content.field_exists(field) || !field.is_animatable() {
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

    /// Masks `id` with the drawn subtree of `source` (D-0033).
    ///
    /// The source node is hidden from the normal draw stream and evaluated in
    /// world space with its own animated transform; when it is a child of `id`
    /// it inherits `id`'s transform and moves with it.  A node cannot mask
    /// itself, cannot be masked by one of its ancestors, and the mask graph may
    /// not contain a cycle.
    ///
    /// # Errors
    ///
    /// [`CoreError::UnknownNode`] for either ID, [`CoreError::MaskSelfReference`],
    /// [`CoreError::MaskSourceIsAncestor`] and [`CoreError::MaskCycle`].
    pub fn set_mask(
        &mut self,
        id: NodeId,
        source: NodeId,
        mode: crate::mask::MaskMode,
        invert: bool,
    ) -> Result<(), CoreError> {
        if let Some(error) = self.mask_problem(id, source) {
            return Err(error);
        }
        self.nodes[id.index()].mask = Some(crate::mask::MaskSpec {
            source,
            mode,
            invert,
        });
        Ok(())
    }

    /// Removes the mask of `id`, if it has one.
    ///
    /// # Errors
    ///
    /// [`CoreError::UnknownNode`] for an unknown ID.
    pub fn clear_mask(&mut self, id: NodeId) -> Result<(), CoreError> {
        self.require(id)?;
        self.nodes[id.index()].mask = None;
        Ok(())
    }

    /// Whether `source` may mask `node` (the rule behind [`Self::set_mask`]).
    #[must_use]
    pub fn can_mask(&self, node: NodeId, source: NodeId) -> bool {
        self.mask_problem(node, source).is_none()
    }

    /// Whether any node uses `id` as its mask source.
    ///
    /// Evaluation skips these nodes in the normal draw stream: they are drawn
    /// only as the mask of the nodes that reference them.
    #[must_use]
    pub fn is_mask_source(&self, id: NodeId) -> bool {
        self.nodes
            .iter()
            .any(|node| node.mask.is_some_and(|mask| mask.source == id))
    }

    /// The reason `source` may not mask `node`, if any.
    fn mask_problem(&self, node: NodeId, source: NodeId) -> Option<CoreError> {
        if self.node(node).is_none() {
            return Some(CoreError::UnknownNode(node));
        }
        if self.node(source).is_none() {
            return Some(CoreError::UnknownNode(source));
        }
        if source == node {
            return Some(CoreError::MaskSelfReference { node });
        }
        if self.is_ancestor_or_self(source, node) {
            return Some(CoreError::MaskSourceIsAncestor { node, source });
        }
        // A cycle appears when following the source's own mask chain reaches
        // the node being masked.
        let mut cursor = self.nodes[source.index()].mask.map(|mask| mask.source);
        for _ in 0..=self.nodes.len() {
            match cursor {
                Some(next) if next == node => return Some(CoreError::MaskCycle { node, source }),
                Some(next) => cursor = self.nodes[next.index()].mask.map(|mask| mask.source),
                None => return None,
            }
        }
        Some(CoreError::MaskCycle { node, source })
    }

    /// Adds an effect of `kind` with default parameters at the top of the
    /// stack.
    ///
    /// # Errors
    ///
    /// [`CoreError::UnknownNode`] for an unknown ID and
    /// [`CoreError::DuplicateEffect`] when the node already has that kind.
    pub fn add_effect(&mut self, id: NodeId, kind: EffectKind) -> Result<(), CoreError> {
        self.push_effect(id, Effect::new(kind))
    }

    /// Adds a prepared effect at the top of the stack.
    ///
    /// # Errors
    ///
    /// Same as [`Document::add_effect`].
    pub fn push_effect(&mut self, id: NodeId, effect: Effect) -> Result<(), CoreError> {
        self.require(id)?;
        let kind = effect.kind();
        if self.nodes[id.index()].effect(kind).is_some() {
            return Err(CoreError::DuplicateEffect { kind: kind.name() });
        }
        self.nodes[id.index()].effects.push(effect);
        Ok(())
    }

    /// Replaces the effect stack with `kinds`, preserving the parameters of
    /// effects that stay.
    ///
    /// Effects whose kind is absent from `kinds` are dropped; effects that
    /// appear only in `kinds` are added with their defaults.  This is what an
    /// `effects` override writes, so reordering in the editor survives a
    /// reload.
    ///
    /// # Errors
    ///
    /// [`CoreError::UnknownNode`] for an unknown ID and
    /// [`CoreError::DuplicateEffect`] when `kinds` repeats a kind.
    pub fn set_effect_stack(&mut self, id: NodeId, kinds: &[EffectKind]) -> Result<(), CoreError> {
        self.require(id)?;
        let mut existing = std::mem::take(&mut self.nodes[id.index()].effects);
        let mut next = Vec::with_capacity(kinds.len());
        for (position, kind) in kinds.iter().enumerate() {
            if kinds[..position].contains(kind) {
                return Err(CoreError::DuplicateEffect { kind: kind.name() });
            }
            let effect = existing
                .iter()
                .position(|effect| effect.is_kind(*kind))
                .map_or_else(|| Effect::new(*kind), |index| existing.remove(index));
            next.push(effect);
        }
        self.nodes[id.index()].effects = next;
        Ok(())
    }

    /// Removes the effect of `kind`.
    ///
    /// # Errors
    ///
    /// [`CoreError::UnknownNode`] for an unknown ID and
    /// [`CoreError::MissingEffect`] when the node has no such effect.
    pub fn remove_effect(&mut self, id: NodeId, kind: EffectKind) -> Result<(), CoreError> {
        self.require(id)?;
        let effects = &mut self.nodes[id.index()].effects;
        let Some(index) = effects.iter().position(|effect| effect.is_kind(kind)) else {
            return Err(CoreError::MissingEffect { kind: kind.name() });
        };
        effects.remove(index);
        Ok(())
    }

    /// Moves the effect of `kind` to `index` in the stack.
    ///
    /// # Errors
    ///
    /// [`CoreError::UnknownNode`] for an unknown ID,
    /// [`CoreError::MissingEffect`] when the node has no such effect and
    /// [`CoreError::InvalidEffectOrder`] for an out-of-range index.
    pub fn move_effect(
        &mut self,
        id: NodeId,
        kind: EffectKind,
        index: usize,
    ) -> Result<(), CoreError> {
        self.require(id)?;
        let effects = &mut self.nodes[id.index()].effects;
        if index >= effects.len() {
            return Err(CoreError::InvalidEffectOrder {
                index,
                len: effects.len(),
            });
        }
        let Some(from) = effects.iter().position(|effect| effect.is_kind(kind)) else {
            return Err(CoreError::MissingEffect { kind: kind.name() });
        };
        let effect = effects.remove(from);
        effects.insert(index, effect);
        Ok(())
    }

    /// Sets the static value of an effect parameter.
    ///
    /// # Errors
    ///
    /// [`CoreError::UnknownNode`], [`CoreError::MissingEffect`] when the node
    /// has no effect of the channel's kind, [`CoreError::InvalidContentValue`]
    /// for a value of the wrong shape or out of range, and
    /// [`CoreError::WrongAxis`] for a missing or unexpected axis.
    pub fn set_effect(
        &mut self,
        id: NodeId,
        channel: EffectChannel,
        value: EffectValueInput,
    ) -> Result<(), CoreError> {
        self.write_effect(id, channel, value, Write::Static)
    }

    /// Adds (or replaces) a keyframe on an effect parameter.
    ///
    /// # Errors
    ///
    /// Same as [`Document::set_effect`], plus
    /// [`CoreError::FrameOutOfRange`] when `frame` is outside the allowed
    /// range.
    pub fn add_effect_keyframe(
        &mut self,
        id: NodeId,
        channel: EffectChannel,
        frame: Frame,
        value: EffectValueInput,
        ease: Ease,
    ) -> Result<(), CoreError> {
        self.write_effect(id, channel, value, Write::Key(frame, ease))
    }

    /// Removes an effect parameter's keyframe at `frame`, if any.
    ///
    /// # Errors
    ///
    /// [`CoreError::UnknownNode`], [`CoreError::MissingEffect`] and
    /// [`CoreError::FrameOutOfRange`].
    pub fn remove_effect_keyframe(
        &mut self,
        id: NodeId,
        channel: EffectChannel,
        frame: Frame,
    ) -> Result<(), CoreError> {
        if !frame_within_bounds(frame) {
            return Err(CoreError::FrameOutOfRange { frame });
        }
        self.require(id)?;
        let node = &mut self.nodes[id.index()];
        let Some(effect) = node
            .effects
            .iter_mut()
            .find(|effect| effect.is_kind(channel.kind))
        else {
            return Err(CoreError::MissingEffect {
                kind: channel.kind.name(),
            });
        };
        match effect.channel_mut(channel.param, channel.axis) {
            Some(EffectChannelMut::Number(animatable)) => {
                animatable.remove_key(frame);
                Ok(())
            }
            Some(EffectChannelMut::Color(animatable)) => {
                animatable.remove_key(frame);
                Ok(())
            }
            None => Err(CoreError::InvalidContentValue {
                field: channel.name(),
                reason: "not a parameter of this effect",
            }),
        }
    }

    fn write_effect(
        &mut self,
        id: NodeId,
        channel: EffectChannel,
        value: EffectValueInput,
        write: Write,
    ) -> Result<(), CoreError> {
        if let Write::Key(frame, _) = write {
            if !frame_within_bounds(frame) {
                return Err(CoreError::FrameOutOfRange { frame });
            }
        }
        self.require(id)?;
        check_effect_value(channel, value)?;
        let node = &mut self.nodes[id.index()];
        let Some(effect) = node
            .effects
            .iter_mut()
            .find(|effect| effect.is_kind(channel.kind))
        else {
            return Err(CoreError::MissingEffect {
                kind: channel.kind.name(),
            });
        };
        let Some(target) = effect.channel_mut(channel.param, channel.axis) else {
            return Err(CoreError::InvalidContentValue {
                field: channel.name(),
                reason: "not a parameter of this effect",
            });
        };
        match (target, value) {
            (EffectChannelMut::Number(animatable), EffectValueInput::Number(number)) => match write
            {
                Write::Static => animatable.set_value(number),
                Write::Key(frame, ease) => animatable.insert_key(Keyframe {
                    frame,
                    value: number,
                    ease,
                }),
            },
            (EffectChannelMut::Color(animatable), EffectValueInput::Color(color)) => match write {
                Write::Static => animatable.set_value(color),
                Write::Key(frame, ease) => animatable.insert_key(Keyframe {
                    frame,
                    value: color,
                    ease,
                }),
            },
            _ => {
                return Err(CoreError::InvalidContentValue {
                    field: channel.name(),
                    reason: "expected a number",
                })
            }
        }
        Ok(())
    }

    fn require(&self, id: NodeId) -> Result<&Node, CoreError> {
        self.node(id).ok_or(CoreError::UnknownNode(id))
    }

    fn index_identity(&mut self, id: NodeId) {
        let key = self.nodes[id.index()].identity.as_str().to_owned();
        self.identities.insert(key, id);
    }

    /// Whether `ancestor` is a strict ancestor of `node`.
    #[must_use]
    pub fn is_ancestor(&self, ancestor: NodeId, node: NodeId) -> bool {
        ancestor != node && self.is_ancestor_or_self(ancestor, node)
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
                paint.fill_gradient = None;
                Ok(())
            }
            _ => Err(wrong_kind(content, field)),
        },
        (ContentField::Stroke, ContentValue::Color(color)) => match content {
            NodeContent::Rect { paint, .. }
            | NodeContent::Circle { paint, .. }
            | NodeContent::Path { paint, .. } => {
                paint.stroke.set_value(color);
                paint.stroke_gradient = None;
                Ok(())
            }
            _ => Err(wrong_kind(content, field)),
        },
        (ContentField::Fill, ContentValue::Gradient(gradient)) => match content {
            NodeContent::Rect { paint, .. }
            | NodeContent::Circle { paint, .. }
            | NodeContent::Path { paint, .. } => set_gradient(paint, PaintSlot::Fill, gradient),
            _ => Err(wrong_kind(content, field)),
        },
        (ContentField::Stroke, ContentValue::Gradient(gradient)) => match content {
            NodeContent::Rect { paint, .. }
            | NodeContent::Circle { paint, .. }
            | NodeContent::Path { paint, .. } => set_gradient(paint, PaintSlot::Stroke, gradient),
            _ => Err(wrong_kind(content, field)),
        },
        (ContentField::Color, ContentValue::Gradient(gradient)) => match content {
            NodeContent::Text(text) => {
                if let Some(value) = &gradient {
                    text.color.set_value(value.fallback_color());
                }
                text.gradient = gradient;
                Ok(())
            }
            _ => Err(wrong_kind(content, field)),
        },
        (
            ContentField::FillStopOffset(_)
            | ContentField::FillStopColor(_)
            | ContentField::StrokeStopOffset(_)
            | ContentField::StrokeStopColor(_),
            value,
        ) => write_stop(content, field, value),
        (ContentField::Color, ContentValue::Color(Some(color))) => match content {
            NodeContent::Text(text) => {
                text.color.set_value(color);
                text.gradient = None;
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
#[allow(clippy::too_many_lines)] // one arm per content field, kept together
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
                paint.fill_gradient = None;
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
                paint.stroke_gradient = None;
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
        (
            ContentField::FillStopOffset(_)
            | ContentField::FillStopColor(_)
            | ContentField::StrokeStopOffset(_)
            | ContentField::StrokeStopColor(_),
            value,
        ) => key_stop(content, field, frame, value, ease)?,
        (
            ContentField::Fill | ContentField::Stroke | ContentField::Color,
            ContentValue::Gradient(_),
        ) => {
            return Err(CoreError::InvalidContentValue {
                field: field.name(),
                reason: "a gradient is animated through its stops, not as one value",
            })
        }
        _ => {
            return Err(CoreError::InvalidContentValue {
                field: field.name(),
                reason: "wrong value type",
            })
        }
    }
    Ok(())
}

/// Writes a static value into one gradient stop.
fn write_stop(
    content: &mut NodeContent,
    field: ContentField,
    value: ContentValue,
) -> Result<(), CoreError> {
    let (_, part, _) = field.stop().ok_or_else(|| CoreError::InvalidContentValue {
        field: field.name(),
        reason: "not a gradient stop field",
    })?;
    let stop = stop_mut(content, field).ok_or_else(|| CoreError::InvalidContentValue {
        field: field.name(),
        reason: "the layer has no gradient stop at this index",
    })?;
    match (part, value) {
        (StopPart::Offset, ContentValue::Number(value)) => {
            check_number(field, value, false)?;
            stop.offset.set_value(value);
            Ok(())
        }
        (StopPart::Color, ContentValue::Color(Some(color))) => {
            stop.color.set_value(color);
            Ok(())
        }
        _ => Err(CoreError::InvalidContentValue {
            field: field.name(),
            reason: "wrong value type for a gradient stop",
        }),
    }
}

/// Writes one keyframe into a gradient stop.
fn key_stop(
    content: &mut NodeContent,
    field: ContentField,
    frame: Frame,
    value: ContentValue,
    ease: Ease,
) -> Result<(), CoreError> {
    let (_, part, _) = field.stop().ok_or_else(|| CoreError::InvalidContentValue {
        field: field.name(),
        reason: "not a gradient stop field",
    })?;
    let stop = stop_mut(content, field).ok_or_else(|| CoreError::InvalidContentValue {
        field: field.name(),
        reason: "the layer has no gradient stop at this index",
    })?;
    match (part, value) {
        (StopPart::Offset, ContentValue::Number(value)) => {
            check_number(field, value, false)?;
            stop.offset.insert_key(Keyframe { frame, value, ease });
            Ok(())
        }
        (StopPart::Color, ContentValue::Color(Some(color))) => {
            stop.color.insert_key(Keyframe {
                frame,
                value: color,
                ease,
            });
            Ok(())
        }
        _ => Err(CoreError::InvalidContentValue {
            field: field.name(),
            reason: "wrong value type for a gradient stop",
        }),
    }
}

/// The mutable gradient stop a field addresses, when it exists.
fn stop_mut(content: &mut NodeContent, field: ContentField) -> Option<&mut crate::GradientStop> {
    let (slot, _, index) = field.stop()?;
    match content {
        NodeContent::Text(text) if slot == PaintSlot::Fill => {
            text.gradient.as_mut()?.stop_mut(index)
        }
        NodeContent::Rect { paint, .. }
        | NodeContent::Circle { paint, .. }
        | NodeContent::Path { paint, .. } => paint.gradient_mut(slot)?.stop_mut(index),
        _ => None,
    }
}

/// Sets or clears one paint slot's gradient, keeping a solid fallback.
fn set_gradient(
    paint: &mut PaintSpec,
    slot: PaintSlot,
    gradient: Option<crate::Gradient>,
) -> Result<(), CoreError> {
    if let Some(gradient) = &gradient {
        if let Some(reason) = gradient.invalid() {
            return Err(CoreError::InvalidGradient { reason });
        }
    }
    match slot {
        PaintSlot::Fill => {
            if let Some(gradient) = gradient {
                paint.fill.set_value(Some(gradient.fallback_color()));
                paint.fill_gradient = Some(gradient);
            } else {
                paint.fill_gradient = None;
            }
        }
        PaintSlot::Stroke => {
            if let Some(gradient) = gradient {
                paint.stroke.set_value(Some(gradient.fallback_color()));
                paint.stroke_gradient = Some(gradient);
            } else {
                paint.stroke_gradient = None;
            }
        }
    }
    Ok(())
}

/// Removes one content keyframe.
fn content_key_remove(content: &mut NodeContent, field: ContentField, frame: Frame) {
    let remove = |value: &mut Animatable<f64>| {
        let _ = value.remove_key(frame);
    };
    if let Some((_, part, _)) = field.stop() {
        if let Some(stop) = stop_mut(content, field) {
            match part {
                StopPart::Offset => {
                    let _ = stop.offset.remove_key(frame);
                }
                StopPart::Color => {
                    let _ = stop.color.remove_key(frame);
                }
            }
        }
        return;
    }
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

/// Validates one effect parameter write.
fn check_effect_value(channel: EffectChannel, value: EffectValueInput) -> Result<(), CoreError> {
    use crate::effect::EffectParam;
    let invalid = |reason: &'static str| CoreError::InvalidContentValue {
        field: channel.name(),
        reason,
    };
    match (channel.param, value) {
        (EffectParam::Color, EffectValueInput::Color(_)) => Ok(()),
        (
            EffectParam::Blur | EffectParam::Radius | EffectParam::Opacity | EffectParam::Intensity,
            EffectValueInput::Number(number),
        ) => {
            if !number.is_finite() {
                return Err(invalid("must be a finite number"));
            }
            match channel.param {
                EffectParam::Opacity if !(0.0..=1.0).contains(&number) => {
                    Err(invalid("must be a number from 0 to 1"))
                }
                EffectParam::Blur | EffectParam::Radius if number < 0.0 => {
                    Err(invalid("must be zero or above"))
                }
                EffectParam::Intensity if number < 0.0 => Err(invalid("must be zero or above")),
                _ => Ok(()),
            }
        }
        (EffectParam::Offset, EffectValueInput::Number(number)) => {
            if number.is_finite() {
                Ok(())
            } else {
                Err(invalid("must be a finite number"))
            }
        }
        (EffectParam::Color, _) => Err(invalid("expected a colour")),
        _ => Err(invalid("expected a number")),
    }
}

fn check_number(field: ContentField, value: f64, positive: bool) -> Result<(), CoreError> {
    let stop_offset = field
        .stop()
        .is_some_and(|(_, part, _)| part == StopPart::Offset);
    let (ok, reason) = if stop_offset
        || matches!(
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
            Err(CoreError::InvalidContentValue { .. })
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

    #[test]
    fn gradients_set_key_and_clear_through_content_fields() {
        let (mut doc, id) = rect_doc();
        let gradient = crate::Gradient::linear(
            Vec2::new(0.0, 0.0),
            Vec2::new(100.0, 0.0),
            [
                (0.0, Color::BLACK),
                (0.5, Color::rgb(255, 0, 0)),
                (1.0, Color::WHITE),
            ],
        )
        .unwrap();
        doc.set_content(
            id,
            ContentField::Fill,
            ContentValue::Gradient(Some(gradient)),
        )
        .unwrap();
        let content = doc.node(id).unwrap().content();
        assert_eq!(content.gradient_stop_count(PaintSlot::Fill), 3);
        assert!(content.field_exists(ContentField::FillStopColor(2)));
        assert!(!content.field_exists(ContentField::FillStopColor(3)));
        assert!(matches!(
            content.static_value(ContentField::Fill),
            Some(ContentValue::Gradient(Some(_)))
        ));
        assert_eq!(
            content.static_value(ContentField::FillStopColor(1)),
            Some(ContentValue::Color(Some(Color::rgb(255, 0, 0))))
        );

        // Stop parts are ordinary channels: static writes and keys.
        doc.set_content(
            id,
            ContentField::FillStopOffset(1),
            ContentValue::Number(0.8),
        )
        .unwrap();
        doc.add_content_keyframe(
            id,
            ContentField::FillStopColor(0),
            12,
            ContentValue::Color(Some(Color::rgb(0, 0, 255))),
            Ease::CubicOut,
        )
        .unwrap();
        assert_eq!(
            doc.node(id)
                .unwrap()
                .content()
                .channel(ContentField::FillStopColor(0))
                .unwrap()
                .key_frames(),
            vec![12]
        );

        // A solid colour replaces the gradient exactly like `fill="none"`.
        doc.set_content(
            id,
            ContentField::Fill,
            ContentValue::Color(Some(Color::rgb(1, 2, 3))),
        )
        .unwrap();
        let content = doc.node(id).unwrap().content();
        assert_eq!(content.gradient_stop_count(PaintSlot::Fill), 0);
        assert!(!content.field_exists(ContentField::FillStopColor(0)));
        assert_eq!(
            content.static_value(ContentField::Fill),
            Some(ContentValue::Color(Some(Color::rgb(1, 2, 3))))
        );
    }

    #[test]
    fn invalid_gradients_and_out_of_range_stops_are_rejected() {
        let (mut doc, id) = rect_doc();
        assert!(matches!(
            crate::Gradient::radial(Vec2::ZERO, 0.0, [(0.0, Color::BLACK), (1.0, Color::WHITE)]),
            Err(CoreError::InvalidGradient { .. })
        ));
        assert!(matches!(
            crate::Gradient::linear(
                Vec2::ZERO,
                Vec2::new(1.0, 0.0),
                [(0.0, Color::BLACK), (2.0, Color::WHITE)]
            ),
            Err(CoreError::InvalidGradient { .. })
        ));
        let gradient = crate::Gradient::linear(
            Vec2::ZERO,
            Vec2::new(1.0, 0.0),
            [(0.0, Color::BLACK), (1.0, Color::WHITE)],
        )
        .unwrap();
        doc.set_content(
            id,
            ContentField::Fill,
            ContentValue::Gradient(Some(gradient)),
        )
        .unwrap();
        assert!(doc
            .set_content(
                id,
                ContentField::FillStopColor(7),
                ContentValue::Color(Some(Color::WHITE))
            )
            .is_err());
        assert!(matches!(
            doc.set_content(
                id,
                ContentField::FillStopOffset(0),
                ContentValue::Number(1.5)
            ),
            Err(CoreError::InvalidContentValue { .. })
        ));
    }

    #[test]
    fn masks_validate_the_source_and_round_trip() {
        let mut doc = document();
        let card = doc.add_node(dot()).unwrap();
        let cutout = doc.add_node(dot()).unwrap();
        doc.set_mask(card, cutout, crate::MaskMode::Luma, true)
            .unwrap();
        let mask = doc.node(card).unwrap().mask().unwrap();
        assert_eq!(mask.source, cutout);
        assert_eq!(mask.mode, crate::MaskMode::Luma);
        assert!(mask.invert);
        assert!(doc.is_mask_source(cutout));
        assert!(!doc.is_mask_source(card));

        doc.clear_mask(card).unwrap();
        assert!(doc.node(card).unwrap().mask().is_none());
        assert!(!doc.is_mask_source(cutout));
    }

    #[test]
    fn masks_reject_self_ancestors_and_cycles() {
        let mut doc = document();
        let outer = doc.add_node(NodeContent::Group).unwrap();
        let inner = doc.add_node(NodeContent::Group).unwrap();
        let leaf = doc.add_node(dot()).unwrap();
        doc.reparent(inner, outer).unwrap();
        doc.reparent(leaf, inner).unwrap();
        assert_eq!(
            doc.set_mask(leaf, leaf, crate::MaskMode::Alpha, false),
            Err(CoreError::MaskSelfReference { node: leaf })
        );
        assert_eq!(
            doc.set_mask(leaf, outer, crate::MaskMode::Alpha, false),
            Err(CoreError::MaskSourceIsAncestor {
                node: leaf,
                source: outer
            })
        );
        // A child may mask its parent (it moves with it).
        doc.set_mask(outer, leaf, crate::MaskMode::Alpha, false)
            .unwrap();
        // Following the chain leaf -> outer -> leaf would cycle.
        assert_eq!(
            doc.set_mask(leaf, outer, crate::MaskMode::Alpha, false),
            Err(CoreError::MaskSourceIsAncestor {
                node: leaf,
                source: outer
            })
        );
        assert_eq!(
            doc.set_mask(leaf, inner, crate::MaskMode::Alpha, false),
            Err(CoreError::MaskSourceIsAncestor {
                node: leaf,
                source: inner
            })
        );
        let other = doc.add_node(dot()).unwrap();
        doc.set_mask(inner, other, crate::MaskMode::Alpha, false)
            .unwrap();
        assert_eq!(
            doc.set_mask(other, inner, crate::MaskMode::Alpha, false),
            Err(CoreError::MaskCycle {
                node: other,
                source: inner
            })
        );
    }

    #[test]
    fn can_mask_agrees_with_set_mask() {
        let mut doc = document();
        let a = doc.add_node(NodeContent::Group).unwrap();
        let b = doc.add_node(dot()).unwrap();
        doc.reparent(b, a).unwrap();
        assert!(!doc.can_mask(a, a));
        assert!(!doc.can_mask(b, a));
        assert!(doc.can_mask(a, b));
        assert_eq!(
            doc.set_mask(b, a, crate::MaskMode::Alpha, false),
            Err(CoreError::MaskSourceIsAncestor { node: b, source: a })
        );
    }
}

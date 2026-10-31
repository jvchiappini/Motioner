//! The composition and its node tree, plus the validated mutation API.

use crate::animation::{Animatable, Ease, Keyframe};
use crate::bounds;
use crate::color::Color;
use crate::error::CoreError;
use crate::math::Vec2;
use crate::node::{BlendMode, Node, NodeContent, NodeId, Property, PropertyValue, Transform};
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
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidGeometry`] for negative or non-finite
    /// sizes and [`CoreError::TooManyNodes`] when IDs are exhausted.
    pub fn add_node(&mut self, content: NodeContent) -> Result<NodeId, CoreError> {
        if let Some(field) = content.invalid_field() {
            return Err(CoreError::InvalidGeometry { field });
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
        });
        self.roots.push(id);
        Ok(id)
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
            Property::Position => apply(&mut transform.position, vector(property, value)?, write),
            Property::Anchor => apply(&mut transform.anchor, vector(property, value)?, write),
            Property::Scale => apply(&mut transform.scale, scale(value)?, write),
            Property::Rotation => apply(&mut transform.rotation, scalar(property, value)?, write),
            Property::Opacity => apply(&mut transform.opacity, scalar(property, value)?, write),
        }
        Ok(())
    }
}

fn apply<T: Copy>(target: &mut Animatable<T>, value: T, write: Write) {
    match write {
        Write::Static => target.set_value(value),
        Write::Key(frame, ease) => target.insert_key(Keyframe { frame, value, ease }),
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
        NodeContent::Circle {
            radius: 4.0,
            paint: Paint {
                fill: Some(Color::WHITE),
                stroke: None,
            },
        }
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
        let bad = NodeContent::Circle {
            radius: -1.0,
            paint: Paint {
                fill: None,
                stroke: None,
            },
        };
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
}

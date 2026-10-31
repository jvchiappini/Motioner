//! Nodes: identity, hierarchy, content and animatable transform.

use std::fmt;

use crate::animation::Animatable;
use crate::color::Color;
use crate::math::Vec2;

/// Persistent identity of a node within one document.
///
/// IDs are assigned sequentially in creation order, so running the same
/// program twice yields the same IDs. They are independent of memory
/// addresses and of the node's position in the hierarchy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(u32);

impl NodeId {
    /// Wraps a raw index. Only the document creates IDs that resolve.
    #[must_use]
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    /// The raw sequential index.
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }

    pub(crate) fn index(self) -> usize {
        self.0 as usize
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "#{}", self.0)
    }
}

/// How a node composites over the layers below it.
///
/// MDF v0 renders only `Normal` (source-over with straight alpha); other
/// blend modes are rejected at authoring time instead of being ignored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BlendMode {
    /// Source-over alpha compositing.
    #[default]
    Normal,
}

impl BlendMode {
    /// Looks up a blend mode by its MDF name.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        (name == "normal").then_some(Self::Normal)
    }

    /// The MDF name of this blend mode.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Normal => "normal",
        }
    }
}

/// A shape outline.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stroke {
    /// Stroke color.
    pub color: Color,
    /// Stroke width in pixels, centered on the shape edge.
    pub width: f64,
}

/// Fill and stroke of a shape. Either may be absent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Paint {
    /// Interior color, if filled.
    pub fill: Option<Color>,
    /// Outline, if stroked.
    pub stroke: Option<Stroke>,
}

/// Horizontal alignment of a text block relative to the node origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TextAlign {
    /// The text starts at the origin.
    #[default]
    Left,
    /// The text is centered on the origin.
    Center,
    /// The text ends at the origin.
    Right,
}

impl TextAlign {
    /// Looks up an alignment by its MDF name.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "left" => Some(Self::Left),
            "center" => Some(Self::Center),
            "right" => Some(Self::Right),
            _ => None,
        }
    }
}

/// A single line of text. The baseline passes through the node origin.
#[derive(Debug, Clone, PartialEq)]
pub struct TextContent {
    /// The characters to draw.
    pub text: String,
    /// Requested font family name.
    pub font: String,
    /// Font size in pixels (em height).
    pub size: f64,
    /// Glyph color.
    pub color: Color,
    /// Horizontal alignment.
    pub align: TextAlign,
}

/// What a node draws.
#[derive(Debug, Clone, PartialEq)]
pub enum NodeContent {
    /// A container that only transforms its children.
    Group,
    /// A rectangle centered on the node origin.
    Rect {
        /// Width in pixels.
        width: f64,
        /// Height in pixels.
        height: f64,
        /// Corner radius in pixels.
        radius: f64,
        /// Fill and stroke.
        paint: Paint,
    },
    /// A circle centered on the node origin.
    Circle {
        /// Radius in pixels.
        radius: f64,
        /// Fill and stroke.
        paint: Paint,
    },
    /// A text line whose baseline passes through the node origin.
    Text(TextContent),
}

impl NodeContent {
    /// Checks that sizes are finite and non-negative and text size is
    /// positive. Returns the name of the first invalid field.
    pub(crate) fn invalid_field(&self) -> Option<&'static str> {
        let bad = |value: f64| !value.is_finite() || value < 0.0;
        let bad_stroke = |paint: &Paint| paint.stroke.is_some_and(|s| bad(s.width));
        match self {
            Self::Group => None,
            Self::Rect {
                width,
                height,
                radius,
                paint,
            } => [
                (bad(*width), "width"),
                (bad(*height), "height"),
                (bad(*radius), "radius"),
                (bad_stroke(paint), "stroke_width"),
            ]
            .into_iter()
            .find_map(|(is_bad, name)| is_bad.then_some(name)),
            Self::Circle { radius, paint } => {
                if bad(*radius) {
                    Some("radius")
                } else if bad_stroke(paint) {
                    Some("stroke_width")
                } else {
                    None
                }
            }
            Self::Text(text) => (bad(text.size) || text.size == 0.0).then_some("size"),
        }
    }
}

/// An animatable property of every node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Property {
    /// Position of the anchor point in the parent space (`pos`).
    Position,
    /// Scale around the anchor point; `1.0` is 100 %.
    Scale,
    /// Clockwise rotation in degrees around the anchor point.
    Rotation,
    /// Anchor point in the node's local space.
    Anchor,
    /// Opacity from `0.0` to `1.0`, multiplied down the hierarchy.
    Opacity,
}

impl Property {
    /// Every property with its MDF name.
    pub const ALL: [(Self, &'static str); 5] = [
        (Self::Position, "pos"),
        (Self::Scale, "scale"),
        (Self::Rotation, "rotation"),
        (Self::Anchor, "anchor"),
        (Self::Opacity, "opacity"),
    ];

    /// Looks up a property by its MDF name (for example `"pos"`).
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .find(|(_, candidate)| *candidate == name)
            .map(|(property, _)| *property)
    }

    /// The MDF name of this property.
    #[must_use]
    pub fn name(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(property, _)| *property == self)
            .map_or("?", |(_, name)| name)
    }
}

/// A value written to a [`Property`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PropertyValue {
    /// A single number.
    Scalar(f64),
    /// A 2D vector.
    Vec2(Vec2),
}

/// The animatable transform and opacity of a node.
///
/// The local matrix is `translate(pos) * rotate(rotation) * scale(scale) *
/// translate(-anchor)`, the After Effects convention.
#[derive(Debug, Clone, PartialEq)]
pub struct Transform {
    /// Position of the anchor in parent space.
    pub position: Animatable<Vec2>,
    /// Scale factors.
    pub scale: Animatable<Vec2>,
    /// Clockwise rotation in degrees.
    pub rotation: Animatable<f64>,
    /// Anchor point in local space.
    pub anchor: Animatable<Vec2>,
    /// Opacity in `0.0..=1.0` (validated on write, clamped on evaluation).
    pub opacity: Animatable<f64>,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            position: Animatable::new(Vec2::ZERO),
            scale: Animatable::new(Vec2::ONE),
            rotation: Animatable::new(0.0),
            anchor: Animatable::new(Vec2::ZERO),
            opacity: Animatable::new(1.0),
        }
    }
}

/// One element of the composition tree.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub(crate) id: NodeId,
    pub(crate) parent: Option<NodeId>,
    pub(crate) children: Vec<NodeId>,
    pub(crate) content: NodeContent,
    pub(crate) transform: Transform,
    pub(crate) blend: BlendMode,
}

impl Node {
    /// The node's persistent ID.
    #[must_use]
    pub fn id(&self) -> NodeId {
        self.id
    }

    /// The parent group, or `None` for a top-level node.
    #[must_use]
    pub fn parent(&self) -> Option<NodeId> {
        self.parent
    }

    /// Children in paint order (first is painted first).
    #[must_use]
    pub fn children(&self) -> &[NodeId] {
        &self.children
    }

    /// What the node draws.
    #[must_use]
    pub fn content(&self) -> &NodeContent {
        &self.content
    }

    /// Transform and opacity.
    #[must_use]
    pub fn transform(&self) -> &Transform {
        &self.transform
    }

    /// Blend mode.
    #[must_use]
    pub fn blend(&self) -> BlendMode {
        self.blend
    }
}

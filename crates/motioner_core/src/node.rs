//! Nodes: identity, hierarchy, content and animatable transform.

use std::fmt;

use crate::animation::{Animatable, AnimatableVec2};
use crate::color::Color;
use crate::math::Vec2;

/// Persistent identity of a node within one document.
///
/// IDs are assigned sequentially in creation order, so running the same
/// program twice yields the same IDs. They are independent of memory
/// addresses and of the node's position in the hierarchy.
///
/// [`NodeId`] is the document's internal handle. The *stable* identity users
/// and override files refer to is [`NodeIdentity`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(u32);

/// A node's stable identity inside a document (D-0022).
///
/// Either an explicit `.id("name")` chosen by the author, or a provenance key
/// derived from the creating call's source location, the user-function call
/// sites on the stack and the enclosing loop indices.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NodeIdentity {
    /// `.id("name")`: unique per document, `[A-Za-z_][A-Za-z0-9_.-]*`.
    Explicit(String),
    /// A key such as `@412/c88/i3`, produced by the DSL.
    Provenance(String),
}

impl NodeIdentity {
    /// Validates an explicit name and wraps it.
    ///
    /// # Errors
    ///
    /// Returns [`crate::CoreError::InvalidIdentity`] when the name is empty,
    /// does not start with an ASCII letter or `_`, or contains a character
    /// outside `[A-Za-z0-9_.-]`.
    pub fn explicit(name: impl Into<String>) -> Result<Self, crate::CoreError> {
        let name = name.into();
        if !is_valid_identity_name(&name) {
            return Err(crate::CoreError::InvalidIdentity { name });
        }
        Ok(Self::Explicit(name))
    }

    /// Wraps a provenance key produced by the interpreter.
    #[must_use]
    pub fn provenance(key: impl Into<String>) -> Self {
        Self::Provenance(key.into())
    }

    /// The text used in override files and shown in the inspector.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Explicit(name) | Self::Provenance(name) => name,
        }
    }

    /// Whether the identity came from `.id("...")`.
    #[must_use]
    pub const fn is_explicit(&self) -> bool {
        matches!(self, Self::Explicit(_))
    }
}

impl fmt::Display for NodeIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// `[A-Za-z_][A-Za-z0-9_.-]*`, checked byte-wise.
#[must_use]
pub fn is_valid_identity_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == b'_') {
        return false;
    }
    bytes.all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'))
}

/// A validated, settable field of [`NodeContent`].
///
/// Names match the override property names of D-0022.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContentField {
    /// Rect width in pixels.
    Width,
    /// Rect height in pixels.
    Height,
    /// Rect corner radius or circle radius in pixels.
    Radius,
    /// Interior colour (`None` clears it).
    Fill,
    /// Outline colour (`None` clears the stroke).
    Stroke,
    /// Outline width in pixels; `0` clears the stroke.
    StrokeWidth,
    /// Text characters.
    Text,
    /// Text em size in pixels.
    Size,
    /// Text glyph colour.
    Color,
    /// Text horizontal alignment.
    Align,
}

impl ContentField {
    /// Every field with its override property name.
    pub const ALL: [(Self, &'static str); 10] = [
        (Self::Width, "width"),
        (Self::Height, "height"),
        (Self::Radius, "radius"),
        (Self::Fill, "fill"),
        (Self::Stroke, "stroke"),
        (Self::StrokeWidth, "stroke_width"),
        (Self::Text, "text"),
        (Self::Size, "size"),
        (Self::Color, "color"),
        (Self::Align, "align"),
    ];

    /// Looks up a field by its override property name.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .find(|(_, candidate)| *candidate == name)
            .map(|(field, _)| *field)
    }

    /// The override property name of this field.
    #[must_use]
    pub fn name(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(field, _)| *field == self)
            .map_or("?", |(_, name)| name)
    }
}

/// A value accepted by [`ContentField`] writes.
#[derive(Debug, Clone, PartialEq)]
pub enum ContentValue {
    /// A number for `width`, `height`, `radius`, `stroke_width`, `size`.
    Number(f64),
    /// A colour for `fill`, `stroke`, `color`; `None` clears an optional one.
    Color(Option<Color>),
    /// Text for `text`.
    Text(String),
    /// Alignment for `align`.
    Align(TextAlign),
}

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
    /// Human name of the node kind ("group", "rect", "circle", "text").
    #[must_use]
    pub const fn kind_name(&self) -> &'static str {
        match self {
            Self::Group => "group",
            Self::Rect { .. } => "rect",
            Self::Circle { .. } => "circle",
            Self::Text(_) => "text",
        }
    }

    /// Whether [`ContentField`] can be written to this kind of content.
    #[must_use]
    pub const fn accepts(&self, field: ContentField) -> bool {
        match self {
            Self::Group => false,
            Self::Rect { .. } => matches!(
                field,
                ContentField::Width
                    | ContentField::Height
                    | ContentField::Radius
                    | ContentField::Fill
                    | ContentField::Stroke
                    | ContentField::StrokeWidth
            ),
            Self::Circle { .. } => matches!(
                field,
                ContentField::Radius
                    | ContentField::Fill
                    | ContentField::Stroke
                    | ContentField::StrokeWidth
            ),
            Self::Text(_) => matches!(
                field,
                ContentField::Text | ContentField::Size | ContentField::Color | ContentField::Align
            ),
        }
    }

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
    /// Whether the property has two independently keyable axes.
    #[must_use]
    pub const fn is_vector(self) -> bool {
        matches!(self, Self::Position | Self::Scale | Self::Anchor)
    }

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
    /// Position of the anchor in parent space (X and Y keyed separately).
    pub position: AnimatableVec2,
    /// Scale factors (X and Y keyed separately).
    pub scale: AnimatableVec2,
    /// Clockwise rotation in degrees.
    pub rotation: Animatable<f64>,
    /// Anchor point in local space (X and Y keyed separately).
    pub anchor: AnimatableVec2,
    /// Opacity in `0.0..=1.0` (validated on write, clamped on evaluation).
    pub opacity: Animatable<f64>,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            position: AnimatableVec2::new(Vec2::ZERO),
            scale: AnimatableVec2::new(Vec2::ONE),
            rotation: Animatable::new(0.0),
            anchor: AnimatableVec2::new(Vec2::ZERO),
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
    pub(crate) identity: NodeIdentity,
    pub(crate) visible: bool,
}

impl Node {
    /// The node's persistent ID.
    #[must_use]
    pub fn id(&self) -> NodeId {
        self.id
    }

    /// The node's stable identity (explicit id or provenance key).
    #[must_use]
    pub fn identity(&self) -> &NodeIdentity {
        &self.identity
    }

    /// Whether the node and its subtree are rendered.
    ///
    /// Visibility is editor/document state, not an animation channel: hiding a
    /// node hides every descendant too. Locking is editor-only state and does
    /// not live here (see [`crate::OverrideSet`]).
    #[must_use]
    pub fn is_visible(&self) -> bool {
        self.visible
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

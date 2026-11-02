//! Nodes: identity, hierarchy, content and animatable transform.

use std::fmt;

use crate::animation::{Animatable, AnimatableVec2};
use crate::color::Color;
use crate::gradient::{Gradient, PaintSlot, StopPart};
use crate::math::Vec2;
use crate::path::{LineCap, LineJoin, PathGeometry};
use crate::time::Frame;

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
    /// Start of the drawn fraction of a path (`0.0..=1.0`).
    TrimStart,
    /// End of the drawn fraction of a path (`0.0..=1.0`).
    TrimEnd,
    /// Shifts the trimmed window along the path (`0.0..=1.0`, wraps).
    TrimOffset,
    /// Dash on-length in local pixels; the gap is equal. `0` means solid.
    Dash,
    /// Shifts the dash pattern along the path, in local pixels.
    DashOffset,
    /// Text characters.
    Text,
    /// Text em size in pixels.
    Size,
    /// Text glyph colour.
    Color,
    /// Text horizontal alignment.
    Align,
    /// Path stroke cap style (discrete).
    Cap,
    /// Path stroke join style (discrete).
    Join,
    /// Offset of fill stop `n` (`fill_stop<n>_offset`, D-0030).
    FillStopOffset(u8),
    /// Colour of fill stop `n` (`fill_stop<n>_color`).
    FillStopColor(u8),
    /// Offset of stroke stop `n` (`stroke_stop<n>_offset`).
    StrokeStopOffset(u8),
    /// Colour of stroke stop `n` (`stroke_stop<n>_color`).
    StrokeStopColor(u8),
}

impl ContentField {
    /// The static fields with their override property name.
    ///
    /// Gradient stops are dynamic (`fill_stop3_color`); parse them with
    /// [`ContentField::from_name`] and describe them with
    /// [`ContentField::name`].
    pub const ALL: [(Self, &'static str); 17] = [
        (Self::Width, "width"),
        (Self::Height, "height"),
        (Self::Radius, "radius"),
        (Self::Fill, "fill"),
        (Self::Stroke, "stroke"),
        (Self::StrokeWidth, "stroke_width"),
        (Self::TrimStart, "trim_start"),
        (Self::TrimEnd, "trim_end"),
        (Self::TrimOffset, "trim_offset"),
        (Self::Dash, "dash"),
        (Self::DashOffset, "dash_offset"),
        (Self::Text, "text"),
        (Self::Size, "size"),
        (Self::Color, "color"),
        (Self::Align, "align"),
        (Self::Cap, "cap"),
        (Self::Join, "join"),
    ];

    /// Whether the field has keyframes (D-0028).
    ///
    /// Content fields are scalar channels: none of them has axes. Text,
    /// alignment, cap and join are discrete and cannot be animated.
    #[must_use]
    pub const fn is_animatable(self) -> bool {
        !matches!(self, Self::Text | Self::Align | Self::Cap | Self::Join)
    }

    /// Looks up a field by its override property name.
    ///
    /// Accepts the static names in [`ContentField::ALL`] and the dynamic
    /// gradient-stop names (`fill_stop0_offset`, `stroke_stop2_color`).
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        if let Some((field, _)) = Self::ALL.iter().find(|(_, candidate)| *candidate == name) {
            return Some(*field);
        }
        let (slot, rest) = if let Some(rest) = name.strip_prefix("fill_stop") {
            (PaintSlot::Fill, rest)
        } else {
            (PaintSlot::Stroke, name.strip_prefix("stroke_stop")?)
        };
        let (index, part) = if let Some(index) = rest.strip_suffix("_offset") {
            (index, StopPart::Offset)
        } else {
            (rest.strip_suffix("_color")?, StopPart::Color)
        };
        let index = index.parse::<u8>().ok()?;
        Some(Self::stop_field(slot, part, index))
    }

    /// The override property name of this field.
    #[must_use]
    pub fn name(self) -> String {
        match self.stop() {
            Some((slot, part, index)) => {
                let slot = match slot {
                    PaintSlot::Fill => "fill",
                    PaintSlot::Stroke => "stroke",
                };
                let part = match part {
                    StopPart::Offset => "offset",
                    StopPart::Color => "color",
                };
                format!("{slot}_stop{index}_{part}")
            }
            None => Self::ALL
                .iter()
                .find(|(field, _)| *field == self)
                .map_or("?", |(_, name)| name)
                .to_owned(),
        }
    }

    /// The gradient stop this field addresses, if any.
    #[must_use]
    pub const fn stop(self) -> Option<(PaintSlot, StopPart, usize)> {
        match self {
            Self::FillStopOffset(index) => {
                Some((PaintSlot::Fill, StopPart::Offset, index as usize))
            }
            Self::FillStopColor(index) => Some((PaintSlot::Fill, StopPart::Color, index as usize)),
            Self::StrokeStopOffset(index) => {
                Some((PaintSlot::Stroke, StopPart::Offset, index as usize))
            }
            Self::StrokeStopColor(index) => {
                Some((PaintSlot::Stroke, StopPart::Color, index as usize))
            }
            _ => None,
        }
    }

    /// Builds the dynamic field for one gradient stop part.
    #[must_use]
    pub const fn stop_field(slot: PaintSlot, part: StopPart, index: u8) -> Self {
        match (slot, part) {
            (PaintSlot::Fill, StopPart::Offset) => Self::FillStopOffset(index),
            (PaintSlot::Fill, StopPart::Color) => Self::FillStopColor(index),
            (PaintSlot::Stroke, StopPart::Offset) => Self::StrokeStopOffset(index),
            (PaintSlot::Stroke, StopPart::Color) => Self::StrokeStopColor(index),
        }
    }
}

/// A value accepted by [`ContentField`] writes.
#[derive(Debug, Clone, PartialEq)]
pub enum ContentValue {
    /// A number for `width`, `height`, `radius`, `stroke_width`, `size`.
    Number(f64),
    /// A colour for `fill`, `stroke`, `color`; `None` clears an optional one.
    Color(Option<Color>),
    /// A gradient for `fill`, `stroke` or text `color`; `None` clears it
    /// (D-0030).
    Gradient(Option<Gradient>),
    /// Text for `text`.
    Text(String),
    /// Alignment for `align`.
    Align(TextAlign),
    /// Stroke cap for `cap`.
    Cap(LineCap),
    /// Stroke join for `join`.
    Join(LineJoin),
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

/// How a node composites over the layers below it (D-0031).
///
/// The formulas are fixed in D-0028: premultiplied alpha in linear light.
/// `Normal`, `Add`, `Multiply` and `Screen` are fixed-function GPU blend
/// states; `Overlay`, `Darken` and `Lighten` need the backdrop, so the renderer
/// composites them through an offscreen layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BlendMode {
    /// Source-over alpha compositing.
    #[default]
    Normal,
    /// Additive: `S + B`, source-over coverage.
    Add,
    /// Multiplicative: `S * B`.
    Multiply,
    /// Screen: `S + B - S * B`.
    Screen,
    /// Multiply in the dark half, screen in the light half (conditioned on the
    /// source colour).
    Overlay,
    /// Per-channel minimum.
    Darken,
    /// Per-channel maximum.
    Lighten,
}

impl BlendMode {
    /// Every mode, in inspector order.
    pub const ALL: [Self; 7] = [
        Self::Normal,
        Self::Add,
        Self::Multiply,
        Self::Screen,
        Self::Overlay,
        Self::Darken,
        Self::Lighten,
    ];

    /// Looks up a blend mode by its MDF name.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "normal" => Some(Self::Normal),
            "add" => Some(Self::Add),
            "multiply" => Some(Self::Multiply),
            "screen" => Some(Self::Screen),
            "overlay" => Some(Self::Overlay),
            "darken" => Some(Self::Darken),
            "lighten" => Some(Self::Lighten),
            _ => None,
        }
    }

    /// The MDF name of this blend mode.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Add => "add",
            Self::Multiply => "multiply",
            Self::Screen => "screen",
            Self::Overlay => "overlay",
            Self::Darken => "darken",
            Self::Lighten => "lighten",
        }
    }

    /// `true` for the modes that need a destination snapshot
    /// (`Overlay`, `Darken`, `Lighten`).
    #[must_use]
    pub const fn needs_backdrop(self) -> bool {
        matches!(self, Self::Overlay | Self::Darken | Self::Lighten)
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

/// The resolved fill and stroke of one shape at one frame.
///
/// This is the value the render plan carries. The document stores the same
/// fields as animatable channels in [`PaintSpec`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Paint {
    /// Interior color, if filled.
    pub fill: Option<Color>,
    /// Outline, if stroked.
    pub stroke: Option<Stroke>,
}

/// The animatable fill and stroke properties of a shape (D-0028).
///
/// A stroke is drawn only when its colour is present **and** its width is
/// positive, so `stroke_width = 0` hides a stroke exactly like `stroke =
/// "none"` does. Channels need not be animated.
#[derive(Debug, Clone, PartialEq)]
pub struct PaintSpec {
    /// Interior colour (`fill`).
    pub fill: Animatable<Option<Color>>,
    /// Outline colour (`stroke`).
    pub stroke: Animatable<Option<Color>>,
    /// Outline width in pixels (`stroke_width`).
    pub stroke_width: Animatable<f64>,
    /// Interior gradient (D-0030); takes precedence over `fill` when present.
    pub fill_gradient: Option<Gradient>,
    /// Outline gradient (D-0030); takes precedence over `stroke` when present.
    pub stroke_gradient: Option<Gradient>,
}

impl PaintSpec {
    /// Creates a paint from its static values.
    #[must_use]
    pub fn new(fill: Option<Color>, stroke: Option<Color>, stroke_width: f64) -> Self {
        Self {
            fill: Animatable::new(fill),
            stroke: Animatable::new(stroke),
            stroke_width: Animatable::new(stroke_width),
            fill_gradient: None,
            stroke_gradient: None,
        }
    }

    /// Creates a paint from a resolved value, as constructors receive it.
    #[must_use]
    pub fn from_paint(paint: Paint) -> Self {
        let stroke = paint.stroke;
        Self {
            fill: Animatable::new(paint.fill),
            stroke: Animatable::new(stroke.map(|stroke| stroke.color)),
            stroke_width: Animatable::new(stroke.map_or(0.0, |stroke| stroke.width)),
            fill_gradient: None,
            stroke_gradient: None,
        }
    }

    /// A solid fill with no stroke.
    #[must_use]
    pub fn solid(color: Color) -> Self {
        Self::new(Some(color), None, 0.0)
    }

    /// No fill and no stroke.
    #[must_use]
    pub fn none() -> Self {
        Self::new(None, None, 0.0)
    }

    /// The gradient of one paint slot, if it has one.
    #[must_use]
    pub fn gradient(&self, slot: PaintSlot) -> Option<&Gradient> {
        match slot {
            PaintSlot::Fill => self.fill_gradient.as_ref(),
            PaintSlot::Stroke => self.stroke_gradient.as_ref(),
        }
    }

    /// The mutable gradient of one paint slot.
    #[must_use]
    pub fn gradient_mut(&mut self, slot: PaintSlot) -> Option<&mut Gradient> {
        match slot {
            PaintSlot::Fill => self.fill_gradient.as_mut(),
            PaintSlot::Stroke => self.stroke_gradient.as_mut(),
        }
    }

    /// The pointer to the channel of one paint field, if it is one.
    pub(crate) fn channel(&self, field: ContentField) -> Option<ContentChannel<'_>> {
        if let Some((slot, part, index)) = field.stop() {
            let stop = self.gradient(slot)?.stop(index)?;
            return Some(match part {
                StopPart::Offset => ContentChannel::Number(&stop.offset),
                StopPart::Color => ContentChannel::Color(&stop.color),
            });
        }
        match field {
            ContentField::Fill => Some(ContentChannel::OptionalColor(&self.fill)),
            ContentField::Stroke => Some(ContentChannel::OptionalColor(&self.stroke)),
            ContentField::StrokeWidth => Some(ContentChannel::Number(&self.stroke_width)),
            _ => None,
        }
    }
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
    /// Font size in pixels (em height); animatable (`size`).
    pub size: Animatable<f64>,
    /// Glyph color; animatable (`color`).
    pub color: Animatable<Color>,
    /// Horizontal alignment.
    pub align: TextAlign,
    /// Glyph fill gradient (D-0030); takes precedence over color when present.
    pub gradient: Option<Gradient>,
}

impl TextContent {
    /// Creates a text line from static values.
    #[must_use]
    pub fn new(
        text: impl Into<String>,
        font: impl Into<String>,
        size: f64,
        color: Color,
        align: TextAlign,
    ) -> Self {
        Self {
            text: text.into(),
            font: font.into(),
            size: Animatable::new(size),
            color: Animatable::new(color),
            align,
            gradient: None,
        }
    }
}

/// A reference to one content field's animation channel (D-0028).
///
/// This is the one place that maps a [`ContentField`] to the matching
/// `Animatable` member, so the inspector, the timeline, `query`, the DSL and
/// the evaluator do not duplicate that table.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ContentChannel<'a> {
    /// A scalar channel (`width`, `radius`, `size`, ...).
    Number(&'a Animatable<f64>),
    /// A required colour channel (text `color`).
    Color(&'a Animatable<Color>),
    /// A colour channel that can also be absent (`fill`, `stroke`).
    OptionalColor(&'a Animatable<Option<Color>>),
}

impl ContentChannel<'_> {
    /// Whether the channel has any keyframe.
    #[must_use]
    pub fn is_animated(&self) -> bool {
        match self {
            Self::Number(value) => !value.keys().is_empty(),
            Self::Color(value) => !value.keys().is_empty(),
            Self::OptionalColor(value) => !value.keys().is_empty(),
        }
    }

    /// The keyed frames, sorted by strictly increasing frame.
    #[must_use]
    pub fn key_frames(&self) -> Vec<Frame> {
        match self {
            Self::Number(value) => value.keys().iter().map(|key| key.frame).collect(),
            Self::Color(value) => value.keys().iter().map(|key| key.frame).collect(),
            Self::OptionalColor(value) => value.keys().iter().map(|key| key.frame).collect(),
        }
    }
}

/// What a node draws.
#[derive(Debug, Clone, PartialEq)]
pub enum NodeContent {
    /// A container that only transforms its children.
    Group,
    /// A rectangle centered on the node origin.
    Rect {
        /// Width in pixels (`width`).
        width: Animatable<f64>,
        /// Height in pixels (`height`).
        height: Animatable<f64>,
        /// Corner radius in pixels (`radius`).
        radius: Animatable<f64>,
        /// Fill and stroke channels.
        paint: PaintSpec,
    },
    /// A circle centered on the node origin.
    Circle {
        /// Radius in pixels (`radius`).
        radius: Animatable<f64>,
        /// Fill and stroke channels.
        paint: PaintSpec,
    },
    /// A vector path in the node's local space (D-0029).
    Path {
        /// Authored points, closed flag, cap and join.
        geometry: PathGeometry,
        /// Fill and stroke channels.
        paint: PaintSpec,
        /// Start of the drawn fraction (`trim_start`, `0.0..=1.0`).
        trim_start: Animatable<f64>,
        /// End of the drawn fraction (`trim_end`, `0.0..=1.0`).
        trim_end: Animatable<f64>,
        /// Shift of the trimmed window (`trim_offset`, wraps).
        trim_offset: Animatable<f64>,
        /// Dash on-length in local pixels (`dash`; `0` means solid).
        dash: Animatable<f64>,
        /// Dash pattern shift in local pixels (`dash_offset`).
        dash_offset: Animatable<f64>,
    },
    /// A text line whose baseline passes through the node origin.
    Text(TextContent),
}

impl NodeContent {
    /// A rectangle from static values.
    #[must_use]
    pub fn rect(width: f64, height: f64, radius: f64, paint: Paint) -> Self {
        Self::Rect {
            width: Animatable::new(width),
            height: Animatable::new(height),
            radius: Animatable::new(radius),
            paint: PaintSpec::from_paint(paint),
        }
    }

    /// A circle from static values.
    #[must_use]
    pub fn circle(radius: f64, paint: Paint) -> Self {
        Self::Circle {
            radius: Animatable::new(radius),
            paint: PaintSpec::from_paint(paint),
        }
    }

    /// A path from static values: fully drawn, solid, untrimmed.
    #[must_use]
    pub fn path(geometry: PathGeometry, paint: Paint) -> Self {
        Self::Path {
            geometry,
            paint: PaintSpec::from_paint(paint),
            trim_start: Animatable::new(0.0),
            trim_end: Animatable::new(1.0),
            trim_offset: Animatable::new(0.0),
            dash: Animatable::new(0.0),
            dash_offset: Animatable::new(0.0),
        }
    }

    /// Human name of the node kind ("group", "rect", "circle", "path", "text").
    #[must_use]
    pub const fn kind_name(&self) -> &'static str {
        match self {
            Self::Group => "group",
            Self::Rect { .. } => "rect",
            Self::Circle { .. } => "circle",
            Self::Path { .. } => "path",
            Self::Text(_) => "text",
        }
    }

    /// Whether [`ContentField`] can be written to this kind of content.
    ///
    /// This is the kind-level check. Gradient stop fields additionally need
    /// the addressed gradient to exist; use [`NodeContent::field_exists`] for
    /// a write.
    #[must_use]
    pub const fn accepts(&self, field: ContentField) -> bool {
        let stop = matches!(
            field,
            ContentField::FillStopOffset(_)
                | ContentField::FillStopColor(_)
                | ContentField::StrokeStopOffset(_)
                | ContentField::StrokeStopColor(_)
        );
        match self {
            Self::Group => false,
            Self::Rect { .. } => {
                stop || matches!(
                    field,
                    ContentField::Width
                        | ContentField::Height
                        | ContentField::Radius
                        | ContentField::Fill
                        | ContentField::Stroke
                        | ContentField::StrokeWidth
                )
            }
            Self::Circle { .. } => {
                stop || matches!(
                    field,
                    ContentField::Radius
                        | ContentField::Fill
                        | ContentField::Stroke
                        | ContentField::StrokeWidth
                )
            }
            Self::Path { .. } => {
                stop || matches!(
                    field,
                    ContentField::Fill
                        | ContentField::Stroke
                        | ContentField::StrokeWidth
                        | ContentField::TrimStart
                        | ContentField::TrimEnd
                        | ContentField::TrimOffset
                        | ContentField::Dash
                        | ContentField::DashOffset
                        | ContentField::Cap
                        | ContentField::Join
                )
            }
            Self::Text(_) => {
                matches!(
                    field,
                    ContentField::FillStopOffset(_) | ContentField::FillStopColor(_)
                ) || matches!(
                    field,
                    ContentField::Text
                        | ContentField::Size
                        | ContentField::Color
                        | ContentField::Align
                )
            }
        }
    }

    /// Number of stops in one paint slot's gradient (0 when it has none).
    #[must_use]
    pub fn gradient_stop_count(&self, slot: PaintSlot) -> usize {
        match self {
            Self::Text(text) if slot == PaintSlot::Fill => {
                text.gradient.as_ref().map_or(0, Gradient::len)
            }
            Self::Rect { paint, .. } | Self::Circle { paint, .. } | Self::Path { paint, .. } => {
                paint.gradient(slot).map_or(0, Gradient::len)
            }
            Self::Text(_) | Self::Group => 0,
        }
    }

    /// Whether `field` addresses something this content actually has.
    ///
    /// Unlike [`NodeContent::accepts`], a gradient stop field is only present
    /// when the target slot has a gradient and the index is in range.
    #[must_use]
    pub fn field_exists(&self, field: ContentField) -> bool {
        if !self.accepts(field) {
            return false;
        }
        let Some((slot, _, index)) = field.stop() else {
            return true;
        };
        match self {
            Self::Text(text) => {
                slot == PaintSlot::Fill
                    && text
                        .gradient
                        .as_ref()
                        .is_some_and(|gradient| index < gradient.len())
            }
            Self::Rect { paint, .. } | Self::Circle { paint, .. } | Self::Path { paint, .. } => {
                paint
                    .gradient(slot)
                    .is_some_and(|gradient| index < gradient.len())
            }
            Self::Group => false,
        }
    }

    /// The animation channel of `field`, or `None` when the kind has no such
    /// field or the field is not animatable (`text`, `align`).
    #[must_use]
    pub fn channel<'a>(&'a self, field: ContentField) -> Option<ContentChannel<'a>> {
        if !self.field_exists(field) || !field.is_animatable() {
            return None;
        }
        let number = |value: &'a Animatable<f64>| Some(ContentChannel::Number(value));
        match (self, field) {
            (Self::Rect { width, .. }, ContentField::Width) => number(width),
            (Self::Rect { height, .. }, ContentField::Height) => number(height),
            (Self::Rect { radius, .. } | Self::Circle { radius, .. }, ContentField::Radius) => {
                number(radius)
            }
            (Self::Text(text), field) if field.stop().is_some() => {
                let (_, part, index) = field.stop()?;
                let stop = text.gradient.as_ref()?.stop(index)?;
                Some(match part {
                    StopPart::Offset => ContentChannel::Number(&stop.offset),
                    StopPart::Color => ContentChannel::Color(&stop.color),
                })
            }
            (Self::Rect { paint, .. } | Self::Circle { paint, .. }, field) => paint.channel(field),
            (
                Self::Path {
                    trim_start,
                    trim_end,
                    trim_offset,
                    dash,
                    dash_offset,
                    paint,
                    ..
                },
                field,
            ) => match field {
                ContentField::TrimStart => number(trim_start),
                ContentField::TrimEnd => number(trim_end),
                ContentField::TrimOffset => number(trim_offset),
                ContentField::Dash => number(dash),
                ContentField::DashOffset => number(dash_offset),
                _ => paint.channel(field),
            },
            (Self::Text(text), ContentField::Size) => number(&text.size),
            (Self::Text(text), ContentField::Color) => Some(ContentChannel::Color(&text.color)),
            _ => None,
        }
    }

    /// The static value of `field`, ignoring keyframes.
    #[must_use]
    pub fn static_value(&self, field: ContentField) -> Option<ContentValue> {
        if let Some((slot, part, index)) = field.stop() {
            let stop = match self {
                Self::Text(text) => text.gradient.as_ref()?.stop(index)?,
                Self::Rect { paint, .. }
                | Self::Circle { paint, .. }
                | Self::Path { paint, .. } => paint.gradient(slot)?.stop(index)?,
                Self::Group => return None,
            };
            return Some(match part {
                StopPart::Offset => ContentValue::Number(stop.offset.value()),
                StopPart::Color => ContentValue::Color(Some(stop.color.value())),
            });
        }
        match (self, field) {
            (Self::Rect { width, .. }, ContentField::Width) => {
                Some(ContentValue::Number(width.value()))
            }
            (Self::Rect { height, .. }, ContentField::Height) => {
                Some(ContentValue::Number(height.value()))
            }
            (Self::Rect { radius, .. } | Self::Circle { radius, .. }, ContentField::Radius) => {
                Some(ContentValue::Number(radius.value()))
            }
            (
                Self::Rect { paint, .. } | Self::Circle { paint, .. } | Self::Path { paint, .. },
                ContentField::Fill,
            ) => Some(match &paint.fill_gradient {
                Some(gradient) => ContentValue::Gradient(Some(gradient.clone())),
                None => ContentValue::Color(paint.fill.value()),
            }),
            (
                Self::Rect { paint, .. } | Self::Circle { paint, .. } | Self::Path { paint, .. },
                ContentField::Stroke,
            ) => Some(match &paint.stroke_gradient {
                Some(gradient) => ContentValue::Gradient(Some(gradient.clone())),
                None => ContentValue::Color(paint.stroke.value()),
            }),
            (
                Self::Rect { paint, .. } | Self::Circle { paint, .. } | Self::Path { paint, .. },
                ContentField::StrokeWidth,
            ) => Some(ContentValue::Number(paint.stroke_width.value())),
            (
                Self::Path {
                    trim_start,
                    trim_end,
                    trim_offset,
                    dash,
                    dash_offset,
                    geometry,
                    ..
                },
                field,
            ) => match field {
                ContentField::TrimStart => Some(ContentValue::Number(trim_start.value())),
                ContentField::TrimEnd => Some(ContentValue::Number(trim_end.value())),
                ContentField::TrimOffset => Some(ContentValue::Number(trim_offset.value())),
                ContentField::Dash => Some(ContentValue::Number(dash.value())),
                ContentField::DashOffset => Some(ContentValue::Number(dash_offset.value())),
                ContentField::Cap => Some(ContentValue::Cap(geometry.cap())),
                ContentField::Join => Some(ContentValue::Join(geometry.join())),
                _ => None,
            },
            (Self::Text(text), ContentField::Text) => Some(ContentValue::Text(text.text.clone())),
            (Self::Text(text), ContentField::Size) => Some(ContentValue::Number(text.size.value())),
            (Self::Text(text), ContentField::Color) => Some(match &text.gradient {
                Some(gradient) => ContentValue::Gradient(Some(gradient.clone())),
                None => ContentValue::Color(Some(text.color.value())),
            }),
            (Self::Text(text), ContentField::Align) => Some(ContentValue::Align(text.align)),
            _ => None,
        }
    }

    /// Checks that static sizes are finite and non-negative and text size is
    /// positive. Returns the name of the first invalid field.
    pub(crate) fn invalid_field(&self) -> Option<&'static str> {
        let bad = |value: f64| !value.is_finite() || value < 0.0;
        let bad_paint = |paint: &PaintSpec| {
            bad(paint.stroke_width.value())
                || paint
                    .fill_gradient
                    .as_ref()
                    .is_some_and(|g| g.invalid().is_some())
                || paint
                    .stroke_gradient
                    .as_ref()
                    .is_some_and(|g| g.invalid().is_some())
                || {
                    // Keyed colours carry no numeric constraint; keyed widths
                    // are validated when they are written.
                    paint.stroke_width.keys().iter().any(|key| bad(key.value))
                }
        };
        match self {
            Self::Group => None,
            Self::Rect {
                width,
                height,
                radius,
                paint,
            } => [
                (bad(width.value()), "width"),
                (bad(height.value()), "height"),
                (bad(radius.value()), "radius"),
                (bad_paint(paint), "stroke_width"),
            ]
            .into_iter()
            .find_map(|(is_bad, name)| is_bad.then_some(name)),
            Self::Circle { radius, paint } => {
                if bad(radius.value()) {
                    Some("radius")
                } else if bad_paint(paint) {
                    Some("stroke_width")
                } else {
                    None
                }
            }
            Self::Path {
                trim_start,
                trim_end,
                trim_offset,
                dash,
                dash_offset,
                paint,
                ..
            } => {
                let in_range = |value: &Animatable<f64>, max: f64| {
                    let ok = |number: f64| number.is_finite() && number >= 0.0 && number <= max;
                    !ok(value.value()) || value.keys().iter().any(|key| !ok(key.value))
                };
                [
                    (in_range(trim_start, 1.0), "trim_start"),
                    (in_range(trim_end, 1.0), "trim_end"),
                    (in_range(trim_offset, 1.0), "trim_offset"),
                    (in_range(dash, f64::INFINITY), "dash"),
                    (in_range(dash_offset, f64::INFINITY), "dash_offset"),
                    (bad_paint(paint), "stroke_width"),
                ]
                .into_iter()
                .find_map(|(is_bad, name)| is_bad.then_some(name))
            }
            Self::Text(text) => {
                let size = text.size.value();
                if text
                    .gradient
                    .as_ref()
                    .is_some_and(|g| g.invalid().is_some())
                {
                    Some("color")
                } else {
                    (bad(size) || size == 0.0).then_some("size")
                }
            }
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

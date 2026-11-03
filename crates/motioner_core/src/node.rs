//! Nodes: identity, hierarchy, content and animatable transform.

use std::fmt;

use crate::animation::{Animatable, AnimatableVec2, Ease};
use crate::color::Color;
use crate::error::CoreError;
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
    /// Text font weight (100..=900, discrete).
    Weight,
    /// Extra advance between characters, in em units.
    Tracking,
    /// Line-step multiplier for multi-line text.
    LineHeight,
    /// Text glyph colour.
    Color,
    /// Text horizontal alignment.
    Align,
    /// Path stroke cap style (discrete).
    Cap,
    /// Path stroke join style (discrete).
    Join,
    /// Image source path (discrete, project-relative).
    Source,
    /// Image fit mode (discrete).
    Fit,
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
    pub const ALL: [(Self, &'static str); 22] = [
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
        (Self::Weight, "weight"),
        (Self::Tracking, "tracking"),
        (Self::LineHeight, "line_height"),
        (Self::Color, "color"),
        (Self::Align, "align"),
        (Self::Cap, "cap"),
        (Self::Join, "join"),
        (Self::Source, "source"),
        (Self::Fit, "fit"),
    ];

    /// Whether the field has keyframes (D-0028).
    ///
    /// Content fields are scalar channels: none of them has axes. Text,
    /// alignment, weight, cap, join, image source and fit are discrete and
    /// cannot be animated.
    #[must_use]
    pub const fn is_animatable(self) -> bool {
        !matches!(
            self,
            Self::Text
                | Self::Align
                | Self::Weight
                | Self::Cap
                | Self::Join
                | Self::Source
                | Self::Fit
        )
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
    /// Image fit for `fit`.
    Fit(ImageFit),
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

/// A block of text. Line 0's baseline passes through the node origin.
#[derive(Debug, Clone, PartialEq)]
pub struct TextContent {
    /// The characters to draw; `\n` starts a new line.
    pub text: String,
    /// Requested font family name.
    pub font: String,
    /// Font weight (100..=900); resolved to the nearest static face.
    pub weight: u16,
    /// Font size in pixels (em height); animatable (`size`).
    pub size: Animatable<f64>,
    /// Extra advance between characters in em units; animatable (`tracking`).
    pub tracking: Animatable<f64>,
    /// Multiplier of the font's natural line step; animatable (`line_height`).
    pub line_height: Animatable<f64>,
    /// Glyph color; animatable (`color`).
    pub color: Animatable<Color>,
    /// Horizontal alignment, applied per line.
    pub align: TextAlign,
    /// Glyph fill gradient (D-0030); takes precedence over color when present.
    pub gradient: Option<Gradient>,
    /// Per-character animators (D-0034), in application order.
    pub animators: Vec<TextAnimator>,
}

impl TextContent {
    /// Creates a text block from static values.
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
            weight: 400,
            size: Animatable::new(size),
            tracking: Animatable::new(0.0),
            line_height: Animatable::new(1.0),
            color: Animatable::new(color),
            align,
            gradient: None,
            animators: Vec::new(),
        }
    }
}

/// One property a [`TextAnimator`] can animate on every character.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CharProperty {
    /// Glyph opacity multiplier in `0.0..=1.0`.
    Opacity,
    /// Glyph offset in local pixels.
    Offset,
    /// Uniform glyph scale around the glyph's pen origin.
    Scale,
    /// Glyph rotation in degrees around the glyph's pen origin.
    Rotation,
    /// Glyph colour.
    Color,
}

impl CharProperty {
    /// Every property with its MDF name.
    pub const ALL: [(Self, &'static str); 5] = [
        (Self::Opacity, "opacity"),
        (Self::Offset, "offset"),
        (Self::Scale, "scale"),
        (Self::Rotation, "rotation"),
        (Self::Color, "color"),
    ];

    /// Looks up a property by its MDF name.
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
            .map_or("opacity", |(_, name)| name)
    }
}

/// The order in which characters enter a [`TextAnimator`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum CharOrder {
    /// First character first.
    #[default]
    Forward,
    /// Last character first.
    Reverse,
    /// Characters closest to the block centre first.
    Center,
}

impl CharOrder {
    /// Every order with its MDF name.
    pub const ALL: [(Self, &'static str); 3] = [
        (Self::Forward, "forward"),
        (Self::Reverse, "reverse"),
        (Self::Center, "center"),
    ];

    /// Looks up an order by its MDF name.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .find(|(_, candidate)| *candidate == name)
            .map(|(order, _)| *order)
    }

    /// The MDF name of this order.
    #[must_use]
    pub fn name(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(order, _)| *order == self)
            .map_or("forward", |(_, name)| name)
    }
}

/// A typed value of a [`CharProperty`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CharValue {
    /// A number for `opacity`, `scale` or `rotation`.
    Number(f64),
    /// A pair for `offset`.
    Vec2(Vec2),
    /// A colour for `color`.
    Color(Color),
}

impl CharValue {
    /// The value as a number when it has one.
    #[must_use]
    pub fn number(self) -> Option<f64> {
        match self {
            Self::Number(value) => Some(value),
            Self::Vec2(_) | Self::Color(_) => None,
        }
    }

    /// The value as a pair when it has one.
    #[must_use]
    pub fn vec2(self) -> Option<Vec2> {
        match self {
            Self::Vec2(value) => Some(value),
            Self::Number(_) | Self::Color(_) => None,
        }
    }

    /// The value as a colour when it has one.
    #[must_use]
    pub fn color(self) -> Option<Color> {
        match self {
            Self::Color(value) => Some(value),
            Self::Number(_) | Self::Vec2(_) => None,
        }
    }
}

/// One per-character animation applied to a text node (D-0034).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextAnimator {
    /// Property this animator drives.
    pub property: CharProperty,
    /// Value before the character starts.
    pub from: CharValue,
    /// Value to reach once the character finishes.
    pub to: CharValue,
    /// First frame of the first character.
    pub start: Frame,
    /// Frames between consecutive characters.
    pub stagger: f64,
    /// Frames one character takes to go from `from` to `to`.
    pub duration: f64,
    /// Ease of the per-character segment.
    pub ease: Ease,
    /// Character order.
    pub order: CharOrder,
}

impl TextAnimator {
    /// The override property that carries a text node's whole animator list.
    ///
    /// The value is the canonical [`TextAnimator::encode_list`] text, so the
    /// overrides file can hold it as a plain string.
    pub const OVERRIDE_PROPERTY: &'static str = "text_animators";

    /// Validates that `from` and `to` match the property's value kind.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidContentValue`] when a value has the wrong
    /// shape for the property.
    #[allow(clippy::too_many_arguments)] // the authored animator call, kept flat
    pub fn new(
        property: CharProperty,
        from: CharValue,
        to: CharValue,
        start: Frame,
        stagger: f64,
        duration: f64,
        ease: Ease,
        order: CharOrder,
    ) -> Result<Self, CoreError> {
        let matches = match property {
            CharProperty::Opacity | CharProperty::Scale | CharProperty::Rotation => {
                from.number().is_some() && to.number().is_some()
            }
            CharProperty::Offset => from.vec2().is_some() && to.vec2().is_some(),
            CharProperty::Color => from.color().is_some() && to.color().is_some(),
        };
        if !matches {
            return Err(CoreError::InvalidContentValue {
                field: property.name().to_owned(),
                reason: "from and to must match the animator's property kind",
            });
        }
        if !stagger.is_finite() || !duration.is_finite() || stagger < 0.0 || duration <= 0.0 {
            return Err(CoreError::InvalidContentValue {
                field: property.name().to_owned(),
                reason: "stagger must be finite and >= 0 and duration must be finite and > 0",
            });
        }
        Ok(Self {
            property,
            from,
            to,
            start,
            stagger,
            duration,
            ease,
            order,
        })
    }

    /// The canonical one-line text of this animator.
    ///
    /// Fields are `property:from:to:start:stagger:duration:ease:order`, with
    /// numbers in the same exact decimal form the overrides file uses.  A list
    /// joins animators with `;`.
    #[must_use]
    pub fn encode(&self) -> String {
        format!(
            "{}:{}:{}:{}:{}:{}:{}:{}",
            self.property.name(),
            encode_char_value(self.from),
            encode_char_value(self.to),
            crate::overrides::format_number(frame_as_f64(self.start)),
            crate::overrides::format_number(self.stagger),
            crate::overrides::format_number(self.duration),
            self.ease.name(),
            self.order.name()
        )
    }

    /// Encodes a whole animator list with [`TextAnimator::encode`].
    #[must_use]
    pub fn encode_list(animators: &[Self]) -> String {
        animators
            .iter()
            .map(Self::encode)
            .collect::<Vec<_>>()
            .join(";")
    }

    /// Parses [`TextAnimator::encode`] text (one animator or a `;`-joined
    /// list).  An empty string is an empty list.
    ///
    /// # Errors
    ///
    /// Returns a human-readable reason naming the malformed field.
    pub fn decode_list(text: &str) -> Result<Vec<Self>, String> {
        let text = text.trim();
        if text.is_empty() {
            return Ok(Vec::new());
        }
        text.split(';').map(Self::decode).collect()
    }

    /// Parses one [`TextAnimator::encode`] line.
    ///
    /// # Errors
    ///
    /// Returns a human-readable reason naming the malformed field.
    pub fn decode(text: &str) -> Result<Self, String> {
        let fields: Vec<&str> = text.trim().split(':').collect();
        if fields.len() != 8 {
            return Err(format!(
                "an animator needs 8 ':'-separated fields (property:from:to:start:stagger:duration:ease:order), got {}",
                fields.len()
            ));
        }
        let property = CharProperty::from_name(fields[0])
            .ok_or_else(|| format!("unknown animator property {:?}", fields[0]))?;
        let from = decode_char_value(property, fields[1])?;
        let to = decode_char_value(property, fields[2])?;
        let start = parse_frame(fields[3])?;
        let stagger = parse_number(fields[4], "stagger")?;
        let duration = parse_number(fields[5], "duration")?;
        let ease =
            Ease::from_name(fields[6]).ok_or_else(|| format!("unknown ease {:?}", fields[6]))?;
        let order = CharOrder::from_name(fields[7])
            .ok_or_else(|| format!("unknown order {:?}", fields[7]))?;
        Self::new(property, from, to, start, stagger, duration, ease, order)
            .map_err(|error| error.to_string())
    }
}

/// A frame as `f64` (frames stay far below 2^53).
#[allow(clippy::cast_precision_loss)]
fn frame_as_f64(frame: Frame) -> f64 {
    frame as f64
}

fn encode_char_value(value: CharValue) -> String {
    match value {
        CharValue::Number(number) => crate::overrides::format_number(number),
        CharValue::Vec2(vector) => format!(
            "({}, {})",
            crate::overrides::format_number(vector.x),
            crate::overrides::format_number(vector.y)
        ),
        CharValue::Color(color) => color.to_hex(),
    }
}

fn decode_char_value(property: CharProperty, text: &str) -> Result<CharValue, String> {
    match property {
        CharProperty::Opacity | CharProperty::Scale | CharProperty::Rotation => {
            Ok(CharValue::Number(parse_number(text, property.name())?))
        }
        CharProperty::Offset => {
            let inner = text
                .strip_prefix('(')
                .and_then(|rest| rest.strip_suffix(')'))
                .ok_or_else(|| format!("offset expects an (x, y) pair, got {text:?}"))?;
            let (x, y) = inner
                .split_once(',')
                .ok_or_else(|| format!("offset expects an (x, y) pair, got {text:?}"))?;
            Ok(CharValue::Vec2(Vec2::new(
                parse_number(x, "offset x")?,
                parse_number(y, "offset y")?,
            )))
        }
        CharProperty::Color => Color::parse_hex(text)
            .map(CharValue::Color)
            .map_err(|_| format!("color expects #RRGGBB[AA], got {text:?}")),
    }
}

fn parse_number(text: &str, what: &str) -> Result<f64, String> {
    let value = text
        .trim()
        .parse::<f64>()
        .map_err(|_| format!("{what} expects a number, got {text:?}"))?;
    if value.is_finite() {
        Ok(value)
    } else {
        Err(format!("{what} must be finite, got {text:?}"))
    }
}

fn parse_frame(text: &str) -> Result<Frame, String> {
    let value = parse_number(text, "start")?;
    if value.fract() != 0.0 {
        return Err(format!("start must be a whole frame, got {text:?}"));
    }
    #[allow(clippy::cast_possible_truncation)] // checked to be integral and finite
    Ok(value as Frame)
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

/// The content key of an image: the SHA-256 of its bytes (D-0035).
///
/// Two paths with identical bytes share one key (and one GPU texture); a
/// changed file is a new key.  The key is computed by the loader, never by the
/// document or the evaluator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ImageKey([u8; 32]);

impl ImageKey {
    /// Wraps the 32 bytes of a SHA-256 digest.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// The raw digest bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Lower-case hex, the form `query` and the renderer use.
    #[must_use]
    pub fn to_hex(&self) -> String {
        let mut text = String::with_capacity(64);
        for byte in self.0 {
            use std::fmt::Write as _;
            let _ = write!(text, "{byte:02x}");
        }
        text
    }
}

impl fmt::Display for ImageKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

/// A resolved image: its content key and natural pixel size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ImageAsset {
    /// SHA-256 of the file bytes.
    pub key: ImageKey,
    /// Natural width in pixels.
    pub width: u32,
    /// Natural height in pixels.
    pub height: u32,
}

/// How an image fills its box (D-0035).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ImageFit {
    /// Letterbox inside the box; the whole image is visible.
    #[default]
    Contain,
    /// Fill the box and crop the overflow through `uv_rect`.
    Cover,
    /// Stretch to the box.
    Fill,
}

impl ImageFit {
    /// Every fit with its MDF name.
    pub const ALL: [(Self, &'static str); 3] = [
        (Self::Contain, "contain"),
        (Self::Cover, "cover"),
        (Self::Fill, "fill"),
    ];

    /// Looks up a fit by its MDF name.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .find(|(_, candidate)| *candidate == name)
            .map(|(fit, _)| *fit)
    }

    /// The MDF name of this fit.
    #[must_use]
    pub fn name(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(fit, _)| *fit == self)
            .map_or("contain", |(_, name)| name)
    }
}

/// The destination rectangle and UV crop of one image at one frame.
///
/// The rectangle is centred on the node origin.  For `Contain` it is smaller
/// than the box (the letterbox is transparent); for `Cover` it is the box and
/// `uv` crops the source; for `Fill` it is the box and `uv` is full.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImageLayout {
    /// Drawn width in local pixels.
    pub width: f64,
    /// Drawn height in local pixels.
    pub height: f64,
    /// Source crop `[u0, v0, u1, v1]` in UV space.
    pub uv: [f64; 4],
}

impl ImageLayout {
    /// Computes the layout for a sampled box and a natural size.
    ///
    /// A zero `box_width` / `box_height` means "natural": both zero uses the
    /// image's pixel size, one zero keeps the aspect from the other.  An
    /// unresolved image (no natural size) falls back to a 100×100 box so the
    /// preview can draw a placeholder.
    #[must_use]
    pub fn compute(
        box_width: f64,
        box_height: f64,
        fit: ImageFit,
        natural: Option<(u32, u32)>,
    ) -> Self {
        let (natural_width, natural_height) = natural
            .map(|(width, height)| (f64::from(width), f64::from(height)))
            .filter(|(width, height)| *width > 0.0 && *height > 0.0)
            .unwrap_or((100.0, 100.0));
        let width = if box_width > 0.0 {
            box_width
        } else if box_height > 0.0 {
            box_height * natural_width / natural_height
        } else {
            natural_width
        };
        let height = if box_height > 0.0 {
            box_height
        } else if box_width > 0.0 {
            box_width * natural_height / natural_width
        } else {
            natural_height
        };
        match fit {
            ImageFit::Fill => Self {
                width,
                height,
                uv: [0.0, 0.0, 1.0, 1.0],
            },
            ImageFit::Contain => {
                let scale = (width / natural_width).min(height / natural_height);
                Self {
                    width: natural_width * scale,
                    height: natural_height * scale,
                    uv: [0.0, 0.0, 1.0, 1.0],
                }
            }
            ImageFit::Cover => {
                let scale = (width / natural_width).max(height / natural_height);
                let visible_width = (width / (natural_width * scale)).clamp(0.0, 1.0);
                let visible_height = (height / (natural_height * scale)).clamp(0.0, 1.0);
                let u0 = (1.0 - visible_width) * 0.5;
                let v0 = (1.0 - visible_height) * 0.5;
                Self {
                    width,
                    height,
                    uv: [u0, v0, u0 + visible_width, v0 + visible_height],
                }
            }
        }
    }
}

/// An image node's content (D-0035).
///
/// `source` is a project-relative path.  `asset` is filled by
/// [`crate::Document::resolve_images`] once the loader has read the bytes;
/// an unresolved node still evaluates, so the preview can draw a placeholder,
/// but `still`/`render` refuse it.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageContent {
    /// Project-relative source path.
    pub source: String,
    /// Resolved key and natural size, when the loader has seen the file.
    pub asset: Option<ImageAsset>,
    /// Destination box width in pixels; `0` keeps the natural width/aspect.
    pub width: Animatable<f64>,
    /// Destination box height in pixels; `0` keeps the natural height/aspect.
    pub height: Animatable<f64>,
    /// How the image fills its box.
    pub fit: ImageFit,
}

impl ImageContent {
    /// Creates an unresolved image node from a source path.
    #[must_use]
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            asset: None,
            width: Animatable::new(0.0),
            height: Animatable::new(0.0),
            fit: ImageFit::Contain,
        }
    }
}

/// One image that could not be resolved, with the reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageIssue {
    /// The node whose source failed.
    pub node: NodeId,
    /// The project-relative path that was tried.
    pub source: String,
    /// Why it failed.
    pub message: String,
}

/// One decoded image ready for upload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedImage {
    /// The resolved asset (key and natural size).
    pub asset: ImageAsset,
    /// Straight-alpha sRGB RGBA8 bytes.
    pub rgba8: Vec<u8>,
}

/// Resolves project-relative image paths to content keys and natural sizes.
///
/// The loader (`motioner_io`) implements this; core, DSL and eval stay free of
/// the file system.  `decoded` is the bridge to the renderer's texture upload:
/// the loader returns every image it has decoded, and the UI converts the
/// bytes into renderer upload records without naming the loader's types.
pub trait ImageResolver {
    /// Resolves one path.
    ///
    /// # Errors
    ///
    /// Returns a human-readable reason for a missing file, an invalid path or
    /// an undecodable image.
    fn resolve(&self, source: &str) -> Result<ImageAsset, String>;

    /// Every decoded image, if the resolver keeps bytes.
    fn decoded(&self) -> Vec<DecodedImage> {
        Vec::new()
    }
}

/// A resolver that rejects every path: the "no assets installed" case.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoImages;

impl ImageResolver for NoImages {
    fn resolve(&self, _source: &str) -> Result<ImageAsset, String> {
        Err("no image loader is installed".to_owned())
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
    /// A raster image centred on the node origin (D-0035).
    Image(ImageContent),
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
            Self::Image(_) => "image",
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
                        | ContentField::Weight
                        | ContentField::Tracking
                        | ContentField::LineHeight
                        | ContentField::Color
                        | ContentField::Align
                )
            }
            Self::Image(_) => matches!(
                field,
                ContentField::Width
                    | ContentField::Height
                    | ContentField::Source
                    | ContentField::Fit
            ),
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
            Self::Text(_) | Self::Group | Self::Image(_) => 0,
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
            Self::Group | Self::Image(_) => false,
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
            (Self::Text(text), ContentField::Tracking) => number(&text.tracking),
            (Self::Text(text), ContentField::LineHeight) => number(&text.line_height),
            (Self::Text(text), ContentField::Color) => Some(ContentChannel::Color(&text.color)),
            (Self::Image(image), ContentField::Width) => number(&image.width),
            (Self::Image(image), ContentField::Height) => number(&image.height),
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
                Self::Group | Self::Image(_) => return None,
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
            (Self::Text(text), ContentField::Weight) => {
                Some(ContentValue::Number(f64::from(text.weight)))
            }
            (Self::Text(text), ContentField::Tracking) => {
                Some(ContentValue::Number(text.tracking.value()))
            }
            (Self::Text(text), ContentField::LineHeight) => {
                Some(ContentValue::Number(text.line_height.value()))
            }
            (Self::Text(text), ContentField::Color) => Some(match &text.gradient {
                Some(gradient) => ContentValue::Gradient(Some(gradient.clone())),
                None => ContentValue::Color(Some(text.color.value())),
            }),
            (Self::Text(text), ContentField::Align) => Some(ContentValue::Align(text.align)),
            (Self::Image(image), ContentField::Width) => {
                Some(ContentValue::Number(image.width.value()))
            }
            (Self::Image(image), ContentField::Height) => {
                Some(ContentValue::Number(image.height.value()))
            }
            (Self::Image(image), ContentField::Source) => {
                Some(ContentValue::Text(image.source.clone()))
            }
            (Self::Image(image), ContentField::Fit) => Some(ContentValue::Fit(image.fit)),
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
                } else if bad(size) || size == 0.0 {
                    Some("size")
                } else if !text.tracking.value().is_finite() {
                    Some("tracking")
                } else if !text.line_height.value().is_finite() || text.line_height.value() <= 0.0 {
                    Some("line_height")
                } else {
                    None
                }
            }
            Self::Image(image) => {
                let width = image.width.value();
                let height = image.height.value();
                if bad(width) {
                    Some("width")
                } else if bad(height) {
                    Some("height")
                } else {
                    None
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
    pub(crate) effects: Vec<crate::effect::Effect>,
    pub(crate) mask: Option<crate::mask::MaskSpec>,
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

    /// The ordered effect stack (bottom to top).
    ///
    /// Empty for a node without effects. At most one effect of each
    /// [`EffectKind`](crate::EffectKind) can be present.
    #[must_use]
    pub fn effects(&self) -> &[crate::effect::Effect] {
        &self.effects
    }

    /// The effect of `kind`, if the node has it.
    #[must_use]
    pub fn effect(&self, kind: crate::EffectKind) -> Option<&crate::effect::Effect> {
        self.effects.iter().find(|effect| effect.is_kind(kind))
    }

    /// The mask applied to this node, if any (D-0033).
    ///
    /// The source node is not drawn on its own; its subtree supplies the
    /// coverage of this node's offscreen layer.
    #[must_use]
    pub fn mask(&self) -> Option<crate::mask::MaskSpec> {
        self.mask
    }
}

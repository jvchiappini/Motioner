//! The ordered per-node effect stack (D-0028 section 3).
//!
//! An effect is a parameter set, not a special case: every parameter is a
//! normal animatable channel addressed by name (`drop_shadow.blur`,
//! `glow.radius`, `drop_shadow.offset.x`), so `key`, `edit`, overrides, bake
//! and `query` work on it with no extra machinery.
//!
//! A node holds at most one effect of each kind, so `<kind>.<param>` is a
//! unique property name; the order of the vector is the order of application
//! ("bottom to top"). Effects apply to the node **and its whole subtree**: a
//! node with effects renders its subtree into an offscreen layer, the chain
//! runs over that image, and the result composites with the node's blend mode.

use crate::animation::{Animatable, AnimatableVec2, Axis};
use crate::color::Color;
use crate::error::CoreError;
use crate::math::Vec2;

/// The kinds of effect a node can carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum EffectKind {
    /// A tinted, blurred, offset copy of the subtree drawn behind it.
    DropShadow,
    /// A Gaussian blur of the whole subtree.
    Blur,
    /// A tinted, blurred copy of the subtree added as light around it.
    Glow,
}

impl EffectKind {
    /// Every kind, in inspector order.
    pub const ALL: [Self; 3] = [Self::DropShadow, Self::Blur, Self::Glow];

    /// Looks up a kind by its MDF name.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "drop_shadow" => Some(Self::DropShadow),
            "blur" => Some(Self::Blur),
            "glow" => Some(Self::Glow),
            _ => None,
        }
    }

    /// The MDF name of this kind.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::DropShadow => "drop_shadow",
            Self::Blur => "blur",
            Self::Glow => "glow",
        }
    }

    /// The parameters this kind accepts, in display order.
    #[must_use]
    pub const fn params(self) -> &'static [EffectParam] {
        match self {
            Self::DropShadow => &[
                EffectParam::Color,
                EffectParam::Offset,
                EffectParam::Blur,
                EffectParam::Opacity,
            ],
            Self::Blur => &[EffectParam::Radius],
            Self::Glow => &[
                EffectParam::Color,
                EffectParam::Radius,
                EffectParam::Intensity,
            ],
        }
    }

    /// Whether the kind accepts `param`.
    #[must_use]
    pub fn accepts(self, param: EffectParam) -> bool {
        self.params().contains(&param)
    }

    /// A human label for the inspector and errors.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::DropShadow => "Drop shadow",
            Self::Blur => "Blur",
            Self::Glow => "Glow",
        }
    }
}

/// One animatable parameter of an effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EffectParam {
    /// Tint colour (drop shadow, glow).
    Color,
    /// Offset in pixels, Y down (drop shadow).
    Offset,
    /// Blur radius in pixels (drop shadow, blur).
    Blur,
    /// Shadow opacity in `0.0..=1.0` (drop shadow).
    Opacity,
    /// Blur radius in pixels (glow).
    Radius,
    /// Added light strength (glow).
    Intensity,
}

impl EffectParam {
    /// The MDF name of the parameter.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Color => "color",
            Self::Offset => "offset",
            Self::Blur => "blur",
            Self::Opacity => "opacity",
            Self::Radius => "radius",
            Self::Intensity => "intensity",
        }
    }

    /// The parameter name, if `name` is one.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|param| param.name() == name)
    }

    /// Every parameter name, for error messages.
    pub const ALL: [Self; 6] = [
        Self::Color,
        Self::Offset,
        Self::Blur,
        Self::Opacity,
        Self::Radius,
        Self::Intensity,
    ];

    /// Whether the parameter is a 2D vector (has `x`/`y` axes).
    #[must_use]
    pub const fn is_vector(self) -> bool {
        matches!(self, Self::Offset)
    }
}

/// One fully-addressed animatable channel of an effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EffectChannel {
    /// The effect kind.
    pub kind: EffectKind,
    /// The parameter.
    pub param: EffectParam,
    /// The axis, for a vector parameter.
    pub axis: Option<Axis>,
}

impl EffectChannel {
    /// Builds a channel, rejecting an axis on a scalar parameter.
    ///
    /// # Errors
    ///
    /// [`CoreError::WrongAxis`] when `param` is scalar and `axis` is present,
    /// or when `param` is a vector and `axis` is absent.
    pub fn new(
        kind: EffectKind,
        param: EffectParam,
        axis: Option<Axis>,
    ) -> Result<Self, CoreError> {
        if kind.accepts(param) && param.is_vector() != axis.is_some() {
            return Err(CoreError::WrongAxis {
                property: param.name(),
                axis: axis.map(Axis::name),
            });
        }
        Ok(Self { kind, param, axis })
    }

    /// The override name **without** the axis: `drop_shadow.offset`.
    ///
    /// Editor key edits store the axis separately, so the base plus axis must
    /// reconstruct [`EffectChannel::name`].
    #[must_use]
    pub fn base_name(self) -> String {
        format!("{}.{}", self.kind.name(), self.param.name())
    }

    /// The override name: `drop_shadow.blur`, `drop_shadow.offset.x`.
    #[must_use]
    pub fn name(self) -> String {
        match self.axis {
            Some(axis) => format!("{}.{}.{}", self.kind.name(), self.param.name(), axis.name()),
            None => format!("{}.{}", self.kind.name(), self.param.name()),
        }
    }

    /// Parses a full channel name, e.g. `glow.radius`.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        let (kind, rest) = name.split_once('.')?;
        let kind = EffectKind::from_name(kind)?;
        let (param, axis) = if let Some((param, axis)) = rest.rsplit_once('.') {
            (param, Some(Axis::from_name(axis)?))
        } else {
            (rest, None)
        };
        let param = EffectParam::from_name(param)?;
        if !kind.accepts(param) || param.is_vector() != axis.is_some() {
            return None;
        }
        Some(Self { kind, param, axis })
    }
}

/// A resolved effect parameter reference, for reading the document.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EffectChannelRef<'a> {
    /// A scalar parameter.
    Number(&'a Animatable<f64>),
    /// A colour parameter.
    Color(&'a Animatable<Color>),
}

/// A mutable effect parameter reference, for writing the document.
#[derive(Debug)]
pub enum EffectChannelMut<'a> {
    /// A scalar parameter.
    Number(&'a mut Animatable<f64>),
    /// A colour parameter.
    Color(&'a mut Animatable<Color>),
}

/// The drop-shadow parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct DropShadowEffect {
    /// Tint colour; its alpha scales the shadow.
    pub color: Animatable<Color>,
    /// Offset in pixels, Y down.
    pub offset: AnimatableVec2,
    /// Blur radius in pixels.
    pub blur: Animatable<f64>,
    /// Shadow opacity in `0.0..=1.0`.
    pub opacity: Animatable<f64>,
}

/// The blur parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct BlurEffect {
    /// Blur radius in pixels.
    pub radius: Animatable<f64>,
}

/// The glow parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct GlowEffect {
    /// Tint colour; its alpha scales the glow.
    pub color: Animatable<Color>,
    /// Blur radius in pixels.
    pub radius: Animatable<f64>,
    /// Added light strength.
    pub intensity: Animatable<f64>,
}

/// One effect of a node's stack.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// A drop shadow ([`EffectKind::DropShadow`]).
    DropShadow(DropShadowEffect),
    /// A blur ([`EffectKind::Blur`]).
    Blur(BlurEffect),
    /// A glow ([`EffectKind::Glow`]).
    Glow(GlowEffect),
}

impl Effect {
    /// A new effect of `kind` with the authored defaults.
    #[must_use]
    pub fn new(kind: EffectKind) -> Self {
        match kind {
            EffectKind::DropShadow => Self::DropShadow(DropShadowEffect {
                color: Animatable::new(Color::rgba(0, 0, 0, 128)),
                offset: AnimatableVec2::new(Vec2::new(4.0, 6.0)),
                blur: Animatable::new(8.0),
                opacity: Animatable::new(0.75),
            }),
            EffectKind::Blur => Self::Blur(BlurEffect {
                radius: Animatable::new(8.0),
            }),
            EffectKind::Glow => Self::Glow(GlowEffect {
                color: Animatable::new(Color::rgba(255, 255, 255, 170)),
                radius: Animatable::new(12.0),
                intensity: Animatable::new(1.0),
            }),
        }
    }

    /// The effect's kind.
    #[must_use]
    pub const fn kind(&self) -> EffectKind {
        match self {
            Self::DropShadow(_) => EffectKind::DropShadow,
            Self::Blur(_) => EffectKind::Blur,
            Self::Glow(_) => EffectKind::Glow,
        }
    }

    /// Whether this effect is of `kind`.
    #[must_use]
    pub fn is_kind(&self, kind: EffectKind) -> bool {
        self.kind() == kind
    }

    /// The channel of `param` (and `axis` for the offset), if this effect has
    /// it.
    #[must_use]
    pub fn channel_ref(
        &self,
        param: EffectParam,
        axis: Option<Axis>,
    ) -> Option<EffectChannelRef<'_>> {
        match self {
            Self::DropShadow(effect) => match (param, axis) {
                (EffectParam::Color, None) => Some(EffectChannelRef::Color(&effect.color)),
                (EffectParam::Offset, Some(axis)) => {
                    Some(EffectChannelRef::Number(effect.offset.axis(axis)))
                }
                (EffectParam::Blur, None) => Some(EffectChannelRef::Number(&effect.blur)),
                (EffectParam::Opacity, None) => Some(EffectChannelRef::Number(&effect.opacity)),
                _ => None,
            },
            Self::Blur(effect) => match (param, axis) {
                (EffectParam::Radius, None) => Some(EffectChannelRef::Number(&effect.radius)),
                _ => None,
            },
            Self::Glow(effect) => match (param, axis) {
                (EffectParam::Color, None) => Some(EffectChannelRef::Color(&effect.color)),
                (EffectParam::Radius, None) => Some(EffectChannelRef::Number(&effect.radius)),
                (EffectParam::Intensity, None) => Some(EffectChannelRef::Number(&effect.intensity)),
                _ => None,
            },
        }
    }

    /// The mutable channel of `param` (and `axis`), if this effect has it.
    #[must_use]
    pub fn channel_mut(
        &mut self,
        param: EffectParam,
        axis: Option<Axis>,
    ) -> Option<EffectChannelMut<'_>> {
        match self {
            Self::DropShadow(effect) => match (param, axis) {
                (EffectParam::Color, None) => Some(EffectChannelMut::Color(&mut effect.color)),
                (EffectParam::Offset, Some(axis)) => {
                    Some(EffectChannelMut::Number(effect.offset.axis_mut(axis)))
                }
                (EffectParam::Blur, None) => Some(EffectChannelMut::Number(&mut effect.blur)),
                (EffectParam::Opacity, None) => Some(EffectChannelMut::Number(&mut effect.opacity)),
                _ => None,
            },
            Self::Blur(effect) => match (param, axis) {
                (EffectParam::Radius, None) => Some(EffectChannelMut::Number(&mut effect.radius)),
                _ => None,
            },
            Self::Glow(effect) => match (param, axis) {
                (EffectParam::Color, None) => Some(EffectChannelMut::Color(&mut effect.color)),
                (EffectParam::Radius, None) => Some(EffectChannelMut::Number(&mut effect.radius)),
                (EffectParam::Intensity, None) => {
                    Some(EffectChannelMut::Number(&mut effect.intensity))
                }
                _ => None,
            },
        }
    }
}

/// A value accepted by an effect channel write.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EffectValueInput {
    /// A number for a scalar parameter.
    Number(f64),
    /// A colour for `color`.
    Color(Color),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_names_round_trip() {
        for kind in EffectKind::ALL {
            assert_eq!(EffectKind::from_name(kind.name()), Some(kind));
            assert_eq!(Effect::new(kind).kind(), kind);
        }
        assert_eq!(EffectKind::from_name("wobble"), None);
    }

    #[test]
    fn channel_names_round_trip() {
        let cases = [
            (
                "drop_shadow.color",
                EffectKind::DropShadow,
                EffectParam::Color,
                None,
            ),
            (
                "drop_shadow.offset.x",
                EffectKind::DropShadow,
                EffectParam::Offset,
                Some(Axis::X),
            ),
            (
                "drop_shadow.blur",
                EffectKind::DropShadow,
                EffectParam::Blur,
                None,
            ),
            (
                "glow.intensity",
                EffectKind::Glow,
                EffectParam::Intensity,
                None,
            ),
            ("blur.radius", EffectKind::Blur, EffectParam::Radius, None),
        ];
        for (name, kind, param, axis) in cases {
            let channel = EffectChannel::from_name(name).unwrap();
            assert_eq!(channel.kind, kind);
            assert_eq!(channel.param, param);
            assert_eq!(channel.axis, axis);
            assert_eq!(channel.name(), name);
        }
        assert_eq!(EffectChannel::from_name("blur.color"), None);
        assert_eq!(EffectChannel::from_name("drop_shadow.offset"), None);
        assert_eq!(EffectChannel::from_name("drop_shadow.blur.x"), None);
    }

    #[test]
    fn channels_read_and_write_every_parameter() {
        for kind in EffectKind::ALL {
            let mut effect = Effect::new(kind);
            for param in kind.params() {
                let axes: &[Option<Axis>] = if param.is_vector() {
                    &[Some(Axis::X), Some(Axis::Y)]
                } else {
                    &[None]
                };
                for &axis in axes {
                    assert!(effect.channel_ref(*param, axis).is_some());
                    assert!(effect.channel_mut(*param, axis).is_some());
                }
            }
        }
    }
}

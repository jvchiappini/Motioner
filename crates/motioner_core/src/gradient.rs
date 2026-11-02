//! Linear and radial gradients with animatable stops (D-0030).
//!
//! A gradient is a static geometry in the node's local space plus a list of
//! stops whose offset and colour are ordinary [`Animatable`] channels, so a
//! stop is keyed, overridden and baked exactly like `fill` (D-0028).
//!
//! Sampling lives in `motioner_eval`; this module only owns the authored
//! value, its validity rules and the one stop lookup every caller uses.

use crate::animation::Animatable;
use crate::color::Color;
use crate::error::CoreError;
use crate::math::Vec2;

/// Most stops one gradient can hold.
///
/// The engine's per-frame stop table is sized from this; a larger list would
/// make a single gradient unbounded and could exhaust the upload buffer.
pub const MAX_GRADIENT_STOPS: usize = 16;

/// Geometry of a gradient, in the node's local pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GradientKind {
    /// Colour varies along the segment `start -> end`; `t` is the projection
    /// of the local position onto that segment.
    Linear {
        /// Start of the ramp (t = 0).
        start: Vec2,
        /// End of the ramp (t = 1).
        end: Vec2,
    },
    /// Colour varies with the distance from `center`; `t = distance / radius`.
    Radial {
        /// Circle centre.
        center: Vec2,
        /// Radius at which t = 1, in local pixels.
        radius: f64,
    },
}

impl GradientKind {
    /// The MDF name ("linear" or "radial").
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Linear { .. } => "linear",
            Self::Radial { .. } => "radial",
        }
    }

    /// The two geometry values the engine pass consumes, in local pixels:
    /// `(start, end)` for linear and `(center, radius)` for radial.
    #[must_use]
    pub const fn geometry(self) -> (Vec2, Vec2) {
        match self {
            Self::Linear { start, end } => (start, end),
            Self::Radial { center, radius } => (center, Vec2::new(radius, 0.0)),
        }
    }

    /// Checks that the geometry is finite and would draw something.
    #[must_use]
    pub fn validate(self) -> Option<&'static str> {
        match self {
            Self::Linear { start, end } => {
                if !start.is_finite() || !end.is_finite() {
                    Some("gradient endpoints must be finite")
                } else if start == end {
                    Some("a linear gradient needs two different endpoints")
                } else {
                    None
                }
            }
            Self::Radial { center, radius } => {
                if !center.is_finite() {
                    Some("the gradient centre must be finite")
                } else if !radius.is_finite() || radius <= 0.0 {
                    Some("a radial gradient needs a positive radius")
                } else {
                    None
                }
            }
        }
    }
}

/// One colour stop. Both parts are content channels (D-0028, D-0030).
#[derive(Debug, Clone, PartialEq)]
pub struct GradientStop {
    /// Position along the ramp, `0.0..=1.0`.
    pub offset: Animatable<f64>,
    /// Colour at that position.
    pub color: Animatable<Color>,
}

impl GradientStop {
    /// A stop from static values.
    #[must_use]
    pub fn new(offset: f64, color: Color) -> Self {
        Self {
            offset: Animatable::new(offset),
            color: Animatable::new(color),
        }
    }

    /// Why this stop's static values are invalid, if any.
    #[must_use]
    pub fn invalid(&self) -> Option<&'static str> {
        let bad_offset = |value: f64| !value.is_finite() || !(0.0..=1.0).contains(&value);
        if bad_offset(self.offset.value())
            || self.offset.keys().iter().any(|key| bad_offset(key.value))
        {
            Some("gradient stop offsets must be finite numbers from 0 to 1")
        } else {
            None
        }
    }
}

/// An authored gradient: static geometry plus animatable stops.
#[derive(Debug, Clone, PartialEq)]
pub struct Gradient {
    kind: GradientKind,
    stops: Vec<GradientStop>,
}

impl Gradient {
    /// Builds and validates a gradient.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidGradient`] when the geometry is not
    /// drawable, the stop count is outside `2..=MAX_GRADIENT_STOPS`, or a
    /// stop is out of range.
    pub fn new(kind: GradientKind, stops: Vec<GradientStop>) -> Result<Self, CoreError> {
        let gradient = Self { kind, stops };
        if let Some(reason) = gradient.invalid() {
            return Err(CoreError::InvalidGradient { reason });
        }
        Ok(gradient)
    }

    /// A linear gradient from static `(offset, colour)` stops.
    ///
    /// # Errors
    ///
    /// See [`Gradient::new`].
    pub fn linear(
        start: Vec2,
        end: Vec2,
        stops: impl IntoIterator<Item = (f64, Color)>,
    ) -> Result<Self, CoreError> {
        Self::new(GradientKind::Linear { start, end }, stops_of(stops))
    }

    /// A radial gradient from static `(offset, colour)` stops.
    ///
    /// # Errors
    ///
    /// See [`Gradient::new`].
    pub fn radial(
        center: Vec2,
        radius: f64,
        stops: impl IntoIterator<Item = (f64, Color)>,
    ) -> Result<Self, CoreError> {
        Self::new(GradientKind::Radial { center, radius }, stops_of(stops))
    }

    /// The gradient geometry.
    #[must_use]
    pub const fn kind(&self) -> GradientKind {
        self.kind
    }

    /// The stops in authored order.
    #[must_use]
    pub fn stops(&self) -> &[GradientStop] {
        &self.stops
    }

    /// The stop at `index`.
    #[must_use]
    pub fn stop(&self, index: usize) -> Option<&GradientStop> {
        self.stops.get(index)
    }

    /// The stop at `index`, mutably.
    #[must_use]
    pub fn stop_mut(&mut self, index: usize) -> Option<&mut GradientStop> {
        self.stops.get_mut(index)
    }

    /// Number of stops.
    #[must_use]
    pub fn len(&self) -> usize {
        self.stops.len()
    }

    /// Whether the gradient has no stops (never valid, but useful for code
    /// that inspects a deserialised value).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.stops.is_empty()
    }

    /// The first stop's static colour, used as the solid fallback.
    #[must_use]
    pub fn fallback_color(&self) -> Color {
        self.stops
            .first()
            .map_or(Color::WHITE, |stop| stop.color.value())
    }

    /// Why this gradient is not drawable, if it is not.
    #[must_use]
    pub fn invalid(&self) -> Option<&'static str> {
        if !(2..=MAX_GRADIENT_STOPS).contains(&self.stops.len()) {
            return Some("a gradient needs between 2 and 16 stops");
        }
        if let Some(reason) = self.kind.validate() {
            return Some(reason);
        }
        self.stops.iter().find_map(GradientStop::invalid)
    }
}

/// The part of a gradient stop a [`crate::ContentField`] addresses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StopPart {
    /// The `offset` channel.
    Offset,
    /// The `color` channel.
    Color,
}

/// Which paint slot owns the gradient a field addresses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PaintSlot {
    /// `fill`.
    Fill,
    /// `stroke`.
    Stroke,
}

fn stops_of(stops: impl IntoIterator<Item = (f64, Color)>) -> Vec<GradientStop> {
    stops
        .into_iter()
        .map(|(offset, color)| GradientStop::new(offset, color))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_geometry_and_stops() {
        let two = [(0.0, Color::BLACK), (1.0, Color::WHITE)];
        assert!(Gradient::linear(Vec2::ZERO, Vec2::new(10.0, 0.0), two).is_ok());
        assert!(Gradient::linear(Vec2::ZERO, Vec2::ZERO, two).is_err());
        assert!(Gradient::radial(Vec2::ZERO, 0.0, two).is_err());
        assert!(Gradient::radial(Vec2::ZERO, 10.0, two).is_ok());
        assert!(Gradient::radial(Vec2::ZERO, 10.0, [(0.0, Color::BLACK)]).is_err());
        assert!(
            Gradient::radial(Vec2::ZERO, 10.0, [(0.0, Color::BLACK), (1.5, Color::WHITE)]).is_err()
        );
        let many = (0..=MAX_GRADIENT_STOPS).map(|i| {
            let offset = u32::try_from(i).map_or(0.0, f64::from);
            (offset / 20.0, Color::WHITE)
        });
        assert!(Gradient::radial(Vec2::ZERO, 10.0, many).is_err());
    }

    #[test]
    fn geometry_round_trips_for_the_engine() {
        let linear = GradientKind::Linear {
            start: Vec2::new(1.0, 2.0),
            end: Vec2::new(3.0, 4.0),
        };
        assert_eq!(
            linear.geometry(),
            (Vec2::new(1.0, 2.0), Vec2::new(3.0, 4.0))
        );
        let radial = GradientKind::Radial {
            center: Vec2::new(5.0, 6.0),
            radius: 7.0,
        };
        assert_eq!(
            radial.geometry(),
            (Vec2::new(5.0, 6.0), Vec2::new(7.0, 0.0))
        );
    }

    #[test]
    fn keyed_offsets_are_validated_too() {
        let mut gradient =
            Gradient::radial(Vec2::ZERO, 10.0, [(0.0, Color::BLACK), (1.0, Color::WHITE)]).unwrap();
        gradient.stops[0].offset.insert_key(crate::Keyframe {
            frame: 0,
            value: 2.0,
            ease: crate::Ease::Linear,
        });
        assert!(gradient.invalid().is_some());
    }
}

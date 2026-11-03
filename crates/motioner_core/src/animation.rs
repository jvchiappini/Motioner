//! Animatable values and keyframes.
//!
//! An [`Animatable`] holds a static value plus an ordered keyframe list.
//! Once a property has keyframes they define its value at every frame;
//! sampling lives in `motioner_eval`.

use crate::time::Frame;

/// A CSS `cubic-bezier(x1, y1, x2, y2)` control pair.
///
/// The four numbers are quantised to one millionth and stored as `i32`, so
/// [`Ease`] keeps an honest `Eq`/`Hash`: two beziers are equal exactly when
/// their displayed numbers are equal.  `x1` and `x2` must lie in `[0, 1]`
/// (otherwise the curve is not a function of time); `y1` and `y2` may
/// overshoot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BezierEase {
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
}

/// The scale that turns a control value into the stored integer.
const BEZIER_SCALE: f64 = 1_000_000.0;
/// Largest control value that fits the quantised `i32` representation.
const BEZIER_MAX: f64 = 2_000.0;

impl BezierEase {
    /// Creates a validated cubic-bezier control pair.
    ///
    /// # Errors
    ///
    /// Returns [`BezierError::XOutOfRange`] when `x1` or `x2` is outside
    /// `[0, 1]`, [`BezierError::NotFinite`] for a non-finite component and
    /// [`BezierError::TooLarge`] when `|y|` is above 2000.
    pub fn new(x1: f64, y1: f64, x2: f64, y2: f64) -> Result<Self, BezierError> {
        for (value, what) in [(x1, "x1"), (y1, "y1"), (x2, "x2"), (y2, "y2")] {
            if !value.is_finite() {
                return Err(BezierError::NotFinite { what });
            }
        }
        for (value, what) in [(x1, "x1"), (x2, "x2")] {
            if !(0.0..=1.0).contains(&value) {
                return Err(BezierError::XOutOfRange { what, value });
            }
        }
        for (value, what) in [(y1, "y1"), (y2, "y2")] {
            if value.abs() > BEZIER_MAX {
                return Err(BezierError::TooLarge { what, value });
            }
        }
        Ok(Self {
            x1: quantise(x1),
            y1: quantise(y1),
            x2: quantise(x2),
            y2: quantise(y2),
        })
    }

    /// The four control values as displayed.
    #[must_use]
    pub fn points(self) -> (f64, f64, f64, f64) {
        (
            dequantise(self.x1),
            dequantise(self.y1),
            dequantise(self.x2),
            dequantise(self.y2),
        )
    }

    /// The MDF call form, `bezier(x1, y1, x2, y2)`.
    #[must_use]
    pub fn to_mdf(self) -> String {
        let (x1, y1, x2, y2) = self.points();
        format!(
            "bezier({}, {}, {}, {})",
            crate::overrides::format_number(x1),
            crate::overrides::format_number(y1),
            crate::overrides::format_number(x2),
            crate::overrides::format_number(y2)
        )
    }
}

/// Why a cubic-bezier control pair is invalid.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BezierError {
    /// A component was NaN or infinite.
    NotFinite {
        /// The offending component name.
        what: &'static str,
    },
    /// `x1` or `x2` was outside `[0, 1]`.
    XOutOfRange {
        /// The offending component name.
        what: &'static str,
        /// The rejected value.
        value: f64,
    },
    /// A `y` component was too large to quantise.
    TooLarge {
        /// The offending component name.
        what: &'static str,
        /// The rejected value.
        value: f64,
    },
}

impl std::fmt::Display for BezierError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFinite { what } => write!(f, "bezier(): {what} must be finite"),
            Self::XOutOfRange { what, value } => {
                write!(f, "bezier(): {what} must be in [0, 1], got {value}")
            }
            Self::TooLarge { what, value } => {
                write!(f, "bezier(): {what} is too large ({value})")
            }
        }
    }
}

impl std::error::Error for BezierError {}

/// Quantises a validated control value, canonicalising `-0.0`.
#[allow(clippy::cast_possible_truncation)] // range is checked before the call
fn quantise(value: f64) -> i32 {
    (value * BEZIER_SCALE).round() as i32
}

/// Restores a quantised control value.
fn dequantise(value: i32) -> f64 {
    f64::from(value) / BEZIER_SCALE
}

/// Easing applied to the segment that *arrives* at a keyframe.
///
/// For keys `k0` and `k1`, the interpolation from `k0` to `k1` uses
/// `k1.ease`. The ease of the first keyframe is therefore unused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Ease {
    /// Constant speed.
    #[default]
    Linear,
    /// Quadratic acceleration from rest.
    QuadIn,
    /// Quadratic deceleration to rest.
    QuadOut,
    /// Quadratic acceleration then deceleration.
    QuadInOut,
    /// Cubic acceleration from rest.
    CubicIn,
    /// Cubic deceleration to rest.
    CubicOut,
    /// Cubic acceleration then deceleration.
    CubicInOut,
    /// Pulls back before accelerating.
    BackIn,
    /// Overshoots the target, then settles.
    BackOut,
    /// Holds the previous value until the arriving key, then jumps.
    Hold,
    /// Elastic overshoot, settling with a damped sine.
    ElasticOut,
    /// Bounces against the target three times.
    BounceOut,
    /// Exponential acceleration and deceleration.
    ExpoInOut,
    /// Sinusoidal acceleration and deceleration.
    SineInOut,
    /// A CSS `cubic-bezier(x1, y1, x2, y2)` curve.
    Bezier(BezierEase),
}

impl Ease {
    /// Every named ease with its MDF name, in declaration order.
    ///
    /// A [`Ease::Bezier`] is not a preset: it carries its own four numbers and
    /// is written as `bezier(...)`.
    pub const ALL: [(Self, &'static str); 14] = [
        (Self::Linear, "linear"),
        (Self::QuadIn, "quad_in"),
        (Self::QuadOut, "quad_out"),
        (Self::QuadInOut, "quad_in_out"),
        (Self::CubicIn, "cubic_in"),
        (Self::CubicOut, "cubic_out"),
        (Self::CubicInOut, "cubic_in_out"),
        (Self::BackIn, "back_in"),
        (Self::BackOut, "back_out"),
        (Self::Hold, "hold"),
        (Self::ElasticOut, "elastic_out"),
        (Self::BounceOut, "bounce_out"),
        (Self::ExpoInOut, "expo_in_out"),
        (Self::SineInOut, "sine_in_out"),
    ];

    /// Looks up a named ease by its MDF name (for example `"quad_out"`).
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .find(|(_, candidate)| *candidate == name)
            .map(|(ease, _)| *ease)
    }

    /// Builds a cubic-bezier ease.
    ///
    /// # Errors
    ///
    /// The same validation as [`BezierEase::new`].
    pub fn bezier(x1: f64, y1: f64, x2: f64, y2: f64) -> Result<Self, BezierError> {
        BezierEase::new(x1, y1, x2, y2).map(Self::Bezier)
    }

    /// The MDF name of a named ease; `"bezier"` for a curve.
    #[must_use]
    pub fn name(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(ease, _)| *ease == self)
            .map_or("bezier", |(_, name)| name)
    }

    /// The MDF value form: a quoted-name payload for presets and
    /// `bezier(...)` for a curve.
    #[must_use]
    pub fn mdf_value(self) -> String {
        match self {
            Self::Bezier(curve) => curve.to_mdf(),
            other => other.name().to_owned(),
        }
    }

    /// The MDF expression form: `"quad_out"` (quoted) or `bezier(...)`.
    #[must_use]
    pub fn mdf_literal(self) -> String {
        match self {
            Self::Bezier(curve) => curve.to_mdf(),
            other => crate::overrides::quote(other.name()),
        }
    }

    /// Whether this is a cubic-bezier curve.
    #[must_use]
    pub const fn is_bezier(self) -> bool {
        matches!(self, Self::Bezier(_))
    }
}

/// A value at a frame, with the ease of the segment arriving at it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Keyframe<T> {
    /// Composition frame of the key.
    pub frame: Frame,
    /// Value at that frame.
    pub value: T,
    /// Ease of the segment from the previous key to this one.
    pub ease: Ease,
}

/// A property value that is either constant or keyframed.
#[derive(Debug, Clone, PartialEq)]
pub struct Animatable<T> {
    value: T,
    keys: Vec<Keyframe<T>>,
}

impl<T: Copy> Animatable<T> {
    /// Creates a constant value with no keyframes.
    #[must_use]
    pub const fn new(value: T) -> Self {
        Self {
            value,
            keys: Vec::new(),
        }
    }

    /// The static value, used when there are no keyframes.
    #[must_use]
    pub const fn value(&self) -> T {
        self.value
    }

    /// Keyframes sorted by strictly increasing frame.
    #[must_use]
    pub fn keys(&self) -> &[Keyframe<T>] {
        &self.keys
    }

    /// Replaces the static value. Keyframes, if any, still take priority.
    pub fn set_value(&mut self, value: T) {
        self.value = value;
    }

    /// Inserts a keyframe, keeping keys sorted by frame.
    ///
    /// A key at an existing frame replaces the old key, matching the
    /// behavior of setting a keyframe twice in a timeline editor.
    pub fn insert_key(&mut self, key: Keyframe<T>) {
        match self.keys.binary_search_by_key(&key.frame, |k| k.frame) {
            Ok(index) => self.keys[index] = key,
            Err(index) => self.keys.insert(index, key),
        }
    }

    /// Removes the keyframe at `frame`, if there is one.
    ///
    /// Returns whether a key was removed. The static value is untouched; if
    /// the last key is removed the property falls back to it.
    pub fn remove_key(&mut self, frame: Frame) -> bool {
        match self.keys.binary_search_by_key(&frame, |k| k.frame) {
            Ok(index) => {
                self.keys.remove(index);
                true
            }
            Err(_) => false,
        }
    }
}

/// One axis of a two-component property.
///
/// Keyframes are per axis: keying X at one frame and Y at another keeps the
/// two curves independent, exactly like an After Effects position property
/// with separate dimension keyframes. A 3D transform extends this list with Z.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Axis {
    /// Horizontal component.
    X,
    /// Vertical component.
    Y,
}

impl Axis {
    /// Every axis with its MDF suffix.
    pub const ALL: [(Self, &'static str); 2] = [(Self::X, "x"), (Self::Y, "y")];

    /// Looks up an axis by its suffix (`"x"` or `"y"`).
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .find(|(_, candidate)| *candidate == name)
            .map(|(axis, _)| *axis)
    }

    /// The MDF suffix of this axis.
    #[must_use]
    pub fn name(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(axis, _)| *axis == self)
            .map_or("?", |(_, name)| name)
    }
}

/// A two-component property whose axes carry independent keyframes.
///
/// The static value is the pair used when an axis has no keyframes. Sampling
/// is `(sample(x), sample(y))`; see `motioner_eval::sample_vec2`.
#[derive(Debug, Clone, PartialEq)]
pub struct AnimatableVec2 {
    x: Animatable<f64>,
    y: Animatable<f64>,
}

impl AnimatableVec2 {
    /// Creates a constant value with no keyframes.
    #[must_use]
    pub const fn new(value: crate::math::Vec2) -> Self {
        Self {
            x: Animatable::new(value.x),
            y: Animatable::new(value.y),
        }
    }

    /// The static pair, used for axes without keyframes.
    #[must_use]
    pub const fn value(&self) -> crate::math::Vec2 {
        crate::math::Vec2::new(self.x.value(), self.y.value())
    }

    /// Replaces one static component. Keyframes, if any, still take
    /// priority.
    pub fn set_axis_value(&mut self, axis: Axis, value: f64) {
        self.axis_mut(axis).set_value(value);
    }

    /// Replaces both static components. Keyframes, if any, still take
    /// priority.
    pub fn set_value(&mut self, value: crate::math::Vec2) {
        self.x.set_value(value.x);
        self.y.set_value(value.y);
    }

    /// The channel of one axis.
    #[must_use]
    pub const fn axis(&self, axis: Axis) -> &Animatable<f64> {
        match axis {
            Axis::X => &self.x,
            Axis::Y => &self.y,
        }
    }

    /// The channel of one axis, mutably.
    pub fn axis_mut(&mut self, axis: Axis) -> &mut Animatable<f64> {
        match axis {
            Axis::X => &mut self.x,
            Axis::Y => &mut self.y,
        }
    }

    /// Inserts a keyframe on both axes (what `key("pos", ...)` does).
    pub fn insert_key(&mut self, frame: Frame, value: crate::math::Vec2, ease: Ease) {
        self.x.insert_key(Keyframe {
            frame,
            value: value.x,
            ease,
        });
        self.y.insert_key(Keyframe {
            frame,
            value: value.y,
            ease,
        });
    }

    /// Inserts a keyframe on one axis only.
    pub fn insert_axis_key(&mut self, axis: Axis, frame: Frame, value: f64, ease: Ease) {
        self.axis_mut(axis)
            .insert_key(Keyframe { frame, value, ease });
    }

    /// Removes the keyframe at `frame` from both axes.
    pub fn remove_key(&mut self, frame: Frame) {
        self.x.remove_key(frame);
        self.y.remove_key(frame);
    }

    /// Removes the keyframe at `frame` from one axis.
    pub fn remove_axis_key(&mut self, axis: Axis, frame: Frame) -> bool {
        self.axis_mut(axis).remove_key(frame)
    }

    /// Every keyed frame of either axis, sorted and unique.
    #[must_use]
    pub fn key_frames(&self) -> Vec<Frame> {
        let mut frames: Vec<Frame> = self
            .x
            .keys()
            .iter()
            .map(|key| key.frame)
            .chain(self.y.keys().iter().map(|key| key.frame))
            .collect();
        frames.sort_unstable();
        frames.dedup();
        frames
    }

    /// Whether either axis has keyframes.
    #[must_use]
    pub fn is_animated(&self) -> bool {
        !self.x.keys().is_empty() || !self.y.keys().is_empty()
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn key(frame: Frame, value: f64) -> Keyframe<f64> {
        Keyframe {
            frame,
            value,
            ease: Ease::Linear,
        }
    }

    #[test]
    fn keys_stay_sorted_and_replace_same_frame() {
        let mut value = Animatable::new(0.0);
        value.insert_key(key(30, 1.0));
        value.insert_key(key(0, 0.0));
        value.insert_key(key(15, 0.5));
        value.insert_key(key(15, 0.25));
        let frames: Vec<_> = value.keys().iter().map(|k| (k.frame, k.value)).collect();
        assert_eq!(frames, vec![(0, 0.0), (15, 0.25), (30, 1.0)]);
    }

    #[test]
    fn ease_names_round_trip() {
        for (ease, name) in Ease::ALL {
            assert_eq!(Ease::from_name(name), Some(ease));
            assert_eq!(ease.name(), name);
        }
        assert_eq!(Ease::from_name("bounce"), None);
    }

    #[test]
    fn bezier_quantises_and_keeps_eq_hash_honest() {
        let a = Ease::bezier(0.25, 0.1, 0.25, 1.0).unwrap();
        let b = Ease::bezier(0.25, 0.1, 0.25, 1.0).unwrap();
        let c = Ease::bezier(0.25, 0.1, 0.25, 1.000_000_1).unwrap();
        assert_eq!(a, b);
        assert_eq!(a, c, "differences below 1e-6 quantise together");
        assert_eq!(a.mdf_value(), "bezier(0.25, 0.1, 0.25, 1)");
        assert_eq!(a.mdf_literal(), "bezier(0.25, 0.1, 0.25, 1)");
        assert!(a.is_bezier());
        // `-0.0` canonicalises to zero.
        let zero = Ease::bezier(0.0, -0.0, 1.0, 1.0).unwrap();
        let (x1, y1, _, _) = match zero {
            Ease::Bezier(curve) => curve.points(),
            _ => unreachable!(),
        };
        assert!(x1.abs() < 1e-12 && y1.abs() < 1e-12);
    }

    #[test]
    fn bezier_rejects_out_of_range_x_and_non_finite_values() {
        assert!(matches!(
            Ease::bezier(-0.1, 0.0, 0.5, 1.0),
            Err(BezierError::XOutOfRange { what: "x1", .. })
        ));
        assert!(matches!(
            Ease::bezier(0.5, 0.0, 1.1, 1.0),
            Err(BezierError::XOutOfRange { what: "x2", .. })
        ));
        assert!(matches!(
            Ease::bezier(0.5, f64::NAN, 0.5, 1.0),
            Err(BezierError::NotFinite { what: "y1" })
        ));
    }

    #[test]
    fn a_named_ease_literal_is_quoted() {
        assert_eq!(Ease::Linear.mdf_literal(), "\"linear\"");
        assert_eq!(Ease::Hold.mdf_value(), "hold");
    }
}

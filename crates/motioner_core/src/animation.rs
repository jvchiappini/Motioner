//! Animatable values and keyframes.
//!
//! An [`Animatable`] holds a static value plus an ordered keyframe list.
//! Once a property has keyframes they define its value at every frame;
//! sampling lives in `motioner_eval`.

use crate::time::Frame;

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
}

impl Ease {
    /// Every ease with its MDF name, in declaration order.
    pub const ALL: [(Self, &'static str); 9] = [
        (Self::Linear, "linear"),
        (Self::QuadIn, "quad_in"),
        (Self::QuadOut, "quad_out"),
        (Self::QuadInOut, "quad_in_out"),
        (Self::CubicIn, "cubic_in"),
        (Self::CubicOut, "cubic_out"),
        (Self::CubicInOut, "cubic_in_out"),
        (Self::BackIn, "back_in"),
        (Self::BackOut, "back_out"),
    ];

    /// Looks up an ease by its MDF name (for example `"quad_out"`).
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .find(|(_, candidate)| *candidate == name)
            .map(|(ease, _)| *ease)
    }

    /// The MDF name of this ease.
    #[must_use]
    pub fn name(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(ease, _)| *ease == self)
            .map_or("linear", |(_, name)| name)
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
}

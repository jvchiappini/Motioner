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

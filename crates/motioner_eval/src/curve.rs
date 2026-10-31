//! Easing functions and keyframe sampling.

use motioner_core::{Animatable, Ease, Frame, Vec2};

/// Overshoot constant of the standard "back" easing (Penner).
const BACK_OVERSHOOT: f64 = 1.701_58;

/// Maps linear progress `t` in `0..=1` through an easing curve.
///
/// All curves satisfy `ease(0) == 0` and `ease(1) == 1`. `BackIn` and
/// `BackOut` leave the unit range in between, by design.
#[must_use]
pub fn ease(curve: Ease, t: f64) -> f64 {
    match curve {
        Ease::Linear => t,
        Ease::QuadIn => t * t,
        Ease::QuadOut => 1.0 - (1.0 - t) * (1.0 - t),
        Ease::QuadInOut => in_out(t, |u| u * u),
        Ease::CubicIn => t * t * t,
        Ease::CubicOut => 1.0 - (1.0 - t).powi(3),
        Ease::CubicInOut => in_out(t, |u| u * u * u),
        Ease::BackIn => back_in(t),
        Ease::BackOut => 1.0 - back_in(1.0 - t),
    }
}

fn back_in(t: f64) -> f64 {
    t * t * ((BACK_OVERSHOOT + 1.0) * t - BACK_OVERSHOOT)
}

/// Builds a symmetric in-out curve from an "in" curve.
fn in_out(t: f64, ease_in: impl Fn(f64) -> f64) -> f64 {
    if t < 0.5 {
        ease_in(2.0 * t) / 2.0
    } else {
        1.0 - ease_in(2.0 * (1.0 - t)) / 2.0
    }
}

/// Values that can be linearly interpolated.
pub trait Interpolate: Copy {
    /// Returns `a + (b - a) * t`.
    fn interpolate(a: Self, b: Self, t: f64) -> Self;
}

impl Interpolate for f64 {
    fn interpolate(a: Self, b: Self, t: f64) -> Self {
        a + (b - a) * t
    }
}

impl Interpolate for Vec2 {
    fn interpolate(a: Self, b: Self, t: f64) -> Self {
        a + (b - a) * t
    }
}

/// Samples an animatable value at a frame.
///
/// Without keyframes the static value is returned. Before the first key
/// and after the last key the value holds. Between two keys the segment
/// is interpolated with the ease of the later key.
#[must_use]
pub fn sample<T: Interpolate>(value: &Animatable<T>, frame: Frame) -> T {
    let keys = value.keys();
    let (Some(first), Some(last)) = (keys.first(), keys.last()) else {
        return value.value();
    };
    if frame <= first.frame {
        return first.value;
    }
    if frame >= last.frame {
        return last.value;
    }
    // `next` is the first key after `frame`; it exists and is not the
    // first key because of the range checks above.
    let next = keys.partition_point(|k| k.frame <= frame);
    let (a, b) = (&keys[next - 1], &keys[next]);
    // Keyframes are validated against `MAX_ABS_FRAME`, so the span
    // `b.frame - a.frame` is at most 2^32, far below 2^53: the conversion
    // to f64 is exact.
    #[allow(clippy::cast_precision_loss)]
    let t = (frame - a.frame) as f64 / (b.frame - a.frame) as f64;
    T::interpolate(a.value, b.value, ease(b.ease, t))
}

#[cfg(test)]
mod tests {
    use super::*;
    use motioner_core::Keyframe;

    fn animated(keys: &[(Frame, f64, Ease)]) -> Animatable<f64> {
        let mut value = Animatable::new(-1.0);
        for &(frame, v, ease) in keys {
            value.insert_key(Keyframe {
                frame,
                value: v,
                ease,
            });
        }
        value
    }

    #[test]
    fn every_ease_hits_its_endpoints() {
        for (curve, name) in Ease::ALL {
            assert!(ease(curve, 0.0).abs() < 1e-12, "{name} at 0");
            assert!((ease(curve, 1.0) - 1.0).abs() < 1e-12, "{name} at 1");
        }
    }

    #[test]
    fn back_out_overshoots() {
        let peak = (1..100)
            .map(|i| ease(Ease::BackOut, f64::from(i) / 100.0))
            .fold(0.0, f64::max);
        assert!(peak > 1.05);
    }

    #[test]
    #[allow(clippy::float_cmp)] // exact comparison is intended: these values are exactly representable
    fn static_value_without_keys() {
        assert_eq!(sample(&Animatable::new(3.0), 10), 3.0);
    }

    #[test]
    #[allow(clippy::float_cmp)] // exact comparison is intended: these values are exactly representable
    fn holds_outside_and_interpolates_inside() {
        let value = animated(&[(10, 0.0, Ease::Linear), (20, 1.0, Ease::Linear)]);
        assert_eq!(sample(&value, 0), 0.0);
        assert_eq!(sample(&value, 15), 0.5);
        assert_eq!(sample(&value, 25), 1.0);
    }

    #[test]
    fn uses_the_ease_of_the_arriving_key() {
        let value = animated(&[(0, 0.0, Ease::Linear), (10, 1.0, Ease::QuadOut)]);
        assert!((sample(&value, 5) - 0.75).abs() < 1e-12);
    }
}

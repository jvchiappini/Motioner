//! Easing functions and keyframe sampling.

use motioner_core::{Animatable, Color, Ease, Frame, Vec2};

/// Overshoot constant of the standard "back" easing (Penner).
const BACK_OVERSHOOT: f64 = 1.701_58;
/// Period of the elastic curve (Penner).
const ELASTIC_PERIOD: f64 = 0.3;

/// Maps linear progress `t` in `0..=1` through an easing curve.
///
/// All curves satisfy `ease(0) == 0` and `ease(1) == 1`. `BackIn`,
/// `BackOut`, `ElasticOut` and `BounceOut` leave the unit range in between, by
/// design. `Hold` is a step.
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
        Ease::Hold => {
            if t < 1.0 {
                0.0
            } else {
                1.0
            }
        }
        Ease::ElasticOut => elastic_out(t),
        Ease::BounceOut => bounce_out(t),
        Ease::ExpoInOut => expo_in_out(t),
        Ease::SineInOut => (std::f64::consts::PI * t).cos().mul_add(-0.5, 0.5),
        Ease::Bezier(curve) => bezier(curve, t),
    }
}

/// Penner `easeOutElastic`: a damped sine that settles on the target.
fn elastic_out(t: f64) -> f64 {
    if t <= 0.0 {
        return 0.0;
    }
    if t >= 1.0 {
        return 1.0;
    }
    let s = ELASTIC_PERIOD / 4.0;
    (-10.0 * t).exp2() * (std::f64::consts::TAU * (t - s) / ELASTIC_PERIOD).sin() + 1.0
}

/// Penner `easeOutBounce`.
fn bounce_out(t: f64) -> f64 {
    const N: f64 = 7.562_5;
    const D: f64 = 2.75;
    if t < 1.0 / D {
        N * t * t
    } else if t < 2.0 / D {
        let t = t - 1.5 / D;
        N * t * t + 0.75
    } else if t < 2.5 / D {
        let t = t - 2.25 / D;
        N * t * t + 0.937_5
    } else {
        let t = t - 2.625 / D;
        N * t * t + 0.984_375
    }
}

/// Penner `easeInOutExpo`.
fn expo_in_out(t: f64) -> f64 {
    if t <= 0.0 {
        return 0.0;
    }
    if t >= 1.0 {
        return 1.0;
    }
    if t < 0.5 {
        0.5 * (20.0 * t - 10.0).exp2()
    } else {
        1.0 - 0.5 * (-20.0 * t + 10.0).exp2()
    }
}

/// Solves a CSS cubic-bezier curve at `x` and returns `y`.
///
/// The solver is deterministic: up to eight Newton–Raphson steps from
/// `t = x`, then a fixed-count bisection fallback. There is no time- or
/// platform-dependent branch. The result satisfies `|x(t) - x| < 1e-7` on the
/// supported range.
fn bezier(curve: motioner_core::BezierEase, x: f64) -> f64 {
    let (x1, y1, x2, y2) = curve.points();
    let cx = 3.0 * x1;
    let bx = 3.0 * (x2 - x1) - cx;
    let ax = 1.0 - cx - bx;
    let cy = 3.0 * y1;
    let by = 3.0 * (y2 - y1) - cy;
    let ay = 1.0 - cy - by;
    let sample_x = |t: f64| ((ax * t + bx) * t + cx) * t;
    let sample_y = |t: f64| ((ay * t + by) * t + cy) * t;
    let derivative_x = |t: f64| (3.0 * ax * t + 2.0 * bx) * t + cx;

    let mut t = x;
    for _ in 0..8 {
        let error = sample_x(t) - x;
        if error.abs() < 1e-9 {
            return sample_y(t);
        }
        let derivative = derivative_x(t);
        if derivative.abs() < 1e-9 {
            break;
        }
        t -= error / derivative;
        if !(0.0..=1.0).contains(&t) {
            break;
        }
    }

    let (mut low, mut high) = (0.0, 1.0);
    for _ in 0..64 {
        t = 0.5 * (low + high);
        let value = sample_x(t);
        if (value - x).abs() < 1e-9 {
            return sample_y(t);
        }
        if value < x {
            low = t;
        } else {
            high = t;
        }
    }
    sample_y(t)
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

impl Interpolate for Color {
    /// Interpolates in linear-light sRGB (D-0028).
    fn interpolate(a: Self, b: Self, t: f64) -> Self {
        Color::lerp(a, b, t)
    }
}

impl Interpolate for Option<Color> {
    /// Interpolates when both endpoints are colours; a `none` endpoint holds
    /// the previous value as a step (D-0028).
    fn interpolate(a: Self, b: Self, t: f64) -> Self {
        match (a, b) {
            (Some(a), Some(b)) => Some(Color::lerp(a, b, t)),
            _ => a,
        }
    }
}

/// Samples an animatable value at a frame.
///
/// Without keyframes the static value is returned. Before the first key
/// and after the last key the value holds. Between two keys the segment
/// is interpolated with the ease of the later key.
#[must_use]
pub fn sample_vec2(value: &motioner_core::AnimatableVec2, frame: Frame) -> motioner_core::Vec2 {
    sample_vec2_at(value, frame_time(frame))
}

/// Samples an animatable value at a fractional frame time.
///
/// This is the motion-blur entry point: `time` may be between integer frames.
/// The integer [`sample`] path calls it with an exact `f64` conversion, so it
/// produces the same value as before.
#[must_use]
pub fn sample_vec2_at(value: &motioner_core::AnimatableVec2, time: f64) -> motioner_core::Vec2 {
    motioner_core::Vec2::new(
        sample_at(value.axis(motioner_core::Axis::X), time),
        sample_at(value.axis(motioner_core::Axis::Y), time),
    )
}

/// Samples an animatable value at a frame.
///
/// Without keyframes the static value is returned. Before the first key and
/// after the last key the value holds. Between two keys the segment is
/// interpolated with the ease of the later key.
#[must_use]
pub fn sample<T: Interpolate>(value: &Animatable<T>, frame: Frame) -> T {
    sample_at(value, frame_time(frame))
}

/// Samples an animatable value at a fractional frame time.
///
/// `time` is in composition frames and may be fractional. The result is
/// continuous for every ease except `Hold`, which steps at the arriving key.
#[must_use]
pub fn sample_at<T: Interpolate>(value: &Animatable<T>, time: f64) -> T {
    let keys = value.keys();
    let (Some(first), Some(last)) = (keys.first(), keys.last()) else {
        return value.value();
    };
    #[allow(clippy::cast_precision_loss)] // frames are bounded by 2^31
    let first_time = first.frame as f64;
    #[allow(clippy::cast_precision_loss)]
    let last_time = last.frame as f64;
    if time <= first_time {
        return first.value;
    }
    if time >= last_time {
        return last.value;
    }
    // `next` is the first key after `time`; it exists and is not the first
    // key because of the range checks above.
    let next = keys.partition_point(|k| frame_time(k.frame) <= time);
    let (a, b) = (&keys[next - 1], &keys[next]);
    #[allow(clippy::cast_precision_loss)] // bounded by 2^31
    let span = (b.frame - a.frame) as f64;
    let t = (time - frame_time(a.frame)) / span;
    T::interpolate(a.value, b.value, ease(b.ease, t))
}

/// Converts an integer frame to the fractional time domain exactly.
fn frame_time(frame: Frame) -> f64 {
    #[allow(clippy::cast_precision_loss)] // frames are bounded by 2^31
    {
        frame as f64
    }
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

    #[test]
    fn colours_interpolate_in_linear_light() {
        let red = motioner_core::Color::rgb(255, 0, 0);
        let green = motioner_core::Color::rgb(0, 255, 0);
        let mid = Color::interpolate(red, green, 0.5);
        // Linear-light midpoints are much brighter than the 8-bit average
        // (128, 128, 0); 0.5 linear is sRGB 188.
        assert_eq!((mid.r, mid.g, mid.b, mid.a), (188, 188, 0, 255));
        assert_eq!(Color::interpolate(red, green, 0.0), red);
        assert_eq!(Color::interpolate(red, green, 1.0), green);
        // Alpha stays linear coverage.
        let half = motioner_core::Color::parse_hex("#FF000080").unwrap();
        let clear = motioner_core::Color::parse_hex("#00000000").unwrap();
        assert_eq!(Color::interpolate(half, clear, 0.5).a, 64);
    }

    #[test]
    fn bezier_solver_matches_reference_values() {
        let css_ease = Ease::bezier(0.25, 0.1, 0.25, 1.0).unwrap();
        let ease_in_out = Ease::bezier(0.42, 0.0, 0.58, 1.0).unwrap();
        let overshoot = Ease::bezier(0.34, 1.56, 0.64, 1.0).unwrap();
        assert!((ease(css_ease, 0.5) - 0.802_403_387_584_857).abs() < 1e-9);
        assert!((ease(ease_in_out, 0.25) - 0.129_161_931_047_319_8).abs() < 1e-9);
        assert!((ease(ease_in_out, 0.5) - 0.5).abs() < 1e-9);
        assert!((ease(ease_in_out, 0.75) - 0.870_838_068_952_68).abs() < 1e-9);
        assert!((ease(overshoot, 0.5) - 1.087_400_670_218_697_8).abs() < 1e-9);
    }

    #[test]
    fn bezier_solver_hits_the_domain_everywhere() {
        let curve = Ease::bezier(0.17, 0.67, 0.83, 0.67).unwrap();
        for step in 0..=1000 {
            let t = f64::from(step) / 1000.0;
            let y = ease(curve, t);
            assert!(y.is_finite(), "t={t}");
        }
        assert!(ease(curve, 0.0).abs() < 1e-12);
        assert!((ease(curve, 1.0) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn penner_eases_have_their_documented_midpoints() {
        assert!((ease(Ease::ElasticOut, 0.5) - 1.015_625).abs() < 1e-9);
        assert!((ease(Ease::BounceOut, 0.5) - 0.765_625).abs() < 1e-9);
        assert!((ease(Ease::ExpoInOut, 0.5) - 0.5).abs() < 1e-9);
        assert!((ease(Ease::SineInOut, 0.5) - 0.5).abs() < 1e-9);
        assert!(ease(Ease::Hold, 0.99).abs() < 1e-12);
        assert!((ease(Ease::Hold, 1.0) - 1.0).abs() < 1e-12);
    }

    #[test]
    #[allow(clippy::float_cmp)] // exact equality is the point
    fn fractional_sampling_matches_integer_sampling() {
        let value = animated(&[(0, 0.0, Ease::Linear), (10, 1.0, Ease::Linear)]);
        for frame in 0..=10 {
            #[allow(clippy::cast_precision_loss)]
            let time = frame as f64;
            assert_eq!(sample(&value, frame), sample_at(&value, time));
        }
        assert!((sample_at(&value, 5.5) - 0.55).abs() < 1e-12);
    }

    #[test]
    fn optional_colours_hold_only_when_one_endpoint_is_absent() {
        let mut value = Animatable::new(None);
        value.insert_key(Keyframe {
            frame: 0,
            value: None,
            ease: Ease::Linear,
        });
        value.insert_key(Keyframe {
            frame: 10,
            value: Some(motioner_core::Color::WHITE),
            ease: Ease::Linear,
        });
        value.insert_key(Keyframe {
            frame: 20,
            value: None,
            ease: Ease::Linear,
        });
        assert_eq!(sample(&value, 5), None);
        assert_eq!(sample(&value, 10), Some(motioner_core::Color::WHITE));
        assert_eq!(sample(&value, 15), Some(motioner_core::Color::WHITE));
        assert_eq!(sample(&value, 20), None);

        // Two present colours interpolate like a required colour.
        let mut both = Animatable::new(None);
        both.insert_key(Keyframe {
            frame: 0,
            value: Some(motioner_core::Color::rgb(255, 0, 0)),
            ease: Ease::Linear,
        });
        both.insert_key(Keyframe {
            frame: 10,
            value: Some(motioner_core::Color::rgb(0, 255, 0)),
            ease: Ease::Linear,
        });
        assert_eq!(
            sample(&both, 5),
            Some(motioner_core::Color::rgb(188, 188, 0))
        );
    }
}

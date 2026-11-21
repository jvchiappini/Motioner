//! Rational time base: integer frames plus a rational frame rate.

use crate::error::CoreError;

/// A frame index in composition time. Frame `0` is the first frame.
///
/// Time is always an integer frame count; seconds only appear at the
/// authoring boundary and are converted once with [`FrameRate`].
pub type Frame = i64;

/// Largest absolute frame index accepted anywhere in the document.
///
/// The limit is `2^31`. Two frames inside this range differ by at most
/// `2^32`, which is far below `2^53`; every such difference converts to
/// `f64` without losing precision, so keyframe interpolation is exact.
/// The limit is also many orders of magnitude below `i64::MAX`, so frame
/// subtraction and addition cannot overflow.
pub const MAX_ABS_FRAME: Frame = 1 << 31;

/// Returns whether `frame` lies within `[-MAX_ABS_FRAME, MAX_ABS_FRAME]`.
///
/// This is the single frame boundary check shared by
/// [`FrameRate::seconds_to_frame`] and `Document::add_keyframe`, so both
/// paths enforce exactly the same limit.
pub(crate) const fn frame_within_bounds(frame: Frame) -> bool {
    frame >= -MAX_ABS_FRAME && frame <= MAX_ABS_FRAME
}

/// A rational frame rate `numerator / denominator` frames per second.
///
/// Rational rates keep NTSC timing (for example `30000/1001`) exact; no
/// floating-point accumulation is involved in frame arithmetic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FrameRate {
    numerator: u32,
    denominator: u32,
}

impl FrameRate {
    /// Creates a frame rate.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidFrameRate`] when either term is zero.
    pub fn new(numerator: u32, denominator: u32) -> Result<Self, CoreError> {
        if numerator == 0 || denominator == 0 {
            return Err(CoreError::InvalidFrameRate {
                numerator,
                denominator,
            });
        }
        Ok(Self {
            numerator,
            denominator,
        })
    }

    /// The numerator (frames).
    #[must_use]
    pub const fn numerator(self) -> u32 {
        self.numerator
    }

    /// The denominator (seconds).
    #[must_use]
    pub const fn denominator(self) -> u32 {
        self.denominator
    }

    /// Frames per second as a float, for display and authoring only.
    #[must_use]
    pub fn as_f64(self) -> f64 {
        f64::from(self.numerator) / f64::from(self.denominator)
    }

    /// Converts an authored time in seconds to the nearest frame.
    ///
    /// Ties round away from zero. This is the only place where seconds
    /// become frames, so keyframes and durations agree on rounding.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::TimeOutOfRange`] when `seconds` is NaN or
    /// infinite, or when the rounded frame would fall outside
    /// `[-MAX_ABS_FRAME, MAX_ABS_FRAME]`.
    pub fn seconds_to_frame(self, seconds: f64) -> Result<Frame, CoreError> {
        let frames = (seconds * f64::from(self.numerator) / f64::from(self.denominator)).round();
        if !frames.is_finite() {
            return Err(CoreError::TimeOutOfRange { seconds });
        }
        // `frames` is finite; the shared range check below proves it fits
        // in `i64` and rejects saturated extremes.
        #[allow(clippy::cast_possible_truncation)]
        let frame = frames as Frame;
        if frame_within_bounds(frame) {
            Ok(frame)
        } else {
            Err(CoreError::TimeOutOfRange { seconds })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fps(numerator: u32, denominator: u32) -> FrameRate {
        FrameRate::new(numerator, denominator).unwrap()
    }

    #[test]
    fn rejects_zero_terms() {
        assert!(FrameRate::new(0, 1).is_err());
        assert!(FrameRate::new(60, 0).is_err());
    }

    #[test]
    fn converts_seconds_to_nearest_frame() {
        let rate = fps(60, 1);
        assert_eq!(rate.seconds_to_frame(0.0).unwrap(), 0);
        assert_eq!(rate.seconds_to_frame(0.8).unwrap(), 48);
        assert_eq!(rate.seconds_to_frame(5.0).unwrap(), 300);
        let ntsc = fps(30000, 1001);
        assert_eq!(ntsc.seconds_to_frame(1.0).unwrap(), 30);
    }

    #[test]
    fn rounds_ties_away_from_zero() {
        let one = fps(1, 1);
        assert_eq!(one.seconds_to_frame(0.5).unwrap(), 1);
        assert_eq!(one.seconds_to_frame(-0.5).unwrap(), -1);
        assert_eq!(one.seconds_to_frame(1.5).unwrap(), 2);
        let two = fps(2, 1);
        assert_eq!(two.seconds_to_frame(0.25).unwrap(), 1);
        assert_eq!(two.seconds_to_frame(-0.25).unwrap(), -1);
        // NTSC 30000/1001: half a second is 14.985 frames, rounds to 15.
        let ntsc = fps(30000, 1001);
        assert_eq!(ntsc.seconds_to_frame(0.5).unwrap(), 15);
    }

    #[test]
    fn negative_times_round_to_negative_frames() {
        let rate = fps(60, 1);
        assert_eq!(rate.seconds_to_frame(-0.1).unwrap(), -6);
        assert_eq!(rate.seconds_to_frame(-1.0).unwrap(), -60);
    }

    #[test]
    fn rejects_non_finite_seconds() {
        let rate = fps(60, 1);
        assert!(matches!(
            rate.seconds_to_frame(f64::NAN),
            Err(CoreError::TimeOutOfRange { .. })
        ));
        assert!(matches!(
            rate.seconds_to_frame(f64::INFINITY),
            Err(CoreError::TimeOutOfRange { .. })
        ));
        assert!(matches!(
            rate.seconds_to_frame(f64::NEG_INFINITY),
            Err(CoreError::TimeOutOfRange { .. })
        ));
    }

    #[test]
    fn accepts_the_exact_frame_limits() {
        let rate = fps(1, 1);
        let max = 2_147_483_648.0_f64;
        assert_eq!(rate.seconds_to_frame(max).unwrap(), MAX_ABS_FRAME);
        assert_eq!(rate.seconds_to_frame(-max).unwrap(), -MAX_ABS_FRAME);
    }

    #[test]
    fn accepts_frames_just_inside_the_limits() {
        let rate = fps(1, 1);
        let inside = 2_147_483_647.0_f64;
        assert_eq!(rate.seconds_to_frame(inside).unwrap(), MAX_ABS_FRAME - 1);
        assert_eq!(
            rate.seconds_to_frame(-inside).unwrap(),
            -(MAX_ABS_FRAME - 1)
        );
    }

    #[test]
    fn rejects_frames_just_outside_the_limits() {
        let rate = fps(1, 1);
        let outside = 2_147_483_649.0_f64;
        assert!(matches!(
            rate.seconds_to_frame(outside),
            Err(CoreError::TimeOutOfRange { .. })
        ));
        assert!(matches!(
            rate.seconds_to_frame(-outside),
            Err(CoreError::TimeOutOfRange { .. })
        ));
        // Huge finite values saturate the cast and are still rejected.
        assert!(matches!(
            rate.seconds_to_frame(1.0e300),
            Err(CoreError::TimeOutOfRange { .. })
        ));
        assert!(matches!(
            rate.seconds_to_frame(-1.0e300),
            Err(CoreError::TimeOutOfRange { .. })
        ));
    }
}

//! Rational time base: integer frames plus a rational frame rate.

use crate::error::CoreError;

/// A frame index in composition time. Frame `0` is the first frame.
///
/// Time is always an integer frame count; seconds only appear at the
/// authoring boundary and are converted once with [`FrameRate`].
pub type Frame = i64;

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
    /// Ties round away from zero and out-of-range values saturate. This is
    /// the only place where seconds become frames, so keyframes and
    /// durations agree on rounding.
    #[must_use]
    pub fn seconds_to_frame(self, seconds: f64) -> Frame {
        let frames = (seconds * f64::from(self.numerator) / f64::from(self.denominator)).round();
        // Saturating float-to-int conversion is the documented behavior.
        #[allow(clippy::cast_possible_truncation)]
        let frame = frames as Frame;
        frame
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_zero_terms() {
        assert!(FrameRate::new(0, 1).is_err());
        assert!(FrameRate::new(60, 0).is_err());
    }

    #[test]
    fn converts_seconds_to_nearest_frame() {
        let fps = FrameRate::new(60, 1).unwrap();
        assert_eq!(fps.seconds_to_frame(0.0), 0);
        assert_eq!(fps.seconds_to_frame(0.8), 48);
        assert_eq!(fps.seconds_to_frame(5.0), 300);
        let ntsc = FrameRate::new(30000, 1001).unwrap();
        assert_eq!(ntsc.seconds_to_frame(1.0), 30);
    }
}

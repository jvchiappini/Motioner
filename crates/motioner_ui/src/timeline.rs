//! Timeline scrubber geometry and label spacing.
//!
//! Frame `0` maps to the left edge of the scrubber rect and frame
//! `duration - 1` to its right edge. Conversions are clamped and round to the
//! nearest frame; a zero-width rect is treated as frame `0` and never divides
//! by zero.

use motioner_core::FrameRate;

/// Minimum horizontal distance between two labels, in pixels.
pub const MIN_TICK_SPACING_PX: f64 = 60.0;

/// Which unit the label spacing uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TickUnit {
    /// Label every `step_frames` frames.
    Frames,
    /// Label at whole-second-ish intervals (`step_frames` apart).
    Seconds,
}

/// The label step chosen for a timeline width.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TickSpacing {
    /// Number of frames between labels.
    pub step_frames: i64,
    /// Whether the labels are frames or seconds.
    pub unit: TickUnit,
}

/// Maps a frame index to an x coordinate inside `[rect_x, rect_x + rect_w]`.
///
/// `frame` is clamped into `0..duration`; a non-positive duration or width
/// returns `rect_x`.
#[must_use]
pub fn frame_to_x(frame: i64, duration: i64, rect_x: f64, rect_w: f64) -> f64 {
    if duration <= 1 || !rect_w.is_finite() || rect_w <= 0.0 {
        return rect_x;
    }
    let last = (duration - 1) as f64;
    let frame = frame.clamp(0, duration - 1) as f64;
    rect_x + frame / last * rect_w
}

/// Maps an x coordinate to the nearest frame inside `0..duration`.
///
/// The input is clamped to the scrubber rect; a non-positive width or duration
/// returns frame `0`.
#[must_use]
#[allow(clippy::cast_possible_truncation)] // the result is clamped to the frame range
pub fn x_to_frame(x: f64, duration: i64, rect_x: f64, rect_w: f64) -> i64 {
    if duration <= 1 || !rect_w.is_finite() || rect_w <= 0.0 {
        return 0;
    }
    let ratio = ((x - rect_x) / rect_w).clamp(0.0, 1.0);
    let frame = (ratio * (duration - 1) as f64).round();
    if !frame.is_finite() {
        return 0;
    }
    (frame as i64).clamp(0, duration - 1)
}

/// Chooses a label step that keeps at least `min_label_px` between labels.
///
/// Whole-second steps are preferred when they fit; otherwise the smallest
/// frame step from a fixed human-friendly ladder is used.
#[must_use]
pub fn tick_spacing(duration: i64, rate: FrameRate, rect_w: f64, min_label_px: f64) -> TickSpacing {
    let duration = duration.max(1);
    let last = (duration - 1).max(1) as f64;
    let pixels_per_frame = rect_w / last;
    if !pixels_per_frame.is_finite() || pixels_per_frame <= 0.0 {
        return TickSpacing {
            step_frames: 1,
            unit: TickUnit::Frames,
        };
    }
    let min_px = min_label_px.max(1.0);

    let numerator = i64::from(rate.numerator());
    let denominator = i64::from(rate.denominator());
    for seconds in [1_i64, 2, 5, 10, 15, 30, 60, 120, 300, 600] {
        let step = ((seconds * numerator + denominator / 2) / denominator).max(1);
        if step as f64 * pixels_per_frame >= min_px {
            return TickSpacing {
                step_frames: step,
                unit: TickUnit::Seconds,
            };
        }
    }

    for step in [1_i64, 2, 5, 10, 20, 30, 60, 120, 300, 600, 1200, 3000, 6000] {
        if step as f64 * pixels_per_frame >= min_px {
            return TickSpacing {
                step_frames: step,
                unit: TickUnit::Frames,
            };
        }
    }

    TickSpacing {
        step_frames: duration - 1,
        unit: TickUnit::Frames,
    }
}

/// Frame indices to label for `spacing`, starting at frame `0`.
///
/// The returned list always includes frame `0` when the composition has at
/// least one frame.
#[must_use]
pub fn tick_frames(duration: i64, spacing: TickSpacing) -> Vec<i64> {
    if duration <= 0 {
        return Vec::new();
    }
    let step = spacing.step_frames.max(1);
    let mut ticks = Vec::new();
    let mut frame = 0;
    while frame < duration {
        ticks.push(frame);
        frame = frame.saturating_add(step);
    }
    ticks
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rate_60() -> FrameRate {
        FrameRate::new(60, 1).unwrap()
    }

    #[test]
    fn endpoints_map_to_the_rect_edges() {
        assert!(close(frame_to_x(0, 300, 10.0, 500.0), 10.0));
        assert!(close(frame_to_x(299, 300, 10.0, 500.0), 510.0));
        assert!(close(frame_to_x(-5, 300, 10.0, 500.0), 10.0));
        assert!(close(frame_to_x(1000, 300, 10.0, 500.0), 510.0));
    }

    #[test]
    fn x_rounds_to_the_nearest_frame_and_clamps() {
        assert_eq!(x_to_frame(10.0, 300, 10.0, 500.0), 0);
        assert_eq!(x_to_frame(510.0, 300, 10.0, 500.0), 299);
        assert_eq!(x_to_frame(-100.0, 300, 10.0, 500.0), 0);
        assert_eq!(x_to_frame(10_000.0, 300, 10.0, 500.0), 299);
        // Half-way round-trips to the nearer frame.
        let x = frame_to_x(150, 300, 0.0, 299.0);
        assert_eq!(x_to_frame(x, 300, 0.0, 299.0), 150);
    }

    #[test]
    fn degenerate_rects_are_safe() {
        assert!(close(frame_to_x(5, 10, 3.0, 0.0), 3.0));
        assert_eq!(x_to_frame(5.0, 10, 3.0, 0.0), 0);
        assert!(close(frame_to_x(0, 1, 7.0, 100.0), 7.0));
        assert_eq!(x_to_frame(7.0, 0, 7.0, 100.0), 0);
    }

    #[test]
    fn wide_timelines_use_second_labels() {
        let spacing = tick_spacing(300, rate_60(), 1200.0, MIN_TICK_SPACING_PX);
        assert_eq!(spacing.unit, TickUnit::Seconds);
        assert_eq!(spacing.step_frames, 60);
        let pixels = spacing.step_frames as f64 * 1200.0 / 299.0;
        assert!(pixels >= MIN_TICK_SPACING_PX);
    }

    #[test]
    fn narrow_timelines_keep_labels_apart() {
        for width in [80.0, 200.0, 400.0] {
            let spacing = tick_spacing(300, rate_60(), width, MIN_TICK_SPACING_PX);
            let pixels = spacing.step_frames as f64 * width / 299.0;
            assert!(
                pixels >= MIN_TICK_SPACING_PX || spacing.step_frames == 299,
                "width {width}: {pixels} px between labels"
            );
        }
    }

    #[test]
    fn ticks_start_at_zero_and_stay_inside_the_composition() {
        let ticks = tick_frames(
            300,
            TickSpacing {
                step_frames: 60,
                unit: TickUnit::Seconds,
            },
        );
        assert_eq!(ticks, vec![0, 60, 120, 180, 240]);
    }

    fn close(actual: f64, expected: f64) -> bool {
        (actual - expected).abs() < 1e-9
    }
}

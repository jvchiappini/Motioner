//! Playback transport and exact rational frame timing.
//!
//! The transport is the only place where wall-clock time enters Motioner. It
//! converts elapsed time into an integer frame index with integer arithmetic:
//! `elapsed_ns * fps_num / (fps_den * 1e9)`. No floating-point accumulator is
//! involved, so NTSC rates such as `24000/1001` do not drift over long
//! playback.

use std::time::Duration;

use motioner_core::FrameRate;

/// What the transport is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// Not advancing; the current frame is fixed.
    Stopped,
    /// Advancing from an anchor frame and an anchor wall-clock instant.
    Playing {
        /// Frame displayed when playback started.
        anchor_frame: i64,
        /// Wall-clock instant playback started.
        anchor_time: Duration,
    },
    /// The user is dragging the playhead; the frame follows the pointer.
    Scrubbing,
}

/// Current frame, playback state, frame rate and duration.
#[derive(Debug, Clone)]
pub struct Transport {
    state: State,
    frame: i64,
    rate: FrameRate,
    duration: i64,
    looping: bool,
}

impl Transport {
    /// Creates a stopped transport at frame `0`.
    #[must_use]
    pub fn new(rate: FrameRate, duration: i64) -> Self {
        Self {
            state: State::Stopped,
            frame: 0,
            rate,
            duration: duration.max(1),
            looping: true,
        }
    }

    /// The frame rate used for playback.
    #[must_use]
    pub fn frame_rate(&self) -> FrameRate {
        self.rate
    }

    /// Duration in frames; valid frames are `0..duration`.
    #[must_use]
    pub fn duration(&self) -> i64 {
        self.duration
    }

    /// The frame currently selected (or shown while paused).
    #[must_use]
    pub fn frame(&self) -> i64 {
        self.frame
    }

    /// Whether the transport is playing.
    #[must_use]
    pub fn is_playing(&self) -> bool {
        matches!(self.state, State::Playing { .. })
    }

    /// Whether the transport is being scrubbed.
    #[must_use]
    pub fn is_scrubbing(&self) -> bool {
        matches!(self.state, State::Scrubbing)
    }

    /// Whether playback loops at the end.
    #[must_use]
    pub fn looping(&self) -> bool {
        self.looping
    }

    /// Enables or disables looping.
    pub fn set_looping(&mut self, looping: bool) {
        self.looping = looping;
    }

    /// Toggles looping.
    pub fn toggle_looping(&mut self) {
        self.looping = !self.looping;
    }

    /// Replaces the frame rate and duration, for example after a reload.
    ///
    /// The current frame is clamped into the new range. While playing,
    /// playback re-anchors at `now` on the frame currently shown, so a new
    /// rate changes the speed from here on instead of jumping to the frame the
    /// old anchor would map to.
    pub fn retime(&mut self, rate: FrameRate, duration: i64, now: Duration) {
        let playing = self.is_playing();
        if playing {
            self.frame = self.frame_at(now);
        }
        self.rate = rate;
        self.duration = duration.max(1);
        self.frame = self.clamp(self.frame);
        if playing {
            self.state = State::Playing {
                anchor_frame: self.frame,
                anchor_time: now,
            };
        }
    }

    /// Starts playback at `now`.
    ///
    /// When playback is not looping and the playhead is on the last frame,
    /// playback restarts from the beginning.
    pub fn play(&mut self, now: Duration) {
        if !self.looping && self.frame >= self.duration - 1 {
            self.frame = 0;
        }
        self.state = State::Playing {
            anchor_frame: self.frame,
            anchor_time: now,
        };
    }

    /// Stops playback, freezing on the frame `now` maps to.
    pub fn pause(&mut self, now: Duration) {
        if self.is_playing() {
            self.frame = self.frame_at(now);
        }
        self.state = State::Stopped;
    }

    /// Toggles between playing and stopped.
    pub fn toggle(&mut self, now: Duration) {
        if self.is_playing() {
            self.pause(now);
        } else {
            self.play(now);
        }
    }

    /// Moves the playhead by `delta` frames and stops playback.
    pub fn step(&mut self, delta: i64) {
        self.state = State::Stopped;
        self.frame = self.clamp(self.frame.saturating_add(delta));
    }

    /// Jumps to the first frame and stops playback.
    pub fn go_to_start(&mut self) {
        self.state = State::Stopped;
        self.frame = 0;
    }

    /// Jumps to the last frame and stops playback.
    pub fn go_to_end(&mut self) {
        self.state = State::Stopped;
        self.frame = self.duration - 1;
    }

    /// Selects a frame (clamped) without entering any other state.
    ///
    /// If the transport is playing, playback stops because there is no clock
    /// instant to re-anchor to; callers that scrub should use
    /// [`Self::begin_scrub`] first.
    pub fn set_frame(&mut self, frame: i64) {
        self.frame = self.clamp(frame);
        if self.is_playing() {
            self.state = State::Stopped;
        }
    }

    /// Enters scrubbing mode; the frame follows [`Self::set_frame`] without
    /// stopping the drag.
    pub fn begin_scrub(&mut self) {
        self.state = State::Scrubbing;
    }

    /// Leaves scrubbing mode, keeping the last scrubbed frame.
    pub fn end_scrub(&mut self) {
        if self.is_scrubbing() {
            self.state = State::Stopped;
        }
    }

    /// Advances the displayed frame to `now` while playing.
    ///
    /// Unlike [`Self::set_frame`] this keeps the playing state. When looping is
    /// off and the last frame is reached, playback stops on that frame.
    pub fn advance(&mut self, now: Duration) {
        if !self.is_playing() {
            return;
        }
        self.frame = self.frame_at(now);
        if !self.looping && self.frame >= self.duration - 1 {
            self.state = State::Stopped;
        }
    }

    /// Moves the playhead to `frame` and re-anchors playback to `now`.
    ///
    /// Used while the audio device is the clock: the frame follows the
    /// played samples, not the wall clock.
    pub fn follow(&mut self, frame: i64, now: Duration) {
        self.frame = self.clamp(frame);
        if self.is_playing() {
            self.state = State::Playing {
                anchor_frame: self.frame,
                anchor_time: now,
            };
        }
    }

    /// Follows the audio clock and handles the end of the mixed range.
    ///
    /// `ended` is true once the device consumed every mixed sample. In loop
    /// mode the transport wraps to the first frame, re-anchors at `now` and
    /// returns `Some(0)`: the caller restarts the audio stream at the first
    /// sample so the next gesture is sample-exact. Without looping, playback
    /// stops on the last frame.
    ///
    /// The return value is the loop-start frame that must be restarted, or
    /// `None` when the transport simply followed the clock.
    pub fn follow_audio(&mut self, position: f64, ended: bool, now: Duration) -> Option<i64> {
        if !self.is_playing() {
            return None;
        }
        if ended {
            if self.looping {
                self.frame = 0;
                self.state = State::Playing {
                    anchor_frame: 0,
                    anchor_time: now,
                };
                return Some(0);
            }
            self.frame = self.duration - 1;
            self.state = State::Stopped;
            return None;
        }
        let frame = audio_frame(position, self.rate);
        self.follow(frame, now);
        None
    }

    /// Maps wall-clock `now` to the frame to display.
    ///
    /// Stopped and scrubbing transports return their current frame unchanged.
    /// While playing, elapsed time is converted with integer arithmetic; loop
    /// mode wraps inside the composition, otherwise the result is clamped to
    /// the last frame.
    #[must_use]
    pub fn frame_at(&self, now: Duration) -> i64 {
        let State::Playing {
            anchor_frame,
            anchor_time,
        } = self.state
        else {
            return self.frame;
        };

        let elapsed = now.saturating_sub(anchor_time);
        let offset = elapsed_frames(elapsed, self.rate);
        let absolute = i128::from(anchor_frame) + i128::from(offset);

        if self.looping {
            let duration = i128::from(self.duration);
            let wrapped = absolute.rem_euclid(duration);
            i64::try_from(wrapped).unwrap_or(0)
        } else {
            let last = i128::from(self.duration - 1);
            i64::try_from(absolute.clamp(0, last)).unwrap_or(0)
        }
    }

    fn clamp(&self, frame: i64) -> i64 {
        frame.clamp(0, self.duration - 1)
    }
}

/// Formats a frame index as non-drop-frame timecode `HH:MM:SS:FF`.
///
/// The frame field uses the nominal integer rate rounded from the rational
/// rate (24 for `24000/1001`). Drop-frame timecode is out of scope, so the
/// displayed timecode can drift from the exact duration at fractional rates;
/// the precise frame index is always shown next to it.
#[must_use]
pub fn timecode(frame: i64, rate: FrameRate) -> String {
    let mut text = String::with_capacity(11);
    write_timecode(&mut text, frame, rate);
    text
}

/// [`timecode`] into an existing buffer, reusing its allocation.
pub fn write_timecode(out: &mut String, frame: i64, rate: FrameRate) {
    use std::fmt::Write as _;
    let numerator = i64::from(rate.numerator());
    let denominator = i64::from(rate.denominator());
    let nominal = ((numerator + denominator / 2) / denominator).max(1);
    let frame = frame.max(0);
    let total_seconds = frame / nominal;
    let frames = frame % nominal;
    let hours = total_seconds / 3600;
    let minutes = total_seconds / 60 % 60;
    let seconds = total_seconds % 60;
    out.clear();
    let _ = write!(out, "{hours:02}:{minutes:02}:{seconds:02}:{frames:02}");
}

/// Frames skipped when playback moved the playhead from `before` to `after`.
///
/// Moving forward by one frame skips nothing. A smaller `after` means playback
/// wrapped at the loop point, so the distance is counted through the end of
/// the `duration`-frame composition. Equal frames skip nothing.
#[must_use]
pub fn skipped_frames(before: i64, after: i64, duration: i64) -> u64 {
    let step = if after >= before {
        after - before
    } else {
        duration.max(1) - before + after
    };
    u64::try_from(step - 1).unwrap_or(0)
}

/// Converts an elapsed duration to whole frames at `rate`.
///
/// `frames = elapsed_ns * fps_num / (fps_den * 1_000_000_000)`, truncated
/// toward zero. Saturating arithmetic keeps extreme inputs finite.
#[must_use]
pub fn elapsed_frames(elapsed: Duration, rate: FrameRate) -> i64 {
    let numerator = u128::from(rate.numerator());
    let denominator = u128::from(rate.denominator()).saturating_mul(1_000_000_000);
    let frames = elapsed.as_nanos().saturating_mul(numerator) / denominator.max(1);
    i64::try_from(frames).unwrap_or(i64::MAX)
}

/// The composition frame at `position` seconds on the audio clock.
///
/// The audio device counts 48 kHz samples exactly, so this is the one mapping
/// between the audio clock and the playhead. Rounding keeps it within half a
/// frame of the device cursor.
#[must_use]
pub fn audio_frame(position: f64, rate: FrameRate) -> i64 {
    if !position.is_finite() {
        return 0;
    }
    #[allow(clippy::cast_possible_truncation)]
    let frame = (position * rate.as_f64()).round() as i64;
    frame
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ntsc() -> FrameRate {
        FrameRate::new(24000, 1001).unwrap()
    }

    fn rate_60() -> FrameRate {
        FrameRate::new(60, 1).unwrap()
    }

    fn at(seconds: f64) -> Duration {
        Duration::from_secs_f64(seconds)
    }

    #[test]
    fn play_pause_and_toggle_follow_the_clock() {
        let mut transport = Transport::new(rate_60(), 300);
        transport.play(at(0.0));
        assert!(transport.is_playing());
        assert_eq!(transport.frame_at(at(1.0)), 60);
        transport.pause(at(2.0));
        assert!(!transport.is_playing());
        assert_eq!(transport.frame(), 120);
        assert_eq!(transport.frame_at(at(5.0)), 120, "paused frames never move");

        transport.toggle(at(10.0));
        assert!(transport.is_playing());
        assert_eq!(transport.frame_at(at(11.0)), 180);
        transport.toggle(at(11.0));
        assert!(!transport.is_playing());
    }

    #[test]
    fn stepping_clamps_at_both_ends_and_stops_playback() {
        let mut transport = Transport::new(rate_60(), 10);
        transport.play(at(0.0));
        transport.step(3);
        assert_eq!(transport.frame(), 3);
        assert!(!transport.is_playing());
        transport.step(-10);
        assert_eq!(transport.frame(), 0);
        transport.step(100);
        assert_eq!(transport.frame(), 9);
        transport.go_to_start();
        assert_eq!(transport.frame(), 0);
        transport.go_to_end();
        assert_eq!(transport.frame(), 9);
    }

    #[test]
    fn set_frame_is_clamped() {
        let mut transport = Transport::new(rate_60(), 10);
        transport.set_frame(5);
        assert_eq!(transport.frame(), 5);
        transport.set_frame(-3);
        assert_eq!(transport.frame(), 0);
        transport.set_frame(40);
        assert_eq!(transport.frame(), 9);
    }

    #[test]
    fn looping_wraps_and_non_looping_clamps() {
        let mut transport = Transport::new(rate_60(), 60);
        transport.set_looping(true);
        transport.play(at(0.0));
        assert_eq!(transport.frame_at(at(1.0)), 0, "one full loop");
        assert_eq!(transport.frame_at(at(1.5)), 30);

        transport.set_looping(false);
        transport.set_frame(0);
        transport.play(at(0.0));
        assert_eq!(transport.frame_at(at(2.0)), 59, "clamped to the last frame");
    }

    #[test]
    fn scrubbing_keeps_the_scrubbed_frame() {
        let mut transport = Transport::new(rate_60(), 60);
        transport.begin_scrub();
        assert!(transport.is_scrubbing());
        transport.set_frame(42);
        assert_eq!(transport.frame_at(at(100.0)), 42);
        transport.end_scrub();
        assert!(!transport.is_scrubbing());
        assert_eq!(transport.frame(), 42);
    }

    #[test]
    fn the_audio_clock_maps_seconds_to_frames_without_drift() {
        let rate = rate_60();
        assert_eq!(audio_frame(0.0, rate), 0);
        assert_eq!(audio_frame(1.0 / 60.0, rate), 1);
        assert_eq!(audio_frame(10.0 * 60.0, rate), 36_000);
        // 48 000 samples are exactly one second at every integer rate.
        for samples in [1_u64, 799, 800, 48_000, 2_880_000] {
            #[allow(clippy::cast_precision_loss)]
            let position = samples as f64 / 48_000.0;
            let expected = ((samples * 60) as f64 / 48_000.0).round() as i64;
            assert_eq!(audio_frame(position, rate), expected);
        }
    }

    #[test]
    fn ntsc_playback_has_no_drift_over_ten_minutes() {
        let mut transport = Transport::new(ntsc(), 1_000_000);
        transport.play(Duration::ZERO);
        // 24000/1001 fps: one second is 23.976 frames, so 600 s are
        // floor(600 * 24000 / 1001) = 14385 frames. Integer arithmetic, not a
        // float accumulator, so the value is exact.
        assert_eq!(transport.frame_at(at(1.0)), 23);
        assert_eq!(transport.frame_at(at(300.0)), 7192);
        assert_eq!(transport.frame_at(at(600.0)), 14385);
        assert_eq!(
            elapsed_frames(Duration::from_mins(10), ntsc()),
            14_385,
            "no accumulated floating-point drift"
        );
    }

    #[test]
    fn duration_changes_clamp_the_current_frame() {
        let mut transport = Transport::new(rate_60(), 300);
        transport.set_frame(250);
        transport.retime(rate_60(), 100, Duration::ZERO);
        assert_eq!(transport.frame(), 99);
        transport.retime(rate_60(), 500, Duration::ZERO);
        assert_eq!(transport.frame(), 99);
    }

    #[test]
    fn retiming_while_playing_keeps_playing_from_the_shown_frame() {
        let mut transport = Transport::new(rate_60(), 1000);
        transport.play(at(0.0));
        // At 2 s we show frame 120; switching to 30 fps must not jump to the
        // frame the old anchor maps to at the new rate.
        transport.retime(FrameRate::new(30, 1).unwrap(), 1000, at(2.0));
        assert!(transport.is_playing());
        assert_eq!(transport.frame_at(at(2.0)), 120);
        assert_eq!(transport.frame_at(at(3.0)), 150);
    }

    #[test]
    fn advance_keeps_playing_and_stops_at_the_end_when_not_looping() {
        let mut transport = Transport::new(rate_60(), 60);
        transport.set_looping(false);
        transport.play(Duration::ZERO);
        transport.advance(at(0.5));
        assert!(transport.is_playing());
        assert_eq!(transport.frame(), 30);
        transport.advance(at(2.0));
        assert!(!transport.is_playing(), "reached the last frame");
        assert_eq!(transport.frame(), 59);
    }

    #[test]
    fn timecode_uses_the_nominal_integer_rate() {
        let ntsc = ntsc();
        assert_eq!(timecode(0, ntsc), "00:00:00:00");
        assert_eq!(timecode(24, ntsc), "00:00:01:00");
        assert_eq!(timecode(48 + 12, ntsc), "00:00:02:12");
        assert_eq!(timecode(24 * 3600, ntsc), "01:00:00:00");
        assert_eq!(timecode(60, rate_60()), "00:00:01:00");
        assert_eq!(timecode(-5, rate_60()), "00:00:00:00");
    }

    #[test]
    fn skipped_frames_count_through_the_loop_point() {
        assert_eq!(skipped_frames(10, 10, 300), 0);
        assert_eq!(skipped_frames(10, 11, 300), 0);
        assert_eq!(skipped_frames(10, 13, 300), 2);
        // 298 -> 299 -> 0 -> 1: frames 299 and 0 were skipped.
        assert_eq!(skipped_frames(298, 1, 300), 2);
        assert_eq!(skipped_frames(299, 0, 300), 0);
    }

    #[test]
    fn a_finished_audio_stream_restarts_looping_playback_at_the_first_frame() {
        let mut transport = Transport::new(rate_60(), 60);
        transport.play(Duration::ZERO);
        assert_eq!(
            transport.follow_audio(0.5, false, at(0.5)),
            None,
            "mid-stream positions only move the playhead"
        );
        assert_eq!(transport.frame(), 30);

        // The mixer reaches the end of the range: wrap and ask the caller to
        // restart the stream on the first sample.
        assert_eq!(transport.follow_audio(1.0, true, at(1.0)), Some(0));
        assert_eq!(
            transport.frame(),
            0,
            "the playhead must not stay on the last frame"
        );
        assert!(transport.is_playing());

        // The restarted stream advances from its own anchor.
        transport.follow_audio(0.1, false, at(1.1));
        assert_eq!(transport.frame(), 6);
    }

    #[test]
    fn a_finished_audio_stream_stops_playback_when_not_looping() {
        let mut transport = Transport::new(rate_60(), 60);
        transport.set_looping(false);
        transport.play(Duration::ZERO);
        assert_eq!(transport.follow_audio(1.0, true, at(1.0)), None);
        assert_eq!(transport.frame(), 59);
        assert!(!transport.is_playing());
    }

    #[test]
    fn a_paused_transport_never_changes_frame() {
        let mut transport = Transport::new(rate_60(), 300);
        transport.set_frame(77);
        for t in [0.0, 1.0, 10.0, 3600.0] {
            assert_eq!(transport.frame_at(at(t)), 77);
        }
    }
}

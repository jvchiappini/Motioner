//! Media analysis against generated media with known truth.

#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)] // synthetic sample/frame indices, bounded by the test signal length

use motioner_analysis::{
    analyze_audio, analyze_image, analyze_video, GrayFrame, MediaOptions, RgbaImage,
};

fn click_track(bpm: f64, seconds: f64) -> Vec<f32> {
    let sample_rate = 44_100.0_f64;
    let mut samples = vec![0.0_f32; (seconds * sample_rate) as usize];
    let period = 60.0 / bpm;
    let mut time = 0.0;
    while time < seconds {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let start = (time * sample_rate) as usize;
        // A 6 ms decaying 1 kHz burst: a clean onset for the envelope.
        for offset in 0..(0.006 * sample_rate) as usize {
            if start + offset >= samples.len() {
                break;
            }
            #[allow(clippy::cast_precision_loss)]
            let t = offset as f64 / sample_rate;
            let envelope = (-t / 0.002).exp();
            #[allow(clippy::cast_precision_loss)]
            let value = envelope * (std::f64::consts::TAU * 1_000.0 * t).sin() * 0.9;
            #[allow(clippy::cast_possible_truncation)]
            let value = value as f32;
            samples[start + offset] += value;
        }
        time += period;
    }
    samples
}

#[test]
fn tempo_and_beats_are_within_tolerance() {
    for bpm in [90.0, 120.0, 128.0] {
        let samples = click_track(bpm, 6.0);
        let analysis = analyze_audio(&samples, 44_100);
        let measured = analysis.bpm.unwrap_or_else(|| panic!("no tempo at {bpm}"));
        assert!((measured - bpm).abs() <= 1.0, "bpm {measured} vs {bpm}");
        assert!(analysis.beats.len() >= 5, "{bpm}: {:?}", analysis.beats);
        let period = 60.0 / bpm;
        for (index, beat) in analysis.beats.iter().enumerate() {
            let expected = index as f64 * period;
            assert!(
                (beat - expected).abs() <= 0.020,
                "{bpm} bpm beat {index}: {beat} vs {expected}"
            );
        }
        assert!(!analysis.downbeats.is_empty());
        assert!(!analysis.sections.is_empty());
    }
}

#[test]
fn an_image_with_a_flat_half_ranks_it_for_text() {
    let width = 240_u32;
    let height = 120_u32;
    let mut rgba8 = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            if x < width / 2 {
                // Busy checkerboard on the left.
                let value = if (x / 8 + y / 8) % 2 == 0 { 20 } else { 230 };
                rgba8.extend_from_slice(&[value, value, value, 255]);
            } else {
                rgba8.extend_from_slice(&[90, 90, 90, 255]);
            }
        }
    }
    let image = RgbaImage {
        width,
        height,
        rgba8,
    };
    let analysis = analyze_image(&image, &MediaOptions::default());
    assert!(!analysis.palette.is_empty());
    let best = analysis.empty_regions.first().expect("a region");
    let center = best.rect.center();
    assert!(
        center.x > f64::from(width) / 2.0,
        "best region should be the flat right half: {best:?}"
    );
    assert!(best.busyness < 0.2, "{best:?}");
    assert!(analysis.salient.is_some());
    assert_eq!(analysis.luminance_grid.len(), 24);
}

fn frame(value: u8) -> GrayFrame {
    GrayFrame {
        width: 32,
        height: 18,
        luma: vec![value; 32 * 18],
    }
}

#[test]
fn video_cuts_land_on_the_known_frames() {
    let mut frames = Vec::new();
    for index in 0..60 {
        let value = match index {
            0..=19 => 30,
            20..=39 => 220,
            _ => 60,
        };
        frames.push(frame(value));
    }
    let analysis = analyze_video(&frames, 0.3);
    assert_eq!(analysis.cuts, vec![20, 40]);
    assert!(analysis.motion.iter().any(|value| *value > 0.5));
    assert!(!analysis.calm_segments.is_empty());
}

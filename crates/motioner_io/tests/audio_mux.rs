//! The MP4 encoder really muxes the mixed track and keeps the video duration.

use std::path::PathBuf;
use std::process::Command;

use motioner_io::{write_wav_i16, FfmpegLocation, Mp4Encoder, VideoSettings, SAMPLE_RATE};

fn temp_dir(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("motioner-audio-mux-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn command_available(program: &str) -> bool {
    Command::new(program)
        .arg("-version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[test]
fn the_muxed_mp4_keeps_the_video_duration() {
    if !command_available("ffmpeg") || !command_available("ffprobe") {
        eprintln!("skipping: ffmpeg/ffprobe are not on PATH");
        return;
    }
    let dir = temp_dir("duration");
    let wav = dir.join("mix.wav");
    let samples: Vec<f32> = (0..SAMPLE_RATE)
        .flat_map(|index| {
            #[allow(clippy::cast_precision_loss)]
            let value = (index as f32 * 0.01).sin() * 0.2;
            [value, value]
        })
        .collect();
    write_wav_i16(&wav, &samples, SAMPLE_RATE).expect("wav");

    let output = dir.join("out.mp4");
    let settings = VideoSettings {
        width: 64,
        height: 64,
        fps_numerator: 30,
        fps_denominator: 1,
        crf: 30,
    };
    let mut encoder = Mp4Encoder::start_with_audio(
        &output,
        &settings,
        &FfmpegLocation::FromEnvOrPath,
        Some(&wav),
    )
    .expect("encoder");
    let frame = vec![0_u8; 64 * 64 * 4];
    for _ in 0..30 {
        encoder.push_frame(&frame).expect("frame");
    }
    encoder.finish().expect("finish");

    let probe = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
            &output.to_string_lossy(),
        ])
        .output()
        .expect("ffprobe");
    assert!(
        probe.status.success(),
        "ffprobe failed: {}",
        String::from_utf8_lossy(&probe.stderr)
    );
    let duration: f64 = String::from_utf8_lossy(&probe.stdout)
        .trim()
        .parse()
        .expect("duration");
    assert!(
        (duration - 1.0).abs() < 0.15,
        "expected a one-second MP4, got {duration}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

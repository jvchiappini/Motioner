//! Headless regression for the preview decode-ahead path.
//!
//! The preview worker fills its bounded cache by decoding a contiguous batch
//! in one `ffmpeg` process ([`motioner_io::decode_frames`]). This test
//! generates an all-intra proxy-sized clip with ffmpeg's own test source and
//! asserts that a 30-frame lookahead batch finishes inside a budget, that the
//! frames differ from each other and that the first batch frame is identical
//! to the single-frame decode. It skips when ffmpeg is not installed.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use motioner_io::{decode_frame, decode_frames, FfmpegLocation, FrameStream};

const WIDTH: u32 = 640;
const HEIGHT: u32 = 360;
const COUNT: usize = 30;

fn ffmpeg_available() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// Generates a 3 s all-intra clip, the shape of a preview proxy.
fn generate(dir: &Path) -> Option<PathBuf> {
    let path = dir.join("proxy.mp4");
    let status = Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=640x360:rate=60:duration=3",
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-crf",
            "30",
            "-g",
            "1",
            "-keyint_min",
            "1",
            "-pix_fmt",
            "yuv420p",
            &path.to_string_lossy(),
        ])
        .status()
        .ok()?;
    status.success().then_some(path)
}

#[test]
fn a_lookahead_batch_decodes_inside_the_budget() {
    if !ffmpeg_available() {
        return;
    }
    let dir = std::env::temp_dir().join(format!(
        "motioner_video_decode_ahead_{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let Some(path) = generate(&dir) else {
        return;
    };

    let location = FfmpegLocation::FromEnvOrPath;
    let started = Instant::now();
    let frames = decode_frames(&path, 0.0, WIDTH, HEIGHT, COUNT, &location).unwrap();
    let elapsed = started.elapsed();

    assert_eq!(frames.len(), COUNT, "one process returns the whole batch");
    for pair in frames.windows(2) {
        assert_ne!(pair[0], pair[1], "consecutive source frames must differ");
    }
    let single = decode_frame(&path, 0.0, WIDTH, HEIGHT, &location).unwrap();
    assert_eq!(frames[0], single, "batch and single decode agree");

    // Debug builds pay for unoptimised rawvideo handling and process startup;
    // release builds are the performance target. Both budgets are generous
    // enough to catch a regression from one batch to one process per frame.
    let budget = if cfg!(debug_assertions) { 15.0 } else { 3.0 };
    assert!(
        elapsed.as_secs_f64() < budget,
        "{COUNT} frames took {elapsed:?}, budget {budget:.1} s"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_persistent_stream_returns_the_same_frames_as_a_batch() {
    if !ffmpeg_available() {
        return;
    }
    let dir = std::env::temp_dir().join(format!("motioner_video_stream_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let Some(path) = generate(&dir) else {
        return;
    };

    let location = FfmpegLocation::FromEnvOrPath;
    let batch = decode_frames(&path, 0.0, WIDTH, HEIGHT, COUNT, &location).unwrap();
    let mut stream = FrameStream::open(&path, 0.0, WIDTH, HEIGHT, &location).unwrap();
    let mut streamed = Vec::with_capacity(COUNT);
    for _ in 0..COUNT {
        match stream.next_frame().unwrap() {
            Some(frame) => streamed.push(frame),
            None => break,
        }
    }
    assert_eq!(streamed, batch, "the worker stream and the batch agree");

    let _ = std::fs::remove_dir_all(&dir);
}

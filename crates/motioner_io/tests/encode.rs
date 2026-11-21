//! End-to-end check of the ffmpeg streaming encoder.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use motioner_io::{ExportError, FfmpegLocation, Mp4Encoder, VideoSettings};

const WIDTH: u32 = 64;
const HEIGHT: u32 = 64;
const FRAMES: u64 = 10;
const FRAME_BYTES: usize = 64 * 64 * 4;

fn temp_mp4_path() -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "motioner-io-encode-{}-{unique}.mp4",
        std::process::id()
    ))
}

fn settings() -> VideoSettings {
    VideoSettings {
        width: WIDTH,
        height: HEIGHT,
        fps_numerator: 24,
        fps_denominator: 1,
        crf: 23,
    }
}

/// Builds a deterministic frame: the frame index drives the red channel and
/// a green square slides horizontally.
fn frame(index: u64) -> Vec<u8> {
    let red = u8::try_from(index.saturating_mul(20)).unwrap_or(u8::MAX);
    let shift = u32::try_from(index.saturating_mul(5) % 48).unwrap_or(0);
    let mut rgba = Vec::with_capacity(FRAME_BYTES);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let in_square = (shift..shift.saturating_add(8)).contains(&x) && (16..24).contains(&y);
            if in_square {
                rgba.extend_from_slice(&[0, 255, 0, 255]);
            } else {
                rgba.extend_from_slice(&[red, 0, 0, 255]);
            }
        }
    }
    assert_eq!(rgba.len(), FRAME_BYTES);
    rgba
}

fn start(path: &Path) -> Result<Option<Mp4Encoder>, Box<dyn std::error::Error>> {
    match Mp4Encoder::start(path, &settings(), &FfmpegLocation::FromEnvOrPath) {
        Ok(encoder) => Ok(Some(encoder)),
        Err(ExportError::FfmpegNotFound { message }) => {
            eprintln!("skipping ffmpeg encode test: {message}");
            Ok(None)
        }
        Err(err) => Err(err.into()),
    }
}

#[test]
fn encodes_ten_frames_to_mp4() -> Result<(), Box<dyn std::error::Error>> {
    let path = temp_mp4_path();
    let Some(mut encoder) = start(&path)? else {
        return Ok(());
    };
    for index in 0..FRAMES {
        encoder.push_frame(&frame(index))?;
    }
    let summary = encoder.finish()?;
    assert_eq!(summary.frames, FRAMES);
    let status = summary.status;
    assert!(status.success(), "ffmpeg failed: {status}");

    if let Some(count) = probe_frame_count(&path)? {
        assert_eq!(count, FRAMES);
    }

    let _ = std::fs::remove_file(&path);
    Ok(())
}

#[test]
fn dropping_an_unfinished_encoder_reaps_the_child() -> Result<(), Box<dyn std::error::Error>> {
    let path = temp_mp4_path();
    let Some(mut encoder) = start(&path)? else {
        return Ok(());
    };
    encoder.push_frame(&frame(0))?;
    drop(encoder);
    let _ = std::fs::remove_file(&path);
    Ok(())
}

fn probe_frame_count(path: &Path) -> Result<Option<u64>, Box<dyn std::error::Error>> {
    let output = match Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-count_frames",
            "-show_entries",
            "stream=nb_read_frames",
            "-of",
            "default=nw=1",
        ])
        .arg(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
    {
        Ok(output) => output,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("skipping ffprobe frame-count check: ffprobe is not on PATH");
            return Ok(None);
        }
        Err(err) => return Err(err.into()),
    };
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(std::io::Error::other(format!("ffprobe failed: {}", stderr.trim())).into());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let Some(raw) = stdout
        .lines()
        .find_map(|line| line.strip_prefix("nb_read_frames="))
    else {
        return Err(std::io::Error::other("ffprobe did not report nb_read_frames").into());
    };
    Ok(Some(raw.trim().parse::<u64>()?))
}

//! Video probing, frame decode and proxy generation through ffmpeg.
//!
//! Frames are decoded only on demand and always from exact timestamps. Proxies
//! are low-resolution all-intra files under `cache/`; `render`/`still` never
//! use them.

use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex};

use crate::encode::{resolve_program, FfmpegLocation};
use crate::error::ExportError;

/// Stream metadata needed to select source frames.
#[derive(Debug, Clone, PartialEq)]
pub struct VideoInfo {
    /// Natural width in pixels.
    pub width: u32,
    /// Natural height in pixels.
    pub height: u32,
    /// Frame-rate numerator.
    pub fps_numerator: u32,
    /// Frame-rate denominator.
    pub fps_denominator: u32,
    /// Length in seconds.
    pub duration: f64,
}

impl VideoInfo {
    /// Frames per second as an `f64`.
    #[must_use]
    pub fn fps(&self) -> f64 {
        f64::from(self.fps_numerator) / f64::from(self.fps_denominator.max(1))
    }
}

/// The `ffprobe` next to the configured `ffmpeg`.
fn probe_program(location: &FfmpegLocation) -> PathBuf {
    match location {
        FfmpegLocation::Path(path) => {
            let name = if cfg!(windows) {
                "ffprobe.exe"
            } else {
                "ffprobe"
            };
            path.with_file_name(name)
        }
        FfmpegLocation::FromEnvOrPath => std::env::var_os("MOTIONER_FFPROBE")
            .filter(|value| !value.is_empty())
            .map_or_else(|| PathBuf::from("ffprobe"), PathBuf::from),
    }
}

/// Reads a video's size, rate and duration.
///
/// # Errors
///
/// [`ExportError::FfmpegNotFound`] when `ffprobe` is missing and
/// [`ExportError::Ffmpeg`] when the stream cannot be read.
pub fn probe(source: &Path, location: &FfmpegLocation) -> Result<VideoInfo, ExportError> {
    let program = probe_program(location);
    let output = Command::new(&program)
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height,r_frame_rate:format=duration",
            "-of",
            "default=noprint_wrappers=1",
            &source.to_string_lossy(),
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .output()
        .map_err(|error| spawn_error(program.as_os_str(), &error))?;
    if !output.status.success() {
        return Err(ffmpeg_error(&output.stderr, "ffprobe failed"));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut width = None;
    let mut height = None;
    let mut rate = None;
    let mut duration = None;
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key {
            "width" => width = value.trim().parse::<u32>().ok(),
            "height" => height = value.trim().parse::<u32>().ok(),
            "r_frame_rate" => rate = parse_rate(value.trim()),
            "duration" => {
                duration = value
                    .trim()
                    .parse::<f64>()
                    .ok()
                    .filter(|value| *value > 0.0);
            }
            _ => {}
        }
    }
    let (Some(width), Some(height), Some((fps_numerator, fps_denominator))) = (width, height, rate)
    else {
        return Err(ExportError::Ffmpeg {
            message: "the video stream has no readable size or frame rate".to_owned(),
        });
    };
    let duration = duration.ok_or_else(|| ExportError::Ffmpeg {
        message: "the video has no readable duration".to_owned(),
    })?;
    Ok(VideoInfo {
        width,
        height,
        fps_numerator,
        fps_denominator,
        duration,
    })
}

/// Parses `30000/1001` or `30`.
fn parse_rate(text: &str) -> Option<(u32, u32)> {
    let (numerator, denominator) = text.split_once('/').unwrap_or((text, "1"));
    let numerator = numerator.parse::<u32>().ok()?;
    let denominator = denominator.parse::<u32>().ok()?.max(1);
    (numerator > 0).then_some((numerator, denominator))
}

/// Decodes the first frame at or after `seconds` to straight RGBA8.
///
/// # Errors
///
/// [`ExportError::FfmpegNotFound`] for a missing binary,
/// [`ExportError::Ffmpeg`] for a decode failure and
/// [`ExportError::SizeMismatch`] when the frame is short.
pub fn decode_frame(
    source: &Path,
    seconds: f64,
    width: u32,
    height: u32,
    location: &FfmpegLocation,
) -> Result<Vec<u8>, ExportError> {
    if seconds < 0.0 || !seconds.is_finite() {
        return Err(ExportError::Ffmpeg {
            message: "the frame timestamp must be finite and non-negative".to_owned(),
        });
    }
    let program = resolve_program(location);
    let output = Command::new(&program)
        .args([
            "-v",
            "error",
            "-ss",
            &format!("{seconds:.6}"),
            "-i",
            &source.to_string_lossy(),
            "-frames:v",
            "1",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgba",
            "-s",
            &format!("{width}x{height}"),
            "-",
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .output()
        .map_err(|error| spawn_error(&program, &error))?;
    if !output.status.success() {
        return Err(ffmpeg_error(
            &output.stderr,
            "ffmpeg failed to decode a video frame",
        ));
    }
    let expected = usize::try_from(u64::from(width) * u64::from(height) * 4).unwrap_or(usize::MAX);
    if output.stdout.len() < expected {
        return Err(ExportError::SizeMismatch {
            expected,
            actual: output.stdout.len(),
        });
    }
    Ok(output.stdout[..expected].to_vec())
}

/// Decodes up to `count` consecutive frames starting at `seconds` in one
/// `ffmpeg` process.
///
/// This is the batch form of [`decode_frame`], used by the preview decode
/// worker to fill the bounded frame cache ahead of the playhead. A proxy is
/// all-intra, so a single seek returns exact consecutive frames. The result
/// may be shorter than `count` near the end of the file; a partial frame is
/// never returned.
///
/// # Errors
///
/// [`ExportError::FfmpegNotFound`] for a missing binary,
/// [`ExportError::Ffmpeg`] for a decode failure and
/// [`ExportError::SizeMismatch`] when not even one complete frame was read.
pub fn decode_frames(
    source: &Path,
    seconds: f64,
    width: u32,
    height: u32,
    count: usize,
    location: &FfmpegLocation,
) -> Result<Vec<Vec<u8>>, ExportError> {
    if seconds < 0.0 || !seconds.is_finite() {
        return Err(ExportError::Ffmpeg {
            message: "the frame timestamp must be finite and non-negative".to_owned(),
        });
    }
    if count == 0 {
        return Ok(Vec::new());
    }
    let program = resolve_program(location);
    let frames = count.to_string();
    let output = Command::new(&program)
        .args([
            "-v",
            "error",
            "-ss",
            &format!("{seconds:.6}"),
            "-i",
            &source.to_string_lossy(),
            "-frames:v",
            &frames,
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgba",
            "-s",
            &format!("{width}x{height}"),
            "-",
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .output()
        .map_err(|error| spawn_error(&program, &error))?;
    if !output.status.success() {
        return Err(ffmpeg_error(
            &output.stderr,
            "ffmpeg failed to decode a video frame batch",
        ));
    }
    let expected = usize::try_from(u64::from(width) * u64::from(height) * 4).unwrap_or(usize::MAX);
    if output.stdout.len() < expected {
        return Err(ExportError::SizeMismatch {
            expected,
            actual: output.stdout.len(),
        });
    }
    Ok(output
        .stdout
        .chunks_exact(expected)
        .map(<[u8]>::to_vec)
        .collect())
}

/// A long-lived sequential decode of consecutive frames.
///
/// The preview worker keeps one stream open while the playhead moves forward:
/// `ffmpeg` decodes on demand and backpressure stops it after the frames the
/// worker has read, so no process is spawned per batch. Dropping the stream
/// kills the child.
pub struct FrameStream {
    child: Child,
    stdout: BufReader<ChildStdout>,
    stderr: Arc<Mutex<Vec<u8>>>,
    expected: usize,
}

impl FrameStream {
    /// Starts decoding at `seconds` and keeps the process alive until EOF or
    /// [`Drop`].
    ///
    /// # Errors
    ///
    /// [`ExportError::FfmpegNotFound`] for a missing binary and
    /// [`ExportError::Ffmpeg`] when the process cannot be started.
    pub fn open(
        source: &Path,
        seconds: f64,
        width: u32,
        height: u32,
        location: &FfmpegLocation,
    ) -> Result<Self, ExportError> {
        if seconds < 0.0 || !seconds.is_finite() {
            return Err(ExportError::Ffmpeg {
                message: "the frame timestamp must be finite and non-negative".to_owned(),
            });
        }
        let program = resolve_program(location);
        let mut child = Command::new(&program)
            .args([
                "-v",
                "error",
                "-ss",
                &format!("{seconds:.6}"),
                "-threads",
                "1",
                "-i",
                &source.to_string_lossy(),
                "-f",
                "rawvideo",
                "-pix_fmt",
                "rgba",
                "-s",
                &format!("{width}x{height}"),
                "-",
            ])
            .stdin(Stdio::null())
            .stderr(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|error| spawn_error(&program, &error))?;
        let stdout = child.stdout.take().ok_or_else(|| ExportError::Ffmpeg {
            message: "ffmpeg produced no stdout pipe".to_owned(),
        })?;
        let stderr = Arc::new(Mutex::new(Vec::new()));
        if let Some(mut pipe) = child.stderr.take() {
            let sink = Arc::clone(&stderr);
            std::thread::spawn(move || {
                let mut data = Vec::new();
                let _ = pipe.read_to_end(&mut data);
                if let Ok(mut sink) = sink.lock() {
                    *sink = data;
                }
            });
        }
        let expected =
            usize::try_from(u64::from(width) * u64::from(height) * 4).unwrap_or(usize::MAX);
        Ok(Self {
            child,
            stdout: BufReader::new(stdout),
            stderr,
            expected,
        })
    }

    /// Reads the next frame, or `None` at the end of the stream.
    ///
    /// # Errors
    ///
    /// [`ExportError::Ffmpeg`] when the pipe fails before a whole frame.
    pub fn next_frame(&mut self) -> Result<Option<Vec<u8>>, ExportError> {
        let mut frame = vec![0_u8; self.expected];
        match self.stdout.read_exact(&mut frame) {
            Ok(()) => Ok(Some(frame)),
            Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => Ok(None),
            Err(error) => Err(ExportError::Ffmpeg {
                message: format!("cannot read a decoded frame: {error}"),
            }),
        }
    }

    /// The last stderr text, for a diagnostic after an unexpected end.
    #[must_use]
    pub fn diagnostic(&self) -> String {
        self.stderr
            .lock()
            .map(|data| String::from_utf8_lossy(&data).trim().to_owned())
            .unwrap_or_default()
    }
}

impl Drop for FrameStream {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Generates a low-resolution all-intra proxy and reports progress in
/// `0.0..=1.0`.
///
/// # Errors
///
/// [`ExportError::FfmpegNotFound`] for a missing binary and
/// [`ExportError::Ffmpeg`] when the encode fails.
pub fn make_proxy(
    source: &Path,
    destination: &Path,
    max_width: u32,
    duration: f64,
    location: &FfmpegLocation,
    mut progress: impl FnMut(f32),
) -> Result<(), ExportError> {
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(|error| ExportError::Io {
            message: format!("cannot create {}: {error}", parent.display()),
        })?;
    }
    let program = resolve_program(location);
    let scale = format!("scale='min({max_width},iw)':-2");
    let mut child = Command::new(&program)
        .args([
            "-v",
            "error",
            "-y",
            "-i",
            &source.to_string_lossy(),
            "-vf",
            &scale,
            "-an",
            "-g",
            "1",
            "-keyint_min",
            "1",
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-crf",
            "30",
            "-progress",
            "pipe:1",
            "-nostats",
            &destination.to_string_lossy(),
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|error| spawn_error(&program, &error))?;
    if let Some(stdout) = child.stdout.take() {
        let reader = BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
            if let Some(value) = line.strip_prefix("out_time_ms=") {
                if let Ok(micros) = value.parse::<u64>() {
                    #[allow(clippy::cast_precision_loss)]
                    let seconds = micros as f64 / 1_000_000.0;
                    let fraction = (seconds / duration.max(0.001)).clamp(0.0, 1.0);
                    #[allow(clippy::cast_possible_truncation)]
                    let fraction = fraction as f32;
                    progress(fraction);
                }
            }
        }
    }
    let mut stderr = Vec::new();
    if let Some(mut stream) = child.stderr.take() {
        let _ = stream.read_to_end(&mut stderr);
    }
    let status = child.wait().map_err(|error| ExportError::Ffmpeg {
        message: format!("cannot wait for ffmpeg: {error}"),
    })?;
    if !status.success() {
        return Err(ffmpeg_error(&stderr, "ffmpeg failed to build a proxy"));
    }
    progress(1.0);
    Ok(())
}

fn ffmpeg_error(stderr: &[u8], fallback: &str) -> ExportError {
    ExportError::Ffmpeg {
        message: String::from_utf8_lossy(stderr)
            .lines()
            .next_back()
            .unwrap_or(fallback)
            .to_owned(),
    }
}

fn spawn_error(program: &std::ffi::OsStr, error: &std::io::Error) -> ExportError {
    if error.kind() == std::io::ErrorKind::NotFound {
        ExportError::FfmpegNotFound {
            message: format!(
                "{} was not found; install ffmpeg or set MOTIONER_FFMPEG",
                program.to_string_lossy()
            ),
        }
    } else {
        ExportError::Ffmpeg {
            message: format!("cannot run {}: {error}", program.to_string_lossy()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_rate_strings_parse() {
        assert_eq!(parse_rate("30000/1001"), Some((30_000, 1001)));
        assert_eq!(parse_rate("25"), Some((25, 1)));
        assert_eq!(parse_rate("0/1"), None);
        assert_eq!(parse_rate("bad"), None);
    }

    #[test]
    fn a_negative_timestamp_is_rejected_before_spawning_ffmpeg() {
        let result = decode_frame(
            Path::new("missing.mp4"),
            -1.0,
            2,
            2,
            &FfmpegLocation::FromEnvOrPath,
        );
        assert!(matches!(result, Err(ExportError::Ffmpeg { .. })));
    }
}

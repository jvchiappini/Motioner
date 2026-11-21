//! Streaming H.264 MP4 encoding through `ffmpeg`.

use std::collections::VecDeque;
use std::ffi::{OsStr, OsString};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStderr, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use crate::error::ExportError;
use crate::frame::expected_len;

/// Environment variable that overrides the `ffmpeg` executable.
const FFMPEG_ENV: &str = "MOTIONER_FFMPEG";

/// Maximum number of stderr bytes kept for error reports.
const STDERR_CAPACITY: usize = 8 * 1024;

/// Settings for a streaming H.264 MP4 encode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoSettings {
    /// Frame width in pixels.
    pub width: u32,
    /// Frame height in pixels.
    pub height: u32,
    /// Frame-rate numerator.
    pub fps_numerator: u32,
    /// Frame-rate denominator.
    pub fps_denominator: u32,
    /// x264 constant rate factor; lower values retain more detail.
    pub crf: u8,
}

impl VideoSettings {
    /// Returns YouTube-oriented 1080p60 settings.
    ///
    /// The result is 1920x1080 at 60 frames per second with CRF 18. The
    /// encoder always uses the `medium` x264 preset, H.264 High profile
    /// through `libx264`, and `yuv420p` output.
    #[must_use]
    pub fn youtube_1080p() -> Self {
        Self {
            width: 1920,
            height: 1080,
            fps_numerator: 60,
            fps_denominator: 1,
            crf: 18,
        }
    }
}

impl Default for VideoSettings {
    fn default() -> Self {
        Self::youtube_1080p()
    }
}

/// Where to find the `ffmpeg` executable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FfmpegLocation {
    /// Use this exact executable path.
    Path(PathBuf),
    /// Use the `MOTIONER_FFMPEG` environment variable when it is set and
    /// non-empty, otherwise `ffmpeg` from `PATH`.
    FromEnvOrPath,
}

/// Outcome of a completed encode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodeSummary {
    /// Number of frames written to `ffmpeg`.
    pub frames: u64,
    /// `ffmpeg`'s exit status; it is successful whenever `finish` returns `Ok`.
    pub status: ExitStatus,
}

/// A streaming encoder that feeds raw RGBA8 frames to `ffmpeg`.
///
/// The child process reads tightly packed, top-row-first RGBA8 frames from
/// its standard input. Dropping an encoder that was not finished kills and
/// reaps the child so no zombie process is left behind.
pub struct Mp4Encoder {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    stderr_buffer: Arc<Mutex<VecDeque<u8>>>,
    stderr_thread: Option<JoinHandle<()>>,
    expected_frame_len: usize,
    frames: u64,
}

impl Mp4Encoder {
    /// Starts `ffmpeg` and prepares it to receive raw RGBA8 frames.
    ///
    /// `ffmpeg`'s standard output is discarded and its standard error is
    /// drained on a background thread into a bounded buffer, so a verbose
    /// child can never block on a full pipe.
    ///
    /// # Errors
    ///
    /// Returns [`ExportError::DimensionsTooLarge`] when the settings cannot be
    /// addressed as bytes, [`ExportError::FfmpegNotFound`] when no `ffmpeg`
    /// executable can be found, and [`ExportError::Ffmpeg`] when the child
    /// cannot be started or its pipes are unavailable.
    pub fn start(
        path: &Path,
        settings: &VideoSettings,
        ffmpeg: &FfmpegLocation,
    ) -> Result<Self, ExportError> {
        Self::start_with_audio(path, settings, ffmpeg, None)
    }

    /// Starts `ffmpeg` with an optional WAV track muxed into the MP4.
    ///
    /// # Errors
    ///
    /// Same failures as [`Mp4Encoder::start`].
    pub fn start_with_audio(
        path: &Path,
        settings: &VideoSettings,
        ffmpeg: &FfmpegLocation,
        audio: Option<&Path>,
    ) -> Result<Self, ExportError> {
        let expected_frame_len = expected_len(settings.width, settings.height)?;
        let program = resolve_program(ffmpeg);
        let mut command = Command::new(&program);
        command
            .args(ffmpeg_arguments_with_audio(settings, path, audio))
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        let mut child = command.spawn().map_err(|err| spawn_error(&program, &err))?;

        let Some(stdin) = child.stdin.take() else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(ExportError::Ffmpeg {
                message: "ffmpeg started without a stdin pipe".to_owned(),
            });
        };
        let Some(stderr) = child.stderr.take() else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(ExportError::Ffmpeg {
                message: "ffmpeg started without a stderr pipe".to_owned(),
            });
        };

        let stderr_buffer = Arc::new(Mutex::new(VecDeque::with_capacity(STDERR_CAPACITY)));
        let shared = Arc::clone(&stderr_buffer);
        let stderr_thread = match std::thread::Builder::new()
            .name("motioner-ffmpeg-stderr".to_owned())
            .spawn(move || drain_stderr(stderr, &shared))
        {
            Ok(handle) => handle,
            Err(err) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(ExportError::Ffmpeg {
                    message: format!("cannot start the ffmpeg stderr reader: {err}"),
                });
            }
        };

        Ok(Self {
            child: Some(child),
            stdin: Some(stdin),
            stderr_buffer,
            stderr_thread: Some(stderr_thread),
            expected_frame_len,
            frames: 0,
        })
    }

    /// Writes one tightly packed, top-row-first RGBA8 frame to `ffmpeg`.
    ///
    /// # Errors
    ///
    /// Returns [`ExportError::SizeMismatch`] when `rgba` is not exactly
    /// `width * height * 4` bytes, [`ExportError::DimensionsTooLarge`] when
    /// the configured dimensions overflow a `usize`, and
    /// [`ExportError::Encode`] when the frame cannot be written, including the
    /// last lines of `ffmpeg`'s stderr so a broken pipe explains itself.
    pub fn push_frame(&mut self, rgba: &[u8]) -> Result<(), ExportError> {
        validate_rgba_bytes(self.expected_frame_len, rgba)?;
        let Some(stdin) = self.stdin.as_mut() else {
            return Err(ExportError::Encode {
                message: "the encoder has already been finished".to_owned(),
            });
        };
        if let Err(err) = stdin.write_all(rgba) {
            let frames = self.frames;
            return Err(ExportError::Encode {
                message: format!(
                    "cannot write frame {frames} to ffmpeg: {err}; ffmpeg stderr: {}",
                    stderr_tail(&self.stderr_buffer)
                ),
            });
        }
        self.frames = self.frames.saturating_add(1);
        Ok(())
    }

    /// Closes the input pipe, waits for `ffmpeg`, and reports the result.
    ///
    /// # Errors
    ///
    /// Returns [`ExportError::Io`] when waiting for the child fails and
    /// [`ExportError::Ffmpeg`] when `ffmpeg` exits with a non-success status,
    /// including the last lines of its stderr.
    pub fn finish(mut self) -> Result<EncodeSummary, ExportError> {
        drop(self.stdin.take());
        let Some(mut child) = self.child.take() else {
            return Err(ExportError::Encode {
                message: "the encoder has already been finished".to_owned(),
            });
        };
        let status = match child.wait() {
            Ok(status) => status,
            Err(err) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(ExportError::Io {
                    message: format!("cannot wait for ffmpeg: {err}"),
                });
            }
        };
        join_thread(self.stderr_thread.take());
        if !status.success() {
            return Err(ExportError::Ffmpeg {
                message: format!(
                    "ffmpeg exited with {status}; ffmpeg stderr: {}",
                    stderr_tail(&self.stderr_buffer)
                ),
            });
        }
        Ok(EncodeSummary {
            frames: self.frames,
            status,
        })
    }
}

impl Drop for Mp4Encoder {
    fn drop(&mut self) {
        drop(self.stdin.take());
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        join_thread(self.stderr_thread.take());
    }
}

/// Checks a frame against a precomputed length without recomputing dimensions.
fn validate_rgba_bytes(expected: usize, rgba: &[u8]) -> Result<(), ExportError> {
    if rgba.len() == expected {
        Ok(())
    } else {
        Err(ExportError::SizeMismatch {
            expected,
            actual: rgba.len(),
        })
    }
}

/// Resolves the executable to launch for the given location.
pub(crate) fn resolve_program(location: &FfmpegLocation) -> OsString {
    match location {
        FfmpegLocation::Path(path) => path.as_os_str().to_owned(),
        FfmpegLocation::FromEnvOrPath => std::env::var_os(FFMPEG_ENV)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| OsString::from("ffmpeg")),
    }
}

/// Builds the typed error for a failed child spawn.
fn spawn_error(program: &OsStr, err: &std::io::Error) -> ExportError {
    if err.kind() == std::io::ErrorKind::NotFound {
        ExportError::FfmpegNotFound {
            message: format!(
                "executable '{}' was not found: install ffmpeg and put it on PATH, or set the {FFMPEG_ENV} environment variable to its full path",
                program.to_string_lossy()
            ),
        }
    } else {
        ExportError::Ffmpeg {
            message: format!("cannot start '{}': {err}", program.to_string_lossy()),
        }
    }
}

/// Reads `ffmpeg`'s stderr until end of stream into a bounded shared buffer.
fn drain_stderr(mut stderr: ChildStderr, buffer: &Mutex<VecDeque<u8>>) {
    let mut chunk = [0_u8; 1024];
    loop {
        match stderr.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => {
                let mut stored = lock_buffer(buffer);
                for &byte in chunk.iter().take(read) {
                    if stored.len() == STDERR_CAPACITY {
                        stored.pop_front();
                    }
                    stored.push_back(byte);
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => break,
        }
    }
}

/// Locks the stderr buffer, recovering the data if a reader panicked.
fn lock_buffer(buffer: &Mutex<VecDeque<u8>>) -> std::sync::MutexGuard<'_, VecDeque<u8>> {
    match buffer.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// Returns the buffered stderr as trimmed text, or a placeholder when empty.
fn stderr_tail(buffer: &Mutex<VecDeque<u8>>) -> String {
    let mut stored = lock_buffer(buffer);
    let text = String::from_utf8_lossy(stored.make_contiguous())
        .trim()
        .to_owned();
    if text.is_empty() {
        "no stderr output".to_owned()
    } else {
        text
    }
}

/// Joins an optional stderr thread, ignoring a panic in that thread.
fn join_thread(thread: Option<JoinHandle<()>>) {
    if let Some(handle) = thread {
        let _ = handle.join();
    }
}

/// Builds the complete `ffmpeg` argument list, including the output path.
#[cfg(test)]
fn ffmpeg_arguments(settings: &VideoSettings, output: &Path) -> Vec<OsString> {
    ffmpeg_arguments_with_audio(settings, output, None)
}

/// Builds the `ffmpeg` arguments with an optional WAV track muxed in.
fn ffmpeg_arguments_with_audio(
    settings: &VideoSettings,
    output: &Path,
    audio: Option<&Path>,
) -> Vec<OsString> {
    let mut args = vec![
        OsString::from("-y"),
        OsString::from("-loglevel"),
        OsString::from("error"),
        OsString::from("-f"),
        OsString::from("rawvideo"),
        OsString::from("-pix_fmt"),
        OsString::from("rgba"),
        OsString::from("-s"),
        OsString::from(format!("{}x{}", settings.width, settings.height)),
        OsString::from("-framerate"),
        OsString::from(format!(
            "{}/{}",
            settings.fps_numerator, settings.fps_denominator
        )),
        OsString::from("-i"),
        OsString::from("-"),
        OsString::from("-vf"),
        OsString::from("scale=out_color_matrix=bt709:out_range=tv"),
        OsString::from("-c:v"),
        OsString::from("libx264"),
        OsString::from("-preset"),
        OsString::from("medium"),
        OsString::from("-crf"),
        OsString::from(settings.crf.to_string()),
        OsString::from("-pix_fmt"),
        OsString::from("yuv420p"),
        OsString::from("-colorspace"),
        OsString::from("bt709"),
        OsString::from("-color_primaries"),
        OsString::from("bt709"),
        OsString::from("-color_trc"),
        OsString::from("bt709"),
        OsString::from("-movflags"),
        OsString::from("+faststart"),
    ];
    if let Some(audio) = audio {
        args.insert(13, OsString::from("-i"));
        args.insert(14, audio.as_os_str().to_owned());
        let after = args
            .iter()
            .position(|argument| argument == "-movflags")
            .unwrap_or(args.len());
        args.splice(
            after..after,
            [
                OsString::from("-c:a"),
                OsString::from("aac"),
                OsString::from("-b:a"),
                OsString::from("192k"),
                OsString::from("-shortest"),
            ],
        );
    }
    args.push(output.as_os_str().to_owned());
    args
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::Path;

    use super::{
        ffmpeg_arguments, ffmpeg_arguments_with_audio, validate_rgba_bytes, FfmpegLocation,
        Mp4Encoder, VideoSettings,
    };
    use crate::error::ExportError;

    #[test]
    fn arguments_match_the_documented_command() {
        let actual = ffmpeg_arguments(&VideoSettings::youtube_1080p(), Path::new("out.mp4"));
        let expected: Vec<OsString> = [
            "-y",
            "-loglevel",
            "error",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgba",
            "-s",
            "1920x1080",
            "-framerate",
            "60/1",
            "-i",
            "-",
            "-vf",
            "scale=out_color_matrix=bt709:out_range=tv",
            "-c:v",
            "libx264",
            "-preset",
            "medium",
            "-crf",
            "18",
            "-pix_fmt",
            "yuv420p",
            "-colorspace",
            "bt709",
            "-color_primaries",
            "bt709",
            "-color_trc",
            "bt709",
            "-movflags",
            "+faststart",
            "out.mp4",
        ]
        .into_iter()
        .map(OsString::from)
        .collect();
        assert_eq!(actual, expected);
    }

    #[test]
    fn an_audio_track_is_muxed_and_shortened_to_the_video() {
        let args = ffmpeg_arguments_with_audio(
            &VideoSettings::youtube_1080p(),
            Path::new("out.mp4"),
            Some(Path::new("mix.wav")),
        );
        let text: Vec<String> = args
            .iter()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect();
        assert!(text.windows(2).any(|pair| pair == ["-i", "mix.wav"]));
        assert!(text.windows(2).any(|pair| pair == ["-c:a", "aac"]));
        assert!(text.iter().any(|argument| argument == "-shortest"));
        assert_eq!(text.last().map(String::as_str), Some("out.mp4"));
    }

    #[test]
    fn missing_ffmpeg_is_reported_as_not_found() {
        let path = std::env::temp_dir().join("motioner-no-such-ffmpeg-12345.exe");
        let location = FfmpegLocation::Path(path);
        let result = Mp4Encoder::start(
            Path::new("out.mp4"),
            &VideoSettings::youtube_1080p(),
            &location,
        );
        match result {
            Err(ExportError::FfmpegNotFound { message }) => {
                assert!(message.contains("MOTIONER_FFMPEG"));
                assert!(message.contains("PATH"));
            }
            Err(other) => panic!("expected FfmpegNotFound, got {other:?}"),
            Ok(_) => panic!("expected FfmpegNotFound, got a running encoder"),
        }
    }

    #[test]
    fn frame_length_mismatch_is_reported() {
        let result = validate_rgba_bytes(64, &[0_u8; 12]);
        match result {
            Err(ExportError::SizeMismatch { expected, actual }) => {
                assert_eq!(expected, 64);
                assert_eq!(actual, 12);
            }
            other => panic!("expected SizeMismatch, got {other:?}"),
        }
    }
}

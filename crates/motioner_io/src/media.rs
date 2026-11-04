//! Media helpers that call the external `ffmpeg` binary.
//!
//! Motioner keeps ffmpeg outside the dependency graph (D-0035 for images;
//! audio and video decode in later phases). Nothing here is on the render hot
//! path: conversions and peaks are produced once and cached in `cache/`.

use std::fmt;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::encode::{resolve_program, FfmpegLocation};
use crate::error::ExportError;

/// Peaks of a decoded audio track, for waveform strips.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioPeaks {
    /// Duration in seconds.
    pub duration: f64,
    /// Peak amplitude per bucket in `0.0..=1.0`, left to right.
    pub peaks: Vec<f32>,
}

/// Decodes `source` to mono `f32` at 8 kHz and returns one peak per bucket.
///
/// # Errors
///
/// [`ExportError::FfmpegNotFound`] when no binary is available and
/// [`ExportError::Ffmpeg`] when decoding fails.
pub fn audio_peaks(
    source: &Path,
    buckets: usize,
    location: &FfmpegLocation,
) -> Result<AudioPeaks, ExportError> {
    let program = resolve_program(location);
    let output = Command::new(&program)
        .args([
            "-v",
            "error",
            "-i",
            &source.to_string_lossy(),
            "-ac",
            "1",
            "-ar",
            "8000",
            "-f",
            "f32le",
            "-",
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .output()
        .map_err(|error| spawn_error(&program, &error))?;
    if !output.status.success() {
        return Err(ExportError::Ffmpeg {
            message: String::from_utf8_lossy(&output.stderr)
                .lines()
                .next_back()
                .unwrap_or("ffmpeg failed")
                .to_owned(),
        });
    }
    let samples: Vec<f32> = output
        .stdout
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect();
    if samples.is_empty() {
        return Err(ExportError::Ffmpeg {
            message: "the audio stream decoded to no samples".to_owned(),
        });
    }
    #[allow(clippy::cast_precision_loss)] // duration display only
    let duration = samples.len() as f64 / 8000.0;
    let buckets = buckets.max(1);
    let mut peaks = Vec::with_capacity(buckets);
    for bucket in 0..buckets {
        let start = bucket * samples.len() / buckets;
        let end = ((bucket + 1) * samples.len() / buckets).max(start + 1);
        let peak = samples[start..end.min(samples.len())]
            .iter()
            .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
        peaks.push(peak.min(1.0));
    }
    Ok(AudioPeaks { duration, peaks })
}

/// Encodes peaks as the cache text: duration on the first line, one peak per
/// line after it.
#[must_use]
pub fn peaks_to_text(peaks: &AudioPeaks) -> String {
    let mut text = format!("{:.6}\n", peaks.duration);
    for peak in &peaks.peaks {
        use std::fmt::Write as _;
        let _ = writeln!(text, "{peak:.6}");
    }
    text
}

/// Parses [`peaks_to_text`] output.
#[must_use]
pub fn peaks_from_text(text: &str) -> Option<AudioPeaks> {
    let mut lines = text.lines();
    let duration = lines.next()?.parse::<f64>().ok()?;
    let peaks: Vec<f32> = lines.filter_map(|line| line.parse::<f32>().ok()).collect();
    (!peaks.is_empty()).then_some(AudioPeaks { duration, peaks })
}

/// Converts the first frame of an image or video into a PNG.
///
/// # Errors
///
/// [`ExportError::Ffmpeg`] when the conversion fails.
pub fn first_frame_png(
    source: &Path,
    destination: &Path,
    location: &FfmpegLocation,
) -> Result<(), ExportError> {
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(|error| ExportError::Io {
            message: format!("cannot create {}: {error}", parent.display()),
        })?;
    }
    let program = resolve_program(location);
    let output = Command::new(&program)
        .args([
            "-v",
            "error",
            "-y",
            "-i",
            &source.to_string_lossy(),
            "-frames:v",
            "1",
            &destination.to_string_lossy(),
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .stdout(Stdio::null())
        .output()
        .map_err(|error| spawn_error(&program, &error))?;
    if !output.status.success() {
        return Err(ExportError::Ffmpeg {
            message: String::from_utf8_lossy(&output.stderr)
                .lines()
                .next_back()
                .unwrap_or("ffmpeg failed")
                .to_owned(),
        });
    }
    Ok(())
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

/// Formats a byte count for the assets panel.
#[must_use]
#[allow(clippy::cast_precision_loss)] // display only
pub fn format_bytes(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = 1024 * KIB;
    const GIB: u64 = 1024 * MIB;
    if bytes >= GIB {
        format!("{:.1} GB", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.1} MB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.0} KB", bytes as f64 / KIB as f64)
    } else {
        format!("{bytes} B")
    }
}

impl fmt::Display for AudioPeaks {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.2}s, {} buckets", self.duration, self.peaks.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peaks_round_trip() {
        let peaks = AudioPeaks {
            duration: 1.5,
            peaks: vec![0.1, 0.5, 1.0],
        };
        let text = peaks_to_text(&peaks);
        assert_eq!(peaks_from_text(&text), Some(peaks));
        assert_eq!(peaks_from_text(""), None);
    }

    #[test]
    fn byte_sizes_are_readable() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(2048), "2 KB");
        assert_eq!(format_bytes(5 * 1024 * 1024), "5.0 MB");
    }
}

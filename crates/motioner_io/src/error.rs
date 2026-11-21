//! Typed errors for PNG and video export.

use std::fmt;

/// An error produced while writing a still image or encoding a video.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportError {
    /// The supplied pixel buffer does not match the requested dimensions.
    SizeMismatch {
        /// Number of bytes required for the requested width and height.
        expected: usize,
        /// Number of bytes actually supplied.
        actual: usize,
    },
    /// The requested dimensions cannot be expressed as a frame byte count.
    DimensionsTooLarge {
        /// Requested width in pixels.
        width: u32,
        /// Requested height in pixels.
        height: u32,
    },
    /// A filesystem operation failed.
    Io {
        /// Description of the failure, including the underlying OS error.
        message: String,
    },
    /// The PNG encoder rejected the image or failed while writing it.
    Png {
        /// Description of the failure, including the underlying encoder error.
        message: String,
    },
    /// No `ffmpeg` executable could be found.
    FfmpegNotFound {
        /// Actionable description naming the ways to provide `ffmpeg`.
        message: String,
    },
    /// `ffmpeg` was found but could not be started or did not exit cleanly.
    Ffmpeg {
        /// Description of the failure, including the last lines of stderr.
        message: String,
    },
    /// Writing frame data to the `ffmpeg` process failed.
    Encode {
        /// Description of the failure, including the last lines of stderr.
        message: String,
    },
}

impl fmt::Display for ExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SizeMismatch { expected, actual } => write!(
                f,
                "frame buffer has {actual} bytes but the dimensions require {expected}"
            ),
            Self::DimensionsTooLarge { width, height } => write!(
                f,
                "frame dimensions {width}x{height} are too large to address as bytes"
            ),
            Self::Io { message } => write!(f, "I/O error: {message}"),
            Self::Png { message } => write!(f, "PNG error: {message}"),
            Self::FfmpegNotFound { message } => write!(f, "ffmpeg not found: {message}"),
            Self::Ffmpeg { message } => write!(f, "ffmpeg error: {message}"),
            Self::Encode { message } => write!(f, "encode error: {message}"),
        }
    }
}

impl std::error::Error for ExportError {}

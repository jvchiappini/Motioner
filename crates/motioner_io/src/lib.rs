//! Project and media I/O for Motioner.
//!
//! Owns the native Motioner Document Format (MDF), the content-addressed
//! asset store, project migrations, autosave and the ffmpeg export
//! pipeline. MDF is shared with the code layer; this crate does not
//! define a second serialization format.
//!
//! This crate's export path operates on plain RGBA8 bytes and never depends
//! on the renderer. [`write_png`] stores a single still as an 8-bit sRGB PNG,
//! and [`Mp4Encoder`] streams frames to `ffmpeg` for H.264 MP4 output.

mod encode;
mod error;
mod fonts;
mod frame;
mod images;
mod png_writer;

pub use encode::{EncodeSummary, FfmpegLocation, Mp4Encoder, VideoSettings};
pub use error::ExportError;
pub use fonts::{load_project_fonts, FontLoadError, ProjectFont};
pub use images::{CachedImages, DecodedImage, ImageError, ImageStore};
pub use png_writer::write_png;

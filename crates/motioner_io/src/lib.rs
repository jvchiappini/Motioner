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

mod assets;
mod audio;
mod backups;
mod dates;
mod encode;
mod error;
mod fonts;
mod frame;
mod images;
mod media;
mod png_writer;
mod project;
mod traces;
mod video;

pub use assets::{
    converted_image, image_key_from_hex, project_fonts, safe_relative, sha256_hex, valid_key,
    AssetError, ImportOutcome, ProjectImages,
};
pub use audio::{
    decode_to_cache, mix_document, write_wav_i16, AudioPcm, AudioStore, CHANNELS, SAMPLE_RATE,
};
pub use backups::{BackupError, BackupInfo, BackupPolicy, STALE_LOCK};
pub use dates::{full_local_date, relative_time};
pub use encode::{EncodeSummary, FfmpegLocation, Mp4Encoder, VideoSettings};
pub use error::ExportError;
pub use fonts::{font_from_original, load_project_fonts, FontLoadError, ProjectFont};
pub use images::{CachedImages, DecodedImage, ImageError, ImageStore};
pub use media::{
    audio_peaks, first_frame_png, format_bytes, peaks_from_text, peaks_to_text, AudioPeaks,
};
pub use png_writer::write_png;
pub use project::{
    default_program, format_utc, format_utc_seconds, normalize_project_dir, path_text,
    project_root, write_atomic, write_atomic_bytes, AppState, AssetEntry, AssetKind,
    CompositionRef, EditorState, Project, ProjectError, ProjectOptions, ProjectSettings,
    RecentProject, WindowPlacement, ASSETS_DIR, BACKUPS_DIR, CACHE_DIR, COMPOSITIONS_DIR,
    EDITOR_FILE, FORMAT_VERSION, PROJECT_EXTENSION, PROJECT_FILE, TRACES_DIR,
};
pub use traces::{append as append_trace, read_all as read_traces, TraceRecord, TRACE_VERSION};
pub use video::{
    decode_frame, decode_frames, make_proxy, probe as probe_video, FrameStream, VideoInfo,
};

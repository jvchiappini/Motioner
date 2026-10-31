//! Project and media I/O for Motioner.
//!
//! Owns the native Motioner Document Format (MDF), the content-addressed
//! asset store, project migrations, autosave and the ffmpeg export
//! pipeline. MDF is shared with the code layer; this crate does not
//! define a second serialization format.

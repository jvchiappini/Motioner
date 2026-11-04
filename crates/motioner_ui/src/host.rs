//! The application hooks the window needs but cannot own (D-0045).
//!
//! `motioner_ui` must not depend on `motioner_io` (the boundary check), so
//! project folders, the native folder picker, the asset registry and the
//! ffmpeg pipeline are reached through this trait. `motioner_app` implements
//! it over `motioner_io`; headless tests can supply a fake.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use motioner_core::ImageResolver;
use motioner_render::FontSet;

use crate::assets::AssetSummary;
use crate::start::{NewProjectRequest, StartEntry};

/// A backup newer than the files on disk, offered after a crash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryInfo {
    /// Backup folder name.
    pub name: String,
    /// Human summary (files and size).
    pub detail: String,
}

/// Project-level services for the window.
pub trait ProjectHost {
    /// Recent projects, most recent first.
    fn recents(&self) -> Vec<StartEntry>;

    /// Shipped example programs.
    fn examples(&self) -> Vec<StartEntry>;

    /// Opens the native folder picker; `None` when the user cancelled.
    fn pick_folder(&self, title: &str, start: Option<&Path>) -> Option<PathBuf>;

    /// Creates the project described by `request` and returns its root.
    ///
    /// # Errors
    ///
    /// A human-readable message shown in the New project dialog.
    fn create_project(&self, request: &NewProjectRequest) -> Result<PathBuf, String>;

    /// Resolves a folder, manifest or composition to the composition file to
    /// open.
    ///
    /// # Errors
    ///
    /// A human-readable message when the path is not a project or program.
    fn open_project(&self, path: &Path) -> Result<PathBuf, String>;

    /// Writes the project manifest and every pending editor file.
    ///
    /// # Errors
    ///
    /// A human-readable I/O message.
    fn save_project(&self, composition: &Path) -> Result<PathBuf, String>;

    /// Copies the project to `destination` and returns the copy's
    /// composition file.
    ///
    /// # Errors
    ///
    /// A human-readable I/O message.
    fn save_as(&self, composition: &Path, destination: &Path) -> Result<PathBuf, String>;

    /// Forgets one recent project.
    fn forget_recent(&self, path: &Path);

    /// Opens the project folder in the file manager.
    fn reveal(&self, composition: &Path);

    /// Fonts available to a composition (bundled plus project faces).
    fn fonts(&self, composition: &Path) -> FontSet;

    /// Image resolver for a composition.
    fn images(&self, composition: &Path) -> Rc<dyn ImageResolver>;

    /// The asset registry as the panel shows it, sorted by key.
    fn assets(&self, composition: &Path) -> Vec<AssetSummary>;

    /// Imports one file and returns its registry entry.
    ///
    /// # Errors
    ///
    /// A human-readable message for an unsupported type or an I/O failure.
    fn import_asset(&self, composition: &Path, source: &Path) -> Result<AssetSummary, String>;

    /// Removes an asset and its blob.
    ///
    /// # Errors
    ///
    /// A human-readable message when the key is unknown or the write fails.
    fn remove_asset(&self, composition: &Path, key: &str) -> Result<(), String>;

    /// Renames an asset key.
    ///
    /// # Errors
    ///
    /// A human-readable message when the key is unknown or the write fails.
    fn rename_asset(&self, composition: &Path, key: &str, name: &str) -> Result<(), String>;

    /// Appends the layer for an asset at `pos` (composition pixels) and/or
    /// `frame`.
    ///
    /// # Errors
    ///
    /// A human-readable message for a kind whose layer does not exist yet or
    /// an I/O failure.
    fn place_asset(
        &self,
        composition: &Path,
        key: &str,
        frame: Option<i64>,
        pos: Option<(f64, f64)>,
    ) -> Result<(), String>;

    /// Keys that no composition references.
    ///
    /// # Errors
    ///
    /// A human-readable message when the project cannot be read.
    fn unused_assets(&self, composition: &Path) -> Result<Vec<String>, String>;

    /// Removes every unreferenced asset, returning how many were removed.
    ///
    /// # Errors
    ///
    /// A human-readable message when a blob cannot be removed.
    fn remove_unused_assets(&self, composition: &Path) -> Result<usize, String>;

    /// Opens the native file picker for one or more media files.
    fn pick_files(&self, title: &str) -> Vec<PathBuf>;

    /// Peak buckets for an audio asset (cached in `cache/`).
    fn waveform(&self, composition: &Path, key: &str, buckets: usize) -> Vec<f32>;

    /// Marks the project as open for this session (lock + heartbeat).
    fn begin_session(&self, composition: &Path);

    /// Removes the session lock after a clean close.
    fn end_session(&self, composition: &Path);

    /// Refreshes the session heartbeat.
    fn heartbeat(&self, composition: &Path);

    /// Writes one snapshot into `backups/` and returns its folder name.
    ///
    /// # Errors
    ///
    /// A human-readable I/O message.
    fn snapshot(&self, composition: &Path) -> Result<String, String>;

    /// A newer backup offered after a crash, if any.
    fn recovery(&self, composition: &Path) -> Option<RecoveryInfo>;

    /// Restores the newest offered backup.
    ///
    /// # Errors
    ///
    /// A human-readable I/O message.
    fn restore(&self, composition: &Path) -> Result<(), String>;

    /// Starts audio playback at `from`; `muted` lists the identities of audio
    /// layers muted by the editor (hidden layers or the solo state).
    ///
    /// # Errors
    ///
    /// A human-readable message when no output device is available.
    fn audio_play(
        &self,
        composition: &Path,
        document: &motioner_core::Document,
        from: motioner_core::Frame,
        muted: &[String],
    ) -> Result<(), String> {
        let _ = (composition, document, from, muted);
        Ok(())
    }

    /// Stops audio playback.
    fn audio_stop(&self) {}

    /// The audio clock in seconds since the composition start, when playing.
    fn audio_position(&self) -> Option<f64> {
        None
    }

    /// Plays a short grain for a scrub, when the host supports it.
    fn audio_scrub(
        &self,
        composition: &Path,
        document: &motioner_core::Document,
        from: motioner_core::Frame,
        muted: &[String],
    ) {
        let _ = (composition, document, from, muted);
    }
}

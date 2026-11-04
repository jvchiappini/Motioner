//! Rolling project backups and crash recovery (D-0045, prompt 11 Phase 4).
//!
//! Every snapshot copies the authoritative files (`project.motion`,
//! `editor.motion`, `compositions/*`) into
//! `backups/<UTC timestamp>/` with the same relative layout. Assets are
//! immutable blobs and are never copied. Snapshots are bounded by count and
//! byte size, oldest first, so a long session cannot fill the disk.
//!
//! A lock file in `cache/session.lock` records the owning PID and a heartbeat
//! timestamp. A lock whose heartbeat is older than [`STALE_LOCK`] means the
//! previous session crashed; [`Project::pending_recovery`] then offers the
//! newest snapshot newer than the manifest.

use std::fmt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::project::{format_utc_seconds, write_atomic, Project, ProjectError};

/// Minimum age of a lock heartbeat before it is considered abandoned.
pub const STALE_LOCK: Duration = Duration::from_mins(2);

/// Snapshot cadence and retention.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackupPolicy {
    /// Seconds between automatic snapshots while a project is open.
    pub interval_seconds: u64,
    /// Maximum number of backup folders kept.
    pub max_backups: usize,
    /// Maximum total bytes kept under `backups/`.
    pub max_bytes: u64,
}

impl Default for BackupPolicy {
    fn default() -> Self {
        Self {
            interval_seconds: 120,
            max_backups: 20,
            max_bytes: 200 * 1024 * 1024,
        }
    }
}

/// One backup folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupInfo {
    /// Folder name (`2026-11-04T12-00-00Z`).
    pub name: String,
    /// Absolute folder.
    pub path: PathBuf,
    /// Files in the snapshot.
    pub files: usize,
    /// Total bytes copied.
    pub bytes: u64,
}

/// Why a backup or restore failed.
#[derive(Debug)]
pub enum BackupError {
    /// A file operation failed.
    Io {
        /// The path involved.
        path: PathBuf,
        /// Underlying message.
        message: String,
    },
    /// The snapshot has no manifest to restore.
    Invalid {
        /// The folder.
        path: PathBuf,
        /// What is missing or invalid.
        message: String,
    },
}

impl fmt::Display for BackupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, message } | Self::Invalid { path, message } => {
                write!(f, "{}: {message}", path.display())
            }
        }
    }
}

impl std::error::Error for BackupError {}

fn io(path: &Path, error: &std::io::Error) -> BackupError {
    BackupError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    }
}

impl Project {
    /// Copies the authoritative files into a new snapshot folder.
    ///
    /// # Errors
    ///
    /// [`BackupError`] for an unreadable file or an unwritable backup folder.
    pub fn snapshot(&self, now: SystemTime) -> Result<BackupInfo, BackupError> {
        self.snapshot_with_policy(now, &BackupPolicy::default())
    }

    /// [`Self::snapshot`] followed by retention pruning.
    ///
    /// # Errors
    ///
    /// Same as [`Self::snapshot`].
    pub fn snapshot_with_policy(
        &self,
        now: SystemTime,
        policy: &BackupPolicy,
    ) -> Result<BackupInfo, BackupError> {
        let seconds = now
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_secs());
        let mut name = format_utc_seconds(seconds).replace(':', "-");
        let mut path = self.backups_dir().join(&name);
        let mut suffix = 2;
        while path.exists() {
            name = format!("{}-{suffix}", format_utc_seconds(seconds).replace(':', "-"));
            path = self.backups_dir().join(&name);
            suffix += 1;
        }
        std::fs::create_dir_all(&path).map_err(|error| io(&path, &error))?;
        let mut files = 0_usize;
        let mut bytes = 0_u64;
        for relative in authoritative_files(self)? {
            let source = self.root().join(&relative);
            let destination = path.join(&relative);
            if let Some(parent) = destination.parent() {
                std::fs::create_dir_all(parent).map_err(|error| io(parent, &error))?;
            }
            std::fs::copy(&source, &destination).map_err(|error| io(&source, &error))?;
            bytes += std::fs::metadata(&destination).map_or(0, |metadata| metadata.len());
            files += 1;
        }
        prune(self, policy)?;
        Ok(BackupInfo {
            name,
            path,
            files,
            bytes,
        })
    }

    /// Snapshots in newest-first order.
    ///
    /// # Errors
    ///
    /// [`BackupError::Io`] when the backup folder cannot be read.
    pub fn backups(&self) -> Result<Vec<BackupInfo>, BackupError> {
        let dir = self.backups_dir();
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(io(&dir, &error)),
        };
        let mut backups = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let (files, bytes) = folder_size(&path);
            backups.push(BackupInfo {
                name: name.to_owned(),
                path,
                files,
                bytes,
            });
        }
        backups.sort_by(|left, right| right.name.cmp(&left.name));
        Ok(backups)
    }

    /// The newest snapshot newer than the manifest, when the last session did
    /// not close cleanly.
    ///
    /// # Errors
    ///
    /// [`BackupError::Io`] when the backup folder cannot be read.
    pub fn pending_recovery(&self) -> Result<Option<BackupInfo>, BackupError> {
        if !self.lock_is_stale()? {
            return Ok(None);
        }
        let manifest = self.manifest_path();
        let modified = std::fs::metadata(&manifest)
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .unwrap_or(UNIX_EPOCH);
        for backup in self.backups()? {
            let when = std::fs::metadata(&backup.path)
                .ok()
                .and_then(|metadata| metadata.modified().ok())
                .unwrap_or(UNIX_EPOCH);
            if when > modified && backup.path.join("project.motion").is_file() {
                return Ok(Some(backup));
            }
        }
        Ok(None)
    }

    /// Restores `backup` over the authoritative files.
    ///
    /// # Errors
    ///
    /// [`BackupError::Invalid`] when the snapshot has no manifest and
    /// [`BackupError::Io`] when a file cannot be copied.
    pub fn restore(&self, backup: &BackupInfo) -> Result<(), BackupError> {
        let manifest = backup.path.join("project.motion");
        if !manifest.is_file() {
            return Err(BackupError::Invalid {
                path: backup.path.clone(),
                message: "the snapshot has no project.motion".to_owned(),
            });
        }
        let (files, _) = folder_size(&backup.path);
        if files == 0 {
            return Err(BackupError::Invalid {
                path: backup.path.clone(),
                message: "the snapshot is empty".to_owned(),
            });
        }
        copy_dir(
            &backup.path,
            self.root(),
            &["project.motion", "editor.motion", "compositions"],
        )?;
        Ok(())
    }

    /// Writes the session lock with this process id and a fresh heartbeat.
    ///
    /// # Errors
    ///
    /// [`BackupError::Io`] when `cache/` cannot be written.
    pub fn begin_session(&self) -> Result<(), BackupError> {
        let path = self.lock_path();
        let text = format!("pid={}\nheartbeat={}\n", std::process::id(), now_seconds());
        write_atomic(&path, &text).map_err(|error| BackupError::Io {
            path,
            message: error.to_string(),
        })
    }

    /// Refreshes the heartbeat of an open session.
    ///
    /// # Errors
    ///
    /// [`BackupError::Io`] when `cache/` cannot be written.
    pub fn heartbeat(&self) -> Result<(), BackupError> {
        self.begin_session()
    }

    /// Removes the session lock after a clean close.
    pub fn end_session(&self) {
        let _ = std::fs::remove_file(self.lock_path());
    }

    /// The session lock path (`cache/session.lock`).
    #[must_use]
    pub fn lock_path(&self) -> PathBuf {
        self.cache_dir().join("session.lock")
    }

    /// Whether a lock exists and its heartbeat is older than [`STALE_LOCK`].
    ///
    /// # Errors
    ///
    /// [`BackupError::Io`] for an unreadable lock file.
    pub fn lock_is_stale(&self) -> Result<bool, BackupError> {
        let path = self.lock_path();
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(io(&path, &error)),
        };
        let heartbeat = text
            .lines()
            .find_map(|line| line.strip_prefix("heartbeat="))
            .and_then(|value| value.trim().parse::<u64>().ok())
            .unwrap_or(0);
        Ok(now_seconds().saturating_sub(heartbeat) > STALE_LOCK.as_secs())
    }
}

fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

/// Every authoritative file, relative to the project root.
fn authoritative_files(project: &Project) -> Result<Vec<PathBuf>, BackupError> {
    let mut files = vec![
        PathBuf::from(crate::project::PROJECT_FILE),
        PathBuf::from(crate::project::EDITOR_FILE),
    ];
    let compositions = project.compositions_dir();
    let entries = match std::fs::read_dir(&compositions) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(files),
        Err(error) => return Err(io(&compositions, &error)),
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) == Some("motion") {
            if let Ok(relative) = path.strip_prefix(project.root()) {
                files.push(relative.to_path_buf());
            }
        }
    }
    files.sort();
    Ok(files)
}

/// Removes the oldest snapshots until the count and size fit `policy`.
fn prune(project: &Project, policy: &BackupPolicy) -> Result<(), BackupError> {
    let mut backups = project.backups()?;
    let mut total: u64 = backups.iter().map(|backup| backup.bytes).sum();
    while backups.len() > policy.max_backups || total > policy.max_bytes {
        let Some(oldest) = backups.pop() else {
            break;
        };
        std::fs::remove_dir_all(&oldest.path).map_err(|error| io(&oldest.path, &error))?;
        total = total.saturating_sub(oldest.bytes);
    }
    Ok(())
}

/// Number of files and total bytes under `dir`, recursively.
fn folder_size(dir: &Path) -> (usize, u64) {
    let mut files = 0;
    let mut bytes = 0;
    let Ok(entries) = std::fs::read_dir(dir) else {
        return (0, 0);
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let (nested_files, nested_bytes) = folder_size(&path);
            files += nested_files;
            bytes += nested_bytes;
        } else {
            files += 1;
            bytes += std::fs::metadata(&path).map_or(0, |metadata| metadata.len());
        }
    }
    (files, bytes)
}

/// Copies the named top-level entries of `from` into `to`.
fn copy_dir(from: &Path, to: &Path, names: &[&str]) -> Result<(), BackupError> {
    for name in names {
        let source = from.join(name);
        if !source.exists() {
            continue;
        }
        let destination = to.join(name);
        if source.is_dir() {
            copy_tree(&source, &destination)?;
        } else {
            copy_file(&source, &destination)?;
        }
    }
    Ok(())
}

fn copy_tree(from: &Path, to: &Path) -> Result<(), BackupError> {
    std::fs::create_dir_all(to).map_err(|error| io(to, &error))?;
    for entry in std::fs::read_dir(from).map_err(|error| io(from, &error))? {
        let entry = entry.map_err(|error| io(from, &error))?;
        let source = entry.path();
        let destination = to.join(entry.file_name());
        if source.is_dir() {
            copy_tree(&source, &destination)?;
        } else {
            copy_file(&source, &destination)?;
        }
    }
    Ok(())
}

fn copy_file(from: &Path, to: &Path) -> Result<(), BackupError> {
    let bytes = std::fs::read(from).map_err(|error| io(from, &error))?;
    write_atomic_bytes(to, &bytes).map_err(|error| BackupError::Io {
        path: to.to_path_buf(),
        message: error.to_string(),
    })
}

fn write_atomic_bytes(path: &Path, bytes: &[u8]) -> Result<(), ProjectError> {
    crate::project::write_atomic_bytes(path, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{ProjectOptions, ProjectSettings};

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("motioner_backup_{}_{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_snapshot_copies_the_authoritative_files_and_prunes() {
        let dir = temp_dir("snapshot");
        let project = Project::create(&dir, &ProjectOptions::new("demo")).unwrap();
        let policy = BackupPolicy {
            max_backups: 2,
            ..BackupPolicy::default()
        };
        for seconds in 0..4 {
            let info = project
                .snapshot_with_policy(UNIX_EPOCH + Duration::from_secs(seconds), &policy)
                .unwrap();
            assert!(info.path.join("project.motion").is_file());
            assert!(info.path.join("compositions/main.motion").is_file());
            assert_eq!(info.files, 3);
        }
        let backups = project.backups().unwrap();
        assert_eq!(backups.len(), 2);
        assert!(backups[0].name > backups[1].name);
        let _ = std::fs::remove_dir_all(project.root());
    }

    #[test]
    fn restore_puts_the_snapshot_back() {
        let dir = temp_dir("restore");
        let project = Project::create(&dir, &ProjectOptions::new("demo")).unwrap();
        let info = project.snapshot(UNIX_EPOCH).unwrap();
        let program = project.composition_path("main");
        std::fs::write(&program, "broken(\\n").unwrap();
        project.restore(&info).unwrap();
        assert!(std::fs::read_to_string(&program).unwrap().contains("comp("));
        let _ = std::fs::remove_dir_all(project.root());
    }

    #[test]
    fn a_stale_lock_offers_the_newer_snapshot() {
        let dir = temp_dir("recovery");
        let project = Project::create(&dir, &ProjectOptions::new("demo")).unwrap();
        let lock = project.lock_path();
        std::fs::create_dir_all(project.cache_dir()).unwrap();
        std::fs::write(&lock, "pid=1\\nheartbeat=0\\n").unwrap();
        let info = project.snapshot(SystemTime::now()).unwrap();
        // A snapshot is newer than the manifest only if we wait a moment on
        // coarse filesystems; touch the manifest back instead.
        let old = SystemTime::UNIX_EPOCH + Duration::from_secs(1);
        let _ = filetime_set(&project.manifest_path(), old);
        assert!(project.lock_is_stale().unwrap());
        let pending = project.pending_recovery().unwrap();
        assert!(pending.is_some(), "snapshot {info:?}");
        project.end_session();
        let _ = std::fs::remove_dir_all(project.root());
    }

    /// Sets a file's modified time on Windows/Linux through the standard
    /// library where possible; ignored when the platform refuses.
    fn filetime_set(path: &Path, time: SystemTime) -> std::io::Result<()> {
        let file = std::fs::OpenOptions::new().write(true).open(path)?;
        file.set_modified(time)
    }

    #[test]
    fn legacy_settings_keep_the_default_policy() {
        let settings = ProjectSettings::new("x");
        assert_eq!(settings.main, "main");
        assert_eq!(BackupPolicy::default().max_backups, 20);
    }
}

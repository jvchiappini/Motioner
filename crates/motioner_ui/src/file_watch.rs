//! Polling file watcher for hot reload.
//!
//! The watcher stats the source file every 250 ms and reports a change once.
//! A short debounce (150 ms) collapses an editor's save-in-two-steps into one
//! reload: the change is only reported after the stamp has been stable for the
//! debounce window. No platform watcher or extra dependency is involved.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

/// Default poll interval.
const POLL_INTERVAL: Duration = Duration::from_millis(250);
/// Default debounce window.
const DEBOUNCE: Duration = Duration::from_millis(150);

/// The parts of file metadata that matter to a reload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Stamp {
    modified: Option<SystemTime>,
    len: u64,
}

fn read_stamp(path: &Path) -> Option<Stamp> {
    let metadata = fs::metadata(path).ok()?;
    Some(Stamp {
        modified: metadata.modified().ok(),
        len: metadata.len(),
    })
}

/// Polls one file and reports stable changes once.
#[derive(Debug)]
pub struct FileWatch {
    path: PathBuf,
    poll_interval: Duration,
    debounce: Duration,
    next_poll: Option<Instant>,
    observed: Option<Stamp>,
    candidate: Option<(Option<Stamp>, Instant)>,
}

impl FileWatch {
    /// Watches `path`, taking the current stamp as the baseline.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self::with_intervals(path, POLL_INTERVAL, DEBOUNCE)
    }

    /// Watches `path` with explicit timings (used by tests).
    #[must_use]
    pub(crate) fn with_intervals(
        path: impl Into<PathBuf>,
        poll_interval: Duration,
        debounce: Duration,
    ) -> Self {
        let path = path.into();
        let observed = read_stamp(&path);
        Self {
            path,
            poll_interval,
            debounce,
            next_poll: None,
            observed,
            candidate: None,
        }
    }

    /// The watched path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Takes the current file state as the new baseline, forgetting any
    /// pending change.
    ///
    /// The editor calls this after writing a file itself, so its own write
    /// never looks like an external change.
    pub fn reset(&mut self) {
        self.observed = read_stamp(&self.path);
        self.candidate = None;
        self.next_poll = None;
    }

    /// Returns `true` exactly once per stable change.
    ///
    /// Call it every frame with the current instant; it stats the file only
    /// when the poll interval elapsed, and checks the debounce on every call.
    pub fn check(&mut self, now: Instant) -> bool {
        if self.next_poll.is_none_or(|deadline| now >= deadline) {
            self.next_poll = Some(now + self.poll_interval);
            let current = read_stamp(&self.path);
            if current == self.observed {
                self.candidate = None;
            } else {
                let already_candidate = self
                    .candidate
                    .as_ref()
                    .is_some_and(|(stamp, _)| *stamp == current);
                if !already_candidate {
                    self.candidate = Some((current, now));
                }
            }
        }

        let Some((stamp, since)) = self.candidate.take() else {
            return false;
        };
        if now.saturating_duration_since(since) >= self.debounce {
            self.observed = stamp;
            true
        } else {
            self.candidate = Some((stamp, since));
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const POLL: Duration = Duration::from_millis(250);
    const DEBOUNCE: Duration = Duration::from_millis(150);

    struct TempFile {
        path: PathBuf,
    }

    impl TempFile {
        fn new(name: &str, contents: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "motioner_ui_watch_{}_{name}.motion",
                std::process::id()
            ));
            fs::write(&path, contents).unwrap();
            Self { path }
        }

        fn write(&self, contents: &str) {
            fs::write(&self.path, contents).unwrap();
        }
    }

    impl Drop for TempFile {
        fn drop(&mut self) {
            fs::remove_file(&self.path).ok();
        }
    }

    #[test]
    fn an_unchanged_file_never_reports() {
        let file = TempFile::new("unchanged", "comp(name=\"A\", duration=1.0)\n");
        let mut watch = FileWatch::with_intervals(&file.path, POLL, DEBOUNCE);
        let start = Instant::now();
        for step in 0..20 {
            assert!(!watch.check(start + Duration::from_millis(step * 100)));
        }
    }

    #[test]
    fn a_change_reports_once_after_the_debounce() {
        let file = TempFile::new("once", "a");
        let mut watch = FileWatch::with_intervals(&file.path, POLL, DEBOUNCE);
        let start = Instant::now();
        assert!(!watch.check(start));

        file.write("bb");
        assert!(!watch.check(start + Duration::from_millis(300)));
        assert!(!watch.check(start + Duration::from_millis(400)));
        assert!(
            watch.check(start + Duration::from_millis(500)),
            "stable change reported after the debounce"
        );
        assert!(
            !watch.check(start + Duration::from_millis(800)),
            "reported exactly once"
        );
    }

    #[test]
    fn two_saves_in_a_row_trigger_one_reload() {
        let file = TempFile::new("two_steps", "a");
        let mut watch = FileWatch::with_intervals(&file.path, POLL, DEBOUNCE);
        let start = Instant::now();
        assert!(!watch.check(start));

        file.write("bb");
        assert!(!watch.check(start + Duration::from_millis(300)));
        file.write("ccc");
        assert!(!watch.check(start + Duration::from_millis(600)));
        assert!(
            watch.check(start + Duration::from_millis(800)),
            "the second write resets the debounce and reports once"
        );
        assert!(!watch.check(start + Duration::from_millis(1200)));
    }

    #[test]
    fn deleting_the_file_counts_as_a_change() {
        let file = TempFile::new("deleted", "a");
        let mut watch = FileWatch::with_intervals(&file.path, POLL, DEBOUNCE);
        let start = Instant::now();
        assert!(!watch.check(start));
        fs::remove_file(&file.path).unwrap();
        assert!(!watch.check(start + Duration::from_millis(300)));
        assert!(watch.check(start + Duration::from_millis(500)));
    }
}

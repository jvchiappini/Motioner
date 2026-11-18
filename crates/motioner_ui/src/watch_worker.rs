//! Background file watcher for hot reload (prompt 18 P1).
//!
//! The window used to stat the program file from its frame loop, which only
//! worked because the loop ran at 60 fps. With on-demand redraw an external
//! edit must wake the loop instead, so the stat moved to a worker that sleeps
//! between checks and calls [`Waker::wake`] when the file changed.
//!
//! The worker only notifies: the window still runs
//! [`PreviewSession::poll_changes`](crate::PreviewSession::poll_changes) on
//! the frame the wake produces, which applies the debounce and ignores the
//! editor's own writes. The watcher's `FileWatch` is therefore a cheap
//! change detector, not a second source of truth.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::file_watch::FileWatch;
use crate::Waker;

/// How long the watcher sleeps between checks. `FileWatch` throttles the
/// actual `stat` to its own poll interval, so this only bounds how late a
/// change can be noticed after the stat would have run.
const TICK: Duration = Duration::from_millis(80);

/// A running watcher; dropping it stops the thread.
pub(crate) struct WatchWorker {
    stop: Arc<AtomicBool>,
}

impl WatchWorker {
    /// Starts watching `program` and `overrides`, waking `waker` when either
    /// changes on disk.
    ///
    /// Returns `None` when the thread cannot be spawned; hot reload then only
    /// happens on the next input, which is the pre-P1 behaviour without the
    /// continuous redraw.
    pub(crate) fn start(program: &Path, overrides: &Path, waker: Waker) -> Option<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let program = program.to_path_buf();
        let overrides = overrides.to_path_buf();
        thread::Builder::new()
            .name("motioner-watch".to_owned())
            .spawn(move || run(&program, &overrides, &waker, &flag))
            .ok()
            .map(|_| Self { stop })
    }
}

impl Drop for WatchWorker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn run(program: &Path, overrides: &Path, waker: &Waker, stop: &AtomicBool) {
    let mut watch = FileWatch::new(program);
    let mut overrides_watch = FileWatch::new(overrides);
    while !stop.load(Ordering::Relaxed) {
        thread::sleep(TICK);
        let now = Instant::now();
        if watch.check(now) || overrides_watch.check(now) {
            waker.wake();
        }
    }
}

//! OS file notifications for hot reload (prompt 18 P2).
//!
//! The window used to stat the program file from its frame loop, which only
//! worked because the loop ran at 60 fps. P1 moved the stat to a thread that
//! slept 80 ms; that still woke the thread 12.5 times per second while idle.
//! P2 uses the OS change notifications (`ReadDirectoryChangesW` on Windows via
//! the `notify` crate): the worker blocks on the channel with **no timeout**,
//! debounces a burst of writes, and wakes the event loop once.
//!
//! The worker only notifies: the window still runs
//! [`PreviewSession::poll_changes`](crate::PreviewSession::poll_changes) on
//! the frame the wake produces, which applies the debounce and ignores the
//! editor's own writes.

use std::path::Path;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use notify::{RecursiveMode, Watcher};

use crate::Waker;

/// Quiet period after the last filesystem event before the UI is woken.
const DEBOUNCE: Duration = Duration::from_millis(150);

/// A running watcher; dropping it stops the OS watcher and the worker.
pub(crate) struct WatchWorker {
    /// Kept alive for as long as the window is open; dropping it closes the
    /// notification channel, which makes the worker's blocking `recv` return.
    _watcher: notify::RecommendedWatcher,
}

impl WatchWorker {
    /// Starts watching `program` and `overrides`, waking `waker` when either
    /// changes on disk.
    ///
    /// A missing file is watched through its parent directory (the file may be
    /// created by the editor). Returns `None` when the OS watcher or the
    /// worker cannot start; hot reload then happens on the next input, which
    /// is the same fallback as a failed stat in P1.
    pub(crate) fn start(program: &Path, overrides: &Path, waker: Waker) -> Option<Self> {
        let (sender, receiver) = mpsc::channel();
        let mut watcher =
            notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
                if let Ok(event) = result {
                    if matches!(
                        event.kind,
                        notify::EventKind::Any
                            | notify::EventKind::Create(_)
                            | notify::EventKind::Modify(_)
                            | notify::EventKind::Remove(_)
                    ) {
                        let _ = sender.send(());
                    }
                }
            })
            .ok()?;
        watch_path(&mut watcher, program)?;
        watch_path(&mut watcher, overrides)?;

        thread::Builder::new()
            .name("motioner-watch".to_owned())
            .spawn(move || run(&receiver, &waker))
            .ok()?;
        Some(Self { _watcher: watcher })
    }
}

/// Watches a file, or its parent when the file does not exist yet.
fn watch_path(watcher: &mut notify::RecommendedWatcher, path: &Path) -> Option<()> {
    let target = if path.exists() { path } else { path.parent()? };
    watcher.watch(target, RecursiveMode::NonRecursive).ok()
}

/// Blocks on notifications; a burst is collapsed into one wake after
/// [`DEBOUNCE`]. `recv` has no timeout, so an idle app has no periodic
/// wakeups at all.
fn run(receiver: &mpsc::Receiver<()>, waker: &Waker) {
    while receiver.recv().is_ok() {
        // Drain the rest of the burst; each event pushes the quiet deadline.
        let mut deadline = Instant::now() + DEBOUNCE;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            match receiver.recv_timeout(remaining) {
                Ok(()) => deadline = Instant::now() + DEBOUNCE,
                Err(mpsc::RecvTimeoutError::Timeout) => break,
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }
        }
        waker.wake();
    }
}

//! Process-wide serialization of GPU work in tests.
//!
//! `wgpu` device creation is not safe to run from several threads at once on
//! every Windows driver: parallel `cargo test` threads intermittently panic
//! inside `wgpu_core` while enumerating adapters or creating a device, which
//! made `cargo test --workspace` fail without `--test-threads=1`.
//!
//! Every test that creates a renderer or a harness takes [`lock`] for its
//! whole body. Acquiring the lock is cheap: GPU tests are seconds long and the
//! suite has a handful of them, so serializing costs less than a flaky gate.
//! The lock is re-entrant-safe across panics (a poisoned mutex is recovered,
//! because a failed test says nothing about the GPU).
//!
//! Cargo runs test binaries one at a time, so a process-wide mutex is enough
//! to serialize the whole workspace run.

use std::sync::{Mutex, MutexGuard, PoisonError};

static GPU: Mutex<()> = Mutex::new(());

/// Held for as long as the guard lives; drop it at the end of the test.
#[derive(Debug)]
#[must_use = "the GPU stays serialized only while the guard is alive"]
pub struct GpuLock {
    _guard: MutexGuard<'static, ()>,
}

/// Waits until no other test in this process is using the GPU.
///
/// # Panics
///
/// Never: a poisoned mutex (a previous test panicked) is recovered on purpose.
pub fn lock() -> GpuLock {
    GpuLock {
        _guard: GPU.lock().unwrap_or_else(PoisonError::into_inner),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_guard_serializes_threads() {
        let first = lock();
        let (sender, receiver) = std::sync::mpsc::channel();
        let handle = std::thread::spawn(move || {
            let _second = lock();
            sender.send(()).unwrap();
        });
        assert!(receiver
            .recv_timeout(std::time::Duration::from_millis(50))
            .is_err());
        drop(first);
        receiver
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        handle.join().unwrap();
    }
}

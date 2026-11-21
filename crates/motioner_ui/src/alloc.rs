//! Optional allocation counter for the steady-state profile.
//!
//! Enable the `alloc-count` feature to install a counting global allocator.
//! The editor reads [`allocations`] into its status bar, so a scroll, a
//! playback or a scrub can be checked for per-frame allocations. Off by
//! default: the gate and normal builds use the system allocator unchanged.
//!
//! Counting is **per thread** and opt-in: [`start`] turns it on for the thread
//! that calls it (the editor's UI thread). The GPU and worker threads share the
//! process allocator but are not counted, so the reading is the chrome's own
//! work rather than background driver traffic.

/// Whether this build counts allocations.
#[cfg(feature = "alloc-count")]
#[must_use]
pub const fn enabled() -> bool {
    true
}

/// Whether this build counts allocations.
#[cfg(not(feature = "alloc-count"))]
#[must_use]
pub const fn enabled() -> bool {
    false
}

/// Starts counting allocations on the calling thread.
#[cfg(feature = "alloc-count")]
pub fn start() {
    TRACKING.with(|flag| flag.set(true));
}

/// Starts counting allocations on the calling thread.
#[cfg(not(feature = "alloc-count"))]
pub const fn start() {}

/// Allocations made on this thread so far (0 without the feature or before
/// [`start`]).
#[cfg(feature = "alloc-count")]
#[must_use]
pub fn allocations() -> u64 {
    COUNT.with(std::cell::Cell::get)
}

/// Allocations made on this thread so far (0 without the feature or before
/// [`start`]).
#[cfg(not(feature = "alloc-count"))]
#[must_use]
pub const fn allocations() -> u64 {
    0
}

#[cfg(feature = "alloc-count")]
std::thread_local! {
    static TRACKING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static COUNT: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

#[cfg(feature = "alloc-count")]
#[inline]
fn counting() -> bool {
    TRACKING.try_with(std::cell::Cell::get).unwrap_or(false)
}

#[cfg(feature = "alloc-count")]
#[inline]
fn bump() {
    let _ = COUNT.try_with(|count| count.set(count.get().saturating_add(1)));
}

#[cfg(feature = "alloc-count")]
#[global_allocator]
static ALLOCATOR: Counting = Counting;

#[cfg(feature = "alloc-count")]
struct Counting;

#[cfg(feature = "alloc-count")]
#[allow(unsafe_code)] // a global allocator is inherently unsafe
unsafe impl std::alloc::GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
        if counting() {
            bump();
        }
        std::alloc::System.alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: std::alloc::Layout) {
        std::alloc::System.dealloc(ptr, layout);
    }

    unsafe fn alloc_zeroed(&self, layout: std::alloc::Layout) -> *mut u8 {
        if counting() {
            bump();
        }
        std::alloc::System.alloc_zeroed(layout)
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: std::alloc::Layout, new_size: usize) -> *mut u8 {
        if counting() {
            bump();
        }
        std::alloc::System.realloc(ptr, layout, new_size)
    }
}

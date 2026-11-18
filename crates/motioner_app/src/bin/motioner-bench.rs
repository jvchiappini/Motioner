//! Bench binary with the counting global allocator (prompt 18 P0).
//!
//! Built only with `--features alloc-stats`:
//!
//! ```text
//! cargo build --release -p motioner_app --bin motioner-bench --features alloc-stats
//! ```
//!
//! It runs the normal CLI through the shared library, so `scripts/perf_bench.py`
//! can measure allocations per frame from the engine's `frame-trace` JSON.
//! `motioner.exe` and `motioner-cli.exe` never enable the feature.

#[global_allocator]
static ALLOC: motioner_ui::alloc_stats::CountingAllocator =
    motioner_ui::alloc_stats::CountingAllocator;

fn main() -> std::process::ExitCode {
    motioner_app::run_cli()
}

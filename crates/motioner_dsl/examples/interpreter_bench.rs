//! Reproducible interpreter benchmark (prompt 12, phase 3).
//!
//! Parses a program once, warms up 20 runs and then measures 200 interpreter
//! runs with the value origins enabled. It reports the **median** and the
//! **p90** of the per-run wall time, never the mean: a loaded host moves the
//! mean with a single outlier, while the median stays comparable.
//!
//! Run it in release mode:
//!
//! ```text
//! cargo run --release -p motioner_dsl --example interpreter_bench -- examples/showcase_pro.motion
//! ```
//!
//! `BENCH_LABEL` labels the output (`baseline`, `current`, ...) so two builds
//! can be run back to back without editing the source. Percentiles use the
//! nearest-rank definition: the value at `ceil(p / 100 * n)` in the sorted
//! samples.

use std::time::Instant;

use motioner_core::{TextBounds, TextMeasurement, TextMetrics, TextRequest};
use motioner_dsl::{execute_detailed_with_metrics, parse, Limits};

const WARMUP_RUNS: usize = 20;
const MEASURED_RUNS: usize = 200;

/// Deterministic metrics so `text_width`/`text_bounds` work without the font
/// renderer. The planner benchmark only cares about the interpreter cost, and
/// both builds use the same metrics.
struct FixedMetrics;

impl TextMetrics for FixedMetrics {
    fn measure(&self, request: &TextRequest<'_>) -> Option<TextMeasurement> {
        Some(TextMeasurement {
            width: request.size * 2.0 + f64::from(u32::try_from(request.text.len()).unwrap_or(0)),
            line_count: 1,
            height: request.size,
            ink: TextBounds {
                left: 1.0,
                top: -request.size,
                right: 3.0,
                bottom: 0.0,
            },
            line_advances: vec![request.size],
        })
    }
}

fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "examples/showcase_pro.motion".to_owned());
    let label = std::env::var("BENCH_LABEL").unwrap_or_else(|_| "current".to_owned());
    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {path}: {error}"));
    let program = parse(&source).unwrap_or_else(|error| panic!("cannot parse {path}: {error}"));
    let limits = Limits::default();

    for _ in 0..WARMUP_RUNS {
        let execution = execute_detailed_with_metrics(&program, &limits, &FixedMetrics)
            .unwrap_or_else(|error| panic!("cannot run {path}: {error}"));
        std::hint::black_box(&execution);
    }

    let mut samples = Vec::with_capacity(MEASURED_RUNS);
    let mut nodes = 0;
    let mut origins = 0;
    for _ in 0..MEASURED_RUNS {
        let start = Instant::now();
        let execution = execute_detailed_with_metrics(&program, &limits, &FixedMetrics)
            .unwrap_or_else(|error| panic!("cannot run {path}: {error}"));
        samples.push(start.elapsed().as_secs_f64() * 1000.0);
        nodes = execution.document.nodes().count();
        origins = execution.origins.entries().len();
        std::hint::black_box(&execution);
    }

    samples.sort_by(f64::total_cmp);
    println!(
        "{label}: median {:.3} ms, p90 {:.3} ms (warm-up {WARMUP_RUNS}, runs {MEASURED_RUNS}, nodes {nodes}, origins {origins}, {path})",
        percentile(&samples, 50),
        percentile(&samples, 90)
    );
}

/// Nearest-rank percentile of sorted `samples` (1-based rank, clamped).
fn percentile(samples: &[f64], percent: usize) -> f64 {
    assert!(!samples.is_empty(), "at least one sample is required");
    let rank = (samples.len() * percent).div_ceil(100);
    samples[rank.saturating_sub(1).min(samples.len() - 1)]
}

//! Reproducible interpreter benchmark (prompt 12, phase 3).
//!
//! Parses a program once, warms up 20 runs and then measures 200 interpreter
//! runs with the value origins enabled. It reports the **median** and the
//! **p90** of the per-run wall time, never the mean: a loaded host moves the
//! mean with a single outlier, while the median stays comparable.
//!
//! With the prelude enabled the benchmark parses through
//! [`motioner_dsl::execute_parsed_with_metrics`], which binds the cached
//! library definitions instead of walking them every run (prompt 13b, C0).
//!
//! Run it in release mode:
//!
//! ```text
//! cargo run --release -p motioner_dsl --example interpreter_bench -- examples/showcase_pro.motion
//! ```
//!
//! `BENCH_LABEL` labels the output (`baseline`, `current`, ...) so two builds
//! can be run back to back without editing the source. `BENCH_PRELUDE=0`
//! measures the bare program as the reference. Percentiles use the
//! nearest-rank definition: the value at `ceil(p / 100 * n)` in the sorted
//! samples.

use std::time::Instant;

use motioner_core::{TextBounds, TextMeasurement, TextMetrics, TextRequest};
use motioner_dsl::{
    execute_detailed_with_metrics, execute_parsed_with_metrics, parse, parse_with_prelude,
    Execution, Limits,
};

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
    let with_prelude = std::env::var("BENCH_PRELUDE").is_ok_and(|value| value != "0");
    // Warm the process-wide prelude cache outside the parse measurement so the
    // reported time is the cost a later run actually pays.
    if with_prelude {
        let _ = parse_with_prelude(&source);
    }
    let parse_start = Instant::now();
    let parsed = if with_prelude {
        Some(
            parse_with_prelude(&source)
                .unwrap_or_else(|error| panic!("cannot parse {path}: {error}")),
        )
    } else {
        None
    };
    let plain = if with_prelude {
        None
    } else {
        Some(parse(&source).unwrap_or_else(|error| panic!("cannot parse {path}: {error}")))
    };
    let parsed = parsed.as_ref();
    let parse_ms = parse_start.elapsed().as_secs_f64() * 1000.0;
    let limits = Limits::default();
    let metrics = FixedMetrics;

    let run = || -> Execution {
        match (&plain, parsed) {
            (Some(program), _) => execute_detailed_with_metrics(program, &limits, &metrics),
            (None, Some(parsed)) => execute_parsed_with_metrics(parsed, &limits, &metrics),
            (None, None) => unreachable!("one of the two parses is always present"),
        }
        .unwrap_or_else(|error| panic!("cannot run {path}: {error}"))
    };

    for _ in 0..WARMUP_RUNS {
        let execution = run();
        std::hint::black_box(&execution);
    }

    let mut samples = Vec::with_capacity(MEASURED_RUNS);
    let mut nodes = 0;
    let mut origins = 0;
    for _ in 0..MEASURED_RUNS {
        let start = Instant::now();
        let execution = run();
        samples.push(start.elapsed().as_secs_f64() * 1000.0);
        nodes = execution.document.nodes().count();
        origins = execution.origins.entries().len();
        std::hint::black_box(&execution);
    }

    samples.sort_by(f64::total_cmp);
    println!(
        "{label}: median {:.3} ms, p90 {:.3} ms (parse {parse_ms:.3} ms, prelude {with_prelude}, warm-up {WARMUP_RUNS}, runs {MEASURED_RUNS}, nodes {nodes}, origins {origins}, {path})",
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

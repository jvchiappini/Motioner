//! Compile time must stay linear in the number of statements.
//!
//! The heavy UI fixture (3,000 rows, each a constructor plus a visibility
//! write) used to spend most of its compile time in the binding finalization
//! pass, which was cubic in the statement count. This test pins the shape:
//! 3,000 nodes may take at most 3.5x the time of 1,000, and in release the
//! 3,000-node program must compile in under a second.

use std::fmt::Write as _;
use std::hint::black_box;
use std::path::Path;
use std::time::{Duration, Instant};

use motioner_core::NoTextMetrics;
use motioner_dsl::{execute_parsed_with_metrics, parse_with_prelude, Limits};

/// Best-of rounds per size, so a single scheduling hiccup cannot fail the test.
const ROUNDS: usize = 3;

/// A synthetic program with the same shape as the heavy UI fixture.
fn program(nodes: usize) -> String {
    let mut source = String::with_capacity(64 + nodes * 64);
    source.push_str(
        "comp(name=\"Scaled\", width=1600, height=900, fps=60, duration=600, background=\"#101014\")\n",
    );
    for index in 0..nodes {
        writeln!(source, "row_{index} = rect(6, 6, fill=\"#141820\")").unwrap();
        writeln!(source, "row_{index}.visible = False").unwrap();
    }
    source
}

/// Compiles `source` once and reports the elapsed time and node count.
fn compile(source: &str) -> (Duration, usize) {
    let start = Instant::now();
    let parsed = parse_with_prelude(source).expect("the scaling program parses");
    let execution = execute_parsed_with_metrics(&parsed, &Limits::default(), &NoTextMetrics)
        .expect("the scaling program runs");
    let elapsed = start.elapsed();
    let nodes = execution.document.len();
    black_box(&execution);
    (elapsed, nodes)
}

#[test]
fn three_thousand_nodes_scale_linearly() {
    let small = program(1_000);
    let large = program(3_000);
    // Warm the process-wide prelude cache and the allocator outside the
    // measured rounds, then give both sizes a full warm-up run.
    let _ = compile(&small);
    let _ = compile(&large);

    let mut best_small = Duration::MAX;
    let mut best_large = Duration::MAX;
    let mut small_nodes = 0;
    let mut large_nodes = 0;
    for _ in 0..ROUNDS {
        let (elapsed, nodes) = compile(&small);
        best_small = best_small.min(elapsed);
        small_nodes = nodes;
        let (elapsed, nodes) = compile(&large);
        best_large = best_large.min(elapsed);
        large_nodes = nodes;
    }

    assert_eq!(small_nodes, 1_000);
    assert_eq!(large_nodes, 3_000);
    assert!(
        best_large < best_small.mul_f64(3.5),
        "3,000 nodes took {best_large:?}, more than 3.5x the {best_small:?} for 1,000"
    );
    if !cfg!(debug_assertions) {
        assert!(
            best_large < Duration::from_secs(1),
            "3,000 nodes took {best_large:?} in release, over the 1 s target"
        );
    }
}

#[test]
fn the_heavy_ui_fixture_compiles_in_release() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../out/reports/phase18/fixtures/heavy_ui_list.motioner/compositions/main.motion");
    let Ok(source) = std::fs::read_to_string(&path) else {
        // `out/` is gitignored: the fixture is absent on a clean checkout.
        return;
    };
    let (elapsed, nodes) = compile(&source);
    assert_eq!(nodes, 3_000);
    if !cfg!(debug_assertions) {
        assert!(
            elapsed < Duration::from_secs(1),
            "the heavy UI fixture took {elapsed:?} in release, over the 1 s target"
        );
    }
}

//! Commit-latency benchmark (prompt 12 phase 6, D-0036).
//!
//! Measures only [`motioner_ui::Editor::execute`] on the biggest example
//! (`examples/showcase_pro.motion`), not loading or parsing: each sample gets a
//! fresh copy of the program, the copy is opened outside the timed region and
//! one write-through edit is timed. It reports the median and p90.
//!
//! ```text
//! cargo run --release -p motioner_app --example commit_bench
//! ```

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use motioner_core::{OverrideValue, Vec2};
use motioner_render::{CpuTextMetrics, FontSet};
use motioner_ui::{Command, EditMode, Editor};

const RUNS: usize = 30;
const BUDGET_MS: f64 = 50.0;

fn open(path: &Path) -> Editor {
    let fonts = FontSet::bundled();
    let mut editor =
        Editor::open_with_metrics(path, Arc::new(CpuTextMetrics::new(fonts))).expect("open");
    editor.set_edit_mode(EditMode::Code);
    editor
}

/// Finds a command that the planner can write as code on this program.
fn candidate(path: &Path) -> Command {
    let mut editor = open(path);
    let nodes: Vec<String> = editor
        .session()
        .document()
        .expect("document")
        .nodes()
        .map(|node| node.identity().as_str().to_owned())
        .collect();
    for id in nodes.iter().take(16) {
        for (property, value) in [
            ("rotation", OverrideValue::Number(1.0)),
            ("opacity", OverrideValue::Number(1.0)),
            ("pos", OverrideValue::Vec2(Vec2::new(1.0, 2.0))),
        ] {
            let command = Command::Edit {
                id: id.clone(),
                property: property.to_owned(),
                value: value.clone(),
            };
            let outcome = editor.execute(&command, Duration::ZERO);
            let planned = outcome.is_ok() && editor.last_commit().is_some_and(|report| report.code);
            if planned {
                return command;
            }
            if std::env::var_os("COMMIT_BENCH_VERBOSE").is_some() {
                match outcome {
                    Ok(_) => eprintln!(
                        "{id} {property}: fallback {:?}",
                        editor.last_commit().map(|report| report.reasons.clone())
                    ),
                    Err(error) => eprintln!("{id} {property}: error {error}"),
                }
            }
            editor = open(path);
        }
    }
    panic!("no write-through candidate in {}", path.display());
}

fn percentile(samples: &[f64], percent: usize) -> f64 {
    let rank = (samples.len() * percent).div_ceil(100);
    samples[rank.saturating_sub(1).min(samples.len() - 1)]
}

fn main() {
    let source_path = PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "examples/showcase_pro.motion".to_owned()),
    );
    let source = std::fs::read_to_string(&source_path).expect("read the program");
    let temp = std::env::temp_dir().join(format!(
        "motioner_commit_bench_{}.motion",
        std::process::id()
    ));
    std::fs::write(&temp, &source).expect("write the scratch program");
    let command = candidate(&temp);
    std::fs::remove_file(&temp).ok();

    let mut samples = Vec::with_capacity(RUNS);
    for _ in 0..RUNS {
        std::fs::write(&temp, &source).expect("write the scratch program");
        let mut editor = open(&temp);
        let start = Instant::now();
        editor
            .execute(&command, Duration::ZERO)
            .expect("the candidate command runs");
        samples.push(start.elapsed().as_secs_f64() * 1000.0);
        assert!(
            editor.last_commit().is_some_and(|report| report.code),
            "the candidate stays write-through"
        );
        std::fs::remove_file(&temp).ok();
    }
    samples.sort_by(f64::total_cmp);
    let median = percentile(&samples, 50);
    let p90 = percentile(&samples, 90);
    println!(
        "commit: median {median:.2} ms, p90 {p90:.2} ms (runs {RUNS}, budget {BUDGET_MS:.0} ms, {})",
        source_path.display()
    );
    assert!(
        median < BUDGET_MS,
        "commit median {median:.2} ms exceeds the budget"
    );
}

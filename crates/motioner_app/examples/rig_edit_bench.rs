//! Rig write-through latency (prompt 16b E2).
//!
//! Measures [`motioner_ui::Editor::apply_rig_edit`] on the mascot demo: one
//! property key is replaced through the rig planner, the candidate is
//! compiled and the manifest compared. Opening the program is outside the
//! timed region. It reports median and p90 over 20 runs.
//!
//! ```text
//! cargo run --release -p motioner_app --example rig_edit_bench
//! ```

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use motioner_core::{Ease, NoTextMetrics, RigValue, Vec2};
use motioner_dsl::RigEdit;
use motioner_io::ProjectAssets;
use motioner_ui::Editor;

const RUNS: usize = 20;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn project() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("motioner_rig_bench_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("examples/rig/assets")).expect("create the scratch project");
    for file in ["mascot_demo.motion", "mascot.motion"] {
        std::fs::copy(
            root().join("examples/rig").join(file),
            dir.join("examples/rig").join(file),
        )
        .expect("copy the rig");
    }
    std::fs::copy(
        root().join("examples/rig/assets/mascot.svg"),
        dir.join("examples/rig/assets/mascot.svg"),
    )
    .expect("copy the symbol");
    dir
}

fn main() {
    let dir = project();
    let source = std::fs::read_to_string(dir.join("examples/rig/mascot_demo.motion"))
        .expect("read the demo");
    let edit = RigEdit::Key {
        rig: "mascot".to_owned(),
        property: "look".to_owned(),
        side: motioner_dsl::RigSide::Both,
        time: 2.6,
        value: RigValue::Vec2(Vec2::new(0.2, -0.2)),
        ease: Ease::CubicOut,
    };
    let mut samples = Vec::with_capacity(RUNS);
    for run in 0..RUNS {
        std::fs::write(dir.join("examples/rig/mascot_demo.motion"), &source)
            .expect("reset the demo");
        let mut editor = Editor::open_with_assets(
            &dir.join("examples/rig/mascot_demo.motion"),
            Arc::new(NoTextMetrics),
            Rc::new(ProjectAssets::new(&dir)),
        )
        .expect("open the demo");
        let start = Instant::now();
        editor.apply_rig_edit(&edit).expect("the rig edit is exact");
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        eprintln!("run {run}: {elapsed:.2} ms");
        samples.push(elapsed);
    }
    samples.sort_by(f64::total_cmp);
    let median = percentile(&samples, 50);
    let p90 = percentile(&samples, 90);
    println!("rig edit: median {median:.2} ms, p90 {p90:.2} ms (runs {RUNS})");
    let _ = std::fs::remove_dir_all(dir);
}

fn percentile(samples: &[f64], percent: usize) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let index = (samples.len() - 1) * percent / 100;
    samples[index.min(samples.len() - 1)]
}

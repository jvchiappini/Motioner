//! Measurement harness for a scene with 50 mascot instances (prompt 16b, E0).
//!
//! Ignored by default so the gate stays fast. Run it in release on the
//! reference machine:
//!
//! ```text
//! cargo test -p motioner_testkit --release --test mascot_scene_bench -- --ignored --nocapture
//! ```
//!
//! It reports the load (build and bake) time, the node and transform-key
//! counts, the resident-memory delta and the rendering throughput at 1080p,
//! which is the editor's preview resolution for this composition.

use std::fmt::Write as _;
use std::time::Instant;

use motioner_core::{Axis, NoTextMetrics};
use motioner_eval::Evaluator;
use motioner_io::ProjectAssets;
use motioner_render::{FrameRenderer, RenderError};

const INSTANCES: usize = 50;
const FRAMES: usize = 60;

fn root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Fifty instances in a 10 x 5 grid at natural size.
fn scene_source() -> String {
    let mut source = String::from(
        "comp(name=\"50 mascots\", width=1920, height=1080, fps=60, duration=15.0, \
         background=\"#0A0A0A\")\nrigs = use(\"examples/rig/mascot.motion\")\n",
    );
    for index in 0..INSTANCES {
        let column = index % 10;
        let row = index / 10;
        let x = 96 + column * 192;
        let y = 108 + row * 216;
        let _ = writeln!(
            source,
            "m{index} = rigs.build(pos=({x}, {y}), scale=1.0, name=\"mascot {index}\")"
        );
        let _ = writeln!(source, "m{index}.pose(\"Feliz\", at=0.5, dur=0.3)");
        let _ = writeln!(source, "m{index}.play(\"salto\", at=2.0, loops=1)");
        let _ = writeln!(source, "m{index}.look.key(1.0, (0.5, 0.0))");
    }
    source
}

fn transform_keys(document: &motioner_core::Document) -> usize {
    document
        .nodes()
        .map(|node| {
            let transform = node.transform();
            [
                transform.position.axis(Axis::X).keys().len(),
                transform.position.axis(Axis::Y).keys().len(),
                transform.scale.axis(Axis::X).keys().len(),
                transform.scale.axis(Axis::Y).keys().len(),
                transform.rotation.keys().len(),
                transform.opacity.keys().len(),
            ]
            .into_iter()
            .sum::<usize>()
        })
        .sum()
}

#[cfg(windows)]
mod memory {
    use std::process::Command;

    /// The process's resident working set in bytes, read from `Get-Process`.
    ///
    /// A measurement harness may shell out; the crate itself stays free of
    /// platform code and unsafe blocks (the workspace forbids `unsafe`).
    pub fn working_set() -> Option<u64> {
        let pid = std::process::id();
        let output = Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                &format!("(Get-Process -Id {pid}).WorkingSet64"),
            ])
            .output()
            .ok()?;
        String::from_utf8(output.stdout).ok()?.trim().parse().ok()
    }
}

#[cfg(not(windows))]
mod memory {
    /// Resident memory is only reported on Windows (the reference machine).
    pub fn working_set() -> Option<u64> {
        None
    }
}

#[allow(clippy::cast_precision_loss)] // measurement display only
fn mib(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

#[test]
#[ignore = "measurement harness"]
fn fifty_mascots_load_and_preview() {
    let _gpu = motioner_testkit::gpu::lock();
    let root = root();
    let assets = ProjectAssets::new(&root);
    let source = scene_source();
    let before = memory::working_set();

    let start = Instant::now();
    let document = motioner_dsl::run_with_assets(
        &source,
        &motioner_dsl::Limits::default(),
        &NoTextMetrics,
        &assets,
    )
    .expect("the 50-instance scene compiles");
    let build_ms = start.elapsed().as_secs_f64() * 1000.0;
    let after = memory::working_set();

    let keys = transform_keys(&document);
    println!(
        "build+bake: {build_ms:.1} ms for {INSTANCES} mascots, {} nodes, {keys} transform keys",
        document.len()
    );
    if let (Some(before), Some(after)) = (before, after) {
        println!(
            "resident memory delta: {:.1} MiB",
            mib(after.saturating_sub(before))
        );
    }

    let mut evaluator = Evaluator::new();
    let mut plan = motioner_eval::RenderPlan::new();
    evaluator.evaluate(&document, 0, &mut plan);
    let mut renderer = match FrameRenderer::new(plan.width(), plan.height()) {
        Ok(renderer) => renderer,
        Err(RenderError::NoAdapter { message }) => {
            eprintln!("skipping preview fps: no usable adapter ({message})");
            return;
        }
        Err(other) => panic!("renderer creation failed: {other}"),
    };
    let mut frame = Vec::new();
    let preview_start = Instant::now();
    for index in 0..FRAMES {
        let frame_number = i64::try_from(index).unwrap_or(0);
        evaluator.evaluate(&document, frame_number, &mut plan);
        renderer
            .render(&document, &plan, &mut frame)
            .expect("a 1080p preview frame renders");
    }
    let seconds = preview_start.elapsed().as_secs_f64();
    println!(
        "preview: {:.1} fps at {}x{} over {FRAMES} frames",
        f64::from(u32::try_from(FRAMES).unwrap_or(0)) / seconds,
        plan.width(),
        plan.height()
    );
}

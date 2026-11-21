//! Measurement harness for the mascot import and rig bake.
//!
//! Ignored by default so the gate stays fast. Run in release:
//! `cargo test -p motioner_io --release --test rig_bench -- --ignored --nocapture`.

use std::time::Instant;

use motioner_core::NoTextMetrics;
use motioner_io::parse_svg;
use motioner_io::ProjectAssets;

fn root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
#[ignore = "measurement harness"]
fn bench_mascot_build_and_svg_import() {
    let root = root();
    let source = std::fs::read_to_string(root.join("examples/rig/mascot_demo.motion")).unwrap();
    let assets = ProjectAssets::new(&root);
    let mut best = f64::MAX;
    for _ in 0..5 {
        let start = Instant::now();
        let document = motioner_dsl::run_with_assets(
            &source,
            &motioner_dsl::Limits::default(),
            &NoTextMetrics,
            &assets,
        )
        .expect("the demo compiles");
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        best = best.min(elapsed);
        let keys: usize = document
            .nodes()
            .map(|node| {
                node.transform()
                    .position
                    .axis(motioner_core::Axis::X)
                    .keys()
                    .len()
                    + node
                        .transform()
                        .position
                        .axis(motioner_core::Axis::Y)
                        .keys()
                        .len()
                    + node
                        .transform()
                        .scale
                        .axis(motioner_core::Axis::X)
                        .keys()
                        .len()
                    + node
                        .transform()
                        .scale
                        .axis(motioner_core::Axis::Y)
                        .keys()
                        .len()
                    + node.transform().rotation.keys().len()
                    + node.transform().opacity.keys().len()
            })
            .sum();
        println!(
            "build+bake {:>8.2} ms, {} nodes, {} transform keys",
            elapsed,
            document.len(),
            keys
        );
    }
    println!(
        "best build+bake: {best:.2} ms (901 frames => {:.3} ms/frame)",
        best / 901.0
    );

    let svg = std::fs::read_to_string(root.join("examples/rig/assets/mascot.svg")).unwrap();
    let mut best = f64::MAX;
    for _ in 0..20 {
        let start = Instant::now();
        let symbol = parse_svg(&svg, "mascot").expect("the mascot parses");
        best = best.min(start.elapsed().as_secs_f64() * 1000.0);
        assert_eq!(symbol.part_count(), 34);
    }
    println!("best mascot.svg import: {best:.3} ms");
}

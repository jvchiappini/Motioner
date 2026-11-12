//! Bake cost with a lattice and geometry drivers on the mascot (D-0059).
//!
//! Run with `cargo run -p motioner_app --release --example deformer_bench`.

use std::path::Path;

use motioner_core::NoTextMetrics;
use motioner_io::ProjectAssets;

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = std::fs::read_to_string(root.join("examples/rig/mascot_demo.motion")).unwrap();
    let source = format!(
        "{source}\n\
         m.prop(\"ltl\", (0.0, 0.0))\n\
         m.prop(\"ltr\", (0.0, 0.0))\n\
         m.prop(\"lbr\", (0.0, 0.0))\n\
         m.prop(\"lbl\", (0.0, 0.0))\n\
         m.lattice(\"body\", corners=(\"ltl\", \"ltr\", \"lbr\", \"lbl\"))\n"
    );
    let assets = ProjectAssets::new(&root);
    let mut samples = Vec::new();
    for _ in 0..5 {
        let start = std::time::Instant::now();
        let document = motioner_dsl::run_with_assets(
            &source,
            &motioner_dsl::Limits::default(),
            &NoTextMetrics,
            &assets,
        )
        .expect("mascot loads");
        let frames = document.composition().duration();
        #[allow(clippy::cast_precision_loss)]
        let frames = frames as f64;
        samples.push(start.elapsed().as_secs_f64() * 1000.0 / frames);
    }
    samples.sort_by(f64::total_cmp);
    println!(
        "mascot bake + lattice: {:.3} ms/frame",
        samples[samples.len() / 2]
    );
}

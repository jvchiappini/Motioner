//! A `vector-effect="non-scaling-stroke"` path keeps its authored width when
//! the symbol instance is squashed by a node scale.
//!
//! This is the mascot's mouth failure in miniature: the mouth group is scaled
//! to 6 % of its height, and a local-space stroke collapsed to a hairline.
//! The reference image is a rendered line; the assertion counts lit rows, so
//! it fails without the world-space stroking path.

use std::sync::Arc;

use motioner_core::NoTextMetrics;
use motioner_dsl::{run_with_assets, AssetResolver, LoadedAsset};
use motioner_eval::{Evaluator, RenderPlan};
use motioner_render::{FrameRenderer, RenderError};

const SVG: &str = include_str!("fixtures/non_scaling_stroke.svg");
const SOURCE: &str = r##"
comp(name="Stroke", width=400, height=400, fps=60, duration=1.0, background="#000000")
art = use("stroke.svg")
s = art(pos=(200, 200), scale=1.0)
s.scale = (1.0, 0.06)
"##;

/// Serves the fixture SVG through the same resolver contract the host uses.
struct Fixture;

impl AssetResolver for Fixture {
    fn load(&self, path: &str, _base: &str) -> Result<LoadedAsset, String> {
        if path != "stroke.svg" {
            return Err(format!("unexpected asset {path}"));
        }
        let symbol = motioner_io::parse_svg(SVG, "stroke").map_err(|error| error.to_string())?;
        Ok(LoadedAsset::Symbol {
            key: path.to_owned(),
            symbol: Arc::new(symbol),
        })
    }
}

#[test]
fn a_squashed_symbol_keeps_its_stroke_width() {
    let _gpu = motioner_testkit::gpu::lock();
    let document = run_with_assets(
        SOURCE,
        &motioner_dsl::Limits::default(),
        &NoTextMetrics,
        &Fixture,
    )
    .expect("the fixture compiles");
    let mut evaluator = Evaluator::new();
    let mut plan = RenderPlan::new();
    evaluator.evaluate(&document, 0, &mut plan);
    let mut renderer = match FrameRenderer::new(plan.width(), plan.height()) {
        Ok(renderer) => renderer,
        Err(RenderError::NoAdapter { message }) => {
            eprintln!("skipping GPU test: no usable adapter ({message})");
            return;
        }
        Err(other) => panic!("renderer creation failed: {other}"),
    };
    let mut frame = Vec::new();
    renderer.render(&document, &plan, &mut frame).unwrap();

    let width = plan.width();
    let lit_rows = (0..plan.height())
        .filter(|&y| {
            let offset = ((y * width + width * 3 / 4) * 4) as usize;
            frame[offset] > 128
        })
        .count();
    assert!(
        (8..=12).contains(&lit_rows),
        "the authored stroke is 10 px, got {lit_rows} lit rows"
    );
}

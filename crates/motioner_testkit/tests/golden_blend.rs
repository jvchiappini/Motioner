//! Golden frame for `examples/blend.motion` (D-0031).
//!
//! Seven swatches, one per blend mode, over a known #808080 plate.  Each tile
//! carries a darker and a lighter half so darken and lighten are both visible.
//!
//! Backdrop-aware modes (overlay, darken, lighten) need the `FerrousEngine`
//! composite pass.  On the current pin the renderer refuses the frame with a
//! typed `Unsupported` error, so this test skips cleanly; once the pin is bumped
//! and `engine-composite` is enabled it compares the still with the D-0028
//! policy (at most 2 per channel, at most 0.1 % of pixels above 2).

use std::error::Error;
use std::fmt::Write as _;

use motioner_eval::{Evaluator, RenderPlan};
use motioner_render::{FrameRenderer, RenderError};
use sha2::{Digest, Sha256};

/// Frame and its embedded golden still.
const FRAME: i64 = 30;
const GOLDEN: &[u8] = include_bytes!("../golden/blend_f30.png");
/// SHA-256 of [`GOLDEN`], pinned so a drifting fixture is caught.
const GOLDEN_SHA: &str = "052dd593d06b51db66d999bdf3f1aa65c1619f85f98366317f6cc9d52f58efcb";
/// Source of the example composition.
const SOURCE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../examples/blend.motion"
));
/// Maximum allowed absolute difference per channel.
const MAX_CHANNEL_DIFF: u8 = 2;

fn hex(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut text, "{byte:02x}").expect("writing to a String cannot fail");
    }
    text
}

#[test]
fn blend_swatches_match_the_golden() -> Result<(), Box<dyn Error>> {
    let _gpu = motioner_testkit::gpu::lock();
    assert_eq!(
        hex(&Sha256::digest(GOLDEN)),
        GOLDEN_SHA,
        "the golden PNG changed; regenerate it with `motioner still` and update the hash"
    );
    let document = motioner_dsl::run(SOURCE, &motioner_dsl::Limits::default())?;
    let mut evaluator = Evaluator::new();
    let mut plan = RenderPlan::new();
    evaluator.evaluate(&document, FRAME, &mut plan);
    let mut renderer = match FrameRenderer::new(plan.width(), plan.height()) {
        Ok(renderer) => renderer,
        Err(RenderError::NoAdapter { message }) => {
            eprintln!("skipping golden test: no usable adapter ({message})");
            return Ok(());
        }
        Err(other) => panic!("renderer creation failed: {other}"),
    };
    let mut actual = Vec::new();
    match renderer.render(&document, &plan, &mut actual) {
        Ok(_) => {}
        Err(RenderError::Unsupported { .. }) => {
            eprintln!(
                "skipping blend golden: the pinned engine has no backdrop composite (D-0031)"
            );
            return Ok(());
        }
        Err(other) => panic!("render failed: {other}"),
    }

    let decoder = png::Decoder::new(std::io::Cursor::new(GOLDEN));
    let mut reader = decoder.read_info()?;
    let mut expected = vec![0_u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut expected)?;
    expected.truncate(info.buffer_size());
    assert_eq!(info.color_type, png::ColorType::Rgba);
    assert_eq!(info.bit_depth, png::BitDepth::Eight);
    assert_eq!((info.width, info.height), (plan.width(), plan.height()));
    assert_eq!(expected.len(), actual.len());

    let mut max_difference = 0_u8;
    let mut differing_pixels = 0_u64;
    for (actual_pixel, expected_pixel) in actual.chunks_exact(4).zip(expected.chunks_exact(4)) {
        let mut pixel_difference = 0_u8;
        for (actual_channel, expected_channel) in actual_pixel.iter().zip(expected_pixel) {
            let difference = i16::from(*actual_channel) - i16::from(*expected_channel);
            let difference = u8::try_from(difference.unsigned_abs()).unwrap_or(u8::MAX);
            pixel_difference = pixel_difference.max(difference);
        }
        max_difference = max_difference.max(pixel_difference);
        if pixel_difference > MAX_CHANNEL_DIFF {
            differing_pixels += 1;
        }
    }
    let total_pixels = (actual.len() / 4) as u64;
    assert!(
        differing_pixels * 1000 <= total_pixels,
        "too many differing pixels: {differing_pixels} of {total_pixels} (max {max_difference})"
    );
    assert!(
        max_difference <= MAX_CHANNEL_DIFF,
        "max channel difference {max_difference} exceeds {MAX_CHANNEL_DIFF}"
    );
    Ok(())
}

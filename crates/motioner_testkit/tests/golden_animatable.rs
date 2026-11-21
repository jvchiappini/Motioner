//! Golden-frame test for `examples/animatable.motion` at frame 45.
//!
//! The composition keys every animatable content property of D-0028 (fill,
//! stroke, stroke width, radius, size and text colour), so this still fails if
//! a content channel stops interpolating. The reference PNG was produced by
//! `motioner still examples/animatable.motion --frame 45` on the reference
//! machine. It is compared with the tolerance policy of D-0028: no channel may
//! differ by more than 2, and at most 0.1 % of the pixels may differ by more
//! than 2. The test skips cleanly when no GPU adapter is available.

use std::error::Error;
use std::fmt::Write as _;

use motioner_eval::{Evaluator, RenderPlan};
use motioner_render::{FrameRenderer, RenderError};
use sha2::{Digest, Sha256};

/// The golden still, embedded so the test does not depend on the cwd.
const GOLDEN_PNG: &[u8] = include_bytes!("../golden/animatable_f45.png");
/// SHA-256 of [`GOLDEN_PNG`], stored to detect a silently replaced reference.
const GOLDEN_SHA256: &str = "de69d5264dc8669c79a6f908602eb841e8565e7e3731e2b00d144617d5c16353";
/// Source of the example composition.
const SOURCE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../examples/animatable.motion"
));
/// The frame the reference was rendered at.
const FRAME: i64 = 45;
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
fn frame_45_matches_the_golden_still() -> Result<(), Box<dyn Error>> {
    let _gpu = motioner_testkit::gpu::lock();
    assert_eq!(
        hex(&Sha256::digest(GOLDEN_PNG)),
        GOLDEN_SHA256,
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
    renderer.render(&document, &plan, &mut actual)?;

    let decoder = png::Decoder::new(std::io::Cursor::new(GOLDEN_PNG));
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
        max_difference <= MAX_CHANNEL_DIFF,
        "max channel difference {max_difference} exceeds {MAX_CHANNEL_DIFF}"
    );
    assert!(
        differing_pixels.saturating_mul(1000) <= total_pixels,
        "{differing_pixels} of {total_pixels} pixels differ by more than {MAX_CHANNEL_DIFF} (limit 0.1 %)"
    );
    Ok(())
}

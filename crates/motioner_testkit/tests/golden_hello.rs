//! Golden-frame test for `examples/hello.motion` at frame 120.
//!
//! The reference PNG was produced by `motioner still examples/hello.motion
//! --frame 120` on the reference machine. It is compared with a documented
//! tolerance: no channel may differ by more than 2, and at most 0.1 % of the
//! pixels may differ by more than 2. The test skips cleanly when no GPU
//! adapter is available, so CPU-only CI stays green.

use std::error::Error;
use std::fmt::Write as _;

use motioner_eval::{Evaluator, RenderPlan};
use motioner_render::{FrameRenderer, RenderError};
use sha2::{Digest, Sha256};

/// The golden still, embedded so the test does not depend on the cwd.
const GOLDEN_PNG: &[u8] = include_bytes!("../golden/hello_f120.png");
/// SHA-256 of [`GOLDEN_PNG`], stored here as information and to detect a
/// silently replaced reference.
const GOLDEN_SHA256: &str = "c2173badd7f23933461473c5d682e40fc9fc1dcbd4ebbe10a1c7c1c7de5cf15c";
/// Source of the example composition.
const SOURCE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../examples/hello.motion"
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
fn frame_120_matches_the_golden_still() -> Result<(), Box<dyn Error>> {
    let _gpu = motioner_testkit::gpu::lock();
    assert_eq!(
        hex(&Sha256::digest(GOLDEN_PNG)),
        GOLDEN_SHA256,
        "the golden PNG changed; regenerate it with `motioner still` and update the hash"
    );

    let document = motioner_dsl::run(SOURCE, &motioner_dsl::Limits::default())?;
    let mut evaluator = Evaluator::new();
    let mut plan = RenderPlan::new();
    evaluator.evaluate(&document, 120, &mut plan);

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

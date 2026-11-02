//! Golden-frame and banding test for `examples/gradients.motion` (D-0030).
//!
//! Frame 30 shows the card ramp, the glowing radial sheen, the gradient
//! hexagon stroke and the ribbon half drawn. Frame 105 shows the finished
//! ribbon and the animated stop offsets.
//!
//! The pinned engine cannot draw gradients yet (`RenderError::Unsupported`,
//! see D-0030), so the test skips cleanly until the pin bump. Once the
//! `engine-gradients` feature is enabled every frame here runs and compares
//! with the D-0028 policy: at most 2 per channel, at most 0.1 % of pixels
//! above 2. The banding check walks a line inside the card and bounds the
//! step between neighbouring pixels; dithering keeps flat ramps off the
//! contour.

use std::error::Error;
use std::fmt::Write as _;

use motioner_eval::{Evaluator, RenderPlan};
use motioner_render::{FrameRenderer, RenderError};
use sha2::{Digest, Sha256};

/// Golden stills, embedded so the test does not depend on the cwd.
const GOLDENS: [(i64, &[u8], &str); 2] = [
    (
        30,
        include_bytes!("../golden/gradients_f30.png"),
        "466dceb414c5e4ed255084790e29b31ecd58fd44a0d765724cb952d2a70d18a4",
    ),
    (
        105,
        include_bytes!("../golden/gradients_f105.png"),
        "d0c73e827cc3afadc019d5326d838957a602ae4bea09adc245dc0535a382aacf",
    ),
];
/// Source of the example composition.
const SOURCE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../examples/gradients.motion"
));
/// Maximum allowed absolute difference per channel.
const MAX_CHANNEL_DIFF: u8 = 2;
/// Largest step between neighbouring pixels inside the card ramp.
const MAX_STEP: u8 = 3;

fn hex(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut text, "{byte:02x}").expect("writing to a String cannot fail");
    }
    text
}

#[test]
fn gradients_match_the_golden_stills() -> Result<(), Box<dyn Error>> {
    let _gpu = motioner_testkit::gpu::lock();
    let document = motioner_dsl::run(SOURCE, &motioner_dsl::Limits::default())?;
    let mut evaluator = Evaluator::new();
    let mut plan = RenderPlan::new();
    let mut renderer: Option<FrameRenderer> = None;

    for (frame, golden, sha) in GOLDENS {
        assert_eq!(
            hex(&Sha256::digest(golden)),
            sha,
            "the golden PNG changed; regenerate it with `motioner still` and update the hash"
        );
        evaluator.evaluate(&document, frame, &mut plan);
        let renderer = match &mut renderer {
            Some(renderer) => renderer,
            slot @ None => {
                let created = match FrameRenderer::new(plan.width(), plan.height()) {
                    Ok(created) => created,
                    Err(RenderError::NoAdapter { message }) => {
                        eprintln!("skipping golden test: no usable adapter ({message})");
                        return Ok(());
                    }
                    Err(other) => panic!("renderer creation failed: {other}"),
                };
                slot.insert(created)
            }
        };
        let mut actual = Vec::new();
        match renderer.render(&document, &plan, &mut actual) {
            Ok(_) => {}
            Err(RenderError::Unsupported { .. }) => {
                eprintln!(
                    "skipping gradient golden: the pinned engine has no gradient pass (D-0030)"
                );
                return Ok(());
            }
            Err(other) => panic!("render failed: {other}"),
        }

        let decoder = png::Decoder::new(std::io::Cursor::new(golden));
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
            "frame {frame}: max channel difference {max_difference} exceeds {MAX_CHANNEL_DIFF}"
        );
        assert!(
            differing_pixels.saturating_mul(1000) <= total_pixels,
            "frame {frame}: {differing_pixels} of {total_pixels} pixels differ by more than \
             {MAX_CHANNEL_DIFF} (limit 0.1 %)"
        );

        if frame == 30 {
            assert_no_banding(&actual, plan.width(), plan.height());
        }
    }
    Ok(())
}

/// Walks a horizontal line inside the card, away from the sheen, the hexagon
/// and the ribbon, and bounds the change between neighbouring pixels.
fn assert_no_banding(pixels: &[u8], width: u32, height: u32) {
    let y = height / 2 + 30;
    let mut max_step = 0u8;
    let mut max_at = 0u32;
    for x in 230..width.saturating_sub(340) {
        if x == 0 {
            continue;
        }
        let current = ((y * width + x) * 4) as usize;
        let previous = ((y * width + x - 1) * 4) as usize;
        for channel in 0..3 {
            let step = pixels[current + channel].abs_diff(pixels[previous + channel]);
            if step > max_step {
                max_step = step;
                max_at = x;
            }
        }
    }
    assert!(
        max_step <= MAX_STEP,
        "the card ramp steps {max_step} code(s) at x = {max_at}; dithering failed"
    );
}

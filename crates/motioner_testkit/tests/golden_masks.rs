//! Golden stills for `examples/masks.motion` (D-0033).
//!
//! Two beats: an alpha-masked reveal (a white bar wipes a headline plate in,
//! frame 36) and an inverted luma cut-out (a grey disc is a translucent window
//! over a gradient card, frame 150).  The committed stills are validated
//! pixel-by-pixel below so the fixtures cannot drift silently, and every run
//! renders both beats and compares them with the stills.

use std::error::Error;
use std::fmt::Write as _;

use motioner_eval::{Evaluator, RenderPlan};
use motioner_render::FrameRenderer;
use sha2::{Digest, Sha256};

/// Frame, embedded still and pinned SHA-256.
const GOLDENS: [(i64, &[u8], &str); 2] = [
    (
        36,
        include_bytes!("../golden/masks_alpha_reveal_f36.png"),
        "b72541fba679d0fbf89e4a30b15fa0cf9cb813470fe353ecf0e7f375855531d2",
    ),
    (
        150,
        include_bytes!("../golden/masks_luma_invert_f150.png"),
        "2c5189e4ca73c3c0f4fe07d8ee5bad3471e4a6901dcf86ddc667de10d149acae",
    ),
];
const SOURCE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../examples/masks.motion"
));
const MAX_CHANNEL_DIFF: u8 = 2;

fn hex(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut text, "{byte:02x}").expect("writing to a String cannot fail");
    }
    text
}

fn decode(golden: &[u8]) -> Result<(Vec<u8>, u32, u32), Box<dyn Error>> {
    let decoder = png::Decoder::new(std::io::Cursor::new(golden));
    let mut reader = decoder.read_info()?;
    let mut pixels = vec![0_u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut pixels)?;
    pixels.truncate(info.buffer_size());
    assert_eq!(info.color_type, png::ColorType::Rgba);
    assert_eq!(info.bit_depth, png::BitDepth::Eight);
    Ok((pixels, info.width, info.height))
}

fn pixel(pixels: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let offset = ((y * width + x) * 4) as usize;
    [
        pixels[offset],
        pixels[offset + 1],
        pixels[offset + 2],
        pixels[offset + 3],
    ]
}

fn within(pixel: [u8; 4], expected: [u8; 3]) -> bool {
    pixel[..3]
        .iter()
        .zip(expected)
        .all(|(actual, expected)| actual.abs_diff(expected) <= MAX_CHANNEL_DIFF)
}

#[test]
fn each_still_shows_its_mask_beat() -> Result<(), Box<dyn Error>> {
    let background = [14, 17, 22];
    let plate = [255, 107, 77];

    for (frame, golden, sha) in GOLDENS {
        assert_eq!(
            hex(&Sha256::digest(golden)),
            sha,
            "golden frame {frame} changed; regenerate it with `motioner still` and update the hash"
        );
        let (pixels, width, height) = decode(golden)?;
        assert_eq!((width, height), (960, 540));

        match frame {
            36 => {
                // The wipe covers x in [260, 700]: the plate is visible only
                // there, and the mask clips it exactly at both edges.
                assert!(
                    within(pixel(&pixels, width, 220, 250), background),
                    "left of the wipe must be clear"
                );
                assert!(
                    within(pixel(&pixels, width, 300, 250), plate),
                    "the revealed plate must show its colour"
                );
                assert!(
                    within(pixel(&pixels, width, 690, 250), plate),
                    "the plate is revealed up to the wipe's right edge"
                );
                assert!(
                    within(pixel(&pixels, width, 710, 250), background),
                    "right of the wipe must be clear"
                );
                // The luma disc sits at x=380 in this beat: the pixel covered
                // by both the card and the disc is darker than the untouched
                // card, while the card outside the disc is unaffected.
                assert!(
                    within(pixel(&pixels, width, 560, 400), [94, 203, 239]),
                    "the card outside the disc keeps its gradient"
                );
                assert!(
                    within(pixel(&pixels, width, 480, 470), [51, 242, 230]),
                    "the card corner outside the disc keeps its gradient"
                );
                let outside = pixel(&pixels, width, 330, 400);
                assert!(
                    outside[1] < 160,
                    "the inverted luma disc must dim the card where it overlaps: {outside:?}"
                );
            }
            150 => {
                // The wipe has slid past the plate; only its last sliver shows.
                assert!(
                    within(pixel(&pixels, width, 600, 250), background),
                    "the wipe must have moved past the plate"
                );
                assert!(
                    within(pixel(&pixels, width, 700, 250), plate),
                    "the last sliver of the reveal stays"
                );
                // The disc moved to x=560 and dims the card there.
                assert!(
                    within(pixel(&pixels, width, 330, 400), [79, 173, 247]),
                    "the card outside the disc keeps its gradient"
                );
                let hole = pixel(&pixels, width, 560, 400);
                assert!(
                    within(hole, [85, 182, 215]),
                    "the luma overlap must be a translucent window: {hole:?}"
                );
                assert!(
                    hole[1] < 190,
                    "the window must be darker than the untouched card: {hole:?}"
                );
                assert!(
                    within(pixel(&pixels, width, 480, 470), [46, 217, 206]),
                    "the pixel covered by the moved disc is dimmed"
                );
            }
            _ => panic!("unexpected frame {frame}"),
        }
    }
    Ok(())
}

/// Renders the beats and compares them with the stills.
#[test]
fn masks_match_the_goldens() -> Result<(), Box<dyn Error>> {
    let _gpu = motioner_testkit::gpu::lock();
    let document = motioner_dsl::run(SOURCE, &motioner_dsl::Limits::default())?;
    let mut plan = RenderPlan::new();
    let mut renderer: Option<FrameRenderer> = None;
    for (frame, golden, _) in GOLDENS {
        Evaluator::new().evaluate(&document, frame, &mut plan);
        let renderer = match &mut renderer {
            Some(renderer) => renderer,
            slot @ None => slot.insert(FrameRenderer::new(plan.width(), plan.height())?),
        };
        let mut actual = Vec::new();
        renderer.render(&document, &plan, &mut actual)?;
        let (expected, width, height) = decode(golden)?;
        assert_eq!((width, height), (plan.width(), plan.height()));
        let mut max_difference = 0_u8;
        let mut differing_pixels = 0_u64;
        for (actual_pixel, expected_pixel) in actual.chunks_exact(4).zip(expected.chunks_exact(4)) {
            let difference = actual_pixel
                .iter()
                .zip(expected_pixel)
                .map(|(a, b)| {
                    u8::try_from(i16::from(*a).abs_diff(i16::from(*b))).unwrap_or(u8::MAX)
                })
                .max()
                .unwrap_or(0);
            max_difference = max_difference.max(difference);
            if difference > MAX_CHANNEL_DIFF {
                differing_pixels += 1;
            }
        }
        let total = (actual.len() / 4) as u64;
        assert!(
            max_difference <= MAX_CHANNEL_DIFF,
            "frame {frame}: max channel difference {max_difference} exceeds {MAX_CHANNEL_DIFF}"
        );
        assert!(
            differing_pixels * 1000 <= total,
            "frame {frame}: {differing_pixels} of {total} pixels differ by more than \
             {MAX_CHANNEL_DIFF}"
        );
    }
    Ok(())
}

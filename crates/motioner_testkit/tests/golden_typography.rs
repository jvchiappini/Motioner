//! Golden stills for `examples/typography.motion` (D-0034).
//!
//! Three beats of one card: a tracked title (frame 30), a centred multi-line
//! paragraph with an animated line height (frame 150) and a per-character
//! reveal (frame 260).  The committed stills are validated pixel-by-pixel so
//! the fixtures cannot drift silently, and every run renders the beats and
//! compares them with the stills.

use std::error::Error;
use std::fmt::Write as _;

use motioner_eval::{Evaluator, RenderPlan};
use motioner_render::FrameRenderer;
use sha2::{Digest, Sha256};

/// Frame, embedded still and pinned SHA-256.
const GOLDENS: [(i64, &[u8], &str); 3] = [
    (
        30,
        include_bytes!("../golden/typography_tracking_f30.png"),
        "b547dd82e89b769643358c904cdf334d9a95ee6dd66aa6bc6fda6234b57d9814",
    ),
    (
        150,
        include_bytes!("../golden/typography_lines_f150.png"),
        "4096a76ce10a7666aa02ad62931c60b3dd15bb45a6103998d4107b2bc53bb412",
    ),
    (
        260,
        include_bytes!("../golden/typography_chars_f260.png"),
        "6bff238a4b752b82e84a7365e9302414eb4f7cee9fb12d4fb8dfabf6b355e5cd",
    ),
];
const SOURCE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../examples/typography.motion"
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
        .all(|(actual, expected)| actual.abs_diff(expected) <= 4)
}

#[test]
fn each_still_shows_its_typography_beat() -> Result<(), Box<dyn Error>> {
    let background = [11, 15, 22];
    let ink = [242, 245, 250];
    let accent = [90, 176, 255];

    for (frame, golden, sha) in GOLDENS {
        assert_eq!(
            hex(&Sha256::digest(golden)),
            sha,
            "golden frame {frame} changed; regenerate it with `motioner still` and update the hash"
        );
        let (pixels, width, height) = decode(golden)?;
        assert_eq!((width, height), (640, 360));

        match frame {
            30 => {
                // Tracking moved the first glyph left and opened a real gap
                // between "T" and "R": the stem is ink, the gap is background.
                assert!(
                    within(pixel(&pixels, width, 183, 120), ink),
                    "the T stem must be inside the moved glyph"
                );
                assert!(
                    within(pixel(&pixels, width, 203, 120), background),
                    "the gap between T and R must be background"
                );
                // The widest tracked line is wider than the untracked one; the
                // left edge records where the tracking moved the run.
                assert!(
                    within(pixel(&pixels, width, 164, 130), background)
                        && within(pixel(&pixels, width, 170, 130), background),
                    "the run must start near x=164 with tracking"
                );
            }
            150 => {
                // Three lines: the first line's ink ends at row 189, the second
                // starts at 205, so row 200 is the inter-line gap; the second
                // line's baseline area carries ink.
                assert!(
                    within(pixel(&pixels, width, 320, 200), background),
                    "the gap between line 1 and line 2 must be background"
                );
                assert!(
                    !within(pixel(&pixels, width, 320, 220), background),
                    "the second line's baseline row must carry ink"
                );
                assert!(
                    !within(pixel(&pixels, width, 280, 260), background),
                    "the third line must be below the second"
                );
            }
            260 => {
                // The reveal has reached "M", "O" and "T"; "N" is still clear.
                assert!(
                    within(pixel(&pixels, width, 200, 160), accent),
                    "the revealed M must be accent-coloured"
                );
                let o = pixel(&pixels, width, 250, 160);
                assert!(
                    o[2] > 200 && o[0] < 130,
                    "the revealed O must be accent-coloured: {o:?}"
                );
                assert!(
                    within(pixel(&pixels, width, 380, 170), background),
                    "the not-yet-revealed N must still be background"
                );
            }
            _ => panic!("unexpected frame {frame}"),
        }
    }
    Ok(())
}

/// Renders the beats and compares them with the stills.
#[test]
fn typography_matches_the_goldens() -> Result<(), Box<dyn Error>> {
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

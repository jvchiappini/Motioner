//! Golden stills for `examples/effects.motion` (D-0032).
//!
//! One effect is visible per beat: frame 0 shows the drop shadow, frame 30 the
//! blurred group and frame 60 the glow. The committed stills are validated
//! pixel-by-pixel below (so the fixtures cannot drift silently), and every
//! run renders the three frames and compares them with the stills.

use std::error::Error;
use std::fmt::Write as _;

use motioner_eval::{Evaluator, RenderPlan};
use motioner_render::FrameRenderer;
use sha2::{Digest, Sha256};

/// Frame, embedded still and pinned SHA-256.
const GOLDENS: [(i64, &[u8], &str); 3] = [
    (
        0,
        include_bytes!("../golden/effects_shadow_f0.png"),
        "364f4539471e9a05fc362619947dc050b8a32b175bb6dc7107ac648a5d5dd00c",
    ),
    (
        30,
        include_bytes!("../golden/effects_blur_f30.png"),
        "d85d4782c077191c4bd2a12318c2f34a2ee9fff692da02460d7b57d4f1ceb9d6",
    ),
    (
        60,
        include_bytes!("../golden/effects_glow_f60.png"),
        "921ac0802b584ef1c661c21395a744eeedbd099d10c7ae81cd41e300138fd799",
    ),
];
const SOURCE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../examples/effects.motion"
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

fn close(a: [u8; 4], b: [u8; 4]) -> bool {
    a[..3].iter().zip(&b[..3]).all(|(x, y)| x.abs_diff(*y) <= 2)
}

#[test]
fn each_still_shows_only_its_effect() -> Result<(), Box<dyn Error>> {
    let background = [14, 17, 22, 255];
    let shadow_point = (322, 300);
    let glow_point = (780, 250);
    let blur_ramp = [392, 396, 400, 404, 408];
    // Inside the card and under its offset shadow: the shadow sits behind the
    // card, so this pixel is the same on every beat.
    let card_point = (300, 300);
    let mut card_pixels = Vec::new();

    for (frame, golden, sha) in GOLDENS {
        assert_eq!(
            hex(&Sha256::digest(golden)),
            sha,
            "golden frame {frame} changed; regenerate it with `motioner still` and update the hash"
        );
        let (pixels, width, height) = decode(golden)?;
        assert_eq!((width, height), (960, 540));
        assert!(close(pixel(&pixels, width, 30, 30), background));

        let shadow = pixel(&pixels, width, shadow_point.0, shadow_point.1);
        card_pixels.push((frame, pixel(&pixels, width, card_point.0, card_point.1)));
        let glow_outside = pixel(&pixels, width, glow_point.0, glow_point.1);
        let ramp: Vec<u8> = blur_ramp
            .iter()
            .map(|x| pixel(&pixels, width, *x, 232)[2])
            .collect();
        let sharp = ramp[0] + 100 < ramp[4];
        let soft = ramp.windows(2).all(|pair| pair[0] <= pair[1]) && ramp[0] + 40 < ramp[4];

        match frame {
            0 => {
                assert!(
                    shadow[0] + 2 < background[0],
                    "frame 0 must show the drop shadow: {shadow:?}"
                );
                assert!(sharp, "frame 0 has no blur: {ramp:?}");
                assert!(
                    glow_outside[1] <= glow_outside[0] + 20,
                    "frame 0 has no glow: {glow_outside:?}"
                );
            }
            30 => {
                assert!(
                    close(shadow, background),
                    "frame 30 must not show the shadow: {shadow:?}"
                );
                assert!(soft, "frame 30 must show the blur: {ramp:?}");
                assert!(
                    glow_outside[1] <= glow_outside[0] + 20,
                    "frame 30 has no glow: {glow_outside:?}"
                );
            }
            60 => {
                assert!(
                    close(shadow, background),
                    "frame 60 must not show the shadow: {shadow:?}"
                );
                assert!(sharp, "frame 60 has no blur: {ramp:?}");
                assert!(
                    glow_outside[1] > glow_outside[0] + 20
                        && glow_outside[2] > glow_outside[0] + 20,
                    "frame 60 must show the glow: {glow_outside:?}"
                );
                // Outer glow: the dark disc itself is not lit.
                let core = pixel(&pixels, width, 730, 250);
                assert!(
                    close(core, [10, 20, 24, 255]),
                    "frame 60: the glow must not light the disc: {core:?}"
                );
            }
            _ => panic!("unexpected frame {frame}"),
        }
    }
    let (_, reference) = card_pixels[0];
    for (frame, card) in card_pixels {
        assert!(
            close(card, reference) && card[0] > 200,
            "frame {frame}: the drop shadow must not darken the card: {card:?}"
        );
    }
    Ok(())
}

/// Renders the three beats and compares them with the stills.
#[test]
fn effects_match_the_goldens() -> Result<(), Box<dyn Error>> {
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

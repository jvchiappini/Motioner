//! Golden stills for `examples/timing.motion` (D-0037).
//!
//! Frame 30 shows the ease comparison: the hold dot is still at its old
//! position mid-segment and the elastic dot is past the line (its overshoot).
//! Frame 400 shows the fast square smeared by motion blur next to a static
//! reference.
//!
//! The motion-blur comparison needs the `engine-accumulation` feature, so it
//! is compiled only when that feature is on; the pinned hashes and the pixel
//! assertions always run.

use std::error::Error;
use std::fmt::Write as _;

#[cfg(feature = "engine-accumulation")]
use motioner_render::FrameRenderer;
use sha2::{Digest, Sha256};

#[cfg(feature = "engine-accumulation")]
const SOURCE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../examples/timing.motion"
));

const GOLDENS: [(i64, &[u8], &str); 2] = [
    (
        30,
        include_bytes!("../../motioner_testkit/golden/timing_eases_f030.png"),
        "688bde0291cd13b4c7aac67d9100da7e15e6474c9149e8ac790f413dfa931557",
    ),
    (
        400,
        include_bytes!("../../motioner_testkit/golden/timing_motion_blur_f400.png"),
        "c18d50b69f9ebfa4b0dcf9cd4ccbd8d0313c130e72e3880e7559128af3705af9",
    ),
];

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

fn near(actual: [u8; 4], expected: [u8; 3], tolerance: u8) -> bool {
    actual[..3]
        .iter()
        .zip(expected)
        .all(|(got, want)| got.abs_diff(want) <= tolerance)
}

#[test]
fn each_still_shows_its_timing_beat() -> Result<(), Box<dyn Error>> {
    for (frame, golden, sha) in GOLDENS {
        assert_eq!(
            hex(&Sha256::digest(golden)),
            sha,
            "golden frame {frame} changed; regenerate it with `motioner still` and update the hash"
        );
        let (pixels, width, height) = decode(golden)?;
        assert_eq!((width, height), (960, 540));
        match frame {
            30 => {
                // The hold row (index 2, y = 76 + 2*62 = 200) has not moved:
                // the arriving key is at 2.2 s, so at 0.5 s the dot is still
                // at its start x = 300.
                assert!(
                    near(pixel(&pixels, width, 300, 200), [242, 193, 78], 4),
                    "the hold dot must still be at its old position"
                );
                // The elastic row overshoots the line: at 0.5 s it is at
                // x ~ 895, past the 740 px destination.
                assert!(
                    near(pixel(&pixels, width, 895, 262), [106, 209, 227], 6),
                    "the elastic dot must be visible at its overshoot"
                );
                // The linear dot is still near the start.
                assert!(
                    near(pixel(&pixels, width, 370, 76), [169, 180, 196], 6),
                    "the linear dot must be near the start"
                );
            }
            400 => {
                // The fast square travels 700 px in 0.2 s (12 frames), so
                // each sub-frame moves 58.33 px. With shutter 180 and 8
                // samples the sub-frame centres span 0.4375 frames, i.e.
                // ~25.5 px. At x = 555 the square is fully covering in 6 of
                // the 8 sub-frames (coverage 0.75); the expected colour is
                // sRGB(0.75*linear(#FF5A6E) + 0.25*linear(#0B0F16)) =
                // (225, 79, 97).
                assert!(
                    near(pixel(&pixels, width, 555, 380), [225, 79, 97], 4),
                    "the blur streak must show the predicted 0.75 coverage"
                );
                // The static reference card is the same colour inside its
                // border (away from the cyan cross).
                assert!(
                    near(pixel(&pixels, width, 450, 140), [27, 36, 51], 4),
                    "the static reference must not blur"
                );
            }
            _ => panic!("unexpected frame {frame}"),
        }
    }
    Ok(())
}

#[cfg(feature = "engine-accumulation")]
#[test]
fn the_timing_beats_render_like_the_goldens() -> Result<(), Box<dyn Error>> {
    let _gpu = motioner_testkit::gpu::lock();
    let document = motioner_dsl::run(SOURCE, &motioner_dsl::Limits::default())?;
    let mut renderer = FrameRenderer::new(960, 540)?;
    for (frame, golden, _) in GOLDENS {
        let mut actual = Vec::new();
        renderer.render_frame(&document, frame, &mut actual)?;
        let (expected, width, height) = decode(golden)?;
        assert_eq!((width, height), (960, 540));
        let mut max_difference = 0_u8;
        let mut differing_pixels = 0_u64;
        for (actual_pixel, expected_pixel) in actual.chunks_exact(4).zip(expected.chunks_exact(4)) {
            let difference = actual_pixel
                .iter()
                .zip(expected_pixel)
                .map(|(a, b)| a.abs_diff(*b))
                .max()
                .unwrap_or(0);
            max_difference = max_difference.max(difference);
            if difference > 2 {
                differing_pixels += 1;
            }
        }
        assert!(
            max_difference <= 2,
            "frame {frame}: max channel difference {max_difference} exceeds 2"
        );
        assert!(
            differing_pixels * 1000 <= (actual.len() / 4) as u64,
            "frame {frame}: {differing_pixels} pixels differ by more than 2"
        );
    }
    Ok(())
}

/// The static reference is byte-identical with motion blur on and off.
#[cfg(feature = "engine-accumulation")]
#[test]
fn a_static_reference_is_identical_with_motion_blur_on_and_off() -> Result<(), Box<dyn Error>> {
    let _gpu = motioner_testkit::gpu::lock();
    let with_blur = motioner_dsl::run(SOURCE, &motioner_dsl::Limits::default())?;
    let without_source = SOURCE.replace("motion_blur=True", "motion_blur=False");
    let without_blur = motioner_dsl::run(&without_source, &motioner_dsl::Limits::default())?;
    let mut renderer = FrameRenderer::new(960, 540)?;
    let mut on = Vec::new();
    let mut off = Vec::new();
    renderer.render_frame(&with_blur, 400, &mut on)?;
    renderer.render_frame(&without_blur, 400, &mut off)?;
    // The reference card is centred at (480, 160) with a 120 px side; compare
    // its interior, away from the moving square.
    for y in 110_u32..210 {
        for x in 430_u32..530 {
            let offset = usize::try_from((y * 960 + x) * 4).expect("pixel offset fits usize");
            assert_eq!(
                &on[offset..offset + 4],
                &off[offset..offset + 4],
                "pixel ({x}, {y}) changed with motion blur"
            );
        }
    }
    Ok(())
}

//! Headless integration test: the live painter reproduces the export golden.
//!
//! `LivePainter` and `FrameRenderer` share one emission path, so painting
//! `hello.motion` frame 120 through `LivePainter` into an offscreen
//! 1920×1080 `Pure2dHarness` with the identity fit must match the exported
//! golden with the same tolerance the golden test uses (max 2 per channel,
//! at most 0.1 % of pixels above 2).
//!
//! A second case letterboxes the same frame into a 960×540 rect and compares
//! sampled pixels of large flat areas against the golden downscaled by two.
//! Anti-aliased edges are not compared: SDF and MSDF coverage at half scale is
//! intentionally different, while flat interiors are scale-invariant.
//!
//! GPU tests skip cleanly (printing `[skip] ...`) when no adapter exists.

use std::sync::Mutex;

use ferrous_engine::wgpu::{Color as ClearColor, TextureFormat};
use ferrous_renderer::headless::Pure2dHarness;
use motioner_eval::{Evaluator, RenderPlan};
use motioner_render::{LivePainter, RenderError, ViewFit};
use sha2::{Digest, Sha256};

static GPU_LOCK: Mutex<()> = Mutex::new(());

const SOURCE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../examples/hello.motion"
));
const GOLDEN_PNG: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../motioner_testkit/golden/hello_f120.png"
));
const GOLDEN_SHA256: &str = "c2173badd7f23933461473c5d682e40fc9fc1dcbd4ebbe10a1c7c1c7de5cf15c";
const MAX_CHANNEL_DIFF: u8 = 2;

struct Frame {
    document: motioner_core::Document,
    plan: RenderPlan,
}

fn frame_120() -> Frame {
    let document = motioner_dsl::run(SOURCE, &motioner_dsl::Limits::default()).unwrap();
    let mut evaluator = Evaluator::new();
    let mut plan = RenderPlan::new();
    evaluator.evaluate(&document, 120, &mut plan);
    Frame { document, plan }
}

fn harness(width: u32, height: u32) -> Option<Pure2dHarness> {
    match Pure2dHarness::new(width, height, TextureFormat::Rgba8UnormSrgb) {
        Ok(harness) => Some(harness),
        Err(error) => {
            println!("[skip] headless GPU unavailable: {error}");
            None
        }
    }
}

fn linear(value: u8) -> f64 {
    let channel = f64::from(value) / 255.0;
    if channel <= 0.040_45 {
        channel / 12.92
    } else {
        ((channel + 0.055) / 1.055).powf(2.4)
    }
}

/// Paints a plan with the live painter into a fresh harness, clearing to the
/// composition background exactly like `FrameRenderer` does.
fn paint(frame: &Frame, width: u32, height: u32, fit: ViewFit) -> Option<Vec<u8>> {
    let mut harness = harness(width, height)?;
    let background = frame.plan.background();
    harness.set_clear_color(ClearColor {
        r: linear(background.r),
        g: linear(background.g),
        b: linear(background.b),
        a: f64::from(background.a) / 255.0,
    });
    let mut painter = match LivePainter::new(harness.renderer_mut()) {
        Ok(painter) => painter,
        Err(RenderError::Font { message }) => panic!("font registration failed: {message}"),
        Err(other) => panic!("painter creation failed: {other}"),
    };
    harness.begin_frame();
    painter.paint(harness.renderer_mut(), &frame.document, &frame.plan, fit);
    harness.render_frame();
    Some(harness.read_pixels())
}

fn golden_rgba() -> Vec<u8> {
    let decoder = png::Decoder::new(std::io::Cursor::new(GOLDEN_PNG));
    let mut reader = decoder.read_info().unwrap();
    let mut bytes = vec![0_u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut bytes).unwrap();
    assert_eq!(info.color_type, png::ColorType::Rgba);
    assert_eq!(info.bit_depth, png::BitDepth::Eight);
    bytes.truncate(info.buffer_size());
    bytes
}

#[test]
fn live_painter_matches_the_golden_frame() {
    let _gpu = motioner_testkit::gpu::lock();
    let _guard = GPU_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    assert_eq!(
        hex(&Sha256::digest(GOLDEN_PNG)),
        GOLDEN_SHA256,
        "the golden PNG changed"
    );
    let frame = frame_120();
    let Some(actual) = paint(&frame, 1920, 1080, ViewFit::IDENTITY) else {
        return;
    };
    let expected = golden_rgba();
    assert_eq!(expected.len(), actual.len());

    let mut max_difference = 0_u8;
    let mut differing = 0_u64;
    for (actual_pixel, expected_pixel) in actual.chunks_exact(4).zip(expected.chunks_exact(4)) {
        let mut pixel_difference = 0_u8;
        for (actual_channel, expected_channel) in actual_pixel.iter().zip(expected_pixel) {
            let difference = i16::from(*actual_channel) - i16::from(*expected_channel);
            pixel_difference =
                pixel_difference.max(u8::try_from(difference.unsigned_abs()).unwrap_or(u8::MAX));
        }
        max_difference = max_difference.max(pixel_difference);
        if pixel_difference > MAX_CHANNEL_DIFF {
            differing += 1;
        }
    }
    let total = (actual.len() / 4) as u64;
    assert!(
        max_difference <= MAX_CHANNEL_DIFF,
        "max channel difference {max_difference} exceeds {MAX_CHANNEL_DIFF}"
    );
    assert!(
        differing.saturating_mul(1000) <= total,
        "{differing} of {total} pixels differ by more than {MAX_CHANNEL_DIFF}"
    );
}

#[test]
fn letterboxed_live_painter_matches_flat_regions_of_the_golden_at_half_scale() {
    let _gpu = motioner_testkit::gpu::lock();
    let _guard = GPU_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let frame = frame_120();
    let fit = ViewFit::contain(1920.0, 1080.0, [0.0, 0.0, 960.0, 540.0], 960.0, 540.0);
    assert!((fit.scale() - 0.5).abs() < 1e-12);
    let Some(actual) = paint(&frame, 960, 540, fit) else {
        return;
    };
    let expected = golden_rgba();

    // Background corners: flat and scale-invariant.
    for (x, y) in [(10_u32, 10_u32), (949, 529), (500, 10)] {
        let got = pixel(&actual, 960, x, y);
        let want = pixel(&expected, 1920, x * 2, y * 2);
        for channel in 0..3 {
            let difference = i16::from(got[channel]) - i16::from(want[channel]);
            assert!(
                difference.unsigned_abs() <= 6,
                "background ({x}, {y}) channel {channel}: live {got:?} vs golden {want:?}"
            );
        }
    }

    // Circle centres: large flat interiors, so a 2:1 sample is still inside.
    for index in 0..5 {
        let x = 240 + index * 120;
        let y = 410;
        let got = pixel(&actual, 960, x, y);
        let want = pixel(&expected, 1920, x * 2, y * 2);
        for channel in 0..3 {
            let difference = i16::from(got[channel]) - i16::from(want[channel]);
            assert!(
                difference.unsigned_abs() <= 12,
                "circle {index} centre ({x}, {y}) channel {channel}: live {got:?} vs golden {want:?}"
            );
        }
    }
}

fn pixel(pixels: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let index = ((y * width + x) * 4) as usize;
    [
        pixels[index],
        pixels[index + 1],
        pixels[index + 2],
        pixels[index + 3],
    ]
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut text, "{byte:02x}").expect("writing to a String cannot fail");
    }
    text
}

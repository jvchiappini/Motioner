//! Golden stills for `examples/images.motion` (D-0035).
//!
//! Beat 1 (frame 30) shows the same non-square image in contain, cover and
//! fill.  Beat 2 (frame 150) shows the alpha badge with a drop shadow inside a
//! moving mask, at 50 % group opacity with multiply blend over a gradient.
//!
//! The stills are validated pixel-by-pixel so the fixtures cannot drift, and
//! every run renders the beats through the pinned engine and compares them.

use std::error::Error;
use std::fmt::Write as _;
use std::path::Path;

use motioner_core::ImageResolver;
use motioner_eval::{Evaluator, RenderPlan};
use motioner_io::{CachedImages, ImageStore};
use motioner_render::{FrameRenderer, ImageData, ImageSet};
use sha2::{Digest, Sha256};

const SOURCE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../examples/images.motion"
));

const GOLDENS: [(i64, &[u8], &str); 2] = [
    (
        30,
        include_bytes!("../../motioner_testkit/golden/images_fit_f030.png"),
        "2d06d78ad40ebd0798fa9ff2457be0fdb572e309049c0a0f2e4066a38b6ee395",
    ),
    (
        150,
        include_bytes!("../../motioner_testkit/golden/images_composite_f150.png"),
        "1bb924ed63f3ad9917f3adf5c9c7f63992d8b3b4ff8ec66b506b82dabed75008",
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

fn near(pixel: [u8; 4], expected: [u8; 3], tolerance: u8) -> bool {
    pixel[..3]
        .iter()
        .zip(expected)
        .all(|(actual, expected)| actual.abs_diff(expected) <= tolerance)
}

fn project_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/motioner_render has a workspace root")
}

fn image_set(store: &ImageStore) -> ImageSet {
    let mut set = ImageSet::new();
    for image in store.decoded() {
        set.insert(ImageData {
            key: image.key,
            width: image.width,
            height: image.height,
            rgba8: image.rgba8,
            revision: 0,
        });
    }
    set
}

/// Loads the document, resolves its images and returns the document plus the
/// decoded set.
fn load() -> Result<(motioner_core::Document, ImageSet), Box<dyn Error>> {
    let document = motioner_dsl::run(SOURCE, &motioner_dsl::Limits::default())?;
    let mut document = document;
    let cached = CachedImages::new(ImageStore::new(project_root()));
    let issues = document.resolve_images(&cached);
    assert!(issues.is_empty(), "{issues:?}");
    let store = cached.into_inner();
    Ok((document, image_set(&store)))
}

#[test]
fn each_still_shows_its_image_beat() -> Result<(), Box<dyn Error>> {
    let background = [11, 15, 22];
    let card = [21, 28, 39];
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
                // Contain: the image is 2:1 inside a 160x112 box, so it is
                // 160x80 with a letterbox above and below.  The gap is the
                // card, the centre is a stripe.
                assert!(
                    near(pixel(&pixels, width, 120, 110), card, 2),
                    "contain letterbox gap must show the card"
                );
                assert!(
                    !near(pixel(&pixels, width, 120, 160), card, 2),
                    "contain centre must be image"
                );
                // Cover: the crop starts at u=1/7; the left edge texel is the
                // first stripe, not the card.
                let edge = pixel(&pixels, width, 241, 160);
                assert!(
                    edge[0] > edge[1] + 60 && edge[0] > 200,
                    "cover crop edge must be the red stripe: {edge:?}"
                );
                // Fill: the image is stretched to the full box; the top-left
                // corner is the green stripe (the first stripe is red, but the
                // stretched corner samples the second band).
                let corner = pixel(&pixels, width, 525, 110);
                assert!(
                    corner[1] > corner[0] + 40 && corner[1] > 150,
                    "fill stretched corner must be a stripe: {corner:?}"
                );
            }
            150 => {
                // The mask covers x 119..339 at this frame.  At (370, 170) the
                // badge would be drawn if it were not masked, so the pixel must
                // equal the backdrop at the same column (the gradient is
                // horizontal).
                let outside = pixel(&pixels, width, 370, 170);
                let backdrop = pixel(&pixels, width, 370, 75);
                assert!(
                    near(outside, [backdrop[0], backdrop[1], backdrop[2]], 2),
                    "outside the mask the badge must be clipped: {outside:?} vs {backdrop:?}"
                );
                // Inside the mask the teal disc shows (multiplied at 50 %).
                let badge = pixel(&pixels, width, 280, 170);
                assert!(
                    badge[1] > badge[0] + 50 && badge[2] > badge[0] + 60,
                    "inside the mask must show the teal badge: {badge:?}"
                );
                // The badge's anti-aliased bottom edge (a half-transparent
                // texel) sits over its own drop shadow: darker than the bare
                // backdrop in the same column.
                let edge = pixel(&pixels, width, 320, 237);
                let edge_backdrop = pixel(&pixels, width, 320, 60);
                assert!(
                    edge[1] + 20 < edge_backdrop[1],
                    "the edge over the shadow must darken the backdrop: {edge:?} vs {edge_backdrop:?}"
                );
                // Past the badge, the shadow alone still darkens the backdrop.
                let shadow = pixel(&pixels, width, 335, 250);
                let shadow_backdrop = pixel(&pixels, width, 335, 60);
                assert!(
                    shadow[1] + 10 < shadow_backdrop[1],
                    "the drop shadow must darken the backdrop: {shadow:?} vs {shadow_backdrop:?}"
                );
            }
            _ => panic!("unexpected frame {frame}"),
        }
        let _ = background;
    }
    Ok(())
}

#[test]
fn the_beats_render_like_the_goldens() -> Result<(), Box<dyn Error>> {
    let _gpu = motioner_testkit::gpu::lock();
    let (document, images) = load()?;
    let mut plan = RenderPlan::new();
    let mut renderer: Option<FrameRenderer> = None;
    for (frame, golden, _) in GOLDENS {
        Evaluator::new().evaluate(&document, frame, &mut plan);
        let renderer = match &mut renderer {
            Some(renderer) => renderer,
            slot @ None => {
                let mut created = FrameRenderer::new(plan.width(), plan.height())?;
                created.set_images(images.clone());
                slot.insert(created)
            }
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

/// The unresolved path is a typed error, never a crash (the preview draws a
/// placeholder instead).
#[test]
fn a_missing_image_is_reported_and_the_plan_is_a_placeholder() -> Result<(), Box<dyn Error>> {
    let source = "\
comp(name=\"Missing\", width=64, height=64)
badge = image(\"examples/assets/does_not_exist.png\")
";
    let mut document = motioner_dsl::run(source, &motioner_dsl::Limits::default())?;
    let cached = CachedImages::new(ImageStore::new(project_root()));
    let issues = document.resolve_images(&cached);
    assert_eq!(issues.len(), 1);
    assert!(issues[0].source.ends_with("does_not_exist.png"));
    let mut plan = RenderPlan::new();
    Evaluator::new().evaluate(&document, 0, &mut plan);
    assert!(plan.first_unresolved_image_node().is_some());
    let _ = document.first_unresolved_image();
    let _ = ImageResolver::resolve(&cached, "examples/assets/does_not_exist.png");
    Ok(())
}

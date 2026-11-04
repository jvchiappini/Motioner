//! Golden frames of `examples/showcase_pro.motion` (Phase 11).
//!
//! Six beats of the 24 s piece: brand, colour, glass/image, mask/kinetic type,
//! motion and outro. The pinned hashes catch a silently replaced reference;
//! the pixel comparison uses the D-0037/D-0044 GPU tolerance (at most 2 per
//! channel and at most 0.1 % of pixels above it). The same composition is also
//! covered one-feature-per-golden by the Phase 1-9 test files.

use std::error::Error;
use std::fmt::Write as _;
use std::path::Path;

use motioner_io::{CachedImages, ImageStore};
use motioner_render::{FrameRenderer, ImageData, ImageSet};
use sha2::{Digest, Sha256};

const SOURCE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../examples/showcase_pro.motion"
));

const GOLDENS: [(i64, &[u8], &str); 6] = [
    (
        180,
        include_bytes!("../../motioner_testkit/golden/showcase_pro_f180.png"),
        "394e63dfa455fe94426b4ae975512b689069ee2297eb6940ce668cd5d432e88c",
    ),
    (
        330,
        include_bytes!("../../motioner_testkit/golden/showcase_pro_f330.png"),
        "a6a227c54d5a5212d10dce6c3d49766c2226f01fe5e26a567840af14fbd79688",
    ),
    (
        600,
        include_bytes!("../../motioner_testkit/golden/showcase_pro_f600.png"),
        "12c61ef096b2e5f3981a0e80a0c7a3f1cf808b7c2841b7b4331be8620fc0dbe4",
    ),
    (
        810,
        include_bytes!("../../motioner_testkit/golden/showcase_pro_f810.png"),
        "7b322626429208ab7a03d9fc231506aaa72cac32baac4a68d2935b4d5f2dd16c",
    ),
    (
        1086,
        include_bytes!("../../motioner_testkit/golden/showcase_pro_f1086.png"),
        "1560326b5d4d867885a74920ead3c73d4cd67bcce7e4c1123f1e7e10d1c017d0",
    ),
    (
        1320,
        include_bytes!("../../motioner_testkit/golden/showcase_pro_f1320.png"),
        "0be8f9a114f4aef00a43ac73eb8fbc07d38ec8e4995e079e290d76b64fab86e8",
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

fn project_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/motioner_render has a workspace root")
}

fn load() -> Result<(motioner_core::Document, ImageSet), Box<dyn Error>> {
    // The program asks `text_bounds` for the optical baseline of "REVEAL", so
    // it must run with the real layout metrics, exactly like `still`/`render`.
    let metrics = motioner_render::CpuTextMetrics::new(motioner_render::FontSet::bundled());
    let mut document =
        motioner_dsl::run_with_metrics(SOURCE, &motioner_dsl::Limits::default(), &metrics)?;
    let cached = CachedImages::new(ImageStore::new(project_root()));
    let issues = document.resolve_images(&cached);
    assert!(issues.is_empty(), "{issues:?}");
    let store = cached.into_inner();
    let mut images = ImageSet::new();
    for image in store.decoded() {
        images.insert(ImageData {
            key: image.key,
            width: image.width,
            height: image.height,
            rgba8: image.rgba8,
        });
    }
    Ok((document, images))
}

#[test]
fn the_showcase_beats_render_like_the_goldens() -> Result<(), Box<dyn Error>> {
    let _gpu = motioner_testkit::gpu::lock();
    let (document, images) = load()?;
    let mut renderer = FrameRenderer::new(1920, 1080)?;
    renderer.set_images(images);
    for (frame, golden, sha) in GOLDENS {
        assert_eq!(
            hex(&Sha256::digest(golden)),
            sha,
            "golden frame {frame} changed; regenerate it with `motioner still` and update the hash"
        );
        let (expected, width, height) = decode(golden)?;
        assert_eq!((width, height), (1920, 1080));
        let mut actual = Vec::new();
        renderer.render_frame(&document, frame, &mut actual)?;
        assert_eq!(actual.len(), expected.len());
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

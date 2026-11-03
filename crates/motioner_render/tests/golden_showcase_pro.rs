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
        "6dd9e0f558b93acc4ba1d0faf105418f06809fe58243434f2f6c8332a7a81547",
    ),
    (
        330,
        include_bytes!("../../motioner_testkit/golden/showcase_pro_f330.png"),
        "c5a0ab9ef72e53feb145c24f664bb49b70f10ec7a63432de0ea1619c94076989",
    ),
    (
        600,
        include_bytes!("../../motioner_testkit/golden/showcase_pro_f600.png"),
        "ed48c54a9822b77262b6e30ad9d9c99d234400b2faa07baecbb77707d377df30",
    ),
    (
        810,
        include_bytes!("../../motioner_testkit/golden/showcase_pro_f810.png"),
        "7b36dcba2ff57d827830493a10bc81a924b226b9137a3f4e9c6e8a911ee32de3",
    ),
    (
        1086,
        include_bytes!("../../motioner_testkit/golden/showcase_pro_f1086.png"),
        "f01cc207b8ff46abd5808f429952292c365e09b89b2a80c169d1a3de918e6851",
    ),
    (
        1320,
        include_bytes!("../../motioner_testkit/golden/showcase_pro_f1320.png"),
        "1e221faf8a27059fbe75bcc254772276a2fa27dd87fc5c553cc7ddb72630618d",
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
    let mut document = motioner_dsl::run(SOURCE, &motioner_dsl::Limits::default())?;
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

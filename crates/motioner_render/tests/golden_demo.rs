//! Golden frames of `examples/demo.motioner` (prompt 11 phase 7).
//!
//! The demo is the integration example: a project folder with a generated
//! image, a generated 60 fps video and a generated music loop. Two beats are
//! pinned (title + subtitle over the video at 1.5 s, the fade at 5.5 s) and
//! the test asserts the exact video source frame selected at each beat.

use std::error::Error;
use std::fmt::Write as _;
use std::path::Path;

use motioner_core::NodeContent;
use motioner_io::{decode_frame, FfmpegLocation, Project, ProjectImages};
use motioner_render::{CpuTextMetrics, FontSet, FrameRenderer, ImageData, ImageSet};
use sha2::{Digest, Sha256};

const GOLDENS: [(i64, &[u8], &str); 2] = [
    (
        90,
        include_bytes!("../../motioner_testkit/golden/demo_f090.png"),
        "030d0fcbb6858a5eb1aa1a083d0bf707e92c45810ce3aae109bbb5504c46a3ea",
    ),
    (
        330,
        include_bytes!("../../motioner_testkit/golden/demo_f330.png"),
        "142c438be71410ad07b8ab53688efe8cc5521c1e238ecf2f03c7e7ad16c01354",
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

fn load() -> Result<(Project, motioner_core::Document, ImageSet), Box<dyn Error>> {
    let project = Project::open(&project_root().join("examples/demo.motioner"))?;
    let source = std::fs::read_to_string(project.composition_path("main"))?;
    let metrics = CpuTextMetrics::new(FontSet::bundled());
    let mut document =
        motioner_dsl::run_with_metrics(&source, &motioner_dsl::Limits::default(), &metrics)?;
    let resolver = ProjectImages::new(&project);
    assert!(document.resolve_images(&resolver).is_empty());
    assert!(document.resolve_videos(&resolver).is_empty());
    let mut images = ImageSet::new();
    for image in resolver.decoded() {
        images.insert(ImageData {
            key: image.key,
            width: image.width,
            height: image.height,
            rgba8: image.rgba8,
            revision: 0,
        });
    }
    Ok((project, document, images))
}

/// Decodes the original source frame of every video node at `frame`.
fn video_images(
    project: &Project,
    document: &motioner_core::Document,
    frame: i64,
) -> Result<Vec<ImageData>, Box<dyn Error>> {
    let rate = document.composition().frame_rate();
    let mut out = Vec::new();
    for node in document.nodes() {
        let NodeContent::Video(video) = node.content() else {
            continue;
        };
        let Some(asset) = video.asset else {
            continue;
        };
        let Some(source_frame) = video.source_frame(frame, rate) else {
            continue;
        };
        let Some(key) = video.texture_key(node.id()) else {
            continue;
        };
        let path = project
            .find_asset(&video.source)
            .map(|entry| project.assets_dir().join(entry.file_name()))
            .ok_or("the demo video asset is missing")?;
        #[allow(clippy::cast_precision_loss)]
        let seconds = source_frame as f64 / asset.fps.as_f64();
        let rgba8 = decode_frame(
            &path,
            seconds,
            asset.width,
            asset.height,
            &FfmpegLocation::FromEnvOrPath,
        )?;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let revision = frame as u64 + 1;
        out.push(ImageData {
            key,
            width: asset.width,
            height: asset.height,
            rgba8,
            revision,
        });
    }
    Ok(out)
}

fn video_source_frame(document: &motioner_core::Document, frame: i64) -> Option<i64> {
    let rate = document.composition().frame_rate();
    document.nodes().find_map(|node| {
        let NodeContent::Video(video) = node.content() else {
            return None;
        };
        video.source_frame(frame, rate)
    })
}

#[test]
fn the_demo_beats_render_like_the_goldens() -> Result<(), Box<dyn Error>> {
    let _gpu = motioner_testkit::gpu::lock();
    let (project, document, images) = load()?;
    let composition = document.composition();
    assert_eq!((composition.width(), composition.height()), (1280, 720));
    assert_eq!(video_source_frame(&document, 90), Some(90));
    assert_eq!(video_source_frame(&document, 330), Some(330));

    let mut renderer = FrameRenderer::new(1280, 720)?;
    renderer.set_images(images);
    for (frame, golden, sha) in GOLDENS {
        assert_eq!(
            hex(&Sha256::digest(golden)),
            sha,
            "golden frame {frame} changed; regenerate it with `motioner still` and update the hash"
        );
        let (expected, width, height) = decode(golden)?;
        assert_eq!((width, height), (1280, 720));
        for image in video_images(&project, &document, frame)? {
            renderer.set_image(image);
        }
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
        let total = u64::from(width) * u64::from(height);
        assert!(
            max_difference <= 2 && differing_pixels * 1000 <= total,
            "frame {frame}: max difference {max_difference}, {differing_pixels}/{total} pixels"
        );
    }
    Ok(())
}

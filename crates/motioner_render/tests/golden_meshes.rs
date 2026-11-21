//! Golden stills for deformed image and video meshes (prompt 16b E5b).
//!
//! Four scenes exercise the renderer's mesh path end to end:
//!
//! 1. `mesh_lattice`: an image bent by a four-corner lattice.
//! 2. `mesh_video_skin`: a synthetic video frame skinned by two bones.
//! 3. `mesh_arm_ik`: an image arm skinned by two bones whose IK target is
//!    keyed, so the elbow bends and the texture follows.
//! 4. `mesh_blend`: an image bulged by a blend shape.
//!
//! The stills are compared pixel-by-pixel.  Set `MOTIONER_WRITE_GOLDENS=1` to
//! regenerate them into `crates/motioner_testkit/golden/` (and copy the report
//! screenshots under `out/reports/phase16b/`).

use std::error::Error;
use std::fmt::Write as _;
use std::path::Path;

use motioner_core::{ImageKey, NodeContent, VideoAsset, VideoResolver};
use motioner_render::{FrameRenderer, ImageData, ImageSet};
use sha2::{Digest, Sha256};

const WIDTH: u32 = 480;
const HEIGHT: u32 = 270;
const FRAME: i64 = 30;

const LATTICE: &str = r##"
comp(name="MeshLattice", width=480, height=270, fps=60, duration=2.0,
     background="#101820")
art = use("examples/rig/assets/mascot.svg")
photo = image("examples/assets/stripes.png", width=300, height=180, fit="fill", mesh=(6, 4))
photo.pos = (240, 135)
m = rig(art, pos=(-4000, -4000))
m.prop("tl", (0.0, 0.0))
m.prop("tr", (0.0, 0.0))
m.prop("br", (0.0, 0.0))
m.prop("bl", (0.0, 0.0))
m.part("photo", photo)
m.lattice("photo", corners=("tl", "tr", "br", "bl"))
m.tl.key(0.0, (-40.0, -30.0))
m.tr.key(0.0, (50.0, -50.0))
m.br.key(0.0, (60.0, 40.0))
m.bl.key(0.0, (-50.0, 30.0))
"##;

const VIDEO_SKIN: &str = r##"
comp(name="MeshVideoSkin", width=480, height=270, fps=60, duration=2.0,
     background="#101820")
art = use("examples/rig/assets/mascot.svg")
clip = video("clip.mp4", width=320, height=160, mesh=(6, 3))
clip.pos = (240, 135)
m = rig(art, pos=(-4000, -4000))
m.prop("bend", 0.0, range=(-90.0, 90.0))
m.part("clip", clip)
m.bone("root", at=(-160.0, 0.0), length=80.0)
m.bone("tip", parent="root", at=(160.0, 0.0), length=80.0, rotation_prop="bend")
m.skin("clip")
m.bend.key(0.0, 0.0)
m.bend.key(1.0, -55.0)
"##;

const ARM_IK: &str = r##"
comp(name="MeshArmIk", width=480, height=270, fps=60, duration=2.0,
     background="#101820")
art = use("examples/rig/assets/mascot.svg")
arm = image("examples/assets/stripes.png", width=320, height=80, fit="fill", mesh=(9, 2))
arm.pos = (250, 160)
arm_group = group()
arm_group.blend = "screen"
arm_group.add(arm)
m = rig(art, pos=(-4000, -4000))
m.prop("target", (0.0, 0.0))
m.prop("pole", (0.0, -300.0))
m.part("arm", arm)
m.bone("root", at=(-160.0, 0.0), length=80.0)
m.bone("tip", parent="root", at=(160.0, 0.0), length=80.0)
m.skin("arm")
m.ik("bone:root", "bone:tip", target="target", pole="pole", tip=160.0)
m.target.key(0.0, (80.0, -40.0))
m.target.key(1.0, (80.0, 60.0))
"##;

const BLEND: &str = r##"
comp(name="MeshBlend", width=480, height=270, fps=60, duration=2.0,
     background="#101820")
art = use("examples/rig/assets/mascot.svg")
photo = image("examples/assets/stripes.png", width=300, height=160, fit="fill", mesh=(4, 3))
photo.pos = (240, 135)
m = rig(art, pos=(-4000, -4000))
m.prop("open", 0.0, range=(0.0, 1.0))
m.part("photo", photo)
m.blend("photo", points=[[-150.0, -80.0], [-60.0, -80.0], [60.0, -80.0], [150.0, -80.0], [-150.0, 0.0], [-70.0, 50.0], [70.0, 50.0], [150.0, 0.0], [-150.0, 80.0], [-60.0, 80.0], [60.0, 80.0], [150.0, 80.0]], weight="open", fit=False)
m.open.key(0.0, 1.0)
"##;

const CROP_SPRITE: &str = r##"
comp(name="MeshCropSprite", width=480, height=270, fps=60, duration=1.0,
     background="#101820")
photo = image("examples/assets/stripes.png", width=300, height=120, fit="cover")
photo.pos = (240, 135)
"##;

const CROP_MESH: &str = r##"
comp(name="MeshCropMesh", width=480, height=270, fps=60, duration=1.0,
     background="#101820")
photo = image("examples/assets/stripes.png", width=300, height=120, fit="cover", mesh=(4, 4))
photo.pos = (240, 135)
"##;

/// A synthetic video asset: the golden inserts its decoded frame by hand.
struct SyntheticVideo;

impl VideoResolver for SyntheticVideo {
    fn resolve_video(&self, _source: &str) -> Result<VideoAsset, String> {
        Ok(VideoAsset {
            key: ImageKey::from_bytes([0x5a; 32]),
            width: 320,
            height: 160,
            fps: motioner_core::FrameRate::new(60, 1).expect("60 fps"),
            duration: 4.0,
        })
    }
}

fn project_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/motioner_render has a workspace root")
}

fn write_png(path: &Path, pixels: &[u8]) -> Result<(), Box<dyn Error>> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = std::fs::File::create(path)?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), WIDTH, HEIGHT);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(pixels)?;
    Ok(())
}

/// A 320x160 checker with a border, used as the synthetic video frame.
fn video_frame() -> ImageData {
    let mut rgba8 = Vec::with_capacity(320 * 160 * 4);
    for y in 0..160 {
        for x in 0..320 {
            let border = x < 3 || y < 3 || x >= 317 || y >= 157;
            let cell = ((x / 16) + (y / 16)) % 2 == 0;
            let color = if border {
                [250, 250, 245, 255]
            } else if cell {
                [90, 200, 120, 255]
            } else {
                [30, 70, 190, 255]
            };
            rgba8.extend_from_slice(&color);
        }
    }
    ImageData {
        key: ImageKey::from_bytes([0x5a; 32]),
        width: 320,
        height: 160,
        rgba8,
        revision: 1,
    }
}

/// Builds the document, resolves its image/video assets and renders one frame.
fn render(source: &str) -> Result<Vec<u8>, Box<dyn Error>> {
    let assets = motioner_io::ProjectAssets::new(project_root());
    let mut document = motioner_dsl::run_with_assets(
        source,
        &motioner_dsl::Limits::default(),
        &motioner_core::NoTextMetrics,
        &assets,
    )?;

    let store = motioner_io::ImageStore::new(project_root());
    let cached = motioner_io::CachedImages::new(store);
    let issues = document.resolve_images(&cached);
    assert!(issues.is_empty(), "{issues:?}");
    let mut images = ImageSet::new();
    for image in cached.into_inner().decoded() {
        images.insert(ImageData {
            key: image.key,
            width: image.width,
            height: image.height,
            rgba8: image.rgba8,
            revision: 0,
        });
    }

    if document
        .nodes()
        .any(|node| matches!(node.content(), NodeContent::Video(_)))
    {
        assert!(document.resolve_videos(&SyntheticVideo).is_empty());
        let frame = video_frame();
        let node = document
            .nodes()
            .find(|node| matches!(node.content(), NodeContent::Video(_)))
            .expect("the scene has a video");
        let NodeContent::Video(video) = node.content() else {
            unreachable!()
        };
        let key = video
            .texture_key(node.id())
            .expect("the video has an asset");
        images.insert(ImageData {
            key,
            revision: 1,
            ..frame
        });
    }

    let mut renderer = FrameRenderer::new(WIDTH, HEIGHT)?;
    renderer.set_images(images);
    let mut pixels = Vec::new();
    renderer.render_frame(&document, FRAME, &mut pixels)?;
    Ok(pixels)
}

fn hex(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut text, "{byte:02x}").expect("writing to a String cannot fail");
    }
    text
}

/// Writes the golden PNGs and the report screenshots.  Run with
/// `MOTIONER_WRITE_GOLDENS=1 cargo test -p motioner_render --test golden_meshes
/// -- --ignored --nocapture`.
#[test]
#[ignore = "writes the golden PNGs; run explicitly with MOTIONER_WRITE_GOLDENS=1"]
fn write_goldens() -> Result<(), Box<dyn Error>> {
    if std::env::var("MOTIONER_WRITE_GOLDENS").as_deref() != Ok("1") {
        return Ok(());
    }
    let _gpu = motioner_testkit::gpu::lock();
    let golden_dir = project_root().join("crates/motioner_testkit/golden");
    let report_dir = project_root().join("out/reports/phase16b");
    for (name, source) in [
        ("mesh_lattice", LATTICE),
        ("mesh_video_skin", VIDEO_SKIN),
        ("mesh_arm_ik", ARM_IK),
        ("mesh_blend", BLEND),
    ] {
        let pixels = render(source)?;
        let file = format!("{name}_f{FRAME:03}.png");
        write_png(&golden_dir.join(&file), &pixels)?;
        write_png(&report_dir.join(format!("e5b_{name}.png")), &pixels)?;
        println!("{file} sha256 {}", hex(&Sha256::digest(&pixels)));
    }
    Ok(())
}

/// The scenes only need the golden hash to be checked once the stills exist.
/// Runs when the golden files are present; otherwise it prints the command to
/// regenerate them (the gate runs after `write_goldens` in this change).
#[test]
fn the_mesh_scenes_render_like_the_goldens() -> Result<(), Box<dyn Error>> {
    let _gpu = motioner_testkit::gpu::lock();
    let golden_dir = project_root().join("crates/motioner_testkit/golden");
    for (name, source) in [
        ("mesh_lattice", LATTICE),
        ("mesh_video_skin", VIDEO_SKIN),
        ("mesh_arm_ik", ARM_IK),
        ("mesh_blend", BLEND),
    ] {
        let path = golden_dir.join(format!("{name}_f{FRAME:03}.png"));
        assert!(
            path.exists(),
            "{} is missing; run MOTIONER_WRITE_GOLDENS=1 cargo test -p motioner_render --test golden_meshes -- --ignored --nocapture",
            path.display()
        );
        let expected = decode(&std::fs::read(&path)?)?;
        let actual = render(source)?;
        assert_eq!(actual.len(), expected.len(), "{name}");
        let mut max_difference = 0_u8;
        let mut differing = 0_u64;
        for (a, b) in actual.chunks_exact(4).zip(expected.chunks_exact(4)) {
            let difference = a
                .iter()
                .zip(b)
                .map(|(x, y)| x.abs_diff(*y))
                .max()
                .unwrap_or(0);
            max_difference = max_difference.max(difference);
            if difference > 2 {
                differing += 1;
            }
        }
        let total = u64::from(WIDTH) * u64::from(HEIGHT);
        assert!(
            max_difference <= 2 && differing * 1000 <= total,
            "{name}: max difference {max_difference}, {differing}/{total} pixels"
        );
    }
    Ok(())
}

/// An undeformed N x M mesh maps its UVs into the node crop exactly like the
/// sprite path: the same `fit="cover"` image renders identically whether it
/// goes through the mesh pipeline or the sprite quad (documented tolerance
/// 3 units, the shared triangle diagonal and edge sampling).
#[test]
fn a_cropped_mesh_matches_the_sprite_crop() -> Result<(), Box<dyn Error>> {
    let _gpu = motioner_testkit::gpu::lock();
    let sprite = render(CROP_SPRITE)?;
    let mesh = render(CROP_MESH)?;
    assert_eq!(sprite.len(), mesh.len());
    let mut max_difference = 0_u8;
    let mut differing = 0_u64;
    for (a, b) in mesh.chunks_exact(4).zip(sprite.chunks_exact(4)) {
        let difference = a
            .iter()
            .zip(b)
            .map(|(x, y)| x.abs_diff(*y))
            .max()
            .unwrap_or(0);
        max_difference = max_difference.max(difference);
        if difference > 2 {
            differing += 1;
        }
    }
    let total = u64::from(WIDTH) * u64::from(HEIGHT);
    assert!(
        max_difference <= 3 && differing * 1000 <= total,
        "cropped mesh vs sprite: max difference {max_difference}, {differing}/{total} pixels"
    );
    Ok(())
}
fn decode(golden: &[u8]) -> Result<Vec<u8>, Box<dyn Error>> {
    let decoder = png::Decoder::new(std::io::Cursor::new(golden));
    let mut reader = decoder.read_info()?;
    let mut pixels = vec![0_u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut pixels)?;
    pixels.truncate(info.buffer_size());
    assert_eq!(info.color_type, png::ColorType::Rgba);
    assert_eq!(info.bit_depth, png::BitDepth::Eight);
    Ok(pixels)
}

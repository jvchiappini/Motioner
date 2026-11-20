//! Per-drawn-image proxy sizing (prompt 18 P4).
//!
//! The canvas used to size a proxy from the composition rectangle, so a 4K
//! image zoomed in beyond the fit scale magnified the smaller texture.  The
//! `PerImage` policy computes the texels one drawn item resolves instead.  At
//! a view scale x world scale product of 4.0 a 4K source needs its full 4K
//! texture, so the proxied frame must equal a `Full` render byte for byte.
//!
//! GPU tests skip cleanly (printing `[skip] ...`) when no adapter exists.

use motioner_core::{Document, ImageAsset, ImageResolver};
use motioner_eval::{Evaluator, RenderPlan};
use motioner_render::{FrameRenderer, ImageData, ImageSet, ProxyTarget, RenderError};

const WIDTH: u32 = 640;
const HEIGHT: u32 = 360;
const SOURCE_WIDTH: u32 = 3840;
const SOURCE_HEIGHT: u32 = 2160;
const KEY: [u8; 32] = [42; 32];

fn document(scale: f64) -> Document {
    let source = format!(
        "comp(name=\"Proxy\", width={WIDTH}, height={HEIGHT}, fps=60, duration=1, \
         background=\"#000000\")\n\
         photo = image(\"photo.png\", width=960, height=540)\n\
         photo.pos = (320, 180)\n\
         photo.scale = ({scale}, {scale})\n"
    );
    motioner_dsl::run(&source, &motioner_dsl::Limits::default())
        .expect("the proxy test document builds")
}

/// Resolves the generated source without touching the file system.
struct GeneratedImage {
    asset: ImageAsset,
}

impl ImageResolver for GeneratedImage {
    fn resolve(&self, _source: &str) -> Result<ImageAsset, String> {
        Ok(self.asset)
    }
}

fn resolved(scale: f64) -> (Document, RenderPlan) {
    let mut document = document(scale);
    let resolver = GeneratedImage {
        asset: ImageAsset {
            key: motioner_core::ImageKey::from_bytes(KEY),
            width: SOURCE_WIDTH,
            height: SOURCE_HEIGHT,
        },
    };
    assert!(document.resolve_images(&resolver).is_empty());
    let mut evaluator = Evaluator::new();
    let mut plan = RenderPlan::new();
    evaluator.evaluate(&document, 0, &mut plan);
    (document, plan)
}

#[allow(clippy::cast_possible_truncation)] // the masks keep every value in u8
fn image_set() -> ImageSet {
    let mut pixels = vec![
        0_u8;
        usize::try_from(u64::from(SOURCE_WIDTH) * u64::from(SOURCE_HEIGHT) * 4)
            .expect("a 4K RGBA buffer fits usize")
    ];
    for y in 0..SOURCE_HEIGHT {
        for x in 0..SOURCE_WIDTH {
            let index =
                usize::try_from((u64::from(y) * u64::from(SOURCE_WIDTH) + u64::from(x)) * 4)
                    .expect("pixel index fits usize");
            pixels[index] = (x & 0xFF) as u8;
            pixels[index + 1] = (y & 0xFF) as u8;
            pixels[index + 2] = ((x / 7 + y / 11) & 0xFF) as u8;
            pixels[index + 3] = 255;
        }
    }
    let mut set = ImageSet::new();
    set.insert(ImageData {
        key: motioner_core::ImageKey::from_bytes(KEY),
        width: SOURCE_WIDTH,
        height: SOURCE_HEIGHT,
        rgba8: pixels,
        revision: 0,
    });
    set
}

fn renderer(proxy: ProxyTarget) -> Option<FrameRenderer> {
    match FrameRenderer::new(WIDTH, HEIGHT) {
        Ok(mut renderer) => {
            renderer.set_proxy_target(proxy);
            Some(renderer)
        }
        Err(RenderError::NoAdapter { message }) => {
            println!("[skip] headless GPU unavailable: {message}");
            None
        }
        Err(other) => panic!("renderer creation failed: {other}"),
    }
}

#[test]
fn a_four_k_image_at_four_hundred_percent_matches_the_full_upload() {
    let _gpu = motioner_testkit::gpu::lock();
    let Some(mut proxied) = renderer(ProxyTarget::PerImage) else {
        return;
    };
    let Some(mut full) = renderer(ProxyTarget::Full) else {
        return;
    };
    let images = image_set();
    proxied.set_images(images.clone());
    full.set_images(images);

    // Seed a small proxy at scale 1: the narrow box (960x540) resolves fewer
    // texels than the 4K source.
    let (small_document, small_plan) = resolved(1.0);
    let mut small = Vec::new();
    proxied
        .render(&small_document, &small_plan, &mut small)
        .expect("the scale-1 frame renders");

    // Zoom to 400 %: the needed size is now 3840x2160, so the painter must
    // free the small texture and register the full one.  If it reused the
    // small texture the result would be a magnified 960x540 proxy.
    let (document, plan) = resolved(4.0);
    let mut actual = Vec::new();
    proxied
        .render(&document, &plan, &mut actual)
        .expect("the 400 % frame renders");

    let mut expected = Vec::new();
    full.render(&document, &plan, &mut expected)
        .expect("the Full reference renders");

    assert_eq!(actual.len(), expected.len());
    assert_eq!(
        actual, expected,
        "a 4K image at view scale x world scale = 4.0 must upload its source"
    );
}

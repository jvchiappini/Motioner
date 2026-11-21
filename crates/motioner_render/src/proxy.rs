//! Proxy sizing and box-filter downsampling for interactive image uploads.
//!
//! An image texture only has to be as large as the pixels the preview actually
//! shows.  [`ProxyTarget`](crate::ProxyTarget) carries that policy; this module
//! turns one decoded [`ImageData`](crate::ImageData) into the bytes that go to
//! the GPU.
//!
//! [`ProxyTarget::Full`] uploads every image at its natural size and is the
//! default for export and the motion-blur preview.  [`ProxyTarget::PerImage`]
//! is the interactive canvas policy: [`needed_size`] computes, per drawn item,
//! the texels the item can actually resolve, and [`target_size`] adds
//! hysteresis so a zoom does not re-upload on every wheel step.
//!
//! The output keeps straight-alpha sRGB RGBA8 and the same UV convention as
//! the source: sprite `uv_rect`s and mesh UVs are normalised, so a smaller
//! texture maps exactly like the full-size one.  Resampling averages in
//! premultiplied space, which avoids the dark fringe a straight-alpha average
//! would leave on transparent edges.

use crate::{ImageData, ProxyTarget};

/// Growth hysteresis: the needed size must exceed the resident size by this
/// factor before the texture is re-uploaded larger.
pub(crate) const GROW_FACTOR: f64 = 1.25;
/// Shrink hysteresis: the resident size must exceed the needed size by this
/// factor before it is replaced with a smaller upload.
pub(crate) const SHRINK_FACTOR: f64 = 2.0;

/// The bytes of one image ready for an upload.
pub(crate) struct Upload<'a> {
    /// Texture width in pixels.
    pub width: u32,
    /// Texture height in pixels.
    pub height: u32,
    /// Straight-alpha sRGB RGBA8.
    pub rgba8: &'a [u8],
}

/// Prepares `image` at exactly `width x height`, using `scratch` when it must
/// be resampled.
///
/// A size equal to the natural one borrows the source bytes.  A malformed
/// source (or a released CPU buffer) returns the source unchanged, so the
/// caller can report a typed upload error instead of uploading garbage.
pub(crate) fn prepare_at<'a>(
    image: &'a ImageData,
    width: u32,
    height: u32,
    scratch: &'a mut Vec<u8>,
) -> Upload<'a> {
    let source = Upload {
        width: image.width,
        height: image.height,
        rgba8: &image.rgba8,
    };
    if (width, height) == (image.width, image.height) || image.rgba8.is_empty() {
        return source;
    }
    if resample(image, width, height, scratch) {
        Upload {
            width,
            height,
            rgba8: scratch.as_slice(),
        }
    } else {
        source
    }
}

/// The fixed upload size of a whole-image policy.
///
/// [`ProxyTarget::PerImage`] resolves to the natural size here; callers that
/// implement it compute the size from the drawn item instead.
pub(crate) fn whole_image_size(image: &ImageData, target: ProxyTarget) -> (u32, u32) {
    match target {
        ProxyTarget::Full | ProxyTarget::PerImage => (image.width, image.height),
        ProxyTarget::Fit {
            max_width,
            max_height,
        } => fit_size(image.width, image.height, max_width, max_height)
            .unwrap_or((image.width, image.height)),
    }
}

/// The texels `natural` must keep to show one drawn item without magnifying.
///
/// `destination` is the item's local box in composition pixels, `world_scale`
/// the item transform's per-axis scale (the lengths of the two transformed
/// basis vectors) and `view_scale` the composition-pixel to physical-pixel
/// scale (zoom times DPI).  `uv` crops the source, so a cropped item needs the
/// full-source width `destination * scale / crop`.  Per-axis scales matter:
/// a node stretched 4x horizontally and left alone vertically needs 4x texels
/// on one axis only, and a single geometric mean would under-resolve it.
///
/// The result is capped at `natural` per axis: a proxy never upscales, and an
/// item already larger than its source is uploaded at full resolution.
pub(crate) fn needed_size(
    natural: (u32, u32),
    uv: [f64; 4],
    destination: (f64, f64),
    world_scale: (f64, f64),
    view_scale: f64,
) -> (u32, u32) {
    if natural.0 == 0 || natural.1 == 0 {
        return natural;
    }
    let crop_width = uv[2] - uv[0];
    let crop_height = uv[3] - uv[1];
    let scale_x = world_scale.0 * view_scale;
    let scale_y = world_scale.1 * view_scale;
    if !(scale_x.is_finite()
        && scale_x > 0.0
        && scale_y.is_finite()
        && scale_y > 0.0
        && crop_width.is_finite()
        && crop_width > 0.0
        && crop_height.is_finite()
        && crop_height > 0.0
        && destination.0.is_finite()
        && destination.0 > 0.0
        && destination.1.is_finite()
        && destination.1 > 0.0)
    {
        return natural;
    }
    (
        needed_side(destination.0, scale_x / crop_width, natural.0),
        needed_side(destination.1, scale_y / crop_height, natural.1),
    )
}

/// One axis of [`needed_size`], capped at `natural`.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // value < natural
fn needed_side(destination: f64, factor: f64, natural: u32) -> u32 {
    let value = (destination * factor).ceil();
    if !value.is_finite() || value >= f64::from(natural) {
        return natural;
    }
    (value.max(1.0) as u64).min(u64::from(natural)) as u32
}

/// Chooses the texture size for `needed` given the resident `current` size.
///
/// A larger `needed` wins only past [`GROW_FACTOR`]; a smaller one replaces the
/// resident texture only past [`SHRINK_FACTOR`].  Between the two the resident
/// texture is reused, which is what keeps a continuous zoom from re-uploading
/// (and, once the CPU pixels are released, re-decoding) on every frame.
pub(crate) fn target_size(current: Option<(u32, u32)>, needed: (u32, u32)) -> (u32, u32) {
    let Some(current) = current else {
        return needed;
    };
    if factor_exceeds(needed, current, GROW_FACTOR)
        || factor_exceeds(current, needed, SHRINK_FACTOR)
    {
        needed
    } else {
        current
    }
}

/// Whether either axis of `larger` exceeds the matching axis of `smaller` by
/// `factor`.
fn factor_exceeds(larger: (u32, u32), smaller: (u32, u32), factor: f64) -> bool {
    let larger_width = f64::from(larger.0);
    let larger_height = f64::from(larger.1);
    let smaller_width = f64::from(smaller.0.max(1));
    let smaller_height = f64::from(smaller.1.max(1));
    larger_width > smaller_width * factor || larger_height > smaller_height * factor
}

/// The largest `width x height` box that fits `max_width x max_height`.
///
/// Returns `None` when the image already fits (never upscale) or when either
/// side is zero.
pub(crate) fn fit_size(
    width: u32,
    height: u32,
    max_width: u32,
    max_height: u32,
) -> Option<(u32, u32)> {
    if width == 0 || height == 0 {
        return None;
    }
    let max_width = max_width.max(1);
    let max_height = max_height.max(1);
    if width <= max_width && height <= max_height {
        return None;
    }
    #[allow(clippy::cast_precision_loss)] // image sides stay below 2^24
    let scale = f64::min(
        f64::from(max_width) / f64::from(width),
        f64::from(max_height) / f64::from(height),
    );
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let fitted_width =
        ((f64::from(width) * scale).round() as u64).clamp(1, u64::from(max_width)) as u32;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let fitted_height =
        ((f64::from(height) * scale).round() as u64).clamp(1, u64::from(max_height)) as u32;
    Some((fitted_width, fitted_height))
}

/// Box-filter downscale of `image` into `out`.
///
/// Returns `false` when the source buffer does not describe `image`'s size.
#[allow(clippy::cast_possible_truncation, clippy::cast_lossless)] // see the body
fn resample(image: &ImageData, width: u32, height: u32, out: &mut Vec<u8>) -> bool {
    let source_width = image.width;
    let source_height = image.height;
    let source_len = u64::from(source_width) * u64::from(source_height) * 4;
    if source_width == 0
        || source_height == 0
        || width == 0
        || height == 0
        || image.rgba8.len() as u64 != source_len
    {
        return false;
    }
    let Some(out_len) = (u64::from(width) * u64::from(height) * 4).try_into().ok() else {
        return false;
    };
    out.clear();
    out.resize(out_len, 0);

    for y in 0..height {
        let rows = u64::from(source_height);
        let y_begin = u64::from(y) * rows / u64::from(height);
        let y_end = ((u64::from(y) + 1) * rows)
            .div_ceil(u64::from(height))
            .min(rows)
            .max(y_begin + 1);
        for x in 0..width {
            let columns = u64::from(source_width);
            let x_begin = u64::from(x) * columns / u64::from(width);
            let x_end = ((u64::from(x) + 1) * columns)
                .div_ceil(u64::from(width))
                .min(columns)
                .max(x_begin + 1);
            let mut alpha = 0_u64;
            let mut red = 0_u64;
            let mut green = 0_u64;
            let mut blue = 0_u64;
            let mut count = 0_u64;
            for source_y in y_begin..y_end {
                for source_x in x_begin..x_end {
                    let index = ((source_y * u64::from(source_width) + source_x) * 4) as usize;
                    let a = u64::from(image.rgba8[index + 3]);
                    alpha += a;
                    red += u64::from(image.rgba8[index]) * a;
                    green += u64::from(image.rgba8[index + 1]) * a;
                    blue += u64::from(image.rgba8[index + 2]) * a;
                    count += 1;
                }
            }
            let (r, g, b, a) = if alpha == 0 || count == 0 {
                (0, 0, 0, 0)
            } else {
                (
                    (red / alpha) as u8,
                    (green / alpha) as u8,
                    (blue / alpha) as u8,
                    (alpha / count) as u8,
                )
            };
            let target = ((u64::from(y) * u64::from(width) + u64::from(x)) * 4) as usize;
            out[target] = r;
            out[target + 1] = g;
            out[target + 2] = b;
            out[target + 3] = a;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use motioner_core::ImageKey;

    fn image(width: u32, height: u32) -> ImageData {
        ImageData {
            key: ImageKey::from_bytes([3; 32]),
            width,
            height,
            rgba8: vec![255; usize::try_from(width * height * 4).unwrap_or(0)],
            revision: 0,
        }
    }

    #[test]
    fn fit_never_upscales() {
        assert_eq!(fit_size(400, 200, 800, 800), None);
        assert_eq!(fit_size(400, 200, 200, 200), Some((200, 100)));
        assert_eq!(fit_size(0, 200, 200, 200), None);
    }

    #[test]
    fn fit_keeps_the_aspect_ratio_within_the_box() {
        let (width, height) = fit_size(1920, 1080, 640, 640).unwrap();
        assert_eq!((width, height), (640, 360));
    }

    #[test]
    fn needed_size_scales_with_the_destination() {
        assert_eq!(
            needed_size(
                (1000, 500),
                [0.0, 0.0, 1.0, 1.0],
                (250.0, 125.0),
                (1.0, 1.0),
                1.0
            ),
            (250, 125)
        );
    }

    #[test]
    fn needed_size_folds_zoom_and_dpi_into_the_view_scale() {
        // 400 % zoom on a 1x display: 250 logical pixels need 1000 texels.
        assert_eq!(
            needed_size(
                (1000, 500),
                [0.0, 0.0, 1.0, 1.0],
                (250.0, 125.0),
                (1.0, 1.0),
                4.0
            ),
            (1000, 500)
        );
        // 200 % zoom at 200 % DPI is also a 4x physical scale.
        assert_eq!(
            needed_size(
                (1000, 500),
                [0.0, 0.0, 1.0, 1.0],
                (250.0, 125.0),
                (1.0, 1.0),
                2.0 * 2.0
            ),
            (1000, 500)
        );
    }

    #[test]
    fn needed_size_accounts_for_the_source_crop() {
        // Half the source width is visible, so the full texture must be twice
        // the destination width to stay pixel-exact.
        assert_eq!(
            needed_size(
                (1000, 500),
                [0.25, 0.0, 0.75, 1.0],
                (500.0, 250.0),
                (1.0, 1.0),
                1.0
            ),
            (1000, 250)
        );
    }

    #[test]
    fn needed_size_uses_each_axis_scale_independently() {
        // A node stretched 4x horizontally and left alone vertically needs 4x
        // texels on the horizontal axis only: a geometric mean would halve it.
        assert_eq!(
            needed_size(
                (1000, 500),
                [0.0, 0.0, 1.0, 1.0],
                (250.0, 500.0),
                (4.0, 1.0),
                1.0
            ),
            (1000, 500)
        );
    }

    #[test]
    fn needed_size_never_upscales_past_the_source() {
        assert_eq!(
            needed_size(
                (1000, 500),
                [0.0, 0.0, 1.0, 1.0],
                (800.0, 400.0),
                (4.0, 4.0),
                4.0
            ),
            (1000, 500)
        );
    }

    #[test]
    fn needed_size_keeps_the_full_source_for_degenerate_inputs() {
        assert_eq!(
            needed_size(
                (800, 600),
                [0.0, 0.0, 1.0, 1.0],
                (0.0, 0.0),
                (1.0, 1.0),
                1.0
            ),
            (800, 600)
        );
        assert_eq!(
            needed_size(
                (800, 600),
                [0.0, 0.0, 0.0, 0.0],
                (100.0, 100.0),
                (1.0, 1.0),
                1.0
            ),
            (800, 600)
        );
        assert_eq!(
            needed_size(
                (800, 600),
                [0.0, 0.0, 1.0, 1.0],
                (100.0, 100.0),
                (f64::NAN, 1.0),
                1.0
            ),
            (800, 600)
        );
    }

    #[test]
    fn target_size_grows_past_the_hysteresis_and_shrinks_only_past_two_times() {
        assert_eq!(target_size(None, (400, 200)), (400, 200));
        // Exactly 1.25x is not enough to grow.
        assert_eq!(target_size(Some((400, 200)), (500, 250)), (400, 200));
        assert_eq!(target_size(Some((400, 200)), (520, 260)), (520, 260));
        // 1.6x smaller keeps the larger texture.
        assert_eq!(target_size(Some((800, 400)), (500, 250)), (800, 400));
        // Past 2x shrinks once.
        assert_eq!(target_size(Some((1000, 500)), (400, 200)), (400, 200));
    }

    #[test]
    fn a_resample_preserves_straight_alpha() {
        let mut source = image(2, 2);
        // Left column opaque red, right column transparent.
        source.rgba8 = vec![255, 0, 0, 255, 255, 0, 0, 255, 0, 0, 0, 0, 0, 0, 0, 0];
        let mut out = Vec::new();
        assert!(resample(&source, 1, 1, &mut out));
        assert_eq!(out.len(), 4);
        // The average alpha is 127 (two opaque, two transparent), and the
        // red stays unpremultiplied.
        assert_eq!(out[3], 127);
        assert_eq!(out[0], 255);
    }

    #[test]
    fn prepare_at_borrows_the_source_when_it_fits() {
        let source = image(8, 8);
        let mut scratch = Vec::new();
        let upload = prepare_at(&source, 8, 8, &mut scratch);
        assert_eq!((upload.width, upload.height), (8, 8));
        assert!(std::ptr::eq(upload.rgba8.as_ptr(), source.rgba8.as_ptr()));
        assert!(scratch.is_empty());
    }
}

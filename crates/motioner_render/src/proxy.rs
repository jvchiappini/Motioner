//! Proxy sizing and box-filter downsampling for interactive image uploads.
//!
//! An image texture only has to be as large as the pixels the preview actually
//! shows.  [`ProxyTarget`](crate::ProxyTarget) carries that limit; this module
//! turns one decoded [`ImageData`](crate::ImageData) into the bytes that go to
//! the GPU.
//!
//! The output keeps straight-alpha sRGB RGBA8 and the same UV convention as
//! the source: sprite `uv_rect`s and mesh UVs are normalised, so a smaller
//! texture maps exactly like the full-size one.  Resampling averages in
//! premultiplied space, which avoids the dark fringe a straight-alpha average
//! would leave on transparent edges.
//!
//! The headless renderer and the export path use [`ProxyTarget::Full`], so
//! their bytes never change.  Only the interactive preview opts into a fit.

use crate::{ImageData, ProxyTarget};

/// The bytes of one image ready for an upload.
pub(crate) struct Upload<'a> {
    /// Texture width in pixels.
    pub width: u32,
    /// Texture height in pixels.
    pub height: u32,
    /// Straight-alpha sRGB RGBA8.
    pub rgba8: &'a [u8],
}

/// Prepares `image` for `target`, using `scratch` when it must be resampled.
///
/// The caller reuses one scratch buffer per painter, so an upload allocates
/// only when the proxy actually changes the resolution.
pub(crate) fn prepare<'a>(
    image: &'a ImageData,
    target: ProxyTarget,
    scratch: &'a mut Vec<u8>,
) -> Upload<'a> {
    let Some((width, height)) = target.size_for(image) else {
        return Upload {
            width: image.width,
            height: image.height,
            rgba8: &image.rgba8,
        };
    };
    if resample(image, width, height, scratch) {
        Upload {
            width,
            height,
            rgba8: scratch.as_slice(),
        }
    } else {
        // Malformed pixel data: keep the source rather than upload garbage.
        Upload {
            width: image.width,
            height: image.height,
            rgba8: &image.rgba8,
        }
    }
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
    fn prepare_borrows_the_source_when_it_fits() {
        let source = image(8, 8);
        let mut scratch = Vec::new();
        let upload = prepare(
            &source,
            ProxyTarget::Fit {
                max_width: 64,
                max_height: 64,
            },
            &mut scratch,
        );
        assert_eq!((upload.width, upload.height), (8, 8));
        assert!(std::ptr::eq(upload.rgba8.as_ptr(), source.rgba8.as_ptr()));
        assert!(scratch.is_empty());
    }
}

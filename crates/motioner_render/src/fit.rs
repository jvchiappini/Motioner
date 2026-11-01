//! Mapping a composition into an arbitrary window rectangle.
//!
//! `Pure2D` world space has its origin at the window centre and `y` up; a
//! composition is `y` down with its origin at the top-left. [`ViewFit`]
//! carries the uniform scale and world-space translation that place the
//! composition inside a letterboxed window rectangle. The headless
//! [`crate::FrameRenderer`] uses [`ViewFit::IDENTITY`], which reproduces the
//! historical composition-sized mapping exactly.

use motioner_eval::Affine2;

/// A uniform composition-to-window mapping.
///
/// `scale` multiplies composition pixels; `offset` is the world-space
/// translation added after the composition flip. The pair is produced by
/// [`ViewFit::contain`] or constructed directly by callers that manage their
/// own camera.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewFit {
    scale: f64,
    offset: (f64, f64),
}

impl Default for ViewFit {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl ViewFit {
    /// The identity fit: one composition pixel per world unit, centred.
    ///
    /// This is the mapping the headless renderer has always used for a target
    /// of exactly the composition size.
    pub const IDENTITY: Self = Self {
        scale: 1.0,
        offset: (0.0, 0.0),
    };

    /// The uniform scale factor.
    #[must_use]
    pub const fn scale(self) -> f64 {
        self.scale
    }

    /// The world-space translation `(x, y)` applied after the flip and scale.
    #[must_use]
    pub const fn offset(self) -> (f64, f64) {
        self.offset
    }

    /// Whether this fit maps any pixels. A zero or non-finite scale draws
    /// nothing.
    #[must_use]
    pub fn is_drawable(self) -> bool {
        self.scale.is_finite() && self.scale > 0.0
    }

    /// Letterboxes a `comp_w` × `comp_h` composition into `rect`.
    ///
    /// `rect` is `[x, y, w, h]` in window pixels with a top-left origin; the
    /// composition is centred inside it and its aspect ratio is preserved.
    /// `window_w` and `window_h` describe the full window in physical pixels.
    ///
    /// Degenerate inputs (zero or non-finite rect or composition sizes, or
    /// non-finite window sizes) return a fit with `scale == 0.0` that draws
    /// nothing. The result never contains NaN.
    #[must_use]
    pub fn contain(comp_w: f64, comp_h: f64, rect: [f64; 4], window_w: f64, window_h: f64) -> Self {
        let [rect_x, rect_y, rect_w, rect_h] = rect;
        let finite = comp_w.is_finite()
            && comp_h.is_finite()
            && rect_x.is_finite()
            && rect_y.is_finite()
            && rect_w.is_finite()
            && rect_h.is_finite()
            && window_w.is_finite()
            && window_h.is_finite();
        if !finite || comp_w <= 0.0 || comp_h <= 0.0 || rect_w <= 0.0 || rect_h <= 0.0 {
            return Self {
                scale: 0.0,
                offset: (0.0, 0.0),
            };
        }

        let scale = (rect_w / comp_w).min(rect_h / comp_h);
        let drawn_w = comp_w * scale;
        let drawn_h = comp_h * scale;
        let pad_x = rect_x + (rect_w - drawn_w) * 0.5;
        let pad_y = rect_y + (rect_h - drawn_h) * 0.5;

        // World space is centred on the window with `y` up:
        //   world = (window_x - window_w / 2, window_h / 2 - window_y).
        // After the composition flip `f = (x - W/2, H/2 - y)`, a point maps
        // to `scale * f + offset`; solve for the offset that lands the
        // composition's top-left at `(pad_x, pad_y)`.
        let offset_x = pad_x + scale * comp_w * 0.5 - window_w * 0.5;
        let offset_y = window_h * 0.5 - pad_y - scale * comp_h * 0.5;

        Self {
            scale,
            offset: (offset_x, offset_y),
        }
    }

    /// Applies this fit to a composition flip, producing the final
    /// composition-pixel to world-space affine.
    ///
    /// `Affine2::then(self, inner)` applies `inner` first, so the scale and
    /// translation are composed with `then(flip)`.
    #[must_use]
    #[cfg_attr(not(feature = "live"), allow(dead_code))]
    pub(crate) fn transform(self, flip: Affine2) -> Affine2 {
        let (tx, ty) = self.offset;
        let scale = Affine2 {
            a: self.scale,
            b: 0.0,
            c: 0.0,
            d: self.scale,
            tx,
            ty,
        };
        scale.then(flip)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(actual: f64, expected: f64) -> bool {
        (actual - expected).abs() < 1e-9
    }

    #[test]
    fn exact_fit_is_the_identity() {
        let fit = ViewFit::contain(1920.0, 1080.0, [0.0, 0.0, 1920.0, 1080.0], 1920.0, 1080.0);
        assert!(close(fit.scale(), 1.0));
        assert!(close(fit.offset().0, 0.0) && close(fit.offset().1, 0.0));
        assert_eq!(fit, ViewFit::IDENTITY);
    }

    #[test]
    fn a_wide_rect_letterboxes_horizontally() {
        let fit = ViewFit::contain(100.0, 100.0, [0.0, 0.0, 400.0, 200.0], 400.0, 200.0);
        assert!(close(fit.scale(), 2.0));
        assert!(close(fit.offset().0, 0.0));
        assert!(close(fit.offset().1, 0.0));
    }

    #[test]
    fn a_tall_rect_letterboxes_vertically() {
        let fit = ViewFit::contain(100.0, 100.0, [0.0, 0.0, 200.0, 400.0], 200.0, 400.0);
        assert!(close(fit.scale(), 2.0));
        assert!(close(fit.offset().0, 0.0));
        assert!(close(fit.offset().1, 0.0));
    }

    #[test]
    fn a_rect_offset_from_the_window_centre_shifts_the_world() {
        let fit = ViewFit::contain(100.0, 100.0, [50.0, 20.0, 100.0, 100.0], 400.0, 300.0);
        assert!(close(fit.scale(), 1.0));
        assert!(close(fit.offset().0, -100.0));
        assert!(close(fit.offset().1, 80.0));
    }

    #[test]
    fn zero_size_rects_draw_nothing_and_never_produce_nan() {
        let fit = ViewFit::contain(100.0, 100.0, [0.0, 0.0, 0.0, 100.0], 400.0, 300.0);
        assert!(close(fit.scale(), 0.0));
        assert!(!fit.is_drawable());
        assert!(fit.offset().0.is_finite() && fit.offset().1.is_finite());

        let fit = ViewFit::contain(0.0, 100.0, [0.0, 0.0, 100.0, 100.0], 400.0, 300.0);
        assert!(!fit.is_drawable());
        assert!(fit.offset().0.is_finite() && fit.offset().1.is_finite());

        let fit = ViewFit::contain(100.0, 100.0, [0.0, 0.0, f64::NAN, 100.0], 400.0, 300.0);
        assert!(!fit.is_drawable());
        assert!(fit.offset().0.is_finite() && fit.offset().1.is_finite());
    }

    #[test]
    fn identity_fit_leaves_the_composition_flip_untouched() {
        let flip = Affine2 {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: -1.0,
            tx: -960.0,
            ty: 540.0,
        };
        assert_eq!(ViewFit::IDENTITY.transform(flip), flip);
    }

    #[test]
    fn a_letterboxed_fit_maps_the_composition_top_left_to_the_rect() {
        // 100x50 composition into a 400x400 rect: scale 4, vertical padding.
        let fit = ViewFit::contain(100.0, 50.0, [0.0, 0.0, 400.0, 400.0], 400.0, 400.0);
        let flip = Affine2 {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: -1.0,
            tx: -50.0,
            ty: 25.0,
        };
        let model = fit.transform(flip);
        let top_left = model.apply(motioner_core::Vec2::ZERO);
        // World of window pixel (0, 100): x = -200, y = 200 - 100 = 100.
        assert!(close(top_left.x, -200.0));
        assert!(close(top_left.y, 100.0));
    }
}

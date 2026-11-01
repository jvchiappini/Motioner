//! The canvas viewport: the one mapping between composition pixels and
//! window pixels (zoom and pan, D-0026).
//!
//! Every overlay, pick, gizmo, snap threshold and the zoom readout uses this
//! mapping, so they can never disagree. The viewport is pure geometry: it
//! depends on the composition size, the canvas area, the requested zoom and
//! the pan, and never on the frame. Zoom and pan are view state and never
//! change a rendered frame.

use motioner_core::Vec2;

use crate::layout::Rect;

/// Smallest zoom the editor accepts (5 %).
pub const MIN_ZOOM: f64 = 0.05;
/// Largest zoom the editor accepts (3200 %).
pub const MAX_ZOOM: f64 = 32.0;
/// One wheel step multiplies the zoom by this.
pub const WHEEL_STEP: f64 = 1.25;

/// A zoom value clamped to [`MIN_ZOOM`]..=[`MAX_ZOOM`].
#[must_use]
pub fn clamp_zoom(zoom: f64) -> f64 {
    if zoom.is_finite() {
        zoom.clamp(MIN_ZOOM, MAX_ZOOM)
    } else {
        1.0
    }
}

/// The zoom after `steps` wheel steps from `current` (1.25 per step, clamped).
#[must_use]
pub fn zoom_by_steps(current: f64, steps: i64) -> f64 {
    // Wheel steps are bounded by the caller; the exponent is an i32.
    #[allow(clippy::cast_possible_wrap)]
    let factor = WHEEL_STEP.powi(steps.clamp(-1_000, 1_000) as i32);
    clamp_zoom(current * factor)
}

/// The pan that keeps `anchor` (composition pixels) under the same window
/// point when the scale changes from `from` to `to`.
///
/// With a centred composition the mapping is
/// `screen = centre + scale * (pan + point - size / 2)`; solving for the new
/// pan keeps the anchor fixed. `size` is the composition size in pixels.
#[must_use]
pub fn anchored_pan(
    pan: (f64, f64),
    from: f64,
    to: f64,
    anchor: Vec2,
    size: (f64, f64),
) -> (f64, f64) {
    if !(from.is_finite() && to.is_finite() && from > 0.0 && to > 0.0) {
        return pan;
    }
    let ratio = from / to;
    let axis =
        |pan: f64, point: f64, size: f64| ratio * (pan + point - size * 0.5) - point + size * 0.5;
    (axis(pan.0, anchor.x, size.0), axis(pan.1, anchor.y, size.1))
}

/// Composition pixels to window pixels for one canvas layout.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    /// Composition width in pixels.
    pub width: f64,
    /// Composition height in pixels.
    pub height: f64,
    /// The composition's rectangle on screen, in whole window pixels.
    pub rect: Rect,
    /// The effective scale after the rectangle is rounded (window pixels per
    /// composition pixel).
    pub scale: f64,
    /// The scale of a fitted composition in the current canvas; `zoom_by`
    /// multiplies from this while the zoom is fitted.
    pub fit_scale: f64,
    /// The applied pan, in composition pixels.
    pub pan: (f64, f64),
}

impl Viewport {
    /// The fit scale of a `width` × `height` composition inside `area`.
    #[must_use]
    pub fn fit_scale(width: f64, height: f64, area: Rect) -> Option<f64> {
        let (w, h) = (f64::from(area.w), f64::from(area.h));
        if !(width.is_finite()
            && height.is_finite()
            && width > 0.0
            && height > 0.0
            && w > 0.0
            && h > 0.0)
        {
            return None;
        }
        Some((w / width).min(h / height))
    }

    /// The viewport of a composition inside `area`. `zoom` is absolute
    /// (`None` fits the composition); `pan` moves the view in composition
    /// pixels, positive right and down.
    #[must_use]
    pub fn new(
        width: f64,
        height: f64,
        area: Rect,
        zoom: Option<f64>,
        pan: (f64, f64),
    ) -> Option<Self> {
        let fit_scale = Self::fit_scale(width, height, area)?;
        let requested = zoom.map_or(fit_scale, clamp_zoom);
        let pan = if pan.0.is_finite() && pan.1.is_finite() {
            pan
        } else {
            (0.0, 0.0)
        };
        let drawn_w = (width * requested).round() as f32;
        let drawn_h = (height * requested).round() as f32;
        if drawn_w <= 0.0 || drawn_h <= 0.0 {
            return None;
        }
        let x = (area.center_x() - drawn_w * 0.5 + (pan.0 * requested) as f32).round();
        let y = (area.center_y() - drawn_h * 0.5 + (pan.1 * requested) as f32).round();
        let rect = Rect::new(x, y, drawn_w, drawn_h);
        // The renderer letterboxes the composition inside `rect`; using the
        // same minimum keeps every overlay on the drawn pixels.
        let scale = (f64::from(drawn_w) / width).min(f64::from(drawn_h) / height);
        Some(Self {
            width,
            height,
            rect,
            scale,
            fit_scale,
            pan,
        })
    }

    /// A composition point in window pixels.
    #[must_use]
    pub fn to_screen(&self, point: Vec2) -> (f64, f64) {
        (
            f64::from(self.rect.x) + point.x * self.scale,
            f64::from(self.rect.y) + point.y * self.scale,
        )
    }

    /// A window point in composition pixels.
    #[must_use]
    pub fn to_composition(&self, x: f32, y: f32) -> Vec2 {
        Vec2::new(
            (f64::from(x) - f64::from(self.rect.x)) / self.scale,
            (f64::from(y) - f64::from(self.rect.y)) / self.scale,
        )
    }

    /// A composition box in window pixels, rounded outwards so a sliver of a
    /// layer is still visible.
    #[must_use]
    pub fn box_to_screen(&self, min: Vec2, max: Vec2) -> Rect {
        let (x0, y0) = self.to_screen(min);
        let (x1, y1) = self.to_screen(max);
        Rect::new(
            x0.floor() as f32,
            y0.floor() as f32,
            (x1.ceil() - x0.floor()) as f32,
            (y1.ceil() - y0.floor()) as f32,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    fn area(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::new(x, y, w, h)
    }

    #[test]
    fn fit_centres_the_composition() {
        let view = Viewport::new(
            1920.0,
            1080.0,
            area(0.0, 0.0, 960.0, 540.0),
            None,
            (0.0, 0.0),
        )
        .unwrap();
        assert!(close(view.scale, 0.5));
        assert_eq!(view.rect, Rect::new(0.0, 0.0, 960.0, 540.0));
        assert!(close(view.to_screen(Vec2::new(960.0, 540.0)).0, 480.0));
    }

    #[test]
    fn a_hundred_percent_shows_a_pixel_per_pixel() {
        let view = Viewport::new(
            1920.0,
            1080.0,
            area(0.0, 0.0, 1920.0, 1080.0),
            Some(1.0),
            (0.0, 0.0),
        )
        .unwrap();
        assert!(close(view.scale, 1.0));
        assert_eq!(view.rect, Rect::new(0.0, 0.0, 1920.0, 1080.0));
        assert_eq!(view.to_composition(0.0, 0.0), Vec2::ZERO);
        assert_eq!(view.to_composition(960.0, 540.0), Vec2::new(960.0, 540.0));
    }

    #[test]
    fn four_hundred_percent_is_centred_and_letters_boxed() {
        let view = Viewport::new(
            1920.0,
            1080.0,
            area(0.0, 0.0, 1600.0, 900.0),
            Some(4.0),
            (0.0, 0.0),
        )
        .unwrap();
        assert!(close(view.scale, 4.0));
        assert_eq!(view.rect, Rect::new(-3040.0, -1710.0, 7680.0, 4320.0));
        // The composition centre is still the canvas centre.
        let (cx, cy) = view.to_screen(Vec2::new(960.0, 540.0));
        assert!(close(cx, 800.0) && close(cy, 450.0));
    }

    #[test]
    fn pan_moves_the_composition_in_composition_pixels() {
        let view = Viewport::new(
            1920.0,
            1080.0,
            area(0.0, 0.0, 1920.0, 1080.0),
            Some(1.0),
            (100.0, -50.0),
        )
        .unwrap();
        assert_eq!(view.rect, Rect::new(100.0, -50.0, 1920.0, 1080.0));
        assert_eq!(
            view.to_composition(100.0, -50.0),
            Vec2::new(0.0, 0.0),
            "the pan origin is the composition origin"
        );
    }

    #[test]
    fn window_and_composition_points_round_trip() {
        let view = Viewport::new(
            1920.0,
            1080.0,
            area(37.0, 11.0, 1500.0, 800.0),
            Some(2.5),
            (12.0, -7.0),
        )
        .unwrap();
        for point in [
            Vec2::new(0.0, 0.0),
            Vec2::new(960.0, 540.0),
            Vec2::new(1920.0, 1080.0),
        ] {
            let (x, y) = view.to_screen(point);
            let back = view.to_composition(x as f32, y as f32);
            assert!(close(back.x, point.x) && close(back.y, point.y));
        }
    }

    #[test]
    fn zooming_keeps_the_anchor_under_the_pointer() {
        let canvas = area(0.0, 0.0, 1600.0, 900.0);
        let before = Viewport::new(1920.0, 1080.0, canvas, None, (0.0, 0.0)).unwrap();
        let anchor_window = (200.0, 700.0);
        let anchor = before.to_composition(anchor_window.0, anchor_window.1);
        let to = zoom_by_steps(before.fit_scale, 3);
        let pan = anchored_pan(
            before.pan,
            before.fit_scale,
            to,
            anchor,
            (before.width, before.height),
        );
        let after = Viewport::new(1920.0, 1080.0, canvas, Some(to), pan).unwrap();
        let (x, y) = after.to_screen(anchor);
        // Whole-pixel rounding of the rect allows a sub-pixel difference.
        assert!((x - f64::from(anchor_window.0)).abs() <= 1.0, "{x}");
        assert!((y - f64::from(anchor_window.1)).abs() <= 1.0, "{y}");
    }

    #[test]
    fn zoom_is_clamped() {
        assert!(close(zoom_by_steps(1.0, 1000), MAX_ZOOM));
        assert!(close(zoom_by_steps(1.0, -1000), MIN_ZOOM));
        assert!(close(zoom_by_steps(1.0, 1), 1.25));
        assert!(close(clamp_zoom(f64::NAN), 1.0));
    }

    #[test]
    fn a_degenerate_canvas_has_no_viewport() {
        assert!(
            Viewport::new(1920.0, 1080.0, area(0.0, 0.0, 0.0, 900.0), None, (0.0, 0.0)).is_none()
        );
        assert!(
            Viewport::new(0.0, 1080.0, area(0.0, 0.0, 900.0, 900.0), None, (0.0, 0.0)).is_none()
        );
    }
}

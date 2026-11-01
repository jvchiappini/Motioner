//! Drawing primitives for the editor chrome on top of the engine's UI batch.
//!
//! [`Painter`] wraps one frame's `GuiBatch`, the UI font, the [`Theme`] and the
//! DPI [`Metrics`]. Every panel draws through it, so there is one place that
//! knows how quads, borders, shadows, hairlines, polygons and text are packed.
//!
//! Two engine helpers are bypassed on purpose: `GuiBatch::border` and
//! `GuiBatch::shadow` pack their parameters into fields that `gui.wgsl` does
//! not read (the shader takes the border width and the shadow blur from
//! `uv0.x`, and the shadow's unexpanded size from `color_b.xy`). The quads are
//! built here with the layout the shader actually uses.
//!
//! Anti-aliased polygons (play triangles, chevrons, keyframe diamonds) are
//! filled as one-pixel scanline quads: each quad's horizontal edges are
//! anti-aliased by the shader's rounded-rect SDF, which gives clean slanted
//! edges without the engine's aliased SVG mesh path.

use ferrous_font::Font;
use ferrous_gui::{
    GuiBatch, GuiQuad, Rect as ClipRect, BORDER_BIT, GRADIENT_BIT, GRADIENT_V_BIT, SHADOW_BIT,
};

use crate::layout::Rect;
use crate::text::{self, Align, TextStyle};
use crate::theme::{Metrics, Theme};

/// One frame's drawing context for the chrome.
pub struct Painter<'a> {
    batch: &'a mut GuiBatch,
    font: &'a Font,
    /// Colours.
    pub theme: &'a Theme,
    /// DPI metrics.
    pub m: Metrics,
}

impl<'a> Painter<'a> {
    /// Wraps a batch for one frame.
    pub fn new(batch: &'a mut GuiBatch, font: &'a Font, theme: &'a Theme, m: Metrics) -> Self {
        Self {
            batch,
            font,
            theme,
            m,
        }
    }

    /// Starts a new layer: everything drawn afterwards (text included) is
    /// painted above everything drawn before.
    pub fn layer(&mut self) {
        self.batch.begin_layer();
    }

    /// Restricts drawing to `rect` until the matching [`Self::pop_clip`].
    pub fn push_clip(&mut self, rect: Rect) {
        self.batch.push_clip(ClipRect {
            x: rect.x,
            y: rect.y,
            width: rect.w,
            height: rect.h,
        });
    }

    /// Ends the innermost clip.
    pub fn pop_clip(&mut self) {
        self.batch.pop_clip();
    }

    /// Fills a rectangle.
    pub fn fill(&mut self, rect: Rect, color: [f32; 4]) {
        self.fill_rounded(rect, color, 0.0);
    }

    /// Fills a rounded rectangle; the radius is clamped to half the short side.
    pub fn fill_rounded(&mut self, rect: Rect, color: [f32; 4], radius: f32) {
        if rect.is_empty() || color[3] <= 0.0 {
            return;
        }
        let radius = radius.clamp(0.0, rect.w.min(rect.h) * 0.5);
        self.batch
            .push_quad(quad(rect, color, [radius; 4], [0.0; 2], [0.0; 4], 0));
    }

    /// Strokes the inside of a rounded rectangle with a `width`-pixel border.
    pub fn stroke_rounded(&mut self, rect: Rect, color: [f32; 4], radius: f32, width: f32) {
        if rect.is_empty() || width <= 0.0 || color[3] <= 0.0 {
            return;
        }
        let radius = radius.clamp(0.0, rect.w.min(rect.h) * 0.5);
        self.batch.push_quad(quad(
            rect,
            color,
            [radius; 4],
            [width, 0.0],
            [0.0; 4],
            BORDER_BIT,
        ));
    }

    /// A soft drop shadow under `rect`, offset down by `offset_y`.
    pub fn shadow(&mut self, rect: Rect, radius: f32, blur: f32, offset_y: f32, color: [f32; 4]) {
        if rect.is_empty() || blur <= 0.0 {
            return;
        }
        let expand = blur * 4.0;
        let outer = Rect::new(
            rect.x - expand,
            rect.y + offset_y - expand,
            rect.w + expand * 2.0,
            rect.h + expand * 2.0,
        );
        let radius = radius.clamp(0.0, rect.w.min(rect.h) * 0.5);
        self.batch.push_quad(quad(
            outer,
            color,
            [radius; 4],
            [blur, 0.0],
            [rect.w, rect.h, 0.0, 0.0],
            SHADOW_BIT,
        ));
    }

    /// A vertical gradient from `top` to `bottom` colour over `rect`, used to
    /// fade content into a panel edge.
    pub fn gradient_v(&mut self, rect: Rect, top: [f32; 4], bottom: [f32; 4]) {
        if rect.is_empty() {
            return;
        }
        self.batch.push_quad(quad(
            rect,
            top,
            [0.0; 4],
            [0.0; 2],
            bottom,
            GRADIENT_BIT | GRADIENT_V_BIT,
        ));
    }

    /// A horizontal hairline from `x0` to `x1` with its top at `y`.
    pub fn hline(&mut self, x0: f32, x1: f32, y: f32, color: [f32; 4]) {
        let t = self.m.hairline();
        self.fill(Rect::new(x0, y, x1 - x0, t), color);
    }

    /// A vertical hairline from `y0` to `y1` with its left edge at `x`.
    pub fn vline(&mut self, x: f32, y0: f32, y1: f32, color: [f32; 4]) {
        let t = self.m.hairline();
        self.fill(Rect::new(x, y0, t, y1 - y0), color);
    }

    /// A filled circle.
    pub fn circle(&mut self, cx: f32, cy: f32, radius: f32, color: [f32; 4]) {
        let d = radius * 2.0;
        self.fill_rounded(Rect::new(cx - radius, cy - radius, d, d), color, radius);
    }

    /// A circle outline `width` pixels thick, inside `radius`.
    pub fn ring(&mut self, cx: f32, cy: f32, radius: f32, width: f32, color: [f32; 4]) {
        let d = radius * 2.0;
        self.stroke_rounded(
            Rect::new(cx - radius, cy - radius, d, d),
            color,
            radius,
            width,
        );
    }

    /// Fills a convex polygon given in pixels (any winding).
    ///
    /// The polygon is cut into one-pixel rows; each row is a quad spanning the
    /// polygon's extent at the row centre, so slanted edges are anti-aliased
    /// horizontally by the quad shader. Partially covered top and bottom rows
    /// are faded by their vertical coverage.
    pub fn polygon(&mut self, points: &[(f32, f32)], color: [f32; 4]) {
        if points.len() < 3 {
            return;
        }
        let (min_y, max_y) = points
            .iter()
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), p| {
                (lo.min(p.1), hi.max(p.1))
            });
        if !(min_y.is_finite() && max_y.is_finite()) || max_y <= min_y {
            return;
        }
        let first = min_y.floor() as i32;
        let last = max_y.ceil() as i32;
        for row in first..last {
            let top = row as f32;
            let coverage = (max_y.min(top + 1.0) - min_y.max(top)).clamp(0.0, 1.0);
            let sample = top + 0.5;
            let sample = sample.clamp(min_y + 1e-3, max_y - 1e-3);
            let Some((x0, x1)) = span_at(points, sample) else {
                continue;
            };
            let mut row_color = color;
            row_color[3] *= coverage;
            self.fill(Rect::new(x0, top, x1 - x0, 1.0), row_color);
        }
    }

    /// Draws text vertically centred in `rect`, ellipsized to fit.
    pub fn text_in(&mut self, text: &str, rect: Rect, align: Align, style: &TextStyle) {
        text::draw_in(self.batch, self.font, text, rect, align, style);
    }

    /// Width of `text` in physical pixels.
    #[must_use]
    pub fn measure(&self, text: &str, style: &TextStyle) -> f32 {
        text::advance(self.font, text, style)
    }

    /// A text style at a logical size in a colour.
    #[must_use]
    pub fn style(&self, logical_size: f32, color: [f32; 4]) -> TextStyle {
        TextStyle::new(self.m.font(logical_size), color)
    }
}

fn quad(
    rect: Rect,
    color: [f32; 4],
    radii: [f32; 4],
    uv0: [f32; 2],
    color_b: [f32; 4],
    flags: u32,
) -> GuiQuad {
    GuiQuad {
        pos: [rect.x, rect.y],
        size: [rect.w, rect.h],
        uv0,
        uv1: [1.0, 1.0],
        color,
        color_b,
        radii,
        tex_index: 0,
        flags,
        z_order: 0.0,
        node_id: 0,
    }
}

/// The horizontal extent of a convex polygon at height `y`.
fn span_at(points: &[(f32, f32)], y: f32) -> Option<(f32, f32)> {
    let mut lo = f32::INFINITY;
    let mut hi = f32::NEG_INFINITY;
    for (i, &(ax, ay)) in points.iter().enumerate() {
        let (bx, by) = points[(i + 1) % points.len()];
        let (y0, y1) = if ay < by { (ay, by) } else { (by, ay) };
        if y < y0 || y > y1 || (y1 - y0).abs() < f32::EPSILON {
            continue;
        }
        let t = (y - ay) / (by - ay);
        let x = ax + t * (bx - ax);
        lo = lo.min(x);
        hi = hi.max(x);
    }
    (hi > lo).then_some((lo, hi))
}

#[cfg(test)]
mod tests {
    use super::span_at;

    #[test]
    fn spans_of_a_triangle() {
        // Right-pointing play triangle: apex at (10, 5).
        let tri = [(0.0, 0.0), (10.0, 5.0), (0.0, 10.0)];
        let (lo, hi) = span_at(&tri, 5.0).unwrap();
        assert!(lo.abs() < 1e-4 && (hi - 10.0).abs() < 1e-4);
        let (lo, hi) = span_at(&tri, 2.5).unwrap();
        assert!(lo.abs() < 1e-4 && (hi - 5.0).abs() < 1e-4);
        assert!(span_at(&tri, 11.0).is_none());
    }

    #[test]
    fn spans_of_a_diamond() {
        let diamond = [(5.0, 0.0), (10.0, 5.0), (5.0, 10.0), (0.0, 5.0)];
        let (lo, hi) = span_at(&diamond, 5.0).unwrap();
        assert!(lo.abs() < 1e-4 && (hi - 10.0).abs() < 1e-4);
        let (lo, hi) = span_at(&diamond, 1.0).unwrap();
        assert!((lo - 4.0).abs() < 1e-4 && (hi - 6.0).abs() < 1e-4);
    }
}

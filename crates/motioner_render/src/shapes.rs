//! Conversion of Motioner render items into `Pure2D` shape instances.
//!
//! Rectangles and circles become rounded-rect SDF instances. Fills use the
//! item's local dimensions; strokes are drawn as a ring inside a shape grown by
//! half the stroke width, which reproduces a stroke centred on the original
//! edge because the engine draws ring bands inside the shape boundary.
//!
//! Corner radii and stroke widths are authored in local pixels and converted to
//! world units with [`crate::space::world_unit_scale`]. Under a non-uniform
//! scale that single factor is an approximation (documented in the README).

use ferrous_2d::render::ShapeInstance;
use motioner_core::Color;
use motioner_eval::{Affine2, ItemShape, RenderItem};

use crate::{space, Canvas2d};

/// Colour, opacity and depth shared by one shape instance.
#[derive(Clone, Copy)]
struct Style {
    color: Color,
    opacity: f64,
    z: f32,
}

/// Number of shape instances a shape item will submit.
///
/// A rectangle or circle submits one instance per present fill/stroke; text
/// items submit none here (they are handled by [`crate::text`]).
pub(crate) fn draw_count(shape: &ItemShape) -> u32 {
    match shape {
        ItemShape::Rect { paint, .. } | ItemShape::Circle { paint, .. } => {
            u32::from(paint.fill.is_some()) + u32::from(has_visible_stroke(paint))
        }
        ItemShape::Text { .. } => 0,
    }
}

/// Whether a paint has a stroke that can leave ink.
///
/// A zero-width stroke is treated as absent: the engine's inside ring would
/// otherwise render a thin anti-aliased line on a stroke the document asked
/// to make invisible.
fn has_visible_stroke(paint: &motioner_core::Paint) -> bool {
    paint.stroke.is_some_and(|stroke| stroke.width > 0.0)
}

/// Submits the fill and stroke instances of a shape item, in that order.
///
/// Returns the number of instances submitted. `z_index` advances once per
/// submitted instance so the caller's draw ordering matches [`draw_count`].
pub(crate) fn draw_item(
    canvas: &mut impl Canvas2d,
    item: &RenderItem,
    flip: Affine2,
    z_index: &mut u32,
    total: u32,
) -> u32 {
    let model = space::shape_model(flip, item.transform);
    let scale = space::world_unit_scale(model);
    match item.shape {
        ItemShape::Rect {
            width,
            height,
            radius,
            paint,
        } => {
            let mut draws = 0;
            if let Some(fill) = paint.fill {
                let style = Style {
                    color: fill,
                    opacity: item.opacity,
                    z: space::z_for(*z_index, total),
                };
                canvas.shape(fill_rect(model, width, height, radius, scale, style));
                *z_index += 1;
                draws += 1;
            }
            if let Some(stroke) = paint.stroke.filter(|stroke| stroke.width > 0.0) {
                let style = Style {
                    color: stroke.color,
                    opacity: item.opacity,
                    z: space::z_for(*z_index, total),
                };
                canvas.shape(stroke_rect(
                    model,
                    width,
                    height,
                    radius,
                    scale,
                    style,
                    stroke.width,
                ));
                *z_index += 1;
                draws += 1;
            }
            draws
        }
        ItemShape::Circle { radius, paint } => {
            let mut draws = 0;
            if let Some(fill) = paint.fill {
                let style = Style {
                    color: fill,
                    opacity: item.opacity,
                    z: space::z_for(*z_index, total),
                };
                canvas.shape(fill_circle(model, radius, scale, style));
                *z_index += 1;
                draws += 1;
            }
            if let Some(stroke) = paint.stroke.filter(|stroke| stroke.width > 0.0) {
                let style = Style {
                    color: stroke.color,
                    opacity: item.opacity,
                    z: space::z_for(*z_index, total),
                };
                canvas.shape(stroke_circle(model, radius, scale, style, stroke.width));
                *z_index += 1;
                draws += 1;
            }
            draws
        }
        ItemShape::Text { .. } => 0,
    }
}

fn fill_rect(
    model: Affine2,
    width: f64,
    height: f64,
    radius: f64,
    scale: f64,
    style: Style,
) -> ShapeInstance {
    instance(
        model,
        (width, height),
        0.0,
        space::to_f32(clamped_radius(radius, width, height) * scale),
        false,
        style,
    )
}

/// Clamps a corner radius to half the smallest side.
///
/// The engine's rounded-rect SDF assumes the radius does not exceed this;
/// larger radii (which CSS-style renderers clamp) would otherwise produce a
/// degenerate field.
fn clamped_radius(radius: f64, width: f64, height: f64) -> f64 {
    radius.max(0.0).min(width.min(height) * 0.5)
}

fn stroke_rect(
    model: Affine2,
    width: f64,
    height: f64,
    radius: f64,
    scale: f64,
    style: Style,
    stroke_width: f64,
) -> ShapeInstance {
    let grown_width = width + stroke_width;
    let grown_height = height + stroke_width;
    // Grow the corner radius with the shape, then clamp it like a fill.
    let radius_local = clamped_radius(radius + stroke_width * 0.5, grown_width, grown_height);
    instance(
        model,
        (grown_width, grown_height),
        space::to_f32(stroke_width * scale),
        space::to_f32(radius_local * scale),
        true,
        style,
    )
}

fn fill_circle(model: Affine2, radius: f64, scale: f64, style: Style) -> ShapeInstance {
    instance(
        model,
        (radius * 2.0, radius * 2.0),
        0.0,
        space::to_f32(radius * scale),
        false,
        style,
    )
}

fn stroke_circle(
    model: Affine2,
    radius: f64,
    scale: f64,
    style: Style,
    stroke_width: f64,
) -> ShapeInstance {
    let grown_radius = radius + stroke_width * 0.5;
    instance(
        model,
        (grown_radius * 2.0, grown_radius * 2.0),
        space::to_f32(stroke_width * scale),
        space::to_f32(grown_radius * scale),
        true,
        style,
    )
}

fn instance(
    model: Affine2,
    size: (f64, f64),
    border: f32,
    corner_radius: f32,
    ring: bool,
    style: Style,
) -> ShapeInstance {
    let width = space::to_f32(size.0);
    let height = space::to_f32(size.1);
    ShapeInstance {
        transform_c0: [
            space::to_f32(model.a) * width,
            space::to_f32(model.b) * width,
            0.0,
            0.0,
        ],
        transform_c1: [
            space::to_f32(model.c) * height,
            space::to_f32(model.d) * height,
            0.0,
            0.0,
        ],
        // z component 1.0 marks "not a line"; w component 3.0 marks a ring.
        transform_c2: [0.0, 0.0, 1.0, if ring { 3.0 } else { 0.0 }],
        transform_c3: [
            space::to_f32(model.tx),
            space::to_f32(model.ty),
            style.z,
            1.0,
        ],
        color: space::rgba(style.color, style.opacity),
        params: [border, corner_radius, 0.0, 1.0],
        ext0: [0.0; 4],
        ext1: 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use motioner_core::{Paint, Stroke};

    fn style() -> Style {
        Style {
            color: Color::WHITE,
            opacity: 1.0,
            z: 0.0,
        }
    }

    fn column_length(column: [f32; 4]) -> f64 {
        f64::from((column[0] * column[0] + column[1] * column[1]).sqrt())
    }

    fn rect_paint(fill: Option<Color>, stroke: Option<Stroke>) -> Paint {
        Paint { fill, stroke }
    }

    #[test]
    fn rect_columns_carry_local_dimensions() {
        let flip = space::composition_flip(400, 300);
        let model = space::shape_model(flip, Affine2::IDENTITY);
        let scale = space::world_unit_scale(model);
        let fill = fill_rect(model, 200.0, 20.0, 0.0, scale, style());
        assert!((column_length(fill.transform_c0) - 200.0).abs() < 1e-3);
        assert!((column_length(fill.transform_c1) - 20.0).abs() < 1e-3);
        assert!(fill.transform_c2[2] > 0.5);
        assert!(fill.transform_c2[3].abs() < f32::EPSILON);
        assert!(fill.params[0].abs() < f32::EPSILON);
    }

    #[test]
    fn stroke_grows_dimensions_and_sets_ring() {
        let flip = space::composition_flip(400, 300);
        let model = space::shape_model(flip, Affine2::IDENTITY);
        let scale = space::world_unit_scale(model);
        let stroke = stroke_rect(model, 100.0, 100.0, 0.0, scale, style(), 10.0);
        assert!((column_length(stroke.transform_c0) - 110.0).abs() < 1e-3);
        assert!((column_length(stroke.transform_c1) - 110.0).abs() < 1e-3);
        assert!(stroke.transform_c2[3] > 2.5);
        assert!((stroke.params[0] - 10.0).abs() < 1e-3);
    }

    #[test]
    fn stroke_radius_is_clamped_to_half_the_smallest_side() {
        let flip = space::composition_flip(400, 300);
        let model = space::shape_model(flip, Affine2::IDENTITY);
        let stroke = stroke_rect(model, 100.0, 100.0, 80.0, 1.0, style(), 10.0);
        assert!(stroke.params[1] <= 55.0);
    }

    #[test]
    fn zero_width_stroke_is_not_drawn() {
        let shape = ItemShape::Rect {
            width: 10.0,
            height: 10.0,
            radius: 0.0,
            paint: rect_paint(
                None,
                Some(Stroke {
                    color: Color::WHITE,
                    width: 0.0,
                }),
            ),
        };
        assert_eq!(draw_count(&shape), 0);
    }

    #[test]
    fn fill_radius_is_clamped_to_half_the_smallest_side() {
        let flip = space::composition_flip(400, 300);
        let model = space::shape_model(flip, Affine2::IDENTITY);
        let fill = fill_rect(model, 100.0, 100.0, 80.0, 1.0, style());
        assert!((fill.params[1] - 50.0).abs() < 1e-3);
    }

    #[test]
    fn draw_count_matches_present_paint() {
        let both = ItemShape::Rect {
            width: 1.0,
            height: 1.0,
            radius: 0.0,
            paint: rect_paint(
                Some(Color::WHITE),
                Some(Stroke {
                    color: Color::WHITE,
                    width: 1.0,
                }),
            ),
        };
        assert_eq!(draw_count(&both), 2);
        let fill_only = ItemShape::Circle {
            radius: 1.0,
            paint: rect_paint(Some(Color::WHITE), None),
        };
        assert_eq!(draw_count(&fill_only), 1);
    }
}

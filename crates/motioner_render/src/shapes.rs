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
use motioner_eval::{Affine2, ItemShape, RenderItem, RenderPlan, ResolvedFill, ResolvedPaint};

use crate::gradients::{self, GradientTable};
use crate::paths::{self, PathCache};
use crate::{space, Canvas2d};

/// Colour, opacity and depth shared by one shape instance.
#[derive(Clone, Copy)]
struct Style {
    color: Color,
    opacity: f64,
    z: f32,
}

/// Number of draw layers an item contributes, for depth spreading.
///
/// A rectangle or circle draws one layer per present fill/stroke; a path
/// draws its fill batch and its stroke batch; text is one layer.
pub(crate) fn draw_layers(plan: &RenderPlan, shape: &ItemShape) -> u32 {
    match shape {
        ItemShape::Rect { paint, .. } | ItemShape::Circle { paint, .. } => {
            u32::from(paint.fill.is_some()) + u32::from(has_visible_stroke(paint))
        }
        ItemShape::Path { index } => {
            let Some(path) = plan.path(*index) else {
                return 0;
            };
            u32::from(path.paint.fill.is_some())
                + u32::from(path.paint.stroke.is_some_and(|stroke| stroke.width > 0.0))
        }
        ItemShape::Text { .. } => 1,
    }
}

/// Number of shape instances an item will submit.
///
/// Whether a paint has a stroke that can leave ink.
///
/// A zero-width stroke is treated as absent: the engine's inside ring would
/// otherwise render a thin anti-aliased line on a stroke the document asked
/// to make invisible.
fn has_visible_stroke(paint: &ResolvedPaint) -> bool {
    paint.stroke.is_some_and(|stroke| stroke.width > 0.0)
}

/// The colour to draw when the engine cannot render gradients (D-0030).
///
/// The headless renderer refuses gradient plans before reaching this point;
/// the live preview keeps drawing the first stop so the editor stays usable
/// while the engine pin is not bumped.
fn paint_color(plan: &RenderPlan, paint: ResolvedFill) -> Color {
    match paint {
        ResolvedFill::Solid(color) => color,
        ResolvedFill::Gradient(index) => plan
            .gradient(index)
            .and_then(|gradient| gradient.stops.first())
            .map_or(Color::WHITE, |stop| stop.color),
    }
}

/// Submits the fill and stroke instances of a shape item, in that order.
///
/// Returns the number of instances submitted. `z_index` advances once per
/// submitted instance so the caller's draw ordering matches [`draw_count`].
#[allow(clippy::too_many_arguments)] // the plan, the canvas, the table and its caches
pub(crate) fn draw_item(
    canvas: &mut impl Canvas2d,
    plan: &RenderPlan,
    cache: &mut PathCache,
    gradients: &GradientTable,
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
                    color: paint_color(plan, fill),
                    opacity: item.opacity,
                    z: space::z_for(*z_index, total),
                };
                canvas.shape(gradients::paint(
                    fill_rect(model, width, height, radius, scale, style),
                    gradients::base_of(gradients, fill),
                ));
                *z_index += 1;
                draws += 1;
            }
            if let Some(stroke) = paint.stroke.filter(|stroke| stroke.width > 0.0) {
                let style = Style {
                    color: paint_color(plan, stroke.paint),
                    opacity: item.opacity,
                    z: space::z_for(*z_index, total),
                };
                canvas.shape(gradients::paint(
                    stroke_rect(model, width, height, radius, scale, style, stroke.width),
                    gradients::base_of(gradients, stroke.paint),
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
                    color: paint_color(plan, fill),
                    opacity: item.opacity,
                    z: space::z_for(*z_index, total),
                };
                canvas.shape(gradients::paint(
                    fill_circle(model, radius, scale, style),
                    gradients::base_of(gradients, fill),
                ));
                *z_index += 1;
                draws += 1;
            }
            if let Some(stroke) = paint.stroke.filter(|stroke| stroke.width > 0.0) {
                let style = Style {
                    color: paint_color(plan, stroke.paint),
                    opacity: item.opacity,
                    z: space::z_for(*z_index, total),
                };
                canvas.shape(gradients::paint(
                    stroke_circle(model, radius, scale, style, stroke.width),
                    gradients::base_of(gradients, stroke.paint),
                ));
                *z_index += 1;
                draws += 1;
            }
            draws
        }
        ItemShape::Path { index } => draw_path(
            canvas, plan, cache, gradients, item, model, index, z_index, total,
        ),
        ItemShape::Text { .. } => 0,
    }
}

/// Submits one path's fill and stroke triangles; `z_index` advances once per
/// present layer, not once per triangle.
#[allow(clippy::too_many_arguments)] // the item's resolved state, kept flat
fn draw_path(
    canvas: &mut impl Canvas2d,
    plan: &RenderPlan,
    cache: &mut PathCache,
    gradients: &GradientTable,
    item: &RenderItem,
    model: Affine2,
    index: usize,
    z_index: &mut u32,
    total: u32,
) -> u32 {
    let Some(path) = plan.path(index) else {
        return 0;
    };
    let meshes = cache.meshes(item.node, path);
    let mut draws = 0;
    if let Some(fill) = path.paint.fill.filter(|_| !meshes.fill.is_empty()) {
        let color = space::linear_rgba(paint_color(plan, fill), item.opacity);
        let z = space::z_for(*z_index, total);
        for triangle in &meshes.fill {
            canvas.shape(gradients::paint(
                paths::instance(model, triangle, color, z),
                gradients::base_of(gradients, fill),
            ));
        }
        *z_index += 1;
        #[allow(clippy::cast_possible_truncation)] // triangle counts fit u32
        {
            draws += meshes.fill.len() as u32;
        }
    }
    if let Some(stroke) = path
        .paint
        .stroke
        .filter(|stroke| stroke.width > 0.0 && !meshes.stroke.is_empty())
    {
        let color = space::linear_rgba(paint_color(plan, stroke.paint), item.opacity);
        let z = space::z_for(*z_index, total);
        for triangle in &meshes.stroke {
            canvas.shape(gradients::paint(
                paths::instance(model, triangle, color, z),
                gradients::base_of(gradients, stroke.paint),
            ));
        }
        *z_index += 1;
        #[allow(clippy::cast_possible_truncation)] // triangle counts fit u32
        {
            draws += meshes.stroke.len() as u32;
        }
    }
    draws
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
        color: space::linear_rgba(style.color, style.opacity),
        params: [border, corner_radius, 0.0, 1.0],
        ext0: [0.0; 4],
        ext1: 0.0,
        #[cfg(feature = "engine-gradients")]
        gradient: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use motioner_core::Stroke;
    use motioner_eval::{ResolvedFill, ResolvedPaint, ResolvedStroke};

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

    fn rect_paint(fill: Option<Color>, stroke: Option<Stroke>) -> ResolvedPaint {
        ResolvedPaint {
            fill: fill.map(ResolvedFill::Solid),
            stroke: stroke.map(|stroke| ResolvedStroke {
                paint: ResolvedFill::Solid(stroke.color),
                width: stroke.width,
            }),
        }
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
        assert_eq!(draw_layers(&RenderPlan::new(), &shape), 0);
    }

    #[test]
    fn fill_radius_is_clamped_to_half_the_smallest_side() {
        let flip = space::composition_flip(400, 300);
        let model = space::shape_model(flip, Affine2::IDENTITY);
        let fill = fill_rect(model, 100.0, 100.0, 80.0, 1.0, style());
        assert!((fill.params[1] - 50.0).abs() < 1e-3);
    }

    #[test]
    fn draw_layers_match_present_paint() {
        let plan = RenderPlan::new();
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
        assert_eq!(draw_layers(&plan, &both), 2);
        let fill_only = ItemShape::Circle {
            radius: 1.0,
            paint: rect_paint(Some(Color::WHITE), None),
        };
        assert_eq!(draw_layers(&plan, &fill_only), 1);
    }
}

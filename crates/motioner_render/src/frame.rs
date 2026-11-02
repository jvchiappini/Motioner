//! Frame loop, plan painting, draw counting, submission and readback.
//!
//! [`paint_plan`] is the single "render plan item → engine draws" path. The
//! headless renderer and [`crate::LivePainter`] both call it, so the golden
//! frame and the live preview cannot drift apart.
//!
//! The plan is walked twice with no allocation: the first pass counts draws so
//! [`crate::space::z_for`] can spread the painter order over a fixed depth
//! window, and the second submits fill/stroke/text work in plan order. The
//! engine sorts by `z`, so shapes and text interleave exactly in the order the
//! evaluator produced.

use std::collections::HashMap;

use ferrous_2d::render::{Effect2d, FontId2d, MAX_EFFECTS_PER_LAYER};
use ferrous_renderer::headless::{HarnessError, Pure2dHarness, DEFAULT_FORMAT};
use motioner_core::{Color, Document, NodeContent, NodeId, TextAlign};
use motioner_eval::{Affine2, EffectValue, ItemShape, RenderItem, RenderPlan};

use crate::paths::PathCache;
use crate::space::{self, to_f32};
use crate::{shapes, text, Canvas2d, FrameRenderer, FrameStats, PaintStats, RenderError, ROBOTO};

/// Creates the engine harness, registers Roboto and wraps it in a
/// [`FrameRenderer`].
pub(crate) fn create(width: u32, height: u32) -> Result<FrameRenderer, RenderError> {
    let mut harness =
        Pure2dHarness::new(width, height, DEFAULT_FORMAT).map_err(map_harness_error)?;
    let font = harness
        .register_font(ROBOTO, ferrous_font::charset::latin_western())
        .map_err(|error| RenderError::Font {
            message: error.to_string(),
        })?;
    let adapter_name = harness.adapter_name();
    Ok(FrameRenderer {
        width,
        height,
        harness,
        font,
        adapter_name,
        clear: None,
        missing_glyphs: HashMap::new(),
        path_cache: PathCache::default(),
    })
}

fn map_harness_error(error: HarnessError) -> RenderError {
    match error {
        HarnessError::DeviceCreation { message } => RenderError::NoAdapter { message },
        // The only format this crate requests is `DEFAULT_FORMAT` and it never
        // resizes the readback ring, so these are unreachable in practice;
        // keep them typed instead of panicking.
        other @ (HarnessError::UnsupportedFormat { .. } | HarnessError::ReadbackPending { .. }) => {
            RenderError::Readback {
                message: other.to_string(),
            }
        }
    }
}

/// Renders one plan into `out` and returns the frame counters.
pub(crate) fn render(
    renderer: &mut FrameRenderer,
    document: &Document,
    plan: &RenderPlan,
    out: &mut Vec<u8>,
) -> Result<FrameStats, RenderError> {
    if plan.width() != renderer.width || plan.height() != renderer.height {
        return Err(RenderError::SizeMismatch {
            expected: (renderer.width, renderer.height),
            actual: (plan.width(), plan.height()),
        });
    }
    apply_clear(renderer, plan.background());
    renderer.harness.begin_frame();

    let flip = space::composition_flip(renderer.width, renderer.height);
    let gradients = crate::gradients::GradientTable::build(plan, flip);
    renderer
        .harness
        .renderer_mut()
        .set_2d_gradient_table(gradients.slots());
    let stats = paint_plan(
        &mut renderer.harness,
        document,
        plan,
        flip,
        renderer.font,
        &mut renderer.missing_glyphs,
        &mut renderer.path_cache,
        &gradients,
    );

    let report = renderer.harness.render_frame();
    renderer
        .harness
        .read_pixels_into(out)
        .map_err(|error| RenderError::Readback {
            message: error.to_string(),
        })?;

    Ok(FrameStats {
        shape_draws: stats.shape_draws,
        text_draws: stats.text_draws,
        draw_calls: report.draw_calls,
        missing_glyphs: stats.missing_glyphs,
        non_finite_items: stats.non_finite_items,
        cpu_ms: report.cpu_ms,
    })
}

/// Paints one plan into a canvas and returns the draw counters.
///
/// `flip` maps composition pixels to world space, including any [`crate::ViewFit`]
/// letterboxing. `font` and `missing_glyphs` are owned by the caller so the
/// per-node glyph measurement cache survives across frames.
#[allow(clippy::too_many_arguments)] // the plan, the canvas and its caches, kept explicit
pub(crate) fn paint_plan(
    canvas: &mut impl Canvas2d,
    document: &Document,
    plan: &RenderPlan,
    flip: Affine2,
    font: FontId2d,
    missing_glyphs: &mut HashMap<NodeId, u32>,
    path_cache: &mut PathCache,
    gradients: &crate::gradients::GradientTable,
) -> PaintStats {
    let mut total = 0u32;
    for item in plan.items() {
        total += shapes::draw_layers(plan, &item.shape);
    }

    let mut z_index = 0u32;
    let mut stats = PaintStats {
        non_finite_items: plan.non_finite_items(),
        ..PaintStats::default()
    };
    // Effect radii and offsets are composition pixels; scale them with the
    // view so a zoomed preview matches the export.
    let effect_scale = space::to_f32(space::world_unit_scale(flip));
    // Each open layer remembers whether the engine accepted it, so a rejected
    // push (invalid effect) is never popped.
    let mut open: Vec<(&motioner_eval::PlanLayer, bool)> = Vec::new();
    let mut next_layer = 0usize;
    for (item_index, item) in plan.items().iter().enumerate() {
        // Layers open before their first item and close after their last one.
        // A group without content opens at the same index as its first child,
        // so several layers can start here (parents first).
        while let Some(layer) = plan.layers().get(next_layer) {
            if layer.start != item_index {
                break;
            }
            let mut effects = [Effect2d::Blur { radius: 0.0 }; MAX_EFFECTS_PER_LAYER];
            let count = layer.effects.len().min(MAX_EFFECTS_PER_LAYER);
            for (slot, effect) in effects.iter_mut().zip(&layer.effects) {
                *slot = engine_effect(effect, effect_scale);
            }
            let pushed = canvas.push_layer(
                1.0,
                layer.blend,
                space::z_for(z_index, total),
                &effects[..count],
            );
            open.push((layer, pushed));
            next_layer += 1;
        }
        match item.shape {
            ItemShape::Text {
                size,
                color,
                align,
                gradient,
                ..
            } => {
                if let Some(missing) = draw_text(
                    canvas,
                    document,
                    item,
                    font,
                    size,
                    color,
                    align,
                    gradient.and_then(|index| gradients.base(index)),
                    flip,
                    missing_glyphs,
                    &mut z_index,
                    total,
                ) {
                    stats.text_draws += 1;
                    stats.missing_glyphs = stats.missing_glyphs.saturating_add(missing);
                }
            }
            _ => {
                stats.shape_draws += shapes::draw_item(
                    canvas,
                    plan,
                    path_cache,
                    gradients,
                    item,
                    flip,
                    &mut z_index,
                    total,
                );
            }
        }
        while let Some(&(layer, pushed)) = open.last() {
            if layer.end != item_index + 1 {
                break;
            }
            if pushed {
                canvas.pop_layer();
            }
            open.pop();
        }
    }
    stats
}

/// Maps one sampled document effect to the engine effect (D-0032).
///
/// Colours go to straight linear RGBA; radii and offsets are scaled from
/// composition pixels to the target by `scale`.
fn engine_effect(effect: &EffectValue, scale: f32) -> Effect2d {
    match *effect {
        EffectValue::DropShadow {
            color,
            offset,
            blur,
            opacity,
        } => Effect2d::DropShadow {
            color: space::linear_rgba(color, 1.0),
            offset: [
                space::to_f32(offset.x) * scale,
                space::to_f32(offset.y) * scale,
            ],
            blur: space::to_f32(blur) * scale,
            opacity: space::to_f32(opacity),
        },
        EffectValue::Blur { radius } => Effect2d::Blur {
            radius: space::to_f32(radius) * scale,
        },
        EffectValue::Glow {
            color,
            radius,
            intensity,
        } => Effect2d::Glow {
            color: space::linear_rgba(color, 1.0),
            radius: space::to_f32(radius) * scale,
            intensity: space::to_f32(intensity),
        },
    }
}

fn apply_clear(renderer: &mut FrameRenderer, background: Color) {
    let clear = wgpu::Color {
        r: f64::from(space::srgb_to_linear(background.r)),
        g: f64::from(space::srgb_to_linear(background.g)),
        b: f64::from(space::srgb_to_linear(background.b)),
        a: f64::from(background.a) / 255.0,
    };
    if renderer.clear != Some(clear) {
        renderer.harness.set_clear_color(clear);
        renderer.clear = Some(clear);
    }
}

#[allow(clippy::too_many_arguments)] // the text item's resolved fields, kept flat
fn draw_text(
    canvas: &mut impl Canvas2d,
    document: &Document,
    item: &RenderItem,
    font: FontId2d,
    size: f64,
    color: Color,
    align: TextAlign,
    gradient: Option<u32>,
    flip: Affine2,
    missing_glyphs: &mut HashMap<NodeId, u32>,
    z_index: &mut u32,
    total: u32,
) -> Option<u32> {
    let node = document.node(item.node)?;
    let NodeContent::Text(content) = node.content() else {
        return None;
    };

    let missing = if let Some(count) = missing_glyphs.get(&item.node).copied() {
        count
    } else {
        let count = text::count_missing(canvas, font, &content.text, size);
        missing_glyphs.insert(item.node, count);
        count
    };

    let model = text::text_model(flip, item.transform);
    let transform = space::model_mat4(model, 1.0, 1.0, space::z_for(*z_index, total));
    *z_index += 1;
    canvas.text(
        font,
        &content.text,
        transform,
        to_f32(size),
        text::align_to_engine(align),
        space::linear_rgba(color, item.opacity),
        gradient,
    );
    Some(missing)
}

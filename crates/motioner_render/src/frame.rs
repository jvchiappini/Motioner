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

use ferrous_2d::render::FontId2d;
use ferrous_renderer::headless::{HarnessError, Pure2dHarness, DEFAULT_FORMAT};
use motioner_core::{Color, Document, NodeContent, NodeId, TextAlign};
use motioner_eval::{Affine2, ItemShape, RenderItem, RenderPlan};

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
    })
}

fn map_harness_error(error: HarnessError) -> RenderError {
    match error {
        HarnessError::DeviceCreation { message } => RenderError::NoAdapter { message },
        // The only format this crate requests is `DEFAULT_FORMAT`, so this is
        // unreachable in practice; keep it typed instead of panicking.
        other @ HarnessError::UnsupportedFormat { .. } => RenderError::Readback {
            message: other.to_string(),
        },
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
    let stats = paint_plan(
        &mut renderer.harness,
        document,
        plan,
        flip,
        renderer.font,
        &mut renderer.missing_glyphs,
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
pub(crate) fn paint_plan(
    canvas: &mut impl Canvas2d,
    document: &Document,
    plan: &RenderPlan,
    flip: Affine2,
    font: FontId2d,
    missing_glyphs: &mut HashMap<NodeId, u32>,
) -> PaintStats {
    let mut total = 0u32;
    for item in plan.items() {
        total += match item.shape {
            ItemShape::Text { .. } => 1,
            _ => shapes::draw_count(&item.shape),
        };
    }

    let mut z_index = 0u32;
    let mut stats = PaintStats {
        non_finite_items: plan.non_finite_items(),
        ..PaintStats::default()
    };
    for item in plan.items() {
        match item.shape {
            ItemShape::Text { size, color, align } => {
                if let Some(missing) = draw_text(
                    canvas,
                    document,
                    item,
                    font,
                    size,
                    color,
                    align,
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
                stats.shape_draws += shapes::draw_item(canvas, item, flip, &mut z_index, total);
            }
        }
    }
    stats
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
        space::rgba(color, item.opacity),
    );
    Some(missing)
}

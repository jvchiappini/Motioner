//! Frame loop: clear colour, draw counting, submission and readback.
//!
//! The frame is built in two passes over the plan with no allocation: the first
//! counts draws so [`crate::space::z_for`] can spread the painter order over a
//! fixed depth window, and the second submits fill/stroke/text work in plan
//! order. The engine sorts by `z`, so shapes and text interleave exactly in the
//! order the evaluator produced.

use std::collections::HashMap;

use ferrous_renderer::headless::{HarnessError, Pure2dHarness, DEFAULT_FORMAT};
use motioner_core::{Color, Document, NodeContent, TextAlign};
use motioner_eval::{Affine2, ItemShape, RenderItem};

use crate::space::{self, to_f32};
use crate::{shapes, text, FrameRenderer, FrameStats, RenderError};

/// Roboto Regular, embedded so rendering never depends on the working
/// directory. Any other requested family falls back to this font.
const ROBOTO: &[u8] = include_bytes!("../../../engine/ferrous/assets/fonts/Roboto-Regular.ttf");

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
    plan: &motioner_eval::RenderPlan,
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

    let mut total = 0u32;
    for item in plan.items() {
        total += match item.shape {
            ItemShape::Text { .. } => 1,
            _ => shapes::draw_count(&item.shape),
        };
    }

    let flip = space::composition_flip(renderer.width, renderer.height);
    let mut z_index = 0u32;
    let mut shape_draws = 0u32;
    let mut text_draws = 0u32;
    let mut missing_glyphs = 0u32;
    for item in plan.items() {
        match item.shape {
            ItemShape::Text { size, color, align } => {
                if let Some(missing) = draw_text(
                    renderer,
                    document,
                    item,
                    size,
                    color,
                    align,
                    flip,
                    &mut z_index,
                    total,
                ) {
                    text_draws += 1;
                    missing_glyphs = missing_glyphs.saturating_add(missing);
                }
            }
            _ => {
                shape_draws +=
                    shapes::draw_item(&mut renderer.harness, item, flip, &mut z_index, total);
            }
        }
    }

    let report = renderer.harness.render_frame();
    renderer
        .harness
        .read_pixels_into(out)
        .map_err(|error| RenderError::Readback {
            message: error.to_string(),
        })?;

    Ok(FrameStats {
        shape_draws,
        text_draws,
        draw_calls: report.draw_calls,
        missing_glyphs,
        non_finite_items: plan.non_finite_items(),
        cpu_ms: report.cpu_ms,
    })
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
    renderer: &mut FrameRenderer,
    document: &Document,
    item: &RenderItem,
    size: f64,
    color: Color,
    align: TextAlign,
    flip: Affine2,
    z_index: &mut u32,
    total: u32,
) -> Option<u32> {
    let node = document.node(item.node)?;
    let NodeContent::Text(content) = node.content() else {
        return None;
    };

    let cached = renderer.missing_glyphs.get(&item.node).copied();
    let missing = if let Some(count) = cached {
        count
    } else {
        let count = text::count_missing(&renderer.harness, renderer.font, &content.text, size);
        renderer.missing_glyphs.insert(item.node, count);
        count
    };

    let model = text::text_model(flip, item.transform);
    let transform = space::model_mat4(model, 1.0, 1.0, space::z_for(*z_index, total));
    *z_index += 1;
    renderer.harness.draw_text(
        renderer.font,
        &content.text,
        transform,
        to_f32(size),
        text::align_to_engine(align),
        space::rgba(color, item.opacity),
    );
    Some(missing)
}

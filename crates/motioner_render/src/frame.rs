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
use motioner_core::{CharOrder, CharProperty, CharValue, Frame, Vec2 as CoreVec2};
use motioner_core::{Color, Document, NodeContent, NodeId, TextAlign};
use motioner_eval::{Affine2, EffectValue, ItemShape, RenderItem, RenderPlan};

use crate::fonts::FontSet;
use crate::paths::PathCache;
use crate::space::{self, to_f32};
use crate::{shapes, text, Canvas2d, FrameRenderer, FrameStats, PaintStats, RenderError};

/// Creates the engine harness, registers Roboto and wraps it in a
/// [`FrameRenderer`].
pub(crate) fn create(
    width: u32,
    height: u32,
    fonts: FontSet,
) -> Result<FrameRenderer, RenderError> {
    let mut harness =
        Pure2dHarness::new(width, height, DEFAULT_FORMAT).map_err(map_harness_error)?;
    let roboto = fonts
        .resolve("Roboto", 400)
        .map_err(RenderError::FontFamily)?;
    let font = harness
        .register_font(roboto.bytes, ferrous_font::charset::latin_western())
        .map_err(|error| RenderError::Font {
            message: error.to_string(),
        })?;
    let font_ids = vec![(("Roboto".to_owned(), 400), font)];
    let adapter_name = harness.adapter_name();
    Ok(FrameRenderer {
        width,
        height,
        harness,
        font,
        fonts,
        font_ids,
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
    prepare_fonts(
        &mut renderer.harness,
        document,
        plan,
        &renderer.fonts,
        &mut renderer.font_ids,
    )?;
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
        &renderer.font_ids,
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

/// Registers one atlas per resolved `(family, weight)` the plan uses.
///
/// A missing family is a typed error naming the family and what was searched;
/// it is raised before any draw, so a frame never renders with a wrong face.
fn prepare_fonts(
    harness: &mut Pure2dHarness,
    document: &Document,
    plan: &RenderPlan,
    fonts: &FontSet,
    font_ids: &mut Vec<((String, u16), FontId2d)>,
) -> Result<(), RenderError> {
    for item in plan.items() {
        if !matches!(item.shape, ItemShape::Text { .. }) {
            continue;
        }
        let Some(NodeContent::Text(content)) =
            document.node(item.node).map(motioner_core::Node::content)
        else {
            continue;
        };
        let resolved = fonts
            .resolve(&content.font, content.weight)
            .map_err(RenderError::FontFamily)?;
        let key = (resolved.family.to_owned(), resolved.weight);
        if font_ids
            .iter()
            .any(|((family, weight), _)| *weight == key.1 && family.eq_ignore_ascii_case(&key.0))
        {
            continue;
        }
        let id = harness
            .register_font(resolved.bytes, ferrous_font::charset::latin_western())
            .map_err(|error| RenderError::Font {
                message: error.to_string(),
            })?;
        font_ids.push((key, id));
    }
    Ok(())
}

/// The engine font handle resolved for a text node, falling back to `fallback`.
fn font_for(
    font_ids: &[((String, u16), FontId2d)],
    family: &str,
    weight: u16,
    fallback: FontId2d,
) -> FontId2d {
    font_ids
        .iter()
        .find(|((name, w), _)| *w == weight && name.eq_ignore_ascii_case(family))
        .map_or(fallback, |(_, id)| *id)
}

/// One layer open during the paint loop.
struct OpenLayer<'a> {
    layer: &'a motioner_eval::PlanLayer,
    /// Whether the engine accepted the push (a rejected layer is never popped).
    pushed: bool,
    /// Whether the mask source layer is open.
    mask_open: bool,
}

/// Paints one plan into a canvas and returns the draw counters.
///
/// `flip` maps composition pixels to world space, including any [`crate::ViewFit`]
/// letterboxing. `font` and `missing_glyphs` are owned by the caller so the
/// per-node glyph measurement cache survives across frames.
#[allow(clippy::too_many_arguments)] // the plan, the canvas, the font table and its caches
#[allow(clippy::too_many_lines)] // the plan walk: open, draw, masks, close
pub(crate) fn paint_plan(
    canvas: &mut impl Canvas2d,
    document: &Document,
    plan: &RenderPlan,
    flip: Affine2,
    font: FontId2d,
    font_ids: &[((String, u16), FontId2d)],
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
    // push (invalid effect) is never popped, and whether its mask source layer
    // is open.
    let mut open: Vec<OpenLayer<'_>> = Vec::new();
    let mut next_layer = 0usize;
    for (item_index, item) in plan.items().iter().enumerate() {
        // A mask source starts after all of the layer's content, so the top
        // open layer is the masked one; begin its source before opening the
        // layers that start at this index (the source's own effect layers).
        if let Some(entry) = open.last_mut() {
            if entry.pushed && !entry.mask_open {
                if let Some(mask) = entry.layer.mask {
                    if mask.start == item_index && mask.end > mask.start {
                        canvas.begin_mask();
                        entry.mask_open = true;
                    }
                }
            }
        }
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
            let pushed = match layer.mask {
                Some(mask) => canvas.push_masked_layer(
                    1.0,
                    layer.blend,
                    space::z_for(z_index, total),
                    &effects[..count],
                    engine_mask(&mask),
                ),
                None => canvas.push_layer(
                    1.0,
                    layer.blend,
                    space::z_for(z_index, total),
                    &effects[..count],
                ),
            };
            open.push(OpenLayer {
                layer,
                pushed,
                mask_open: false,
            });
            next_layer += 1;
        }
        match item.shape {
            ItemShape::Text {
                size,
                weight,
                tracking,
                line_height,
                color,
                align,
                gradient,
                animations,
                ..
            } => {
                if let Some(missing) = draw_text(
                    canvas,
                    document,
                    item,
                    font,
                    font_ids,
                    size,
                    weight,
                    tracking,
                    line_height,
                    color,
                    align,
                    gradient.and_then(|index| gradients.base(index)),
                    animations,
                    plan,
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
        while let Some(entry) = open.last() {
            if entry.layer.end != item_index + 1 {
                break;
            }
            if entry.pushed {
                if entry.layer.mask.is_some() {
                    // An empty source (start == end) never reached the
                    // mid-stream begin above; begin it here so the engine
                    // still composes a deterministic transparent mask.
                    if !entry.mask_open {
                        canvas.begin_mask();
                    }
                    canvas.end_mask();
                }
                canvas.pop_layer();
            }
            open.pop();
        }
    }
    stats
}

/// Maps one sampled document mask to the canvas request (D-0033).
fn engine_mask(mask: &motioner_eval::PlanMask) -> crate::CanvasMask {
    crate::CanvasMask {
        mode: mask.mode,
        invert: mask.invert,
    }
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
    fallback_font: FontId2d,
    font_ids: &[((String, u16), FontId2d)],
    size: f64,
    weight: u16,
    tracking: f64,
    line_height: f64,
    color: Color,
    align: TextAlign,
    gradient: Option<u32>,
    animations: Option<usize>,
    plan: &RenderPlan,
    flip: Affine2,
    missing_glyphs: &mut HashMap<NodeId, u32>,
    z_index: &mut u32,
    total: u32,
) -> Option<u32> {
    let node = document.node(item.node)?;
    let NodeContent::Text(content) = node.content() else {
        return None;
    };
    let font = font_for(font_ids, &content.font, weight, fallback_font);

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

    let style = ferrous_2d::render::TextStyle2d {
        size_px: to_f32(size),
        align: text::align_to_engine(align),
        tracking: to_f32(tracking),
        line_height: to_f32(line_height),
    };
    if let Some(index) = animations {
        let mut layout = Vec::new();
        let _ = canvas.layout_text(font, &content.text, style, &mut layout);
        let animators = plan.text_animations(index).unwrap_or(&[]);
        let draws = animated_draws(
            item,
            &layout,
            animators,
            plan.frame(),
            color,
            gradient,
            &transform,
        );
        canvas.glyphs(font, &draws);
    } else {
        canvas.text_styled(
            font,
            &content.text,
            transform,
            style,
            space::linear_rgba(color, item.opacity),
            gradient,
        );
    }
    Some(missing)
}

/// A glyph index as a stagger rank (text blocks are far below `u32::MAX`).
fn glyph_rank(index: usize) -> f64 {
    f64::from(u32::try_from(index).unwrap_or(u32::MAX))
}

/// A frame delta as `f64`.
#[allow(clippy::cast_precision_loss)] // frame counts stay far below 2^52
fn frame_f64(frames: Frame) -> f64 {
    frames as f64
}

/// Builds one per-glyph draw per laid-out character, applying every animator
/// in order (D-0034).
#[allow(clippy::too_many_arguments)] // the item, its layout and the frame
fn animated_draws(
    item: &RenderItem,
    layout: &[ferrous_font::GlyphLayout],
    animators: &[motioner_core::TextAnimator],
    frame: Frame,
    base_color: Color,
    gradient: Option<u32>,
    base_transform: &glam::Mat4,
) -> Vec<ferrous_2d::render::GlyphDraw2d> {
    let count = layout.len();
    let centre = (glyph_rank(count) - 1.0) * 0.5;
    layout
        .iter()
        .enumerate()
        .map(|(index, glyph)| {
            let mut state = GlyphState::default();
            for animator in animators {
                let rank = match animator.order {
                    CharOrder::Forward => glyph_rank(index),
                    CharOrder::Reverse => glyph_rank(count - 1 - index),
                    CharOrder::Center => (glyph_rank(index) - centre).abs(),
                };
                let local = frame_f64(frame - animator.start) - rank * animator.stagger;
                let progress = (local / animator.duration).clamp(0.0, 1.0);
                state.apply(animator, motioner_eval::ease(animator.ease, progress));
            }
            let color = match state.color {
                Some(animated) => space::linear_rgba(animated, item.opacity * state.opacity),
                None => space::linear_rgba(base_color, item.opacity * state.opacity),
            };
            let gradient = if state.color.is_some() {
                None
            } else {
                gradient
            };
            let transform = if state.is_identity() {
                *base_transform
            } else {
                let pivot = glam::Vec3::new(glyph.x, glyph.baseline_y, 0.0);
                let offset = glam::Vec3::new(to_f32(state.offset.x), -to_f32(state.offset.y), 0.0);
                let local = glam::Mat4::from_translation(pivot + offset)
                    * glam::Mat4::from_rotation_z(to_f32(state.rotation).to_radians())
                    * glam::Mat4::from_scale(glam::Vec3::new(
                        to_f32(state.scale),
                        to_f32(state.scale),
                        1.0,
                    ))
                    * glam::Mat4::from_translation(-pivot);
                *base_transform * local
            };
            ferrous_2d::render::GlyphDraw2d {
                glyph: glyph.clone(),
                transform,
                color,
                gradient,
            }
        })
        .collect()
}

/// Per-glyph animated state, starting from the identity.
struct GlyphState {
    opacity: f64,
    offset: CoreVec2,
    scale: f64,
    rotation: f64,
    color: Option<Color>,
}

impl Default for GlyphState {
    fn default() -> Self {
        Self {
            opacity: 1.0,
            offset: CoreVec2::default(),
            scale: 1.0,
            rotation: 0.0,
            color: None,
        }
    }
}

impl GlyphState {
    fn is_identity(&self) -> bool {
        (self.opacity - 1.0).abs() < f64::EPSILON
            && self.offset.x.abs() < f64::EPSILON
            && self.offset.y.abs() < f64::EPSILON
            && (self.scale - 1.0).abs() < f64::EPSILON
            && self.rotation.abs() < f64::EPSILON
    }

    fn apply(&mut self, animator: &motioner_core::TextAnimator, progress: f64) {
        match (animator.property, animator.from, animator.to) {
            (CharProperty::Opacity, CharValue::Number(from), CharValue::Number(to)) => {
                self.opacity = from + (to - from) * progress;
            }
            (CharProperty::Offset, CharValue::Vec2(from), CharValue::Vec2(to)) => {
                self.offset = CoreVec2::new(
                    from.x + (to.x - from.x) * progress,
                    from.y + (to.y - from.y) * progress,
                );
            }
            (CharProperty::Scale, CharValue::Number(from), CharValue::Number(to)) => {
                self.scale = from + (to - from) * progress;
            }
            (CharProperty::Rotation, CharValue::Number(from), CharValue::Number(to)) => {
                self.rotation = from + (to - from) * progress;
            }
            (CharProperty::Color, CharValue::Color(from), CharValue::Color(to)) => {
                self.color = Some(Color::lerp(from, to, progress));
            }
            _ => {}
        }
    }
}

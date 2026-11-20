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

use rustc_hash::FxHashMap as HashMap;

use ferrous_2d::render::{Effect2d, FontId2d, MAX_EFFECTS_PER_LAYER};
use ferrous_renderer::headless::{HarnessError, Pure2dHarness, DEFAULT_FORMAT};
use motioner_core::{CharOrder, CharProperty, CharValue, Frame, Vec2 as CoreVec2};
use motioner_core::{Color, Document, NodeContent, NodeId, TextAlign};
use motioner_eval::{Affine2, EffectValue, ItemShape, RenderItem, RenderPlan};

use crate::fonts::FontSet;
use crate::paths::PathCache;
use crate::space::{self, to_f32};
use crate::{
    proxy, shapes, text, Canvas2d, FrameRenderer, FrameStats, PaintStats, ProxyTarget, RenderError,
};

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
        missing_glyphs: HashMap::default(),
        path_cache: PathCache::default(),
        images: crate::ImageSet::new(),
        image_textures: crate::ImageTextures::default(),
        image_targets: HashMap::default(),
        proxy: crate::ProxyTarget::Full,
        image_scratch: Vec::new(),
        image_inventory: crate::ImageInventory::new(),
        mesh_vertices: Vec::new(),
        mesh_indices: HashMap::default(),
        texture_budget: crate::DEFAULT_TEXTURE_BUDGET,
        evaluator: motioner_eval::Evaluator::new(),
        plan: RenderPlan::new(),
    })
}

/// A conservative axis-aligned box in composition pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Aabb {
    /// Left edge.
    pub min_x: f64,
    /// Top edge.
    pub min_y: f64,
    /// Right edge.
    pub max_x: f64,
    /// Bottom edge.
    pub max_y: f64,
}

/// The conservative composition-space AABB of one plan item, or `None` when
/// the plan has no cheap bound for it (text needs measurement, and a missing
/// path/mesh is not culled either).
///
/// The box is always at least as large as what the item draws: the local
/// bounds include the stroke, an image mesh takes the hull of its points, and
/// the item transform is applied to the local box corners (the AABB of a
/// transformed box contains the transformed contents for any affine).
/// The item's local box (before the item transform) and the outward pad a
/// non-scaling stroke needs in composition space.
fn local_bounds(
    item: &RenderItem,
    plan: &RenderPlan,
) -> Option<(CoreVec2, CoreVec2, f64)> {
    let (local_min, local_max, non_scaling) = match item.shape {
        ItemShape::Rect {
            width,
            height,
            paint,
            ..
        } => {
            let half_stroke = paint
                .stroke
                .map_or(0.0, |stroke| stroke.width.max(0.0) * 0.5);
            let half_w = width.abs() * 0.5 + half_stroke;
            let half_h = height.abs() * 0.5 + half_stroke;
            (
                CoreVec2::new(-half_w, -half_h),
                CoreVec2::new(half_w, half_h),
                0.0,
            )
        }
        ItemShape::Circle { radius, paint, .. } => {
            let half_stroke = paint
                .stroke
                .map_or(0.0, |stroke| stroke.width.max(0.0) * 0.5);
            let r = radius.abs() + half_stroke;
            (CoreVec2::new(-r, -r), CoreVec2::new(r, r), 0.0)
        }
        ItemShape::Path { index } => {
            let path = plan.path(index)?;
            let bounds = crate::path_bounds(path)?;
            let stroke = path.paint.stroke;
            // `path_bounds` covers half the stroke width. A sharp miter join can
            // reach up to twice the width past the centreline, so pad the local
            // box by that much rather than under-estimating an off-screen edge.
            let miter = stroke.map_or(0.0, |stroke| stroke.width.abs() * 2.0);
            let grow = stroke
                .and_then(|stroke| stroke.non_scaling)
                .map_or(0.0, |width| width.abs() * 0.5);
            (
                CoreVec2::new(bounds[0] - miter, bounds[1] - miter),
                CoreVec2::new(bounds[2] + miter, bounds[3] + miter),
                grow,
            )
        }
        ItemShape::Image {
            width,
            height,
            mesh,
            ..
        } => {
            if let Some(sample) = mesh.and_then(|index| plan.mesh(index)) {
                if sample.points.is_empty() {
                    (
                        CoreVec2::new(-width.abs() * 0.5, -height.abs() * 0.5),
                        CoreVec2::new(width.abs() * 0.5, height.abs() * 0.5),
                        0.0,
                    )
                } else {
                    let mut min = sample.points[0];
                    let mut max = sample.points[0];
                    for point in &sample.points[1..] {
                        min.x = min.x.min(point.x);
                        min.y = min.y.min(point.y);
                        max.x = max.x.max(point.x);
                        max.y = max.y.max(point.y);
                    }
                    (min, max, 0.0)
                }
            } else {
                (
                    CoreVec2::new(-width.abs() * 0.5, -height.abs() * 0.5),
                    CoreVec2::new(width.abs() * 0.5, height.abs() * 0.5),
                    0.0,
                )
            }
        }
        // Text bounds need the measured glyph layout; culling it would need a
        // measurement on every frame, so it is always drawn.
        ItemShape::Text { .. } => return None,
    };
    Some((local_min, local_max, non_scaling))
}

pub(crate) fn item_bounds(item: &RenderItem, plan: &RenderPlan) -> Option<Aabb> {
    let (local_min, local_max, non_scaling) = local_bounds(item, plan)?;
    let transform = item.transform;
    let corners = [
        transform.apply(local_min),
        transform.apply(CoreVec2::new(local_max.x, local_min.y)),
        transform.apply(CoreVec2::new(local_min.x, local_max.y)),
        transform.apply(local_max),
    ];
    let mut aabb = Aabb {
        min_x: f64::INFINITY,
        min_y: f64::INFINITY,
        max_x: f64::NEG_INFINITY,
        max_y: f64::NEG_INFINITY,
    };
    for corner in corners {
        if !corner.x.is_finite() || !corner.y.is_finite() {
            return None;
        }
        aabb.min_x = aabb.min_x.min(corner.x);
        aabb.min_y = aabb.min_y.min(corner.y);
        aabb.max_x = aabb.max_x.max(corner.x);
        aabb.max_y = aabb.max_y.max(corner.y);
    }
    if non_scaling > 0.0 {
        aabb.min_x -= non_scaling;
        aabb.min_y -= non_scaling;
        aabb.max_x += non_scaling;
        aabb.max_y += non_scaling;
    }
    Some(aabb)
}

/// Whether the item at `index` belongs to a layer with an effect chain.
///
/// A blur, glow or drop shadow can bleed ink from outside its source item's
/// bounds, so culling such an item would change pixels near the edge. Those
/// items are always kept.
fn in_effected_layer(index: usize, plan: &RenderPlan) -> bool {
    plan.layers()
        .iter()
        .any(|layer| !layer.effects.is_empty() && layer.start <= index && index < layer.end)
}

/// Whether the item at `index` is entirely outside `visible` (`[x, y, width,
/// height]`, in composition pixels) and can be skipped.
///
/// Only a *strict* separation culls: an item that touches or crosses the
/// boundary is kept, and an item with no cheap bound (text, or one inside an
/// effect layer) is always kept.
pub(crate) fn item_is_culled(
    index: usize,
    item: &RenderItem,
    plan: &RenderPlan,
    visible: [f64; 4],
) -> bool {
    if !(visible[0].is_finite()
        && visible[1].is_finite()
        && visible[2].is_finite()
        && visible[3].is_finite()
        && visible[2] >= 0.0
        && visible[3] >= 0.0)
    {
        return false;
    }
    let Some(bounds) = item_bounds(item, plan) else {
        return false;
    };
    let (right, bottom) = (visible[0] + visible[2], visible[1] + visible[3]);
    let separated = bounds.max_x < visible[0]
        || bounds.min_x > right
        || bounds.max_y < visible[1]
        || bounds.min_y > bottom;
    // Only a candidate (already fully outside) needs the effect-layer check,
    // which walks the layer list; items on screen never pay for it.
    separated && !in_effected_layer(index, plan)
}

/// Uploads the textures the plan draws, one per image key.
///
/// `targets` is a reused per-key map, so a frame that draws several items of
/// the same image uploads one texture sized for the largest of them.  With
/// [`ProxyTarget::PerImage`] the size comes from
/// [`crate::proxy::needed_size`]; `view_scale` is the composition-pixel to
/// physical-pixel factor (1.0 for the headless target).  `hidden` matches
/// [`paint_plan`]'s skip set, so a solo/shy preview never fetches an image it
/// will not draw.  `visible` is the composition-space region [`paint_plan`]
/// will draw, so an off-screen item does not upload a texture either.
/// `scratch` is reused for a downscale.
#[allow(clippy::too_many_arguments)] // the plan, the proxy and its scratch
pub(crate) fn prepare_images(
    canvas: &mut impl Canvas2d,
    images: &crate::ImageSet,
    textures: &mut crate::ImageTextures,
    targets: &mut HashMap<motioner_core::ImageKey, (u32, u32)>,
    plan: &RenderPlan,
    proxy: ProxyTarget,
    view_scale: f64,
    hidden: Option<&std::collections::BTreeSet<NodeId>>,
    visible: Option<[f64; 4]>,
    scratch: &mut Vec<u8>,
) -> Result<(), RenderError> {
    textures.clear_missing();
    targets.clear();
    for (index, item) in plan.items().iter().enumerate() {
        if hidden.is_some_and(|hidden| hidden.contains(&item.node))
            || visible.is_some_and(|rect| item_is_culled(index, item, plan, rect))
        {
            continue;
        }
        let ItemShape::Image {
            key: Some(key),
            width,
            height,
            uv,
            ..
        } = item.shape
        else {
            continue;
        };
        let Some(image) = images.get(&key) else {
            continue;
        };
        let target = match proxy {
            ProxyTarget::PerImage => {
                let (scale_x, scale_y) = space::world_axis_scales(item.transform);
                proxy::needed_size(
                    (image.width, image.height),
                    uv,
                    (width, height),
                    (scale_x, scale_y),
                    view_scale,
                )
            }
            other => proxy::whole_image_size(image, other),
        };
        let slot = targets.entry(key).or_insert(target);
        slot.0 = slot.0.max(target.0);
        slot.1 = slot.1.max(target.1);
    }
    let mut first_error = None;
    for (key, target) in targets.iter() {
        let Some(image) = images.get(key) else {
            continue;
        };
        if let Err(error) =
            crate::ensure_image_texture(canvas, textures, *key, image, *target, scratch)
        {
            // Try every key, then report the first failure: one bad image must
            // not stop the others from becoming drawable.
            if first_error.is_none() {
                first_error = Some(error);
            }
        }
    }
    match first_error {
        Some(error) => Err(error),
        None => Ok(()),
    }
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

/// Paints one plan into the harness target and submits the frame.
///
/// The caller decides whether to read the target back (`render`) or accumulate
/// it as a motion-blur sub-frame (`render_motion_blur`).
fn render_plan_into_target(
    renderer: &mut FrameRenderer,
    document: &Document,
    plan: &RenderPlan,
) -> Result<(PaintStats, ferrous_renderer::headless::FrameReport), RenderError> {
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
    prepare_images(
        &mut renderer.harness,
        &renderer.images,
        &mut renderer.image_textures,
        &mut renderer.image_targets,
        plan,
        renderer.proxy,
        1.0,
        None,
        None,
        &mut renderer.image_scratch,
    )?;
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
        &renderer.images,
        &renderer.image_textures,
        &mut renderer.mesh_vertices,
        &mut renderer.mesh_indices,
        None,
        None,
    );
    let report = renderer.harness.render_frame();
    Ok((stats, report))
}

fn read_target(renderer: &mut FrameRenderer, out: &mut Vec<u8>) -> Result<(), RenderError> {
    let bytes = renderer.harness.renderer().texture_bytes_2d();
    if bytes > renderer.texture_budget {
        return Err(RenderError::TextureBudget {
            bytes,
            limit: renderer.texture_budget,
        });
    }
    renderer
        .harness
        .read_pixels_into(out)
        .map_err(|error| RenderError::Readback {
            message: error.to_string(),
        })
}

fn stats_from(paint: PaintStats, report: ferrous_renderer::headless::FrameReport) -> FrameStats {
    FrameStats {
        shape_draws: paint.shape_draws,
        text_draws: paint.text_draws,
        draw_calls: report.draw_calls,
        missing_glyphs: paint.missing_glyphs,
        non_finite_items: paint.non_finite_items,
        cpu_ms: report.cpu_ms,
    }
}

/// Renders one plan into `out` and returns the frame counters.
pub(crate) fn render(
    renderer: &mut FrameRenderer,
    document: &Document,
    plan: &RenderPlan,
    out: &mut Vec<u8>,
) -> Result<FrameStats, RenderError> {
    let (paint, report) = render_plan_into_target(renderer, document, plan)?;
    read_target(renderer, out)?;
    Ok(stats_from(paint, report))
}

/// Renders integer frame `frame`, honouring the composition's motion blur.
///
/// With `motion_blur` off this is exactly [`render`] with an integer plan, so
/// existing goldens stay byte-identical. With it on, the frame is evaluated at
/// `samples` sub-frame times centred on the frame and averaged in linear light
/// through the engine accumulation API.
pub(crate) fn render_frame(
    renderer: &mut FrameRenderer,
    document: &Document,
    frame: Frame,
    out: &mut Vec<u8>,
) -> Result<FrameStats, RenderError> {
    let composition = document.composition();
    // Move the scratch out of the renderer so the frame helpers can borrow
    // the renderer mutably; the buffers are put back unchanged afterwards.
    let mut evaluator = std::mem::take(&mut renderer.evaluator);
    let mut plan = std::mem::take(&mut renderer.plan);
    let stats = if composition.motion_blur() {
        render_motion_blur(renderer, document, frame, &mut evaluator, &mut plan, out)
    } else {
        evaluator.evaluate(document, frame, &mut plan);
        render(renderer, document, &plan, out)
    };
    renderer.evaluator = evaluator;
    renderer.plan = plan;
    stats
}

/// The sub-frame sampling times of a motion-blurred frame (D-0037).
///
/// Exposed so the editor preview accumulates exactly the same instants as
/// `still` and `render`.
pub fn motion_blur_times(document: &Document, frame: Frame, samples: u32) -> Vec<f64> {
    let composition = document.composition();
    let shutter = composition.shutter();
    let samples = samples.clamp(1, 64);
    (0..samples)
        .map(|index| {
            let subframe = f64::from(index);
            let offset = (shutter / 360.0) * ((subframe + 0.5) / f64::from(samples) - 0.5);
            #[allow(clippy::cast_precision_loss)] // frames are bounded by 2^31
            {
                frame as f64 + offset
            }
        })
        .collect()
}

fn render_motion_blur(
    renderer: &mut FrameRenderer,
    document: &Document,
    frame: Frame,
    evaluator: &mut motioner_eval::Evaluator,
    plan: &mut RenderPlan,
    out: &mut Vec<u8>,
) -> Result<FrameStats, RenderError> {
    let samples = document.composition().samples();
    render_motion_blur_samples(renderer, document, frame, samples, evaluator, plan, out)
}

pub(crate) fn render_motion_blur_samples(
    renderer: &mut FrameRenderer,
    document: &Document,
    frame: Frame,
    samples: u32,
    evaluator: &mut motioner_eval::Evaluator,
    plan: &mut RenderPlan,
    out: &mut Vec<u8>,
) -> Result<FrameStats, RenderError> {
    let times = motion_blur_times(document, frame, samples);
    renderer
        .harness
        .begin_accumulation_2d()
        .map_err(|error| RenderError::Accumulation {
            message: error.to_string(),
        })?;
    let mut last = FrameStats::default();
    for time in times {
        evaluator.evaluate_fractional(document, frame, time, plan);
        let (paint, report) = render_plan_into_target(renderer, document, plan)?;
        renderer
            .harness
            .accumulate_frame_2d(1.0)
            .map_err(|error| RenderError::Accumulation {
                message: error.to_string(),
            })?;
        last = stats_from(paint, report);
    }
    renderer
        .harness
        .resolve_accumulation_2d()
        .map_err(|error| RenderError::Accumulation {
            message: error.to_string(),
        })?;
    read_target(renderer, out)?;
    Ok(last)
}

/// One font request per text item in `plan`, in plan order.
fn text_faces<'a>(
    document: &'a Document,
    plan: &'a RenderPlan,
) -> impl Iterator<Item = (&'a str, u16)> + 'a {
    plan.items().iter().filter_map(move |item| {
        if !matches!(item.shape, ItemShape::Text { .. }) {
            return None;
        }
        let NodeContent::Text(content) = document.node(item.node)?.content() else {
            return None;
        };
        Some((content.font.as_str(), content.weight))
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
    for (family, weight) in text_faces(document, plan) {
        let resolved = fonts
            .resolve(family, weight)
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
    images: &crate::ImageSet,
    image_textures: &crate::ImageTextures,
    mesh_vertices: &mut Vec<ferrous_2d::render::MeshVertex2d>,
    mesh_indices: &mut HashMap<(u32, u32), Vec<u32>>,
    hidden: Option<&std::collections::BTreeSet<NodeId>>,
    visible: Option<[f64; 4]>,
) -> PaintStats {
    let culled = |index: usize, item: &RenderItem| {
        hidden.is_some_and(|hidden| hidden.contains(&item.node))
            || visible.is_some_and(|rect| item_is_culled(index, item, plan, rect))
    };
    let mut total = 0u32;
    {
        // Prompt 17 S2: first-frame composition paint, split shapes/items.
        let _span = ferrous_core::startup::span("motioner: paint shapes");
        for (index, item) in plan.items().iter().enumerate() {
            if !culled(index, item) {
                total += shapes::draw_layers(plan, &item.shape);
            }
        }
    }
    let _items_span = ferrous_core::startup::span("motioner: paint items");

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
        let skipped = culled(item_index, item);
        match item.shape {
            _ if skipped => {}
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
                let _text_span = ferrous_core::startup::span("motioner: paint text");
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
                let _shape_span = ferrous_core::startup::span("motioner: paint images and shapes");
                stats.shape_draws += shapes::draw_item(
                    canvas,
                    plan,
                    path_cache,
                    gradients,
                    images,
                    image_textures,
                    mesh_vertices,
                    mesh_indices,
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
        let count = text::count_missing(canvas, font, &content.text);
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

#[cfg(test)]
mod tests {
    use super::*;
    use motioner_core::Vec2;
    use motioner_eval::ResolvedPaint;

    fn rect_item(position: Vec2, width: f64, height: f64) -> RenderItem {
        RenderItem {
            node: NodeId::from_raw(1),
            transform: Affine2::translation(position),
            opacity: 1.0,
            blend: motioner_core::BlendMode::Normal,
            shape: ItemShape::Rect {
                width,
                height,
                radius: 0.0,
                paint: ResolvedPaint::default(),
            },
        }
    }

    const VISIBLE: [f64; 4] = [0.0, 0.0, 100.0, 100.0];

    #[test]
    fn a_fully_off_canvas_item_is_culled() {
        let plan = RenderPlan::new();
        let right = rect_item(Vec2::new(200.0, 50.0), 10.0, 10.0);
        assert!(item_is_culled(0, &right, &plan, VISIBLE));
        let above = rect_item(Vec2::new(50.0, -200.0), 10.0, 10.0);
        assert!(item_is_culled(0, &above, &plan, VISIBLE));
    }

    #[test]
    fn an_item_that_intersects_the_canvas_is_kept() {
        let plan = RenderPlan::new();
        let inside = rect_item(Vec2::new(50.0, 50.0), 10.0, 10.0);
        assert!(!item_is_culled(0, &inside, &plan, VISIBLE));
        // Straddles the right edge: half in, half out.
        let edge = rect_item(Vec2::new(99.0, 50.0), 10.0, 10.0);
        assert!(!item_is_culled(0, &edge, &plan, VISIBLE));
        // Exactly touching the edge still counts as visible.
        let touching = rect_item(Vec2::new(105.0, 50.0), 10.0, 10.0);
        assert!(!item_is_culled(0, &touching, &plan, VISIBLE));
    }

    #[test]
    fn an_item_without_a_cheap_bound_is_never_culled() {
        let plan = RenderPlan::new();
        let text = RenderItem {
            node: NodeId::from_raw(2),
            transform: Affine2::translation(Vec2::new(10_000.0, 10_000.0)),
            opacity: 1.0,
            blend: motioner_core::BlendMode::Normal,
            shape: ItemShape::Text {
                size: 12.0,
                weight: 400,
                tracking: 0.0,
                line_height: 1.0,
                color: Color::WHITE,
                gradient: None,
                align: motioner_core::TextAlign::Left,
                multiline: false,
                animations: None,
            },
        };
        assert!(item_bounds(&text, &plan).is_none());
        assert!(!item_is_culled(0, &text, &plan, VISIBLE));
    }

    #[test]
    fn an_off_canvas_item_inside_an_effected_layer_is_kept() {
        // A blur/glow/shadow can bleed ink past the item's own bounds, so an
        // off-canvas source inside such a layer must still be drawn. The DSL
        // gives us a real plan with one effect layer.
        let source = "comp(name=\"C\", width=100, height=100, fps=60, duration=1)\n\
                      r = rect(10, 10)\n\
                      r.pos = (1000, 1000)\n\
                      r.effect(\"blur\", radius=64)\n";
        let document = motioner_dsl::run(source, &motioner_dsl::Limits::default())
            .expect("the effect test document builds");
        let mut plan = RenderPlan::new();
        motioner_eval::Evaluator::new().evaluate(&document, 0, &mut plan);
        assert!(!plan.items().is_empty());
        assert!(!plan.layers().is_empty());
        assert!(!item_is_culled(0, &plan.items()[0], &plan, VISIBLE));
    }
}

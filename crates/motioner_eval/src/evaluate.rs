//! Frame evaluation: document plus frame to render plan.

use std::collections::HashSet;

use motioner_core::{
    AnimatableVec2, Axis, BlendMode, DashSpec, Document, Effect, Frame, GeometryChannel, Gradient,
    Mesh, Node, NodeContent, NodeId, PaintSpec, TextAnimator, TrimSpec,
};

use crate::affine::Affine2;
use crate::curve::{ease, keys_outside_range, sample_at, sample_geometry_at, sample_vec2_at};
use crate::plan::{
    EffectValue, ItemShape, PlanLayer, PlanMask, RenderItem, RenderPlan, ResolvedFill,
    ResolvedGradient, ResolvedPaint, ResolvedPath, ResolvedStop, ResolvedStroke,
};

/// One entry of the traversal stack.
#[derive(Debug, Clone, Copy)]
enum Pending {
    /// Visit a node, with its parent's resolved state.
    ///
    /// `mask_for` marks a visit of a mask source: its items belong to the mask
    /// range of the layer at that index and are not drawn as content.
    Visit {
        id: NodeId,
        parent_transform: Affine2,
        parent_opacity: f64,
        mask_for: Option<usize>,
    },
    /// Close the layer opened for the node at the given index.
    CloseLayer { index: usize },
    /// Close the mask range of the layer at the given index.
    CloseMask { index: usize },
    /// A top-level subtree finished; capture it for the static-subtree cache.
    CloseRoot {
        root: NodeId,
        starts: FragmentStarts,
    },
}

/// Where a root subtree's contribution begins in the plan being built.
#[derive(Debug, Clone, Copy)]
struct FragmentStarts {
    items: usize,
    layers: usize,
    paths: usize,
    meshes: usize,
    gradients: usize,
    text_animations: usize,
    non_finite_items: u32,
}

impl FragmentStarts {
    fn of(plan: &RenderPlan) -> Self {
        Self {
            items: plan.items.len(),
            layers: plan.layers.len(),
            paths: plan.paths.len(),
            meshes: plan.meshes.len(),
            gradients: plan.gradients.len(),
            text_animations: plan.text_animations.len(),
            non_finite_items: plan.non_finite_items,
        }
    }
}

/// The plan contribution of one top-level subtree, kept between frames.
#[derive(Debug, Default)]
struct RootFragment {
    items: Vec<RenderItem>,
    layers: Vec<PlanLayer>,
    paths: Vec<ResolvedPath>,
    meshes: Vec<motioner_core::MeshSample>,
    gradients: Vec<ResolvedGradient>,
    text_animations: Vec<Vec<TextAnimator>>,
    non_finite_items: u32,
}

/// One cached root subtree, valid for the frames it was evaluated at.
#[derive(Debug)]
struct RootCacheEntry {
    revision: u64,
    /// Composition time the fragment was produced at.
    time: f64,
    fragment: RootFragment,
}

/// Arena bases used to shift a fragment's local indices into a live plan.
#[derive(Debug, Clone, Copy)]
struct FragmentBases {
    items: usize,
    paths: usize,
    meshes: usize,
    gradients: usize,
    text_animations: usize,
}

/// Allocations from the previous plan, recycled into cached fragments.
#[derive(Debug, Default)]
struct FragmentScratch {
    layers: Vec<PlanLayer>,
    gradients: Vec<ResolvedGradient>,
    text_animations: Vec<Vec<TextAnimator>>,
}

/// Evaluates documents into render plans.
///
/// Evaluation is a pure function of the document and the frame: no clock,
/// no randomness, no I/O. The evaluator keeps a traversal stack between
/// calls so steady-state evaluation does not allocate.
#[derive(Debug, Default)]
pub struct Evaluator {
    stack: Vec<Pending>,
    /// Every node used as a mask source (skipped in the normal stream).
    mask_sources: HashSet<NodeId>,
    /// Last evaluated top-level subtrees, indexed by `NodeId::raw()`.
    root_cache: Vec<Option<RootCacheEntry>>,
    /// Memo for the current window: 0 unknown, 1 visiting, 2 constant, 3 not.
    static_states: Vec<u8>,
    /// Scratch stack of open layer indices for the effect-layer flags.
    effect_stack: Vec<usize>,
    /// Recycled layer, gradient and text-animation storage.
    scratch: FragmentScratch,
    /// Revision the cache entries belong to.
    cache_revision: Option<u64>,
    /// Time of the previous evaluation covered by the cache.
    cache_time: Option<f64>,
    /// Frame of the previous evaluation covered by the cache.
    cache_frame: Option<Frame>,
    /// Test-only count of reused root subtrees.
    #[cfg(test)]
    cache_hits: u32,
    /// Test-only count of re-evaluated root subtrees.
    #[cfg(test)]
    cache_misses: u32,
}

impl Evaluator {
    /// Creates an evaluator.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Fills `plan` with the items visible at `frame`, in paint order.
    ///
    /// Nodes are visited depth-first: a node is painted before its
    /// children, and siblings in their stored order. Opacity multiplies
    /// down the hierarchy; a subtree whose accumulated opacity is zero is
    /// skipped. Groups transform their children and draw nothing.
    ///
    /// A node whose resolved transform or accumulated opacity is not finite
    /// is skipped together with its subtree, because no renderer can draw
    /// it; each such skip increments [`RenderPlan::non_finite_items`].
    #[allow(clippy::too_many_lines)] // one pass over the traversal kinds, kept linear
    pub fn evaluate(&mut self, document: &Document, frame: Frame, plan: &mut RenderPlan) {
        self.evaluate_fractional(document, frame, frame_time(frame), plan);
    }

    /// Fills `plan` with the items visible at a fractional frame `time`.
    ///
    /// `frame` is the integer frame used for the plan metadata (and therefore
    /// for deterministic caching); `time` is the composition time in frames and
    /// may be fractional. The integer [`Self::evaluate`] path passes the exact
    /// `f64` conversion of `frame`, so it stays byte-identical.
    pub fn evaluate_fractional(
        &mut self,
        document: &Document,
        frame: Frame,
        time: f64,
        plan: &mut RenderPlan,
    ) {
        self.reset(document, frame, plan);
        self.push_children(document.roots(), Affine2::IDENTITY, 1.0);
        self.run(document, frame, time, plan, None);
        self.finish(plan);
    }

    /// Fills `plan` with the items visible at `frame`, reusing cached static
    /// top-level subtrees when nothing they read changed.
    ///
    /// `revision` identifies the document contents: bump it whenever the
    /// document changes. Consecutive calls with the same revision reuse a
    /// top-level subtree when no keyframe in its whole read closure (its
    /// descendants and every mask source they read) lies between the previous
    /// evaluation time and `frame`. The produced plan is item-for-item
    /// identical to [`Self::evaluate`] at the same frame; only the work is
    /// skipped. A revision change invalidates every cached subtree.
    pub fn evaluate_cached(
        &mut self,
        document: &Document,
        revision: u64,
        frame: Frame,
        plan: &mut RenderPlan,
    ) {
        let time = frame_time(frame);
        let previous = self.prepare_cache(document, revision);
        let previous_time = previous.map(|(_, time)| time);
        let window = previous.map(|(from, _)| {
            if from <= frame {
                (from, frame)
            } else {
                (frame, from)
            }
        });
        if let Some((from, to)) = window {
            self.compute_static(document, from, to);
        }
        self.recycle_plan_arenas(plan);
        self.reset(document, frame, plan);
        for &root in document.roots() {
            let constant = self.static_states.get(root.raw() as usize).copied() == Some(2);
            let hit = window.is_some()
                && constant
                && self
                    .root_cache
                    .get(root.raw() as usize)
                    .is_some_and(|slot| {
                        slot.as_ref().is_some_and(|entry| {
                            entry.revision == revision
                                && previous_time.is_some_and(|previous| {
                                    entry.time.to_bits() == previous.to_bits()
                                })
                        })
                    });
            if hit {
                let mut reused = false;
                if let Some(Some(entry)) = self.root_cache.get_mut(root.raw() as usize) {
                    append_fragment(plan, &entry.fragment, &mut self.scratch);
                    entry.time = time;
                    reused = true;
                }
                #[cfg(test)]
                {
                    if reused {
                        self.cache_hits = self.cache_hits.saturating_add(1);
                    }
                }
                if reused {
                    continue;
                }
            }
            #[cfg(test)]
            {
                self.cache_misses = self.cache_misses.saturating_add(1);
            }
            let starts = FragmentStarts::of(plan);
            self.stack.push(Pending::CloseRoot { root, starts });
            self.push_children(std::slice::from_ref(&root), Affine2::IDENTITY, 1.0);
            self.run(document, frame, time, plan, Some((revision, time)));
        }
        self.finish(plan);
        self.scratch.layers.clear();
        self.scratch.gradients.clear();
        self.scratch.text_animations.clear();
        self.cache_revision = Some(revision);
        self.cache_time = Some(time);
        self.cache_frame = Some(frame);
    }

    /// Moves the previous plan's voidable arenas into the recycle scratch.
    fn recycle_plan_arenas(&mut self, plan: &mut RenderPlan) {
        self.scratch.layers.append(&mut plan.layers);
        self.scratch.gradients.append(&mut plan.gradients);
        self.scratch
            .text_animations
            .append(&mut plan.text_animations);
    }

    /// Resets the plan and the per-evaluation scratch state.
    fn reset(&mut self, document: &Document, frame: Frame, plan: &mut RenderPlan) {
        let composition = document.composition();
        plan.width = composition.width();
        plan.height = composition.height();
        plan.frame = frame;
        plan.background = Some(composition.background());
        plan.items.clear();
        plan.layers.clear();
        plan.gradients.clear();
        plan.text_animations.clear();
        plan.effect_layer_items.clear();
        plan.non_finite_items = 0;

        self.stack.clear();
        self.mask_sources.clear();
        for node in document.nodes() {
            if let Some(mask) = node.mask() {
                self.mask_sources.insert(mask.source);
            }
        }
    }

    /// Drops empty layers and rebuilds the derived effect-layer flags.
    fn finish(&mut self, plan: &mut RenderPlan) {
        // A layer whose subtree draws nothing is not a layer at all.
        plan.layers.retain(|layer| !layer.is_empty());
        self.rebuild_effect_flags(plan);
    }

    /// Walks the plan once, in paint order, and flags items inside effect
    /// layers. Layers nest, so a stack keeps this O(items + layers).
    fn rebuild_effect_flags(&mut self, plan: &mut RenderPlan) {
        let count = plan.items.len();
        plan.effect_layer_items.clear();
        plan.effect_layer_items.resize(count, false);
        self.effect_stack.clear();
        let mut open_effects: usize = 0;
        let mut next = 0;
        for index in 0..count {
            while let Some(&top) = self.effect_stack.last() {
                if plan.layers[top].end <= index {
                    if !plan.layers[top].effects.is_empty() {
                        open_effects = open_effects.saturating_sub(1);
                    }
                    self.effect_stack.pop();
                } else {
                    break;
                }
            }
            while next < plan.layers.len() && plan.layers[next].start <= index {
                let layer = &plan.layers[next];
                if layer.start <= index && layer.end > index {
                    if !layer.effects.is_empty() {
                        open_effects = open_effects.saturating_add(1);
                    }
                    self.effect_stack.push(next);
                }
                next += 1;
            }
            plan.effect_layer_items[index] = open_effects > 0;
        }
    }

    /// Prepares the root cache for `revision` and returns the previous
    /// evaluation frame and time.
    fn prepare_cache(&mut self, document: &Document, revision: u64) -> Option<(Frame, f64)> {
        if self.cache_revision != Some(revision) {
            self.cache_revision = Some(revision);
            self.cache_time = None;
            self.cache_frame = None;
            self.root_cache.clear();
            self.static_states.clear();
        }
        if self.root_cache.len() < document.len() {
            self.root_cache.resize_with(document.len(), || None);
        }
        if self.static_states.len() < document.len() {
            self.static_states.resize(document.len(), 0);
        }
        self.cache_frame.zip(self.cache_time)
    }

    /// Computes, for every reachable node, whether its closure is constant
    /// over `from..=to`.
    fn compute_static(&mut self, document: &Document, from: Frame, to: Frame) {
        self.static_states.clear();
        self.static_states.resize(document.len(), 0);
        if from == to {
            self.static_states.fill(2);
            return;
        }
        for &root in document.roots() {
            self.subtree_const(document, root, from, to);
        }
    }

    /// Whether `id`'s whole read closure has no key inside `from..=to`.
    ///
    /// The closure follows children and mask sources, so a mask source that
    /// moves invalidates the subtree it is drawn into as well.
    fn subtree_const(&mut self, document: &Document, id: NodeId, from: Frame, to: Frame) -> bool {
        let index = id.raw() as usize;
        match self.static_states.get(index).copied() {
            Some(2) => return true,
            Some(1 | 3) => return false,
            _ => {}
        }
        if let Some(state) = self.static_states.get_mut(index) {
            *state = 1;
        } else {
            return false;
        }
        let Some(node) = document.node(id) else {
            if let Some(state) = self.static_states.get_mut(index) {
                *state = 3;
            }
            return false;
        };
        let mut constant = local_static(node, from, to);
        if constant {
            for &child in node.children() {
                if !self.subtree_const(document, child, from, to) {
                    constant = false;
                    break;
                }
            }
        }
        if constant {
            if let Some(mask) = node.mask() {
                constant = self.subtree_const(document, mask.source, from, to);
            }
        }
        if let Some(state) = self.static_states.get_mut(index) {
            *state = if constant { 2 } else { 3 };
        }
        constant
    }

    /// Captures the plan range a finished root subtree produced.
    fn capture_root(
        &mut self,
        plan: &RenderPlan,
        root: NodeId,
        starts: FragmentStarts,
        revision: u64,
        time: f64,
    ) {
        let index = root.raw() as usize;
        if index >= self.root_cache.len() {
            self.root_cache.resize_with(index + 1, || None);
        }
        let Some(slot) = self.root_cache.get_mut(index) else {
            return;
        };
        let entry = slot.get_or_insert_with(|| RootCacheEntry {
            revision,
            time,
            fragment: RootFragment::default(),
        });
        entry.revision = revision;
        entry.time = time;
        entry.fragment.items.clear();
        for item in &plan.items[starts.items..] {
            let mut item = *item;
            strip_shape(&mut item.shape, starts);
            entry.fragment.items.push(item);
        }
        entry.fragment.layers.clear();
        for layer in &plan.layers[starts.layers..] {
            let mut layer = layer.clone();
            layer.start = layer.start.saturating_sub(starts.items);
            layer.end = layer.end.saturating_sub(starts.items);
            if let Some(mask) = &mut layer.mask {
                mask.start = mask.start.saturating_sub(starts.items);
                mask.end = mask.end.saturating_sub(starts.items);
            }
            entry.fragment.layers.push(layer);
        }
        entry.fragment.paths.clear();
        for path in &plan.paths[starts.paths..] {
            let mut path = path.clone();
            strip_paint(&mut path.paint, starts);
            entry.fragment.paths.push(path);
        }
        entry.fragment.meshes.clear();
        entry
            .fragment
            .meshes
            .extend_from_slice(&plan.meshes[starts.meshes..]);
        entry.fragment.gradients.clear();
        entry
            .fragment
            .gradients
            .extend_from_slice(&plan.gradients[starts.gradients..]);
        entry.fragment.text_animations.clear();
        entry
            .fragment
            .text_animations
            .extend_from_slice(&plan.text_animations[starts.text_animations..]);
        entry.fragment.non_finite_items = plan
            .non_finite_items
            .saturating_sub(starts.non_finite_items);
    }

    /// Pops the traversal stack until it is empty.
    #[allow(clippy::too_many_lines)] // one traversal of every node kind
    fn run(
        &mut self,
        document: &Document,
        frame: Frame,
        time: f64,
        plan: &mut RenderPlan,
        capture: Option<(u64, f64)>,
    ) {
        while let Some(pending) = self.stack.pop() {
            let (pending_id, parent_transform, parent_opacity, mask_for) = match pending {
                Pending::Visit {
                    id,
                    parent_transform,
                    parent_opacity,
                    mask_for,
                } => (id, parent_transform, parent_opacity, mask_for),
                Pending::CloseLayer { index } => {
                    if let Some(layer) = plan.layers.get_mut(index) {
                        layer.end = plan.items.len();
                    }
                    continue;
                }
                Pending::CloseMask { index } => {
                    if let Some(mask) = plan
                        .layers
                        .get_mut(index)
                        .and_then(|layer| layer.mask.as_mut())
                    {
                        mask.end = plan.items.len();
                    }
                    continue;
                }
                Pending::CloseRoot { root, starts } => {
                    if let Some((revision, cached_time)) = capture {
                        self.capture_root(plan, root, starts, revision, cached_time);
                    }
                    continue;
                }
            };
            let Some(node) = document.node(pending_id) else {
                continue;
            };
            // A mask source is not part of the normal draw stream; it is drawn
            // only where a masked node asks for it.
            if mask_for.is_none() && self.mask_sources.contains(&node.id()) {
                continue;
            }
            // A hidden node is not drawn, and neither is anything below it:
            // visibility is inherited, like opacity but without a value.
            if !node.is_visible() {
                continue;
            }
            let props = node.transform();
            let local = Affine2::layer(
                sample_vec2_at(&props.position, time),
                sample_at(&props.rotation, time),
                sample_vec2_at(&props.scale, time),
                sample_vec2_at(&props.anchor, time),
            );
            let transform = parent_transform.then(local);
            let opacity = parent_opacity * sample_at(&props.opacity, time).clamp(0.0, 1.0);
            if !transform.is_finite() || !opacity.is_finite() {
                plan.non_finite_items = plan.non_finite_items.saturating_add(1);
                continue;
            }
            if opacity <= 0.0 {
                continue;
            }
            // The mask source's first item is where its range begins; a
            // source that draws nothing leaves an empty range.
            if let Some(layer_index) = mask_for {
                if let Some(mask) = plan
                    .layers
                    .get_mut(layer_index)
                    .and_then(|layer| layer.mask.as_mut())
                {
                    mask.start = plan.items.len();
                }
            }
            // A node with effects, a non-normal blend or a mask renders its
            // subtree into an offscreen layer.  The layer opens before the
            // node's own item and closes after the last descendant and the
            // mask source.
            if !node.effects().is_empty()
                || node.blend() != BlendMode::Normal
                || node.mask().is_some()
            {
                let index = plan.layers.len();
                let mask = node.mask().map(|spec| PlanMask {
                    start: plan.items.len(),
                    end: plan.items.len(),
                    mode: spec.mode,
                    invert: spec.invert,
                });
                plan.layers.push(PlanLayer {
                    node: node.id(),
                    start: plan.items.len(),
                    end: plan.items.len(),
                    blend: node.blend(),
                    effects: resolved_effects(node, time),
                    mask,
                });
                self.stack.push(Pending::CloseLayer { index });
                if let Some(spec) = node.mask() {
                    // A source inside the masked subtree moves with it; any
                    // other source is drawn in world space with its own
                    // transform, so its ancestors' transforms do not leak in.
                    let parent = if document.is_ancestor(node.id(), spec.source) {
                        transform
                    } else {
                        Affine2::IDENTITY
                    };
                    self.stack.push(Pending::CloseMask { index });
                    self.stack.push(Pending::Visit {
                        id: spec.source,
                        parent_transform: parent,
                        parent_opacity: 1.0,
                        mask_for: Some(index),
                    });
                }
            }
            if let NodeContent::Video(video) = node.content() {
                if video.asset.is_some()
                    && video
                        .source_frame(frame, document.composition().frame_rate())
                        .is_none()
                {
                    continue;
                }
            }
            if let Some(shape) = item_shape(node.id(), node.content(), time, plan) {
                plan.items.push(RenderItem {
                    node: node.id(),
                    transform,
                    opacity,
                    blend: node.blend(),
                    shape,
                });
            }
            self.push_children(node.children(), transform, opacity);
        }
    }

    /// Pushes nodes so that the first one is popped first.
    fn push_children(&mut self, ids: &[NodeId], parent_transform: Affine2, parent_opacity: f64) {
        self.stack
            .extend(ids.iter().rev().map(|&id| Pending::Visit {
                id,
                parent_transform,
                parent_opacity,
                mask_for: None,
            }));
    }
}

/// Appends a cached root subtree to `plan`, shifting its arena indices.
fn append_fragment(plan: &mut RenderPlan, fragment: &RootFragment, scratch: &mut FragmentScratch) {
    let bases = FragmentBases {
        items: plan.items.len(),
        paths: plan.paths.len(),
        meshes: plan.meshes.len(),
        gradients: plan.gradients.len(),
        text_animations: plan.text_animations.len(),
    };
    for item in &fragment.items {
        let mut item = *item;
        remap_shape(&mut item.shape, bases);
        plan.items.push(item);
    }
    for source in &fragment.layers {
        if source.is_empty() {
            continue;
        }
        let mut layer = match scratch.layers.pop() {
            Some(mut layer) => {
                layer.node = source.node;
                layer.blend = source.blend;
                layer.mask = source.mask;
                layer.effects.clear();
                layer.effects.extend_from_slice(&source.effects);
                layer
            }
            None => source.clone(),
        };
        layer.start = source.start + bases.items;
        layer.end = source.end + bases.items;
        if let Some(mask) = &mut layer.mask {
            mask.start += bases.items;
            mask.end += bases.items;
        }
        plan.layers.push(layer);
    }
    for path in &fragment.paths {
        let mut path = path.clone();
        remap_paint(&mut path.paint, bases);
        plan.paths.push(path);
    }
    plan.meshes.extend_from_slice(&fragment.meshes);
    for gradient in &fragment.gradients {
        match scratch.gradients.pop() {
            Some(mut slot) => {
                slot.kind = gradient.kind;
                slot.stops.clear();
                slot.stops.extend_from_slice(&gradient.stops);
                plan.gradients.push(slot);
            }
            None => plan.gradients.push(gradient.clone()),
        }
    }
    for animators in &fragment.text_animations {
        match scratch.text_animations.pop() {
            Some(mut slot) => {
                slot.clear();
                slot.extend_from_slice(animators);
                plan.text_animations.push(slot);
            }
            None => plan.text_animations.push(animators.clone()),
        }
    }
    plan.non_finite_items = plan
        .non_finite_items
        .saturating_add(fragment.non_finite_items);
}

/// Shifts the arena indices of one shape by the fragment bases.
fn remap_shape(shape: &mut ItemShape, bases: FragmentBases) {
    match shape {
        ItemShape::Rect { paint, .. } | ItemShape::Circle { paint, .. } => {
            remap_paint(paint, bases);
        }
        ItemShape::Path { index } => *index += bases.paths,
        ItemShape::Text {
            gradient,
            animations,
            ..
        } => {
            if let Some(index) = gradient {
                *index += bases.gradients;
            }
            if let Some(index) = animations {
                *index += bases.text_animations;
            }
        }
        ItemShape::Image { mesh, .. } => {
            if let Some(index) = mesh {
                *index += bases.meshes;
            }
        }
    }
}

/// Shifts the gradient indices of a resolved paint by the fragment bases.
fn remap_paint(paint: &mut ResolvedPaint, bases: FragmentBases) {
    if let Some(fill) = &mut paint.fill {
        remap_fill(fill, bases);
    }
    if let Some(stroke) = &mut paint.stroke {
        remap_fill(&mut stroke.paint, bases);
    }
}

fn remap_fill(fill: &mut ResolvedFill, bases: FragmentBases) {
    if let ResolvedFill::Gradient(index) = fill {
        *index += bases.gradients;
    }
}

/// Makes one shape's arena indices relative to its fragment.
fn strip_shape(shape: &mut ItemShape, starts: FragmentStarts) {
    match shape {
        ItemShape::Rect { paint, .. } | ItemShape::Circle { paint, .. } => {
            strip_paint(paint, starts);
        }
        ItemShape::Path { index } => *index = index.saturating_sub(starts.paths),
        ItemShape::Text {
            gradient,
            animations,
            ..
        } => {
            if let Some(index) = gradient {
                *index = index.saturating_sub(starts.gradients);
            }
            if let Some(index) = animations {
                *index = index.saturating_sub(starts.text_animations);
            }
        }
        ItemShape::Image { mesh, .. } => {
            if let Some(index) = mesh {
                *index = index.saturating_sub(starts.meshes);
            }
        }
    }
}

/// Makes one paint's gradient indices relative to its fragment.
fn strip_paint(paint: &mut ResolvedPaint, starts: FragmentStarts) {
    if let Some(fill) = &mut paint.fill {
        strip_fill(fill, starts);
    }
    if let Some(stroke) = &mut paint.stroke {
        strip_fill(&mut stroke.paint, starts);
    }
}

fn strip_fill(fill: &mut ResolvedFill, starts: FragmentStarts) {
    if let ResolvedFill::Gradient(index) = fill {
        *index = index.saturating_sub(starts.gradients);
    }
}

/// Whether every track read by `node` itself is constant over the window.
fn local_static(node: &Node, from: Frame, to: Frame) -> bool {
    let transform = node.transform();
    vec2_static(&transform.position, from, to)
        && vec2_static(&transform.scale, from, to)
        && vec2_static(&transform.anchor, from, to)
        && keys_outside_range(&transform.rotation, from, to)
        && keys_outside_range(&transform.opacity, from, to)
        && effects_static(node.effects(), from, to)
        && content_static(node.content(), from, to)
}

fn vec2_static(value: &AnimatableVec2, from: Frame, to: Frame) -> bool {
    keys_outside_range(value.axis(Axis::X), from, to)
        && keys_outside_range(value.axis(Axis::Y), from, to)
}

fn paint_static(paint: &PaintSpec, from: Frame, to: Frame) -> bool {
    keys_outside_range(&paint.fill, from, to)
        && keys_outside_range(&paint.stroke, from, to)
        && keys_outside_range(&paint.stroke_width, from, to)
        && paint
            .fill_gradient
            .as_ref()
            .is_none_or(|gradient| gradient_static(gradient, from, to))
        && paint
            .stroke_gradient
            .as_ref()
            .is_none_or(|gradient| gradient_static(gradient, from, to))
}

fn gradient_static(gradient: &Gradient, from: Frame, to: Frame) -> bool {
    gradient.stops().iter().all(|stop| {
        keys_outside_range(&stop.offset, from, to) && keys_outside_range(&stop.color, from, to)
    })
}

fn geometry_static(geometry: &GeometryChannel, from: Frame, to: Frame) -> bool {
    span_static(
        geometry.keys().first().map(|key| key.frame),
        geometry.keys().last().map(|key| key.frame),
        from,
        to,
    )
}

fn mesh_static(mesh: &Mesh, from: Frame, to: Frame) -> bool {
    span_static(
        mesh.keys().first().map(|key| key.frame),
        mesh.keys().last().map(|key| key.frame),
        from,
        to,
    )
}

/// The same rule as [`keys_outside_range`] for a manually keyed channel.
fn span_static(first: Option<Frame>, last: Option<Frame>, from: Frame, to: Frame) -> bool {
    if from == to {
        return true;
    }
    let (from, to) = if from < to { (from, to) } else { (to, from) };
    match (first, last) {
        (_, Some(last)) if last <= from => true,
        (Some(first), _) if first >= to => true,
        (None, _) => true,
        _ => false,
    }
}

fn content_static(content: &NodeContent, from: Frame, to: Frame) -> bool {
    match content {
        // Groups and audio contribute no draw items of their own; groups still
        // recurse into their children, and audio only samples its transform.
        NodeContent::Group | NodeContent::Audio(_) => true,
        NodeContent::Rect {
            width,
            height,
            radius,
            paint,
        } => {
            keys_outside_range(width, from, to)
                && keys_outside_range(height, from, to)
                && keys_outside_range(radius, from, to)
                && paint_static(paint, from, to)
        }
        NodeContent::Circle { radius, paint } => {
            keys_outside_range(radius, from, to) && paint_static(paint, from, to)
        }
        NodeContent::Path {
            geometry,
            paint,
            trim_start,
            trim_end,
            trim_offset,
            dash,
            dash_offset,
        } => {
            geometry_static(geometry, from, to)
                && paint_static(paint, from, to)
                && keys_outside_range(trim_start, from, to)
                && keys_outside_range(trim_end, from, to)
                && keys_outside_range(trim_offset, from, to)
                && keys_outside_range(dash, from, to)
                && keys_outside_range(dash_offset, from, to)
        }
        NodeContent::Text(text) => {
            keys_outside_range(&text.size, from, to)
                && keys_outside_range(&text.tracking, from, to)
                && keys_outside_range(&text.line_height, from, to)
                && keys_outside_range(&text.color, from, to)
                && text
                    .gradient
                    .as_ref()
                    .is_none_or(|gradient| gradient_static(gradient, from, to))
        }
        NodeContent::Image(image) => {
            keys_outside_range(&image.width, from, to)
                && keys_outside_range(&image.height, from, to)
                && mesh_static(&image.mesh, from, to)
        }
        // Video culling reads the composition frame directly, so it cannot be
        // reused across different frames even when its tracks are static.
        NodeContent::Video(_) => false,
    }
}

fn effects_static(effects: &[Effect], from: Frame, to: Frame) -> bool {
    effects.iter().all(|effect| match effect {
        Effect::DropShadow(effect) => {
            keys_outside_range(&effect.color, from, to)
                && vec2_static(&effect.offset, from, to)
                && keys_outside_range(&effect.blur, from, to)
                && keys_outside_range(&effect.opacity, from, to)
        }
        Effect::Blur(effect) => keys_outside_range(&effect.radius, from, to),
        Effect::Glow(effect) => {
            keys_outside_range(&effect.color, from, to)
                && keys_outside_range(&effect.radius, from, to)
                && keys_outside_range(&effect.intensity, from, to)
        }
    })
}

/// Samples a node's effect stack, in application order.
fn resolved_effects(node: &Node, time: f64) -> Vec<EffectValue> {
    node.effects()
        .iter()
        .map(|effect| match effect {
            Effect::DropShadow(effect) => EffectValue::DropShadow {
                color: sample_at(&effect.color, time),
                offset: sample_vec2_at(&effect.offset, time),
                blur: sample_at(&effect.blur, time),
                opacity: sample_at(&effect.opacity, time),
            },
            Effect::Blur(effect) => EffectValue::Blur {
                radius: sample_at(&effect.radius, time),
            },
            Effect::Glow(effect) => EffectValue::Glow {
                color: sample_at(&effect.color, time),
                radius: sample_at(&effect.radius, time),
                intensity: sample_at(&effect.intensity, time),
            },
        })
        .collect()
}

#[allow(clippy::too_many_lines)] // one arm per content kind, kept together
fn item_shape(
    node: motioner_core::NodeId,
    content: &NodeContent,
    time: f64,
    plan: &mut RenderPlan,
) -> Option<ItemShape> {
    match content {
        NodeContent::Group | NodeContent::Audio(_) => None,
        NodeContent::Rect {
            width,
            height,
            radius,
            paint,
        } => Some(ItemShape::Rect {
            width: sample_at(width, time),
            height: sample_at(height, time),
            radius: sample_at(radius, time),
            paint: resolved_paint(paint, time, plan),
        }),
        NodeContent::Circle { radius, paint } => Some(ItemShape::Circle {
            radius: sample_at(radius, time),
            paint: resolved_paint(paint, time, plan),
        }),
        NodeContent::Path {
            geometry,
            paint,
            trim_start,
            trim_end,
            trim_offset,
            dash,
            dash_offset,
        } => {
            let resolved = resolved_paint(paint, time, plan);
            let geometry = sample_geometry_at(geometry, time)?;
            let index = plan.paths.len();
            plan.paths.push(ResolvedPath {
                geometry,
                trim: TrimSpec {
                    start: sample_at(trim_start, time),
                    end: sample_at(trim_end, time),
                    offset: sample_at(trim_offset, time),
                },
                dash: DashSpec {
                    length: sample_at(dash, time),
                    offset: sample_at(dash_offset, time),
                },
                paint: resolved,
            });
            Some(ItemShape::Path { index })
        }
        NodeContent::Text(text) => {
            let (color, gradient) = match text.gradient.as_ref() {
                Some(gradient) => match resolve_gradient(gradient, time, plan) {
                    ResolvedFill::Gradient(index) => (sample_at(&text.color, time), Some(index)),
                    ResolvedFill::Solid(color) => (color, None),
                },
                None => (sample_at(&text.color, time), None),
            };
            let animations = if text.animators.is_empty() {
                None
            } else {
                let index = plan.text_animations.len();
                plan.text_animations.push(text.animators.clone());
                Some(index)
            };
            Some(ItemShape::Text {
                size: sample_at(&text.size, time),
                weight: text.weight,
                tracking: sample_at(&text.tracking, time),
                line_height: sample_at(&text.line_height, time),
                color,
                gradient,
                align: text.align,
                multiline: text.text.contains('\n'),
                animations,
            })
        }
        NodeContent::Image(image) => {
            let layout = motioner_core::ImageLayout::compute(
                sample_at(&image.width, time),
                sample_at(&image.height, time),
                image.fit,
                image.asset.map(|asset| (asset.width, asset.height)),
            );
            let mesh = if image.mesh.is_default_quad() {
                None
            } else {
                image.mesh.sample(time, ease).map(|mut mesh| {
                    for point in &mut mesh.points {
                        point.x *= layout.width;
                        point.y *= layout.height;
                    }
                    plan.push_mesh(mesh)
                })
            };
            Some(ItemShape::Image {
                key: image.asset.map(|asset| asset.key),
                width: layout.width,
                height: layout.height,
                uv: layout.uv,
                mesh,
            })
        }
        NodeContent::Video(video) => {
            let layout = motioner_core::ImageLayout::compute(
                sample_at(&video.width, time),
                sample_at(&video.height, time),
                video.fit,
                video.asset.map(|asset| (asset.width, asset.height)),
            );
            let mesh = if video.mesh.is_default_quad() {
                None
            } else {
                video.mesh.sample(time, ease).map(|mut mesh| {
                    for point in &mut mesh.points {
                        point.x *= layout.width;
                        point.y *= layout.height;
                    }
                    plan.push_mesh(mesh)
                })
            };
            Some(ItemShape::Image {
                key: video.texture_key(node),
                width: layout.width,
                height: layout.height,
                uv: layout.uv,
                mesh,
            })
        }
    }
}

/// Samples the fill and stroke channels into the resolved paint of one frame.
///
/// A stroke is drawn only when its colour is present and its width is above
/// zero, the same rule the constructors use for `stroke_width = 0`.
fn resolved_paint(spec: &PaintSpec, time: f64, plan: &mut RenderPlan) -> ResolvedPaint {
    let width = sample_at(&spec.stroke_width, time);
    let fill = match spec.fill_gradient.as_ref() {
        Some(gradient) => Some(resolve_gradient(gradient, time, plan)),
        None => sample_at(&spec.fill, time).map(ResolvedFill::Solid),
    };
    let stroke = if width > 0.0 {
        match spec.stroke_gradient.as_ref() {
            Some(gradient) => Some(ResolvedStroke {
                paint: resolve_gradient(gradient, time, plan),
                width,
                non_scaling: spec.non_scaling_stroke,
            }),
            None => sample_at(&spec.stroke, time).map(|color| ResolvedStroke {
                paint: ResolvedFill::Solid(color),
                width,
                non_scaling: spec.non_scaling_stroke,
            }),
        }
    } else {
        None
    };
    ResolvedPaint { fill, stroke }
}

/// Samples a gradient's stops and pushes it into the plan arena.
///
/// Stops are clamped to `0.0..=1.0` and sorted, so a keyed offset that crosses
/// a neighbour still produces a valid ramp. A gradient that degenerates to
/// fewer than two stops falls back to its first colour instead of drawing a
/// flat ramp.
fn resolve_gradient(gradient: &Gradient, time: f64, plan: &mut RenderPlan) -> ResolvedFill {
    let mut stops: Vec<ResolvedStop> = gradient
        .stops()
        .iter()
        .map(|stop| ResolvedStop {
            offset: sample_at(&stop.offset, time).clamp(0.0, 1.0),
            color: sample_at(&stop.color, time),
        })
        .collect();
    if stops.len() < 2 {
        return ResolvedFill::Solid(gradient.fallback_color());
    }
    stops.sort_by(|a, b| a.offset.total_cmp(&b.offset));
    let index = plan.gradients.len();
    plan.gradients.push(ResolvedGradient {
        kind: gradient.kind(),
        stops,
    });
    ResolvedFill::Gradient(index)
}

/// Converts an integer frame to the fractional time domain exactly.
fn frame_time(frame: Frame) -> f64 {
    #[allow(clippy::cast_precision_loss)] // frames are bounded by 2^31
    {
        frame as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use motioner_core::{
        Color, Composition, Ease, FrameRate, Paint, Property, PropertyValue, TextAlign,
        TextContent, Vec2, MAX_ABS_FRAME,
    };

    fn document() -> Document {
        let fps = FrameRate::new(60, 1).unwrap();
        Document::new(Composition::new("Test", 100, 100, fps, 60, Color::BLACK).unwrap())
    }

    fn square(doc: &mut Document) -> NodeId {
        doc.add_node(NodeContent::rect(
            10.0,
            10.0,
            0.0,
            Paint {
                fill: Some(Color::WHITE),
                stroke: None,
            },
        ))
        .unwrap()
    }

    fn evaluate(doc: &Document, frame: Frame) -> RenderPlan {
        let mut plan = RenderPlan::new();
        Evaluator::new().evaluate(doc, frame, &mut plan);
        plan
    }

    /// Wraps `leaf` in `levels` groups, each scaled by `MAX_SCALE`, and
    /// returns the outermost group.
    fn nested_scaled_groups(doc: &mut Document, levels: usize, leaf: NodeId) -> NodeId {
        let mut current = doc.add_node(NodeContent::Group).unwrap();
        doc.set_property(
            current,
            Property::Scale,
            PropertyValue::Scalar(motioner_core::bounds::MAX_SCALE),
        )
        .unwrap();
        doc.reparent(leaf, current).unwrap();
        for _ in 1..levels {
            let outer = doc.add_node(NodeContent::Group).unwrap();
            doc.set_property(
                outer,
                Property::Scale,
                PropertyValue::Scalar(motioner_core::bounds::MAX_SCALE),
            )
            .unwrap();
            doc.reparent(current, outer).unwrap();
            current = outer;
        }
        current
    }

    #[test]
    fn paints_parents_before_children_in_order() {
        let mut doc = document();
        let group = doc.add_node(NodeContent::Group).unwrap();
        let a = square(&mut doc);
        let b = square(&mut doc);
        let c = square(&mut doc);
        doc.reparent(a, group).unwrap();
        doc.reparent(b, group).unwrap();
        let order: Vec<_> = evaluate(&doc, 0).items().iter().map(|i| i.node).collect();
        assert_eq!(order, vec![a, b, c]);
    }

    #[test]
    fn group_transform_and_opacity_propagate() {
        let mut doc = document();
        let group = doc.add_node(NodeContent::Group).unwrap();
        let leaf = square(&mut doc);
        doc.reparent(leaf, group).unwrap();
        doc.set_property(
            group,
            Property::Position,
            PropertyValue::Vec2(Vec2::new(50.0, 20.0)),
        )
        .unwrap();
        doc.set_property(group, Property::Opacity, PropertyValue::Scalar(0.5))
            .unwrap();
        doc.set_property(
            leaf,
            Property::Position,
            PropertyValue::Vec2(Vec2::new(5.0, 5.0)),
        )
        .unwrap();
        doc.set_property(leaf, Property::Opacity, PropertyValue::Scalar(0.5))
            .unwrap();
        let plan = evaluate(&doc, 0);
        let item = plan.items()[0];
        assert_eq!(item.transform.apply(Vec2::ZERO), Vec2::new(55.0, 25.0));
        assert!((item.opacity - 0.25).abs() < 1e-12);
    }

    #[test]
    fn a_playback_frame_allocates_nothing_after_warmup() {
        let mut doc = document();
        let id = square(&mut doc);
        doc.add_keyframe(
            id,
            Property::Position,
            0,
            PropertyValue::Vec2(Vec2::new(0.0, 0.0)),
            Ease::Linear,
        )
        .unwrap();
        doc.add_keyframe(
            id,
            Property::Position,
            60,
            PropertyValue::Vec2(Vec2::new(50.0, 0.0)),
            Ease::Linear,
        )
        .unwrap();
        let mut evaluator = Evaluator::new();
        let mut plan = RenderPlan::new();
        for frame in 0..10 {
            evaluator.evaluate(&doc, frame, &mut plan);
        }
        ferrous_core::alloc_stats::reset();
        let scope = ferrous_core::alloc_stats::ThreadScope::new();
        evaluator.evaluate(&doc, 11, &mut plan);
        let delta = scope.finish();
        assert_eq!(delta.allocating_calls(), 0, "{delta:?}");
    }

    #[test]
    fn invisible_subtrees_are_skipped() {
        let mut doc = document();
        let id = square(&mut doc);
        doc.add_keyframe(
            id,
            Property::Opacity,
            0,
            PropertyValue::Scalar(0.0),
            Ease::Linear,
        )
        .unwrap();
        doc.add_keyframe(
            id,
            Property::Opacity,
            10,
            PropertyValue::Scalar(1.0),
            Ease::Linear,
        )
        .unwrap();
        assert!(evaluate(&doc, 0).items().is_empty());
        assert_eq!(evaluate(&doc, 5).items().len(), 1);
    }

    #[test]
    fn hidden_nodes_and_their_descendants_are_skipped() {
        let mut doc = document();
        let group = doc.add_node(NodeContent::Group).unwrap();
        let leaf = square(&mut doc);
        doc.reparent(leaf, group).unwrap();
        assert_eq!(evaluate(&doc, 0).items().len(), 1, "visible by default");
        doc.set_visible(group, false).unwrap();
        assert!(
            evaluate(&doc, 0).items().is_empty(),
            "a hidden group hides its child"
        );
        doc.set_visible(group, true).unwrap();
        doc.set_visible(leaf, false).unwrap();
        assert!(evaluate(&doc, 0).items().is_empty());
        doc.set_visible(leaf, true).unwrap();
        assert_eq!(evaluate(&doc, 0).items().len(), 1);
    }

    #[test]
    fn evaluation_is_repeatable() {
        let mut doc = document();
        let id = square(&mut doc);
        doc.add_keyframe(
            id,
            Property::Rotation,
            0,
            PropertyValue::Scalar(0.0),
            Ease::Linear,
        )
        .unwrap();
        doc.add_keyframe(
            id,
            Property::Rotation,
            60,
            PropertyValue::Scalar(360.0),
            Ease::BackOut,
        )
        .unwrap();
        let mut evaluator = Evaluator::new();
        let mut first = RenderPlan::new();
        let mut second = RenderPlan::new();
        evaluator.evaluate(&doc, 17, &mut first);
        evaluator.evaluate(&doc, 3, &mut second);
        evaluator.evaluate(&doc, 17, &mut second);
        assert_eq!(first, second);
    }

    #[test]
    fn keyframes_at_frame_limits_sample_finite() {
        let mut doc = document();
        let id = square(&mut doc);
        doc.add_keyframe(
            id,
            Property::Position,
            -MAX_ABS_FRAME,
            PropertyValue::Vec2(Vec2::new(-motioner_core::bounds::MAX_POSITION, 0.0)),
            Ease::Linear,
        )
        .unwrap();
        doc.add_keyframe(
            id,
            Property::Position,
            MAX_ABS_FRAME,
            PropertyValue::Vec2(Vec2::new(motioner_core::bounds::MAX_POSITION, 0.0)),
            Ease::Linear,
        )
        .unwrap();
        let plan = evaluate(&doc, 0);
        assert_eq!(plan.non_finite_items(), 0);
        assert_eq!(plan.items().len(), 1);
        let item = plan.items()[0];
        assert!(item.transform.is_finite());
        assert!(item.opacity.is_finite());
    }

    #[test]
    fn sixty_four_scale_levels_stay_finite() {
        let mut doc = document();
        let leaf = square(&mut doc);
        nested_scaled_groups(&mut doc, 64, leaf);
        let plan = evaluate(&doc, 0);
        // MAX_SCALE^64 is about 1e256, below f64::MAX (~1.8e308).
        assert_eq!(plan.non_finite_items(), 0);
        assert_eq!(plan.items().len(), 1);
        assert!(plan.items()[0].transform.is_finite());
    }

    #[test]
    fn deep_scale_nesting_skips_non_finite_items() {
        let mut doc = document();
        let leaf = square(&mut doc);
        // MAX_SCALE^80 is about 1e320, above f64::MAX, so the chain
        // overflows around level 78; 80 leaves a safety margin.
        nested_scaled_groups(&mut doc, 80, leaf);
        let plan = evaluate(&doc, 0);
        assert!(plan.non_finite_items() > 0);
        assert!(plan.items().is_empty());
        assert!(plan
            .items()
            .iter()
            .all(|item| item.transform.is_finite() && item.opacity.is_finite()));
    }

    #[test]
    fn paths_are_sampled_into_the_plan_arena() {
        use motioner_core::{
            ContentField, ContentValue, LineCap, LineJoin, PathGeometry, PathPoint, Stroke,
        };
        let mut doc = document();
        let geometry = PathGeometry::new(
            vec![
                PathPoint::corner(Vec2::new(0.0, 0.0)),
                PathPoint::corner(Vec2::new(50.0, 0.0)),
                PathPoint::corner(Vec2::new(50.0, 50.0)),
            ],
            false,
            LineCap::Round,
            LineJoin::Round,
        )
        .unwrap();
        let id = doc
            .add_node(NodeContent::path(
                geometry,
                Paint {
                    fill: Some(Color::WHITE),
                    stroke: Some(Stroke {
                        color: Color::WHITE,
                        width: 4.0,
                    }),
                },
            ))
            .unwrap();
        for (frame, value) in [(0, 0.0), (10, 1.0)] {
            doc.add_content_keyframe(
                id,
                ContentField::TrimEnd,
                frame,
                ContentValue::Number(value),
                Ease::Linear,
            )
            .unwrap();
        }
        doc.add_content_keyframe(
            id,
            ContentField::Dash,
            0,
            ContentValue::Number(8.0),
            Ease::Linear,
        )
        .unwrap();

        let plan = evaluate(&doc, 5);
        assert_eq!(plan.items().len(), 1);
        assert_eq!(plan.paths().len(), 1);
        let ItemShape::Path { index } = plan.items()[0].shape else {
            panic!("a path item");
        };
        let path = plan.path(index).unwrap();
        assert!((path.trim.start - 0.0).abs() < 1e-9);
        assert!((path.trim.end - 0.5).abs() < 1e-9);
        assert!((path.dash.length - 8.0).abs() < 1e-9);
        assert_eq!(path.geometry.cap(), LineCap::Round);
        assert_eq!(path.paint.stroke.map(|stroke| stroke.width), Some(4.0));
    }

    #[test]
    fn gradients_are_sampled_sorted_and_clamped_into_the_plan() {
        use motioner_core::{ContentField, ContentValue, Ease, Gradient};
        let mut doc = document();
        let gradient = Gradient::linear(
            Vec2::ZERO,
            Vec2::new(100.0, 0.0),
            [(0.0, Color::BLACK), (1.0, Color::WHITE)],
        )
        .unwrap();
        let id = doc
            .add_node(NodeContent::rect(
                100.0,
                100.0,
                0.0,
                Paint {
                    fill: Some(Color::WHITE),
                    stroke: None,
                },
            ))
            .unwrap();
        doc.set_content(
            id,
            ContentField::Fill,
            ContentValue::Gradient(Some(gradient)),
        )
        .unwrap();
        // Keying a stop offset past its neighbour still samples a valid ramp.
        doc.add_content_keyframe(
            id,
            ContentField::FillStopOffset(0),
            0,
            ContentValue::Number(0.25),
            Ease::Linear,
        )
        .unwrap();
        doc.add_content_keyframe(
            id,
            ContentField::FillStopOffset(1),
            0,
            ContentValue::Number(0.75),
            Ease::Linear,
        )
        .unwrap();
        doc.add_content_keyframe(
            id,
            ContentField::FillStopColor(1),
            0,
            ContentValue::Color(Some(Color::rgb(255, 0, 0))),
            Ease::Linear,
        )
        .unwrap();

        let plan = evaluate(&doc, 0);
        assert_eq!(plan.gradients().len(), 1);
        let ItemShape::Rect { paint, .. } = plan.items()[0].shape else {
            panic!("a rect item");
        };
        let Some(ResolvedFill::Gradient(index)) = paint.fill else {
            panic!("a gradient fill");
        };
        let gradient = plan.gradient(index).unwrap();
        assert_eq!(gradient.stops.len(), 2);
        assert!((gradient.stops[0].offset - 0.25).abs() < 1e-9);
        assert!((gradient.stops[1].offset - 0.75).abs() < 1e-9);
        assert_eq!(gradient.stops[1].color, Color::rgb(255, 0, 0));
    }

    #[test]
    fn text_gradients_use_the_same_arena() {
        use motioner_core::{ContentField, ContentValue, Gradient};
        let mut doc = document();
        let id = doc
            .add_node(NodeContent::Text(TextContent::new(
                "Hi",
                "Inter",
                40.0,
                Color::WHITE,
                TextAlign::Left,
            )))
            .unwrap();
        let gradient = Gradient::radial(
            Vec2::new(20.0, 0.0),
            40.0,
            [(0.0, Color::WHITE), (1.0, Color::BLACK)],
        )
        .unwrap();
        doc.set_content(
            id,
            ContentField::Color,
            ContentValue::Gradient(Some(gradient)),
        )
        .unwrap();
        let plan = evaluate(&doc, 0);
        assert_eq!(plan.gradients().len(), 1);
        let ItemShape::Text { gradient, .. } = plan.items()[0].shape else {
            panic!("a text item");
        };
        assert_eq!(gradient, Some(0));
    }

    #[test]
    fn an_effect_wraps_the_whole_subtree() {
        let mut doc = document();
        let group = doc.add_node(NodeContent::Group).unwrap();
        let first = square(&mut doc);
        let second = square(&mut doc);
        doc.reparent(first, group).unwrap();
        doc.reparent(second, group).unwrap();
        doc.add_effect(group, motioner_core::EffectKind::Blur)
            .unwrap();

        let plan = evaluate(&doc, 0);
        assert_eq!(plan.items().len(), 2);
        assert_eq!(plan.layers().len(), 1);
        let layer = &plan.layers()[0];
        assert_eq!(layer.node, group);
        assert_eq!((layer.start, layer.end), (0, 2));
        assert_eq!(layer.effects.len(), 1);
        assert!(matches!(layer.effects[0], EffectValue::Blur { .. }));
        assert_eq!(plan.first_effect_node(), Some(group));
    }

    #[test]
    fn nested_layers_close_in_order() {
        let mut doc = document();
        let outer = doc.add_node(NodeContent::Group).unwrap();
        let inner = doc.add_node(NodeContent::Group).unwrap();
        let leaf = square(&mut doc);
        doc.reparent(inner, outer).unwrap();
        doc.reparent(leaf, inner).unwrap();
        doc.set_blend(outer, BlendMode::Multiply).unwrap();
        doc.add_effect(inner, motioner_core::EffectKind::Glow)
            .unwrap();

        let plan = evaluate(&doc, 0);
        assert_eq!(plan.items().len(), 1);
        assert_eq!(plan.layers().len(), 2);
        // Opening order: outer first, then inner; both wrap the leaf.
        assert_eq!(plan.layers()[0].node, outer);
        assert_eq!((plan.layers()[0].start, plan.layers()[0].end), (0, 1));
        assert_eq!(plan.layers()[1].node, inner);
        assert_eq!((plan.layers()[1].start, plan.layers()[1].end), (0, 1));
        assert!(plan.layers()[0].effects.is_empty());
        assert!(!plan.layers()[1].effects.is_empty());
    }

    #[test]
    fn empty_effect_layers_are_dropped() {
        let mut doc = document();
        let group = doc.add_node(NodeContent::Group).unwrap();
        doc.add_effect(group, motioner_core::EffectKind::DropShadow)
            .unwrap();
        let plan = evaluate(&doc, 0);
        assert!(plan.items().is_empty());
        assert!(plan.layers().is_empty());
        assert_eq!(plan.first_effect_node(), None);
    }

    #[test]
    fn mask_items_follow_the_content_and_are_not_drawn_as_content() {
        let mut doc = document();
        let card = square(&mut doc);
        let cutout = square(&mut doc);
        doc.set_mask(card, cutout, motioner_core::MaskMode::Alpha, false)
            .unwrap();

        let plan = evaluate(&doc, 0);
        // Only the masked node's own content is a normal item.
        assert_eq!(plan.items().len(), 2);
        assert_eq!(plan.items()[0].node, card);
        assert_eq!(plan.items()[1].node, cutout);
        assert_eq!(plan.layers().len(), 1);
        let layer = &plan.layers()[0];
        assert_eq!(layer.node, card);
        assert_eq!((layer.start, layer.end), (0, 2));
        assert_eq!(layer.content_end(), 1);
        let mask = layer.mask.expect("a mask");
        assert_eq!((mask.start, mask.end), (1, 2));
        assert_eq!(mask.mode, motioner_core::MaskMode::Alpha);
        assert!(!mask.invert);
        assert_eq!(plan.first_mask_node(), Some(card));
        assert!(doc.is_mask_source(cutout));
    }

    #[test]
    fn an_unused_mask_source_is_not_drawn() {
        let mut doc = document();
        let card = square(&mut doc);
        let cutout = square(&mut doc);
        doc.set_mask(card, cutout, motioner_core::MaskMode::Luma, true)
            .unwrap();
        let plan = evaluate(&doc, 0);
        let drawn: Vec<_> = plan.items().iter().map(|item| item.node).collect();
        assert!(drawn.contains(&card));
        assert!(
            drawn.contains(&cutout),
            "the source is drawn inside the mask"
        );
        // Removing the mask makes the source a normal node again.
        doc.clear_mask(card).unwrap();
        let plan = evaluate(&doc, 0);
        assert_eq!(plan.items().len(), 2);
        assert!(plan.layers().is_empty());
        assert_eq!(plan.first_mask_node(), None);
    }

    #[test]
    fn a_mask_source_child_moves_with_the_masked_node() {
        let mut doc = document();
        let card = doc.add_node(NodeContent::Group).unwrap();
        let body = square(&mut doc);
        doc.reparent(body, card).unwrap();
        let cutout = square(&mut doc);
        doc.reparent(cutout, card).unwrap();
        doc.set_property(
            card,
            Property::Position,
            PropertyValue::Vec2(Vec2::new(50.0, 20.0)),
        )
        .unwrap();
        doc.set_property(
            cutout,
            Property::Position,
            PropertyValue::Vec2(Vec2::new(5.0, 5.0)),
        )
        .unwrap();
        doc.set_mask(card, cutout, motioner_core::MaskMode::Alpha, false)
            .unwrap();
        let plan = evaluate(&doc, 0);
        let source = plan
            .items()
            .iter()
            .find(|item| item.node == cutout)
            .expect("the source item");
        assert_eq!(
            source.transform.apply(Vec2::ZERO),
            Vec2::new(55.0, 25.0),
            "a source child inherits the masked node's transform"
        );
    }

    #[test]
    fn a_mask_source_outside_the_subtree_uses_its_own_transform_only() {
        let mut doc = document();
        let group = doc.add_node(NodeContent::Group).unwrap();
        doc.set_property(
            group,
            Property::Position,
            PropertyValue::Vec2(Vec2::new(30.0, 0.0)),
        )
        .unwrap();
        let card = square(&mut doc);
        doc.reparent(card, group).unwrap();
        let cutout = square(&mut doc);
        doc.reparent(cutout, group).unwrap();
        doc.set_property(
            cutout,
            Property::Position,
            PropertyValue::Vec2(Vec2::new(7.0, 3.0)),
        )
        .unwrap();
        doc.set_mask(card, cutout, motioner_core::MaskMode::Alpha, false)
            .unwrap();
        let plan = evaluate(&doc, 0);
        let source = plan
            .items()
            .iter()
            .find(|item| item.node == cutout)
            .expect("the source item");
        assert_eq!(
            source.transform.apply(Vec2::ZERO),
            Vec2::new(7.0, 3.0),
            "a source outside the subtree is evaluated in world space"
        );
    }

    #[test]
    fn a_nested_mask_source_with_effects_opens_its_own_layer() {
        let mut doc = document();
        let card = square(&mut doc);
        let cutout = square(&mut doc);
        doc.add_effect(cutout, motioner_core::EffectKind::Blur)
            .unwrap();
        doc.set_mask(card, cutout, motioner_core::MaskMode::Alpha, false)
            .unwrap();
        let plan = evaluate(&doc, 0);
        assert_eq!(plan.layers().len(), 2);
        let outer = &plan.layers()[0];
        let inner = &plan.layers()[1];
        assert_eq!(outer.node, card);
        assert_eq!(inner.node, cutout);
        assert_eq!((inner.start, inner.end), (1, 2));
        assert_eq!(outer.mask.expect("outer mask").start, 1);
    }

    #[test]
    fn effect_layer_flags_include_nested_layers_and_mask_items() {
        let mut doc = document();
        let outer = doc.add_node(NodeContent::Group).unwrap();
        let inner = doc.add_node(NodeContent::Group).unwrap();
        doc.reparent(inner, outer).unwrap();
        let card = square(&mut doc);
        doc.reparent(card, inner).unwrap();
        let cutout = square(&mut doc);
        doc.reparent(cutout, inner).unwrap();
        doc.set_mask(card, cutout, motioner_core::MaskMode::Alpha, false)
            .unwrap();
        doc.add_effect(outer, motioner_core::EffectKind::Blur)
            .unwrap();
        let plain = square(&mut doc);

        let evaluated = evaluate(&doc, 0);
        let order: Vec<_> = evaluated.items().iter().map(|item| item.node).collect();
        assert_eq!(order, vec![card, cutout, plain]);
        assert!(evaluated.item_in_effect_layer(0), "the masked content");
        assert!(evaluated.item_in_effect_layer(1), "the mask source item");
        assert!(
            !evaluated.item_in_effect_layer(2),
            "a sibling outside the layer"
        );
        assert!(!evaluated.item_in_effect_layer(3), "out of range");
    }

    #[test]
    fn a_mask_only_layer_does_not_flag_its_items() {
        let mut doc = document();
        let card = square(&mut doc);
        let cutout = square(&mut doc);
        doc.set_mask(card, cutout, motioner_core::MaskMode::Alpha, false)
            .unwrap();
        let plan = evaluate(&doc, 0);
        assert_eq!(plan.layers().len(), 1);
        assert!(plan.layers()[0].effects.is_empty());
        for index in 0..plan.items().len() {
            assert!(!plan.item_in_effect_layer(index));
        }
    }

    #[test]
    fn effect_layer_flags_are_repeatable() {
        let mut doc = document();
        let group = doc.add_node(NodeContent::Group).unwrap();
        let leaf = square(&mut doc);
        doc.reparent(leaf, group).unwrap();
        doc.add_effect(group, motioner_core::EffectKind::Glow)
            .unwrap();
        let first = evaluate(&doc, 7);
        let second = evaluate(&doc, 7);
        assert_eq!(first, second);
        for index in 0..first.items().len() {
            assert_eq!(
                first.item_in_effect_layer(index),
                second.item_in_effect_layer(index)
            );
        }
    }

    fn cached_document() -> Document {
        use motioner_core::{
            CharOrder, CharProperty, CharValue, ContentField, ContentValue, EffectKind, Gradient,
            LineCap, LineJoin, MaskMode, PathGeometry, PathPoint, Stroke, TextAnimator,
        };

        let mut doc = document();
        let animated = square(&mut doc);
        for (frame, position) in [(0, Vec2::new(0.0, 0.0)), (60, Vec2::new(50.0, 0.0))] {
            doc.add_keyframe(
                animated,
                Property::Position,
                frame,
                PropertyValue::Vec2(position),
                Ease::Linear,
            )
            .unwrap();
        }

        let group = doc.add_node(NodeContent::Group).unwrap();
        doc.add_effect(group, EffectKind::Glow).unwrap();

        let card = square(&mut doc);
        doc.reparent(card, group).unwrap();
        let cutout = square(&mut doc);
        doc.reparent(cutout, group).unwrap();
        doc.set_mask(card, cutout, MaskMode::Alpha, false).unwrap();

        let gradient = Gradient::linear(
            Vec2::ZERO,
            Vec2::new(100.0, 0.0),
            [(0.0, Color::BLACK), (1.0, Color::WHITE)],
        )
        .unwrap();
        let rect = doc
            .add_node(NodeContent::rect(
                100.0,
                100.0,
                0.0,
                Paint {
                    fill: Some(Color::WHITE),
                    stroke: None,
                },
            ))
            .unwrap();
        doc.set_content(
            rect,
            ContentField::Fill,
            ContentValue::Gradient(Some(gradient)),
        )
        .unwrap();
        doc.reparent(rect, group).unwrap();

        let geometry = PathGeometry::new(
            vec![
                PathPoint::corner(Vec2::new(0.0, 0.0)),
                PathPoint::corner(Vec2::new(40.0, 0.0)),
                PathPoint::corner(Vec2::new(40.0, 40.0)),
            ],
            false,
            LineCap::Round,
            LineJoin::Round,
        )
        .unwrap();
        let path = doc
            .add_node(NodeContent::path(
                geometry,
                Paint {
                    fill: Some(Color::WHITE),
                    stroke: Some(Stroke {
                        color: Color::WHITE,
                        width: 3.0,
                    }),
                },
            ))
            .unwrap();
        doc.reparent(path, group).unwrap();

        let text = doc
            .add_node(NodeContent::Text(TextContent::new(
                "Cache",
                "Inter",
                20.0,
                Color::WHITE,
                TextAlign::Left,
            )))
            .unwrap();
        let animator = TextAnimator::new(
            CharProperty::Opacity,
            CharValue::Number(0.0),
            CharValue::Number(1.0),
            0,
            1.0,
            10.0,
            Ease::Linear,
            CharOrder::Forward,
        )
        .unwrap();
        doc.add_text_animator(text, animator).unwrap();
        doc.reparent(text, group).unwrap();
        doc
    }

    #[test]
    fn cached_evaluation_matches_uncached_over_a_run() {
        let doc = cached_document();
        let mut plain = Evaluator::new();
        let mut cached = Evaluator::new();
        let mut expected = RenderPlan::new();
        let mut actual = RenderPlan::new();
        for frame in 0..24 {
            plain.evaluate(&doc, frame, &mut expected);
            cached.evaluate_cached(&doc, 11, frame, &mut actual);
            assert_eq!(expected, actual, "frame {frame}");
        }
        assert!(
            cached.cache_hits > 0,
            "the static root should have been reused"
        );
        assert!(
            cached.cache_misses > 0,
            "the animated root must be re-evaluated"
        );
    }

    #[test]
    fn cached_evaluation_is_allocation_free_after_warmup() {
        let mut doc = document();
        square(&mut doc);
        let animated = square(&mut doc);
        doc.add_keyframe(
            animated,
            Property::Position,
            0,
            PropertyValue::Vec2(Vec2::new(0.0, 0.0)),
            Ease::Linear,
        )
        .unwrap();
        doc.add_keyframe(
            animated,
            Property::Position,
            60,
            PropertyValue::Vec2(Vec2::new(50.0, 0.0)),
            Ease::Linear,
        )
        .unwrap();
        let mut evaluator = Evaluator::new();
        let mut plan = RenderPlan::new();
        for frame in 0..10 {
            evaluator.evaluate_cached(&doc, 5, frame, &mut plan);
        }
        ferrous_core::alloc_stats::reset();
        let scope = ferrous_core::alloc_stats::ThreadScope::new();
        evaluator.evaluate_cached(&doc, 5, 11, &mut plan);
        let delta = scope.finish();
        assert_eq!(delta.allocating_calls(), 0, "{delta:?}");
    }

    #[test]
    fn a_cached_static_subtree_reuses_arena_allocations() {
        use motioner_core::{
            CharOrder, CharProperty, CharValue, ContentField, ContentValue, EffectKind, Gradient,
            TextAnimator,
        };

        let mut doc = document();
        let group = doc.add_node(NodeContent::Group).unwrap();
        doc.add_effect(group, EffectKind::Blur).unwrap();
        let gradient = Gradient::linear(
            Vec2::ZERO,
            Vec2::new(50.0, 0.0),
            [(0.0, Color::BLACK), (1.0, Color::WHITE)],
        )
        .unwrap();
        let rect = doc
            .add_node(NodeContent::rect(
                50.0,
                50.0,
                0.0,
                Paint {
                    fill: Some(Color::WHITE),
                    stroke: None,
                },
            ))
            .unwrap();
        doc.set_content(
            rect,
            ContentField::Fill,
            ContentValue::Gradient(Some(gradient)),
        )
        .unwrap();
        doc.reparent(rect, group).unwrap();
        let text = doc
            .add_node(NodeContent::Text(TextContent::new(
                "Arena",
                "Inter",
                20.0,
                Color::WHITE,
                TextAlign::Left,
            )))
            .unwrap();
        let animator = TextAnimator::new(
            CharProperty::Opacity,
            CharValue::Number(0.0),
            CharValue::Number(1.0),
            0,
            1.0,
            10.0,
            Ease::Linear,
            CharOrder::Forward,
        )
        .unwrap();
        doc.add_text_animator(text, animator).unwrap();
        doc.reparent(text, group).unwrap();

        let mut evaluator = Evaluator::new();
        let mut plan = RenderPlan::new();
        for frame in 0..10 {
            evaluator.evaluate_cached(&doc, 9, frame, &mut plan);
        }
        ferrous_core::alloc_stats::reset();
        let scope = ferrous_core::alloc_stats::ThreadScope::new();
        evaluator.evaluate_cached(&doc, 9, 11, &mut plan);
        let delta = scope.finish();
        assert_eq!(delta.allocating_calls(), 0, "{delta:?}");
    }

    #[test]
    fn a_revision_change_invalidates_the_cache() {
        let mut doc = document();
        let id = square(&mut doc);
        let mut evaluator = Evaluator::new();
        let mut plan = RenderPlan::new();
        evaluator.evaluate_cached(&doc, 1, 0, &mut plan);
        let before = plan.items()[0].transform;
        doc.set_property(
            id,
            Property::Position,
            PropertyValue::Vec2(Vec2::new(12.0, 0.0)),
        )
        .unwrap();
        evaluator.evaluate_cached(&doc, 2, 1, &mut plan);
        assert_ne!(plan.items()[0].transform, before);
        assert_eq!(
            plan.items()[0].transform.apply(Vec2::ZERO),
            Vec2::new(12.0, 0.0)
        );
        assert_eq!(plan, evaluate(&doc, 1));
    }

    #[test]
    fn a_key_inside_the_window_invalidates_the_subtree() {
        let mut doc = document();
        let id = square(&mut doc);
        doc.add_keyframe(
            id,
            Property::Position,
            0,
            PropertyValue::Vec2(Vec2::new(0.0, 0.0)),
            Ease::Linear,
        )
        .unwrap();
        doc.add_keyframe(
            id,
            Property::Position,
            10,
            PropertyValue::Vec2(Vec2::new(50.0, 0.0)),
            Ease::Linear,
        )
        .unwrap();
        let mut evaluator = Evaluator::new();
        let mut plan = RenderPlan::new();
        evaluator.evaluate_cached(&doc, 3, 4, &mut plan);
        let x4 = plan.items()[0].transform.apply(Vec2::ZERO).x;
        evaluator.evaluate_cached(&doc, 3, 5, &mut plan);
        let x5 = plan.items()[0].transform.apply(Vec2::ZERO).x;
        assert!(x5 > x4, "the animated node must not be frozen");
        assert_eq!(plan, evaluate(&doc, 5));
    }
}

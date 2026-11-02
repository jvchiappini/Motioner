//! The render plan: a flat, ordered list of resolved draw items.

use motioner_core::{
    BlendMode, Color, DashSpec, GradientKind, NodeId, PathGeometry, TextAlign, TrimSpec, Vec2,
};

use crate::affine::Affine2;

/// One paint source resolved for a frame (D-0030).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ResolvedFill {
    /// A straight sRGB colour.
    Solid(Color),
    /// Index into [`RenderPlan::gradients`].
    Gradient(usize),
}

/// A stroke: a paint source and its width in local pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedStroke {
    /// Stroke colour or gradient.
    pub paint: ResolvedFill,
    /// Width in local pixels.
    pub width: f64,
}

/// The fill and stroke of one item at one frame.
///
/// Gradients are arena indices, so render items stay `Copy` even though a
/// gradient owns a list of stops.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ResolvedPaint {
    /// Interior paint, if filled.
    pub fill: Option<ResolvedFill>,
    /// Outline, if stroked.
    pub stroke: Option<ResolvedStroke>,
}

/// One stop sampled at the frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResolvedStop {
    /// Position along the ramp, clamped to `0.0..=1.0`.
    pub offset: f64,
    /// Colour at that position.
    pub color: Color,
}

/// A gradient sampled at the frame, with stops sorted by offset.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedGradient {
    /// Linear or radial geometry in the item's local space.
    pub kind: GradientKind,
    /// Stops in increasing offset order.
    pub stops: Vec<ResolvedStop>,
}

/// Geometry of a render item in the node's local space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ItemShape {
    /// A rectangle centered on the local origin.
    Rect {
        /// Width in local pixels.
        width: f64,
        /// Height in local pixels.
        height: f64,
        /// Corner radius in local pixels.
        radius: f64,
        /// Fill and stroke.
        paint: ResolvedPaint,
    },
    /// A circle centered on the local origin.
    Circle {
        /// Radius in local pixels.
        radius: f64,
        /// Fill and stroke.
        paint: ResolvedPaint,
    },
    /// A text line of the item's node, baseline on the local origin. The
    /// string itself stays in the document; renderers prepare glyphs once
    /// per node instead of copying text every frame.
    Text {
        /// Font size in local pixels.
        size: f64,
        /// Glyph colour fallback (the gradient when gradient is set).
        color: Color,
        /// Index into [`RenderPlan::gradients`] when the glyphs use one.
        gradient: Option<usize>,
        /// Horizontal alignment.
        align: TextAlign,
    },
    /// A vector path; the geometry and trim values are in the plan's path
    /// arena at this index (D-0029).
    Path {
        /// Index into [`RenderPlan::paths`].
        index: usize,
    },
}

/// One path item resolved for a frame: authored geometry, trim, dash and
/// paint.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedPath {
    /// Authored points, closed flag, cap and join.
    pub geometry: PathGeometry,
    /// Trim window sampled at the frame.
    pub trim: TrimSpec,
    /// Dash pattern sampled at the frame.
    pub dash: DashSpec,
    /// Resolved fill and stroke.
    pub paint: ResolvedPaint,
}

/// One resolved draw for a frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderItem {
    /// The node that produced this item.
    pub node: NodeId,
    /// Local-to-composition transform (pixels, y down).
    pub transform: Affine2,
    /// Accumulated opacity in `0.0..=1.0`.
    pub opacity: f64,
    /// How the item composites.
    pub blend: BlendMode,
    /// What to draw.
    pub shape: ItemShape,
}

/// One effect sampled at the frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EffectValue {
    /// A tinted, blurred, offset copy of the layer drawn behind it.
    DropShadow {
        /// Straight sRGB tint, alpha included.
        color: Color,
        /// Offset in pixels, Y down.
        offset: Vec2,
        /// Blur radius in pixels.
        blur: f64,
        /// Shadow opacity in `0.0..=1.0`.
        opacity: f64,
    },
    /// A Gaussian blur of the layer.
    Blur {
        /// Blur radius in pixels.
        radius: f64,
    },
    /// A tinted, blurred copy of the layer added as light around it.
    Glow {
        /// Straight sRGB tint, alpha included.
        color: Color,
        /// Blur radius in pixels.
        radius: f64,
        /// Added light strength.
        intensity: f64,
    },
}

/// A subtree rendered into an offscreen layer before it composites.
///
/// The layer wraps `items[start..end]`; the range is the node's whole drawn
/// subtree in paint order, so layers nest properly.  A layer exists when the
/// node has an effect chain or a non-normal blend mode; without effects it is
/// the same per-draw grouping the blend modes have always used.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanLayer {
    /// The node that owns the layer.
    pub node: NodeId,
    /// First item index in the layer (inclusive).
    pub start: usize,
    /// One past the last item index in the layer (exclusive).
    pub end: usize,
    /// How the finished layer composites into its parent.
    pub blend: BlendMode,
    /// Effect chain, already sampled at the frame.
    pub effects: Vec<EffectValue>,
}

impl PlanLayer {
    /// Whether the layer wraps no draws (and can be skipped).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.end <= self.start
    }
}

/// Everything a renderer needs to draw one frame, in paint order.
///
/// A plan is reused across frames: [`crate::Evaluator::evaluate`] clears
/// and refills it, so the item buffer is allocated once.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RenderPlan {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) background: Option<Color>,
    pub(crate) items: Vec<RenderItem>,
    /// Offscreen layers wrapping item ranges, in opening order.
    pub(crate) layers: Vec<PlanLayer>,
    /// Path geometry referenced by [`ItemShape::Path`] items.
    pub(crate) paths: Vec<ResolvedPath>,
    /// Gradients referenced by [`ResolvedFill::Gradient`] values.
    pub(crate) gradients: Vec<ResolvedGradient>,
    /// Nodes skipped during the last evaluation because their resolved
    /// transform or opacity was not finite. Their subtrees were skipped
    /// too and are not counted individually.
    pub(crate) non_finite_items: u32,
}

impl RenderPlan {
    /// Creates an empty plan.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Canvas width in pixels.
    #[must_use]
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Canvas height in pixels.
    #[must_use]
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Background color painted before the first item.
    #[must_use]
    pub fn background(&self) -> Color {
        self.background.unwrap_or(Color::BLACK)
    }

    /// Items in paint order (first is painted first).
    #[must_use]
    pub fn items(&self) -> &[RenderItem] {
        &self.items
    }

    /// Offscreen layers wrapping item ranges, in opening order.
    ///
    /// A renderer walks the items and pushes the layers whose `start` is the
    /// current index, popping them when the index reaches `end`.  Empty layers
    /// are already filtered out.
    #[must_use]
    pub fn layers(&self) -> &[PlanLayer] {
        &self.layers
    }

    /// The first node with an effect chain, if any.
    ///
    /// Renderers without effect support report a typed unsupported-feature
    /// error naming this node.
    #[must_use]
    pub fn first_effect_node(&self) -> Option<NodeId> {
        self.layers
            .iter()
            .find(|layer| !layer.effects.is_empty())
            .map(|layer| layer.node)
    }

    /// The resolved path at `index`, when the item references one.
    #[must_use]
    pub fn path(&self, index: usize) -> Option<&ResolvedPath> {
        self.paths.get(index)
    }

    /// Every resolved path of the frame, in item order.
    #[must_use]
    pub fn paths(&self) -> &[ResolvedPath] {
        &self.paths
    }

    /// The resolved gradient at index, when an item references one.
    #[must_use]
    pub fn gradient(&self, index: usize) -> Option<&ResolvedGradient> {
        self.gradients.get(index)
    }

    /// Every resolved gradient of the frame, in item order.
    #[must_use]
    pub fn gradients(&self) -> &[ResolvedGradient] {
        &self.gradients
    }

    /// Adds a resolved gradient and returns its arena index.
    ///
    /// Evaluation uses this; renderer tests build small plans by hand.
    pub fn push_gradient(&mut self, gradient: ResolvedGradient) -> usize {
        let index = self.gradients.len();
        self.gradients.push(gradient);
        index
    }

    /// The first item whose text fill is a gradient, if any (D-0030).
    ///
    /// The engine's text pass has no gradient yet, so this is the part that
    /// stays unsupported even when the shape pass can draw gradients.
    #[must_use]
    pub fn first_text_gradient_node(&self) -> Option<NodeId> {
        self.items.iter().find_map(|item| {
            let yes = matches!(
                item.shape,
                ItemShape::Text {
                    gradient: Some(_),
                    ..
                }
            );
            yes.then_some(item.node)
        })
    }

    /// The first item that uses a gradient, if any (D-0030).
    ///
    /// Renderers that cannot draw gradients use this to report a typed
    /// unsupported-feature error naming the node.
    #[must_use]
    pub fn first_gradient_node(&self) -> Option<NodeId> {
        let gradient = |paint: &ResolvedPaint| {
            paint
                .fill
                .is_some_and(|fill| matches!(fill, ResolvedFill::Gradient(_)))
                || paint
                    .stroke
                    .is_some_and(|stroke| matches!(stroke.paint, ResolvedFill::Gradient(_)))
        };
        self.items.iter().find_map(|item| {
            let yes = match item.shape {
                ItemShape::Rect { paint, .. } | ItemShape::Circle { paint, .. } => gradient(&paint),
                ItemShape::Path { index } => self.path(index).is_some_and(|p| gradient(&p.paint)),
                ItemShape::Text { gradient, .. } => gradient.is_some(),
            };
            yes.then_some(item.node)
        })
    }

    /// Number of nodes skipped by the last [`crate::Evaluator::evaluate`]
    /// call because their resolved transform or accumulated opacity was
    /// not finite. Skipping a node also skips its subtree, so descendants
    /// are not counted. Reset to zero at the start of every evaluation.
    #[must_use]
    pub fn non_finite_items(&self) -> u32 {
        self.non_finite_items
    }
}

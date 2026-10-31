//! The render plan: a flat, ordered list of resolved draw items.

use motioner_core::{BlendMode, Color, NodeId, Paint, TextAlign};

use crate::affine::Affine2;

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
        paint: Paint,
    },
    /// A circle centered on the local origin.
    Circle {
        /// Radius in local pixels.
        radius: f64,
        /// Fill and stroke.
        paint: Paint,
    },
    /// A text line of the item's node, baseline on the local origin. The
    /// string itself stays in the document; renderers prepare glyphs once
    /// per node instead of copying text every frame.
    Text {
        /// Font size in local pixels.
        size: f64,
        /// Glyph color.
        color: Color,
        /// Horizontal alignment.
        align: TextAlign,
    },
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
}

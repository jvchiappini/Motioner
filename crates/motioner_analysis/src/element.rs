//! One frame of the composition as analysable elements.
//!
//! The evaluator already resolves every transform, opacity and paint, so an
//! element is a thin, exact projection of one [`RenderItem`]: its composition
//! box, its kind, its text and the order it paints in. Groups have no item;
//! their box is the union of their descendants, which is what the layout
//! reports need.

use motioner_core::{Color, Document, NodeContent, NodeId, TextAlign};
use motioner_eval::{ItemShape, RenderPlan};

use crate::geom::Box2;
use crate::text::{text_local_box, TextMeasure};

/// What an element draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ElementKind {
    /// A text block.
    Text,
    /// A rectangle, circle or vector path.
    Shape,
    /// A raster image or video frame.
    Image,
    /// A group: no pixels of its own, a union of its descendants.
    Group,
}

impl ElementKind {
    /// Stable lower-case name used in tool results.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Shape => "shape",
            Self::Image => "image",
            Self::Group => "group",
        }
    }
}

/// The text-specific facts of a text element.
#[derive(Debug, Clone, PartialEq)]
pub struct TextElement {
    /// The string as authored (`\n` included).
    pub text: String,
    /// Resolved em size in composition pixels.
    pub size: f64,
    /// Font weight.
    pub weight: u16,
    /// Resolved glyph colour.
    pub color: Color,
    /// Horizontal alignment.
    pub align: TextAlign,
    /// Whitespace-separated word count (0 for blank text).
    pub words: usize,
}

/// One node visible at one frame.
#[derive(Debug, Clone, PartialEq)]
pub struct Element {
    /// Document id.
    pub id: NodeId,
    /// Display name when the program set one.
    pub name: Option<String>,
    /// Stable identity (explicit id or provenance key).
    pub identity: String,
    /// What it draws.
    pub kind: ElementKind,
    /// Composition-space box; `None` for a group that draws nothing.
    pub bounds: Option<Box2>,
    /// Accumulated opacity in `0.0..=1.0`.
    pub opacity: f64,
    /// Paint order, lowest first.
    pub z: usize,
    /// Text facts for a text element.
    pub text: Option<TextElement>,
    /// Primary fill or glyph colour, when there is one.
    pub color: Option<Color>,
    /// Whether the element is visually busy behind text: an image (its
    /// content is unknown) or a gradient-filled shape.
    pub busy: bool,
}

impl Element {
    /// The text, when this is a text element.
    #[must_use]
    pub fn as_text(&self) -> Option<&TextElement> {
        self.text.as_ref()
    }

    /// Whether the element draws a box at all.
    #[must_use]
    pub fn has_bounds(&self) -> bool {
        self.bounds.is_some()
    }

    /// The box, or an empty box at the origin when the element draws nothing.
    #[must_use]
    pub fn bounds_or_empty(&self) -> Box2 {
        self.bounds.unwrap_or(Box2 {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 0.0,
            max_y: 0.0,
        })
    }
}

/// Every element visible at one evaluated frame.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneFrame {
    /// Integer frame index.
    pub frame: i64,
    /// Canvas box in composition pixels.
    pub canvas: Box2,
    /// Elements, painted first to last.
    pub elements: Vec<Element>,
    /// Composition background.
    pub background: Color,
}

impl SceneFrame {
    /// The canvas scaled inwards by `factor` (0.9 for title safe).
    #[must_use]
    pub fn safe_area(&self, factor: f64) -> Box2 {
        self.canvas.scaled(factor)
    }
}

/// The local box, text facts, primary colour and busy flag of one item.
struct LocalFacts {
    bounds: Box2,
    text: Option<TextElement>,
    color: Option<Color>,
    busy: bool,
}

fn local_box(
    document: &Document,
    plan: &RenderPlan,
    item: &motioner_eval::RenderItem,
    measure: &dyn TextMeasure,
) -> Option<LocalFacts> {
    let busy = match item.shape {
        ItemShape::Image { .. } => true,
        ItemShape::Rect { paint, .. } | ItemShape::Circle { paint, .. } => {
            matches!(paint.fill, Some(motioner_eval::ResolvedFill::Gradient(_)))
        }
        ItemShape::Path { index } => plan.path(index).is_some_and(|path| {
            matches!(
                path.paint.fill,
                Some(motioner_eval::ResolvedFill::Gradient(_))
            ) || matches!(
                path.paint.stroke.map(|stroke| stroke.paint),
                Some(motioner_eval::ResolvedFill::Gradient(_))
            )
        }),
        ItemShape::Text { gradient, .. } => gradient.is_some(),
    };
    let color = match item.shape {
        ItemShape::Rect { paint, .. } | ItemShape::Circle { paint, .. } => {
            paint.fill.and_then(|fill| match fill {
                motioner_eval::ResolvedFill::Solid(color) => Some(color),
                motioner_eval::ResolvedFill::Gradient(_) => None,
            })
        }
        ItemShape::Path { index } => {
            plan.path(index)
                .and_then(|path| path.paint.fill)
                .and_then(|fill| match fill {
                    motioner_eval::ResolvedFill::Solid(color) => Some(color),
                    motioner_eval::ResolvedFill::Gradient(_) => None,
                })
        }
        ItemShape::Text { color, .. } => Some(color),
        ItemShape::Image { .. } => None,
    };
    let bounds = match item.shape {
        ItemShape::Rect { width, height, .. } | ItemShape::Image { width, height, .. } => {
            Box2::new(-width * 0.5, -height * 0.5, width * 0.5, height * 0.5)?
        }
        ItemShape::Circle { radius, .. } => Box2::new(-radius, -radius, radius, radius)?,
        ItemShape::Path { index } => {
            let path = plan.path(index)?;
            Box2::from_array(path.geometry.local_bounds()?)?
        }
        ItemShape::Text {
            size,
            align,
            line_height,
            color: glyph_color,
            ..
        } => {
            let node = document.node(item.node)?;
            let NodeContent::Text(content) = node.content() else {
                return None;
            };
            let bounds = text_local_box(content, size, line_height, align, measure)?;
            let words = content.text.split_whitespace().count();
            let text = Some(TextElement {
                text: content.text.clone(),
                size,
                weight: content.weight,
                color: glyph_color,
                align,
                words,
            });
            return Some(LocalFacts {
                bounds,
                text,
                color: Some(glyph_color),
                busy,
            });
        }
    };
    Some(LocalFacts {
        bounds,
        text: None,
        color,
        busy,
    })
}

/// Projects the plan's items into elements, adding groups from the document.
///
/// Node boxes are the axis-aligned bounds of their mapped local box; a group's
/// box is the union of its drawn descendants. Only visible nodes with a
/// non-zero accumulated opacity appear (that is exactly what the plan holds).
#[must_use]
pub fn frame_elements(
    document: &Document,
    plan: &RenderPlan,
    frame: i64,
    measure: &dyn TextMeasure,
) -> SceneFrame {
    let composition = document.composition();
    let canvas = Box2::new(
        0.0,
        0.0,
        f64::from(composition.width()),
        f64::from(composition.height()),
    )
    .unwrap_or(Box2 {
        min_x: 0.0,
        min_y: 0.0,
        max_x: 0.0,
        max_y: 0.0,
    });
    let mut elements: Vec<Element> = Vec::with_capacity(plan.items().len());
    for (z, item) in plan.items().iter().enumerate() {
        let Some(node) = document.node(item.node) else {
            continue;
        };
        let Some(facts) = local_box(document, plan, item, measure) else {
            continue;
        };
        let bounds = facts.bounds.transformed(item.transform);
        elements.push(Element {
            id: item.node,
            name: node.name().map(str::to_owned),
            identity: node.identity().to_string(),
            kind: match item.shape {
                ItemShape::Text { .. } => ElementKind::Text,
                ItemShape::Image { .. } => ElementKind::Image,
                ItemShape::Rect { .. } | ItemShape::Circle { .. } | ItemShape::Path { .. } => {
                    ElementKind::Shape
                }
            },
            bounds,
            opacity: item.opacity,
            z,
            text: facts.text,
            color: facts.color,
            busy: facts.busy,
        });
    }
    // Groups: every ancestor of a drawn node gets the union of its subtree.
    let mut group_bounds: Vec<(NodeId, Box2)> = Vec::new();
    for element in &elements {
        let Some(bounds) = element.bounds else {
            continue;
        };
        let mut current = document
            .node(element.id)
            .and_then(motioner_core::Node::parent);
        while let Some(parent) = current {
            if let Some(entry) = group_bounds.iter_mut().find(|(id, _)| *id == parent) {
                entry.1 = entry.1.union(bounds);
            } else {
                group_bounds.push((parent, bounds));
            }
            current = document.node(parent).and_then(motioner_core::Node::parent);
        }
    }
    for (id, bounds) in group_bounds {
        if elements.iter().any(|element| element.id == id) {
            continue;
        }
        let Some(node) = document.node(id) else {
            continue;
        };
        elements.push(Element {
            id,
            name: node.name().map(str::to_owned),
            identity: node.identity().to_string(),
            kind: ElementKind::Group,
            bounds: Some(bounds),
            opacity: 1.0,
            z: elements.len(),
            text: None,
            color: None,
            busy: false,
        });
    }
    SceneFrame {
        frame,
        canvas,
        elements,
        background: plan.background(),
    }
}

/// The element for `id`, when it is visible at this frame.
#[must_use]
pub fn element_of(scene: &SceneFrame, id: NodeId) -> Option<&Element> {
    scene.elements.iter().find(|element| element.id == id)
}

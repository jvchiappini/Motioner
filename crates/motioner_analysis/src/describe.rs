//! `describe_frame`: the exact text twin of a render (D-0043).

use motioner_core::{Color, Document, NodeId};
use motioner_eval::RenderPlan;

use crate::element::{frame_elements, Element, SceneFrame};
use crate::text::TextMeasure;

/// A description of one frame: every visible element with its box, kind,
/// colour, text and paint order.
#[derive(Debug, Clone, PartialEq)]
pub struct FrameDescription {
    /// Integer frame index.
    pub frame: i64,
    /// Canvas size in pixels.
    pub width: u32,
    /// Canvas height in pixels.
    pub height: u32,
    /// Composition background.
    pub background: Color,
    /// Visible elements, painted first to last.
    pub elements: Vec<Element>,
}

impl FrameDescription {
    /// The element of `id`, when visible.
    #[must_use]
    pub fn element(&self, id: NodeId) -> Option<&Element> {
        self.elements.iter().find(|element| element.id == id)
    }

    /// The frame as reusable scene data (the layout reports take it).
    #[must_use]
    pub fn scene(&self) -> SceneFrame {
        SceneFrame {
            frame: self.frame,
            canvas: crate::geom::Box2 {
                min_x: 0.0,
                min_y: 0.0,
                max_x: f64::from(self.width),
                max_y: f64::from(self.height),
            },
            elements: self.elements.clone(),
            background: self.background,
        }
    }
}

/// Describes the evaluated frame, optionally keeping only a set of node ids.
///
/// Kids are matched by id; a group kept by the filter but drawn through its
/// children still appears with the union of those children's boxes.
#[must_use]
pub fn describe_frame(
    document: &Document,
    plan: &RenderPlan,
    frame: i64,
    ids: Option<&[NodeId]>,
    measure: &dyn TextMeasure,
) -> FrameDescription {
    let scene = frame_elements(document, plan, frame, measure);
    let keep = |element: &Element| match ids {
        None => true,
        Some(ids) => ids.contains(&element.id),
    };
    FrameDescription {
        frame,
        width: plan.width(),
        height: plan.height(),
        background: plan.background(),
        elements: scene.elements.into_iter().filter(keep).collect::<Vec<_>>(),
    }
}

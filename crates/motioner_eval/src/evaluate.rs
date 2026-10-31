//! Frame evaluation: document plus frame to render plan.

use motioner_core::{Document, Frame, NodeContent, NodeId};

use crate::affine::Affine2;
use crate::curve::sample;
use crate::plan::{ItemShape, RenderItem, RenderPlan};

/// A node waiting to be visited, with its parent's resolved state.
#[derive(Debug, Clone, Copy)]
struct Pending {
    id: NodeId,
    parent_transform: Affine2,
    parent_opacity: f64,
}

/// Evaluates documents into render plans.
///
/// Evaluation is a pure function of the document and the frame: no clock,
/// no randomness, no I/O. The evaluator keeps a traversal stack between
/// calls so steady-state evaluation does not allocate.
#[derive(Debug, Default)]
pub struct Evaluator {
    stack: Vec<Pending>,
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
    pub fn evaluate(&mut self, document: &Document, frame: Frame, plan: &mut RenderPlan) {
        let composition = document.composition();
        plan.width = composition.width();
        plan.height = composition.height();
        plan.background = Some(composition.background());
        plan.items.clear();

        self.stack.clear();
        self.push_children(document.roots(), Affine2::IDENTITY, 1.0);
        while let Some(pending) = self.stack.pop() {
            let Some(node) = document.node(pending.id) else {
                continue;
            };
            let props = node.transform();
            let local = Affine2::layer(
                sample(&props.position, frame),
                sample(&props.rotation, frame),
                sample(&props.scale, frame),
                sample(&props.anchor, frame),
            );
            let transform = pending.parent_transform.then(local);
            let opacity = pending.parent_opacity * sample(&props.opacity, frame).clamp(0.0, 1.0);
            if opacity <= 0.0 {
                continue;
            }
            if let Some(shape) = item_shape(node.content()) {
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
        self.stack.extend(ids.iter().rev().map(|&id| Pending {
            id,
            parent_transform,
            parent_opacity,
        }));
    }
}

fn item_shape(content: &NodeContent) -> Option<ItemShape> {
    match content {
        NodeContent::Group => None,
        &NodeContent::Rect {
            width,
            height,
            radius,
            paint,
        } => Some(ItemShape::Rect {
            width,
            height,
            radius,
            paint,
        }),
        &NodeContent::Circle { radius, paint } => Some(ItemShape::Circle { radius, paint }),
        NodeContent::Text(text) => Some(ItemShape::Text {
            size: text.size,
            color: text.color,
            align: text.align,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use motioner_core::{
        Color, Composition, Ease, FrameRate, Paint, Property, PropertyValue, Vec2,
    };

    fn document() -> Document {
        let fps = FrameRate::new(60, 1).unwrap();
        Document::new(Composition::new("Test", 100, 100, fps, 60, Color::BLACK).unwrap())
    }

    fn square(doc: &mut Document) -> NodeId {
        doc.add_node(NodeContent::Rect {
            width: 10.0,
            height: 10.0,
            radius: 0.0,
            paint: Paint {
                fill: Some(Color::WHITE),
                stroke: None,
            },
        })
        .unwrap()
    }

    fn evaluate(doc: &Document, frame: Frame) -> RenderPlan {
        let mut plan = RenderPlan::new();
        Evaluator::new().evaluate(doc, frame, &mut plan);
        plan
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
}

//! Frame evaluation: document plus frame to render plan.

use motioner_core::{Document, Frame, NodeContent, NodeId, Paint, PaintSpec, Stroke};

use crate::affine::Affine2;
use crate::curve::{sample, sample_vec2};
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
    ///
    /// A node whose resolved transform or accumulated opacity is not finite
    /// is skipped together with its subtree, because no renderer can draw
    /// it; each such skip increments [`RenderPlan::non_finite_items`].
    pub fn evaluate(&mut self, document: &Document, frame: Frame, plan: &mut RenderPlan) {
        let composition = document.composition();
        plan.width = composition.width();
        plan.height = composition.height();
        plan.background = Some(composition.background());
        plan.items.clear();
        plan.non_finite_items = 0;

        self.stack.clear();
        self.push_children(document.roots(), Affine2::IDENTITY, 1.0);
        while let Some(pending) = self.stack.pop() {
            let Some(node) = document.node(pending.id) else {
                continue;
            };
            // A hidden node is not drawn, and neither is anything below it:
            // visibility is inherited, like opacity but without a value.
            if !node.is_visible() {
                continue;
            }
            let props = node.transform();
            let local = Affine2::layer(
                sample_vec2(&props.position, frame),
                sample(&props.rotation, frame),
                sample_vec2(&props.scale, frame),
                sample_vec2(&props.anchor, frame),
            );
            let transform = pending.parent_transform.then(local);
            let opacity = pending.parent_opacity * sample(&props.opacity, frame).clamp(0.0, 1.0);
            if !transform.is_finite() || !opacity.is_finite() {
                plan.non_finite_items = plan.non_finite_items.saturating_add(1);
                continue;
            }
            if opacity <= 0.0 {
                continue;
            }
            if let Some(shape) = item_shape(node.content(), frame) {
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

fn item_shape(content: &NodeContent, frame: Frame) -> Option<ItemShape> {
    match content {
        NodeContent::Group => None,
        NodeContent::Rect {
            width,
            height,
            radius,
            paint,
        } => Some(ItemShape::Rect {
            width: sample(width, frame),
            height: sample(height, frame),
            radius: sample(radius, frame),
            paint: resolved_paint(paint, frame),
        }),
        NodeContent::Circle { radius, paint } => Some(ItemShape::Circle {
            radius: sample(radius, frame),
            paint: resolved_paint(paint, frame),
        }),
        NodeContent::Text(text) => Some(ItemShape::Text {
            size: sample(&text.size, frame),
            color: sample(&text.color, frame),
            align: text.align,
        }),
    }
}

/// Samples the fill and stroke channels into the resolved paint of one frame.
///
/// A stroke is drawn only when its colour is present and its width is above
/// zero, the same rule the constructors use for `stroke_width = 0`.
fn resolved_paint(spec: &PaintSpec, frame: Frame) -> Paint {
    let width = sample(&spec.stroke_width, frame);
    let stroke = sample(&spec.stroke, frame)
        .filter(|_| width > 0.0)
        .map(|color| Stroke { color, width });
    Paint {
        fill: sample(&spec.fill, frame),
        stroke,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use motioner_core::{
        Color, Composition, Ease, FrameRate, Paint, Property, PropertyValue, Vec2, MAX_ABS_FRAME,
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
}

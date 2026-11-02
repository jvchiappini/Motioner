//! Frame evaluation: document plus frame to render plan.

use std::collections::HashSet;

use motioner_core::{
    BlendMode, DashSpec, Document, Effect, Frame, Gradient, Node, NodeContent, NodeId, PaintSpec,
    TrimSpec,
};

use crate::affine::Affine2;
use crate::curve::{sample, sample_vec2};
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
        let composition = document.composition();
        plan.width = composition.width();
        plan.height = composition.height();
        plan.background = Some(composition.background());
        plan.items.clear();
        plan.layers.clear();
        plan.gradients.clear();
        plan.non_finite_items = 0;

        self.stack.clear();
        self.mask_sources.clear();
        for node in document.nodes() {
            if let Some(mask) = node.mask() {
                self.mask_sources.insert(mask.source);
            }
        }
        self.push_children(document.roots(), Affine2::IDENTITY, 1.0);
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
                sample_vec2(&props.position, frame),
                sample(&props.rotation, frame),
                sample_vec2(&props.scale, frame),
                sample_vec2(&props.anchor, frame),
            );
            let transform = parent_transform.then(local);
            let opacity = parent_opacity * sample(&props.opacity, frame).clamp(0.0, 1.0);
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
                    effects: resolved_effects(node, frame),
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
            if let Some(shape) = item_shape(node.content(), frame, plan) {
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
        // A layer whose subtree draws nothing is not a layer at all.
        plan.layers.retain(|layer| !layer.is_empty());
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

/// Samples a node's effect stack, in application order.
fn resolved_effects(node: &Node, frame: Frame) -> Vec<EffectValue> {
    node.effects()
        .iter()
        .map(|effect| match effect {
            Effect::DropShadow(effect) => EffectValue::DropShadow {
                color: sample(&effect.color, frame),
                offset: sample_vec2(&effect.offset, frame),
                blur: sample(&effect.blur, frame),
                opacity: sample(&effect.opacity, frame),
            },
            Effect::Blur(effect) => EffectValue::Blur {
                radius: sample(&effect.radius, frame),
            },
            Effect::Glow(effect) => EffectValue::Glow {
                color: sample(&effect.color, frame),
                radius: sample(&effect.radius, frame),
                intensity: sample(&effect.intensity, frame),
            },
        })
        .collect()
}

fn item_shape(content: &NodeContent, frame: Frame, plan: &mut RenderPlan) -> Option<ItemShape> {
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
            paint: resolved_paint(paint, frame, plan),
        }),
        NodeContent::Circle { radius, paint } => Some(ItemShape::Circle {
            radius: sample(radius, frame),
            paint: resolved_paint(paint, frame, plan),
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
            let resolved = resolved_paint(paint, frame, plan);
            let index = plan.paths.len();
            plan.paths.push(ResolvedPath {
                geometry: geometry.clone(),
                trim: TrimSpec {
                    start: sample(trim_start, frame),
                    end: sample(trim_end, frame),
                    offset: sample(trim_offset, frame),
                },
                dash: DashSpec {
                    length: sample(dash, frame),
                    offset: sample(dash_offset, frame),
                },
                paint: resolved,
            });
            Some(ItemShape::Path { index })
        }
        NodeContent::Text(text) => {
            let (color, gradient) = match text.gradient.as_ref() {
                Some(gradient) => match resolve_gradient(gradient, frame, plan) {
                    ResolvedFill::Gradient(index) => (sample(&text.color, frame), Some(index)),
                    ResolvedFill::Solid(color) => (color, None),
                },
                None => (sample(&text.color, frame), None),
            };
            Some(ItemShape::Text {
                size: sample(&text.size, frame),
                color,
                gradient,
                align: text.align,
            })
        }
    }
}

/// Samples the fill and stroke channels into the resolved paint of one frame.
///
/// A stroke is drawn only when its colour is present and its width is above
/// zero, the same rule the constructors use for `stroke_width = 0`.
fn resolved_paint(spec: &PaintSpec, frame: Frame, plan: &mut RenderPlan) -> ResolvedPaint {
    let width = sample(&spec.stroke_width, frame);
    let fill = match spec.fill_gradient.as_ref() {
        Some(gradient) => Some(resolve_gradient(gradient, frame, plan)),
        None => sample(&spec.fill, frame).map(ResolvedFill::Solid),
    };
    let stroke = if width > 0.0 {
        match spec.stroke_gradient.as_ref() {
            Some(gradient) => Some(ResolvedStroke {
                paint: resolve_gradient(gradient, frame, plan),
                width,
            }),
            None => sample(&spec.stroke, frame).map(|color| ResolvedStroke {
                paint: ResolvedFill::Solid(color),
                width,
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
fn resolve_gradient(gradient: &Gradient, frame: Frame, plan: &mut RenderPlan) -> ResolvedFill {
    let mut stops: Vec<ResolvedStop> = gradient
        .stops()
        .iter()
        .map(|stop| ResolvedStop {
            offset: sample(&stop.offset, frame).clamp(0.0, 1.0),
            color: sample(&stop.color, frame),
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
}

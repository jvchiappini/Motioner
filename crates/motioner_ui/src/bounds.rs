//! Selection bounds and canvas hit testing in composition pixels.
//!
//! Every plan item has a local box (a rectangle, a circle's square, or a text
//! line's advance × ascender/descender) that its affine transform maps into
//! the composition. A node's bounds are the axis-aligned union of the mapped
//! corners of its own items and its descendants' items, so selecting a group
//! frames everything inside it. Hit testing walks items top-most first and
//! tests the pointer against each item's *oriented* box through the inverse
//! transform, so rotated shapes are picked precisely.
//!
//! Text measurement needs the engine font, so it is passed in as a closure;
//! the tests use a fixed-width stand-in.

use motioner_core::{Document, NodeContent, NodeId, TextAlign, Vec2};
use motioner_eval::{Affine2, ItemShape, RenderItem, RenderPlan};
use motioner_render::TextExtent;

/// An axis-aligned box in composition pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    /// Left edge.
    pub min_x: f64,
    /// Top edge.
    pub min_y: f64,
    /// Right edge.
    pub max_x: f64,
    /// Bottom edge.
    pub max_y: f64,
}

impl Bounds {
    fn empty() -> Self {
        Self {
            min_x: f64::INFINITY,
            min_y: f64::INFINITY,
            max_x: f64::NEG_INFINITY,
            max_y: f64::NEG_INFINITY,
        }
    }

    fn add(&mut self, p: Vec2) {
        self.min_x = self.min_x.min(p.x);
        self.min_y = self.min_y.min(p.y);
        self.max_x = self.max_x.max(p.x);
        self.max_y = self.max_y.max(p.y);
    }

    fn is_valid(&self) -> bool {
        self.min_x.is_finite()
            && self.min_y.is_finite()
            && self.max_x >= self.min_x
            && self.max_y >= self.min_y
    }
}

/// Measures a text line (`text`, `size`) in composition pixels.
pub trait MeasureText {
    /// The extent of one line, or `None` if it cannot be measured.
    fn extent(&self, text: &str, size: f64) -> Option<TextExtent>;
}

impl<F: Fn(&str, f64) -> Option<TextExtent>> MeasureText for F {
    fn extent(&self, text: &str, size: f64) -> Option<TextExtent> {
        self(text, size)
    }
}

/// The local `[x0, y0, x1, y1]` box of one item (y down, origin at the node).
fn local_box(
    document: &Document,
    plan: &RenderPlan,
    item: &RenderItem,
    measure: &impl MeasureText,
) -> Option<[f64; 4]> {
    match item.shape {
        ItemShape::Rect { width, height, .. } | ItemShape::Image { width, height, .. } => {
            Some([-width * 0.5, -height * 0.5, width * 0.5, height * 0.5])
        }
        ItemShape::Circle { radius, .. } => Some([-radius, -radius, radius, radius]),
        ItemShape::Path { index } => motioner_render::path_bounds(plan.path(index)?),
        ItemShape::Text { size, align, .. } => {
            let NodeContent::Text(content) = document.node(item.node)?.content() else {
                return None;
            };
            let extent = measure.extent(&content.text, size)?;
            let x0 = match align {
                TextAlign::Left => 0.0,
                TextAlign::Center => -extent.advance * 0.5,
                TextAlign::Right => -extent.advance,
            };
            Some([x0, -extent.ascender, x0 + extent.advance, -extent.descender])
        }
    }
}

/// Whether `node` is `ancestor` or lies below it.
fn descends_from(document: &Document, mut node: NodeId, ancestor: NodeId) -> bool {
    loop {
        if node == ancestor {
            return true;
        }
        match document.node(node).and_then(motioner_core::Node::parent) {
            Some(parent) => node = parent,
            None => return false,
        }
    }
}

/// The composition-space bounds of `node` and its descendants in `plan`.
///
/// Returns `None` when nothing of the node is drawn in this frame (for
/// example an empty group, or text that cannot be measured).
#[must_use]
pub fn node_bounds(
    document: &Document,
    plan: &RenderPlan,
    node: NodeId,
    measure: &impl MeasureText,
) -> Option<Bounds> {
    let mut bounds = Bounds::empty();
    for item in plan.items() {
        if !descends_from(document, item.node, node) {
            continue;
        }
        let Some([x0, y0, x1, y1]) = local_box(document, plan, item, measure) else {
            continue;
        };
        for (x, y) in [(x0, y0), (x1, y0), (x1, y1), (x0, y1)] {
            bounds.add(item.transform.apply(Vec2::new(x, y)));
        }
    }
    bounds.is_valid().then_some(bounds)
}

fn inverse(t: Affine2) -> Option<Affine2> {
    let det = t.determinant();
    if !det.is_finite() || det.abs() < 1e-12 {
        return None;
    }
    let inv = 1.0 / det;
    Some(Affine2 {
        a: t.d * inv,
        b: -t.b * inv,
        c: -t.c * inv,
        d: t.a * inv,
        tx: (t.c * t.ty - t.d * t.tx) * inv,
        ty: (t.b * t.tx - t.a * t.ty) * inv,
    })
}

/// The top-most node drawn under `point` (composition pixels), if any.
#[must_use]
pub fn hit_node(
    document: &Document,
    plan: &RenderPlan,
    point: Vec2,
    measure: &impl MeasureText,
) -> Option<NodeId> {
    hit_node_where(document, plan, point, measure, |_| true)
}

/// Like [`hit_node`], ignoring nodes for which `allow` returns `false`.
///
/// The editor uses it to skip locked layers: they cannot be picked on the
/// canvas.
#[must_use]
pub fn hit_node_where(
    document: &Document,
    plan: &RenderPlan,
    point: Vec2,
    measure: &impl MeasureText,
    allow: impl Fn(NodeId) -> bool,
) -> Option<NodeId> {
    plan.items().iter().rev().find_map(|item| {
        if item.opacity <= 0.0 || !allow(item.node) {
            return None;
        }
        let [x0, y0, x1, y1] = local_box(document, plan, item, measure)?;
        let local = inverse(item.transform)?.apply(point);
        let inside = match item.shape {
            ItemShape::Circle { radius, .. } => {
                local.x * local.x + local.y * local.y <= radius * radius
            }
            ItemShape::Path { index } => plan.path(index).is_some_and(|path| {
                path.geometry.contains(local)
                    || path.paint.stroke.is_some_and(|stroke| {
                        stroke.width > 0.0 && path.geometry.near(local, stroke.width * 0.5 + 2.0)
                    })
            }),
            _ => local.x >= x0 && local.x <= x1 && local.y >= y0 && local.y <= y1,
        };
        inside.then_some(item.node)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use motioner_eval::Evaluator;

    const HELLO: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/hello.motion"
    ));

    /// Half an em per character, ascender 0.9 em, descender -0.25 em.
    #[allow(clippy::unnecessary_wraps)] // must match `MeasureText`'s `Option` result
    fn measure(text: &str, size: f64) -> Option<TextExtent> {
        Some(TextExtent {
            advance: text.chars().count() as f64 * size * 0.5,
            ascender: size * 0.9,
            descender: -size * 0.25,
        })
    }

    fn evaluated(frame: i64) -> (Document, RenderPlan) {
        let document = motioner_dsl::run(HELLO, &motioner_dsl::Limits::default()).unwrap();
        let mut plan = RenderPlan::new();
        Evaluator::new().evaluate(&document, frame, &mut plan);
        (document, plan)
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn circle_bounds_follow_the_transform() {
        let (document, plan) = evaluated(120);
        let rows = crate::outline_rows(&document);
        let first_circle = rows
            .iter()
            .find(|r| r.kind == crate::outline::NodeKind::Circle)
            .unwrap()
            .node;
        let b = node_bounds(&document, &plan, first_circle, &measure).unwrap();
        // Dot 1 at (480, 820), radius 24, fully scaled in by frame 120.
        assert!(close(b.min_x, 456.0) && close(b.max_x, 504.0), "{b:?}");
        assert!(close(b.min_y, 796.0) && close(b.max_y, 844.0), "{b:?}");
    }

    #[test]
    fn centred_text_bounds_straddle_the_origin() {
        let (document, plan) = evaluated(120);
        let title = document.roots()[0];
        let b = node_bounds(&document, &plan, title, &measure).unwrap();
        // "Motioner" = 8 chars * 80 px = 640 px wide, centred on x = 960.
        assert!(close(b.min_x, 640.0) && close(b.max_x, 1280.0), "{b:?}");
        assert!(
            close(b.min_y, 480.0 - 144.0) && close(b.max_y, 480.0 + 40.0),
            "{b:?}"
        );
    }

    #[test]
    fn group_bounds_cover_their_children() {
        let (document, plan) = evaluated(150);
        let rows = crate::outline_rows(&document);
        let group = rows
            .iter()
            .find(|r| r.kind == crate::outline::NodeKind::Group)
            .unwrap()
            .node;
        let child = rows
            .iter()
            .find(|r| r.kind == crate::outline::NodeKind::Rect)
            .unwrap()
            .node;
        let g = node_bounds(&document, &plan, group, &measure).unwrap();
        let c = node_bounds(&document, &plan, child, &measure).unwrap();
        assert_eq!(g, c, "a group with one child frames that child");
        // The rotated 96 px square is wider than 96 px on screen.
        assert!(g.max_x - g.min_x > 96.0);
    }

    #[test]
    fn hits_pick_the_top_most_item_and_respect_rotation() {
        let (document, plan) = evaluated(150);
        let rows = crate::outline_rows(&document);
        let circle = rows
            .iter()
            .filter(|r| r.kind == crate::outline::NodeKind::Circle)
            .nth(2)
            .unwrap()
            .node;
        assert_eq!(
            hit_node(&document, &plan, Vec2::new(960.0, 820.0), &measure),
            Some(circle)
        );
        // Just outside the circle's radius but inside its square box: no hit.
        assert_eq!(
            hit_node(
                &document,
                &plan,
                Vec2::new(960.0 + 20.0, 820.0 + 20.0),
                &measure
            ),
            None
        );
        let rect = rows
            .iter()
            .find(|r| r.kind == crate::outline::NodeKind::Rect)
            .unwrap()
            .node;
        assert_eq!(
            hit_node(&document, &plan, Vec2::new(240.0, 220.0), &measure),
            Some(rect)
        );
        assert_eq!(
            hit_node(&document, &plan, Vec2::new(5.0, 5.0), &measure),
            None
        );
    }

    #[test]
    fn invisible_items_are_not_hit() {
        let (document, plan) = evaluated(0);
        // At frame 0 the title has opacity 0 and the spinners are hidden.
        assert_eq!(
            hit_node(&document, &plan, Vec2::new(960.0, 500.0), &measure),
            None
        );
    }

    #[test]
    fn inverse_round_trips() {
        let t = Affine2::layer(
            Vec2::new(10.0, 20.0),
            33.0,
            Vec2::new(2.0, 0.5),
            Vec2::new(3.0, 4.0),
        );
        let p = Vec2::new(7.0, -2.0);
        let back = inverse(t).unwrap().apply(t.apply(p));
        assert!(close(back.x, p.x) && close(back.y, p.y));
        assert!(inverse(Affine2 {
            a: 0.0,
            b: 0.0,
            c: 0.0,
            d: 0.0,
            tx: 1.0,
            ty: 1.0
        })
        .is_none());
    }

    const PATHS: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/paths.motion"
    ));

    #[test]
    fn path_bounds_and_hits_follow_the_outline() {
        let document = motioner_dsl::run(PATHS, &motioner_dsl::Limits::default()).unwrap();
        let mut plan = RenderPlan::new();
        motioner_eval::Evaluator::new().evaluate(&document, 45, &mut plan);
        let star = document.find("star").unwrap();
        let bounds = node_bounds(&document, &plan, star, &measure).unwrap();
        // The star's outer radius is 170 px around (200, 270).
        assert!(bounds.min_x < 60.0 && bounds.max_x > 340.0, "{bounds:?}");
        assert!(bounds.min_y < 130.0 && bounds.max_y > 410.0, "{bounds:?}");
        assert_eq!(
            hit_node(&document, &plan, Vec2::new(200.0, 270.0), &measure),
            Some(star)
        );
        // A corner of the bounding box lies between two spikes: no hit. The
        // full-canvas backdrop is excluded because it is behind everything.
        let backdrop = document.roots()[0];
        assert_eq!(
            hit_node_where(
                &document,
                &plan,
                Vec2::new(355.0, 115.0),
                &measure,
                |node| node != backdrop,
            ),
            None
        );
    }
}

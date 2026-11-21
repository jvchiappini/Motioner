//! `layout_report`: overlaps, safe area, alignment and type hierarchy.
//!
//! Everything is computed from one evaluated [`SceneFrame`]: exact boxes,
//! exact paint order, exact text sizes. "Busy" background means an image or a
//! gradient, whose pixels the scene graph cannot describe.

use motioner_core::NodeId;

use crate::element::{Element, ElementKind, SceneFrame};
use crate::geom::Box2;
use crate::rules::Rules;

/// Two text blocks that overlap.
#[derive(Debug, Clone, PartialEq)]
pub struct TextOverlap {
    /// First node.
    pub a: NodeId,
    /// Second node.
    pub b: NodeId,
    /// Shared area as a fraction of the smaller box.
    pub fraction: f64,
}

/// A text block over a busy shape or image.
#[derive(Debug, Clone, PartialEq)]
pub struct BusyOverlap {
    /// The text node.
    pub text: NodeId,
    /// The busy shape or image.
    pub busy: NodeId,
    /// Shared area as a fraction of the text box.
    pub fraction: f64,
}

/// Text outside the title-safe rectangle.
#[derive(Debug, Clone, PartialEq)]
pub struct SafeViolation {
    /// The text node.
    pub id: NodeId,
    /// How far outside the canvas the box is, in pixels (0 when inside).
    pub outside_pixels: f64,
}

/// Something drawn entirely outside the canvas.
#[derive(Debug, Clone, PartialEq)]
pub struct OffScreen {
    /// The node.
    pub id: NodeId,
    /// Whether it is text.
    pub text: bool,
}

/// Text boxes sharing an edge.
#[derive(Debug, Clone, PartialEq)]
pub struct AlignmentGroup {
    /// `left`, `center` or `right`.
    pub edge: &'static str,
    /// Shared coordinate in pixels.
    pub coordinate: f64,
    /// Nodes on that edge.
    pub nodes: Vec<NodeId>,
}

/// One type size and how many texts use it.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeLevel {
    /// Em size in pixels.
    pub size: f64,
    /// Number of text blocks at this size.
    pub count: usize,
}

/// The layout facts of one frame.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutReport {
    /// Frame index.
    pub frame: i64,
    /// Canvas box.
    pub canvas: Box2,
    /// Title-safe box used for the checks.
    pub title_safe: Box2,
    /// Text/text overlaps.
    pub text_overlaps: Vec<TextOverlap>,
    /// Text over a busy shape or image.
    pub busy_overlaps: Vec<BusyOverlap>,
    /// Text outside the title-safe box.
    pub safe_violations: Vec<SafeViolation>,
    /// Elements entirely outside the canvas.
    pub off_screen: Vec<OffScreen>,
    /// Groups of two or more texts sharing an edge.
    pub alignment_groups: Vec<AlignmentGroup>,
    /// Distinct type sizes, largest first.
    pub type_levels: Vec<TypeLevel>,
}

fn visible_texts(scene: &SceneFrame) -> impl Iterator<Item = &Element> {
    scene
        .elements
        .iter()
        .filter(|element| element.kind == ElementKind::Text && element.opacity > 0.05)
}

fn distance_outside(box_: Box2, container: Box2) -> f64 {
    let horizontal = (container.min_x - box_.min_x)
        .max(box_.max_x - container.max_x)
        .max(0.0);
    let vertical = (container.min_y - box_.min_y)
        .max(box_.max_y - container.max_y)
        .max(0.0);
    horizontal.max(vertical)
}

/// Analyses one scene frame.
#[allow(clippy::too_many_lines)] // one pass per rule family, kept linear
#[must_use]
pub fn layout_report(scene: &SceneFrame, rules: &Rules) -> LayoutReport {
    let title_safe = scene.safe_area(rules.title_safe);

    let texts: Vec<&Element> = visible_texts(scene).collect();
    let mut text_overlaps = Vec::new();
    for (index, first) in texts.iter().enumerate() {
        for second in texts.iter().skip(index + 1) {
            let (Some(a), Some(b)) = (first.bounds, second.bounds) else {
                continue;
            };
            let shared = a.intersection_area(b);
            let smaller = a.area().min(b.area()).max(1.0);
            let fraction = shared / smaller;
            if fraction > rules.text_overlap_fraction {
                text_overlaps.push(TextOverlap {
                    a: first.id,
                    b: second.id,
                    fraction,
                });
            }
        }
    }

    let mut busy_overlaps = Vec::new();
    let busy: Vec<&Element> = scene
        .elements
        .iter()
        .filter(|element| element.busy && element.kind != ElementKind::Text)
        .collect();
    for text in &texts {
        let Some(text_box) = text.bounds else {
            continue;
        };
        for shape in &busy {
            let Some(shape_box) = shape.bounds else {
                continue;
            };
            let fraction = text_box.intersection_area(shape_box) / text_box.area().max(1.0);
            if fraction > rules.text_busy_fraction {
                busy_overlaps.push(BusyOverlap {
                    text: text.id,
                    busy: shape.id,
                    fraction,
                });
            }
        }
    }

    let safe_violations = texts
        .iter()
        .filter_map(|text| {
            let box_ = text.bounds?;
            let outside = distance_outside(box_, title_safe);
            (outside > 0.0).then_some(SafeViolation {
                id: text.id,
                outside_pixels: outside,
            })
        })
        .collect();

    let minimum_area = scene.canvas.area() * rules.offscreen_min_area_fraction;
    let off_screen = scene
        .elements
        .iter()
        .filter(|element| element.kind != ElementKind::Group)
        .filter_map(|element| {
            let box_ = element.bounds?;
            let text = element.kind == ElementKind::Text;
            // A particle or an element mid-wipe has no meaningful size; only
            // text and real content are "off screen".
            if !text && box_.area() < minimum_area {
                return None;
            }
            (box_.intersection_area(scene.canvas) <= 0.0).then_some(OffScreen {
                id: element.id,
                text,
            })
        })
        .collect();

    let mut alignment_groups = Vec::new();
    for edge in ["left", "center", "right"] {
        let mut groups: Vec<(f64, Vec<NodeId>)> = Vec::new();
        for text in &texts {
            let Some(box_) = text.bounds else {
                continue;
            };
            let value = match edge {
                "left" => box_.min_x,
                "center" => box_.center().x,
                _ => box_.max_x,
            };
            if let Some(group) = groups
                .iter_mut()
                .find(|(existing, _)| (value - *existing).abs() <= rules.alignment_tolerance)
            {
                group.1.push(text.id);
            } else {
                groups.push((value, vec![text.id]));
            }
        }
        for (value, nodes) in groups {
            if nodes.len() >= 2 {
                alignment_groups.push(AlignmentGroup {
                    edge,
                    coordinate: value,
                    nodes,
                });
            }
        }
    }

    let mut type_levels: Vec<TypeLevel> = Vec::new();
    for text in &texts {
        let Some(content) = text.as_text() else {
            continue;
        };
        let size = (content.size * 10.0).round() / 10.0;
        if let Some(level) = type_levels
            .iter_mut()
            .find(|level| (level.size - size).abs() < 1e-9)
        {
            level.count += 1;
        } else {
            type_levels.push(TypeLevel { size, count: 1 });
        }
    }
    type_levels.sort_by(|a, b| b.size.total_cmp(&a.size));

    LayoutReport {
        frame: scene.frame,
        canvas: scene.canvas,
        title_safe,
        text_overlaps,
        busy_overlaps,
        safe_violations,
        off_screen,
        alignment_groups,
        type_levels,
    }
}

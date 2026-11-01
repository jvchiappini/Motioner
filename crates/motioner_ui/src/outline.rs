//! Flat scene-outline model built from a document tree.
//!
//! Rows are produced depth-first in document order: roots in paint order,
//! then each group's children in paint order. The order is deterministic for a
//! given document, so the outline is stable across reloads that produce the
//! same program.
//!
//! MDF nodes have no user names yet, so rows are named by kind with a running
//! number ("Circle 3"); text layers use their text. A short `detail` (size or
//! radius) is shown beside the name in a dimmer colour.

use motioner_core::{Document, Frame, NodeContent, NodeId, Transform};

/// What kind of row an outline entry describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    /// A transform-only container.
    Group,
    /// A rectangle.
    Rect,
    /// A circle.
    Circle,
    /// A text line.
    Text,
}

impl NodeKind {
    /// The kind's display name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Group => "Group",
            Self::Rect => "Rect",
            Self::Circle => "Circle",
            Self::Text => "Text",
        }
    }

    fn of(content: &NodeContent) -> Self {
        match content {
            NodeContent::Group => Self::Group,
            NodeContent::Rect { .. } => Self::Rect,
            NodeContent::Circle { .. } => Self::Circle,
            NodeContent::Text(_) => Self::Text,
        }
    }
}

/// One flat outline row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineRow {
    /// Nesting depth; roots are `0`.
    pub depth: u32,
    /// The node this row selects.
    pub node: NodeId,
    /// Display name ("Circle 3", or the text of a text layer).
    pub name: String,
    /// Short secondary description ("r24", "96 × 96").
    pub detail: String,
    /// Kind of node.
    pub kind: NodeKind,
    /// Whether the node has children (groups only).
    pub has_children: bool,
    /// Whether a later row has the same depth before the parent closes, used
    /// to draw indentation guides.
    pub has_next_sibling: bool,
    /// Keyframe frames of the node's transform, sorted and unique.
    pub keyframes: Vec<Frame>,
}

/// Builds the outline rows for `document`.
#[must_use]
pub fn outline_rows(document: &Document) -> Vec<OutlineRow> {
    let mut rows = Vec::with_capacity(document.len());
    let mut counters = [0u32; 4];
    let roots = document.roots();
    for (index, &root) in roots.iter().enumerate() {
        push_rows(
            document,
            root,
            0,
            index + 1 < roots.len(),
            &mut counters,
            &mut rows,
        );
    }
    rows
}

fn push_rows(
    document: &Document,
    id: NodeId,
    depth: u32,
    has_next_sibling: bool,
    counters: &mut [u32; 4],
    rows: &mut Vec<OutlineRow>,
) {
    let Some(node) = document.node(id) else {
        return;
    };
    let kind = NodeKind::of(node.content());
    let counter = &mut counters[kind as usize];
    *counter += 1;
    let (name, detail) = describe(node.content(), *counter);
    let children = node.children();
    rows.push(OutlineRow {
        depth,
        node: id,
        name,
        detail,
        kind,
        has_children: !children.is_empty(),
        has_next_sibling,
        keyframes: keyframe_frames(node.transform()),
    });
    for (index, &child) in children.iter().enumerate() {
        push_rows(
            document,
            child,
            depth + 1,
            index + 1 < children.len(),
            counters,
            rows,
        );
    }
}

fn describe(content: &NodeContent, ordinal: u32) -> (String, String) {
    match content {
        NodeContent::Group => (format!("Group {ordinal}"), String::new()),
        NodeContent::Rect { width, height, .. } => (
            format!("Rect {ordinal}"),
            format!("{} × {}", number(*width), number(*height)),
        ),
        NodeContent::Circle { radius, .. } => {
            (format!("Circle {ordinal}"), format!("r{}", number(*radius)))
        }
        NodeContent::Text(text) => {
            let single_line = text.text.replace('\n', " ");
            let name = if single_line.trim().is_empty() {
                format!("Text {ordinal}")
            } else {
                single_line
            };
            (name, format!("{} px", number(text.size)))
        }
    }
}

/// Every keyframe frame on the node's transform channels, sorted and unique.
#[must_use]
pub fn keyframe_frames(transform: &Transform) -> Vec<Frame> {
    let mut frames: Vec<Frame> = transform
        .position
        .keys()
        .iter()
        .map(|k| k.frame)
        .chain(transform.scale.keys().iter().map(|k| k.frame))
        .chain(transform.anchor.keys().iter().map(|k| k.frame))
        .chain(transform.rotation.keys().iter().map(|k| k.frame))
        .chain(transform.opacity.keys().iter().map(|k| k.frame))
        .collect();
    frames.sort_unstable();
    frames.dedup();
    frames
}

/// Formats a size for display: integers without decimals, others with one.
#[must_use]
pub fn number(value: f64) -> String {
    if value.fract().abs() < 1e-9 {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HELLO: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/hello.motion"
    ));

    fn hello() -> Document {
        motioner_dsl::run(HELLO, &motioner_dsl::Limits::default()).unwrap()
    }

    #[test]
    fn rows_follow_depth_first_document_order() {
        let document = hello();
        let rows = outline_rows(&document);
        assert_eq!(rows.len(), 10);
        assert_eq!(rows[0].node, document.roots()[0]);
        assert_eq!(rows[0].depth, 0);
        assert_eq!(rows[0].kind, NodeKind::Text);
        assert_eq!(rows[0].name, "Motioner");
        assert_eq!(rows[0].detail, "160 px");

        // Every spinner group is followed by its single rect child.
        let groups: Vec<usize> = rows
            .iter()
            .enumerate()
            .filter(|(_, row)| row.kind == NodeKind::Group)
            .map(|(index, _)| index)
            .collect();
        assert_eq!(groups.len(), 2);
        for index in groups {
            assert!(rows[index].has_children);
            assert_eq!(rows[index + 1].depth, rows[index].depth + 1);
            assert_eq!(rows[index + 1].kind, NodeKind::Rect);
            assert_eq!(rows[index + 1].detail, "96 × 96");
            assert!(!rows[index + 1].has_next_sibling, "only child");
        }
        assert_eq!(rows[6].name, "Group 1");
        assert_eq!(rows[7].name, "Rect 1");
        assert_eq!(rows[9].name, "Rect 2");
        assert!(!rows[8].has_next_sibling, "last root");
    }

    #[test]
    fn rows_are_deterministic() {
        let document = hello();
        assert_eq!(outline_rows(&document), outline_rows(&document));
    }

    #[test]
    fn circles_are_numbered_and_carry_the_radius() {
        let document = hello();
        let circles: Vec<_> = outline_rows(&document)
            .into_iter()
            .filter(|row| row.kind == NodeKind::Circle)
            .collect();
        assert_eq!(circles.len(), 5);
        for (index, row) in circles.iter().enumerate() {
            assert_eq!(row.name, format!("Circle {}", index + 1));
            assert_eq!(row.detail, "r24");
        }
    }

    #[test]
    fn keyframes_are_sorted_and_unique() {
        let document = hello();
        let rows = outline_rows(&document);
        // The title keys opacity and position at 0.0 s and 0.8 s (60 fps).
        assert_eq!(rows[0].keyframes, vec![0, 48]);
        // Spinner 1 keys rotation at 1.0 s/3.5 s and opacity at 1.0 s/1.5 s.
        assert_eq!(rows[6].keyframes, vec![60, 90, 210]);
        // The rect inside has no keys of its own.
        assert!(rows[7].keyframes.is_empty());
    }
}

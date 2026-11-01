//! Flat scene-outline model built from a document tree.
//!
//! Rows are produced depth-first in document order: roots in paint order,
//! then each group's children in paint order. The order is deterministic for a
//! given document, so the outline is stable across reloads that produce the
//! same program.

use motioner_core::{Document, NodeContent, NodeId};

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
    /// The short name shown in the outline.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Group => "group",
            Self::Rect => "rect",
            Self::Circle => "circle",
            Self::Text => "text",
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
    /// Short human-readable label.
    pub label: String,
    /// Kind of node.
    pub kind: NodeKind,
}

/// Builds the outline rows for `document`.
#[must_use]
pub fn outline_rows(document: &Document) -> Vec<OutlineRow> {
    let mut rows = Vec::with_capacity(document.len());
    for &root in document.roots() {
        push_rows(document, root, 0, &mut rows);
    }
    rows
}

fn push_rows(document: &Document, id: NodeId, depth: u32, rows: &mut Vec<OutlineRow>) {
    let Some(node) = document.node(id) else {
        return;
    };
    let (label, kind) = describe(node.content());
    rows.push(OutlineRow {
        depth,
        node: id,
        label,
        kind,
    });
    for &child in node.children() {
        push_rows(document, child, depth + 1, rows);
    }
}

fn describe(content: &NodeContent) -> (String, NodeKind) {
    match content {
        NodeContent::Group => ("Group".to_owned(), NodeKind::Group),
        NodeContent::Rect {
            width,
            height,
            radius,
            ..
        } => {
            let mut label = format!("Rect {} x {}", number(*width), number(*height));
            if *radius > 0.0 {
                label.push_str("  r");
                label.push_str(&number(*radius));
            }
            (label, NodeKind::Rect)
        }
        NodeContent::Circle { radius, .. } => {
            (format!("Circle r{}", number(*radius)), NodeKind::Circle)
        }
        NodeContent::Text(text) => {
            let mut preview: String = text.text.replace('\n', " ").chars().take(24).collect();
            if text.text.chars().count() > 24 {
                preview.push('…');
            }
            (format!("Text \"{preview}\""), NodeKind::Text)
        }
    }
}

fn number(value: f64) -> String {
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
        assert!(!rows.is_empty());
        assert_eq!(rows[0].node, document.roots()[0]);
        assert_eq!(rows[0].depth, 0);
        assert_eq!(rows[0].kind, NodeKind::Text);
        assert!(rows[0].label.starts_with("Text \"Motioner\""));

        // Every spinner group is followed by its single rect child.
        let groups: Vec<usize> = rows
            .iter()
            .enumerate()
            .filter(|(_, row)| row.kind == NodeKind::Group)
            .map(|(index, _)| index)
            .collect();
        assert_eq!(groups.len(), 2);
        for index in groups {
            assert_eq!(rows[index + 1].depth, rows[index].depth + 1);
            assert_eq!(rows[index + 1].kind, NodeKind::Rect);
            assert_eq!(rows[index + 1].label, "Rect 96 x 96  r12");
        }
    }

    #[test]
    fn rows_are_deterministic() {
        let document = hello();
        assert_eq!(outline_rows(&document), outline_rows(&document));
    }

    #[test]
    fn circle_labels_carry_the_radius() {
        let document = hello();
        let circles: Vec<_> = outline_rows(&document)
            .into_iter()
            .filter(|row| row.kind == NodeKind::Circle)
            .collect();
        assert_eq!(circles.len(), 5);
        assert!(circles.iter().all(|row| row.label == "Circle r24"));
    }
}

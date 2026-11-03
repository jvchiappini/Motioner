//! Flat scene-outline model built from a document tree.
//!
//! Rows are produced depth-first in document order: roots in paint order,
//! then each group's children in paint order. The order is deterministic for a
//! given document, so the outline is stable across reloads that produce the
//! same program.
//!
//! A row prefers the node's display name (the binding it was created from or
//! `node.name = ...`); otherwise it is named by kind with a running number
//! ("Circle 3") and text layers use their text. A short `detail` (size or
//! radius) is shown beside the name in a dimmer colour.

use motioner_core::{Document, Frame, Node, NodeContent, NodeId, Transform};

/// What kind of row an outline entry describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    /// A transform-only container.
    Group,
    /// A rectangle.
    Rect,
    /// A circle.
    Circle,
    /// A vector path.
    Path,
    /// A text line.
    Text,
    /// A raster image.
    Image,
}

impl NodeKind {
    /// The kind's display name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Group => "Group",
            Self::Rect => "Rect",
            Self::Circle => "Circle",
            Self::Path => "Path",
            Self::Text => "Text",
            Self::Image => "Image",
        }
    }

    fn of(content: &NodeContent) -> Self {
        match content {
            NodeContent::Group => Self::Group,
            NodeContent::Rect { .. } => Self::Rect,
            NodeContent::Circle { .. } => Self::Circle,
            NodeContent::Path { .. } => Self::Path,
            NodeContent::Text(_) => Self::Text,
            NodeContent::Image(_) => Self::Image,
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
    /// The node's stable identity (explicit id or provenance key).
    pub identity: String,
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
    /// The node's mask relationship (D-0033), drawn as a badge.
    pub mask_role: MaskRole,
    /// Whether the layer list and timeline show this row after collapsing and
    /// filtering. Ancestors of a match stay visible.
    pub visible: bool,
    /// Frames where the node and its ancestors are drawn. Computed once per
    /// document generation so scrolling and filtering never re-sample the
    /// timeline.
    pub visible_runs: Vec<crate::timeline_model::VisibleRun>,
}

/// How a row participates in masks, for the layers-panel badge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaskRole {
    /// Neither masked nor a source.
    None,
    /// Carries a mask.
    Masked,
    /// Supplies another node's mask.
    Source,
    /// Both: it is masked and used as a source.
    Both,
}

impl MaskRole {
    /// `true` when the row shows the mask badge.
    #[must_use]
    pub const fn shows_badge(self) -> bool {
        !matches!(self, Self::None)
    }
}

/// Builds the outline rows for `document`.
#[must_use]
pub fn outline_rows(document: &Document) -> Vec<OutlineRow> {
    let mut rows = Vec::with_capacity(document.len());
    let mut counters = [0u32; 6];
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
    counters: &mut [u32; 6],
    rows: &mut Vec<OutlineRow>,
) {
    let Some(node) = document.node(id) else {
        return;
    };
    let kind = NodeKind::of(node.content());
    let counter = &mut counters[kind as usize];
    *counter += 1;
    let (fallback, detail) = describe(node.content(), *counter);
    let name = node.name().map_or(fallback, str::to_owned);
    let children = node.children();
    rows.push(OutlineRow {
        depth,
        node: id,
        name,
        identity: node.identity().as_str().to_owned(),
        detail,
        kind,
        has_children: !children.is_empty(),
        has_next_sibling,
        keyframes: node_keyframes(node),
        mask_role: match (node.mask().is_some(), document.is_mask_source(id)) {
            (true, true) => MaskRole::Both,
            (true, false) => MaskRole::Masked,
            (false, true) => MaskRole::Source,
            (false, false) => MaskRole::None,
        },
        visible: true,
        visible_runs: crate::timeline_model::visible_runs(
            document,
            id,
            document.composition().duration(),
        ),
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
            format!("{} × {}", number(width.value()), number(height.value())),
        ),
        NodeContent::Circle { radius, .. } => (
            format!("Circle {ordinal}"),
            format!("r{}", number(radius.value())),
        ),
        NodeContent::Path { geometry, .. } => (
            format!("Path {ordinal}"),
            format!(
                "{} pts{}",
                geometry.points().len(),
                if geometry.closed() { ", closed" } else { "" }
            ),
        ),
        NodeContent::Text(text) => {
            let single_line = text.text.replace('\n', " ");
            let name = if single_line.trim().is_empty() {
                format!("Text {ordinal}")
            } else {
                single_line
            };
            (name, format!("{} px", number(text.size.value())))
        }
        NodeContent::Image(image) => {
            // The source path can be very long; the layer name is the file
            // stem and the detail is the natural size (D-0035, phase 9).
            let stem = std::path::Path::new(&image.source)
                .file_stem()
                .and_then(std::ffi::OsStr::to_str)
                .filter(|stem| !stem.is_empty())
                .map_or_else(|| format!("Image {ordinal}"), ToOwned::to_owned);
            let detail = image.asset.map_or_else(
                || "unresolved".to_owned(),
                |asset| format!("{} × {}", asset.width, asset.height),
            );
            (stem, detail)
        }
    }
}

/// Marks rows hidden by a collapsed ancestor or by the layer filter.
///
/// `filter` matches the display name, the kind ("Circle") and the identity
/// (`@288`, `title`) case-insensitively. A matching row keeps its whole
/// ancestor chain visible; with no filter every row outside a collapsed
/// subtree is visible.
pub fn apply_visibility(
    rows: &mut [OutlineRow],
    collapsed: &std::collections::BTreeSet<usize>,
    filter: &str,
) {
    let parents = parent_indices(rows);
    let query = filter.trim().to_lowercase();
    let mut keep = vec![true; rows.len()];
    if query.is_empty() {
        for index in 0..rows.len() {
            let mut hidden = false;
            let mut current = parents[index];
            while let Some(parent) = current {
                if collapsed.contains(&parent) {
                    hidden = true;
                    break;
                }
                current = parents[parent];
            }
            keep[index] = !hidden;
        }
    } else {
        let mut matched = vec![false; rows.len()];
        for (index, row) in rows.iter().enumerate() {
            matched[index] = row.name.to_lowercase().contains(&query)
                || row.kind.name().to_lowercase().contains(&query)
                || row.identity.to_lowercase().contains(&query);
        }
        keep.fill(false);
        for (index, is_match) in matched.iter().enumerate() {
            if !is_match {
                continue;
            }
            let mut current = Some(index);
            while let Some(row) = current {
                keep[row] = true;
                current = parents[row];
            }
        }
    }
    for (row, visible) in rows.iter_mut().zip(keep) {
        row.visible = visible;
    }
}

fn parent_indices(rows: &[OutlineRow]) -> Vec<Option<usize>> {
    let mut parents = vec![None; rows.len()];
    let mut stack: Vec<usize> = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        while stack
            .last()
            .is_some_and(|&parent| rows[parent].depth >= row.depth)
        {
            stack.pop();
        }
        parents[index] = stack.last().copied();
        stack.push(index);
    }
    parents
}

/// Every keyframe frame on the node's channels, sorted and unique.
fn node_keyframes(node: &Node) -> Vec<Frame> {
    let mut frames = keyframe_frames(node.transform());
    for (field, _) in motioner_core::ContentField::ALL {
        if let Some(channel) = node.content().channel(field) {
            frames.extend(channel.key_frames());
        }
    }
    frames.sort_unstable();
    frames.dedup();
    frames
}

/// Every keyframe frame on the node's transform channels, sorted and unique.
#[must_use]
pub fn keyframe_frames(transform: &Transform) -> Vec<Frame> {
    let mut frames: Vec<Frame> = transform
        .position
        .key_frames()
        .into_iter()
        .chain(transform.scale.key_frames())
        .chain(transform.anchor.key_frames())
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
        assert_eq!(rows[0].name, "title", "the binding gives the default name");
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
        // `spinner` is called twice, so its local bindings carry the call
        // ordinal (display names disambiguate repeated calls).
        assert_eq!(rows[6].name, "holder 1");
        assert_eq!(rows[7].name, "square 1");
        assert_eq!(rows[8].name, "holder 2");
        assert_eq!(rows[9].name, "square 2");
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
            assert_eq!(row.name, format!("dot {index}"));
            assert_eq!(row.detail, "r24");
        }
    }

    #[test]
    fn a_filter_keeps_matching_rows_and_their_ancestors() {
        let document = hello();
        let mut rows = outline_rows(&document);
        apply_visibility(&mut rows, &std::collections::BTreeSet::new(), "square");
        let visible: Vec<bool> = rows.iter().map(|row| row.visible).collect();
        let first_square = rows
            .iter()
            .position(|row| row.kind == NodeKind::Rect)
            .unwrap();
        assert!(visible[first_square], "the match stays");
        assert!(
            visible[first_square - 1],
            "its group parent stays so the tree still makes sense"
        );
        assert!(!visible[0], "an unrelated root is hidden");
        // The filter matches identities too, not only labels.
        let identity = rows[0].identity.clone();
        apply_visibility(&mut rows, &std::collections::BTreeSet::new(), &identity);
        assert!(rows[0].visible);
    }

    #[test]
    fn collapsing_a_group_hides_its_descendants() {
        let document = hello();
        let mut rows = outline_rows(&document);
        let group = rows.iter().position(|row| row.has_children).unwrap();
        let child = group + 1;
        let mut collapsed = std::collections::BTreeSet::new();
        collapsed.insert(group);
        apply_visibility(&mut rows, &collapsed, "");
        assert!(rows[group].visible);
        assert!(!rows[child].visible);
    }

    #[test]
    fn mask_roles_mark_the_masked_node_and_its_source() {
        let mut document = hello();
        let masked = document.roots()[0];
        let source = document
            .add_node(NodeContent::circle(
                8.0,
                motioner_core::Paint {
                    fill: Some(motioner_core::Color::WHITE),
                    stroke: None,
                },
            ))
            .unwrap();
        document
            .set_mask(masked, source, motioner_core::MaskMode::Alpha, false)
            .unwrap();
        let rows = outline_rows(&document);
        assert_eq!(rows[0].mask_role, MaskRole::Masked);
        assert!(rows[0].mask_role.shows_badge());
        let source_row = rows.iter().find(|row| row.node == source).unwrap();
        assert_eq!(source_row.mask_role, MaskRole::Source);
        assert!(rows[1..rows.len() - 1]
            .iter()
            .all(|row| row.mask_role == MaskRole::None));
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

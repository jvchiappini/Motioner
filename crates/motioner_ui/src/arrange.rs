//! Reorder and reparent decisions for an outline drag.
//!
//! The drawing and the pointer live elsewhere; this module turns "row A was
//! dropped on row B at placement C" into the typed structural [`Command`] the
//! planner understands, so the decision is a pure function that can be tested
//! without a window and reused by the future `GUIMaker` port.

use crate::command::Command;
use crate::outline::{NodeKind, OutlineRow};

/// Where a dragged row lands relative to the row under the pointer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropPlacement {
    /// Above the target row (same parent).
    Above,
    /// Below the target row (same parent).
    Below,
    /// Inside the target row, which must be a group.
    Inside,
}

impl DropPlacement {
    /// The placement for a pointer at `y` inside a row of height `height`.
    ///
    /// The top and bottom quarters reorder; the middle reparents into a group.
    #[must_use]
    pub fn at(y: f32, top: f32, height: f32) -> Self {
        if height <= 0.0 {
            return Self::Inside;
        }
        let fraction = ((y - top) / height).clamp(0.0, 1.0);
        if fraction < 0.25 {
            Self::Above
        } else if fraction > 0.75 {
            Self::Below
        } else {
            Self::Inside
        }
    }
}

/// The command that moves the layer of row `dragged` onto row `target`.
///
/// Returns `None` when the drop changes nothing or is not representable: a row
/// dropped on itself, a group dropped into its own subtree, or an "inside"
/// drop on a layer that is not a group.
#[must_use]
pub fn drop_command(
    rows: &[OutlineRow],
    dragged: usize,
    target: usize,
    placement: DropPlacement,
) -> Option<Command> {
    let from = rows.get(dragged)?;
    let onto = rows.get(target)?;
    if dragged == target || is_descendant(rows, target, dragged) {
        return None;
    }
    let id = from.identity.clone();
    if placement == DropPlacement::Inside {
        if onto.kind != NodeKind::Group {
            return None;
        }
        if parent_row(rows, dragged) == Some(target) {
            return None;
        }
        return Some(Command::ReparentLayer {
            id,
            parent: Some(onto.identity.clone()),
        });
    }
    let Some(parent_index) = parent_row(rows, target) else {
        // A top-level target: reorder among the top-level layers.
        let mut to = root_index(rows, target)?;
        if placement == DropPlacement::Below {
            to += 1;
        }
        if let Some(current) = root_index(rows, dragged) {
            if current < to {
                to -= 1;
            }
            if current == to {
                return None;
            }
        } else {
            // A layer inside a group dropped between root layers: unparent it.
            return Some(Command::ReparentLayer { id, parent: None });
        }
        return Some(Command::ReorderLayer { id, to });
    };
    let parent = rows.get(parent_index)?;
    if parent_row(rows, dragged) == Some(parent_index) {
        return None;
    }
    Some(Command::ReparentLayer {
        id,
        parent: Some(parent.identity.clone()),
    })
}

/// Whether row `candidate` sits inside the subtree rooted at `root`.
fn is_descendant(rows: &[OutlineRow], candidate: usize, root: usize) -> bool {
    let Some(root_row) = rows.get(root) else {
        return false;
    };
    if candidate <= root {
        return false;
    }
    let mut depth = root_row.depth;
    for row in &rows[root + 1..=candidate] {
        if row.depth <= depth {
            return false;
        }
        depth = row.depth;
    }
    true
}

/// The index of `row` among the root (depth 0) rows.
fn root_index(rows: &[OutlineRow], row: usize) -> Option<usize> {
    if rows.get(row)?.depth != 0 {
        return None;
    }
    Some(rows[..row].iter().filter(|row| row.depth == 0).count())
}

/// The index of the group row that directly contains `row`.
fn parent_row(rows: &[OutlineRow], row: usize) -> Option<usize> {
    let depth = rows.get(row)?.depth;
    if depth == 0 {
        return None;
    }
    (0..row).rev().find(|index| rows[*index].depth + 1 == depth)
}

#[cfg(test)]
mod tests {
    use super::{drop_command, DropPlacement};
    use crate::command::Command;
    use crate::outline::{MaskRole, NodeKind, OutlineRow};
    use motioner_core::NodeId;

    fn row(index: u32, depth: u32, kind: NodeKind, identity: &str) -> OutlineRow {
        OutlineRow {
            depth,
            node: NodeId::from_raw(index),
            name: identity.to_owned(),
            identity: identity.to_owned(),
            detail: String::new(),
            kind,
            has_children: matches!(kind, NodeKind::Group),
            has_next_sibling: false,
            keyframes: Vec::new(),
            mask_role: MaskRole::None,
            visible: true,
            visible_runs: Vec::new(),
        }
    }

    fn scene() -> Vec<OutlineRow> {
        vec![
            row(0, 0, NodeKind::Rect, "a"),
            row(1, 0, NodeKind::Group, "g"),
            row(2, 1, NodeKind::Circle, "c1"),
            row(3, 1, NodeKind::Circle, "c2"),
            row(4, 0, NodeKind::Text, "t"),
        ]
    }

    #[test]
    fn dropping_above_or_below_a_root_reorders() {
        let rows = scene();
        assert_eq!(
            drop_command(&rows, 0, 4, DropPlacement::Above),
            Some(Command::ReorderLayer {
                id: "a".to_owned(),
                to: 1,
            })
        );
        assert_eq!(
            drop_command(&rows, 4, 0, DropPlacement::Below),
            Some(Command::ReorderLayer {
                id: "t".to_owned(),
                to: 1,
            })
        );
        assert_eq!(drop_command(&rows, 0, 0, DropPlacement::Above), None);
    }

    #[test]
    fn dropping_onto_a_group_reparents_into_it() {
        let rows = scene();
        assert_eq!(
            drop_command(&rows, 0, 1, DropPlacement::Inside),
            Some(Command::ReparentLayer {
                id: "a".to_owned(),
                parent: Some("g".to_owned()),
            })
        );
        // Dropping a group into its own child is refused.
        assert_eq!(drop_command(&rows, 1, 2, DropPlacement::Inside), None);
        // Dropping next to a child reparents into that child's group.
        assert_eq!(
            drop_command(&rows, 0, 3, DropPlacement::Below),
            Some(Command::ReparentLayer {
                id: "a".to_owned(),
                parent: Some("g".to_owned()),
            })
        );
        // Already a direct child: nothing changes.
        assert_eq!(drop_command(&rows, 2, 3, DropPlacement::Below), None);
    }

    #[test]
    fn a_non_group_cannot_contain_a_drop() {
        let rows = scene();
        assert_eq!(drop_command(&rows, 0, 4, DropPlacement::Inside), None);
    }

    #[test]
    fn a_pointer_in_the_middle_means_inside() {
        assert_eq!(DropPlacement::at(2.0, 0.0, 24.0), DropPlacement::Above);
        assert_eq!(DropPlacement::at(12.0, 0.0, 24.0), DropPlacement::Inside);
        assert_eq!(DropPlacement::at(23.0, 0.0, 24.0), DropPlacement::Below);
    }
}

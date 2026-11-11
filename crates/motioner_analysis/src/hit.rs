//! `hit_test`: which elements lie under a point, topmost first.

use motioner_core::{NodeId, Vec2};

use crate::element::{ElementKind, SceneFrame};

/// A hit with its element and paint order.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hit {
    /// The node under the point.
    pub id: NodeId,
    /// Paint order (higher is on top).
    pub z: usize,
    /// The element kind.
    pub kind: ElementKind,
}

/// Every drawn element whose box contains `point`, topmost first.
///
/// Groups are not returned: they draw nothing, so a pointer never lands on
/// one. A point on the edge of a box hits it.
#[must_use]
pub fn hit_test(scene: &SceneFrame, point: Vec2) -> Vec<Hit> {
    let mut hits: Vec<Hit> = scene
        .elements
        .iter()
        .filter(|element| element.kind != ElementKind::Group)
        .filter(|element| element.bounds.is_some_and(|bounds| bounds.contains(point)))
        .map(|element| Hit {
            id: element.id,
            z: element.z,
            kind: element.kind,
        })
        .collect();
    hits.sort_by_key(|hit| std::cmp::Reverse(hit.z));
    hits
}

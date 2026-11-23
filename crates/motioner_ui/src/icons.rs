//! The shared vector icon set: re-exported from `GUIMaker`.
//!
//! Icons are designed on a 16-unit grid and filled as SVG paths, so they stay
//! sharp at any DPI. The definitions live in `guimaker::icons`; this module
//! only maps them onto the editor's painter and adds the outline-kind mapping
//! that is specific to Motioner. There is no second copy of the geometry.

pub use guimaker::icons::Icon;

use crate::layout::Rect;
use crate::outline::NodeKind;
use crate::paint::Painter;

/// Draws `icon` centred in `rect` in `color`.
///
/// `surface` is accepted for call-site compatibility; the shared icon paths
/// are self-contained, so no cut-out colour is needed.
pub fn draw(p: &mut Painter<'_>, icon: Icon, rect: Rect, color: [f32; 4], _surface: [f32; 4]) {
    let side = rect.w.min(rect.h);
    let box_rect = Rect::new(
        rect.center_x() - side * 0.5,
        rect.center_y() - side * 0.5,
        side,
        side,
    );
    p.svg_path(icon.path(), box_rect, color);
}

/// The icon for an outline node kind.
#[must_use]
pub fn for_kind(kind: NodeKind) -> Icon {
    match kind {
        NodeKind::Group => Icon::Group,
        NodeKind::Rect => Icon::Rect,
        NodeKind::Circle => Icon::Circle,
        NodeKind::Path => Icon::Path,
        NodeKind::Text => Icon::Text,
        NodeKind::Image => Icon::Image,
        NodeKind::Audio => Icon::Audio,
        NodeKind::Video => Icon::Video,
    }
}

#[cfg(test)]
mod tests {
    use super::{for_kind, Icon};
    use crate::outline::NodeKind;

    #[test]
    fn every_shared_icon_has_a_path() {
        for icon in Icon::ALL {
            assert!(!icon.path().is_empty(), "{icon:?} has no path");
        }
    }

    #[test]
    fn outline_kinds_map_to_real_icons() {
        for kind in [
            NodeKind::Group,
            NodeKind::Rect,
            NodeKind::Circle,
            NodeKind::Path,
            NodeKind::Text,
            NodeKind::Image,
            NodeKind::Audio,
            NodeKind::Video,
        ] {
            assert!(Icon::ALL.contains(&for_kind(kind)));
        }
    }
}

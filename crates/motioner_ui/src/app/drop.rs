//! Routing of operating-system file drops onto the editor surfaces.
//!
//! The engine reports a drop as a path and a kind, never a screen position, so
//! the window samples the pointer when the event arrives. This module keeps the
//! mapping from that point to a surface pure and testable: the target decides
//! whether the file is only imported or also placed as a layer.

use crate::layout::Layout;

/// Where a dragged or dropped file lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropTarget {
    /// The window chrome: import only.
    Window,
    /// The left panel: import into the project's asset registry.
    Assets,
    /// The composition canvas: import and place a layer at the drop point.
    Canvas,
}

impl DropTarget {
    /// Whether a dropped file should also become a layer.
    #[must_use]
    pub const fn places(self) -> bool {
        matches!(self, Self::Canvas)
    }
}

/// The surface under `(x, y)` while a file is over the window.
#[must_use]
pub fn target_at(layout: &Layout, x: f32, y: f32) -> Option<DropTarget> {
    if !layout.window.contains(x, y) {
        return None;
    }
    if layout.layers.rect.contains(x, y) {
        return Some(DropTarget::Assets);
    }
    if layout.canvas.contains(x, y) {
        return Some(DropTarget::Canvas);
    }
    Some(DropTarget::Window)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::PanelSizes;

    #[test]
    fn a_drop_on_the_canvas_places_a_layer_and_anywhere_else_only_imports() {
        let layout = Layout::compute(1600.0, 900.0, 1.0, PanelSizes::default());
        let canvas = layout.canvas;
        assert_eq!(
            target_at(&layout, canvas.center_x(), canvas.center_y()),
            Some(DropTarget::Canvas)
        );
        let panel = layout.layers.rect;
        assert_eq!(
            target_at(&layout, panel.center_x(), panel.center_y()),
            Some(DropTarget::Assets)
        );
        let top = layout.top_bar;
        assert_eq!(
            target_at(&layout, top.center_x(), top.center_y()),
            Some(DropTarget::Window)
        );
        assert_eq!(target_at(&layout, -1.0, 10.0), None);
        assert!(DropTarget::Canvas.places());
        assert!(!DropTarget::Assets.places());
        assert!(!DropTarget::Window.places());
    }
}

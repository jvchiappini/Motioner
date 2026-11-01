//! Canvas gizmos: hit testing, dragging, scaling, rotating and snapping.
//!
//! Everything here is pure geometry in **composition pixels** (the canvas
//! letterboxes the composition; the app converts window pixels first). The
//! caller turns the resulting values into override commands.
//!
//! The selection is drawn as an axis-aligned box around the layer's bounds
//! with four corner handles, plus a rotation handle above the top edge. A
//! rotated layer therefore gets an axis-aligned frame (After Effects shows an
//! oriented frame; the model here keeps the maths exact and simple).

use motioner_core::Vec2;

use crate::bounds::Bounds;
use crate::layout::Rect;

/// A grab point on the selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handle {
    /// The body of the selection (drag moves).
    Move,
    /// Top-left corner (scale).
    TopLeft,
    /// Top-right corner (scale).
    TopRight,
    /// Bottom-right corner (scale).
    BottomRight,
    /// Bottom-left corner (scale).
    BottomLeft,
    /// The rotation handle above the top edge.
    Rotate,
}

/// Which corner a scale drag started from, and therefore which one stays put.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Corner {
    /// Top-left.
    TopLeft,
    /// Top-right.
    TopRight,
    /// Bottom-right.
    BottomRight,
    /// Bottom-left.
    BottomLeft,
}

impl Handle {
    /// The cursor this handle uses.
    #[must_use]
    pub fn cursor(self) -> GizmoCursor {
        match self {
            Self::Move => GizmoCursor::Move,
            Self::Rotate => GizmoCursor::Grab,
            Self::TopLeft | Self::BottomRight => GizmoCursor::NwseResize,
            Self::TopRight | Self::BottomLeft => GizmoCursor::NeswResize,
        }
    }
}

/// A cursor shape a handle asks for (mapped to the engine cursor by the app).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GizmoCursor {
    /// Four-way move.
    Move,
    /// Grab/rotate.
    Grab,
    /// Diagonal (top-left to bottom-right).
    NwseResize,
    /// Diagonal (top-right to bottom-left).
    NeswResize,
}

/// Screen-size of a corner handle, in composition pixels at the current zoom.
#[must_use]
pub fn handle_size(zoom: f64) -> f64 {
    (7.0 / zoom.max(1e-6)).max(1.0)
}

/// The rotation handle's distance above the top edge, in composition pixels.
#[must_use]
pub fn rotate_offset(zoom: f64) -> f64 {
    (22.0 / zoom.max(1e-6)).max(4.0)
}

/// The rectangle of one handle in composition pixels.
#[must_use]
pub fn handle_rect(bounds: Bounds, handle: Handle, zoom: f64) -> Option<Rect> {
    let side = handle_size(zoom) as f32;
    let point = match handle {
        Handle::Move => return None,
        Handle::Rotate => Vec2::new(
            (bounds.min_x + bounds.max_x) * 0.5,
            bounds.min_y - rotate_offset(zoom),
        ),
        Handle::TopLeft => Vec2::new(bounds.min_x, bounds.min_y),
        Handle::TopRight => Vec2::new(bounds.max_x, bounds.min_y),
        Handle::BottomRight => Vec2::new(bounds.max_x, bounds.max_y),
        Handle::BottomLeft => Vec2::new(bounds.min_x, bounds.max_y),
    };
    Some(Rect::new(
        (point.x as f32) - side * 0.5,
        (point.y as f32) - side * 0.5,
        side,
        side,
    ))
}

/// The handle under `point` (composition pixels), rotation first.
#[must_use]
pub fn hit_handle(bounds: Bounds, point: Vec2, zoom: f64) -> Option<Handle> {
    for handle in [
        Handle::Rotate,
        Handle::TopLeft,
        Handle::TopRight,
        Handle::BottomRight,
        Handle::BottomLeft,
    ] {
        if let Some(rect) = handle_rect(bounds, handle, zoom) {
            let reach = f64::from(rect.w) * 0.5;
            let cx = f64::from(rect.x) + reach;
            let cy = f64::from(rect.y) + reach;
            if (point.x - cx).abs() <= reach && (point.y - cy).abs() <= reach {
                return Some(handle);
            }
        }
    }
    let inside = point.x >= bounds.min_x
        && point.x <= bounds.max_x
        && point.y >= bounds.min_y
        && point.y <= bounds.max_y;
    inside.then_some(Handle::Move)
}

/// Moves `start` by `delta`, optionally locked to one axis.
#[must_use]
pub fn move_position(start: Vec2, delta: Vec2, lock_axis: bool) -> Vec2 {
    if !lock_axis {
        return Vec2::new(start.x + delta.x, start.y + delta.y);
    }
    if delta.x.abs() >= delta.y.abs() {
        Vec2::new(start.x + delta.x, start.y)
    } else {
        Vec2::new(start.x, start.y + delta.y)
    }
}

/// The scale factors for a corner drag.
///
/// `pointer` and `origin` are composition-space points; `bounds` is the
/// selection box at the start of the drag and `anchor` is the point that stays
/// fixed (the opposite corner, or the node's anchor when `from_anchor`).
/// `keep_aspect` uses the larger factor on both axes.
#[must_use]
pub fn scale_factors(
    bounds: Bounds,
    corner: Corner,
    pointer: Vec2,
    anchor: Vec2,
    keep_aspect: bool,
) -> Vec2 {
    let width = (bounds.max_x - bounds.min_x).max(1e-6);
    let height = (bounds.max_y - bounds.min_y).max(1e-6);
    let start = match corner {
        Corner::TopLeft => Vec2::new(bounds.min_x, bounds.min_y),
        Corner::TopRight => Vec2::new(bounds.max_x, bounds.min_y),
        Corner::BottomRight => Vec2::new(bounds.max_x, bounds.max_y),
        Corner::BottomLeft => Vec2::new(bounds.min_x, bounds.max_y),
    };
    // Scaling about the anchor: the corner is the anchor plus the scaled
    // vector from the anchor to the dragged corner.
    let base = Vec2::new(start.x - anchor.x, start.y - anchor.y);
    let sx = if base.x.abs() < 1e-9 {
        1.0
    } else {
        (pointer.x - anchor.x) / base.x
    };
    let sy = if base.y.abs() < 1e-9 {
        1.0
    } else {
        (pointer.y - anchor.y) / base.y
    };
    let _ = (width, height);
    if keep_aspect {
        let s = if sx.abs() >= sy.abs() { sx } else { sy };
        Vec2::new(s, s)
    } else {
        Vec2::new(sx, sy)
    }
}

/// The point that stays fixed for a corner drag.
#[must_use]
pub fn scale_anchor(bounds: Bounds, corner: Corner, from_anchor: Option<Vec2>) -> Vec2 {
    if let Some(anchor) = from_anchor {
        return anchor;
    }
    match corner {
        Corner::TopLeft => Vec2::new(bounds.max_x, bounds.max_y),
        Corner::TopRight => Vec2::new(bounds.min_x, bounds.max_y),
        Corner::BottomRight => Vec2::new(bounds.min_x, bounds.min_y),
        Corner::BottomLeft => Vec2::new(bounds.max_x, bounds.min_y),
    }
}

/// The rotation in degrees of `pointer` around `center`, measured like the
/// model's rotation (clockwise, y down).
#[must_use]
pub fn angle_of(center: Vec2, pointer: Vec2) -> f64 {
    (pointer.y - center.y)
        .atan2(pointer.x - center.x)
        .to_degrees()
}

/// Rotation delta of a drag, snapped to 15° steps when `snap`.
#[must_use]
pub fn rotation_delta(start_angle: f64, pointer_angle: f64, snap: bool) -> f64 {
    let delta = pointer_angle - start_angle;
    if snap {
        (delta / 15.0).round() * 15.0
    } else {
        delta
    }
}

/// A guide line shown while snapping.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Guide {
    /// Vertical guide x, or horizontal guide y.
    pub at: f64,
    /// Whether the guide is vertical.
    pub vertical: bool,
}

/// Snaps a moving selection's box to the composition edges/centre and to
/// other boxes.
///
/// Returns the corrected delta and the guides that produced it. The threshold
/// is in composition pixels; the caller scales it by the zoom.
#[must_use]
pub fn snap_delta(
    moving: Bounds,
    delta: Vec2,
    composition: (f64, f64),
    others: &[Bounds],
    threshold: f64,
) -> (Vec2, Vec<Guide>) {
    let mut guides = Vec::new();
    let mut dx = delta.x;
    let mut dy = delta.y;
    // Candidate x lines: composition edges and centre, then other boxes.
    let mut xs = vec![0.0, composition.0 * 0.5, composition.0];
    let mut ys = vec![0.0, composition.1 * 0.5, composition.1];
    for other in others {
        xs.extend([other.min_x, (other.min_x + other.max_x) * 0.5, other.max_x]);
        ys.extend([other.min_y, (other.min_y + other.max_y) * 0.5, other.max_y]);
    }
    let moved_x = [
        moving.min_x + delta.x,
        (moving.min_x + moving.max_x) * 0.5 + delta.x,
        moving.max_x + delta.x,
    ];
    let moved_y = [
        moving.min_y + delta.y,
        (moving.min_y + moving.max_y) * 0.5 + delta.y,
        moving.max_y + delta.y,
    ];
    if let Some((best, line)) = closest(&xs, &moved_x, threshold) {
        dx += best;
        guides.push(Guide {
            at: line,
            vertical: true,
        });
    }
    if let Some((best, line)) = closest(&ys, &moved_y, threshold) {
        dy += best;
        guides.push(Guide {
            at: line,
            vertical: false,
        });
    }
    (Vec2::new(dx, dy), guides)
}

/// The smallest correction in `-threshold..=threshold` that puts one of
/// `sources` on one of `lines`, with the line it hit.
fn closest(lines: &[f64], sources: &[f64], threshold: f64) -> Option<(f64, f64)> {
    let mut best: Option<(f64, f64)> = None;
    for &line in lines {
        for &source in sources {
            let correction = line - source;
            if correction.abs() <= threshold
                && best.is_none_or(|(current, _)| correction.abs() < current.abs())
            {
                best = Some((correction, line));
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds() -> Bounds {
        Bounds {
            min_x: 100.0,
            min_y: 200.0,
            max_x: 300.0,
            max_y: 260.0,
        }
    }

    #[test]
    fn handles_hit_their_corners_and_the_rotation_grip() {
        let b = bounds();
        assert_eq!(
            hit_handle(b, Vec2::new(100.0, 200.0), 1.0),
            Some(Handle::TopLeft)
        );
        assert_eq!(
            hit_handle(b, Vec2::new(300.0, 260.0), 1.0),
            Some(Handle::BottomRight)
        );
        assert_eq!(
            hit_handle(b, Vec2::new(200.0, 200.0 - rotate_offset(1.0)), 1.0),
            Some(Handle::Rotate)
        );
        assert_eq!(
            hit_handle(b, Vec2::new(200.0, 230.0), 1.0),
            Some(Handle::Move)
        );
        assert_eq!(hit_handle(b, Vec2::new(0.0, 0.0), 1.0), None);
    }

    #[test]
    fn move_locks_to_the_dominant_axis() {
        assert_eq!(
            move_position(Vec2::new(1.0, 2.0), Vec2::new(10.0, 3.0), false),
            Vec2::new(11.0, 5.0)
        );
        assert_eq!(
            move_position(Vec2::new(1.0, 2.0), Vec2::new(10.0, 3.0), true),
            Vec2::new(11.0, 2.0)
        );
        assert_eq!(
            move_position(Vec2::new(1.0, 2.0), Vec2::new(3.0, 9.0), true),
            Vec2::new(1.0, 11.0)
        );
    }

    #[test]
    fn scaling_from_the_opposite_corner_keeps_it_fixed() {
        let b = bounds();
        let anchor = scale_anchor(b, Corner::BottomRight, None);
        assert_eq!(anchor, Vec2::new(100.0, 200.0));
        // Drag the bottom-right corner to twice the size.
        let pointer = Vec2::new(500.0, 320.0);
        let factors = scale_factors(b, Corner::BottomRight, pointer, anchor, false);
        assert!((factors.x - 2.0).abs() < 1e-9, "{factors:?}");
        assert!((factors.y - 2.0).abs() < 1e-9, "{factors:?}");
        // Keep-aspect uses the larger axis.
        let stretched = scale_factors(
            b,
            Corner::BottomRight,
            Vec2::new(500.0, 230.0),
            anchor,
            false,
        );
        let locked = scale_factors(
            b,
            Corner::BottomRight,
            Vec2::new(500.0, 230.0),
            anchor,
            true,
        );
        assert!((locked.x - locked.y).abs() < 1e-9);
        assert!((locked.x - stretched.x.max(stretched.y)).abs() < 1e-9);
    }

    #[test]
    fn rotation_delta_snaps_to_fifteen_degrees() {
        assert!((rotation_delta(0.0, 44.0, false) - 44.0).abs() < 1e-9);
        assert!((rotation_delta(0.0, 44.0, true) - 45.0).abs() < 1e-9);
        assert!((rotation_delta(0.0, 7.0, true)).abs() < 1e-9);
        assert!((rotation_delta(90.0, 10.0, false) + 80.0).abs() < 1e-9);
    }

    #[test]
    fn snapping_lands_on_the_composition_centre_and_guides() {
        let moving = Bounds {
            min_x: 950.0,
            min_y: 500.0,
            max_x: 970.0,
            max_y: 520.0,
        };
        // The box's centre is at 960; a small drag must snap back to it.
        let (delta, guides) = snap_delta(moving, Vec2::new(2.0, 0.0), (1920.0, 1080.0), &[], 8.0);
        assert!((delta.x - 0.0).abs() < 1e-9, "{delta:?}");
        assert!(guides
            .iter()
            .any(|g| g.vertical && (g.at - 960.0).abs() < 1e-9));
        // A large drag is untouched.
        let (delta, guides) = snap_delta(moving, Vec2::new(200.0, 0.0), (1920.0, 1080.0), &[], 8.0);
        assert!((delta.x - 200.0).abs() < 1e-9);
        assert!(guides.is_empty());
    }

    #[test]
    fn snapping_uses_other_layers_edges() {
        let moving = Bounds {
            min_x: 10.0,
            min_y: 10.0,
            max_x: 30.0,
            max_y: 30.0,
        };
        let other = Bounds {
            min_x: 100.0,
            min_y: 100.0,
            max_x: 200.0,
            max_y: 200.0,
        };
        // Moving the left edge near the other's left edge snaps onto it.
        let (delta, guides) = snap_delta(
            moving,
            Vec2::new(88.0, 0.0),
            (1920.0, 1080.0),
            &[other],
            4.0,
        );
        assert!((delta.x - 90.0).abs() < 1e-9, "{delta:?}");
        assert!(guides
            .iter()
            .any(|g| g.vertical && (g.at - 100.0).abs() < 1e-9));
    }
}

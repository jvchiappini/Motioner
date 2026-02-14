//! Animation runtime / DSL helpers — central place to add new animation types.
use crate::scene::Shape;

/// Public interface used by the UI/renderer to resolve an element's animated position.
/// This replaces the previous `animated_xy_for` implementation that lived in `canvas.rs`.
pub fn animated_xy_for(shape: &Shape, project_time: f32, _project_duration: f32) -> (f32, f32) {
    match shape {
        crate::scene::Shape::Circle {
            x, y, animations, ..
        }
        | crate::scene::Shape::Rect {
            x, y, animations, ..
        } => {
            // prefer the last animation that matches this time (same behaviour as before)
            // Prefer the last animation that matches this time. Use MoveAnimation's
            // sampling implementation to avoid duplicating interpolation logic.
            for a in animations.iter().rev() {
                if let Some(ma) = crate::animations::move_animation::MoveAnimation::from_scene(a) {
                    let (ix, iy) = ma.sample_position(*x, *y, project_time);
                    return (ix, iy);
                }
            }
            (*x, *y)
        }
        _ => (0.0, 0.0),
    }
}

/// Convert a serialized `scene::Animation` into a DSL snippet for an element.
/// Returns None for animation kinds that we don't know how to render yet.
pub fn animation_to_dsl(
    anim: &crate::scene::Animation,
    element_name: &str,
    indent: &str,
) -> Option<String> {
    match anim {
        crate::scene::Animation::Move {
            to_x,
            to_y,
            start,
            end,
            easing: _,
        } => {
            // Reuse MoveAnimation's formatting for consistency
            if let Some(ma) = crate::animations::move_animation::MoveAnimation::from_scene(anim) {
                Some(ma.to_dsl_snippet(element_name, indent))
            } else {
                None
            }
        }
    }
}

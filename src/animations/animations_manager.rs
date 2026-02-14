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
            for a in animations.iter().rev() {
                if let crate::scene::Animation::Move {
                    to_x,
                    to_y,
                    start,
                    end,
                    ..
                } = a
                {
                    if project_time >= *start && project_time <= *end {
                        // interpolate from base to target
                        let local_t = if (*end - *start).abs() < std::f32::EPSILON {
                            1.0
                        } else {
                            (project_time - *start) / (*end - *start)
                        };
                        let ix = *x + local_t * (to_x - *x);
                        let iy = *y + local_t * (to_y - *y);
                        return (ix, iy);
                    } else if project_time > *end {
                        return (*to_x, *to_y);
                    } else if project_time < *start {
                        return (*x, *y);
                    }
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

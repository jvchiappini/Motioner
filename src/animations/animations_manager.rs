//! Animation runtime / DSL helpers — central place to add new animation types.
use crate::scene::Shape;
use serde::{Deserialize, Serialize};

/// Animation model (moved here from `scene.rs`). Re-exported from `crate::scene` for
/// backward compatibility.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Animation {
    Move {
        to_x: f32,
        to_y: f32,
        /// start time in seconds
        start: f32,
        /// end time in seconds
        end: f32,
        easing: crate::animations::Easing,
    },
}

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

// Conversion `Animation -> DSL` is now handled by each shape module
// (e.g. `circle::to_dsl_with_animations`). The centralized helper was removed
// to keep responsibilities local to each shape.

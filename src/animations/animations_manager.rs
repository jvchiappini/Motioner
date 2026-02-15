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
            // Compute position by applying move animations in chronological order so
            // multiple sequential animations chain correctly. For each Move animation:
            // - if project_time < start: stop (we haven't reached this animation yet)
            // - if project_time >= end: advance current position to the animation's target
            // - if start <= project_time < end: interpolate from the *current* position
            //   at animation start toward the animation target and return.
            let mut curr_x = *x;
            let mut curr_y = *y;

            // collect Move animations and sort by start time to be robust
            let mut moves: Vec<crate::animations::move_animation::MoveAnimation> = animations
                .iter()
                .filter_map(|a| crate::animations::move_animation::MoveAnimation::from_scene(a))
                .collect();
            moves.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap_or(std::cmp::Ordering::Equal));

            for ma in moves.iter() {
                if project_time < ma.start {
                    // haven't reached this animation yet — keep current position
                    break;
                }

                if project_time >= ma.end {
                    // animation finished — commit its target as the new current position
                    curr_x = ma.to_x;
                    curr_y = ma.to_y;
                    continue;
                }

                // project_time is within this animation — interpolate from curr_{x,y}
                let (ix, iy) = ma.sample_position(curr_x, curr_y, project_time);
                return (ix, iy);
            }

            (curr_x, curr_y)
        }
        _ => (0.0, 0.0),
    }
}

// Conversion `Animation -> DSL` is now handled by each shape module
// (e.g. `circle::to_dsl_with_animations`). The centralized helper was removed
// to keep responsibilities local to each shape.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chained_move_animations_chain_correctly() {
        use crate::scene::Shape;

        // Build a circle that moves in four sequential steps (clockwise square)
        let mut animations = Vec::new();
        animations.push(crate::scene::Animation::Move { to_x: 1.0, to_y: 0.0, start: 0.0, end: 0.1, easing: crate::scene::Easing::Linear });
        animations.push(crate::scene::Animation::Move { to_x: 1.0, to_y: 1.0, start: 0.1, end: 0.2, easing: crate::scene::Easing::Linear });
        animations.push(crate::scene::Animation::Move { to_x: 0.0, to_y: 1.0, start: 0.2, end: 0.3, easing: crate::scene::Easing::Linear });
        animations.push(crate::scene::Animation::Move { to_x: 0.0, to_y: 0.0, start: 0.3, end: 0.4, easing: crate::scene::Easing::Linear });

        let shape = Shape::Circle {
            name: "C".to_string(),
            x: 0.0,
            y: 0.0,
            radius: 0.1,
            color: [0, 0, 0, 255],
            spawn_time: 0.0,
            animations,
            visible: true,
        };

        // mid-first-segment
        let (x1, y1) = animated_xy_for(&shape, 0.05, 1.0);
        assert!((x1 - 0.5).abs() < 1e-3 && (y1 - 0.0).abs() < 1e-3);

        // mid-second-segment
        let (x2, y2) = animated_xy_for(&shape, 0.15, 1.0);
        assert!((x2 - 1.0).abs() < 1e-3 && (y2 - 0.5).abs() < 1e-3);

        // mid-third-segment
        let (x3, y3) = animated_xy_for(&shape, 0.25, 1.0);
        assert!((x3 - 0.5).abs() < 1e-3 && (y3 - 1.0).abs() < 1e-3);

        // after all animations complete -> back to origin
        let (x4, y4) = animated_xy_for(&shape, 0.45, 1.0);
        assert!((x4 - 0.0).abs() < 1e-3 && (y4 - 0.0).abs() < 1e-3);
    }
}

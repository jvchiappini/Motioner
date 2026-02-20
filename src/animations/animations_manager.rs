//! Animation runtime / DSL helpers — central place to add new animation types.
use crate::scene::Shape;
use serde::{Deserialize, Serialize};

/// Animation model (moved here from `scene.rs`). Re-exported from `crate::scene` for
/// backward compatibility.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
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
    // Use Shape helpers so this function doesn't need to pattern-match on
    // every Shape variant — keeps the animation runtime shape-agnostic.
    let (mut curr_x, mut curr_y) = shape.xy();
    let animations = shape.animations();

    // Compute position by applying move animations in chronological order so
    // multiple sequential animations chain correctly. For each Move animation:
    // - if project_time < start: stop (we haven't reached this animation yet)
    // - if project_time >= end: advance current position to the animation's target
    // - if start <= project_time < end: interpolate from the *current* position
    //   at animation start toward the animation target and return.

    // collect Move animations and sort by start time to be robust
    let mut moves: Vec<crate::animations::move_animation::MoveAnimation> = animations
        .iter()
        .filter_map(crate::animations::move_animation::MoveAnimation::from_scene)
        .collect();
    moves.sort_by(|a, b| {
        a.start
            .partial_cmp(&b.start)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

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

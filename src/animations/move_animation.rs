// The `move_animation` module previously contained CPU interpolation logic
// for element movement.  After the GPU rewrite the real-time animation path
// is executed entirely on the graphics card; high‑level `MoveAnimation`
// objects are recorded with scene/keyframe data and converted to GPU commands
// during dispatch.  All deprecated CPU helpers have now been removed — only
// the GPU conversion helper and small evaluation utility remain for testing.
use crate::scene::Easing;
use serde::{Deserialize, Serialize};

// only keep easing modules that are referenced by `to_dsl_block` and
// other code paths.  all other legacy helpers were removed along with
// their files above, so the corresponding `mod` declarations are gone.
pub mod bounce;
pub mod elastic;
pub mod spring;

// ─── MoveAnimation ───────────────────────────────────────────────────────────

/// Concrete implementation for a linear "move" animation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MoveAnimation {
    pub to_x: f32,
    pub to_y: f32,
    pub start: f32,
    pub end: f32,
    pub easing: Easing,
}

impl MoveAnimation {
    /// Create from scene::Animation::Move (returns None for other variants).
    pub fn from_scene(anim: &crate::scene::Animation) -> Option<Self> {
        // currently `Animation` only has the `Move` variant; keep the
        // `Option` return type for future extensibility.  Clippy warns that
        // the `if let` is irrefutable, so silence that specific lint here.
        #[allow(irrefutable_let_patterns)]
        if let crate::scene::Animation::Move {
            to_x,
            to_y,
            start,
            end,
            easing,
        } = anim
        {
            Some(MoveAnimation {
                to_x: *to_x,
                to_y: *to_y,
                start: *start,
                end: *end,
                easing: easing.clone(),
            })
        } else {
            None
        }
    }
}

/// Helper used by CPU paths (`sample_position`, element_store, etc.) to
/// convert a normalized time `t` (0..1) into progress according to the
/// supplied easing.  Most curves fall back to linear because the GPU pipeline
/// now executes the real easing maths; keeping this helper avoids needing all
/// those `compute_progress` functions in individual modules.
pub fn evaluate_easing_cpu(t: f32, easing: &Easing) -> f32 {
    let t = t.clamp(0.0, 1.0);
    match easing {
        Easing::Linear => t,
        Easing::EaseIn { power } => t.powf(*power),
        Easing::EaseOut { power } => 1.0 - (1.0 - t).powf(*power),
        Easing::EaseInOut { power } => {
            if t < 0.5 {
                0.5 * (2.0 * t).powf(*power)
            } else {
                1.0 - 0.5 * (2.0 * (1.0 - t)).powf(*power)
            }
        }
        Easing::Step => {
            // teleport: return 0 until strictly after start, then 1
            if t <= 0.0 {
                0.0
            } else {
                1.0
            }
        }
        // all other easing kinds currently map to linear in the CPU fallback
        _ => t,
    }
}

// --- legacy parsing helpers ------------------------------------------------
// legacy parsing helpers removed: parsing is now handled by dsl::parser

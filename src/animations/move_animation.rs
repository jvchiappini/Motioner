#![allow(dead_code)]
// The `move_animation` module previously contained CPU interpolation logic
// for element movement.  After the GPU rewrite the real-time animation path
// is executed entirely on the graphics card; high‑level `MoveAnimation`
// objects are recorded with scene/keyframe data and converted to GPU commands
// during dispatch.  All deprecated CPU helpers have now been removed — only
// the GPU conversion helper and small evaluation utility remain for testing.
use crate::scene::Easing;
use serde::{Deserialize, Serialize};

pub mod bezier;
pub mod bounce;
pub mod circ;
pub mod custom;
pub mod custom_bezier;
pub mod ease_in;
pub mod ease_in_out;
pub mod ease_out;
pub mod elastic;
pub mod expo;
pub mod linear;
pub mod sine;
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
        let crate::scene::Animation::Move {
            to_x,
            to_y,
            start,
            end,
            easing,
        } = anim;

        Some(MoveAnimation {
            to_x: *to_x,
            to_y: *to_y,
            start: *start,
            end: *end,
            easing: easing.clone(),
        })
    }


    pub fn to_dsl_block(&self, element_name: Option<&str>, indent: &str) -> String {
        let ease_str = match &self.easing {
            Easing::Linear => "linear".to_string(),
            Easing::EaseIn { power } => format!("ease_in(power = {:.3})", power),
            Easing::EaseOut { power } => format!("ease_out(power = {:.3})", power),
            Easing::EaseInOut { power } => format!("ease_in_out(power = {:.3})", power),
            Easing::Custom { points } => {
                let pts: Vec<String> = points
                    .iter()
                    .map(|(t, v)| format!("({:.3}, {:.3})", t, v))
                    .collect();
                format!("custom(points = [{}])", pts.join(", "))
            }
            Easing::CustomBezier { points } => {
                let pts: Vec<String> = points
                    .iter()
                    .map(|p| {
                        format!(
                            "(({:.3}, {:.3}), ({:.3}, {:.3}), ({:.3}, {:.3}))",
                            p.pos.0,
                            p.pos.1,
                            p.handle_left.0,
                            p.handle_left.1,
                            p.handle_right.0,
                            p.handle_right.1
                        )
                    })
                    .collect();
                format!("custom_bezier(points = [{}])", pts.join(", "))
            }
            Easing::Bezier { p1, p2 } => format!(
                "bezier(p1 = ({:.3}, {:.3}), p2 = ({:.3}, {:.3}))",
                p1.0, p1.1, p2.0, p2.1
            ),
            Easing::Sine => "sine".to_string(),
            Easing::Expo => "expo".to_string(),
            Easing::Circ => "circ".to_string(),
            Easing::Spring {
                damping,
                stiffness,
                mass,
            } => spring::to_dsl_string(*damping, *stiffness, *mass),
            Easing::Elastic { amplitude, period } => elastic::to_dsl_string(*amplitude, *period),
            Easing::Bounce { bounciness } => bounce::to_dsl_string(*bounciness),
        };

        let mut out = format!("{}move {{\n", indent);
        let inner_indent = format!("{}    ", indent);

        if let Some(name) = element_name {
            out.push_str(&format!("{}element = \"{}\",\n", inner_indent, name));
        }

        out.push_str(&format!(
            "{}to = ({:.3}, {:.3}),\n",
            inner_indent, self.to_x, self.to_y
        ));
        out.push_str(&format!(
            "{}during = {:.3} -> {:.3},\n",
            inner_indent, self.start, self.end
        ));
        out.push_str(&format!("{}ease = {}\n", inner_indent, ease_str));
        out.push_str(&format!("{}}}\n", indent));
        out
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
        // all other easing kinds currently map to linear in the CPU fallback
        _ => t,
    }
}

// --- legacy parsing helpers ------------------------------------------------
// legacy parsing helpers removed: parsing is now handled by dsl::parser

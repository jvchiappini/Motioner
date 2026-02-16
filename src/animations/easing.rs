use serde::{Deserialize, Serialize};

/// Easing kinds for animations (moved from `scene.rs`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Easing {
    Linear,
    /// Symmetric ease-in/out parameterized by `power` (1.0 == linear).
    /// DSL: `type = ease_in_out(power = 2.0)`
    EaseInOut {
        power: f32,
    },
    /// Power-based ease-in: progress = t^power
    EaseIn {
        power: f32,
    },
    /// Power-based ease-out: progress = 1 - (1-t)^power
    EaseOut {
        power: f32,
    },
}

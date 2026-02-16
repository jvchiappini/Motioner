use serde::{Deserialize, Serialize};

/// Easing kinds for animations (moved from `scene.rs`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Easing {
    Linear,
    /// Parametrized `Lerp` — not constant speed. `power` controls the
    /// ease-in/ease-out curvature (1.0 == linear). DSL: `type = lerp(power = 2.0)`
    Lerp { power: f32 },
}

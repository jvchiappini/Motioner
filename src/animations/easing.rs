use serde::{Deserialize, Serialize};

/// Easing kinds for animations (moved from `scene.rs`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Easing {
    Linear,
}

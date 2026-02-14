use serde::{Deserialize, Serialize};

// Easing and Shape moved to dedicated modules. Re-export them here for
// backward-compatibility so existing `crate::scene::Easing` / `crate::scene::Shape`
// references keep working.
pub use crate::animations::easing::Easing;
pub use crate::shapes::shapes_manager::Shape;
pub use crate::shapes::shapes_manager::Scene;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Animation {
    /// Move animation with normalized time (0.0 - 1.0)
    Move {
        to_x: f32,
        to_y: f32,
        /// start time in seconds
        start: f32,
        /// end time in seconds
        end: f32,
        easing: Easing,
    },
}

// The `Shape` type and its helpers live in `src/shapes/shapes_manager.rs` now.

// Re-export helpers implemented in `src/shapes/shapes_manager.rs` so external
// users can keep calling `crate::scene::get_shape*` and `crate::scene::move_node`.
pub use crate::shapes::shapes_manager::{get_shape, get_shape_mut, move_node};

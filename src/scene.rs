// `Animation` is defined in `crate::animations::animations_manager` and re-exported
pub use crate::animations::animations_manager::Animation;

// Easing and Shape moved to dedicated modules. Re-export them here for
// backward-compatibility so existing `crate::scene::Easing` / `crate::scene::Shape`
// references keep working.
pub use crate::animations::easing::{Easing, BezierPoint};
pub use crate::shapes::shapes_manager::Shape;
pub use crate::shapes::shapes_manager::Scene;

// Animation moved to `crate::animations::animations_manager::Animation`.

// The `Shape` type and its helpers live in `src/shapes/shapes_manager.rs` now.

// Re-export helpers implemented in `src/shapes/shapes_manager.rs` so external
// users can keep calling `crate::scene::get_shape*` and `crate::scene::move_node`.
pub use crate::shapes::shapes_manager::{get_shape, get_shape_mut, move_node};

// `Animation` is defined in `crate::animations::animations_manager` and re-exported
pub use crate::animations::animations_manager::Animation;

// Easing and Shape moved to dedicated modules. Re-export them here for
// backward-compatibility so existing `crate::scene::Easing` / `crate::scene::Shape`
// references keep working.
pub use crate::animations::easing::{BezierPoint, Easing};
pub use crate::shapes::shapes_manager::Shape;

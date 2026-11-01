//! Deterministic evaluation for Motioner.
//!
//! Turns a document and a frame number into a flat [`RenderPlan`] by
//! sampling keyframes, applying easing and propagating transforms and
//! opacity through the hierarchy. It is pure: no wall clock, no ambient
//! randomness and no I/O. Preview and export share this evaluator.

mod affine;
mod curve;
mod evaluate;
mod plan;

pub use affine::Affine2;
pub use curve::{ease, sample, sample_vec2, Interpolate};
pub use evaluate::Evaluator;
pub use plan::{ItemShape, RenderItem, RenderPlan, ResolvedPath};

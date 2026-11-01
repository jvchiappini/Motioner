//! Core document model for Motioner.
//!
//! This crate owns the compiled project representation: persistent node
//! IDs, the node hierarchy, animatable values and keyframes, colors and
//! the rational time base. It is the root of the dependency graph and must
//! never depend on graphics, UI, AI or platform crates.
//!
//! The document is produced by executing an MDF program (see
//! `motioner_dsl`); every mutation goes through the [`Document`] API so
//! there is one validated path into the model.

mod animation;
mod color;
mod document;
mod error;
mod math;
mod node;
mod overrides;
mod time;

pub mod bounds;

pub use animation::{Animatable, AnimatableVec2, Axis, Ease, Keyframe};
pub use color::{Color, ColorParseError};
pub use document::{Composition, Document, MAX_CANVAS_SIZE};
pub use error::CoreError;
pub use math::Vec2;
pub use node::{
    is_valid_identity_name, BlendMode, ContentField, ContentValue, Node, NodeContent, NodeId,
    NodeIdentity, Paint, Property, PropertyValue, Stroke, TextAlign, TextContent, Transform,
};
pub use overrides::{
    format_number, quote, seconds_of, split_property, OverrideAction, OverrideEntry, OverrideIssue,
    OverrideSet, OverrideValue, LOCKED, NAME, VISIBLE,
};
pub use time::{Frame, FrameRate, MAX_ABS_FRAME};

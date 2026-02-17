//! Utilities to modify shape/element parameters centrally.
//!
//! Central place for operations that change shape state (position, scale,
//! style, etc.). Keeps mutation logic out of DSL/event layers so new
//! modifiers can be added consistently.

pub mod element_modifiers;
pub mod move_element;

pub use move_element::MoveElement;

// Future exports: scale_element, set_opacity, set_fill_color, etc.

//! Core document model for Motioner.
//!
//! This crate owns the editable project representation: persistent IDs,
//! the node hierarchy, animatable values, typed commands and undo. It is
//! the root of the dependency graph and must never depend on graphics,
//! UI, AI or platform crates.

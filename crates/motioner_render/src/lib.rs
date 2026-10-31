//! Rendering adapter for Motioner.
//!
//! Wraps `FerrousEngine` behind a Motioner-owned boundary: converts render
//! plans into engine work, owns the render graph, the compositor, effects
//! and color management. Engine types must not leak past this crate.
//!
//! The engine integration is not implemented yet.

//! Live preview window for Motioner compositions.
//!
//! This crate is the Motioner-owned editor front end built on the pinned
//! `FerrousEngine` UI stack (D-0020). It opens an `.motion` (MDF) file, shows
//! the composition letterboxed in a viewport, and exposes a transport, a
//! timeline scrubber, a scene outline and a diagnostics status bar.
//!
//! # Determinism exception
//!
//! Rendered pixels depend only on the document and the integer frame index.
//! The preview is the **only** place in Motioner where a wall clock is
//! allowed, and only to choose *which* frame index to show during playback:
//! the transport converts elapsed time into a frame number with exact rational
//! arithmetic. The wall clock never changes what a frame looks like — the
//! evaluator and renderer see only the selected integer frame. Pausing the
//! transport therefore freezes the frame exactly, and export remains
//! bit-for-bit deterministic.

// UI geometry is bounded by the window size and the composition canvas (at
// most `MAX_CANVAS_SIZE` square), so narrowing to the engine's `f32` is a
// deliberate, loss-free-in-practice boundary rather than an accident.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

mod app;
mod bounds;
mod file_watch;
mod icons;
mod input;
mod inspect;
mod layout;
mod outline;
mod paint;
mod panels;
mod session;
mod text;
mod theme;
mod timeline;
mod transport;
mod widgets;
mod window;

pub use bounds::{hit_node, node_bounds, Bounds, MeasureText};
pub use file_watch::FileWatch;
pub use input::{action_for, apply, Action, Effect, Modifiers};
pub use inspect::{inspect_comp, inspect_node, Prop, Section, Value};
pub use layout::{Hit, Layout, PanelSizes, Rect, Splitter, TransportButton};
pub use outline::{outline_rows, NodeKind, OutlineRow};
pub use session::{DiagnosticInfo, PreviewSession, SessionError};
pub use text::{advance, ellipsize, Glyphs, TextStyle};
pub use theme::{Metrics, Srgb, Theme};
pub use timeline::{
    frame_to_x, tick_frames, tick_spacing, x_to_frame, TickSpacing, TickUnit, MIN_TICK_SPACING_PX,
};
pub use transport::{elapsed_frames, skipped_frames, timecode, Transport};
pub use window::{run_preview, PreviewOptions, UiError, DEFAULT_WINDOW_SIZE};

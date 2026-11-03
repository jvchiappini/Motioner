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

mod alloc;
mod app;
mod bounds;
mod capture;
mod clipboard;
mod command;
mod control;
mod edit;
mod editor;
mod file_watch;
mod gizmo;
mod graph;
mod icons;
mod input;
mod inspect;
mod json;
mod labels;
mod layout;
mod outline;
mod paint;
mod panels;
mod session;
mod text;
mod text_edit;
mod theme;
mod timeline;
mod timeline_model;
mod transport;
mod viewport;
mod widgets;
mod window;

pub use bounds::{hit_node, hit_node_where, node_bounds, Bounds, MeasureText};
pub use capture::{CaptureResult, PngWriter, DELIVERY_FRAMES};
pub use command::{
    parse_calls, parse_commands, Call, Command, CommandError, CommandKind, ErrorKind, KeySpec,
    Panel, Time,
};
pub use control::{
    default_endpoint_file, send as send_control, ControlServer, Endpoint, Pending, Request,
};
pub use edit::{apply as apply_edit, EditCommand, KeyRef, MoveKey, UndoStack};
pub use editor::{channel_has_key, channel_name, Editor, Effect, Outcome, ViewState, CHANNELS};
pub use file_watch::FileWatch;
pub use gizmo::{Corner, Guide, Handle};
pub use input::{command_for, Modifiers, Toggles};
pub use inspect::{
    hex, inspect_comp, inspect_node, kind_label, Axis, Channel, Editing, Field, FieldTarget,
    Header, Item, Layout as InspectLayout, Model as InspectModel, Row, RowValue,
};
pub use json::Json;
pub use labels::{content_label, transform_label};
pub use layout::{Hit, Layout, PanelSizes, Rect, Splitter, TopBarToggle, TransportButton};
pub use outline::{outline_rows, NodeKind, OutlineRow};
pub use session::{
    overrides_path, write_atomic, DiagnosticInfo, PreviewSession, SessionError, WRITE_DEBOUNCE,
};
pub use text::{advance, ellipsize, Glyphs, TextStyle};
pub use text_edit::{EditKey, TextEdit};
pub use theme::{Metrics, Srgb, Theme};
pub use timeline::{
    frame_to_x, tick_frames, tick_spacing, x_to_frame, TickSpacing, TickUnit, MIN_TICK_SPACING_PX,
};
pub use timeline_model::{RowKind as TimelineRowKind, TimelineRow, TimelineRows, VisibleRun};
pub use transport::{elapsed_frames, skipped_frames, timecode, Transport};
pub use viewport::{
    anchored_pan, clamp_zoom, zoom_by_steps, Viewport, MAX_ZOOM, MIN_ZOOM, WHEEL_STEP,
};
pub use window::{run_preview, PreviewOptions, UiError, DEFAULT_WINDOW_SIZE};

//! The editor panels. Each module draws one region of the [`Layout`] from a
//! read-only [`View`] of the app state; none of them mutates state or reads
//! input, so the drawn frame is a pure function of the view.
//!
//! By convention `p` is the [`crate::paint::Painter`], `v` the [`View`] and
//! `t` the theme in every drawing function.

#![allow(clippy::many_single_char_names)]

pub mod assets;
pub mod canvas;
pub mod inspector;
pub mod layers;
pub mod status;
pub mod timeline;
pub mod top_bar;

use motioner_core::{Document, FrameRate};

use crate::inspect::{Editing, FieldTarget, Layout as InspectLayout, Model as InspectModel};
use crate::layout::{Hit, Layout, Rect};
use crate::outline::OutlineRow;
use crate::session::DiagnosticInfo;
use crate::widgets::Interaction;

/// Frame-time readouts for the status bar.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Stats {
    /// Measured UI frames per second.
    pub ui_fps: f32,
    /// CPU time of the last plan evaluation, in milliseconds.
    pub eval_ms: f32,
    /// Frames skipped by playback because the UI could not keep up.
    pub dropped: u64,
    /// Allocation count (`alloc-count` feature), `0` otherwise.
    pub allocations: u64,
    /// Wall time of the last offscreen motion-blur render, in milliseconds.
    pub blur_ms: f32,
}

/// A tooltip request: anchor rectangle, label and optional shortcut.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TooltipRequest {
    /// The widget the tooltip points at.
    pub anchor: Rect,
    /// What the widget does.
    pub label: &'static str,
    /// Keyboard shortcut, if any.
    pub shortcut: Option<&'static str>,
}

/// Everything the panels need to draw one frame.
#[allow(clippy::struct_excessive_bools)] // independent UI flags, not a state machine
#[derive(Debug)]
pub struct View<'a> {
    /// Window layout.
    pub layout: &'a Layout,
    /// Source file name.
    pub file_name: &'a str,
    /// Last good document.
    pub document: Option<&'a Document>,
    /// Outline rows of the document.
    pub outline: &'a [OutlineRow],
    /// Outline indices visible after collapsing and filtering.
    pub layer_rows: &'a [usize],
    /// Selected outline row.
    pub selected: Option<usize>,
    /// Element under the pointer.
    pub hover: Option<Hit>,
    /// Element the primary button went down on.
    pub pressed: Option<Hit>,
    /// Current frame.
    pub frame: i64,
    /// Composition length in frames.
    pub duration: i64,
    /// Composition frame rate.
    pub rate: FrameRate,
    /// Whether playback is running.
    pub playing: bool,
    /// Whether playback loops.
    pub looping: bool,
    /// Diagnostics of the last load.
    pub diagnostics: &'a [DiagnosticInfo],
    /// The composition's rectangle on screen, if drawable.
    pub comp_rect: Option<Rect>,
    /// Canvas zoom (composition pixels to physical pixels).
    pub zoom: f64,
    /// Selected layer bounds on screen.
    pub selection_bounds: Option<Rect>,
    /// Size of the selected layer in composition pixels (for the label).
    pub selection_size: Option<(f64, f64)>,
    /// Hovered layer bounds on screen.
    pub hover_bounds: Option<Rect>,
    /// Whether the safe-area guides are shown.
    pub safe_area: bool,
    /// Whether the preview is showing motion blur (D-0037).
    pub motion_blur_preview: bool,
    /// Frame-time readouts.
    pub stats: Stats,
    /// Layer list scroll offset in pixels.
    pub layer_scroll: f32,
    /// Inspector scroll offset in pixels.
    pub inspector_scroll: f32,
    /// `1.0` right after a successful reload, fading to `0.0`.
    pub reload_pulse: f32,
    /// Inspector model (a layer or the composition).
    pub inspector: &'a InspectModel,
    /// Rows of the inspector laid out for this frame (already scrolled).
    pub inspector_layout: &'a InspectLayout,
    /// Field currently being typed into, if any.
    pub editing: Option<&'a Editing>,
    /// A field whose last commit was rejected (drawn in the danger colour).
    pub inspector_field_error: Option<FieldTarget>,
    /// Tooltip to show this frame.
    pub tooltip: Option<TooltipRequest>,
    /// Override warnings (orphans and kind mismatches).
    pub warnings: &'a [DiagnosticInfo],
    /// Whether the snapping toggle is on.
    pub snap: bool,
    /// Layer filter text, for the layers panel search box.
    pub filter: &'a str,
    /// Solo/shy state per outline index: `(solo, shy)`.
    pub layer_flags: &'a [(bool, bool)],
    /// Whether shy layers are globally shown.
    pub show_shy: bool,
    /// The open graph editor, when one is shown.
    pub graph: Option<&'a crate::graph::GraphModel>,
    /// Accent guide lines drawn while snapping.
    pub guides: &'a [crate::gizmo::Guide],
    /// Box-selection rectangle in the track area.
    pub marquee: Option<Rect>,
    /// Timeline rows (layer rows and expanded property rows).
    pub timeline: &'a crate::TimelineRows,
    /// Cached value text per timeline row, rewritten in place each frame.
    pub timeline_values: &'a [String],
    /// Cached layer count for the layers panel header.
    pub layer_count: &'a str,
    /// Cached timeline header/ruler strings.
    pub chrome: &'a crate::panels::timeline::ChromeText,
    /// Laid-out timeline rows for this frame.
    pub timeline_layout: &'a crate::timeline_model::RowsLayout,
    /// Keys selected in the timeline.
    pub selected_keys: &'a [crate::edit::KeyRef],
    /// Outline rows whose property rows are expanded.
    pub expanded: &'a std::collections::BTreeSet<usize>,
    /// Orphaned overrides shown at the bottom of the layers panel.
    pub orphans: &'a [OrphanRow],
    /// Whether and lock state per outline row.
    pub layer_states: &'a [(bool, bool)],
    /// Whether the identity was copied in the last moment.
    pub copied: bool,
    /// Frame delta of a key-drag in progress.
    pub retime_delta: Option<i64>,
    /// The list the left panel shows.
    pub left_tab: crate::LeftTab,
    /// Project assets.
    pub assets: &'a crate::AssetsModel,
    /// Whether the File menu is open.
    pub menu_open: bool,
    /// The File-menu item under the pointer.
    pub menu_hover: Option<crate::MenuAction>,
    /// A backup offered after a crash.
    pub recovery: Option<&'a crate::RecoveryInfo>,
}

/// One orphaned override row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrphanRow {
    /// Identity the override is keyed by.
    pub id: String,
    /// Property it sets.
    pub property: String,
    /// Best candidate node for re-binding, when one exists.
    pub suggestion: Option<String>,
}

impl View<'_> {
    /// The interaction state of a hit target.
    #[must_use]
    pub fn interaction(&self, target: Hit) -> Interaction {
        Interaction {
            hovered: self.hover == Some(target),
            pressed: self.pressed == Some(target) && self.hover == Some(target),
            disabled: false,
            active: false,
        }
    }

    /// Whether the document failed to load and nothing good is on screen.
    #[must_use]
    pub fn has_error(&self) -> bool {
        !self.diagnostics.is_empty()
    }
}

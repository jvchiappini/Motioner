//! The editor panels. Each module draws one region of the [`Layout`] from a
//! read-only [`View`] of the app state; none of them mutates state or reads
//! input, so the drawn frame is a pure function of the view.
//!
//! By convention `p` is the [`crate::paint::Painter`], `v` the [`View`] and
//! `t` the theme in every drawing function.

#![allow(clippy::many_single_char_names)]

pub mod canvas;
pub mod inspector;
pub mod layers;
pub mod status;
pub mod timeline;
pub mod top_bar;

use motioner_core::{Document, FrameRate};

use crate::inspect::Section;
use crate::layout::{Hit, Layout, Rect};
use crate::outline::{NodeKind, OutlineRow};
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
}

/// What the inspector shows.
#[derive(Debug, Clone, PartialEq)]
pub enum InspectorContent {
    /// A selected layer.
    Layer {
        /// Layer name.
        name: String,
        /// Layer kind.
        kind: NodeKind,
        /// Property sections.
        sections: Vec<Section>,
    },
    /// The composition (nothing selected).
    Composition(Vec<Section>),
    /// Nothing loaded.
    Empty,
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
pub struct View<'a> {
    /// Window layout.
    pub layout: &'a Layout,
    /// Source file name.
    pub file_name: &'a str,
    /// Last good document.
    pub document: Option<&'a Document>,
    /// Outline rows of the document.
    pub outline: &'a [OutlineRow],
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
    /// Frame-time readouts.
    pub stats: Stats,
    /// Layer list scroll offset in pixels.
    pub layer_scroll: f32,
    /// Timeline track scroll offset in pixels.
    pub track_scroll: f32,
    /// Inspector scroll offset in pixels.
    pub inspector_scroll: f32,
    /// `1.0` right after a successful reload, fading to `0.0`.
    pub reload_pulse: f32,
    /// Inspector content.
    pub inspector: &'a InspectorContent,
    /// Tooltip to show this frame.
    pub tooltip: Option<TooltipRequest>,
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

    /// The outline row under the pointer, from the layer list or the
    /// timeline names column.
    #[must_use]
    pub fn hovered_row(&self) -> Option<usize> {
        match self.hover {
            Some(Hit::LayerRow(i) | Hit::LayerEye(i) | Hit::LayerLock(i) | Hit::Track(i)) => {
                Some(i)
            }
            _ => None,
        }
    }

    /// Whether the document failed to load and nothing good is on screen.
    #[must_use]
    pub fn has_error(&self) -> bool {
        !self.diagnostics.is_empty()
    }
}

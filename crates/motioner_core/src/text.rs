//! Engine-free text measurement contract (D-0034).
//!
//! Core, DSL and eval must not depend on the engine or the file system, but the
//! DSL needs real advances for `text_width`/`text_bounds`. The
//! [`TextMetrics`] trait is that boundary: `motioner_render` implements it with
//! the same `ferrous_font` layout the renderer draws with, and `motioner_io`
//! loads the font bytes.

use crate::node::TextAlign;

/// Everything that affects a text block's metrics.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextRequest<'a> {
    /// The string to measure; `\n` starts a new line.
    pub text: &'a str,
    /// Em size in composition pixels.
    pub size: f64,
    /// Font family name.
    pub font: &'a str,
    /// Font weight (100..=900).
    pub weight: u16,
    /// Extra advance between characters, in em units.
    pub tracking: f64,
    /// Line-step multiplier.
    pub line_height: f64,
    /// Per-line alignment.
    pub align: TextAlign,
}

/// An axis-aligned box in Motioner local pixels, **Y down**.
///
/// The origin is the baseline origin of line 0, so the box is directly usable
/// for layout and hit testing in the document's coordinate space.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TextBounds {
    /// Left edge.
    pub left: f64,
    /// Top edge (negative above the baseline).
    pub top: f64,
    /// Right edge.
    pub right: f64,
    /// Bottom edge.
    pub bottom: f64,
}

impl TextBounds {
    /// The empty box.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            left: 0.0,
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
        }
    }

    /// Width of the box.
    #[must_use]
    pub fn width(&self) -> f64 {
        self.right - self.left
    }

    /// Height of the box.
    #[must_use]
    pub fn height(&self) -> f64 {
        self.bottom - self.top
    }
}

/// Metrics of a laid-out text block, in composition pixels.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TextMeasurement {
    /// Width of the widest line, including kerning and tracking.
    pub width: f64,
    /// Number of newline-separated lines (at least 1).
    pub line_count: u32,
    /// First line's ascender through the last line's descender.
    pub height: f64,
    /// Union of every visible glyph's ink box.
    pub ink: TextBounds,
    /// Advance of each line, top to bottom.
    pub line_advances: Vec<f64>,
}

/// Measures text for the interpreter and the editor.
///
/// `Send + Sync` lets the interpreter thread borrow one implementation while
/// the caller keeps it alive.
pub trait TextMetrics: Send + Sync {
    /// Measures one block, or returns `None` when the family/weight is not
    /// available.
    fn measure(&self, request: &TextRequest<'_>) -> Option<TextMeasurement>;
}

/// The "no fonts installed" implementation: every request fails.
///
/// Used by `motioner_dsl::run` and by tests that never call the measurement
/// builtins.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoTextMetrics;

impl TextMetrics for NoTextMetrics {
    fn measure(&self, _request: &TextRequest<'_>) -> Option<TextMeasurement> {
        None
    }
}

//! Rendering adapter for Motioner.
//!
//! This crate is the Motioner-owned boundary over the pinned `FerrousEngine`.
//! It turns an evaluated [`motioner_eval::RenderPlan`] into a headless frame:
//! shapes and text are converted into engine work, rendered into an off-screen
//! target and read back as straight-alpha sRGB RGBA8 bytes.
//!
//! Engine types (`ferrous_*`, `wgpu`, `glam`) are an implementation detail: no
//! public signature or public error payload in this crate mentions them. The
//! renderer is deterministic — the only wall-clock value it reports is
//! [`FrameStats::cpu_ms`], which is diagnostic and never drives a decision.
//!
//! The coordinate, colour, depth and text contracts are documented in the crate
//! README and in the private `space`, `shapes` and `text` modules.

mod frame;
mod shapes;
mod space;
mod text;

use std::collections::HashMap;
use std::fmt;

use ferrous_2d::render::FontId2d;
use ferrous_renderer::headless::Pure2dHarness;
use motioner_core::Document;
use motioner_eval::RenderPlan;

/// A headless frame renderer bound to one composition size.
///
/// A renderer owns the GPU harness, the embedded Roboto atlas and a small cache
/// of per-text-node glyph measurements. Reuse one renderer across frames: the
/// harness keeps its staging buffers, so steady-state frames do not allocate
/// GPU readback buffers and only reuse the caller's output buffer.
pub struct FrameRenderer {
    width: u32,
    height: u32,
    harness: Pure2dHarness,
    font: FontId2d,
    adapter_name: String,
    clear: Option<wgpu::Color>,
    missing_glyphs: HashMap<motioner_core::NodeId, u32>,
}

/// Per-frame counters reported by [`FrameRenderer::render`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameStats {
    /// Fill and stroke instances submitted for shape items.
    pub shape_draws: u32,
    /// Text items submitted (one engine draw call per text line).
    pub text_draws: u32,
    /// Engine `FrameReport::draw_calls` for this frame.
    pub draw_calls: u32,
    /// Characters without a glyph, measured once per text node and cached.
    pub missing_glyphs: u32,
    /// Items skipped by evaluation because their transform or opacity was not
    /// finite.
    pub non_finite_items: u32,
    /// Wall-clock render time reported by the engine, in milliseconds.
    ///
    /// Diagnostic only: never use it for a rendering decision.
    pub cpu_ms: f64,
}

/// Errors returned while creating a renderer or rendering a frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderError {
    /// No usable GPU adapter (or device) was available.
    NoAdapter {
        /// Underlying adapter/device diagnostic.
        message: String,
    },
    /// The plan's composition size does not match the renderer size.
    SizeMismatch {
        /// Size the renderer was created with.
        expected: (u32, u32),
        /// Size of the supplied plan.
        actual: (u32, u32),
    },
    /// The embedded font could not be registered with the engine.
    Font {
        /// Underlying font error.
        message: String,
    },
    /// Reading the rendered frame back into the output buffer failed.
    Readback {
        /// Underlying readback error.
        message: String,
    },
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoAdapter { message } => {
                write!(f, "no usable GPU adapter: {message}")
            }
            Self::SizeMismatch { expected, actual } => write!(
                f,
                "render plan is {}x{} but the renderer is {}x{}",
                actual.0, actual.1, expected.0, expected.1
            ),
            Self::Font { message } => write!(f, "font registration failed: {message}"),
            Self::Readback { message } => write!(f, "frame readback failed: {message}"),
        }
    }
}

impl std::error::Error for RenderError {}

impl FrameRenderer {
    /// Creates a headless renderer for a `width` × `height` composition.
    ///
    /// The engine registers the embedded Roboto font once. Other font families
    /// requested by a document fall back to Roboto (see the crate README).
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::NoAdapter`] when the engine cannot create a
    /// device (no hardware adapter and no software fallback), and
    /// [`RenderError::Font`] when the embedded font cannot be registered.
    pub fn new(width: u32, height: u32) -> Result<Self, RenderError> {
        frame::create(width, height)
    }

    /// Renders one frame and writes exactly `width * height * 4` bytes into
    /// `out`.
    ///
    /// The bytes are straight-alpha sRGB RGBA8 with row 0 at the top. `out` is
    /// resized but its allocation is reused across calls.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::SizeMismatch`] when the plan size differs from
    /// the renderer size, and [`RenderError::Readback`] when reading the
    /// rendered texture back fails.
    pub fn render(
        &mut self,
        document: &Document,
        plan: &RenderPlan,
        out: &mut Vec<u8>,
    ) -> Result<FrameStats, RenderError> {
        frame::render(self, document, plan, out)
    }

    /// The name of the GPU adapter the engine selected.
    #[must_use]
    pub fn adapter_name(&self) -> &str {
        &self.adapter_name
    }
}

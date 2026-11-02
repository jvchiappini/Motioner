//! Rendering adapter for Motioner.
//!
//! This crate is the Motioner-owned boundary over the pinned `FerrousEngine`.
//! It turns an evaluated [`motioner_eval::RenderPlan`] into a headless frame:
//! shapes and text are converted into engine work, rendered into an off-screen
//! target and read back as straight-alpha sRGB RGBA8 bytes.
//!
//! Engine types (`ferrous_*`, `wgpu`, `glam`) are an implementation detail: no
//! public signature or public error payload in this crate mentions them, with
//! one documented exception. The `live` cargo feature exposes [`LivePainter`],
//! whose `new` and `paint` methods name [`ferrous_renderer::Renderer`] so the
//! editor can paint the same plan into the window's `Pure2D` pass. The default
//! build stays engine-free at the API boundary.
//!
//! The renderer is deterministic — the only wall-clock value it reports is
//! [`FrameStats::cpu_ms`], which is diagnostic and never drives a decision.
//!
//! The coordinate, colour, depth and text contracts are documented in the crate
//! README and in the private `space`, `shapes`, `text` and `fit` modules.

mod fit;
mod frame;
mod gradients;
mod paths;
mod shapes;
mod space;
mod text;

pub use fit::ViewFit;
pub use paths::path_bounds;
pub use space::{linear_rgba, srgb_to_linear};

use std::collections::HashMap;
use std::fmt;

use ferrous_2d::render::{FontId2d, ShapeInstance};
use ferrous_font::{LineMetrics, TextAlign as EngineTextAlign};
use ferrous_renderer::headless::Pure2dHarness;
use motioner_core::Document;
use motioner_eval::RenderPlan;

/// Roboto Regular, embedded so rendering never depends on the working
/// directory. Any other requested family falls back to this font.
pub(crate) const ROBOTO: &[u8] =
    include_bytes!("../../../engine/ferrous/assets/fonts/Roboto-Regular.ttf");

/// The single drawing surface abstraction shared by the headless and live
/// paths.
///
/// Every "plan item → engine draw" decision lives above this trait; the two
/// implementations only forward to the matching engine method. That is what
/// guarantees the golden frame and the live preview cannot drift apart.
trait Canvas2d {
    /// Submits one shape instance.
    fn shape(&mut self, instance: ShapeInstance);

    /// Submits one text line with an already-built model matrix.
    fn text(
        &mut self,
        font: FontId2d,
        text: &str,
        transform: glam::Mat4,
        size_px: f32,
        align: EngineTextAlign,
        color: [f32; 4],
    );

    /// Measures one text line for the missing-glyph counter.
    fn measure(
        &self,
        font: FontId2d,
        text: &str,
        size_px: f32,
        align: EngineTextAlign,
    ) -> Option<LineMetrics>;
}

impl Canvas2d for Pure2dHarness {
    fn shape(&mut self, instance: ShapeInstance) {
        self.draw_shape(instance);
    }

    fn text(
        &mut self,
        font: FontId2d,
        text: &str,
        transform: glam::Mat4,
        size_px: f32,
        align: EngineTextAlign,
        color: [f32; 4],
    ) {
        self.draw_text(font, text, transform, size_px, align, color);
    }

    fn measure(
        &self,
        font: FontId2d,
        text: &str,
        size_px: f32,
        align: EngineTextAlign,
    ) -> Option<LineMetrics> {
        self.measure_text(font, text, size_px, align)
    }
}

impl Canvas2d for ferrous_renderer::Renderer {
    fn shape(&mut self, instance: ShapeInstance) {
        self.draw_2d_shape(instance);
    }

    fn text(
        &mut self,
        font: FontId2d,
        text: &str,
        transform: glam::Mat4,
        size_px: f32,
        align: EngineTextAlign,
        color: [f32; 4],
    ) {
        self.draw_text_2d(font, text, transform, size_px, align, color);
    }

    fn measure(
        &self,
        font: FontId2d,
        text: &str,
        size_px: f32,
        align: EngineTextAlign,
    ) -> Option<LineMetrics> {
        self.measure_text_2d(font, text, size_px, align)
    }
}

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
    path_cache: paths::PathCache,
}

/// Counters returned by one plan paint into a drawing surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PaintStats {
    /// Fill and stroke instances submitted for shape items.
    pub shape_draws: u32,
    /// Text items submitted (one engine draw call per text line).
    pub text_draws: u32,
    /// Characters without a glyph, measured once per text node and cached.
    pub missing_glyphs: u32,
    /// Items skipped by evaluation because their transform or opacity was not
    /// finite.
    pub non_finite_items: u32,
}

/// Horizontal advance and vertical extent of one measured text line, in
/// composition pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextExtent {
    /// Pen advance including kerning.
    pub advance: f64,
    /// Distance from the baseline up to the ascender; positive.
    pub ascender: f64,
    /// Distance from the baseline down to the descender; negative.
    pub descender: f64,
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

/// A drawing feature the pinned engine cannot render yet (D-0030).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Feature {
    /// Per-pixel linear/radial gradients with animatable stops.
    Gradient,
}

impl Feature {
    /// Human name used in errors.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Gradient => "gradient",
        }
    }
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
    /// The pinned engine cannot draw this feature yet.
    ///
    /// The document is valid and evaluated; only the GPU pass is missing.
    /// Motioner refuses instead of silently drawing the solid fallback.
    Unsupported {
        /// The missing feature.
        feature: Feature,
        /// The first node that needs it.
        node: Option<motioner_core::NodeId>,
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
            Self::Unsupported { feature, node } => match node {
                Some(node) => write!(
                    f,
                    "{} is not supported by the pinned engine yet (node {node}); see D-0030",
                    feature.name()
                ),
                None => write!(
                    f,
                    "{} is not supported by the pinned engine yet; see D-0030",
                    feature.name()
                ),
            },
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

/// Paints plans into a live [`ferrous_renderer::Renderer`].
///
/// This is the only public API of this crate that names an engine type; it is
/// compiled only with the `live` feature, which only `motioner_ui` enables.
/// The painter owns the embedded font registration and the per-node
/// missing-glyph cache for one renderer, so a frame loop allocates nothing.
///
/// [`LivePainter::paint`] only queues shapes and text into the renderer's
/// `Pure2D` batchers; it never clears, submits or presents. The engine runner
/// draws the batchers later in the same frame, which is what puts the UI panels
/// on top.
#[cfg(feature = "live")]
pub struct LivePainter {
    font: FontId2d,
    missing_glyphs: HashMap<motioner_core::NodeId, u32>,
    path_cache: paths::PathCache,
}

#[cfg(feature = "live")]
impl LivePainter {
    /// Registers the embedded font with `renderer` and prepares a painter.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::Font`] when the font cannot be registered.
    pub fn new(renderer: &mut ferrous_renderer::Renderer) -> Result<Self, RenderError> {
        let font = renderer
            .register_font_2d(ROBOTO, ferrous_font::charset::latin_western())
            .map_err(|error| RenderError::Font {
                message: error.to_string(),
            })?;
        Ok(Self {
            font,
            missing_glyphs: HashMap::new(),
            path_cache: paths::PathCache::default(),
        })
    }

    /// Drops the cached per-node missing-glyph counts.
    ///
    /// Call this after loading a different document: node IDs are reused by a
    /// new program, so a cached count could describe another text line. The
    /// pixels are unaffected (glyph lookup happens in the engine), only the
    /// reported [`PaintStats::missing_glyphs`] would be stale.
    pub fn clear_cache(&mut self) {
        self.missing_glyphs.clear();
        self.path_cache = paths::PathCache::default();
    }

    /// Measures one text line of the embedded font at `size` composition
    /// pixels, for selection bounds and hit testing.
    ///
    /// Returns `None` when the engine cannot measure the line. The values are
    /// in the same composition pixels as `size`, before any [`ViewFit`].
    #[must_use]
    pub fn text_extent(
        &self,
        renderer: &ferrous_renderer::Renderer,
        text: &str,
        size: f64,
    ) -> Option<TextExtent> {
        let size_px = space::to_f32(size);
        let metrics = renderer.measure_text_2d(self.font, text, size_px, EngineTextAlign::Left)?;
        Some(TextExtent {
            advance: f64::from(metrics.advance),
            ascender: f64::from(metrics.ascender),
            descender: f64::from(metrics.descender),
        })
    }

    /// Queues the plan's shapes and text into `renderer`, mapped by `fit`.
    ///
    /// A fit that draws nothing (for example a zero-size viewport) returns
    /// zeroed [`PaintStats`] without touching the renderer.
    pub fn paint(
        &mut self,
        renderer: &mut ferrous_renderer::Renderer,
        document: &Document,
        plan: &RenderPlan,
        fit: ViewFit,
    ) -> PaintStats {
        if !fit.is_drawable() {
            return PaintStats::default();
        }
        let flip = fit.transform(space::composition_flip(plan.width(), plan.height()));
        let gradients = gradients::GradientTable::build(plan, flip);
        #[cfg(feature = "engine-gradients")]
        renderer.set_2d_gradient_table(gradients.slots());
        frame::paint_plan(
            renderer,
            document,
            plan,
            flip,
            self.font,
            &mut self.missing_glyphs,
            &mut self.path_cache,
            &gradients,
        )
    }
}

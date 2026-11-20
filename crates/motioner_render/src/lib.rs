//! Rendering adapter for Motioner.
//!
//! This crate is the Motioner-owned boundary over the pinned `FerrousEngine`.
//! It turns an evaluated [`motioner_eval::RenderPlan`] into a headless frame:
//! shapes and text are converted into engine work, rendered into an off-screen
//! target and read back as straight-alpha sRGB RGBA8 bytes.
//!
//! Engine types (`ferrous_*`, `wgpu`, `glam`) are an implementation detail: no
//! public signature or public error payload in this crate mentions them, with
//! one documented exception. The `live` cargo feature exposes `LivePainter`,
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
mod fonts;
mod frame;
mod gradients;
mod paths;
mod proxy;
mod shapes;
mod space;
mod text;
/// Agent render tools: overlays, strips, sheets, zoom, compare, cache and the
/// textless background render (D-0043 phase 8).
pub mod tools;

pub use fit::ViewFit;
pub use fonts::{
    document_faces, CpuTextMetrics, FontFace, FontFamilyError, FontSet, FontWarning, ResolvedFace,
};
pub use paths::path_bounds;
pub use space::{linear_rgba, srgb_to_linear};

use rustc_hash::FxHashMap as HashMap;
use std::cell::{Cell, RefCell};
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

use ferrous_2d::render::{FontId2d, ShapeInstance};
use ferrous_core::memory::{self, Subsystem};
use ferrous_font::TextAlign as EngineTextAlign;
use ferrous_renderer::headless::Pure2dHarness;
use motioner_core::Document;
use motioner_eval::RenderPlan;

/// Roboto Regular, embedded so rendering never depends on the working
/// directory. Any other requested family falls back to this font.
pub(crate) const ROBOTO: &[u8] =
    include_bytes!("../../../engine/ferrous/assets/fonts/Roboto-Regular.ttf");

/// Default image-texture budget: 512 MiB of resident GPU bytes.
pub const DEFAULT_TEXTURE_BUDGET: u64 = 512 * 1024 * 1024;

/// Default CPU budget for decoded image pixels held by the interactive
/// painter: 256 MiB.
///
/// The headless renderer keeps every decoded pixel (export and goldens never
/// trade resolution for memory).  The preview enforces this budget once the
/// textures exist, dropping the least recently used resident copies first.
pub const DEFAULT_IMAGE_CPU_BUDGET: u64 = 256 * 1024 * 1024;

/// How large the GPU copies of decoded images may be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProxyTarget {
    /// Upload every decoded image at its natural size (headless, export and
    /// the default everywhere).
    #[default]
    Full,
    /// Downscale an image that does not fit `max_width x max_height`.
    ///
    /// The preview can size a whole image to the composition rectangle this
    /// way.  UVs stay normalised, so the image still maps 1:1 at 100 % zoom.
    /// Above 100 % zoom the GPU magnifies this smaller texture (bilinear), so
    /// the image looks softer than an export while shapes and text stay exact.
    Fit {
        /// Largest texture width to upload.
        max_width: u32,
        /// Largest texture height to upload.
        max_height: u32,
    },
    /// Size every texture from the item that draws it (the interactive
    /// canvas).
    ///
    /// Each painted frame computes the texels each drawn item's crop, node
    /// scale, view zoom and DPI can resolve and re-uploads a texture when it
    /// is more than 1.25x too small.  A texture that becomes too large is kept
    /// until it is past 2x, so zooming does not thrash.  When the CPU pixels
    /// were released, the painter reports the key through
    /// `LivePainter::missing_image_pixels` (compiled only with the `live`
    /// feature) instead of uploading a blurry placeholder, and keeps drawing
    /// the existing texture meanwhile.
    PerImage,
}

/// Owner tag for image CPU entries, so two painters do not unregister each
/// other's inventory rows.
fn next_image_owner() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// Registers and unregisters the CPU pixels one painter/renderer owns.
#[derive(Debug)]
struct ImageInventory {
    owner: u64,
}

impl ImageInventory {
    fn new() -> Self {
        Self {
            owner: next_image_owner(),
        }
    }

    /// Inventory label of one image's CPU pixels.
    fn label(&self, key: &motioner_core::ImageKey) -> String {
        format!("motioner:image-cpu#{}:{}", self.owner, key)
    }

    /// Registers the resident pixels of `image` (a no-op when dropped).
    #[allow(clippy::cast_lossless)] // usize -> u64 is lossless on supported targets
    fn register(&self, image: &ImageData) {
        if image.rgba8.is_empty() {
            return;
        }
        memory::register_cpu(
            self.label(&image.key),
            image.rgba8.len() as u64,
            Subsystem::Assets,
        );
    }

    /// Removes the entry for `key` (removal, resize or a pixel drop).
    fn unregister(&self, key: &motioner_core::ImageKey) {
        memory::unregister_cpu(&self.label(key), Subsystem::Assets);
    }

    /// Removes every entry this owner registered for `images`.
    fn unregister_all(&self, images: &ImageSet) {
        for key in images.keys() {
            memory::unregister_cpu(&self.label(key), Subsystem::Assets);
        }
    }
}

/// A mask request in Motioner terms.
///
/// The canvas impls translate this into the engine's `Mask2d`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CanvasMask {
    /// Alpha or luma coverage.
    pub mode: motioner_core::MaskMode,
    /// Whether the coverage is inverted.
    pub invert: bool,
}

/// A decoded image ready for upload (D-0035).
#[derive(Debug, Clone)]
pub struct ImageData {
    /// Content key (SHA-256 of the file bytes).
    pub key: motioner_core::ImageKey,
    /// Natural width in pixels.
    pub width: u32,
    /// Natural height in pixels.
    pub height: u32,
    /// Straight-alpha sRGB RGBA8, `width * height * 4` bytes.
    pub rgba8: Vec<u8>,
    /// Changes when the bytes for the same key change. A live video frame
    /// reuses one key and bumps this so the texture is rewritten in place.
    pub revision: u64,
}

/// Every decoded image a renderer may draw, keyed by content hash.
///
/// The set also remembers a least-recently-used order for the CPU pixels, so
/// the renderer can release old copies once their GPU texture exists.
/// Lookups through [`Self::get`] count as use.
#[derive(Debug, Clone, Default)]
pub struct ImageSet {
    images: HashMap<motioner_core::ImageKey, ImageData>,
    /// Last-use clock per key, for the CPU-pixel LRU.
    last_use: RefCell<HashMap<motioner_core::ImageKey, u64>>,
    clock: Cell<u64>,
}

impl ImageSet {
    /// An empty set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds (or replaces) one image.
    pub fn insert(&mut self, image: ImageData) {
        self.touch(&image.key);
        self.images.insert(image.key, image);
    }

    /// The image for `key`, when present.  Counts as a use for the LRU.
    #[must_use]
    pub fn get(&self, key: &motioner_core::ImageKey) -> Option<&ImageData> {
        let image = self.images.get(key)?;
        self.touch(key);
        Some(image)
    }

    /// Number of images.
    #[must_use]
    pub fn len(&self) -> usize {
        self.images.len()
    }

    /// Whether the set is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.images.is_empty()
    }

    /// Every image in the set.
    pub fn iter(&self) -> impl Iterator<Item = (&motioner_core::ImageKey, &ImageData)> {
        self.images.iter()
    }

    /// Every key in the set.
    pub(crate) fn keys(&self) -> impl Iterator<Item = &motioner_core::ImageKey> {
        self.images.keys()
    }

    /// Resident CPU bytes of every decoded image.
    #[must_use]
    #[allow(clippy::cast_lossless)] // usize -> u64 is lossless on supported targets
    pub(crate) fn total_bytes(&self) -> u64 {
        self.images
            .values()
            .map(|image| image.rgba8.len() as u64)
            .sum()
    }

    /// Releases the pixels of `key`, keeping its key, size and revision.
    ///
    /// Returns whether anything was resident.  The matching texture must
    /// already exist; the caller unregisters the inventory row.
    pub(crate) fn drop_pixels(&mut self, key: &motioner_core::ImageKey) -> bool {
        let Some(image) = self.images.get_mut(key) else {
            return false;
        };
        let resident = !image.rgba8.is_empty();
        image.rgba8 = Vec::new();
        resident
    }

    /// Drops least-recently-used resident pixels until `total_bytes <= budget`.
    ///
    /// `is_uploaded` must answer `true` only for keys whose GPU texture exists;
    /// an image evicted before its upload could never be drawn.  `out` receives
    /// the dropped keys, oldest first, so the caller can unregister them.
    #[allow(clippy::cast_lossless)] // usize -> u64 is lossless on supported targets
    pub(crate) fn evict_over_budget_into(
        &mut self,
        budget: u64,
        is_uploaded: impl Fn(&motioner_core::ImageKey) -> bool,
        out: &mut Vec<motioner_core::ImageKey>,
    ) {
        out.clear();
        let mut total = self.total_bytes();
        while total > budget {
            let Some(key) = self.oldest_resident(&is_uploaded) else {
                break;
            };
            let released = self
                .images
                .get_mut(&key)
                .map_or(0, |image| image.rgba8.len() as u64);
            let _ = self.drop_pixels(&key);
            total = total.saturating_sub(released);
            self.last_use.borrow_mut().remove(&key);
            out.push(key);
        }
    }

    fn touch(&self, key: &motioner_core::ImageKey) {
        let next = self.clock.get().wrapping_add(1);
        self.clock.set(next);
        self.last_use.borrow_mut().insert(*key, next);
    }

    fn oldest_resident(
        &self,
        is_uploaded: &impl Fn(&motioner_core::ImageKey) -> bool,
    ) -> Option<motioner_core::ImageKey> {
        let order = self.last_use.borrow();
        self.images
            .iter()
            .filter(|(key, image)| !image.rgba8.is_empty() && is_uploaded(key))
            .min_by_key(|(key, _)| order.get(*key).copied().unwrap_or(0))
            .map(|(key, _)| *key)
    }
}

/// One resident GPU image texture and the inputs it was built from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ImageTexture {
    /// Engine texture handle.
    pub(crate) id: u32,
    /// Uploaded width in pixels.
    pub(crate) width: u32,
    /// Uploaded height in pixels.
    pub(crate) height: u32,
    /// Image revision the texels came from.
    pub(crate) revision: u64,
}

/// Why a key cannot be recovered by the background re-decode worker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReloadFailure {
    /// A live video frame: the worker re-decodes project images only, so it
    /// can never return fresh pixels for this key.
    Dynamic,
    /// The source could not be decoded again (removed, unreadable, ...).
    Unavailable,
}

/// Upper bound on the sticky negative cache, so a document that keeps changing
/// image sets cannot grow it without limit.
pub(crate) const MAX_RELOAD_FAILURES: usize = 256;

/// Per-key image textures plus the keys whose CPU pixels are needed again.
///
/// The size is tracked next to the revision so a zoom can decide whether the
/// resident texture already resolves the drawn pixels, instead of assuming a
/// revision bump is the only reason to re-upload.
#[derive(Debug, Clone, Default)]
pub(crate) struct ImageTextures {
    entries: HashMap<motioner_core::ImageKey, ImageTexture>,
    missing: Vec<motioner_core::ImageKey>,
    /// Keys whose pixels change every frame (a video). Proxies never apply to
    /// them: resampling a full frame on the frame thread costs more than the
    /// texture saves.
    dynamic: std::collections::HashSet<motioner_core::ImageKey>,
    /// Sticky negative cache: a key that already failed a re-decode (or can
    /// never be re-decoded) is not reported as missing again. Cleared when the
    /// image set is replaced, and per key when fresh pixels arrive.
    failed: Vec<(motioner_core::ImageKey, ReloadFailure)>,
}

impl ImageTextures {
    /// The resident texture for `key`.
    pub(crate) fn get(&self, key: &motioner_core::ImageKey) -> Option<&ImageTexture> {
        self.entries.get(key)
    }

    /// Every resident texture key.
    pub(crate) fn keys(&self) -> impl Iterator<Item = &motioner_core::ImageKey> {
        self.entries.keys()
    }

    /// Inserts (or replaces) one texture.
    pub(crate) fn insert(&mut self, key: motioner_core::ImageKey, texture: ImageTexture) {
        self.entries.insert(key, texture);
    }

    /// Removes one texture.
    pub(crate) fn remove(&mut self, key: &motioner_core::ImageKey) {
        self.entries.remove(key);
    }

    /// Takes every texture out; the caller frees the engine handles.
    pub(crate) fn drain(
        &mut self,
    ) -> impl Iterator<Item = (motioner_core::ImageKey, ImageTexture)> + '_ {
        self.missing.clear();
        self.dynamic.clear();
        self.failed.clear();
        self.entries.drain()
    }

    /// Whether `key` is a live frame whose pixels change every frame.
    pub(crate) fn is_dynamic(&self, key: &motioner_core::ImageKey) -> bool {
        self.dynamic.contains(key)
    }

    /// Marks `key` as a live frame; its texture is uploaded at natural size.
    pub(crate) fn mark_dynamic(&mut self, key: motioner_core::ImageKey) {
        self.dynamic.insert(key);
    }

    /// Forgets the previous frame's missing keys.
    pub(crate) fn clear_missing(&mut self) {
        self.missing.clear();
    }

    /// Forgets that `key` needs pixels (they arrived).
    pub(crate) fn clear_missing_key(&mut self, key: &motioner_core::ImageKey) {
        self.missing.retain(|candidate| candidate != key);
    }

    /// Records that `key` needs a fresh decode before it can be re-uploaded.
    ///
    /// A dynamic key (live video frame) and a key already known to be
    /// unrecoverable are never reported: the worker cannot help with either,
    /// so reporting them would only spawn (and re-spawn) a doomed decode.
    pub(crate) fn mark_missing(&mut self, key: motioner_core::ImageKey) {
        if self.dynamic.contains(&key) || self.failure(&key).is_some() {
            return;
        }
        if !self.missing.contains(&key) {
            self.missing.push(key);
        }
    }

    /// Records that `key` cannot be re-decoded, so it is never reported again
    /// until the image set is replaced or fresh pixels arrive for it.
    pub(crate) fn mark_failure(&mut self, key: motioner_core::ImageKey, reason: ReloadFailure) {
        self.missing.retain(|candidate| *candidate != key);
        if let Some(entry) = self
            .failed
            .iter_mut()
            .find(|(candidate, _)| *candidate == key)
        {
            entry.1 = reason;
            return;
        }
        if self.failed.len() >= MAX_RELOAD_FAILURES {
            // Bounded: evict one entry rather than grow without limit. Which
            // one is immaterial; a stale key simply gets one more attempt.
            self.failed.swap_remove(0);
        }
        self.failed.push((key, reason));
    }

    /// Forgets the failure of `key`: fresh pixels arrived, so a later release
    /// may ask for it again.
    pub(crate) fn clear_failure(&mut self, key: &motioner_core::ImageKey) {
        self.failed.retain(|(candidate, _)| candidate != key);
    }

    /// The recorded failure for `key`, when it is not recoverable.
    #[must_use]
    pub(crate) fn failure(&self, key: &motioner_core::ImageKey) -> Option<ReloadFailure> {
        self.failed
            .iter()
            .find(|(candidate, _)| candidate == key)
            .map(|(_, reason)| *reason)
    }

    /// Keys whose texture needs an upload but whose pixels were released.
    #[must_use]
    pub(crate) fn missing(&self) -> &[motioner_core::ImageKey] {
        &self.missing
    }
}

/// Ensures the texture for `image` resolves `target` pixels, re-uploading only
/// when the resident texture is too small, too large or from another revision.
///
/// Returns `Ok(Some(id))` when a texture can be drawn (possibly the previous,
/// lower-resolution one while its pixels are being re-fetched), `Ok(None)`
/// when nothing exists yet, and `Err` when the engine rejects the upload.
pub(crate) fn ensure_image_texture(
    canvas: &mut impl Canvas2d,
    textures: &mut ImageTextures,
    key: motioner_core::ImageKey,
    image: &ImageData,
    target: (u32, u32),
    scratch: &mut Vec<u8>,
) -> Result<Option<u32>, RenderError> {
    let current = textures.get(&key).copied();
    // A revision change under the same key means live pixels (a video frame):
    // the first one marks the key dynamic and the texture moves to natural
    // size, so later frames are rewritten in place instead of being resampled
    // to a proxy on every frame.
    let dynamic = textures.is_dynamic(&key)
        || current.is_some_and(|texture| texture.revision != image.revision);
    if dynamic {
        textures.mark_dynamic(key);
    }
    let size = if dynamic {
        (image.width, image.height)
    } else {
        proxy::target_size(
            current.map(|texture| (texture.width, texture.height)),
            target,
        )
    };
    if let Some(texture) = current {
        if texture.revision == image.revision && (texture.width, texture.height) == size {
            return Ok(Some(texture.id));
        }
    }
    if image.rgba8.is_empty() {
        // The interactive preview released the CPU pixels after its first
        // upload.  Keep drawing the resident texture and let the app fetch the
        // pixels for this key off the frame thread.
        //
        // A dynamic key (live video frame) is the exception: the re-decode
        // worker only reads project images, so reporting it as missing would
        // spawn a doomed worker on every paint. The video path supplies its
        // pixels through `set_image` instead.
        if dynamic {
            textures.mark_failure(key, ReloadFailure::Dynamic);
        } else {
            textures.mark_missing(key);
        }
        return Ok(current.map(|texture| texture.id));
    }
    let upload = proxy::prepare_at(image, size.0, size.1, scratch);
    // `prepare_at` falls back to the source when the buffer cannot be
    // resampled (a malformed length).  A resident texture at that size already
    // holds the best pixels available, so it is not re-uploaded every frame.
    if (upload.width, upload.height) != size {
        if let Some(texture) = current {
            if (texture.width, texture.height) == (upload.width, upload.height) {
                return Ok(Some(texture.id));
            }
        }
    }
    if let Some(texture) = current {
        if (texture.width, texture.height) == (upload.width, upload.height) {
            if canvas
                .update_image(texture.id, upload.width, upload.height, upload.rgba8)
                .is_ok()
            {
                textures.insert(
                    key,
                    ImageTexture {
                        id: texture.id,
                        width: upload.width,
                        height: upload.height,
                        revision: image.revision,
                    },
                );
                return Ok(Some(texture.id));
            }
            canvas.free_image(texture.id);
            textures.remove(&key);
        } else {
            // The engine cannot resize in place, so a size change frees and
            // registers a new texture.
            canvas.free_image(texture.id);
            textures.remove(&key);
        }
    }
    match canvas.register_image(upload.width, upload.height, upload.rgba8) {
        Ok(id) => {
            textures.insert(
                key,
                ImageTexture {
                    id,
                    width: upload.width,
                    height: upload.height,
                    revision: image.revision,
                },
            );
            Ok(Some(id))
        }
        Err(error) => {
            textures.remove(&key);
            Err(error)
        }
    }
}

/// The single drawing surface abstraction shared by the headless and live
/// paths.
///
/// Every "plan item → engine draw" decision lives above this trait; the two
/// implementations only forward to the matching engine method. That is what
/// guarantees the golden frame and the live preview cannot drift apart.
trait Canvas2d {
    /// Submits one shape instance.
    fn shape(&mut self, instance: ShapeInstance);

    /// Characters `text` cannot draw with `font`.
    ///
    /// Allocation-free: the missing-glyph badge must not pay for a measure on
    /// every text node of every frame.
    fn missing_glyphs(&self, font: FontId2d, text: &str) -> u32;

    /// Lays out a whole styled block.
    fn layout_text(
        &self,
        font: FontId2d,
        text: &str,
        style: ferrous_2d::render::TextStyle2d,
        out: &mut Vec<ferrous_font::GlyphLayout>,
    ) -> Option<ferrous_font::TextMetrics>;

    /// Draws a whole styled block.
    #[allow(clippy::too_many_arguments)] // the plan item fields, kept flat
    fn text_styled(
        &mut self,
        font: FontId2d,
        text: &str,
        transform: glam::Mat4,
        style: ferrous_2d::render::TextStyle2d,
        color: [f32; 4],
        gradient: Option<u32>,
    );

    /// Draws pre-laid-out glyphs.
    fn glyphs(&mut self, font: FontId2d, glyphs: &[ferrous_2d::render::GlyphDraw2d]);

    /// Uploads one image texture and returns its engine texture id.
    ///
    /// `rgba8` is straight-alpha sRGB with exactly `width * height * 4` bytes;
    /// proxy resampling happens above this trait.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::Image`] when the pinned engine has no image
    /// support or the upload fails.
    fn register_image(&mut self, width: u32, height: u32, rgba8: &[u8])
        -> Result<u32, RenderError>;

    /// Rewrites an existing image texture in place (same dimensions).
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::Image`] when the pinned engine rejects the
    /// update.
    fn update_image(
        &mut self,
        texture_id: u32,
        width: u32,
        height: u32,
        rgba8: &[u8],
    ) -> Result<(), RenderError>;

    /// Frees an image texture so a resized frame can be registered again.
    fn free_image(&mut self, texture_id: u32);

    /// Queues one sprite quad for an image texture.
    fn draw_image(&mut self, texture_id: u32, instance: ferrous_2d::render::SpriteInstance);

    /// Queues one textured triangle mesh for an image texture.
    ///
    /// `vertices` are in the node's local pixel space (the same space the
    /// sprite's `width x height` box scales) and `transform` places them in
    /// world space; the mesh samples `texture_id` exactly like a sprite.
    fn draw_mesh(
        &mut self,
        texture_id: u32,
        vertices: &[ferrous_2d::render::MeshVertex2d],
        indices: &[u32],
        transform: glam::Mat4,
    );

    /// Opens an offscreen layer for a group of draws (D-0031, D-0032).
    ///
    /// `blend` selects how the finished layer composites into its parent and
    /// `effects` is the post-process chain run on the layer before that.
    /// Returns `false` when the engine rejected the layer (nothing was opened,
    /// so the caller must not pop it).
    fn push_layer(
        &mut self,
        opacity: f64,
        blend: motioner_core::BlendMode,
        depth: f32,
        effects: &[ferrous_2d::render::Effect2d],
    ) -> bool;

    /// Opens an offscreen layer masked by `mask` (D-0033).
    ///
    /// The mask source is drawn between [`Canvas2d::begin_mask`] and
    /// [`Canvas2d::end_mask`] after the content.  Returns `false` when the
    /// engine rejected the layer (nothing was opened, so the caller must not
    /// pop it or draw a source).
    fn push_masked_layer(
        &mut self,
        opacity: f64,
        blend: motioner_core::BlendMode,
        depth: f32,
        effects: &[ferrous_2d::render::Effect2d],
        mask: CanvasMask,
    ) -> bool;

    /// Starts the mask source of the innermost masked layer.
    fn begin_mask(&mut self);

    /// Ends the mask source and attaches it to the masked layer.
    fn end_mask(&mut self);

    /// Closes the innermost layer.
    fn pop_layer(&mut self);
}

/// The engine mask type for a canvas mask request.
fn engine_mask(mask: CanvasMask) -> ferrous_2d::render::Mask2d {
    use ferrous_2d::render::MaskMode2d;
    ferrous_2d::render::Mask2d {
        mode: match mask.mode {
            motioner_core::MaskMode::Alpha => MaskMode2d::Alpha,
            motioner_core::MaskMode::Luma => MaskMode2d::Luma,
        },
        invert: mask.invert,
    }
}

/// The engine composite mode of a node mode (D-0031).
fn composite_blend(blend: motioner_core::BlendMode) -> ferrous_2d::render::CompositeMode2d {
    use ferrous_2d::render::CompositeMode2d;
    use motioner_core::BlendMode;
    match blend {
        BlendMode::Normal => CompositeMode2d::Normal,
        BlendMode::Add => CompositeMode2d::Add,
        BlendMode::Multiply => CompositeMode2d::Multiply,
        BlendMode::Screen => CompositeMode2d::Screen,
        BlendMode::Overlay => CompositeMode2d::Overlay,
        BlendMode::Darken => CompositeMode2d::Darken,
        BlendMode::Lighten => CompositeMode2d::Lighten,
    }
}

impl Canvas2d for Pure2dHarness {
    fn shape(&mut self, instance: ShapeInstance) {
        self.draw_shape(instance);
    }

    fn missing_glyphs(&self, font: FontId2d, text: &str) -> u32 {
        self.renderer().missing_glyphs_2d(font, text)
    }

    fn layout_text(
        &self,
        font: FontId2d,
        text: &str,
        style: ferrous_2d::render::TextStyle2d,
        out: &mut Vec<ferrous_font::GlyphLayout>,
    ) -> Option<ferrous_font::TextMetrics> {
        self.layout_text(font, text, style, out)
    }

    fn text_styled(
        &mut self,
        font: FontId2d,
        text: &str,
        transform: glam::Mat4,
        style: ferrous_2d::render::TextStyle2d,
        color: [f32; 4],
        gradient: Option<u32>,
    ) {
        self.draw_text_styled_with_gradient(font, text, transform, style, color, gradient);
    }

    fn glyphs(&mut self, font: FontId2d, glyphs: &[ferrous_2d::render::GlyphDraw2d]) {
        self.draw_glyphs(font, glyphs);
    }

    fn register_image(
        &mut self,
        width: u32,
        height: u32,
        rgba8: &[u8],
    ) -> Result<u32, RenderError> {
        self.register_texture_2d(
            width,
            height,
            rgba8,
            ferrous_2d::render::TextureOptions2d::photo(),
        )
        .map_err(|error| RenderError::Image {
            message: error.to_string(),
        })
    }

    fn update_image(
        &mut self,
        texture_id: u32,
        width: u32,
        height: u32,
        rgba8: &[u8],
    ) -> Result<(), RenderError> {
        self.renderer_mut()
            .update_sprite_texture(texture_id, width, height, rgba8)
            .map_err(|error| RenderError::Image {
                message: error.to_string(),
            })
    }

    fn free_image(&mut self, texture_id: u32) {
        self.renderer_mut().free_sprite_texture(texture_id);
    }

    fn draw_image(&mut self, texture_id: u32, instance: ferrous_2d::render::SpriteInstance) {
        self.draw_sprite(texture_id, instance);
    }

    fn draw_mesh(
        &mut self,
        texture_id: u32,
        vertices: &[ferrous_2d::render::MeshVertex2d],
        indices: &[u32],
        transform: glam::Mat4,
    ) {
        self.draw_textured_mesh(
            texture_id,
            vertices,
            indices,
            transform,
            ferrous_2d::render::BlendMode2d::Normal,
        );
    }

    fn push_layer(
        &mut self,
        opacity: f64,
        blend: motioner_core::BlendMode,
        depth: f32,
        effects: &[ferrous_2d::render::Effect2d],
    ) -> bool {
        let opacity = space::to_f32(opacity);
        let desc = ferrous_2d::render::Layer2d::new(opacity, composite_blend(blend), depth);
        if effects.is_empty() {
            Pure2dHarness::push_layer(self, desc).is_ok()
        } else {
            Pure2dHarness::push_layer_with_effects(self, desc, effects).is_ok()
        }
    }

    fn push_masked_layer(
        &mut self,
        opacity: f64,
        blend: motioner_core::BlendMode,
        depth: f32,
        effects: &[ferrous_2d::render::Effect2d],
        mask: CanvasMask,
    ) -> bool {
        let opacity = space::to_f32(opacity);
        let desc = ferrous_2d::render::Layer2d::new(opacity, composite_blend(blend), depth);
        Pure2dHarness::push_masked_layer(self, desc, effects, engine_mask(mask)).is_ok()
    }

    fn begin_mask(&mut self) {
        let _ = Pure2dHarness::begin_mask(self);
    }

    fn end_mask(&mut self) {
        let _ = Pure2dHarness::end_mask(self);
    }

    fn pop_layer(&mut self) {
        let _ = Pure2dHarness::pop_layer(self);
    }
}

impl Canvas2d for ferrous_renderer::Renderer {
    fn shape(&mut self, instance: ShapeInstance) {
        self.draw_2d_shape(instance);
    }

    fn missing_glyphs(&self, font: FontId2d, text: &str) -> u32 {
        self.missing_glyphs_2d(font, text)
    }

    fn layout_text(
        &self,
        font: FontId2d,
        text: &str,
        style: ferrous_2d::render::TextStyle2d,
        out: &mut Vec<ferrous_font::GlyphLayout>,
    ) -> Option<ferrous_font::TextMetrics> {
        ferrous_renderer::Renderer::layout_text_2d(self, font, text, style, out)
    }

    fn text_styled(
        &mut self,
        font: FontId2d,
        text: &str,
        transform: glam::Mat4,
        style: ferrous_2d::render::TextStyle2d,
        color: [f32; 4],
        gradient: Option<u32>,
    ) {
        ferrous_renderer::Renderer::draw_text_2d_with_gradient_styled(
            self, font, text, transform, style, color, gradient,
        );
    }

    fn glyphs(&mut self, font: FontId2d, glyphs: &[ferrous_2d::render::GlyphDraw2d]) {
        ferrous_renderer::Renderer::draw_glyphs_2d(self, font, glyphs);
    }

    fn register_image(
        &mut self,
        width: u32,
        height: u32,
        rgba8: &[u8],
    ) -> Result<u32, RenderError> {
        self.register_texture_2d(
            width,
            height,
            rgba8,
            ferrous_2d::render::TextureOptions2d::photo(),
        )
        .map_err(|error| RenderError::Image {
            message: error.to_string(),
        })
    }

    fn update_image(
        &mut self,
        texture_id: u32,
        width: u32,
        height: u32,
        rgba8: &[u8],
    ) -> Result<(), RenderError> {
        ferrous_renderer::Renderer::update_sprite_texture(self, texture_id, width, height, rgba8)
            .map_err(|error| RenderError::Image {
                message: error.to_string(),
            })
    }

    fn free_image(&mut self, texture_id: u32) {
        ferrous_renderer::Renderer::free_sprite_texture(self, texture_id);
    }

    fn draw_image(&mut self, texture_id: u32, instance: ferrous_2d::render::SpriteInstance) {
        ferrous_renderer::Renderer::draw_sprite_2d(self, texture_id, instance);
    }

    fn draw_mesh(
        &mut self,
        texture_id: u32,
        vertices: &[ferrous_2d::render::MeshVertex2d],
        indices: &[u32],
        transform: glam::Mat4,
    ) {
        ferrous_renderer::Renderer::draw_textured_mesh_2d(
            self,
            texture_id,
            vertices,
            indices,
            transform,
            ferrous_2d::render::BlendMode2d::Normal,
        );
    }

    fn push_layer(
        &mut self,
        opacity: f64,
        blend: motioner_core::BlendMode,
        depth: f32,
        effects: &[ferrous_2d::render::Effect2d],
    ) -> bool {
        let opacity = space::to_f32(opacity);
        let desc = ferrous_2d::render::Layer2d::new(opacity, composite_blend(blend), depth);
        if effects.is_empty() {
            ferrous_renderer::Renderer::push_layer_2d(self, desc).is_ok()
        } else {
            ferrous_renderer::Renderer::push_layer_2d_with_effects(self, desc, effects).is_ok()
        }
    }

    fn push_masked_layer(
        &mut self,
        opacity: f64,
        blend: motioner_core::BlendMode,
        depth: f32,
        effects: &[ferrous_2d::render::Effect2d],
        mask: CanvasMask,
    ) -> bool {
        let opacity = space::to_f32(opacity);
        let desc = ferrous_2d::render::Layer2d::new(opacity, composite_blend(blend), depth);
        ferrous_renderer::Renderer::push_layer_2d_masked(self, desc, effects, engine_mask(mask))
            .is_ok()
    }

    fn begin_mask(&mut self) {
        let _ = ferrous_renderer::Renderer::begin_mask_2d(self);
    }

    fn end_mask(&mut self) {
        let _ = ferrous_renderer::Renderer::end_mask_2d(self);
    }

    fn pop_layer(&mut self) {
        let _ = ferrous_renderer::Renderer::pop_layer_2d(self);
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
    /// Roboto Regular, registered eagerly for selection bounds and fallback.
    font: FontId2d,
    /// Faces the document may use: project faces first, bundled last.
    fonts: FontSet,
    /// Registered engine handles, keyed by resolved family and weight.
    font_ids: Vec<((String, u16), FontId2d)>,
    adapter_name: String,
    clear: Option<wgpu::Color>,
    missing_glyphs: HashMap<motioner_core::NodeId, u32>,
    path_cache: paths::PathCache,
    images: ImageSet,
    image_textures: ImageTextures,
    /// Reused per-key target sizes for one frame's image pre-pass.
    image_targets: HashMap<motioner_core::ImageKey, (u32, u32)>,
    /// Upload size policy; the headless/export default is [`ProxyTarget::Full`].
    proxy: ProxyTarget,
    /// Reused buffer for a proxy downscale (never touched by `Full`).
    image_scratch: Vec<u8>,
    /// Inventory rows for this renderer's decoded-image CPU pixels.
    image_inventory: ImageInventory,
    /// Reused mesh vertex scratch and one index buffer per topology.
    mesh_vertices: Vec<ferrous_2d::render::MeshVertex2d>,
    mesh_indices: HashMap<(u32, u32), Vec<u32>>,
    /// Reused cull flags, open-layer stack and animated-text buffers.
    paint_scratch: frame::PaintScratch,
    texture_budget: u64,
    /// Reused across frame renders, so a preview does not rebuild the
    /// evaluator scratch or the plan's item buffers every call.
    evaluator: motioner_eval::Evaluator,
    plan: RenderPlan,
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
#[derive(Debug, Clone, Copy, PartialEq, Default)]
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

/// A drawing feature the pinned engine cannot render yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Feature {
    /// Styled text layout (D-0034). No longer raised: the pinned engine
    /// carries block layout. Kept so future pin gaps reuse the mechanism.
    TextLayout,
    /// Image textures (D-0035). No longer raised: the pinned engine carries
    /// `Renderer::register_texture_2d`. Kept so future pin gaps reuse it.
    Images,
    /// Temporal accumulation (D-0038). No longer raised: the pinned engine
    /// carries the accumulation API. Kept so future pin gaps reuse it.
    Accumulation,
}

impl Feature {
    /// Human name used in errors.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::TextLayout => "styled text layout",
            Self::Images => "image textures",
            Self::Accumulation => "temporal accumulation",
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
    /// A requested font family is in neither the project nor the bundled set.
    FontFamily(FontFamilyError),
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
    /// An image texture could not be uploaded.
    Image {
        /// Underlying texture error.
        message: String,
    },
    /// An image node's source was never resolved (missing or invalid file).
    UnresolvedImage {
        /// The project-relative path that failed.
        source: String,
    },
    /// The registered image textures exceed the configured budget.
    TextureBudget {
        /// Resident bytes.
        bytes: u64,
        /// Configured limit.
        limit: u64,
    },
    /// The engine accumulation pass failed (D-0038).
    Accumulation {
        /// Underlying typed error.
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
            Self::FontFamily(error) => write!(f, "{error}"),
            Self::Readback { message } => write!(f, "frame readback failed: {message}"),
            Self::Unsupported { feature, node } => match node {
                Some(node) => write!(
                    f,
                    "{} is not supported by the pinned engine yet (node {node}); see D-0034",
                    feature.name()
                ),
                None => write!(
                    f,
                    "{} is not supported by the pinned engine yet; see D-0034",
                    feature.name()
                ),
            },
            Self::Image { message } => write!(f, "image texture upload failed: {message}"),
            Self::UnresolvedImage { source } => write!(
                f,
                "image {source:?} could not be loaded; check that it is a project-relative PNG"
            ),
            Self::TextureBudget { bytes, limit } => write!(
                f,
                "image textures use {bytes} bytes, above the {limit}-byte budget"
            ),
            Self::Accumulation { message } => {
                write!(f, "temporal accumulation failed: {message}")
            }
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
        frame::create(width, height, FontSet::bundled())
    }

    /// Creates a headless renderer with additional project faces.
    ///
    /// Faces in `fonts` that share a family and weight with a bundled face win.
    /// The bytes are loaded by `motioner_io`; this crate never reads a path.
    ///
    /// # Errors
    ///
    /// Same as [`Self::new`].
    pub fn with_fonts(width: u32, height: u32, fonts: FontSet) -> Result<Self, RenderError> {
        frame::create(width, height, fonts)
    }

    /// The unresolved font set (project plus bundled faces).
    #[must_use]
    pub fn font_set(&self) -> &FontSet {
        &self.fonts
    }

    /// Replaces the decoded images this renderer may draw (D-0035).
    ///
    /// Call it after loading a document; textures are registered lazily on the
    /// first frame that uses each key and reused by key afterwards.  The CPU
    /// pixels stay resident: the headless path is the export path.
    pub fn set_images(&mut self, images: ImageSet) {
        self.image_inventory.unregister_all(&self.images);
        for image in images.iter().map(|(_, image)| image) {
            self.image_inventory.register(image);
        }
        self.images = images;
        // Dropping the ids without freeing them leaked one GPU texture per
        // image on every reload; the engine owns them until told otherwise.
        for (_, texture) in self.image_textures.drain() {
            self.harness.free_image(texture.id);
        }
    }

    /// Adds or replaces one decoded image without dropping the other
    /// textures. A live video calls this every displayed frame; a newer
    /// `revision` rewrites the same texture in place.
    pub fn set_image(&mut self, image: ImageData) {
        let key = image.key;
        self.image_inventory.unregister(&key);
        self.images.insert(image);
        self.image_textures.clear_missing_key(&key);
        self.image_textures.clear_failure(&key);
        if let Some(stored) = self.images.get(&key) {
            self.image_inventory.register(stored);
        }
    }

    /// The current upload size policy.
    #[must_use]
    pub const fn proxy_target(&self) -> ProxyTarget {
        self.proxy
    }

    /// Changes the upload size policy for images uploaded from now on.
    ///
    /// The headless renderer defaults to [`ProxyTarget::Full`] and the export
    /// path never changes it.  [`ProxyTarget::PerImage`] is available for a
    /// caller that draws into a view with a known scale.  Existing textures
    /// keep their size; replacing the image set, bumping a revision or a
    /// per-image target past the hysteresis applies the new policy.
    pub fn set_proxy_target(&mut self, target: ProxyTarget) {
        self.proxy = target;
    }

    /// Drops every CPU pixel copy whose GPU texture already exists.
    ///
    /// The headless and export paths must not call this: they are the
    /// documented owners of the decoded pixels.  It exists for an interactive
    /// offscreen renderer (the motion-blur preview) that wants the same
    /// release the live painter performs.
    pub fn release_image_cpu_pixels(&mut self) {
        let keys: Vec<motioner_core::ImageKey> = self.image_textures.keys().copied().collect();
        for key in keys {
            if self.images.drop_pixels(&key) {
                self.image_inventory.unregister(&key);
            }
        }
    }

    /// Image keys a `PerImage` render needed at a larger size after the CPU
    /// pixels had been released.
    ///
    /// The headless renderer normally keeps its pixels, and export uses
    /// [`ProxyTarget::Full`], so this stays empty on those paths.  It is the
    /// diagnostic counterpart of `LivePainter::missing_image_pixels` for a
    /// caller that released pixels from an offscreen renderer.
    #[must_use]
    pub fn missing_image_pixels(&self) -> &[motioner_core::ImageKey] {
        self.image_textures.missing()
    }

    /// The configured image-texture budget in bytes.
    #[must_use]
    pub fn texture_budget(&self) -> u64 {
        self.texture_budget
    }

    /// Sets the image-texture budget; a frame above it fails with
    /// [`RenderError::TextureBudget`].
    pub fn set_texture_budget(&mut self, bytes: u64) {
        self.texture_budget = bytes;
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

    /// Renders integer frame `frame`, honouring `comp(motion_blur=...)`.
    ///
    /// With motion blur off this is exactly [`Self::render`] on the integer
    /// frame. With it on, `samples` sub-frames are evaluated at centred
    /// sub-frame times and averaged in linear light by the engine.
    ///
    /// # Errors
    ///
    /// [`RenderError::Accumulation`] when the engine accumulation pass fails,
    /// plus every error [`Self::render`] can return.
    pub fn render_frame(
        &mut self,
        document: &Document,
        frame: motioner_core::Frame,
        out: &mut Vec<u8>,
    ) -> Result<FrameStats, RenderError> {
        frame::render_frame(self, document, frame, out)
    }

    /// Renders integer frame `frame` into the target without reading it back.
    ///
    /// The export loop uses this with [`Self::submit_readback`] and
    /// [`Self::take_readback`] to overlap the GPU copy with the next frame's
    /// render, so the GPU never waits for the CPU (prompt 18 P6). Motion blur
    /// must keep using [`Self::render_frame`].
    ///
    /// # Errors
    ///
    /// Every error [`Self::render`] can return. The target is not read back.
    pub fn render_frame_deferred(
        &mut self,
        document: &Document,
        frame: motioner_core::Frame,
    ) -> Result<FrameStats, RenderError> {
        frame::render_frame_deferred(self, document, frame)
    }

    /// Records the current target into the engine readback ring.
    ///
    /// The copy is submitted and mapped asynchronously; the mapped pixels are
    /// taken with [`Self::take_readback`], oldest first.
    ///
    /// # Errors
    ///
    /// [`RenderError::Readback`] when every ring slot is still in flight or the
    /// copy cannot be encoded.
    pub fn submit_readback(&mut self) -> Result<(), RenderError> {
        self.harness
            .submit_readback()
            .map(|_index| ())
            .map_err(|error| RenderError::Readback {
                message: error.to_string(),
            })
    }

    /// Copies the oldest submitted readback into `out`; `false` when none is
    /// pending yet.
    ///
    /// # Errors
    ///
    /// [`RenderError::Readback`] when the map failed for the oldest frame.
    pub fn take_readback(&mut self, out: &mut Vec<u8>) -> Result<bool, RenderError> {
        self.harness
            .take_readback(out)
            .map_err(|error| RenderError::Readback {
                message: error.to_string(),
            })
    }

    /// Renders `frame` through the motion-blur accumulation pass with an
    /// explicit sample count, regardless of the composition's own flag.
    ///
    /// The editor preview calls this while scrubbing and pauses (D-0037
    /// Part A); `still` and `render` use [`Self::render_frame`], which reads
    /// the setting from the composition.
    ///
    /// # Errors
    ///
    /// [`RenderError::Accumulation`] when the engine accumulation pass fails,
    /// plus every error [`Self::render`] can return.
    pub fn render_preview_frame(
        &mut self,
        document: &Document,
        frame: motioner_core::Frame,
        samples: u32,
        out: &mut Vec<u8>,
    ) -> Result<FrameStats, RenderError> {
        let mut evaluator = std::mem::take(&mut self.evaluator);
        let mut plan = std::mem::take(&mut self.plan);
        let stats = frame::render_motion_blur_samples(
            self,
            document,
            frame,
            samples,
            &mut evaluator,
            &mut plan,
            out,
        );
        self.evaluator = evaluator;
        self.plan = plan;
        stats
    }

    /// The name of the GPU adapter the engine selected.
    #[must_use]
    pub fn adapter_name(&self) -> &str {
        &self.adapter_name
    }
}

impl Drop for FrameRenderer {
    fn drop(&mut self) {
        self.image_inventory.unregister_all(&self.images);
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
    fonts: FontSet,
    font_ids: Vec<((String, u16), FontId2d)>,
    missing_glyphs: HashMap<motioner_core::NodeId, u32>,
    path_cache: paths::PathCache,
    images: ImageSet,
    image_textures: ImageTextures,
    /// Reused per-key target sizes for one paint's image pre-pass.
    image_targets: HashMap<motioner_core::ImageKey, (u32, u32)>,
    /// Upload size policy; the app opts into [`ProxyTarget::PerImage`].
    proxy: ProxyTarget,
    /// Composition-space region the current view shows (`[x, y, w, h]`).
    /// Items whose conservative bounds are entirely outside it are skipped.
    /// `None` draws every item (tests and any caller without a viewport).
    visible: Option<[f64; 4]>,
    /// Reused buffer for a proxy downscale.
    image_scratch: Vec<u8>,
    /// Textures whose image set was replaced, freed on the next paint.
    pending_image_frees: Vec<u32>,
    /// CPU budget for decoded-image pixels in the interactive preview.
    image_cpu_budget: u64,
    /// Reused key list for a budget release.
    image_drop_scratch: Vec<motioner_core::ImageKey>,
    /// Inventory rows for this painter's decoded-image CPU pixels.
    image_inventory: ImageInventory,
    /// Reused mesh vertex scratch and one index buffer per topology.
    mesh_vertices: Vec<ferrous_2d::render::MeshVertex2d>,
    mesh_indices: HashMap<(u32, u32), Vec<u32>>,
    /// Reused cull flags, open-layer stack and animated-text buffers.
    paint_scratch: frame::PaintScratch,
    /// Resolved document faces, cached by font-set revision and a hash of the
    /// document's text faces (prompt 18 P2/P3).
    faces_key: Option<(u64, u64)>,
    faces_cache: Vec<(String, u16)>,
    /// Bumped by [`Self::set_fonts`], so the face cache sees the new set.
    fonts_revision: u64,
}

#[cfg(feature = "live")]
impl LivePainter {
    /// Registers the embedded font with `renderer` and prepares a painter.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::Font`] when the font cannot be registered.
    pub fn new(renderer: &mut ferrous_renderer::Renderer) -> Result<Self, RenderError> {
        Self::with_fonts(renderer, FontSet::bundled())
    }

    /// [`Self::new`] with extra project faces (D-0034).
    ///
    /// Registers **every** face up front. Callers that want to pay for faces
    /// only when a document uses them should use [`Self::lazy`].
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::Font`] when a face cannot be registered.
    pub fn with_fonts(
        renderer: &mut ferrous_renderer::Renderer,
        fonts: FontSet,
    ) -> Result<Self, RenderError> {
        let mut painter = Self::with_fallback(renderer, fonts)?;
        for face in painter.fonts.all() {
            // One span per face: the MSDF bake is the single most expensive
            // thing the preview does before its first frame (prompt 17 S0/S1).
            let _span = ferrous_core::startup::span_with(|| {
                format!("motioner: font {} {}", face.family, face.weight)
            });
            let id = register_face(renderer, &face.family, face.weight, face.bytes.as_ref())?;
            painter
                .font_ids
                .push(((face.family.clone(), face.weight), id));
        }
        Ok(painter)
    }

    /// A painter that registers only its fallback face (prompt 17 S2).
    ///
    /// Composition faces stay unresolved until [`Self::paint`] meets a text
    /// node that uses them: [`Self::paint`] registers every face the visible
    /// text needs before it draws, so no frame can render with a substituted
    /// font. On a warm cache that registration is the atlas upload (a few
    /// milliseconds); a worker that already baked the face can skip the bake
    /// with [`Self::add_baked_face`].
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::Font`] when the fallback face cannot be
    /// registered.
    pub fn lazy(
        renderer: &mut ferrous_renderer::Renderer,
        fonts: FontSet,
    ) -> Result<Self, RenderError> {
        Self::with_fallback(renderer, fonts)
    }

    /// Builds the painter with no composition face registered yet.
    fn with_fallback(
        renderer: &mut ferrous_renderer::Renderer,
        fonts: FontSet,
    ) -> Result<Self, RenderError> {
        Ok(Self {
            font: register_face(renderer, "Roboto", 400, ROBOTO)?,
            fonts,
            font_ids: Vec::new(),
            missing_glyphs: HashMap::default(),
            path_cache: paths::PathCache::default(),
            images: ImageSet::new(),
            image_textures: ImageTextures::default(),
            image_targets: HashMap::default(),
            proxy: ProxyTarget::Full,
            visible: None,
            image_scratch: Vec::new(),
            pending_image_frees: Vec::new(),
            image_cpu_budget: DEFAULT_IMAGE_CPU_BUDGET,
            image_drop_scratch: Vec::new(),
            image_inventory: ImageInventory::new(),
            mesh_vertices: Vec::new(),
            mesh_indices: HashMap::default(),
            paint_scratch: frame::PaintScratch::default(),
            faces_key: None,
            faces_cache: Vec::new(),
            fonts_revision: 0,
        })
    }

    /// The font set this painter registered.
    #[must_use]
    pub fn font_set(&self) -> &FontSet {
        &self.fonts
    }

    /// Replaces the face set faces are resolved from (a newly opened document).
    ///
    /// Already registered atlases stay: the next paint registers whatever the
    /// new set resolves to and is still missing, so switching documents never
    /// re-uploads a face the engine already has.
    pub fn set_fonts(&mut self, fonts: FontSet) {
        self.faces_key = None;
        self.fonts_revision = self.fonts_revision.wrapping_add(1);
        self.fonts = fonts;
        // A new project may ship different bytes under the same family and
        // weight. The engine dedupes by font key, so registering again is
        // cheap and returns the right atlas instead of the previous project's.
        self.font_ids.clear();
    }

    /// Whether a resolved face is registered with the engine.
    #[must_use]
    pub fn face_registered(&self, family: &str, weight: u16) -> bool {
        self.font_ids
            .iter()
            .any(|((name, w), _)| *w == weight && name.eq_ignore_ascii_case(family))
    }

    /// The engine handle of a registered face.
    #[must_use]
    pub fn font_id(&self, family: &str, weight: u16) -> Option<ferrous_2d::render::FontId2d> {
        self.font_ids
            .iter()
            .find(|((name, w), _)| *w == weight && name.eq_ignore_ascii_case(family))
            .map(|(_, id)| *id)
    }

    /// Registers a face that was baked elsewhere (usually on a worker).
    ///
    /// The atlas is uploaded here, on the frame thread, and becomes the face
    /// the plan uses. Registering the same face twice is a no-op: the engine
    /// shares one atlas per [`ferrous_font::FontKey`].
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::Font`] when the atlas cannot be uploaded.
    pub fn add_baked_face(
        &mut self,
        renderer: &mut ferrous_renderer::Renderer,
        family: &str,
        weight: u16,
        baked: ferrous_font::BakedAtlas,
    ) -> Result<(), RenderError> {
        if self.face_registered(family, weight) {
            return Ok(());
        }
        let id = renderer
            .register_baked_font_2d(baked)
            .map_err(|error| RenderError::Font {
                message: error.to_string(),
            })?;
        self.font_ids.push(((family.to_owned(), weight), id));
        Ok(())
    }

    /// Registers the project faces the document needs but are not on the GPU
    /// yet, before anything is drawn.
    ///
    /// A family the font set does not have keeps the documented fallback to
    /// Roboto; a face the set *does* have is always registered first, which is
    /// what makes a wrong-face frame impossible.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::Font`] when a face cannot be registered.
    pub fn ensure_faces(
        &mut self,
        renderer: &mut ferrous_renderer::Renderer,
        document: &Document,
        _plan: &RenderPlan,
    ) -> Result<(), RenderError> {
        // The key covers both inputs `document_faces` reads: the document's
        // text faces (content hash, not a pointer, so a new document at a
        // recycled address cannot hit the old list) and the font set revision.
        let key = (self.fonts_revision, text_face_fingerprint(document));
        if self.faces_key != Some(key) {
            self.faces_cache = document_faces(document, &self.fonts);
            self.faces_key = Some(key);
        }
        for (family, weight) in &self.faces_cache {
            if self.face_registered(family, *weight) {
                continue;
            }
            let _span = ferrous_core::startup::span_with(|| {
                format!("motioner: font on demand {family} {weight}")
            });
            let face = self
                .fonts
                .resolve(family, *weight)
                .map_err(RenderError::FontFamily)?;
            let id = register_face(renderer, face.family, face.weight, face.bytes)?;
            self.font_ids
                .push(((face.family.to_owned(), face.weight), id));
        }
        Ok(())
    }

    /// Replaces the decoded images the preview may draw.
    ///
    /// The caller (the app) selects [`ProxyTarget::PerImage`] before the first
    /// paint and may release the CPU pixels once the textures exist; see
    /// [`Self::release_image_cpu_pixels`] and [`Self::missing_image_pixels`].
    pub fn set_images(&mut self, images: ImageSet) {
        self.image_inventory.unregister_all(&self.images);
        for image in images.iter().map(|(_, image)| image) {
            self.image_inventory.register(image);
        }
        self.images = images;
        // No renderer here: queue the old textures for the next paint.
        self.pending_image_frees
            .extend(self.image_textures.drain().map(|(_, texture)| texture.id));
    }

    /// Adds or replaces one image without dropping the other textures.
    pub fn set_image(&mut self, image: ImageData) {
        let key = image.key;
        self.image_inventory.unregister(&key);
        self.images.insert(image);
        self.image_textures.clear_missing_key(&key);
        self.image_textures.clear_failure(&key);
        if let Some(stored) = self.images.get(&key) {
            self.image_inventory.register(stored);
        }
    }

    /// The current upload size policy.
    #[must_use]
    pub const fn proxy_target(&self) -> ProxyTarget {
        self.proxy
    }

    /// Changes the upload size policy for images uploaded from now on.
    ///
    /// The canvas passes [`ProxyTarget::PerImage`] so each drawn image is
    /// uploaded at the texels its crop, scale, zoom and DPI resolve, and a
    /// zoom-in re-uploads a sharper texture past the hysteresis.  The
    /// headless renderer and the motion-blur preview keep
    /// [`ProxyTarget::Full`].  Existing textures keep their size until the
    /// next draw decides they are too small or too large.
    pub fn set_proxy_target(&mut self, target: ProxyTarget) {
        self.proxy = target;
    }

    /// Sets the composition-space region the view shows (`[x, y, width,
    /// height]`).
    ///
    /// [`Self::paint`] and [`Self::paint_visible`] skip an item only when its
    /// conservative bounds are *entirely* outside this rectangle; an item that
    /// intersects it is drawn exactly as before. `None` (the default) disables
    /// culling, which is what the tests and any caller without a viewport use.
    pub fn set_visible_composition_rect(&mut self, visible: Option<[f64; 4]>) {
        self.visible = visible;
    }

    /// Image keys whose texture needs a larger upload but whose CPU pixels
    /// were already released.
    ///
    /// The app fetches these off the frame thread and installs each one with
    /// [`Self::set_image`]; until then the painter keeps drawing the resident
    /// texture.  The list is rewritten on every paint, so a key disappears as
    /// soon as its pixels arrive or its texture is large enough.
    #[must_use]
    pub fn missing_image_pixels(&self) -> &[motioner_core::ImageKey] {
        self.image_textures.missing()
    }

    /// Whether `key` is a live frame (a video) whose pixels change every
    /// frame. Such a key is never a candidate for a background re-decode: the
    /// worker reads project images, not decoded video frames.
    #[must_use]
    pub fn is_dynamic_image(&self, key: &motioner_core::ImageKey) -> bool {
        self.image_textures.is_dynamic(key)
    }

    /// Remembers that a background re-decode of `key` failed.
    ///
    /// The key is not reported through [`Self::missing_image_pixels`] again
    /// until fresh pixels arrive for it ([`Self::set_image`]) or the image set
    /// is replaced ([`Self::set_images`]); this is what stops a
    /// permanently-unrecoverable key from spawning a worker on every paint.
    pub fn mark_image_reload_failed(&mut self, key: motioner_core::ImageKey) {
        self.image_textures
            .mark_failure(key, ReloadFailure::Unavailable);
    }

    /// The CPU budget for decoded-image pixels.
    #[must_use]
    pub const fn image_cpu_budget(&self) -> u64 {
        self.image_cpu_budget
    }

    /// Sets the CPU budget; [`DEFAULT_IMAGE_CPU_BUDGET`] is 256 MiB.
    pub fn set_image_cpu_budget(&mut self, bytes: u64) {
        self.image_cpu_budget = bytes;
    }

    /// Drops every CPU pixel copy whose GPU texture already exists.
    ///
    /// This is the interactive-preview release.  Whoever still needs the
    /// pixels must not call it:
    ///
    /// - **export** (`FrameRenderer::render*`) reads the renderer's own
    ///   `ImageSet`, never this painter's;
    /// - **readback** reads the rendered target, not the source images;
    /// - **re-pack** (a `revision` bump or a new image set) installs fresh
    ///   pixels before the upload, so dropped bytes are never reused; a static
    ///   image whose texture exists is not uploaded again;
    /// - **mesh UV sampling** reads only the UVs, which stay normalised.
    ///
    /// The GPU texture is untouched, so the next frame draws the same pixels.
    pub fn release_image_cpu_pixels(&mut self) {
        self.release_pixels(0);
    }

    /// Drops least-recently-used pixels until the CPU budget is met.
    fn enforce_image_cpu_budget(&mut self) {
        if self.images.total_bytes() <= self.image_cpu_budget {
            return;
        }
        self.release_pixels(self.image_cpu_budget);
    }

    fn release_pixels(&mut self, budget: u64) {
        let mut dropped = std::mem::take(&mut self.image_drop_scratch);
        self.images.evict_over_budget_into(
            budget,
            |key| self.image_textures.get(key).is_some(),
            &mut dropped,
        );
        for key in &dropped {
            self.image_inventory.unregister(key);
        }
        self.image_drop_scratch = dropped;
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
    ///
    /// Every face the plan's text uses is registered first (see
    /// [`Self::ensure_faces`]), so a lazily built painter never draws a
    /// substituted font, not even on its first frame.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::Font`] / [`RenderError::FontFamily`] when a face
    /// cannot be registered.
    pub fn paint(
        &mut self,
        renderer: &mut ferrous_renderer::Renderer,
        document: &Document,
        plan: &RenderPlan,
        fit: ViewFit,
    ) -> Result<PaintStats, RenderError> {
        if !fit.is_drawable() {
            return Ok(PaintStats::default());
        }
        for texture in self.pending_image_frees.drain(..) {
            renderer.free_sprite_texture(texture);
        }
        self.ensure_faces(renderer, document, plan)?;
        let flip = fit.transform(space::composition_flip(plan.width(), plan.height()));
        let gradients = {
            let _span = ferrous_core::startup::span("motioner: gradients");
            let gradients = gradients::GradientTable::build(plan, flip);
            renderer.set_2d_gradient_table(gradients.slots());
            gradients
        };
        // Live paint never fails on an image upload: a rejected texture draws
        // the placeholder and the next frame retries.  The headless path
        // propagates the error instead.
        let _ = frame::prepare_images(
            renderer,
            &self.images,
            &mut self.image_textures,
            &mut self.image_targets,
            plan,
            self.proxy,
            fit.scale(),
            None,
            self.visible,
            &mut self.image_scratch,
            &mut self.paint_scratch,
        );
        let stats = frame::paint_plan(
            renderer,
            document,
            plan,
            flip,
            self.font,
            &self.font_ids,
            &mut self.missing_glyphs,
            &mut self.path_cache,
            &gradients,
            &self.images,
            &self.image_textures,
            &mut self.mesh_vertices,
            &mut self.mesh_indices,
            None,
            self.visible,
            &mut self.paint_scratch,
        );
        self.enforce_image_cpu_budget();
        Ok(stats)
    }

    /// [`Self::paint`] that skips the given nodes and their subtrees.
    ///
    /// Used by the editor's solo/shy preview (D-0044). The render and still
    /// paths never call it, so editor state cannot change exported pixels.
    ///
    /// # Errors
    ///
    /// The same conditions as [`Self::paint`].
    pub fn paint_visible(
        &mut self,
        renderer: &mut ferrous_renderer::Renderer,
        document: &Document,
        plan: &RenderPlan,
        fit: ViewFit,
        hidden: &std::collections::BTreeSet<motioner_core::NodeId>,
    ) -> Result<PaintStats, RenderError> {
        if !fit.is_drawable() {
            return Ok(PaintStats::default());
        }
        for texture in self.pending_image_frees.drain(..) {
            renderer.free_sprite_texture(texture);
        }
        self.ensure_faces(renderer, document, plan)?;
        let flip = fit.transform(space::composition_flip(plan.width(), plan.height()));
        let gradients = {
            let _span = ferrous_core::startup::span("motioner: gradients");
            let gradients = gradients::GradientTable::build(plan, flip);
            renderer.set_2d_gradient_table(gradients.slots());
            gradients
        };
        let _ = frame::prepare_images(
            renderer,
            &self.images,
            &mut self.image_textures,
            &mut self.image_targets,
            plan,
            self.proxy,
            fit.scale(),
            Some(hidden),
            self.visible,
            &mut self.image_scratch,
            &mut self.paint_scratch,
        );
        let stats = frame::paint_plan(
            renderer,
            document,
            plan,
            flip,
            self.font,
            &self.font_ids,
            &mut self.missing_glyphs,
            &mut self.path_cache,
            &gradients,
            &self.images,
            &self.image_textures,
            &mut self.mesh_vertices,
            &mut self.mesh_indices,
            Some(hidden),
            self.visible,
            &mut self.paint_scratch,
        );
        self.enforce_image_cpu_budget();
        Ok(stats)
    }
}

#[cfg(feature = "live")]
impl Drop for LivePainter {
    fn drop(&mut self) {
        self.image_inventory.unregister_all(&self.images);
    }
}

/// A hash of every text face a document uses, in node order.
///
/// This is the document half of the [`LivePainter`] face-cache key.  It reads
/// only what `document_faces` reads (each text node's `font` and `weight`),
/// and hashes lengths between fields so two different node lists cannot
/// collide.
#[cfg(feature = "live")]
fn text_face_fingerprint(document: &Document) -> u64 {
    use std::hash::Hasher as _;
    let mut hasher = rustc_hash::FxHasher::default();
    for node in document.nodes() {
        let motioner_core::NodeContent::Text(content) = node.content() else {
            continue;
        };
        hasher.write_usize(content.font.len());
        hasher.write(content.font.as_bytes());
        hasher.write_u16(content.weight);
    }
    hasher.finish()
}

/// Registers one face with the engine, mapping its error to [`RenderError`].
#[cfg(feature = "live")]
fn register_face(
    renderer: &mut ferrous_renderer::Renderer,
    _family: &str,
    _weight: u16,
    bytes: &[u8],
) -> Result<ferrous_2d::render::FontId2d, RenderError> {
    renderer
        .register_font_2d(bytes, ferrous_font::charset::latin_western())
        .map_err(|error| RenderError::Font {
            message: error.to_string(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use motioner_core::ImageKey;

    fn image(marker: u8, pixels: u32) -> ImageData {
        ImageData {
            key: ImageKey::from_bytes([marker; 32]),
            width: pixels,
            height: 1,
            rgba8: vec![0; usize::try_from(pixels * 4).unwrap_or(0)],
            revision: 0,
        }
    }

    #[test]
    fn image_cpu_budget_evicts_least_recently_used_pixels() {
        let mut set = ImageSet::new();
        let (a, b, c) = (image(1, 1), image(2, 1), image(3, 1));
        let (ka, kb, kc) = (a.key, b.key, c.key);
        set.insert(a);
        set.insert(b);
        set.insert(c);
        // Touching `a` makes `b` the least recently used.
        assert!(set.get(&ka).is_some());
        let mut dropped = Vec::new();
        set.evict_over_budget_into(8, |_| true, &mut dropped);
        assert_eq!(dropped, vec![kb]);
        assert!(set.get(&ka).is_some_and(|image| !image.rgba8.is_empty()));
        assert!(set.get(&kb).is_some_and(|image| image.rgba8.is_empty()));
        assert!(set.get(&kc).is_some_and(|image| !image.rgba8.is_empty()));
        assert_eq!(set.total_bytes(), 8);
    }

    #[test]
    fn image_cpu_budget_never_evicts_an_unuploaded_image() {
        let mut set = ImageSet::new();
        let a = image(4, 1);
        let key = a.key;
        set.insert(a);
        let mut dropped = Vec::new();
        set.evict_over_budget_into(0, |_| false, &mut dropped);
        assert!(dropped.is_empty());
        assert!(set.get(&key).is_some_and(|image| !image.rgba8.is_empty()));
    }

    /// A canvas that records the texture calls instead of drawing.
    struct RecordingCanvas {
        next_id: u32,
        registered: Vec<(u32, u32, u32)>,
        updated: Vec<(u32, u32, u32)>,
        freed: Vec<u32>,
    }

    impl RecordingCanvas {
        fn new() -> Self {
            Self {
                next_id: 1,
                registered: Vec::new(),
                updated: Vec::new(),
                freed: Vec::new(),
            }
        }
    }

    impl Canvas2d for RecordingCanvas {
        fn shape(&mut self, _instance: ShapeInstance) {}

        fn missing_glyphs(&self, _font: FontId2d, _text: &str) -> u32 {
            0
        }

        fn layout_text(
            &self,
            _font: FontId2d,
            _text: &str,
            _style: ferrous_2d::render::TextStyle2d,
            _out: &mut Vec<ferrous_font::GlyphLayout>,
        ) -> Option<ferrous_font::TextMetrics> {
            None
        }

        fn text_styled(
            &mut self,
            _font: FontId2d,
            _text: &str,
            _transform: glam::Mat4,
            _style: ferrous_2d::render::TextStyle2d,
            _color: [f32; 4],
            _gradient: Option<u32>,
        ) {
        }

        fn glyphs(&mut self, _font: FontId2d, _glyphs: &[ferrous_2d::render::GlyphDraw2d]) {}

        fn register_image(
            &mut self,
            width: u32,
            height: u32,
            _rgba8: &[u8],
        ) -> Result<u32, RenderError> {
            let id = self.next_id;
            self.next_id += 1;
            self.registered.push((id, width, height));
            Ok(id)
        }

        fn update_image(
            &mut self,
            texture_id: u32,
            width: u32,
            height: u32,
            _rgba8: &[u8],
        ) -> Result<(), RenderError> {
            self.updated.push((texture_id, width, height));
            Ok(())
        }

        fn free_image(&mut self, texture_id: u32) {
            self.freed.push(texture_id);
        }

        fn draw_image(&mut self, _texture_id: u32, _instance: ferrous_2d::render::SpriteInstance) {}

        fn draw_mesh(
            &mut self,
            _texture_id: u32,
            _vertices: &[ferrous_2d::render::MeshVertex2d],
            _indices: &[u32],
            _transform: glam::Mat4,
        ) {
        }

        fn push_layer(
            &mut self,
            _opacity: f64,
            _blend: motioner_core::BlendMode,
            _depth: f32,
            _effects: &[ferrous_2d::render::Effect2d],
        ) -> bool {
            false
        }

        fn push_masked_layer(
            &mut self,
            _opacity: f64,
            _blend: motioner_core::BlendMode,
            _depth: f32,
            _effects: &[ferrous_2d::render::Effect2d],
            _mask: CanvasMask,
        ) -> bool {
            false
        }

        fn begin_mask(&mut self) {}

        fn end_mask(&mut self) {}

        fn pop_layer(&mut self) {}
    }

    fn pixel_image(width: u32, height: u32) -> ImageData {
        ImageData {
            key: ImageKey::from_bytes([7; 32]),
            width,
            height,
            rgba8: vec![200; usize::try_from(width * height * 4).unwrap_or(0)],
            revision: 0,
        }
    }

    #[test]
    fn a_live_frame_switches_to_natural_size_and_updates_in_place() {
        let mut canvas = RecordingCanvas::new();
        let mut textures = ImageTextures::default();
        let mut scratch = Vec::new();
        let mut image = pixel_image(400, 200);
        let key = image.key;

        // First frame: a proxy is fine for a still.
        let first = ensure_image_texture(
            &mut canvas,
            &mut textures,
            key,
            &image,
            (200, 100),
            &mut scratch,
        )
        .expect("upload succeeds")
        .expect("texture exists");
        assert_eq!(canvas.registered, vec![(first, 200, 100)]);

        // A revision change marks the key as a live frame: the texture moves
        // to natural size once, so later frames are not resampled per frame.
        image.revision = 1;
        let second = ensure_image_texture(
            &mut canvas,
            &mut textures,
            key,
            &image,
            (200, 100),
            &mut scratch,
        )
        .expect("upload succeeds")
        .expect("texture exists");
        assert_eq!(canvas.freed, vec![first]);
        assert_eq!(canvas.registered.last(), Some(&(second, 400, 200)));

        // Later frames are rewritten in place at natural size.
        image.revision = 2;
        let third = ensure_image_texture(
            &mut canvas,
            &mut textures,
            key,
            &image,
            (200, 100),
            &mut scratch,
        )
        .expect("update succeeds")
        .expect("texture exists");
        assert_eq!(third, second);
        assert_eq!(canvas.updated, vec![(second, 400, 200)]);
        assert_eq!(canvas.freed, vec![first]);
    }

    #[test]
    fn a_malformed_source_is_uploaded_once_and_not_again() {
        let mut canvas = RecordingCanvas::new();
        let mut textures = ImageTextures::default();
        let mut scratch = Vec::new();
        let mut image = pixel_image(8, 8);
        // A wrong RGBA length: `prepare_at` cannot resample and falls back to
        // the source, so the texture is the source size and is not re-uploaded
        // on every paint.
        image.rgba8.truncate(16);
        let key = image.key;

        let first = ensure_image_texture(
            &mut canvas,
            &mut textures,
            key,
            &image,
            (4, 4),
            &mut scratch,
        )
        .expect("upload succeeds")
        .expect("texture exists");
        assert_eq!(canvas.registered, vec![(first, 8, 8)]);

        let again = ensure_image_texture(
            &mut canvas,
            &mut textures,
            key,
            &image,
            (4, 4),
            &mut scratch,
        )
        .expect("reuse succeeds")
        .expect("texture exists");
        assert_eq!(again, first);
        assert_eq!(canvas.registered.len(), 1, "no per-frame re-upload");
        assert!(canvas.updated.is_empty() && canvas.freed.is_empty());
    }

    #[test]
    fn a_zoomed_in_image_reuploads_past_the_hysteresis_and_keeps_the_larger_one_on_zoom_out() {
        let mut canvas = RecordingCanvas::new();
        let mut textures = ImageTextures::default();
        let mut scratch = Vec::new();
        let mut image = pixel_image(400, 200);
        let key = image.key;

        let first = ensure_image_texture(
            &mut canvas,
            &mut textures,
            key,
            &image,
            (200, 100),
            &mut scratch,
        )
        .expect("register succeeds")
        .expect("first upload");
        assert_eq!(canvas.registered, vec![(first, 200, 100)]);
        assert!(textures.missing().is_empty());

        // Exactly 1.25x is the hysteresis boundary: reuse, no upload.
        let same = ensure_image_texture(
            &mut canvas,
            &mut textures,
            key,
            &image,
            (250, 125),
            &mut scratch,
        )
        .expect("reuse succeeds")
        .expect("texture exists");
        assert_eq!(same, first);
        assert_eq!(canvas.registered.len(), 1);
        assert!(canvas.updated.is_empty() && canvas.freed.is_empty());

        // Past 1.25x: free and register at the larger size.
        let larger = ensure_image_texture(
            &mut canvas,
            &mut textures,
            key,
            &image,
            (260, 130),
            &mut scratch,
        )
        .expect("grow succeeds")
        .expect("texture exists");
        assert_ne!(larger, first);
        assert_eq!(canvas.freed, vec![first]);
        assert_eq!(canvas.registered.last(), Some(&(larger, 260, 130)));
        assert!(canvas.updated.is_empty(), "a resize never updates in place");

        // A revision change means live pixels: the key becomes dynamic and the
        // texture moves to natural size once, so later frames are not
        // resampled to a proxy on the frame thread.
        image.revision = 1;
        let natural = ensure_image_texture(
            &mut canvas,
            &mut textures,
            key,
            &image,
            (260, 130),
            &mut scratch,
        )
        .expect("upload succeeds")
        .expect("texture exists");
        assert_ne!(natural, larger);
        assert_eq!(canvas.freed, vec![first, larger]);
        assert_eq!(canvas.registered.last(), Some(&(natural, 400, 200)));

        // The next live frame is rewritten in place, proxies ignored.
        image.revision = 2;
        let rewritten = ensure_image_texture(
            &mut canvas,
            &mut textures,
            key,
            &image,
            (120, 60),
            &mut scratch,
        )
        .expect("update succeeds")
        .expect("texture exists");
        assert_eq!(rewritten, natural);
        assert_eq!(canvas.updated, vec![(natural, 400, 200)]);
        assert_eq!(canvas.freed, vec![first, larger]);
    }

    #[test]
    fn a_released_pixel_buffer_is_reported_instead_of_uploaded() {
        let mut canvas = RecordingCanvas::new();
        let mut textures = ImageTextures::default();
        let mut scratch = Vec::new();
        let mut image = pixel_image(400, 200);
        let key = image.key;
        let id = ensure_image_texture(
            &mut canvas,
            &mut textures,
            key,
            &image,
            (400, 200),
            &mut scratch,
        )
        .expect("register succeeds")
        .expect("first upload");

        image.rgba8.clear();
        // The resident texture is still drawn while the pixels are fetched.
        let kept = ensure_image_texture(
            &mut canvas,
            &mut textures,
            key,
            &image,
            (800, 400),
            &mut scratch,
        )
        .expect("missing pixels are not an upload error")
        .expect("the previous texture is still usable");
        assert_eq!(kept, id);
        assert_eq!(textures.missing().len(), 1);
        assert_eq!(textures.missing()[0], key);
        assert_eq!(canvas.registered.len(), 1, "no blurry upscale upload");
        assert!(canvas.updated.is_empty());
    }

    #[test]
    fn a_dynamic_key_is_never_reported_missing() {
        let mut canvas = RecordingCanvas::new();
        let mut textures = ImageTextures::default();
        let mut scratch = Vec::new();
        let mut image = pixel_image(400, 200);
        let key = image.key;

        // The first upload is a still; a revision bump marks the key dynamic.
        ensure_image_texture(
            &mut canvas,
            &mut textures,
            key,
            &image,
            (400, 200),
            &mut scratch,
        )
        .expect("register succeeds");
        image.revision = 1;
        ensure_image_texture(
            &mut canvas,
            &mut textures,
            key,
            &image,
            (400, 200),
            &mut scratch,
        )
        .expect("live frame uploads");
        assert!(textures.is_dynamic(&key));

        // The preview released the pixels; a later live frame bumps the
        // revision again. The worker cannot decode a video frame, so the key
        // must not be reported (that would spawn a doomed worker every paint).
        image.revision = 2;
        image.rgba8.clear();
        let kept = ensure_image_texture(
            &mut canvas,
            &mut textures,
            key,
            &image,
            (400, 200),
            &mut scratch,
        )
        .expect("a missing live buffer is not an upload error")
        .expect("the resident texture is still usable");
        assert_eq!(Some(kept), canvas.registered.last().map(|(id, ..)| *id));
        assert!(
            textures.missing().is_empty(),
            "a dynamic key is never re-decodable"
        );
    }

    #[test]
    fn a_failed_key_is_not_requested_again_until_pixels_arrive() {
        let mut canvas = RecordingCanvas::new();
        let mut textures = ImageTextures::default();
        let mut scratch = Vec::new();
        let mut image = pixel_image(400, 200);
        let key = image.key;

        // Upload once, then release the pixels so a larger target reports it.
        ensure_image_texture(
            &mut canvas,
            &mut textures,
            key,
            &image,
            (400, 200),
            &mut scratch,
        )
        .expect("register succeeds");
        image.rgba8.clear();
        ensure_image_texture(
            &mut canvas,
            &mut textures,
            key,
            &image,
            (800, 400),
            &mut scratch,
        )
        .expect("released pixels are not an error");
        assert_eq!(textures.missing().len(), 1);
        assert_eq!(textures.missing()[0], key);

        // The worker failed: the key is cached and must stay out of `missing`
        // on every later paint, or the app would spawn a worker per paint.
        textures.mark_failure(key, ReloadFailure::Unavailable);
        assert_eq!(textures.failure(&key), Some(ReloadFailure::Unavailable));
        assert!(textures.missing().is_empty());

        textures.clear_missing();
        ensure_image_texture(
            &mut canvas,
            &mut textures,
            key,
            &image,
            (800, 400),
            &mut scratch,
        )
        .expect("a known failure is not an upload error");
        assert!(
            textures.missing().is_empty(),
            "a failed key is sticky across paints"
        );

        // Fresh pixels clear the failure, so a later release can retry.
        textures.clear_failure(&key);
        textures.clear_missing();
        ensure_image_texture(
            &mut canvas,
            &mut textures,
            key,
            &image,
            (800, 400),
            &mut scratch,
        )
        .expect("released pixels are not an error");
        assert_eq!(textures.missing().len(), 1);
        assert_eq!(textures.missing()[0], key);
    }

    #[test]
    fn replacing_the_image_set_clears_the_failure_cache() {
        let mut textures = ImageTextures::default();
        let key = ImageKey::from_bytes([9; 32]);
        textures.mark_failure(key, ReloadFailure::Dynamic);
        assert_eq!(textures.failure(&key), Some(ReloadFailure::Dynamic));
        let _ = textures.drain().count();
        assert_eq!(textures.failure(&key), None);
        assert!(textures.missing().is_empty());
    }

    #[test]
    fn the_failure_cache_is_bounded() {
        let mut textures = ImageTextures::default();
        for index in 0..u32::try_from(MAX_RELOAD_FAILURES + 32).unwrap_or(0) {
            let key = ImageKey::from_bytes({
                let mut bytes = [0_u8; 32];
                bytes[..4].copy_from_slice(&index.to_le_bytes());
                bytes
            });
            textures.mark_failure(key, ReloadFailure::Unavailable);
        }
        assert_eq!(textures.failed.len(), MAX_RELOAD_FAILURES);
    }

    #[cfg(feature = "live")]
    fn document(font: &str, weight: u16) -> motioner_core::Document {
        let source = format!(
            "comp(name=\"T\", width=100, height=100, fps=60, duration=1)\n\
             t = text(\"a\", \"{font}\", 20, \"#ffffff\", \"left\", {weight})\n"
        );
        motioner_dsl::run(&source, &motioner_dsl::Limits::default()).unwrap()
    }

    #[cfg(feature = "live")]
    #[test]
    fn the_face_cache_key_covers_the_document_faces() {
        let roboto = document("Roboto", 400);
        let inter = document("Inter", 400);
        let bold = document("Roboto", 700);
        assert_eq!(
            text_face_fingerprint(&roboto),
            text_face_fingerprint(&document("Roboto", 400))
        );
        assert_ne!(
            text_face_fingerprint(&roboto),
            text_face_fingerprint(&inter)
        );
        assert_ne!(text_face_fingerprint(&roboto), text_face_fingerprint(&bold));
    }
}

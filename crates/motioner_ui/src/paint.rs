//! Drawing primitives for the editor chrome on top of the engine's UI batch.
//!
//! [`Painter`] wraps one frame's `GuiBatch`, the UI font, the [`Theme`] and the
//! DPI [`Metrics`]. Every panel draws through it, so there is one place that
//! knows how quads, borders, shadows, hairlines, polygons and text are packed.
//!
//! Two engine helpers are bypassed on purpose: `GuiBatch::border` and
//! `GuiBatch::shadow` pack their parameters into fields that `gui.wgsl` does
//! not read (the shader takes the border width and the shadow blur from
//! `uv0.x`, and the shadow's unexpanded size from `color_b.xy`). The quads are
//! built here with the layout the shader actually uses.
//!
//! Anti-aliased polygons (play triangles, chevrons, keyframe diamonds) are
//! filled as one-pixel scanline quads: each quad's horizontal edges are
//! anti-aliased by the shader's rounded-rect SDF, which gives clean slanted
//! edges without the engine's aliased SVG mesh path.

use std::collections::HashMap;

use ferrous_font::Font;
use ferrous_gui::{
    GuiBatch, GuiQuad, Rect as ClipRect, BORDER_BIT, GRADIENT_BIT, GRADIENT_V_BIT, SHADOW_BIT,
};

use crate::layout::Rect;
use crate::text::{self, Align, TextStyle};
use crate::theme::{Metrics, Theme};

/// Changed area of one frame, in physical pixels.
///
/// A retained painter accumulates the rectangles of the panels it had to
/// rebuild. `full` means the whole window must be repainted, either because
/// the painter is uncached or because a cached panel disappeared.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FrameDamage {
    /// Rectangles the frame changed.
    pub rects: Vec<Rect>,
    /// Whether the whole window changed.
    pub full: bool,
}

impl FrameDamage {
    /// An empty damage set.
    #[must_use]
    pub fn new() -> Self {
        Self {
            rects: Vec::new(),
            full: false,
        }
    }

    /// Damaged fraction of a `width` x `height` window, clamped to `0.0..=1.0`.
    ///
    /// Overlapping rectangles are counted once, and rectangles are clipped to
    /// the window before the area is measured. `full` always returns `1.0`.
    #[must_use]
    pub fn damaged_fraction(&self, width: f32, height: f32) -> f64 {
        if self.full {
            return 1.0;
        }
        let window = f64::from(width.max(0.0)) * f64::from(height.max(0.0));
        if window <= 0.0 {
            return 0.0;
        }
        (union_area(&self.rects, width, height) / window).clamp(0.0, 1.0)
    }
}

/// The subsystem a [`PanelKey`] belongs to.
///
/// Every top-level panel has its own kind. Per-row caches use
/// [`PanelKind::Row`]; the row group they belong to is the enclosing panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PanelKind {
    /// Canvas frame, selection, guides and status cards.
    CanvasOverlays,
    /// Top bar.
    TopBar,
    /// Layers panel.
    Layers,
    /// Assets panel.
    Assets,
    /// Inspector panel.
    Inspector,
    /// Timeline.
    Timeline,
    /// Status bar.
    Status,
    /// File menu dropdown.
    FileMenu,
    /// View menu dropdown.
    ViewMenu,
    /// Add Layer menu dropdown.
    AddMenu,
    /// Convert-on-open banner.
    OverridesBanner,
    /// Pending-overrides list.
    OverridesList,
    /// Read-only code panel.
    CodePanel,
    /// Floating tooltip.
    Tooltip,
    /// One virtualised list row.
    Row,
}

/// Identity of one cached panel or row.
///
/// `revision` is the caller's hash of every input the panel reads (document
/// state, selection, hover, scroll offsets, strings, and so on). `rect` is
/// stored as raw `f32` bits so the key is `Eq`. `metrics` is the DPI scale
/// bits, `theme` is a hash of every theme colour and `font` is the atlas
/// identity whose glyph UVs are baked into text quads. The scissor leaving the
/// parent painter is folded into the cache slot, not this struct.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PanelKey {
    /// Which panel or row this is.
    pub kind: PanelKind,
    /// Hash of every input the panel reads.
    pub revision: u64,
    /// Panel rectangle as `[x, y, w, h]` `f32` bits.
    pub rect: [u32; 4],
    /// DPI scale as `f32` bits.
    pub metrics: u32,
    /// Hash of the theme colours.
    pub theme: u64,
    /// Hash of the font atlas identity and icon set.
    pub font: u64,
}

impl PanelKey {
    /// Builds a key for `kind` at `rect` with `revision`.
    ///
    /// The font identity is left at `0`; paint code builds keys through
    /// `Painter::panel_key` so a baked atlas change invalidates the cached
    /// text as well.
    #[must_use]
    pub fn new(kind: PanelKind, revision: u64, rect: Rect, theme: &Theme, m: Metrics) -> Self {
        Self {
            kind,
            revision,
            rect: [
                rect.x.to_bits(),
                rect.y.to_bits(),
                rect.w.to_bits(),
                rect.h.to_bits(),
            ],
            metrics: m.scale.to_bits(),
            theme: theme_identity(theme),
            font: 0,
        }
    }

    /// The rectangle this key was built from.
    #[must_use]
    pub fn rect(self) -> Rect {
        Rect::new(
            f32::from_bits(self.rect[0]),
            f32::from_bits(self.rect[1]),
            f32::from_bits(self.rect[2]),
            f32::from_bits(self.rect[3]),
        )
    }
}

/// FNV-1a revision builder used by the panels.
///
/// Every panel hashes each input it reads into one `u64`; a single changed
/// input changes the [`PanelKey`] and forces that panel to be rebuilt.
#[derive(Debug, Clone, Copy)]
pub struct PanelRevision(u64);

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

impl PanelRevision {
    /// A new revision.
    #[must_use]
    pub fn new() -> Self {
        Self(FNV_OFFSET)
    }

    /// Mixes a `u64`.
    #[must_use]
    pub fn u64(self, value: u64) -> Self {
        Self((self.0 ^ value).wrapping_mul(FNV_PRIME))
    }

    /// Mixes a `usize`.
    #[must_use]
    pub fn usize(self, value: usize) -> Self {
        self.u64(value as u64)
    }

    /// Mixes an `i64`.
    #[must_use]
    pub fn i64(self, value: i64) -> Self {
        self.u64(value as u64)
    }

    /// Mixes an `f32` by its bits.
    #[must_use]
    pub fn f32(self, value: f32) -> Self {
        self.u64(u64::from(value.to_bits()))
    }

    /// Mixes an `f64` by its bits.
    #[must_use]
    pub fn f64(self, value: f64) -> Self {
        self.u64(value.to_bits())
    }

    /// Mixes a `bool`.
    #[must_use]
    pub fn bool(self, value: bool) -> Self {
        self.u64(u64::from(value))
    }

    /// Mixes a string (length and bytes).
    #[must_use]
    pub fn str(mut self, value: &str) -> Self {
        self = self.usize(value.len());
        for byte in value.bytes() {
            self = self.u64(u64::from(byte));
        }
        self
    }

    /// Mixes a rectangle.
    #[must_use]
    pub fn rect(self, value: Rect) -> Self {
        self.f32(value.x).f32(value.y).f32(value.w).f32(value.h)
    }

    /// Mixes any standard-hashable value.
    #[must_use]
    pub fn hashable<T: std::hash::Hash>(self, value: &T) -> Self {
        use std::hash::Hasher;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        value.hash(&mut hasher);
        self.u64(hasher.finish())
    }

    /// The final revision.
    #[must_use]
    pub fn finish(self) -> u64 {
        self.0
    }
}

impl Default for PanelRevision {
    fn default() -> Self {
        Self::new()
    }
}

/// A key plus the scissor active when the panel starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct SlotKey {
    key: PanelKey,
    clip: u64,
}

/// One retained panel batch.
struct CachedPanel {
    batch: GuiBatch,
    last_seen: u64,
}

/// One retained row batch.
struct CachedRow {
    batch: GuiBatch,
    last_seen: u64,
}

/// The rows of one panel kind, kept across panel revisions so an unchanged
/// row replays even when the panel around it is rebuilt.
#[derive(Default)]
struct RowGroup {
    rows: HashMap<SlotKey, CachedRow>,
    last_seen: u64,
}

/// Retained panel and row batches for the editor chrome.
///
/// Call [`Self::begin_frame`] once per frame, then paint each panel through
/// [`Painter::panel`] or [`Painter::row`], then read [`Painter::damage`].
pub struct PanelCache {
    frame: u64,
    active: bool,
    panels: HashMap<SlotKey, CachedPanel>,
    groups: HashMap<PanelKind, RowGroup>,
    damage: FrameDamage,
    force_full: bool,
}

impl PanelCache {
    /// An empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self {
            frame: 0,
            active: false,
            panels: HashMap::new(),
            groups: HashMap::new(),
            damage: FrameDamage::new(),
            force_full: false,
        }
    }

    /// Starts a frame.
    ///
    /// Drops panels and row groups that were not painted in the previous
    /// frame, and marks the frame as fully damaged when a previously cached
    /// panel disappeared. Clears the accumulated damage of the new frame.
    pub fn begin_frame(&mut self) {
        self.active = true;
        self.frame = self.frame.wrapping_add(1);
        self.damage.rects.clear();
        self.damage.full = self.force_full;
        self.force_full = false;
        let frame = self.frame;
        let mut dropped: Vec<Rect> = Vec::new();
        self.panels.retain(|slot, entry| {
            let seen = entry.last_seen.saturating_add(1) >= frame;
            if !seen {
                dropped.push(slot.key.rect());
            }
            seen
        });
        self.groups
            .retain(|_, group| group.last_seen.saturating_add(1) >= frame);
        self.damage.rects.extend(dropped);
    }

    /// Drops every cached batch and damages the whole window.
    pub fn invalidate_all(&mut self) {
        self.panels.clear();
        self.groups.clear();
        self.damage.full = true;
        self.force_full = true;
    }

    /// The damage of the current frame.
    ///
    /// A panel whose key changed this frame leaves its previous entry behind
    /// until the next prune; that entry's rectangle is damaged here rather
    /// than turning the whole window into damage (a hover that rebuilds one
    /// panel must not report a full repaint).
    #[must_use]
    fn snapshot(&mut self) -> FrameDamage {
        let frame = self.frame;
        let stale: Vec<Rect> = self
            .panels
            .iter()
            .filter(|(_, entry)| entry.last_seen != frame)
            .map(|(slot, _)| slot.key.rect())
            .collect();
        self.damage.rects.extend(stale);
        self.damage.clone()
    }
}

impl Default for PanelCache {
    fn default() -> Self {
        Self::new()
    }
}

/// One frame's drawing context for the chrome.
pub struct Painter<'a> {
    batch: &'a mut GuiBatch,
    font: &'a Font,
    /// Colours.
    pub theme: &'a Theme,
    /// DPI metrics.
    pub m: Metrics,
    cache: Option<&'a mut PanelCache>,
    rows: Option<&'a mut HashMap<SlotKey, CachedRow>>,
    row_frame: u64,
    theme_hash: u64,
    font_hash: u64,
}

impl<'a> Painter<'a> {
    /// Wraps a batch for one frame, uncached.
    ///
    /// Every [`Self::panel`] call paints directly and [`Self::damage`]
    /// reports the whole window.
    pub fn new(batch: &'a mut GuiBatch, font: &'a Font, theme: &'a Theme, m: Metrics) -> Self {
        Self {
            batch,
            font,
            theme,
            m,
            cache: None,
            rows: None,
            row_frame: 0,
            theme_hash: theme_identity(theme),
            font_hash: font_identity(font),
        }
    }

    /// Wraps a batch and its retained panel cache.
    #[must_use]
    pub fn cached(
        batch: &'a mut GuiBatch,
        font: &'a Font,
        theme: &'a Theme,
        m: Metrics,
        cache: &'a mut PanelCache,
    ) -> Self {
        Self {
            batch,
            font,
            theme,
            m,
            cache: Some(cache),
            rows: None,
            row_frame: 0,
            theme_hash: theme_identity(theme),
            font_hash: font_identity(font),
        }
    }

    /// Draws one panel, replaying its retained batch when its key is unchanged.
    ///
    /// On a miss the closure paints into a batch of its own that starts with
    /// the painter's active scissor. Clips pushed inside the closure must be
    /// balanced; one that leaks forces a full repaint. The result is appended
    /// to the frame batch with [`GuiBatch::extend`] and retained under `key`
    /// plus the entering scissor.
    #[allow(clippy::needless_pass_by_value)] // the key is small, `Copy` and the public contract
    pub fn panel(&mut self, key: PanelKey, paint: impl FnOnce(&mut Painter<'_>)) {
        if self.cache.is_none() {
            paint(self);
            return;
        }
        if !self.cache.as_deref().is_some_and(|cache| cache.active) {
            if let Some(cache) = self.cache.as_deref_mut() {
                cache.damage.full = true;
            }
            paint(self);
            return;
        }
        let font = self.font;
        let theme = self.theme;
        let m = self.m;
        let theme_hash = self.theme_hash;
        let font_hash = self.font_hash;
        let clip = clip_identity(self.batch);
        let slot = SlotKey { key, clip };
        if let Some(cache) = self.cache.as_deref_mut() {
            if let Some(entry) = cache.panels.get_mut(&slot) {
                let frame = cache.frame;
                entry.last_seen = frame;
                if let Some(group) = cache.groups.get_mut(&key.kind) {
                    group.last_seen = frame;
                }
                let replay = &entry.batch;
                self.batch.extend_from(replay);
                return;
            }
        }
        let Some(cache) = self.cache.as_deref_mut() else {
            return;
        };
        let frame = cache.frame;
        let mut group = cache.groups.remove(&key.kind).unwrap_or_default();
        group.last_seen = frame;
        let entering = self.batch.current_scissor;
        // Reuse the slot's previous batch as the paint scratch: its buffers are
        // already allocated at the old size, so a miss allocates nothing when
        // the panel's primitive count is stable.
        let mut fresh = cache
            .panels
            .remove(&slot)
            .map_or_else(GuiBatch::new, |entry| entry.batch);
        fresh.clear();
        fresh.current_scissor = entering;
        if let Some(clip) = entering {
            fresh.scissor_stack.push(clip);
        }
        {
            let mut child = Painter {
                batch: &mut fresh,
                font,
                theme,
                m,
                cache: None,
                rows: Some(&mut group.rows),
                row_frame: frame,
                theme_hash,
                font_hash,
            };
            paint(&mut child);
        }
        let balanced = fresh.current_scissor == entering
            && fresh.scissor_stack.len() == usize::from(entering.is_some());
        let damage = fresh.damage_union.map_or_else(
            || key.rect(),
            |rect| Rect::new(rect.x, rect.y, rect.width, rect.height),
        );
        group.rows.retain(|_, row| row.last_seen == frame);
        cache.groups.insert(key.kind, group);
        cache.damage.rects.push(damage);
        if !balanced {
            cache.damage.full = true;
            fresh.current_scissor = entering;
            fresh.scissor_stack.clear();
            if let Some(clip) = entering {
                fresh.scissor_stack.push(clip);
            }
        }
        fresh.damage_union = None;
        cache.panels.insert(
            slot,
            CachedPanel {
                batch: fresh,
                last_seen: frame,
            },
        );
        let stored = &cache.panels[&slot].batch;
        self.batch.extend_from(stored);
    }

    /// Draws one virtualised row, replaying it when its key is unchanged.
    ///
    /// Must be called while a panel is being painted. Rows entirely outside
    /// the painter's active scissor are culled without painting. The clip
    /// rules are the same as [`Self::panel`].
    #[allow(clippy::needless_pass_by_value)] // the key is small, `Copy` and the public contract
    pub fn row(&mut self, key: PanelKey, paint: impl FnOnce(&mut Painter<'_>)) {
        if self.rows.is_none() {
            paint(self);
            return;
        }
        let clip = clip_identity(self.batch);
        if let Some(scissor) = self.batch.current_scissor {
            let clip_rect = Rect::new(scissor.x, scissor.y, scissor.width, scissor.height);
            if !key.rect().overlaps(clip_rect) {
                return;
            }
        }
        let frame = self.row_frame;
        let font = self.font;
        let theme = self.theme;
        let m = self.m;
        let theme_hash = self.theme_hash;
        let font_hash = self.font_hash;
        let entering = self.batch.current_scissor;
        let slot = SlotKey { key, clip };
        let Some(rows) = self.rows.as_deref_mut() else {
            return;
        };
        if let Some(entry) = rows.get_mut(&slot) {
            entry.last_seen = frame;
            let replay = &entry.batch;
            self.batch.extend_from(replay);
            return;
        }
        let mut fresh = rows
            .remove(&slot)
            .map_or_else(GuiBatch::new, |entry| entry.batch);
        fresh.clear();
        fresh.current_scissor = entering;
        if let Some(clip) = entering {
            fresh.scissor_stack.push(clip);
        }
        {
            let mut child = Painter {
                batch: &mut fresh,
                font,
                theme,
                m,
                cache: None,
                rows: None,
                row_frame: 0,
                theme_hash,
                font_hash,
            };
            paint(&mut child);
        }
        let balanced = fresh.current_scissor == entering
            && fresh.scissor_stack.len() == usize::from(entering.is_some());
        if balanced {
            fresh.damage_union = None;
            rows.insert(
                slot,
                CachedRow {
                    batch: fresh,
                    last_seen: frame,
                },
            );
            let stored = &rows[&slot].batch;
            self.batch.extend_from(stored);
        } else {
            self.batch.extend(fresh);
        }
    }

    /// The damage accumulated this frame.
    #[must_use]
    pub fn damage(&mut self) -> FrameDamage {
        match self.cache.as_deref_mut() {
            Some(cache) => cache.snapshot(),
            None => FrameDamage {
                rects: Vec::new(),
                full: true,
            },
        }
    }

    /// Builds a panel key with this painter's theme, DPI and font identity.
    #[must_use]
    pub fn panel_key(&self, kind: PanelKind, revision: u64, rect: Rect) -> PanelKey {
        let mut key = PanelKey::new(kind, revision, rect, self.theme, self.m);
        key.theme = self.theme_hash;
        key.font = self.font_hash;
        key
    }

    /// Draws a decoded RGBA8 image as a sampled grid of solid cells.
    ///
    /// A small grid (at most `columns x rows`) reads as a real thumbnail at
    /// timeline scale without one engine draw call per source pixel. The grid
    /// stays the golden path: [`Painter`] holds no device or queue handle, so a
    /// single textured quad needs a GPU texture the caller uploads (see the
    /// retain/thumbnail hand-off). Row and panel caches keep this off the
    /// steady-state path: it only runs when a thumbnail's row is rebuilt.
    #[allow(clippy::too_many_arguments)] // one thumbnail's sampling inputs
    pub fn image_grid(
        &mut self,
        rect: Rect,
        width: u32,
        height: u32,
        rgba8: &[u8],
        columns: u32,
        rows: u32,
        alpha: f32,
    ) {
        if width == 0 || height == 0 || columns == 0 || rows == 0 || rect.is_empty() {
            return;
        }
        if rgba8.len() < usize::try_from(width * height * 4).unwrap_or(usize::MAX) {
            return;
        }
        let cell_w = rect.w / columns as f32;
        let cell_h = rect.h / rows as f32;
        for row in 0..rows {
            for column in 0..columns {
                #[allow(clippy::cast_precision_loss)]
                let sx = ((column as f32 + 0.5) / columns as f32 * width as f32) as u32;
                #[allow(clippy::cast_precision_loss)]
                let sy = ((row as f32 + 0.5) / rows as f32 * height as f32) as u32;
                let index = usize::try_from((sy.min(height - 1) * width + sx.min(width - 1)) * 4)
                    .unwrap_or(0);
                let Some(pixel) = rgba8.get(index..index + 4) else {
                    continue;
                };
                let color =
                    crate::theme::Srgb(u32::from_be_bytes([0, pixel[0], pixel[1], pixel[2]]))
                        .alpha(alpha * f32::from(pixel[3]) / 255.0);
                self.fill(
                    Rect::new(
                        rect.x + column as f32 * cell_w,
                        rect.y + row as f32 * cell_h,
                        cell_w.ceil(),
                        cell_h.ceil(),
                    ),
                    color,
                );
            }
        }
    }

    /// Starts a new layer: everything drawn afterwards (text included) is
    /// painted above everything drawn before.
    pub fn layer(&mut self) {
        self.batch.begin_layer();
    }

    /// Restricts drawing to `rect` until the matching [`Self::pop_clip`].
    pub fn push_clip(&mut self, rect: Rect) {
        self.batch.push_clip(ClipRect {
            x: rect.x,
            y: rect.y,
            width: rect.w,
            height: rect.h,
        });
    }

    /// Ends the innermost clip.
    pub fn pop_clip(&mut self) {
        self.batch.pop_clip();
    }

    /// Fills a rectangle.
    pub fn fill(&mut self, rect: Rect, color: [f32; 4]) {
        self.fill_rounded(rect, color, 0.0);
    }

    /// Fills a rounded rectangle; the radius is clamped to half the short side.
    pub fn fill_rounded(&mut self, rect: Rect, color: [f32; 4], radius: f32) {
        if rect.is_empty() || color[3] <= 0.0 {
            return;
        }
        let radius = radius.clamp(0.0, rect.w.min(rect.h) * 0.5);
        self.batch
            .push_quad(quad(rect, color, [radius; 4], [0.0; 2], [0.0; 4], 0));
    }

    /// Strokes the inside of a rounded rectangle with a `width`-pixel border.
    pub fn stroke_rounded(&mut self, rect: Rect, color: [f32; 4], radius: f32, width: f32) {
        if rect.is_empty() || width <= 0.0 || color[3] <= 0.0 {
            return;
        }
        let radius = radius.clamp(0.0, rect.w.min(rect.h) * 0.5);
        self.batch.push_quad(quad(
            rect,
            color,
            [radius; 4],
            [width, 0.0],
            [0.0; 4],
            BORDER_BIT,
        ));
    }

    /// A soft drop shadow under `rect`, offset down by `offset_y`.
    pub fn shadow(&mut self, rect: Rect, radius: f32, blur: f32, offset_y: f32, color: [f32; 4]) {
        if rect.is_empty() || blur <= 0.0 {
            return;
        }
        let expand = blur * 4.0;
        let outer = Rect::new(
            rect.x - expand,
            rect.y + offset_y - expand,
            rect.w + expand * 2.0,
            rect.h + expand * 2.0,
        );
        let radius = radius.clamp(0.0, rect.w.min(rect.h) * 0.5);
        self.batch.push_quad(quad(
            outer,
            color,
            [radius; 4],
            [blur, 0.0],
            [rect.w, rect.h, 0.0, 0.0],
            SHADOW_BIT,
        ));
    }

    /// A vertical gradient from `top` to `bottom` colour over `rect`, used to
    /// fade content into a panel edge.
    pub fn gradient_v(&mut self, rect: Rect, top: [f32; 4], bottom: [f32; 4]) {
        if rect.is_empty() {
            return;
        }
        self.batch.push_quad(quad(
            rect,
            top,
            [0.0; 4],
            [0.0; 2],
            bottom,
            GRADIENT_BIT | GRADIENT_V_BIT,
        ));
    }

    /// A horizontal hairline from `x0` to `x1` with its top at `y`.
    pub fn hline(&mut self, x0: f32, x1: f32, y: f32, color: [f32; 4]) {
        let t = self.m.hairline();
        self.fill(Rect::new(x0, y, x1 - x0, t), color);
    }

    /// A vertical hairline from `y0` to `y1` with its left edge at `x`.
    pub fn vline(&mut self, x: f32, y0: f32, y1: f32, color: [f32; 4]) {
        let t = self.m.hairline();
        self.fill(Rect::new(x, y0, t, y1 - y0), color);
    }

    /// One anti-aliased line segment with round caps.
    ///
    /// Packs the engine's `LINE_BIT` quad: the engine computes the exact
    /// fragment distance to the segment, so diagonals are anti-aliased without
    /// stair-stepping through axis-aligned quads.
    pub fn line(&mut self, from: (f32, f32), to: (f32, f32), width: f32, color: [f32; 4]) {
        self.batch
            .line([from.0, from.1], [to.0, to.1], width, color);
    }

    /// A filled circle.
    pub fn circle(&mut self, cx: f32, cy: f32, radius: f32, color: [f32; 4]) {
        let d = radius * 2.0;
        self.fill_rounded(Rect::new(cx - radius, cy - radius, d, d), color, radius);
    }

    /// A circle outline `width` pixels thick, inside `radius`.
    pub fn ring(&mut self, cx: f32, cy: f32, radius: f32, width: f32, color: [f32; 4]) {
        let d = radius * 2.0;
        self.stroke_rounded(
            Rect::new(cx - radius, cy - radius, d, d),
            color,
            radius,
            width,
        );
    }

    /// Fills a convex polygon given in pixels (any winding).
    ///
    /// The polygon is cut into one-pixel rows; each row is a quad spanning the
    /// polygon's extent at the row centre, so slanted edges are anti-aliased
    /// horizontally by the quad shader. Partially covered top and bottom rows
    /// are faded by their vertical coverage.
    pub fn polygon(&mut self, points: &[(f32, f32)], color: [f32; 4]) {
        if points.len() < 3 {
            return;
        }
        let (min_y, max_y) = points
            .iter()
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), p| {
                (lo.min(p.1), hi.max(p.1))
            });
        if !(min_y.is_finite() && max_y.is_finite()) || max_y <= min_y {
            return;
        }
        let first = min_y.floor() as i32;
        let last = max_y.ceil() as i32;
        for row in first..last {
            let top = row as f32;
            let coverage = (max_y.min(top + 1.0) - min_y.max(top)).clamp(0.0, 1.0);
            let sample = top + 0.5;
            let sample = sample.clamp(min_y + 1e-3, max_y - 1e-3);
            let Some((x0, x1)) = span_at(points, sample) else {
                continue;
            };
            let mut row_color = color;
            row_color[3] *= coverage;
            self.fill(Rect::new(x0, top, x1 - x0, 1.0), row_color);
        }
    }

    /// Draws a keyframe diamond inside `rect`: filled for a solid key,
    /// outlined for a key between keys, an inherited key or an unselected
    /// marker.
    pub fn diamond(&mut self, rect: Rect, color: [f32; 4], filled: bool) {
        let cx = rect.center_x();
        let cy = rect.center_y();
        let rx = rect.w * 0.5;
        let ry = rect.h * 0.5;
        let points = [(cx, cy - ry), (cx + rx, cy), (cx, cy + ry), (cx - rx, cy)];
        if filled {
            self.polygon(&points, color);
            return;
        }
        let w = self.m.hairline().max(1.0);
        let steps = (rx.max(ry) * 2.0).ceil().max(2.0) as i32;
        for (a, b) in [(0, 1), (1, 2), (2, 3), (3, 0)] {
            for step in 0..=steps {
                let t = step as f32 / steps as f32;
                let (x0, y0) = points[a];
                let (x1, y1) = points[b];
                let x = x0 + (x1 - x0) * t;
                let y = y0 + (y1 - y0) * t;
                self.fill(
                    Rect::new((x - w * 0.5).round(), (y - w * 0.5).round(), w, w),
                    color,
                );
            }
        }
    }

    /// Draws text vertically centred in `rect`, ellipsized to fit.
    pub fn text_in(&mut self, text: &str, rect: Rect, align: Align, style: &TextStyle) {
        text::draw_in(self.batch, self.font, text, rect, align, style);
    }

    /// Width of `text` in physical pixels.
    #[must_use]
    pub fn measure(&self, text: &str, style: &TextStyle) -> f32 {
        text::advance(self.font, text, style)
    }

    /// A text style at a logical size in a colour.
    #[must_use]
    pub fn style(&self, logical_size: f32, color: [f32; 4]) -> TextStyle {
        TextStyle::new(self.m.font(logical_size), color)
    }
}

/// Area of the union of `rects`, clipped to `width` x `height`.
fn union_area(rects: &[Rect], width: f32, height: f32) -> f64 {
    let mut xs: Vec<f32> = Vec::with_capacity(rects.len() * 2);
    let mut clipped: Vec<Rect> = Vec::with_capacity(rects.len());
    for rect in rects {
        let x0 = rect.x.max(0.0);
        let y0 = rect.y.max(0.0);
        let x1 = rect.right().min(width);
        let y1 = rect.bottom().min(height);
        if x1 > x0 && y1 > y0 {
            clipped.push(Rect::new(x0, y0, x1 - x0, y1 - y0));
            xs.push(x0);
            xs.push(x1);
        }
    }
    if clipped.is_empty() {
        return 0.0;
    }
    xs.sort_by(f32::total_cmp);
    xs.dedup();
    let mut area = 0.0_f64;
    for pair in xs.windows(2) {
        let (x0, x1) = (pair[0], pair[1]);
        if x1 <= x0 {
            continue;
        }
        let mut spans: Vec<(f32, f32)> = Vec::new();
        for rect in &clipped {
            if rect.x < x1 && rect.right() > x0 {
                spans.push((rect.y, rect.bottom()));
            }
        }
        spans.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut covered = 0.0_f32;
        let mut current: Option<(f32, f32)> = None;
        for (y0, y1) in spans {
            match current {
                None => current = Some((y0, y1)),
                Some((top, bottom)) if y0 <= bottom => current = Some((top, bottom.max(y1))),
                Some((top, bottom)) => {
                    covered += bottom - top;
                    current = Some((y0, y1));
                }
            }
        }
        if let Some((top, bottom)) = current {
            covered += bottom - top;
        }
        area += f64::from(x1 - x0) * f64::from(covered);
    }
    area
}

/// Hash of every theme colour, so a theme swap invalidates every panel.
fn theme_identity(theme: &Theme) -> u64 {
    let mut rev = PanelRevision::new();
    for color in [
        theme.pasteboard,
        theme.app,
        theme.panel,
        theme.raised,
        theme.control,
        theme.control_hover,
        theme.control_pressed,
        theme.row_hover,
        theme.hairline,
        theme.border,
        theme.text,
        theme.text_body,
        theme.text_muted,
        theme.text_disabled,
        theme.accent,
        theme.accent_hover,
        theme.selection,
        theme.hover_outline,
        theme.danger,
        theme.danger_soft,
        theme.warning,
        theme.success,
        theme.keyframe,
        theme.track_bar,
        theme.track_bar_selected,
        theme.track_bar_hidden,
        theme.sub_row,
        theme.timeline_curve,
        theme.shadow,
        theme.tooltip,
    ] {
        for channel in color {
            rev = rev.f32(channel);
        }
    }
    rev.finish()
}

/// Hash of the font atlas identity and icon set.
///
/// The atlas content key changes when a face is rebaked, so every cached text
/// run is rebuilt with the new glyph UVs.
fn font_identity(font: &Font) -> u64 {
    let key = font.atlas.key.raw();
    let rev = PanelRevision::new().u64(key as u64).u64((key >> 64) as u64);
    let icons = font
        .icons
        .as_ref()
        .map_or(0, |icons| icons.icons.len() as u64 + 1);
    rev.u64(icons).finish()
}

/// Hash of the painter's active scissor.
fn clip_identity(batch: &GuiBatch) -> u64 {
    let mut hash = FNV_OFFSET;
    match batch.current_scissor {
        Some(rect) => {
            hash = (hash ^ 1).wrapping_mul(FNV_PRIME);
            for value in [rect.x, rect.y, rect.width, rect.height] {
                hash = (hash ^ u64::from(value.to_bits())).wrapping_mul(FNV_PRIME);
            }
        }
        None => hash = hash.wrapping_mul(FNV_PRIME),
    }
    hash
}

fn quad(
    rect: Rect,
    color: [f32; 4],
    radii: [f32; 4],
    uv0: [f32; 2],
    color_b: [f32; 4],
    flags: u32,
) -> GuiQuad {
    GuiQuad {
        pos: [rect.x, rect.y],
        size: [rect.w, rect.h],
        uv0,
        uv1: [1.0, 1.0],
        color,
        color_b,
        radii,
        tex_index: 0,
        flags,
        z_order: 0.0,
        node_id: 0,
    }
}

/// The horizontal extent of a convex polygon at height `y`.
fn span_at(points: &[(f32, f32)], y: f32) -> Option<(f32, f32)> {
    let mut lo = f32::INFINITY;
    let mut hi = f32::NEG_INFINITY;
    for (i, &(ax, ay)) in points.iter().enumerate() {
        let (bx, by) = points[(i + 1) % points.len()];
        let (y0, y1) = if ay < by { (ay, by) } else { (by, ay) };
        if y < y0 || y > y1 || (y1 - y0).abs() < f32::EPSILON {
            continue;
        }
        let t = (y - ay) / (by - ay);
        let x = ax + t * (bx - ax);
        lo = lo.min(x);
        hi = hi.max(x);
    }
    (hi > lo).then_some((lo, hi))
}

#[cfg(test)]
mod tests {
    use super::{
        span_at, CachedPanel, FrameDamage, PanelCache, PanelKey, PanelKind, PanelRevision, SlotKey,
    };
    use crate::layout::Rect;
    use crate::theme::{Metrics, Theme};
    use ferrous_gui::GuiBatch;

    #[test]
    fn frame_damage_counts_overlapping_rects_once() {
        let mut damage = FrameDamage::new();
        damage.rects.push(Rect::new(0.0, 0.0, 100.0, 100.0));
        damage.rects.push(Rect::new(50.0, 50.0, 100.0, 100.0));
        assert!((damage.damaged_fraction(200.0, 200.0) - 0.4375).abs() < 1e-9);
    }

    #[test]
    fn clipped_and_full_damage_are_clamped() {
        let mut damage = FrameDamage::new();
        damage.rects.push(Rect::new(-100.0, -100.0, 150.0, 150.0));
        assert!((damage.damaged_fraction(100.0, 100.0) - 0.25).abs() < 1e-9);
        damage.full = true;
        assert!((damage.damaged_fraction(100.0, 100.0) - 1.0).abs() < 1e-9);
        assert!((damage.damaged_fraction(0.0, 0.0) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn invalidating_marks_the_next_frame_full() {
        let mut cache = PanelCache::new();
        cache.begin_frame();
        cache.invalidate_all();
        assert!(cache.snapshot().full);
        cache.begin_frame();
        assert!(cache.snapshot().full);
    }

    #[test]
    fn a_panel_that_disappears_damages_its_own_rect() {
        let theme = Theme::dark();
        let mut cache = PanelCache::new();
        cache.begin_frame();
        let key = PanelKey::new(
            PanelKind::Layers,
            1,
            Rect::new(0.0, 0.0, 10.0, 10.0),
            &theme,
            Metrics::new(1.0),
        );
        let slot = SlotKey { key, clip: 0 };
        cache.panels.insert(
            slot,
            CachedPanel {
                batch: GuiBatch::new(),
                last_seen: cache.frame,
            },
        );
        assert!(!cache.snapshot().full);
        cache.begin_frame();
        let stale = cache.snapshot();
        assert!(
            !stale.full,
            "a panel that was not painted damages only its own area, never the window"
        );
        assert!(
            stale.rects.contains(&Rect::new(0.0, 0.0, 10.0, 10.0)),
            "the stale panel's previous rectangle is damaged"
        );
        cache.begin_frame();
        let dropped = cache.snapshot();
        assert!(!dropped.full);
        cache.begin_frame();
        assert!(
            cache.snapshot().rects.is_empty(),
            "once the dropped panel is pruned, a static frame damages nothing"
        );
        cache.invalidate_all();
        cache.begin_frame();
        assert!(cache.snapshot().full, "an explicit invalidation is full");
    }

    #[test]
    fn each_panel_key_component_changes_the_key() {
        let theme = Theme::dark();
        let metrics = Metrics::new(1.0);
        let rect = Rect::new(1.0, 2.0, 30.0, 40.0);
        let base = PanelKey::new(PanelKind::Layers, 1, rect, &theme, metrics);
        assert_eq!(
            base,
            PanelKey::new(PanelKind::Layers, 1, rect, &theme, metrics)
        );
        assert_ne!(
            base,
            PanelKey::new(PanelKind::Timeline, 1, rect, &theme, metrics)
        );
        assert_ne!(
            base,
            PanelKey::new(PanelKind::Layers, 2, rect, &theme, metrics)
        );
        assert_ne!(
            base,
            PanelKey::new(
                PanelKind::Layers,
                1,
                Rect::new(1.0, 2.0, 31.0, 40.0),
                &theme,
                metrics
            )
        );
        assert_ne!(
            base,
            PanelKey::new(PanelKind::Layers, 1, rect, &theme, Metrics::new(1.25))
        );
        let mut other = Theme::dark();
        other.panel = [1.0, 0.0, 0.0, 1.0];
        assert_ne!(
            base,
            PanelKey::new(PanelKind::Layers, 1, rect, &other, metrics)
        );
        let mut with_font = base;
        with_font.font = 7;
        assert_ne!(base, with_font);
    }

    #[test]
    fn every_revision_input_changes_the_hash() {
        let base = PanelRevision::new().finish();
        assert_ne!(base, PanelRevision::new().u64(1).finish());
        assert_ne!(base, PanelRevision::new().usize(1).finish());
        assert_ne!(base, PanelRevision::new().i64(1).finish());
        assert_ne!(base, PanelRevision::new().f32(1.0).finish());
        assert_ne!(base, PanelRevision::new().f64(1.0).finish());
        assert_ne!(base, PanelRevision::new().bool(true).finish());
        assert_ne!(base, PanelRevision::new().str("a").finish());
        assert_ne!(
            base,
            PanelRevision::new()
                .rect(Rect::new(0.0, 0.0, 1.0, 1.0))
                .finish()
        );
        assert_eq!(
            PanelRevision::new().str("abc").finish(),
            PanelRevision::new().str("abc").finish()
        );
        assert_ne!(
            PanelRevision::new().str("abc").finish(),
            PanelRevision::new().str("abd").finish()
        );
    }

    #[test]
    fn spans_of_a_triangle() {
        // Right-pointing play triangle: apex at (10, 5).
        let tri = [(0.0, 0.0), (10.0, 5.0), (0.0, 10.0)];
        let (lo, hi) = span_at(&tri, 5.0).unwrap();
        assert!(lo.abs() < 1e-4 && (hi - 10.0).abs() < 1e-4);
        let (lo, hi) = span_at(&tri, 2.5).unwrap();
        assert!(lo.abs() < 1e-4 && (hi - 5.0).abs() < 1e-4);
        assert!(span_at(&tri, 11.0).is_none());
    }

    #[test]
    fn spans_of_a_diamond() {
        let diamond = [(5.0, 0.0), (10.0, 5.0), (5.0, 10.0), (0.0, 5.0)];
        let (lo, hi) = span_at(&diamond, 5.0).unwrap();
        assert!(lo.abs() < 1e-4 && (hi - 10.0).abs() < 1e-4);
        let (lo, hi) = span_at(&diamond, 1.0).unwrap();
        assert!((lo - 4.0).abs() < 1e-4 && (hi - 6.0).abs() < 1e-4);
    }
}

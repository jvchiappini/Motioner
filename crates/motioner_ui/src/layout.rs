//! Window layout and hit testing, in physical pixels with a top-left origin.
//!
//! Everything here is pure: [`Layout::compute`] turns the window size, the
//! scale factor and the user's splitter positions into rectangles, and
//! [`Layout::hit`] maps a pointer position to the element under it. Drawing
//! and input both read the same [`Layout`], so what is drawn is exactly what
//! is clickable.
//!
//! ```text
//! +------------------------------------------------------------------+
//! | top bar                                                          |
//! +-----------+------------------------------------------+-----------+
//! | layers    | canvas (pasteboard + composition)        | inspector |
//! +-----------+------------------------------------------+-----------+
//! | timecode        |<  <   (>)   >  >|   loop          fps · frames |
//! | names column    | ruler                                          |
//! |                 | tracks                                         |
//! +------------------------------------------------------------------+
//! | status bar                                                       |
//! +------------------------------------------------------------------+
//! ```

use crate::theme::{size, Metrics};

/// An axis-aligned rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    /// Left edge.
    pub x: f32,
    /// Top edge.
    pub y: f32,
    /// Width (never negative).
    pub w: f32,
    /// Height (never negative).
    pub h: f32,
}

impl Rect {
    /// Creates a rectangle; negative sizes are clamped to zero.
    #[must_use]
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            x,
            y,
            w: w.max(0.0),
            h: h.max(0.0),
        }
    }

    /// Right edge.
    #[must_use]
    pub fn right(self) -> f32 {
        self.x + self.w
    }

    /// Bottom edge.
    #[must_use]
    pub fn bottom(self) -> f32 {
        self.y + self.h
    }

    /// Horizontal centre.
    #[must_use]
    pub fn center_x(self) -> f32 {
        self.x + self.w * 0.5
    }

    /// Vertical centre.
    #[must_use]
    pub fn center_y(self) -> f32 {
        self.y + self.h * 0.5
    }

    /// Whether the rectangle has no area.
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.w <= 0.0 || self.h <= 0.0
    }

    /// Half-open containment: the left/top edges are inside, the right/bottom
    /// edges are not, so adjacent rectangles never both claim a point.
    #[must_use]
    pub fn contains(self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.right() && y >= self.y && y < self.bottom()
    }

    /// Shrinks every side by `d` (clamped to an empty rectangle).
    #[must_use]
    pub fn inset(self, d: f32) -> Self {
        self.inset_xy(d, d)
    }

    /// Shrinks the left/right sides by `dx` and the top/bottom by `dy`.
    #[must_use]
    pub fn inset_xy(self, dx: f32, dy: f32) -> Self {
        Self::new(
            self.x + dx,
            self.y + dy,
            self.w - 2.0 * dx,
            self.h - 2.0 * dy,
        )
    }

    /// Splits off the top `h` pixels: `(top, rest)`.
    #[must_use]
    pub fn split_top(self, h: f32) -> (Self, Self) {
        let h = h.clamp(0.0, self.h);
        (
            Self::new(self.x, self.y, self.w, h),
            Self::new(self.x, self.y + h, self.w, self.h - h),
        )
    }

    /// Splits off the bottom `h` pixels: `(rest, bottom)`.
    #[must_use]
    pub fn split_bottom(self, h: f32) -> (Self, Self) {
        let h = h.clamp(0.0, self.h);
        (
            Self::new(self.x, self.y, self.w, self.h - h),
            Self::new(self.x, self.bottom() - h, self.w, h),
        )
    }

    /// Splits off the left `w` pixels: `(left, rest)`.
    #[must_use]
    pub fn split_left(self, w: f32) -> (Self, Self) {
        let w = w.clamp(0.0, self.w);
        (
            Self::new(self.x, self.y, w, self.h),
            Self::new(self.x + w, self.y, self.w - w, self.h),
        )
    }

    /// Splits off the right `w` pixels: `(rest, right)`.
    #[must_use]
    pub fn split_right(self, w: f32) -> (Self, Self) {
        let w = w.clamp(0.0, self.w);
        (
            Self::new(self.x, self.y, self.w - w, self.h),
            Self::new(self.right() - w, self.y, w, self.h),
        )
    }

    /// Whether two rectangles share any area.
    #[must_use]
    pub fn overlaps(self, other: Self) -> bool {
        self.x < other.right()
            && other.x < self.right()
            && self.y < other.bottom()
            && other.y < self.bottom()
    }

    /// The `[x, y, w, h]` array used by [`motioner_render::ViewFit::contain`].
    #[must_use]
    pub fn to_f64(self) -> [f64; 4] {
        [
            f64::from(self.x),
            f64::from(self.y),
            f64::from(self.w),
            f64::from(self.h),
        ]
    }
}

/// User-adjustable panel sizes in logical pixels (changed by splitters).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PanelSizes {
    /// Layers panel width.
    pub left: f32,
    /// Inspector width.
    pub right: f32,
    /// Timeline height.
    pub timeline: f32,
}

impl Default for PanelSizes {
    fn default() -> Self {
        Self {
            left: size::LEFT_PANEL,
            right: size::RIGHT_PANEL,
            timeline: size::TIMELINE,
        }
    }
}

/// One of the three draggable panel edges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Splitter {
    /// Between the layers panel and the canvas.
    Left,
    /// Between the canvas and the inspector.
    Right,
    /// Between the canvas row and the timeline.
    Timeline,
}

/// A transport control in the timeline header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportButton {
    /// Jump to the first frame.
    ToStart,
    /// Step one frame back.
    StepBack,
    /// Play or pause.
    PlayPause,
    /// Step one frame forward.
    StepForward,
    /// Jump to the last frame.
    ToEnd,
    /// Toggle looping.
    Loop,
}

impl TransportButton {
    /// Every button, left to right.
    pub const ALL: [Self; 6] = [
        Self::ToStart,
        Self::StepBack,
        Self::PlayPause,
        Self::StepForward,
        Self::ToEnd,
        Self::Loop,
    ];
}

/// A toggle in the top bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TopBarToggle {
    /// Title/action safe-area guides on the canvas.
    SafeArea,
}

/// What the pointer is over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    /// A transport button.
    Transport(TransportButton),
    /// A top-bar toggle.
    Toggle(TopBarToggle),
    /// A row of the layers panel (index into the outline).
    LayerRow(usize),
    /// The visibility eye of a layer row.
    LayerEye(usize),
    /// The lock toggle of a layer row.
    LayerLock(usize),
    /// The solo toggle of a layer row.
    LayerSolo(usize),
    /// The shy toggle of a layer row.
    LayerShy(usize),
    /// The layers-panel search/filter field.
    LayerFilter,
    /// The global "show shy layers" switch next to the filter.
    ShowShy,
    /// A timeline track (index into the outline).
    Track(usize),
    /// The ruler or the empty track area: click or drag scrubs.
    Scrub,
    /// A panel splitter.
    Splitter(Splitter),
    /// The canvas.
    Canvas,
    /// The inspector body (scrolls).
    Inspector,
    /// An editable inspector field: item index in the layout and field index.
    InspectField {
        /// Index into the inspector layout items.
        item: usize,
        /// Index into that row's fields.
        field: usize,
    },
    /// The stopwatch of an inspector row.
    InspectStopwatch(usize),
    /// The key diamond of an inspector row.
    InspectDiamond(usize),
    /// One segment of an align control.
    InspectSegment {
        /// Index into the inspector layout items.
        item: usize,
        /// `0` left, `1` centre, `2` right.
        segment: usize,
    },
    /// The "Copy" button next to the layer identity.
    InspectCopy,
    /// A canvas gizmo handle.
    Gizmo(crate::gizmo::Handle),
    /// The snapping toggle in the top bar, left of the safe-area toggle.
    SnapToggle,
    /// The zoom readout in the top bar; clicking it fits the composition.
    ZoomPill,
    /// A timeline row (index into the timeline row model).
    TimelineRow(usize),
    /// The stopwatch of a timeline property row.
    TimelineStopwatch(usize),
    /// The previous-key button of a timeline property row.
    TimelinePrevKey(usize),
    /// The add/remove-key button of a timeline property row.
    TimelineKeyToggle(usize),
    /// The next-key button of a timeline property row.
    TimelineNextKey(usize),
    /// The value field of a timeline property row.
    TimelineField(usize),
    /// A keyframe diamond of a timeline row at a frame.
    TimelineDiamond {
        /// Row index in the timeline model.
        row: usize,
        /// Composition frame of the diamond.
        frame: i64,
    },
    /// A row of the orphans section.
    OrphanRow(usize),
    /// The delete action of an orphan row.
    OrphanDelete(usize),
    /// The re-bind action of an orphan row (only with a suggestion).
    OrphanRebind(usize),
    /// The chevron that expands a layer's property rows.
    LayerChevron(usize),
    /// The graph editor's empty plot area (pan, wheel zoom).
    GraphPlot,
    /// A key point in the graph editor.
    GraphKey(usize),
    /// A draggable bezier control point.
    GraphHandle {
        /// Segment index.
        segment: usize,
        /// Which control point.
        kind: crate::graph::HandleKind,
    },
}

/// A panel with a header strip and a body.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Panel {
    /// Whole panel.
    pub rect: Rect,
    /// Header strip.
    pub header: Rect,
    /// Content below the header.
    pub body: Rect,
}

impl Panel {
    fn new(rect: Rect, m: Metrics) -> Self {
        let (header, body) = rect.split_top(m.px(size::PANEL_HEADER));
        Self { rect, header, body }
    }
}

/// Timeline sub-rectangles.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct TimelineLayout {
    /// Whole timeline area.
    pub rect: Rect,
    /// Transport strip on top.
    pub header: Rect,
    /// Left column cell beside the ruler.
    pub corner: Rect,
    /// Layer names column below the corner.
    pub names: Rect,
    /// Ruler with frame/second ticks.
    pub ruler: Rect,
    /// Track lanes below the ruler.
    pub tracks: Rect,
    /// Horizontal span that maps frames: frame 0 at `x`, the last frame at
    /// `x + w`. Inset from the ruler so the playhead never sits on an edge.
    pub scrub: Rect,
}

/// The full window layout.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layout {
    /// Scale used to build this layout.
    pub metrics: Metrics,
    /// Whole window.
    pub window: Rect,
    /// Top bar.
    pub top_bar: Rect,
    /// Status bar.
    pub status_bar: Rect,
    /// Layers panel (left).
    pub layers: Panel,
    /// Inspector panel (right).
    pub inspector: Panel,
    /// Canvas between the panels.
    pub canvas: Rect,
    /// Timeline (bottom).
    pub timeline: TimelineLayout,
    /// Transport buttons, in [`TransportButton::ALL`] order.
    pub transport: [(TransportButton, Rect); 6],
    /// Safe-area toggle in the top bar.
    pub safe_area_toggle: Rect,
    /// Snapping toggle in the top bar, left of the safe-area toggle.
    pub snap_toggle: Rect,
    /// Zoom readout in the top bar; a click fits the composition.
    pub zoom_pill: Rect,
    /// Splitter hit zones.
    pub splitters: [(Splitter, Rect); 3],
    /// Height of one layer/track row.
    pub row_h: f32,
}

impl Layout {
    /// Computes the layout for a window of `width` × `height` physical pixels.
    ///
    /// Panels keep their requested sizes when they fit and shrink otherwise,
    /// so the canvas keeps at least the minimum canvas size (160 px, scaled) in both
    /// directions whenever the window is large enough to allow it. Every
    /// rectangle stays inside the window.
    #[must_use]
    pub fn compute(width: f32, height: f32, scale: f32, sizes: PanelSizes) -> Self {
        let m = Metrics::new(scale);
        let window = Rect::new(0.0, 0.0, width.max(0.0).floor(), height.max(0.0).floor());
        let (top_bar, rest) = window.split_top(m.px(size::TOP_BAR));
        let (middle, status_bar) = rest.split_bottom(m.px(size::STATUS_BAR));

        let min_canvas = m.px(size::MIN_CANVAS);
        // The timeline takes at most ~42 % of the height below the top bar,
        // unless the user drags it taller.
        let requested = m.px(sizes.timeline);
        let default_cap = if (sizes.timeline - size::TIMELINE).abs() < f32::EPSILON {
            (middle.h * 0.42).round()
        } else {
            f32::INFINITY
        };
        let timeline_h = clamp_panel(
            requested.min(default_cap),
            m.px(size::TRANSPORT + size::RULER + size::ROW),
            middle.h - min_canvas,
        );
        let (upper, timeline_rect) = middle.split_bottom(timeline_h);

        // Side panels never take more than about a quarter of the window
        // each, so small windows keep a usable canvas.
        let side_room = (upper.w - min_canvas).max(0.0);
        let left_w = clamp_panel(
            m.px(sizes.left),
            m.px(180.0),
            (side_room * 0.5).min((upper.w * 0.22).round()),
        );
        let right_w = clamp_panel(
            m.px(sizes.right),
            m.px(220.0),
            (side_room * 0.5).min((upper.w * 0.24).round()),
        );
        let (layers_rect, rest) = upper.split_left(left_w);
        let (canvas, inspector_rect) = rest.split_right(right_w);

        let timeline = timeline_layout(timeline_rect, left_w, m);
        let transport = transport_buttons(timeline.header, m);
        let zoom_pill = {
            let h = m.px(20.0);
            Rect::new(
                top_bar.right() - m.px(size::PAD) - m.px(56.0),
                (top_bar.center_y() - h * 0.5).round(),
                m.px(56.0),
                h,
            )
        };
        let safe_area_toggle = {
            let side = m.px(size::BUTTON);
            let x = zoom_pill.x - m.px(size::GRID * 2.0) - side;
            Rect::new(x, top_bar.center_y() - side * 0.5, side, side)
        };
        let snap_toggle = Rect::new(
            safe_area_toggle.x - m.px(size::BUTTON) - m.px(size::GRID),
            safe_area_toggle.y,
            safe_area_toggle.w,
            safe_area_toggle.h,
        );

        let hit = m.px(size::SPLITTER_HIT);
        let splitters = [
            (
                Splitter::Left,
                Rect::new(layers_rect.right() - hit * 0.5, upper.y, hit, upper.h),
            ),
            (
                Splitter::Right,
                Rect::new(inspector_rect.x - hit * 0.5, upper.y, hit, upper.h),
            ),
            (
                Splitter::Timeline,
                Rect::new(window.x, timeline_rect.y - hit * 0.5, window.w, hit),
            ),
        ];

        Self {
            metrics: m,
            window,
            top_bar,
            status_bar,
            layers: Panel::new(layers_rect, m),
            inspector: Panel::new(inspector_rect, m),
            canvas,
            timeline,
            transport,
            safe_area_toggle,
            snap_toggle,
            zoom_pill,
            splitters,
            row_h: m.px(size::ROW),
        }
    }

    /// The rectangle of layer row `index`, scrolled by `scroll` pixels.
    #[must_use]
    pub fn layer_row(&self, index: usize, scroll: f32) -> Rect {
        let body = self.layers.body;
        let y = body.y + self.metrics.px(size::GRID) + index as f32 * self.row_h - scroll;
        Rect::new(body.x, y, body.w, self.row_h)
    }

    /// The eye toggle inside a layer row.
    #[must_use]
    pub fn layer_eye(&self, row: Rect) -> Rect {
        let side = self.metrics.px(size::ICON + size::GRID);
        Rect::new(
            row.right() - self.metrics.px(size::PAD) - side + self.metrics.px(size::GRID * 0.5),
            row.center_y() - side * 0.5,
            side,
            side,
        )
    }

    /// The lock toggle inside a layer row, left of the eye.
    #[must_use]
    pub fn layer_lock(&self, row: Rect) -> Rect {
        let eye = self.layer_eye(row);
        Rect::new(eye.x - eye.w, eye.y, eye.w, eye.h)
    }

    /// The solo toggle inside a layer row, left of the lock.
    #[must_use]
    pub fn layer_solo(&self, row: Rect) -> Rect {
        let lock = self.layer_lock(row);
        Rect::new(lock.x - lock.w, lock.y, lock.w, lock.h)
    }

    /// The shy toggle inside a layer row, left of solo.
    #[must_use]
    pub fn layer_shy(&self, row: Rect) -> Rect {
        let solo = self.layer_solo(row);
        Rect::new(solo.x - solo.w, solo.y, solo.w, solo.h)
    }

    /// The layers-panel filter field in the panel header.
    #[must_use]
    pub fn layer_filter(&self) -> Rect {
        let header = self.layers.header;
        let width = (header.w * 0.44).min(self.metrics.px(180.0));
        let h = self.metrics.px(18.0);
        Rect::new(
            header.right() - self.metrics.px(size::PAD) - width,
            (header.center_y() - h * 0.5).round(),
            width,
            h,
        )
    }

    /// The "show shy layers" switch, left of the filter field.
    #[must_use]
    pub fn shy_toggle(&self) -> Rect {
        let filter = self.layer_filter();
        let side = filter.h;
        Rect::new(
            filter.x - self.metrics.px(size::GRID) - side,
            filter.y,
            side,
            side,
        )
    }

    /// The header rectangle of the orphans section, below `rows` rows.
    #[must_use]
    pub fn orphans_header(&self, rows: usize, scroll: f32) -> Rect {
        let body = self.layers.body;
        let y = body.y + self.metrics.px(size::GRID) + rows as f32 * self.row_h - scroll
            + self.metrics.px(size::GRID * 2.0);
        Rect::new(body.x, y, body.w, self.metrics.px(24.0))
    }

    /// One orphan row below the header.
    #[must_use]
    pub fn orphan_row(&self, index: usize, rows: usize, scroll: f32) -> Rect {
        let header = self.orphans_header(rows, scroll);
        let y = header.bottom() + index as f32 * self.row_h;
        Rect::new(header.x, y, header.w, self.row_h)
    }

    /// The delete action inside an orphan row.
    #[must_use]
    pub fn orphan_delete(&self, row: Rect) -> Rect {
        let side = self.metrics.px(size::ICON + size::GRID);
        Rect::new(
            row.right() - self.metrics.px(size::PAD) - side,
            row.center_y() - side * 0.5,
            side,
            side,
        )
    }

    /// The re-bind action inside an orphan row, left of the delete action.
    #[must_use]
    pub fn orphan_rebind(&self, row: Rect) -> Rect {
        let delete = self.orphan_delete(row);
        Rect::new(delete.x - delete.w, delete.y, delete.w, delete.h)
    }

    /// The graph editor's caption strip, above the plot and below the ruler.
    ///
    /// Keeping the caption outside the plot is what guarantees it never hides
    /// a key or a bezier handle.
    #[must_use]
    pub fn graph_caption(&self) -> Rect {
        let t = self.timeline;
        let pad = self.metrics.px(size::PAD + size::GRID);
        let height = self.metrics.px(16.0);
        Rect::new(
            t.names.x + pad,
            t.tracks.y + pad * 0.5,
            (t.rect.right() - t.names.x - pad * 2.0).max(0.0),
            height,
        )
    }

    /// The graph editor's plot rectangle: the timeline track area inset by
    /// the panel padding and by the caption strip above it.
    #[must_use]
    pub fn graph_plot(&self) -> Rect {
        let t = self.timeline;
        let pad = self.metrics.px(size::PAD + size::GRID);
        let caption = self.metrics.px(16.0 + size::GRID);
        Rect::new(
            t.names.x + pad,
            t.tracks.y + pad + caption,
            (t.rect.right() - t.names.x - pad * 2.0).max(0.0),
            (t.tracks.h - pad * 2.0 - caption).max(0.0),
        )
    }

    /// The rectangle of timeline track `index`, scrolled by `scroll` pixels.
    #[must_use]
    pub fn track_row(&self, index: usize, scroll: f32) -> Rect {
        let tracks = self.timeline.tracks;
        let y = tracks.y + index as f32 * self.row_h - scroll;
        Rect::new(self.timeline.names.x, y, self.timeline.rect.w, self.row_h)
    }

    /// The element under `(x, y)`, given how many outline rows exist and how
    /// far the layer list and the timeline are scrolled.
    ///
    /// Splitters win over the panels they border; buttons win over the strip
    /// they sit in.
    #[must_use]
    pub fn hit(
        &self,
        x: f32,
        y: f32,
        rows: usize,
        layer_scroll: f32,
        track_scroll: f32,
    ) -> Option<Hit> {
        if !self.window.contains(x, y) {
            return None;
        }
        if let Some((splitter, _)) = self.splitters.iter().find(|(_, r)| r.contains(x, y)) {
            return Some(Hit::Splitter(*splitter));
        }
        if let Some((button, _)) = self.transport.iter().find(|(_, r)| r.contains(x, y)) {
            return Some(Hit::Transport(*button));
        }
        if self.safe_area_toggle.contains(x, y) {
            return Some(Hit::Toggle(TopBarToggle::SafeArea));
        }
        if self.snap_toggle.contains(x, y) {
            return Some(Hit::SnapToggle);
        }
        if self.zoom_pill.contains(x, y) {
            return Some(Hit::ZoomPill);
        }
        if self.layer_filter().contains(x, y) {
            return Some(Hit::LayerFilter);
        }
        if self.shy_toggle().contains(x, y) {
            return Some(Hit::ShowShy);
        }
        if self.layers.body.contains(x, y) {
            let index =
                self.row_index(y - self.layers.body.y - self.metrics.px(size::GRID) + layer_scroll);
            return index.filter(|&i| i < rows).map(|i| {
                let row = self.layer_row(i, layer_scroll);
                if self.layer_eye(row).contains(x, y) {
                    Hit::LayerEye(i)
                } else if self.layer_lock(row).contains(x, y) {
                    Hit::LayerLock(i)
                } else if self.layer_solo(row).contains(x, y) {
                    Hit::LayerSolo(i)
                } else if self.layer_shy(row).contains(x, y) {
                    Hit::LayerShy(i)
                } else {
                    Hit::LayerRow(i)
                }
            });
        }
        let t = &self.timeline;
        if t.names.contains(x, y) {
            let index = self.row_index(y - t.tracks.y + track_scroll);
            return index.filter(|&i| i < rows).map(Hit::Track);
        }
        if t.ruler.contains(x, y) || t.tracks.contains(x, y) {
            return Some(Hit::Scrub);
        }
        if self.canvas.contains(x, y) {
            return Some(Hit::Canvas);
        }
        if self.inspector.body.contains(x, y) {
            return Some(Hit::Inspector);
        }
        None
    }

    fn row_index(&self, offset: f32) -> Option<usize> {
        if offset < 0.0 || self.row_h <= 0.0 {
            return None;
        }
        Some((offset / self.row_h).floor() as usize)
    }
}

/// The scroll offset that brings an item spanning `item_top..item_top+item_h`
/// (in unscrolled content coordinates) fully into a viewport of height
/// `view_h`, moving as little as possible. An item taller than the viewport
/// is aligned to its top.
#[must_use]
pub fn scroll_into_view(scroll: f32, item_top: f32, item_h: f32, view_h: f32) -> f32 {
    if item_top < scroll || item_h >= view_h {
        item_top.max(0.0)
    } else if item_top + item_h > scroll + view_h {
        (item_top + item_h - view_h).max(0.0)
    } else {
        scroll
    }
}

/// Panel sizes after dragging `which` splitter by `delta` logical pixels
/// from `start`.
///
/// Dragging the left splitter right widens the layers panel; dragging the
/// right splitter left widens the inspector; dragging the timeline splitter
/// up makes the timeline taller. Results are snapped to the 4 px grid and
/// kept above each panel's minimum; [`Layout::compute`] applies the maximums.
#[must_use]
pub fn resize_panels(start: PanelSizes, which: Splitter, delta: (f32, f32)) -> PanelSizes {
    let snap = |v: f32| (v / size::GRID).round() * size::GRID;
    let mut sizes = start;
    match which {
        Splitter::Left => sizes.left = snap((start.left + delta.0).max(180.0)),
        Splitter::Right => sizes.right = snap((start.right - delta.0).max(220.0)),
        Splitter::Timeline => {
            let min = size::TRANSPORT + size::RULER + size::ROW;
            sizes.timeline = snap((start.timeline - delta.1).max(min));
        }
    }
    sizes
}

/// Clamps a panel size between `min` and `max`, preferring `max` when the two
/// conflict (a tiny window gives the panel whatever room is left).
fn clamp_panel(requested: f32, min: f32, max: f32) -> f32 {
    let max = max.max(0.0);
    requested.max(min).min(max).max(0.0)
}

fn timeline_layout(rect: Rect, names_w: f32, m: Metrics) -> TimelineLayout {
    let (header, below) = rect.split_top(m.px(size::TRANSPORT));
    let (ruler_row, tracks_row) = below.split_top(m.px(size::RULER));
    let (corner, ruler) = ruler_row.split_left(names_w);
    let (names, tracks) = tracks_row.split_left(names_w);
    let inset = m.px(size::PAD + size::GRID);
    let scrub = Rect::new(
        ruler.x + inset,
        ruler.y,
        ruler.w - 2.0 * inset,
        ruler.h + tracks.h,
    );
    TimelineLayout {
        rect,
        header,
        corner,
        names,
        ruler,
        tracks,
        scrub,
    }
}

fn transport_buttons(header: Rect, m: Metrics) -> [(TransportButton, Rect); 6] {
    let side = m.px(size::BUTTON);
    let primary = m.px(size::BUTTON_PRIMARY);
    let gap = m.px(size::GRID);
    let loop_gap = m.px(size::GRID * 3.0);
    let row_w = side * 5.0 + primary + gap * 4.0 + loop_gap;
    let mut x = (header.center_x() - row_w * 0.5).round();
    let cy = header.center_y();
    TransportButton::ALL.map(|button| {
        let s = if button == TransportButton::PlayPause {
            primary
        } else {
            side
        };
        if button == TransportButton::Loop {
            x += loop_gap - gap;
        }
        let rect = Rect::new(x, (cy - s * 0.5).round(), s, s);
        x += s + gap;
        (button, rect)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZES: [(f32, f32); 4] = [
        (1600.0, 900.0),
        (1280.0, 720.0),
        (1920.0, 1080.0),
        (800.0, 500.0),
    ];

    fn regions(layout: &Layout) -> Vec<(&'static str, Rect)> {
        vec![
            ("top", layout.top_bar),
            ("status", layout.status_bar),
            ("layers", layout.layers.rect),
            ("inspector", layout.inspector.rect),
            ("canvas", layout.canvas),
            ("timeline", layout.timeline.rect),
        ]
    }

    fn inside(outer: Rect, inner: Rect) -> bool {
        inner.x >= outer.x - 1e-3
            && inner.y >= outer.y - 1e-3
            && inner.right() <= outer.right() + 1e-3
            && inner.bottom() <= outer.bottom() + 1e-3
    }

    #[test]
    fn regions_tile_the_window_without_overlap() {
        for scale in [1.0, 1.25, 1.5] {
            for (w, h) in SIZES {
                let layout = Layout::compute(w, h, scale, PanelSizes::default());
                let regions = regions(&layout);
                let mut area = 0.0;
                for (i, (name, a)) in regions.iter().enumerate() {
                    assert!(
                        inside(layout.window, *a),
                        "{name} outside at {w}x{h}@{scale}"
                    );
                    area += a.w * a.h;
                    for (other, b) in &regions[i + 1..] {
                        assert!(
                            !a.overlaps(*b),
                            "{name} overlaps {other} at {w}x{h}@{scale}"
                        );
                    }
                }
                assert!(
                    (area - w * h).abs() < 1.0,
                    "regions cover the window at {w}x{h}@{scale}"
                );
            }
        }
    }

    #[test]
    fn edges_sit_on_whole_pixels_and_the_scaled_grid() {
        for scale in [1.0, 1.25] {
            let layout = Layout::compute(1600.0, 900.0, scale, PanelSizes::default());
            let grid = layout.metrics.px(size::GRID);
            for (name, r) in regions(&layout) {
                for edge in [r.x, r.y, r.right(), r.bottom()] {
                    assert!((edge - edge.round()).abs() < 1e-3, "{name} edge {edge}");
                }
            }
            // Fixed-size bars are whole multiples of the scaled grid.
            for (name, h) in [
                ("top", layout.top_bar.h),
                ("status", layout.status_bar.h),
                ("row", layout.row_h),
            ] {
                assert!(
                    (h / grid - (h / grid).round()).abs() < 1e-3,
                    "{name} {h} not on the {grid} grid"
                );
            }
        }
    }

    #[test]
    fn default_panels_keep_their_size_on_a_large_window() {
        let layout = Layout::compute(1600.0, 900.0, 1.0, PanelSizes::default());
        assert!((layout.layers.rect.w - size::LEFT_PANEL).abs() < f32::EPSILON);
        assert!((layout.inspector.rect.w - size::RIGHT_PANEL).abs() < f32::EPSILON);
        assert!((layout.timeline.rect.h - size::TIMELINE).abs() < f32::EPSILON);
        // A small, scaled window caps the side panels so the canvas stays wide.
        let small = Layout::compute(1280.0, 720.0, 1.25, PanelSizes::default());
        assert!(small.canvas.w >= small.window.w * 0.5, "{}", small.canvas.w);
    }

    #[test]
    fn canvas_keeps_a_minimum_size() {
        let huge = PanelSizes {
            left: 2000.0,
            right: 2000.0,
            timeline: 2000.0,
        };
        for (w, h) in [(1280.0, 720.0), (1920.0, 1080.0)] {
            let layout = Layout::compute(w, h, 1.0, huge);
            assert!(
                layout.canvas.w >= size::MIN_CANVAS - 1.0,
                "{w}x{h}: {}",
                layout.canvas.w
            );
            assert!(
                layout.canvas.h >= size::MIN_CANVAS - 1.0,
                "{w}x{h}: {}",
                layout.canvas.h
            );
        }
    }

    #[test]
    fn a_tiny_window_degrades_without_panicking() {
        for (w, h) in [(0.0, 0.0), (10.0, 10.0), (200.0, 120.0)] {
            let layout = Layout::compute(w, h, 1.0, PanelSizes::default());
            for (name, r) in regions(&layout) {
                assert!(r.w >= 0.0 && r.h >= 0.0, "{name} at {w}x{h}");
                assert!(inside(layout.window, r), "{name} outside at {w}x{h}");
            }
            let _ = layout.hit(5.0, 5.0, 3, 0.0, 0.0);
        }
    }

    #[test]
    fn transport_is_centred_in_its_strip() {
        let layout = Layout::compute(1600.0, 900.0, 1.0, PanelSizes::default());
        let header = layout.timeline.header;
        for (button, r) in layout.transport {
            assert!(inside(header, r), "{button:?} outside the strip");
        }
        let play = layout.transport[2].1;
        let first = layout.transport[0].1;
        let last = layout.transport[4].1;
        let span_centre = (first.x + last.right()) * 0.5;
        assert!(
            (span_centre - play.center_x()).abs() <= 1.0,
            "play button is the visual centre"
        );
        for pair in layout.transport.windows(2) {
            assert!(!pair[0].1.overlaps(pair[1].1));
        }
    }

    #[test]
    fn hit_testing_matches_the_drawn_rectangles() {
        let layout = Layout::compute(1600.0, 900.0, 1.0, PanelSizes::default());
        let (button, r) = layout.transport[2];
        assert_eq!(
            layout.hit(r.center_x(), r.center_y(), 4, 0.0, 0.0),
            Some(Hit::Transport(button))
        );

        let row = layout.layer_row(1, 0.0);
        assert_eq!(
            layout.hit(row.x + 40.0, row.center_y(), 4, 0.0, 0.0),
            Some(Hit::LayerRow(1))
        );
        let eye = layout.layer_eye(row);
        assert_eq!(
            layout.hit(eye.center_x(), eye.center_y(), 4, 0.0, 0.0),
            Some(Hit::LayerEye(1))
        );
        let lock = layout.layer_lock(row);
        assert!(!lock.overlaps(eye));
        assert_eq!(
            layout.hit(lock.center_x(), lock.center_y(), 4, 0.0, 0.0),
            Some(Hit::LayerLock(1))
        );
        // A row past the end of the outline hits nothing.
        let past = layout.layer_row(7, 0.0);
        assert_eq!(
            layout.hit(past.x + 40.0, past.center_y(), 4, 0.0, 0.0),
            None
        );
        // Scrolling moves rows under the pointer.
        let scrolled = layout.layer_row(2, layout.row_h);
        assert_eq!(
            layout.hit(scrolled.x + 40.0, scrolled.center_y(), 4, layout.row_h, 0.0),
            Some(Hit::LayerRow(2))
        );

        let track = layout.track_row(0, 0.0);
        assert_eq!(
            layout.hit(track.x + 20.0, track.center_y(), 4, 0.0, 0.0),
            Some(Hit::Track(0))
        );
        let ruler = layout.timeline.ruler;
        assert_eq!(
            layout.hit(ruler.center_x(), ruler.center_y(), 4, 0.0, 0.0),
            Some(Hit::Scrub)
        );
        let canvas = layout.canvas;
        assert_eq!(
            layout.hit(canvas.center_x(), canvas.center_y(), 4, 0.0, 0.0),
            Some(Hit::Canvas)
        );
        let toggle = layout.safe_area_toggle;
        assert_eq!(
            layout.hit(toggle.center_x(), toggle.center_y(), 4, 0.0, 0.0),
            Some(Hit::Toggle(TopBarToggle::SafeArea))
        );
        assert_eq!(layout.hit(-1.0, 10.0, 4, 0.0, 0.0), None);
    }

    #[test]
    fn splitters_win_over_panel_edges() {
        let layout = Layout::compute(1600.0, 900.0, 1.0, PanelSizes::default());
        let edge = layout.layers.rect.right();
        let y = layout.canvas.center_y();
        assert_eq!(
            layout.hit(edge, y, 0, 0.0, 0.0),
            Some(Hit::Splitter(Splitter::Left))
        );
        assert_eq!(
            layout.hit(edge - 2.0, y, 0, 0.0, 0.0),
            Some(Hit::Splitter(Splitter::Left))
        );
        let top = layout.timeline.rect.y;
        assert_eq!(
            layout.hit(800.0, top, 0, 0.0, 0.0),
            Some(Hit::Splitter(Splitter::Timeline))
        );
    }

    #[test]
    fn names_column_lines_up_with_the_layers_panel() {
        for scale in [1.0, 1.25] {
            let layout = Layout::compute(1920.0, 1080.0, scale, PanelSizes::default());
            assert!((layout.timeline.names.w - layout.layers.rect.w).abs() < f32::EPSILON);
            assert!((layout.timeline.ruler.x - layout.canvas.x).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn scrolling_reveals_items_with_minimal_movement() {
        // Viewport of 100 px scrolled to 50: items 50..150 are visible.
        assert!(
            (scroll_into_view(50.0, 60.0, 20.0, 100.0) - 50.0).abs() < f32::EPSILON,
            "already visible"
        );
        assert!(
            (scroll_into_view(50.0, 10.0, 20.0, 100.0) - 10.0).abs() < f32::EPSILON,
            "above: align top"
        );
        assert!(
            (scroll_into_view(50.0, 160.0, 20.0, 100.0) - 80.0).abs() < f32::EPSILON,
            "below: align bottom"
        );
        assert!(
            (scroll_into_view(0.0, 40.0, 200.0, 100.0) - 40.0).abs() < f32::EPSILON,
            "too tall: top"
        );
    }

    #[test]
    fn splitters_resize_on_the_grid() {
        let start = PanelSizes::default();
        let wider = resize_panels(start, Splitter::Left, (37.0, 0.0));
        assert!((wider.left - 300.0).abs() < f32::EPSILON, "{}", wider.left);
        assert!((wider.right - start.right).abs() < f32::EPSILON);
        let inspector = resize_panels(start, Splitter::Right, (-30.0, 0.0));
        assert!(
            (inspector.right - 320.0).abs() < f32::EPSILON,
            "{}",
            inspector.right
        );
        let taller = resize_panels(start, Splitter::Timeline, (0.0, -50.0));
        assert!(
            // 320 + 50 = 370, snapped to the 4 px grid.
            (taller.timeline - 372.0).abs() < f32::EPSILON,
            "{}",
            taller.timeline
        );
        let tiny = resize_panels(start, Splitter::Left, (-1000.0, 0.0));
        assert!((tiny.left - 180.0).abs() < f32::EPSILON, "minimum width");
    }

    #[test]
    fn rect_helpers() {
        let r = Rect::new(10.0, 20.0, 100.0, 50.0);
        assert!(r.contains(10.0, 20.0));
        assert!(!r.contains(110.0, 20.0), "right edge is exclusive");
        assert!(r.inset(60.0).is_empty());
        let (top, rest) = r.split_top(10.0);
        assert!((top.h + rest.h - r.h).abs() < f32::EPSILON);
        let (left, right) = r.split_left(500.0);
        assert!((left.w - 100.0).abs() < f32::EPSILON && right.is_empty());
        assert!(Rect::new(0.0, 0.0, -5.0, 3.0).is_empty());
    }
}

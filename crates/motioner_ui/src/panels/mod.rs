//! The editor panels. Each module draws one region of the [`Layout`] from a
//! read-only [`View`] of the app state; none of them mutates state or reads
//! input, so the drawn frame is a pure function of the view.
//!
//! By convention `p` is the [`crate::paint::Painter`], `v` the [`View`] and
//! `t` the theme in every drawing function.

#![allow(clippy::many_single_char_names)]

pub mod assets;
pub mod canvas;
pub mod code;
pub mod inspector;
pub mod layers;
pub mod overrides;
pub mod status;
pub mod timeline;
pub mod top_bar;

use motioner_core::{Document, FrameRate};

use crate::inspect::{Editing, FieldTarget, Item, Layout as InspectLayout, Model as InspectModel};
use crate::layout::{Hit, Layout, Rect};
use crate::outline::OutlineRow;
use crate::paint::{HoverKey, PanelKind};
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
#[derive(Debug, Clone, PartialEq)]
pub struct TooltipRequest {
    /// The widget the tooltip points at.
    pub anchor: Rect,
    /// What the widget does.
    pub label: String,
    /// Keyboard shortcut, if any.
    pub shortcut: Option<&'static str>,
}

/// Everything the panels need to draw one frame.
#[allow(clippy::struct_excessive_bools)] // independent UI flags, not a state machine
#[allow(clippy::struct_field_names)] // view_menu_* and code_* read best with the panel name
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
    /// Decoded-video cache bytes.
    pub video_bytes: usize,
    /// Proxy progress text, when a background job is running.
    pub proxy_status: Option<&'a str>,
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
    /// Whether the View menu is open.
    pub view_menu_open: bool,
    /// The View-menu item under the pointer.
    pub view_menu_hover: Option<crate::layout::ViewAction>,
    /// Whether the Add Layer menu is open.
    pub add_menu_open: bool,
    /// The Add Layer menu row under the pointer.
    pub add_menu_hover: Option<usize>,
    /// Pending overrides, one row each.
    pub override_rows: &'a [crate::OverrideRow],
    /// Whether the pending-overrides list is open.
    pub overrides_open: bool,
    /// The read-only code panel (lines and last-edit highlight).
    pub code_panel: &'a crate::CodePanel,
    /// Whether the code panel is open.
    pub code_open: bool,
    /// Whether the one-time convert-on-open banner is shown.
    pub bake_offer: bool,
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

    /// The stable hover code of the pointer when it belongs to `kind`.
    ///
    /// Panels hash this into their revision; a hit another panel paints leaves
    /// it at `0`, so a pointer move over a layer row does not invalidate the
    /// timeline, the inspector or the top bar.
    #[must_use]
    pub fn hover_code(&self, kind: PanelKind) -> u64 {
        crate::paint::hover_code(self.hover, kind)
    }

    /// The hover state to pass to [`crate::paint::Painter::panel_hover`].
    ///
    /// `rect` is the hovered widget when the hit is not a cached row; rows
    /// report their own damage from the painter's row cache.
    #[must_use]
    pub fn hover_key(&self, kind: PanelKind) -> HoverKey {
        let code = self.hover_code(kind);
        if code == 0 {
            return HoverKey::none();
        }
        HoverKey::new(code, self.hover_rect())
    }

    /// The rectangle of the hovered widget, for a hover-only repaint.
    fn hover_rect(&self) -> Option<Rect> {
        let hit = self.hover?;
        Some(match hit {
            Hit::LayerRow(index)
            | Hit::LayerEye(index)
            | Hit::LayerLock(index)
            | Hit::LayerSolo(index)
            | Hit::LayerShy(index) => self
                .layout
                .layer_row(self.layer_position(index), self.layer_scroll),
            Hit::OrphanRow(index) | Hit::OrphanDelete(index) | Hit::OrphanRebind(index) => self
                .layout
                .orphan_row(index, self.layer_rows.len(), self.layer_scroll),
            Hit::LeftTab(tab) => self.layout.left_tab(tab),
            Hit::AssetRow(index) => self
                .layout
                .asset_row(self.asset_position(index), self.layer_scroll),
            Hit::AssetCell(index) => self
                .layout
                .asset_cell(self.asset_position(index), self.layer_scroll),
            Hit::AssetAdd => self.layout.asset_toolbar().0,
            Hit::AssetView(view) => {
                let (_, list, grid, _) = self.layout.asset_toolbar();
                if view == crate::assets::AssetView::List {
                    list
                } else {
                    grid
                }
            }
            Hit::AssetRemoveUnused => self.layout.asset_toolbar().3,
            Hit::Transport(button) => self
                .layout
                .transport
                .iter()
                .find(|(candidate, _)| *candidate == button)
                .map_or(self.layout.timeline.header, |(_, rect)| *rect),
            Hit::TimelineRow(index)
            | Hit::TimelineStopwatch(index)
            | Hit::TimelinePrevKey(index)
            | Hit::TimelineKeyToggle(index)
            | Hit::TimelineNextKey(index)
            | Hit::TimelineField(index)
            | Hit::TimelineDiamond { row: index, .. }
            | Hit::LayerChevron(index) => self.timeline_row_rect(index),
            Hit::InspectField { item, .. }
            | Hit::InspectStopwatch(item)
            | Hit::InspectDiamond(item)
            | Hit::InspectSegment { item, .. } => self.inspector_item_rect(item),
            Hit::InspectCopy => self
                .inspector_layout
                .items
                .iter()
                .find_map(|item| match item {
                    Item::Header { copy, .. } => Some(*copy),
                    _ => None,
                })
                .unwrap_or(self.layout.inspector.header),
            Hit::Toggle(_) => self.layout.safe_area_toggle,
            Hit::SnapToggle => self.layout.snap_toggle,
            Hit::ZoomPill => self.layout.zoom_pill,
            Hit::FileMenu => self.layout.file_button,
            Hit::ViewMenu => self.layout.view_button,
            Hit::RecoverRestore => self.layout.recovery_restore,
            Hit::RecoverDismiss => self.layout.recovery_dismiss,
            Hit::Gizmo(handle) => self.gizmo_handle_rect(handle),
            _ => return None,
        })
    }

    /// A small box around one selection handle.
    fn gizmo_handle_rect(&self, handle: crate::gizmo::Handle) -> Rect {
        use crate::gizmo::Handle;
        let Some(bounds) = self.selection_bounds else {
            return self.layout.canvas;
        };
        let side = self.layout.metrics.px(9.0);
        let x = match handle {
            Handle::TopLeft | Handle::BottomLeft => bounds.x,
            Handle::TopRight | Handle::BottomRight => bounds.right(),
            _ => bounds.center_x(),
        };
        let y = match handle {
            Handle::TopLeft | Handle::TopRight => bounds.y,
            Handle::BottomLeft | Handle::BottomRight => bounds.bottom(),
            Handle::Rotate => bounds.y - self.layout.metrics.px(22.0),
            Handle::Move => bounds.center_y(),
        };
        Rect::new(x - side * 0.5, y - side * 0.5, side, side)
    }

    /// The position of an outline row in the filtered layer list.
    fn layer_position(&self, index: usize) -> usize {
        self.layer_rows
            .iter()
            .position(|candidate| *candidate == index)
            .unwrap_or(index)
    }

    /// The position of an asset entry in the filtered list or grid.
    fn asset_position(&self, index: usize) -> usize {
        let count = self.assets.visible_count(self.filter);
        (0..count)
            .find(|position| self.assets.list_index(*position, self.filter) == Some(index))
            .unwrap_or(index)
    }

    /// The laid-out rectangle of timeline row `index`.
    fn timeline_row_rect(&self, index: usize) -> Rect {
        self.timeline_layout
            .rows
            .iter()
            .find(|row| row.row == index)
            .map_or(self.layout.timeline.tracks, |row| row.rect)
    }

    /// The laid-out rectangle of inspector item `index`.
    fn inspector_item_rect(&self, index: usize) -> Rect {
        match self.inspector_layout.items.get(index) {
            Some(
                Item::Header { rect, .. } | Item::Section { rect, .. } | Item::Row { rect, .. },
            ) => *rect,
            None => self.layout.inspector.body,
        }
    }
}

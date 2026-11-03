//! Painting the composition and the selection overlays, and resolving canvas
//! picks.
//!
//! Picking a layer needs text measurement, which needs the renderer, so a
//! press stores a point in `pending_pick` and the next paint resolves it into
//! a `select` or `deselect` command. Gizmo hit testing uses the selection
//! bounds of the previous frame (one frame of latency, invisible in use).

use std::time::Instant;

use ferrous_renderer::Renderer;
use motioner_core::{NodeId, OverrideAction, Vec2};
use motioner_render::{ImageData, ImageSet, TextExtent, ViewFit};

use super::PreviewApp;
use crate::bounds::{self, Bounds};
use crate::command::Command;
use crate::editor::ViewState;
use crate::layout::{Hit, Layout, Rect};
use crate::panels;
use crate::theme::size;
use crate::viewport::Viewport;
use crate::PreviewSession;

/// Converts the session's decoded images into renderer upload records.
fn image_set(images: Vec<motioner_core::DecodedImage>) -> ImageSet {
    let mut set = ImageSet::new();
    for image in images {
        set.insert(ImageData {
            key: image.asset.key,
            width: image.asset.width,
            height: image.asset.height,
            rgba8: image.rgba8,
        });
    }
    set
}

/// What a canvas click landed on.
pub(super) enum Pick {
    /// An unlocked layer.
    Node(NodeId),
    /// Empty canvas (the selection is cleared).
    Nothing,
}

/// On-screen bounds produced while painting the canvas.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct CanvasBounds {
    pub(super) selection: Option<Rect>,
    pub(super) size: Option<(f64, f64)>,
    pub(super) hover: Option<Rect>,
    bounds: Option<Bounds>,
}

impl PreviewApp {
    pub(super) fn paint_canvas(
        &mut self,
        renderer: &mut Renderer,
        layout: &Layout,
        frame: i64,
    ) -> CanvasBounds {
        let viewport = viewport(self.editor.session(), self.editor.view(), layout);
        let comp_rect = viewport.map(|view| view.rect);
        let background = self
            .editor
            .session()
            .composition()
            .map(motioner_core::Composition::background);
        panels::canvas::paint_backdrop(
            renderer,
            &self.theme,
            (layout.window.w, layout.window.h),
            layout.canvas,
            comp_rect.zip(background),
            layout.metrics.px(24.0),
        );
        let pick = self.pending_pick.take();
        let (out, picked) = self.canvas_overlays(renderer, layout, frame, viewport, pick);
        if let Some(picked) = picked {
            let command = match picked {
                Pick::Node(node) => Command::Select(self.editor.node_identity(node)),
                Pick::Nothing => Command::Deselect,
            };
            self.run(&command);
        }
        self.last_bounds = out.bounds;
        out
    }

    /// Paints the composition, computes the selection and hover bounds, and
    /// resolves a pending pick ([`Pick`]).
    fn canvas_overlays(
        &mut self,
        renderer: &mut Renderer,
        layout: &Layout,
        frame: i64,
        viewport: Option<Viewport>,
        pick: Option<Vec2>,
    ) -> (CanvasBounds, Option<Pick>) {
        let mut out = CanvasBounds::default();
        let hovered = self.hovered_node();
        let selected = self.editor.selected();
        let Some(view) = viewport else {
            return (out, None);
        };
        let rect = view.rect;
        let stale = self.editor.session().plan_frame() != Some(frame);
        let started = Instant::now();
        // Only a click needs the lock states; do not copy the set every frame.
        let overrides = pick.map(|_| self.editor.session().overrides().clone());
        let generation = self.editor.session().generation();
        if self.painter_generation != Some(generation) {
            let images = self.editor.session().decoded_images();
            let set = image_set(images);
            if let Some(painter) = self.painter.as_mut() {
                painter.set_images(set.clone());
            }
            self.blur.set_images(set);
            self.painter_generation = Some(generation);
            self.blur_generation = Some(generation);
        }
        let playing = self.editor.transport().is_playing();
        let blur_on = self.editor.motion_blur_preview()
            && self
                .editor
                .session()
                .composition()
                .is_some_and(motioner_core::Composition::motion_blur);
        let hidden = self
            .editor
            .has_preview_filter()
            .then(|| self.editor.preview_hidden());
        let Some((document, plan)) = self.editor.session_mut().evaluate(frame) else {
            return (out, None);
        };
        if stale {
            self.eval_ms = started.elapsed().as_secs_f32() * 1000.0;
        }
        let comp = document.composition();
        if blur_on
            && hidden.is_none()
            && self.blur.paint(
                renderer,
                document,
                frame,
                rect,
                (layout.window.w, layout.window.h),
                playing,
                generation,
            )
        {
            // The accumulated frame is on screen; the selection overlays below
            // still run against the same document and plan.
        } else if let Some(painter) = self.painter.as_mut() {
            let fit = ViewFit::contain(
                f64::from(comp.width()),
                f64::from(comp.height()),
                rect.to_f64(),
                f64::from(layout.window.w),
                f64::from(layout.window.h),
            );
            if let Some(hidden) = &hidden {
                painter.paint_visible(renderer, document, plan, fit, hidden);
            } else {
                painter.paint(renderer, document, plan, fit);
            }
        }
        let painter = self.painter.as_ref();
        let renderer_ref = &*renderer;
        let measure = |text: &str, size: f64| -> Option<TextExtent> {
            painter?.text_extent(renderer_ref, text, size)
        };
        let to_screen = |b: Bounds| {
            view.box_to_screen(Vec2::new(b.min_x, b.min_y), Vec2::new(b.max_x, b.max_y))
        };
        if let Some(b) =
            selected.and_then(|node| bounds::node_bounds(document, plan, node, &measure))
        {
            out.selection = Some(to_screen(b));
            out.size = Some((b.max_x - b.min_x, b.max_y - b.min_y));
            out.bounds = Some(b);
        }
        out.hover = hovered
            .and_then(|node| bounds::node_bounds(document, plan, node, &measure))
            .map(to_screen);
        let picked = pick.map(|point| {
            let locked = |node: NodeId| {
                document.node(node).is_some_and(|n| {
                    overrides.as_ref().is_some_and(|set| {
                        set.get(n.identity().as_str(), motioner_core::LOCKED, None)
                            .is_some_and(|entry| entry.action == OverrideAction::Locked(true))
                    })
                })
            };
            bounds::hit_node_where(document, plan, point, &measure, |node| !locked(node))
                .map_or(Pick::Nothing, Pick::Node)
        });
        (out, picked)
    }

    fn hovered_node(&self) -> Option<NodeId> {
        let outline = match self.pointer.hover? {
            Hit::LayerRow(row) | Hit::LayerEye(row) | Hit::LayerLock(row) | Hit::Track(row) => row,
            Hit::TimelineRow(row) | Hit::TimelineDiamond { row, .. } => {
                self.editor.timeline().rows().get(row)?.kind.outline()
            }
            _ => return None,
        };
        self.editor.outline().get(outline).map(|row| row.node)
    }

    pub(super) fn zoom(&self, layout: &Layout) -> f64 {
        viewport(self.editor.session(), self.editor.view(), layout).map_or(1.0, |view| view.scale)
    }

    pub(super) fn composition_size(&self) -> (f64, f64) {
        self.editor
            .session()
            .composition()
            .map_or((1920.0, 1080.0), |c| {
                (f64::from(c.width()), f64::from(c.height()))
            })
    }

    /// Bounds of the other layers, for snapping (shape bounds only; text
    /// layers need the renderer and are skipped).
    pub(super) fn other_bounds(&self, selected: NodeId) -> Vec<Bounds> {
        let Some(document) = self.editor.session().document() else {
            return Vec::new();
        };
        let plan = self.editor.session().plan();
        self.editor
            .outline()
            .iter()
            .filter(|row| row.node != selected)
            .filter_map(|row| {
                bounds::node_bounds(
                    document,
                    plan,
                    row.node,
                    &|_: &str, _: f64| -> Option<TextExtent> { None },
                )
            })
            .collect()
    }
}

/// The canvas area the composition is fitted into: the canvas inset by a
/// margin that shrinks on very small canvases.
pub(super) fn canvas_area(layout: &Layout) -> Rect {
    let short_side = layout.canvas.w.min(layout.canvas.h);
    let margin = layout
        .metrics
        .px(size::GRID * 8.0)
        .min((short_side * 0.06).round());
    layout.canvas.inset(margin)
}

/// The canvas viewport from the view state: fit, zoom and pan (D-0026).
pub(super) fn viewport(
    session: &PreviewSession,
    view: &ViewState,
    layout: &Layout,
) -> Option<Viewport> {
    let comp = session.composition()?;
    Viewport::new(
        f64::from(comp.width()),
        f64::from(comp.height()),
        canvas_area(layout),
        view.zoom,
        view.pan,
    )
}

/// The fit scale of the composition in the current canvas.
pub(super) fn fit_zoom(session: &PreviewSession, layout: &Layout) -> f64 {
    let Some(comp) = session.composition() else {
        return 1.0;
    };
    Viewport::fit_scale(
        f64::from(comp.width()),
        f64::from(comp.height()),
        canvas_area(layout),
    )
    .unwrap_or(1.0)
}

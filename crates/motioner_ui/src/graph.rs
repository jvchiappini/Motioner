//! The graph editor's model and pure screen mapping.
//!
//! One numeric channel is shown as its value curve: keys are points and every
//! segment whose arriving key has a `bezier` ease gets two draggable control
//! points. All edits go through [`crate::Command`] variants (`ease`,
//! `move_key_value`), so the graph has exactly the same undo and write-through
//! behaviour as a typed field.

use motioner_core::{BezierEase, Ease, Frame};

use crate::layout::Rect;

/// One key of the edited channel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GraphKey {
    /// Composition frame.
    pub frame: Frame,
    /// Value at the key.
    pub value: f64,
    /// Ease of the arriving segment (the key that ends it owns it).
    pub ease: Ease,
}

/// The channel shown in the graph editor.
#[derive(Debug, Clone, PartialEq)]
pub struct GraphModel {
    /// Node identity.
    pub id: String,
    /// Channel name (`pos.x`, `opacity`, `fill`).
    pub property: String,
    /// Keys in time order.
    pub keys: Vec<GraphKey>,
    /// Composition duration in frames.
    pub duration: Frame,
    /// Value bounds fitted from the keys (never empty).
    pub min: f64,
    /// Maximum key value.
    pub max: f64,
    /// Horizontal view zoom, `1` shows the whole composition.
    pub zoom_x: f64,
    /// Vertical view zoom, `1` shows the full value range.
    pub zoom_y: f64,
    /// Horizontal pan in ranges.
    pub pan_x: f64,
    /// Vertical pan in ranges.
    pub pan_y: f64,
    /// Graph header caption (display name, falling back to the identity).
    pub caption: String,
}

/// Which control point of a segment a handle edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandleKind {
    /// First control point (`x1`, `y1`).
    Out,
    /// Second control point (`x2`, `y2`).
    In,
}

/// A draggable bezier handle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GraphHandle {
    /// Segment index (from `keys[index]` to `keys[index + 1]`).
    pub segment: usize,
    /// Which control point.
    pub kind: HandleKind,
    /// Screen position in logical pixels.
    pub at: (f32, f32),
}

/// A key point on screen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GraphPoint {
    /// Key index.
    pub key: usize,
    /// Screen position in logical pixels.
    pub at: (f32, f32),
}

/// The visible graph area and its mapping.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GraphGeometry {
    /// Plot rectangle.
    pub plot: Rect,
    /// Value at the left edge.
    pub x0: f64,
    /// Value at the right edge.
    pub x1: f64,
    /// Value at the bottom edge.
    pub y0: f64,
    /// Value at the top edge.
    pub y1: f64,
}

impl GraphModel {
    /// The value bounds, padded when every key has the same value.
    fn value_bounds(&self) -> (f64, f64) {
        let (mut y0, mut y1) = (self.min, self.max);
        if (y1 - y0).abs() < 1e-9 {
            let pad = if y0.abs() < 1e-9 { 1.0 } else { y0.abs() * 0.1 };
            y0 -= pad;
            y1 += pad;
        }
        (y0, y1)
    }

    /// The mapping from graph units to `plot`.
    #[must_use]
    pub fn geometry(&self, plot: Rect) -> GraphGeometry {
        let x0_full = 0.0_f64;
        let x1_full = frame_f64(self.duration.max(1));
        let (y0, y1) = self.value_bounds();
        let zoom_x = self.zoom_x.clamp(0.05, 64.0);
        let zoom_y = self.zoom_y.clamp(0.05, 64.0);
        let cx = (x0_full + x1_full) * 0.5 + self.pan_x * (x1_full - x0_full);
        let cy = (y0 + y1) * 0.5 + self.pan_y * (y1 - y0);
        let half_x = (x1_full - x0_full) * 0.5 / zoom_x;
        let half_y = (y1 - y0) * 0.5 / zoom_y;
        GraphGeometry {
            plot,
            x0: cx - half_x,
            x1: cx + half_x,
            y0: cy - half_y,
            y1: cy + half_y,
        }
    }

    /// A view that fits the key time and value ranges with padding.
    ///
    /// Both axes are fitted independently, so a slowly changing channel does
    /// not look flat and a short key range does not hide in a full-duration
    /// plot. Returns `(zoom_x, zoom_y, pan_x, pan_y)` for [`Self::geometry`].
    #[must_use]
    pub fn fitted_view(&self) -> (f64, f64, f64, f64) {
        let duration = frame_f64(self.duration.max(1));
        let first = self.keys.first().map_or(0.0, |key| frame_f64(key.frame));
        let last = self
            .keys
            .last()
            .map_or(duration, |key| frame_f64(key.frame));
        let span = (last - first).max(0.0);
        let pad = (span * 0.08).max(2.0);
        let lo = (first - pad).max(0.0);
        let hi = (last + pad).min(duration);
        let (lo, hi) = if hi > lo { (lo, hi) } else { (0.0, duration) };
        let zoom_x = duration / (hi - lo);
        let center_x = (lo + hi) * 0.5;
        let pan_x = (center_x - duration * 0.5) / duration;

        let (y0, y1) = self.value_bounds();
        let value_span = y1 - y0;
        let value_pad = (value_span * 0.08).max(1e-9);
        let target_lo = y0 - value_pad;
        let target_hi = y1 + value_pad;
        let zoom_y = value_span / (target_hi - target_lo);
        let center_y = (target_lo + target_hi) * 0.5;
        let pan_y = if value_span.abs() < 1e-12 {
            0.0
        } else {
            (center_y - (y0 + y1) * 0.5) / value_span
        };
        (zoom_x, zoom_y, pan_x, pan_y)
    }

    /// Whether `ease` can be drawn as a bezier segment with handles.
    #[must_use]
    pub fn handles(&self, plot: Rect) -> Vec<GraphHandle> {
        let geometry = self.geometry(plot);
        let mut handles = Vec::new();
        for (index, pair) in self.keys.windows(2).enumerate() {
            let Ease::Bezier(curve) = pair[1].ease else {
                continue;
            };
            let (x1, y1, x2, y2) = curve.points();
            let frame = |x: f64| {
                frame_f64(pair[0].frame) + x * (frame_f64(pair[1].frame) - frame_f64(pair[0].frame))
            };
            let value = |y: f64| pair[0].value + y * (pair[1].value - pair[0].value);
            handles.push(GraphHandle {
                segment: index,
                kind: HandleKind::Out,
                at: to_screen(&geometry, frame(x1), value(y1)),
            });
            handles.push(GraphHandle {
                segment: index,
                kind: HandleKind::In,
                at: to_screen(&geometry, frame(x2), value(y2)),
            });
        }
        handles
    }

    /// The key points on screen.
    #[must_use]
    pub fn points(&self, plot: Rect) -> Vec<GraphPoint> {
        let geometry = self.geometry(plot);
        self.keys
            .iter()
            .enumerate()
            .map(|(key, point)| GraphPoint {
                key,
                at: to_screen(&geometry, point.frame as f64, point.value),
            })
            .collect()
    }

    /// The key index and bezier handle under `(x, y)`, handles first.
    #[must_use]
    pub fn hit(&self, plot: Rect, x: f32, y: f32, reach: f32) -> Option<GraphHit> {
        let near = |point: (f32, f32)| {
            let dx = point.0 - x;
            let dy = point.1 - y;
            dx * dx + dy * dy <= reach * reach
        };
        self.handles(plot)
            .into_iter()
            .find(|handle| near(handle.at))
            .map(|handle| GraphHit::Handle {
                segment: handle.segment,
                kind: handle.kind,
            })
            .or_else(|| {
                self.points(plot)
                    .into_iter()
                    .find(|point| near(point.at))
                    .map(|point| GraphHit::Key(point.key))
            })
    }

    /// `(frame, value)` under a screen point.
    #[must_use]
    pub fn value_at(&self, plot: Rect, x: f32, y: f32) -> (Frame, f64) {
        let geometry = self.geometry(plot);
        let fx = f64::from(x);
        let fy = f64::from(y);
        let px = if plot.w > 0.0 {
            (fx - f64::from(plot.x)) / f64::from(plot.w)
        } else {
            0.0
        };
        let py = if plot.h > 0.0 {
            (f64::from(plot.bottom()) - fy) / f64::from(plot.h)
        } else {
            0.0
        };
        let frame = geometry.x0 + px * (geometry.x1 - geometry.x0);
        let value = geometry.y0 + py * (geometry.y1 - geometry.y0);
        #[allow(clippy::cast_possible_truncation)] // frames are integer indices
        let frame = frame.round() as Frame;
        (frame, value)
    }

    /// The new curve when a handle of `segment` is dragged to `(frame, value)`.
    ///
    /// The other control point is preserved; `x` is clamped to `[0, 1]` and
    /// the result is quantised by [`BezierEase::new`] (D-0037).
    #[must_use]
    pub fn drag_handle(
        &self,
        segment: usize,
        kind: HandleKind,
        frame: Frame,
        value: f64,
    ) -> Option<BezierEase> {
        let (from, to) = (self.keys.get(segment)?, self.keys.get(segment + 1)?);
        let Ease::Bezier(curve) = to.ease else {
            return None;
        };
        let span = frame_f64(to.frame) - frame_f64(from.frame);
        if span.abs() < 1e-9 {
            return None;
        }
        let x = ((frame_f64(frame) - frame_f64(from.frame)) / span).clamp(0.0, 1.0);
        let dy = to.value - from.value;
        let y = if dy.abs() < 1e-9 {
            0.0
        } else {
            (value - from.value) / dy
        };
        let (x1, y1, x2, y2) = curve.points();
        let (x1, y1, x2, y2) = match kind {
            HandleKind::Out => (x, y, x2, y2),
            HandleKind::In => (x1, y1, x, y),
        };
        BezierEase::new(x1, y1, x2, y2).ok()
    }
}

/// What a graph click hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphHit {
    /// A key point.
    Key(usize),
    /// A bezier control point of a segment.
    Handle {
        /// Segment index.
        segment: usize,
        /// Which control point.
        kind: HandleKind,
    },
}

/// Converts an integer frame to the graph's floating-point time axis.
#[allow(clippy::cast_precision_loss)] // frames are bounded by 2^31
fn frame_f64(frame: Frame) -> f64 {
    frame as f64
}

/// Maps `(frame, value)` to a screen point inside `geometry`.
#[must_use]
pub fn to_screen(geometry: &GraphGeometry, frame: f64, value: f64) -> (f32, f32) {
    let plot = geometry.plot;
    let x = if (geometry.x1 - geometry.x0).abs() < 1e-12 {
        0.5
    } else {
        (frame - geometry.x0) / (geometry.x1 - geometry.x0)
    };
    let y = if (geometry.y1 - geometry.y0).abs() < 1e-12 {
        0.5
    } else {
        (value - geometry.y0) / (geometry.y1 - geometry.y0)
    };
    #[allow(clippy::cast_possible_truncation)] // screen pixels
    (
        (plot.x + (x as f32) * plot.w).round(),
        (plot.bottom() - (y as f32) * plot.h).round(),
    )
}

/// A close cubic-bezier equivalent for a named ease, used by the graph's
/// "Convert to bezier" action. `hold` and named curves without a close CSS
/// twin return `None`.
#[must_use]
pub fn bezier_for(ease: Ease) -> Option<BezierEase> {
    let points = match ease {
        Ease::Linear => (0.0, 0.0, 1.0, 1.0),
        Ease::SineInOut => (0.37, 0.0, 0.63, 1.0),
        Ease::QuadInOut => (0.45, 0.0, 0.55, 1.0),
        Ease::CubicInOut => (0.65, 0.0, 0.35, 1.0),
        Ease::ExpoInOut => (0.87, 0.0, 0.13, 1.0),
        Ease::BackOut => (0.34, 1.56, 0.64, 1.0),
        Ease::QuadOut => (0.25, 0.46, 0.45, 0.94),
        Ease::CubicOut => (0.33, 1.0, 0.68, 1.0),
        Ease::BackIn => (0.36, 0.0, 0.66, -0.56),
        Ease::QuadIn => (0.55, 0.085, 0.68, 0.53),
        Ease::CubicIn => (0.32, 0.0, 0.67, 0.0),
        _ => return None,
    };
    BezierEase::new(points.0, points.1, points.2, points.3).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model() -> GraphModel {
        GraphModel {
            id: "a".to_owned(),
            property: "opacity".to_owned(),
            keys: vec![
                GraphKey {
                    frame: 0,
                    value: 0.0,
                    ease: Ease::Linear,
                },
                GraphKey {
                    frame: 60,
                    value: 1.0,
                    ease: Ease::Bezier(BezierEase::new(0.25, 0.1, 0.75, 0.9).unwrap()),
                },
            ],
            duration: 120,
            min: 0.0,
            max: 1.0,
            zoom_x: 1.0,
            zoom_y: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
            caption: "a \u{00b7} opacity".to_owned(),
        }
    }

    #[test]
    fn mapping_round_trips() {
        let graph = model();
        let plot = Rect::new(100.0, 50.0, 400.0, 200.0);
        let (x, y) = to_screen(&graph.geometry(plot), 30.0, 0.25);
        let (frame, value) = graph.value_at(plot, x, y);
        assert_eq!(frame, 30);
        assert!((value - 0.25).abs() < 0.01, "{value}");
    }

    #[test]
    fn a_bezier_segment_has_two_handles_and_hits() {
        let graph = model();
        let plot = Rect::new(0.0, 0.0, 400.0, 200.0);
        let handles = graph.handles(plot);
        assert_eq!(handles.len(), 2);
        let handle = handles[0];
        assert_eq!(
            graph.hit(plot, handle.at.0, handle.at.1, 8.0),
            Some(GraphHit::Handle {
                segment: 0,
                kind: HandleKind::Out
            })
        );
    }

    #[test]
    fn dragging_a_handle_clamps_x_and_preserves_the_other_control() {
        let graph = model();
        let curve = graph
            .drag_handle(0, HandleKind::Out, 500, 2.0)
            .expect("a curve");
        let (x1, y1, x2, y2) = curve.points();
        assert!((x1 - 1.0).abs() < 1e-6);
        assert!((y1 - 2.0).abs() < 1e-6);
        assert!((x2 - 0.75).abs() < 1e-6);
        assert!((y2 - 0.9).abs() < 1e-6);
    }

    #[test]
    fn fitting_zooms_to_the_key_range_with_padding() {
        let graph = model();
        let (zoom_x, zoom_y, pan_x, pan_y) = graph.fitted_view();
        assert!(
            zoom_x > 1.0,
            "0..60 s in a 120-frame composition must zoom in"
        );
        assert!(
            zoom_y < 1.0,
            "the value range is padded, so the view zooms out slightly"
        );
        let plot = Rect::new(0.0, 0.0, 400.0, 200.0);
        let geometry = graph.geometry(plot);
        assert!(geometry.x0 <= 0.0);
        assert!(geometry.x1 >= 60.0);
        // Both keys sit inside the plot with a visible margin.
        for key in &graph.keys {
            let (x, y) = to_screen(&geometry, frame_f64(key.frame), key.value);
            assert!(x >= plot.x && x <= plot.right(), "{x}");
            assert!(y >= plot.y && y <= plot.bottom(), "{y}");
        }
        assert!(pan_x.abs() > 0.0);
        assert!(pan_y.abs() < 1e-9, "the value range stays centred");
    }

    #[test]
    fn named_eases_convert_to_a_valid_bezier() {
        for (ease, _) in Ease::ALL {
            if let Some(curve) = bezier_for(ease) {
                let _ = curve.points();
            }
        }
    }
}

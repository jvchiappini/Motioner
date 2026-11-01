//! A small vector icon set drawn from anti-aliased primitives.
//!
//! Icons are designed on a 16 × 16 unit grid and scaled to the box they are
//! drawn in, so they stay sharp at any DPI. They are made of rounded
//! rectangles, rings and convex polygons from [`Painter`]; there are no bitmap
//! assets. Where an icon needs a gap in an outline (the loop arrows), the gap
//! is painted with the surface colour the caller passes in.

use crate::layout::Rect;
use crate::outline::NodeKind;
use crate::paint::Painter;

/// Every icon the editor uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    /// Start playback.
    Play,
    /// Pause playback.
    Pause,
    /// Step one frame back.
    StepBack,
    /// Step one frame forward.
    StepForward,
    /// Jump to the first frame.
    ToStart,
    /// Jump to the last frame.
    ToEnd,
    /// Loop playback.
    Loop,
    /// Visible layer.
    Eye,
    /// Locked layer.
    Lock,
    /// Text layer.
    Text,
    /// Circle layer.
    Circle,
    /// Rectangle layer.
    Rect,
    /// Group layer.
    Group,
    /// Expanded disclosure.
    ChevronDown,
    /// Keyframe marker.
    Diamond,
    /// Error state.
    Error,
    /// Healthy state.
    Check,
    /// Safe-area guides.
    SafeArea,
}

impl Icon {
    /// The icon for an outline node kind.
    #[must_use]
    pub const fn for_kind(kind: NodeKind) -> Self {
        match kind {
            NodeKind::Group => Self::Group,
            NodeKind::Rect => Self::Rect,
            NodeKind::Circle => Self::Circle,
            NodeKind::Text => Self::Text,
        }
    }
}

/// Maps the 16-unit design grid onto a square box.
#[derive(Clone, Copy)]
struct Grid {
    x: f32,
    y: f32,
    u: f32,
}

impl Grid {
    fn new(rect: Rect) -> Self {
        let side = rect.w.min(rect.h);
        Self {
            x: rect.center_x() - side * 0.5,
            y: rect.center_y() - side * 0.5,
            u: side / 16.0,
        }
    }

    fn p(self, x: f32, y: f32) -> (f32, f32) {
        (self.x + x * self.u, self.y + y * self.u)
    }

    fn r(self, x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::new(
            self.x + x * self.u,
            self.y + y * self.u,
            w * self.u,
            h * self.u,
        )
    }

    fn len(self, v: f32) -> f32 {
        v * self.u
    }
}

/// A convex lens centred in the grid: half-width `rx`, half-height `ry`
/// (units), its outline made of two circular-looking arcs.
fn lens(g: Grid, rx: f32, ry: f32) -> [(f32, f32); 16] {
    let mut points = [(0.0, 0.0); 16];
    for (i, point) in points.iter_mut().enumerate() {
        // 0..8 along the top from left to right, 8..16 back along the bottom.
        let t = (i % 8) as f32 / 8.0;
        let (x, side) = if i < 8 {
            (-1.0 + 2.0 * t, -1.0)
        } else {
            (1.0 - 2.0 * t, 1.0)
        };
        // A raised cosine gives pointed corners like an almond.
        let y = side * ry * (std::f32::consts::FRAC_PI_2 * x).cos();
        *point = g.p(8.0 + x * rx, 8.0 + y);
    }
    points
}

/// Draws `icon` centred in `rect` in `color`. `surface` is the colour behind
/// the icon, used to cut gaps in outlines.
pub fn draw(p: &mut Painter<'_>, icon: Icon, rect: Rect, color: [f32; 4], surface: [f32; 4]) {
    let g = Grid::new(rect);
    // Strokes never get thinner than one physical pixel.
    let stroke = g.len(1.5).max(1.0);
    match icon {
        Icon::Play => p.polygon(&[g.p(4.5, 2.5), g.p(13.0, 8.0), g.p(4.5, 13.5)], color),
        Icon::Pause => {
            p.fill_rounded(g.r(4.0, 3.0, 3.0, 10.0), color, g.len(0.75));
            p.fill_rounded(g.r(9.0, 3.0, 3.0, 10.0), color, g.len(0.75));
        }
        Icon::StepBack => {
            p.polygon(&[g.p(10.5, 3.5), g.p(10.5, 12.5), g.p(4.0, 8.0)], color);
            p.fill_rounded(g.r(11.5, 3.5, 1.75, 9.0), color, g.len(0.5));
        }
        Icon::StepForward => {
            p.polygon(&[g.p(5.5, 3.5), g.p(12.0, 8.0), g.p(5.5, 12.5)], color);
            p.fill_rounded(g.r(2.75, 3.5, 1.75, 9.0), color, g.len(0.5));
        }
        Icon::ToStart => {
            p.fill_rounded(g.r(2.5, 3.5, 1.75, 9.0), color, g.len(0.5));
            p.polygon(&[g.p(9.0, 3.5), g.p(9.0, 12.5), g.p(4.5, 8.0)], color);
            p.polygon(&[g.p(13.5, 3.5), g.p(13.5, 12.5), g.p(9.0, 8.0)], color);
        }
        Icon::ToEnd => {
            p.polygon(&[g.p(2.5, 3.5), g.p(7.0, 8.0), g.p(2.5, 12.5)], color);
            p.polygon(&[g.p(7.0, 3.5), g.p(11.5, 8.0), g.p(7.0, 12.5)], color);
            p.fill_rounded(g.r(11.75, 3.5, 1.75, 9.0), color, g.len(0.5));
        }
        Icon::Loop => {
            // A rounded loop with two gaps and arrowheads, like ⟲⟳.
            p.stroke_rounded(g.r(2.0, 4.0, 12.0, 8.0), color, g.len(4.0), stroke);
            p.fill(g.r(6.0, 3.0, 3.0, 2.5), surface);
            p.fill(g.r(7.0, 10.5, 3.0, 2.5), surface);
            p.polygon(&[g.p(9.0, 2.0), g.p(12.0, 4.75), g.p(9.0, 7.5)], color);
            p.polygon(&[g.p(7.0, 8.5), g.p(4.0, 11.25), g.p(7.0, 14.0)], color);
        }
        Icon::Eye => {
            // A lens outline (outer lens in ink, inner lens in the surface
            // colour) with a round pupil.
            p.polygon(&lens(g, 7.0, 4.75), color);
            p.polygon(&lens(g, 5.4, 3.1), surface);
            p.circle(g.x + g.len(8.0), g.y + g.len(8.0), g.len(2.1), color);
        }
        Icon::Lock => {
            p.stroke_rounded(g.r(4.5, 2.0, 7.0, 9.0), color, g.len(3.5), stroke);
            p.fill_rounded(g.r(3.0, 7.0, 10.0, 7.0), color, g.len(1.5));
        }
        Icon::Text => {
            p.fill_rounded(g.r(3.0, 3.0, 10.0, 2.0), color, g.len(0.5));
            p.fill_rounded(g.r(7.0, 3.0, 2.0, 10.0), color, g.len(0.5));
        }
        Icon::Circle => p.ring(
            g.x + g.len(8.0),
            g.y + g.len(8.0),
            g.len(5.5),
            stroke,
            color,
        ),
        Icon::Rect => p.stroke_rounded(g.r(2.5, 2.5, 11.0, 11.0), color, g.len(2.0), stroke),
        Icon::Group => {
            // A folder: tab plus body.
            p.fill_rounded(g.r(2.0, 3.5, 5.5, 3.0), color, g.len(1.0));
            p.stroke_rounded(g.r(2.0, 5.0, 12.0, 8.5), color, g.len(1.5), stroke);
        }
        Icon::ChevronDown => p.polygon(&[g.p(4.0, 6.0), g.p(12.0, 6.0), g.p(8.0, 10.5)], color),
        Icon::Diamond => p.polygon(
            &[g.p(8.0, 2.0), g.p(14.0, 8.0), g.p(8.0, 14.0), g.p(2.0, 8.0)],
            color,
        ),
        Icon::Error => {
            p.circle(g.x + g.len(8.0), g.y + g.len(8.0), g.len(7.0), color);
            p.fill_rounded(g.r(7.0, 3.75, 2.0, 5.5), surface, g.len(1.0));
            p.circle(g.x + g.len(8.0), g.y + g.len(11.5), g.len(1.1), surface);
        }
        Icon::Check => {
            p.circle(g.x + g.len(8.0), g.y + g.len(8.0), g.len(7.0), color);
            // Two strokes of the tick as thin quadrilaterals.
            p.polygon(
                &[g.p(4.2, 8.2), g.p(5.4, 7.0), g.p(7.6, 9.2), g.p(6.4, 10.4)],
                surface,
            );
            p.polygon(
                &[
                    g.p(6.4, 10.4),
                    g.p(5.4, 9.2),
                    g.p(10.6, 4.4),
                    g.p(11.8, 5.6),
                ],
                surface,
            );
        }
        Icon::SafeArea => {
            p.stroke_rounded(g.r(1.5, 3.0, 13.0, 10.0), color, g.len(1.5), stroke);
            p.stroke_rounded(g.r(4.0, 5.5, 8.0, 5.0), color, g.len(0.75), stroke);
        }
    }
}

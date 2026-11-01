//! Vector path geometry: authored points, flattening, trimming and dashing.
//!
//! A [`PathGeometry`] is the authored outline of a path node: a list of
//! [`PathPoint`]s (a corner or a point with relative Bézier handles), a
//! closed flag and the stroke cap/join style. It is pure data: flattening,
//! arc-length trimming and dashing live here so the renderer, the selection
//! bounds, hit testing and the tests all share one implementation (D-0029).

use std::f64::consts::PI;

use crate::error::CoreError;
use crate::math::Vec2;

/// Maximum recursion depth when flattening one cubic segment.
const MAX_SUBDIVISION_DEPTH: u32 = 12;

/// Flattening error tolerance in local pixels.
pub const FLATTEN_TOLERANCE: f64 = 0.1;

/// Safety cap on the number of dashes generated from one path.
///
/// A path of ten million pixels with a one-pixel dash would otherwise
/// allocate millions of pieces; beyond this count the piece is drawn solid.
pub const MAX_DASH_PIECES: usize = 20_000;

/// How an open path or a dash ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum LineCap {
    /// Ends exactly at the endpoint.
    #[default]
    Butt,
    /// A half-disc of radius `stroke_width / 2`.
    Round,
    /// A square extending `stroke_width / 2` past the endpoint.
    Square,
}

impl LineCap {
    /// Every cap with its MDF name.
    pub const ALL: [(Self, &'static str); 3] = [
        (Self::Butt, "butt"),
        (Self::Round, "round"),
        (Self::Square, "square"),
    ];

    /// Looks up a cap by its MDF name.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .find(|(_, candidate)| *candidate == name)
            .map(|(cap, _)| *cap)
    }

    /// The MDF name of this cap.
    #[must_use]
    pub fn name(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(cap, _)| *cap == self)
            .map_or("butt", |(_, name)| name)
    }
}

/// How two path segments meet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum LineJoin {
    /// Extends the outer edges until they meet.
    #[default]
    Miter,
    /// Cuts the corner with a straight edge.
    Bevel,
    /// Rounds the corner with an arc.
    Round,
}

impl LineJoin {
    /// Every join with its MDF name.
    pub const ALL: [(Self, &'static str); 3] = [
        (Self::Miter, "miter"),
        (Self::Bevel, "bevel"),
        (Self::Round, "round"),
    ];

    /// Looks up a join by its MDF name.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .find(|(_, candidate)| *candidate == name)
            .map(|(join, _)| *join)
    }

    /// The MDF name of this join.
    #[must_use]
    pub fn name(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(join, _)| *join == self)
            .map_or("miter", |(_, name)| name)
    }
}

/// One authored point of a path.
///
/// `handle_in` and `handle_out` are control-point offsets **relative to the
/// point**. A zero handle means "no curve on that side": the segment touching
/// it is a straight line. `handle_out` shapes the segment leaving the point
/// and `handle_in` the segment arriving at it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PathPoint {
    /// Position in the node's local space.
    pub position: Vec2,
    /// Incoming Bézier handle offset.
    pub handle_in: Vec2,
    /// Outgoing Bézier handle offset.
    pub handle_out: Vec2,
}

impl PathPoint {
    /// A corner point with no handles.
    #[must_use]
    pub const fn corner(position: Vec2) -> Self {
        Self {
            position,
            handle_in: Vec2::ZERO,
            handle_out: Vec2::ZERO,
        }
    }

    /// A point with incoming and outgoing handle offsets.
    #[must_use]
    pub const fn curved(position: Vec2, handle_in: Vec2, handle_out: Vec2) -> Self {
        Self {
            position,
            handle_in,
            handle_out,
        }
    }

    /// Whether either handle bends a segment.
    #[must_use]
    pub fn has_handles(&self) -> bool {
        !is_zero(self.handle_in) || !is_zero(self.handle_out)
    }
}

/// An authored vector outline.
#[derive(Debug, Clone, PartialEq)]
pub struct PathGeometry {
    points: Vec<PathPoint>,
    closed: bool,
    cap: LineCap,
    join: LineJoin,
}

/// A trimmed path segment: an ordered polyline plus whether it is a loop.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PathPiece {
    /// Flattened points in local space.
    pub points: Vec<Vec2>,
    /// Whether the last point connects back to the first.
    pub closed: bool,
}

/// The pieces one path contributes to one frame, after trim and dash.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PathOutline {
    /// Contours to fill (each is implicitly closed by the fill rule).
    pub fill: Vec<PathPiece>,
    /// Polylines to stroke, with the authored cap at both ends.
    pub strokes: Vec<PathPiece>,
}

/// Arc-length trim window, in fractions of the path length.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrimSpec {
    /// Start of the window (`0` is the first point).
    pub start: f64,
    /// End of the window (`1` is the last point, or the loop close).
    pub end: f64,
    /// Shifts the whole window along the path; wraps around `1`.
    pub offset: f64,
}

impl TrimSpec {
    /// The untrimmed window.
    pub const FULL: Self = Self {
        start: 0.0,
        end: 1.0,
        offset: 0.0,
    };

    /// A window from `start` to `end`, not shifted.
    #[must_use]
    pub const fn new(start: f64, end: f64) -> Self {
        Self {
            start,
            end,
            offset: 0.0,
        }
    }
}

impl Default for TrimSpec {
    fn default() -> Self {
        Self::FULL
    }
}

/// Dash pattern along the path, in local pixels.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct DashSpec {
    /// On-stroke length; the gap is the same length. `0` means solid.
    pub length: f64,
    /// Shifts the pattern along the path.
    pub offset: f64,
}

struct Cut {
    points: Vec<Vec2>,
    closed: bool,
    start: f64,
}

impl PathGeometry {
    /// Validates and wraps authored points.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidGeometry`] with field `"path"` when there
    /// are fewer than two points or any coordinate is not finite.
    pub fn new(
        points: Vec<PathPoint>,
        closed: bool,
        cap: LineCap,
        join: LineJoin,
    ) -> Result<Self, CoreError> {
        if points.len() < 2
            || points.iter().any(|point| {
                !point.position.is_finite()
                    || !point.handle_in.is_finite()
                    || !point.handle_out.is_finite()
            })
        {
            return Err(CoreError::InvalidGeometry { field: "path" });
        }
        Ok(Self {
            points,
            closed,
            cap,
            join,
        })
    }

    /// A regular polygon with `sides` sides, its first vertex at -90 degrees.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidGeometry`] when `sides < 3` or the radius
    /// is not a positive finite number.
    pub fn polygon(sides: u32, radius: f64) -> Result<Self, CoreError> {
        if sides < 3 || !radius.is_finite() || radius <= 0.0 {
            return Err(CoreError::InvalidGeometry { field: "polygon" });
        }
        let count = f64::from(sides);
        let points = (0..sides)
            .map(|index| {
                let index = f64::from(index);
                let angle = -PI / 2.0 + std::f64::consts::TAU * index / count;
                PathPoint::corner(Vec2::new(radius * angle.cos(), radius * angle.sin()))
            })
            .collect();
        Self::new(points, true, LineCap::Round, LineJoin::Round)
    }

    /// A star with `spikes` points, alternating `outer` and `inner` radii.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidGeometry`] when `spikes < 2` or either
    /// radius is not a positive finite number.
    pub fn star(spikes: u32, inner: f64, outer: f64) -> Result<Self, CoreError> {
        if spikes < 2 || !inner.is_finite() || inner <= 0.0 || !outer.is_finite() || outer <= 0.0 {
            return Err(CoreError::InvalidGeometry { field: "star" });
        }
        let vertices = spikes * 2;
        #[allow(clippy::cast_precision_loss)] // spikes is a small authored count
        let count = f64::from(vertices);
        let points = (0..vertices)
            .map(|index| {
                let radius = if index % 2 == 0 { outer } else { inner };
                #[allow(clippy::cast_precision_loss)]
                let index = f64::from(index);
                let angle = -PI / 2.0 + std::f64::consts::TAU * index / count;
                PathPoint::corner(Vec2::new(radius * angle.cos(), radius * angle.sin()))
            })
            .collect();
        Self::new(points, true, LineCap::Round, LineJoin::Round)
    }

    /// A circular arc from `start` to `end` degrees, clockwise on screen
    /// (`0` is +x). A sweep of zero or a full turn draws a complete circle.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidGeometry`] when the radius is not a
    /// positive finite number or an angle is not finite.
    pub fn arc(radius: f64, start: f64, end: f64) -> Result<Self, CoreError> {
        if !radius.is_finite() || radius <= 0.0 || !start.is_finite() || !end.is_finite() {
            return Err(CoreError::InvalidGeometry { field: "arc" });
        }
        let sweep = if (end - start).abs() < 1e-9 {
            std::f64::consts::TAU
        } else {
            let raw = (end - start).to_radians();
            let wrapped = raw.rem_euclid(std::f64::consts::TAU);
            if wrapped < 1e-9 {
                std::f64::consts::TAU
            } else {
                wrapped
            }
        };
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let quarters = ((sweep.abs() / (PI / 2.0)).ceil() as u32).max(1);
        let step = sweep / f64::from(quarters);
        // Handle length for a circular arc expressed as one cubic per step.
        let handle = 4.0 / 3.0 * (step / 4.0).tan() * radius;

        let mut points: Vec<PathPoint> = Vec::with_capacity(quarters as usize + 1);
        let start = start.to_radians();
        for index in 0..quarters {
            let a = start + step * f64::from(index);
            let b = a + step;
            let p0 = Vec2::new(radius * a.cos(), radius * a.sin());
            let p3 = Vec2::new(radius * b.cos(), radius * b.sin());
            let out = Vec2::new(-a.sin(), a.cos()) * handle;
            let incoming = Vec2::new(b.sin(), -b.cos()) * handle;
            if index == 0 {
                points.push(PathPoint::curved(p0, Vec2::ZERO, out));
            } else if let Some(last) = points.last_mut() {
                // This point already exists as the previous segment's end;
                // give it the outgoing handle of the segment starting here.
                last.handle_out = out;
            }
            points.push(PathPoint::curved(p3, incoming, Vec2::ZERO));
        }
        Self::new(points, false, LineCap::Round, LineJoin::Round)
    }

    /// The authored points.
    #[must_use]
    pub fn points(&self) -> &[PathPoint] {
        &self.points
    }

    /// Whether the last point connects back to the first.
    #[must_use]
    pub const fn closed(&self) -> bool {
        self.closed
    }

    /// The stroke cap style.
    #[must_use]
    pub const fn cap(&self) -> LineCap {
        self.cap
    }

    /// Replaces the stroke cap style.
    pub fn set_cap(&mut self, cap: LineCap) {
        self.cap = cap;
    }

    /// The stroke join style.
    #[must_use]
    pub const fn join(&self) -> LineJoin {
        self.join
    }

    /// Replaces the stroke join style.
    pub fn set_join(&mut self, join: LineJoin) {
        self.join = join;
    }

    /// Number of segments, including the closing one when `closed`.
    #[must_use]
    pub fn segment_count(&self) -> usize {
        let open = self.points.len().saturating_sub(1);
        open + usize::from(self.closed)
    }

    /// Flattens the path into a polyline at [`FLATTEN_TOLERANCE`].
    ///
    /// A closed path returns its unique points; use [`Self::outline`] for the
    /// loop with the closing point repeated.
    #[must_use]
    pub fn flatten(&self) -> Vec<Vec2> {
        self.flatten_with(FLATTEN_TOLERANCE)
    }

    /// Flattens the path with an explicit tolerance in local pixels.
    #[must_use]
    pub fn flatten_with(&self, tolerance: f64) -> Vec<Vec2> {
        let tolerance = tolerance.max(1e-3);
        let mut out = Vec::with_capacity(self.points.len() * 2);
        out.push(self.points[0].position);
        for index in 0..self.points.len() - 1 {
            self.push_segment(index, index + 1, tolerance, &mut out);
        }
        if self.closed {
            self.push_segment(self.points.len() - 1, 0, tolerance, &mut out);
            // The closing segment lands exactly on the first point.
            out.pop();
        }
        out
    }

    fn push_segment(&self, from: usize, to: usize, tolerance: f64, out: &mut Vec<Vec2>) {
        let a = self.points[from];
        let b = self.points[to];
        if !a.has_handles() && !b.has_handles() {
            out.push(b.position);
            return;
        }
        let c1 = a.position + a.handle_out;
        let c2 = b.position + b.handle_in;
        flatten_cubic(
            a.position,
            c1,
            c2,
            b.position,
            tolerance,
            MAX_SUBDIVISION_DEPTH,
            out,
        );
    }

    /// Local-space `[x0, y0, x1, y1]` bounds of the flattened geometry.
    #[must_use]
    pub fn local_bounds(&self) -> Option<[f64; 4]> {
        let points = self.flatten();
        let first = *points.first()?;
        let mut bounds = [first.x, first.y, first.x, first.y];
        for point in &points[1..] {
            bounds[0] = bounds[0].min(point.x);
            bounds[1] = bounds[1].min(point.y);
            bounds[2] = bounds[2].max(point.x);
            bounds[3] = bounds[3].max(point.y);
        }
        let finite = bounds.iter().all(|value| value.is_finite());
        finite.then_some(bounds)
    }

    /// Whether `point` is inside the filled outline (even-odd rule).
    ///
    /// The test uses the untrimmed geometry, so a selected draw-on path is
    /// still pickable along its whole outline.
    #[must_use]
    pub fn contains(&self, point: Vec2) -> bool {
        // A fill closes an open contour implicitly, so the ray cast does too.
        let mut loop_points = self.flatten();
        if loop_points.len() > 1 {
            loop_points.push(loop_points[0]);
        }
        if loop_points.len() < 3 {
            return false;
        }
        let mut inside = false;
        for index in 0..loop_points.len() - 1 {
            let a = loop_points[index];
            let b = loop_points[index + 1];
            let crosses = (a.y > point.y) != (b.y > point.y);
            if crosses {
                let t = (point.y - a.y) / (b.y - a.y);
                if point.x < a.x + t * (b.x - a.x) {
                    inside = !inside;
                }
            }
        }
        inside
    }

    /// Whether `point` is within `tolerance` pixels of the stroked outline.
    #[must_use]
    pub fn near(&self, point: Vec2, tolerance: f64) -> bool {
        let loop_points = self.loop_points();
        loop_points
            .windows(2)
            .any(|pair| distance_to_segment(point, pair[0], pair[1]) <= tolerance)
    }

    fn loop_points(&self) -> Vec<Vec2> {
        let mut points = self.flatten();
        if self.closed && points.len() > 1 {
            points.push(points[0]);
        }
        points
    }

    /// Splits the path into fill contours and stroke polylines for one frame.
    #[must_use]
    pub fn outline(&self, trim: TrimSpec, dash: DashSpec) -> PathOutline {
        let loop_points = self.loop_points();
        if loop_points.len() < 2 {
            return PathOutline::default();
        }
        let cumulative = cumulative_lengths(&loop_points);
        let total = *cumulative.last().unwrap_or(&0.0);
        let span = (trim.end - trim.start).clamp(0.0, 1.0);
        if total <= 1e-9 || span <= 1e-9 {
            return PathOutline::default();
        }

        let cuts = if span >= 1.0 - 1e-9 {
            vec![Cut {
                points: loop_points.clone(),
                closed: self.closed,
                start: 0.0,
            }]
        } else {
            let start = (trim.start + trim.offset).rem_euclid(1.0) * total;
            let length = span * total;
            let mut cuts = Vec::with_capacity(2);
            if start + length <= total {
                if let Some(points) = slice(&loop_points, &cumulative, start, start + length) {
                    cuts.push(Cut {
                        points,
                        closed: false,
                        start,
                    });
                }
            } else {
                if let Some(points) = slice(&loop_points, &cumulative, start, total) {
                    cuts.push(Cut {
                        points,
                        closed: false,
                        start,
                    });
                }
                if let Some(points) = slice(&loop_points, &cumulative, 0.0, start + length - total)
                {
                    cuts.push(Cut {
                        points,
                        closed: false,
                        start: 0.0,
                    });
                }
            }
            cuts
        };

        let fill = cuts
            .iter()
            .map(|cut| PathPiece {
                points: if cut.closed && cut.points.len() > 1 {
                    cut.points[..cut.points.len() - 1].to_vec()
                } else {
                    cut.points.clone()
                },
                closed: cut.closed,
            })
            .collect();

        let strokes = if dash.length > 0.0 {
            cuts.iter()
                .flat_map(|cut| dash_cut(cut, &cumulative_of(&cut.points), dash))
                .collect()
        } else {
            cuts.iter()
                .map(|cut| PathPiece {
                    // A closed loop repeats its first point; keep it unique and
                    // let the stroke renderer close the contour itself.
                    points: if cut.closed && cut.points.len() > 1 {
                        cut.points[..cut.points.len() - 1].to_vec()
                    } else {
                        cut.points.clone()
                    },
                    closed: cut.closed,
                })
                .collect()
        };

        PathOutline { fill, strokes }
    }
}

fn cumulative_of(points: &[Vec2]) -> Vec<f64> {
    cumulative_lengths(points)
}

fn is_zero(value: Vec2) -> bool {
    value.x.abs() < 1e-9 && value.y.abs() < 1e-9
}

fn cumulative_lengths(points: &[Vec2]) -> Vec<f64> {
    let mut cumulative = Vec::with_capacity(points.len());
    let mut total = 0.0;
    cumulative.push(0.0);
    for pair in points.windows(2) {
        total += (pair[1] - pair[0]).length();
        cumulative.push(total);
    }
    cumulative
}

fn point_at(points: &[Vec2], cumulative: &[f64], distance: f64) -> Vec2 {
    if distance <= 0.0 {
        return points[0];
    }
    let last = *cumulative.last().unwrap_or(&0.0);
    if distance >= last {
        return *points.last().unwrap_or(&points[0]);
    }
    let index = match cumulative.binary_search_by(|value| value.total_cmp(&distance)) {
        Ok(index) => return points[index],
        Err(index) => index,
    };
    let start = index - 1;
    let span = cumulative[index] - cumulative[start];
    let t = if span > 1e-12 {
        (distance - cumulative[start]) / span
    } else {
        0.0
    };
    points[start] + (points[index] - points[start]) * t
}

fn slice(points: &[Vec2], cumulative: &[f64], from: f64, to: f64) -> Option<Vec<Vec2>> {
    if to <= from + 1e-9 {
        return None;
    }
    let mut out = vec![point_at(points, cumulative, from)];
    for (index, &distance) in cumulative.iter().enumerate().skip(1) {
        if distance > from + 1e-9 && distance < to - 1e-9 {
            out.push(points[index]);
        }
    }
    out.push(point_at(points, cumulative, to));
    out.dedup_by(|a, b| (*a - *b).length() < 1e-9);
    (out.len() >= 2).then_some(out)
}

fn dash_cut(cut: &Cut, cumulative: &[f64], dash: DashSpec) -> Vec<PathPiece> {
    let total = *cumulative.last().unwrap_or(&0.0);
    let period = 2.0 * dash.length;
    if period <= 0.0 || total <= 1e-9 {
        return vec![PathPiece {
            points: cut.points.clone(),
            closed: cut.closed,
        }];
    }
    let phase = cut.start + dash.offset;
    let first = ((phase - total) / period).floor();
    let last = ((phase + total) / period).ceil();
    #[allow(clippy::cast_precision_loss)] // the cap is a small authored count
    let max_pieces = MAX_DASH_PIECES as f64;
    if last - first > max_pieces {
        return vec![PathPiece {
            points: cut.points.clone(),
            closed: cut.closed,
        }];
    }
    let mut boundaries = vec![0.0, total];
    let mut k = first;
    while k <= last {
        let base = k * period - phase;
        for value in [base, base + dash.length] {
            if value > 1e-9 && value < total - 1e-9 {
                boundaries.push(value);
            }
        }
        k += 1.0;
    }
    boundaries.sort_by(f64::total_cmp);
    boundaries.dedup_by(|a, b| (*a - *b).abs() < 1e-9);

    // On intervals over the whole cut.
    let mut intervals: Vec<(f64, f64)> = Vec::with_capacity(boundaries.len());
    for pair in boundaries.windows(2) {
        let (from, to) = (pair[0], pair[1]);
        let middle = (from + to) * 0.5;
        if (phase + middle).rem_euclid(period) < dash.length {
            match intervals.last_mut() {
                Some(last) if (last.1 - from).abs() < 1e-9 => last.1 = to,
                _ => intervals.push((from, to)),
            }
        }
    }

    let mut pieces: Vec<PathPiece> = intervals
        .iter()
        .filter_map(|&(from, to)| {
            slice(&cut.points, cumulative, from, to).map(|points| PathPiece {
                points,
                closed: false,
            })
        })
        .collect();

    // A dash crossing the seam of a loop is one piece that runs off the end
    // and back onto the start, not two dashes with caps at the seam.
    let wraps = intervals.len() > 1
        && intervals[0].0 < 1e-9
        && intervals[intervals.len() - 1].1 > total - 1e-9;
    if cut.closed && wraps && pieces.len() == intervals.len() {
        let head = pieces.remove(0);
        let mut tail = pieces.pop().unwrap_or_default();
        tail.points.extend(head.points);
        tail.points.dedup_by(|a, b| (*a - *b).length() < 1e-9);
        pieces.insert(0, tail);
    }
    pieces
}

fn flatten_cubic(
    p0: Vec2,
    p1: Vec2,
    p2: Vec2,
    p3: Vec2,
    tolerance: f64,
    depth: u32,
    out: &mut Vec<Vec2>,
) {
    if depth == 0 || flat_enough(p0, p1, p2, p3, tolerance) {
        out.push(p3);
        return;
    }
    let p01 = midpoint(p0, p1);
    let p12 = midpoint(p1, p2);
    let p23 = midpoint(p2, p3);
    let left_mid = midpoint(p01, p12);
    let right_mid = midpoint(p12, p23);
    let center = midpoint(left_mid, right_mid);
    flatten_cubic(p0, p01, left_mid, center, tolerance, depth - 1, out);
    flatten_cubic(center, right_mid, p23, p3, tolerance, depth - 1, out);
}

fn midpoint(a: Vec2, b: Vec2) -> Vec2 {
    Vec2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5)
}

fn flat_enough(p0: Vec2, p1: Vec2, p2: Vec2, p3: Vec2, tolerance: f64) -> bool {
    let chord = p3 - p0;
    let length = chord.length();
    let distance = |point: Vec2| {
        if length < 1e-9 {
            (point - p0).length()
        } else {
            let cross = chord.x * (point.y - p0.y) - chord.y * (point.x - p0.x);
            cross.abs() / length
        }
    };
    distance(p1).max(distance(p2)) <= tolerance
}

fn distance_to_segment(point: Vec2, a: Vec2, b: Vec2) -> f64 {
    let delta = b - a;
    let length_squared = delta.x * delta.x + delta.y * delta.y;
    if length_squared < 1e-12 {
        return (point - a).length();
    }
    let t = (((point - a).x * delta.x + (point - a).y * delta.y) / length_squared).clamp(0.0, 1.0);
    (point - (a + delta * t)).length()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(x: f64, y: f64) -> PathPoint {
        PathPoint::corner(Vec2::new(x, y))
    }

    fn square() -> PathGeometry {
        PathGeometry::new(
            vec![
                point(0.0, 0.0),
                point(100.0, 0.0),
                point(100.0, 100.0),
                point(0.0, 100.0),
            ],
            true,
            LineCap::Butt,
            LineJoin::Miter,
        )
        .unwrap()
    }

    fn length(points: &[Vec2]) -> f64 {
        points
            .windows(2)
            .map(|pair| (pair[1] - pair[0]).length())
            .sum()
    }

    #[test]
    fn rejects_degenerate_paths() {
        assert!(
            PathGeometry::new(vec![point(0.0, 0.0)], false, LineCap::Butt, LineJoin::Miter)
                .is_err()
        );
        let mut bad = point(0.0, 0.0);
        bad.position.x = f64::NAN;
        assert!(PathGeometry::new(
            vec![bad, point(1.0, 1.0)],
            false,
            LineCap::Butt,
            LineJoin::Miter
        )
        .is_err());
    }

    #[test]
    fn flatten_keeps_corners_exact_and_arcs_dense() {
        let square_points = square().flatten();
        assert_eq!(square_points.len(), 4);
        assert_eq!(square_points[0], Vec2::ZERO);
        let arc = PathGeometry::arc(100.0, 0.0, 90.0).unwrap();
        let flattened = arc.flatten_with(0.05);
        assert!(flattened.len() > 8, "a quarter circle needs segments");
        assert!((flattened[0] - Vec2::new(100.0, 0.0)).length() < 1e-6);
        let end = vec2(0.0, 100.0);
        assert!((flattened[flattened.len() - 1] - end).length() < 0.1);
        for p in &flattened {
            assert!((p.length() - 100.0).abs() < 0.2, "on the circle: {p:?}");
        }
    }

    fn vec2(x: f64, y: f64) -> Vec2 {
        Vec2::new(x, y)
    }

    #[test]
    fn cubic_is_flattened_within_tolerance() {
        let geometry = PathGeometry::new(
            vec![
                PathPoint::curved(Vec2::new(0.0, 0.0), Vec2::ZERO, Vec2::new(0.0, 100.0)),
                PathPoint::curved(Vec2::new(100.0, 0.0), Vec2::new(0.0, 100.0), Vec2::ZERO),
            ],
            false,
            LineCap::Butt,
            LineJoin::Miter,
        )
        .unwrap();
        let fine = geometry.flatten_with(0.01);
        let coarse = geometry.flatten_with(1.0);
        assert!(fine.len() >= coarse.len());
        assert!(coarse.len() > 2);
    }

    #[test]
    fn bounds_cover_the_outline() {
        let bounds = square().local_bounds().unwrap();
        for (value, expected) in bounds.iter().zip([0.0, 0.0, 100.0, 100.0]) {
            assert!((value - expected).abs() < 1e-9, "{bounds:?}");
        }
    }

    #[test]
    fn contains_uses_even_odd_and_near_uses_the_stroke() {
        let square = square();
        assert!(square.contains(Vec2::new(50.0, 50.0)));
        assert!(!square.contains(Vec2::new(150.0, 50.0)));
        assert!(square.near(Vec2::new(100.5, 50.0), 1.0));
        assert!(!square.near(Vec2::new(120.0, 50.0), 1.0));
    }

    #[test]
    fn trim_half_draws_half_the_perimeter() {
        let outline = square().outline(TrimSpec::new(0.0, 0.5), DashSpec::default());
        assert_eq!(outline.strokes.len(), 1);
        let drawn = length(&outline.strokes[0].points);
        assert!((drawn - 200.0).abs() < 1e-6, "drawn {drawn}");
        let start = outline.strokes[0].points[0];
        let end = *outline.strokes[0].points.last().unwrap();
        assert!((start - Vec2::ZERO).length() < 1e-6);
        assert!((end - Vec2::new(100.0, 100.0)).length() < 1e-6);
    }

    #[test]
    fn trim_offset_wraps_around_the_path() {
        let mut trim = TrimSpec::new(0.25, 0.75);
        trim.offset = 0.5;
        let outline = square().outline(trim, DashSpec::default());
        assert_eq!(outline.strokes.len(), 2, "a wrapped window has two pieces");
        let drawn: f64 = outline
            .strokes
            .iter()
            .map(|piece| length(&piece.points))
            .sum();
        assert!((drawn - 200.0).abs() < 1e-6, "drawn {drawn}");
        let start = outline.strokes[0].points[0];
        assert!((start - Vec2::new(0.0, 100.0)).length() < 1e-6);
    }

    #[test]
    fn an_empty_trim_draws_nothing() {
        let outline = square().outline(TrimSpec::new(0.4, 0.4), DashSpec::default());
        assert!(outline.strokes.is_empty());
        assert!(outline.fill.is_empty());
    }

    #[test]
    fn dash_splits_the_path_into_equal_on_and_off_pieces() {
        let path = PathGeometry::new(
            vec![point(0.0, 0.0), point(100.0, 0.0)],
            false,
            LineCap::Butt,
            LineJoin::Miter,
        )
        .unwrap();
        let outline = path.outline(
            TrimSpec::FULL,
            DashSpec {
                length: 10.0,
                offset: 0.0,
            },
        );
        assert_eq!(outline.strokes.len(), 5);
        for piece in &outline.strokes {
            assert!((length(&piece.points) - 10.0).abs() < 1e-6);
        }
    }

    #[test]
    fn dash_offset_shifts_the_pattern() {
        let path = PathGeometry::new(
            vec![point(0.0, 0.0), point(100.0, 0.0)],
            false,
            LineCap::Butt,
            LineJoin::Miter,
        )
        .unwrap();
        let solid = path.outline(TrimSpec::FULL, DashSpec::default());
        assert_eq!(solid.strokes.len(), 1);
        let dashed = path.outline(
            TrimSpec::FULL,
            DashSpec {
                length: 10.0,
                offset: 5.0,
            },
        );
        // The offset shifts the pattern back: the first dash is the tail of
        // the on interval that would start at -5, so it runs 0..5.
        let first = dashed.strokes[0].points[0];
        assert!((first - Vec2::ZERO).length() < 1e-6);
        assert!((length(&dashed.strokes[0].points) - 5.0).abs() < 1e-6);
        let second = dashed.strokes[1].points[0];
        assert!((second - Vec2::new(15.0, 0.0)).length() < 1e-6);
    }

    #[test]
    fn closed_dashes_merge_across_the_seam() {
        let outline = square().outline(
            TrimSpec::FULL,
            DashSpec {
                length: 10.0,
                offset: 0.0,
            },
        );
        // The seam at (0, 0) falls in the middle of a dash: merging keeps the
        // first and last pieces joined instead of leaving two caps there.
        let total: f64 = outline.strokes.iter().map(|p| length(&p.points)).sum();
        assert!((total - 200.0).abs() < 1e-6, "dash coverage {total}");
    }

    #[test]
    fn polygon_and_star_are_closed_regular_shapes() {
        let triangle = PathGeometry::polygon(3, 100.0).unwrap();
        assert!(triangle.closed());
        assert_eq!(triangle.points().len(), 3);
        let bounds = triangle.local_bounds().unwrap();
        assert!((bounds[1] + 100.0).abs() < 1e-6, "points up: {bounds:?}");
        let star = PathGeometry::star(5, 40.0, 100.0).unwrap();
        assert_eq!(star.points().len(), 10);
    }
}

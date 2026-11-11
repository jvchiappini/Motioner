//! Axis-aligned boxes and the vocabulary the reports share.
//!
//! Composition pixels, y down, origin at the top-left corner of the canvas.

use motioner_core::Vec2;
use motioner_eval::Affine2;

/// An axis-aligned box in composition pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Box2 {
    /// Left edge.
    pub min_x: f64,
    /// Top edge.
    pub min_y: f64,
    /// Right edge.
    pub max_x: f64,
    /// Bottom edge.
    pub max_y: f64,
}

impl Box2 {
    /// A box from edges; `None` when any edge is not finite or it is inverted.
    #[must_use]
    pub fn new(min_x: f64, min_y: f64, max_x: f64, max_y: f64) -> Option<Self> {
        let finite = [min_x, min_y, max_x, max_y]
            .iter()
            .all(|value| value.is_finite());
        (finite && max_x >= min_x && max_y >= min_y).then_some(Self {
            min_x,
            min_y,
            max_x,
            max_y,
        })
    }

    /// A box from a `[x0, y0, x1, y1]` array.
    #[must_use]
    pub fn from_array(bounds: [f64; 4]) -> Option<Self> {
        Self::new(bounds[0], bounds[1], bounds[2], bounds[3])
    }

    /// Width in pixels.
    #[must_use]
    pub fn width(&self) -> f64 {
        self.max_x - self.min_x
    }

    /// Height in pixels.
    #[must_use]
    pub fn height(&self) -> f64 {
        self.max_y - self.min_y
    }

    /// Area in square pixels.
    #[must_use]
    pub fn area(&self) -> f64 {
        self.width() * self.height()
    }

    /// Centre point.
    #[must_use]
    pub fn center(&self) -> Vec2 {
        Vec2::new(
            (self.min_x + self.max_x) * 0.5,
            (self.min_y + self.max_y) * 0.5,
        )
    }

    /// The smallest box containing both.
    #[must_use]
    pub fn union(self, other: Self) -> Self {
        Self {
            min_x: self.min_x.min(other.min_x),
            min_y: self.min_y.min(other.min_y),
            max_x: self.max_x.max(other.max_x),
            max_y: self.max_y.max(other.max_y),
        }
    }

    /// The overlap of two boxes; `None` when they do not touch.
    #[must_use]
    pub fn intersection(self, other: Self) -> Option<Self> {
        Self::new(
            self.min_x.max(other.min_x),
            self.min_y.max(other.min_y),
            self.max_x.min(other.max_x),
            self.max_y.min(other.max_y),
        )
    }

    /// Area shared with `other`, zero when disjoint.
    #[must_use]
    pub fn intersection_area(self, other: Self) -> f64 {
        self.intersection(other).map_or(0.0, |box_| box_.area())
    }

    /// Whether `point` is inside (edges included).
    #[must_use]
    pub fn contains(&self, point: Vec2) -> bool {
        point.x >= self.min_x
            && point.x <= self.max_x
            && point.y >= self.min_y
            && point.y <= self.max_y
    }

    /// Whether `other` lies entirely inside this box.
    #[must_use]
    pub fn contains_box(&self, other: Self) -> bool {
        other.min_x >= self.min_x
            && other.min_y >= self.min_y
            && other.max_x <= self.max_x
            && other.max_y <= self.max_y
    }

    /// This box scaled towards its centre by `factor` (use `0.9` for title
    /// safe).
    #[must_use]
    pub fn scaled(self, factor: f64) -> Self {
        let center = self.center();
        let half_w = self.width() * 0.5 * factor;
        let half_h = self.height() * 0.5 * factor;
        Self {
            min_x: center.x - half_w,
            min_y: center.y - half_h,
            max_x: center.x + half_w,
            max_y: center.y + half_h,
        }
    }

    /// This box grown by `pixels` on every side.
    #[must_use]
    pub fn expanded(self, pixels: f64) -> Self {
        Self {
            min_x: self.min_x - pixels,
            min_y: self.min_y - pixels,
            max_x: self.max_x + pixels,
            max_y: self.max_y + pixels,
        }
    }

    /// The box mapped into composition space and re-aligned to the axes.
    ///
    /// A rotated item maps to the bounding box of its four mapped corners.
    #[must_use]
    pub fn transformed(self, transform: Affine2) -> Option<Self> {
        let corners = [
            Vec2::new(self.min_x, self.min_y),
            Vec2::new(self.max_x, self.min_y),
            Vec2::new(self.max_x, self.max_y),
            Vec2::new(self.min_x, self.max_y),
        ];
        let mapped = corners.map(|corner| transform.apply(corner));
        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        for point in mapped {
            min_x = min_x.min(point.x);
            min_y = min_y.min(point.y);
            max_x = max_x.max(point.x);
            max_y = max_y.max(point.y);
        }
        Self::new(min_x, min_y, max_x, max_y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlap_is_the_smaller_shared_area() {
        let a = Box2::new(0.0, 0.0, 10.0, 10.0).unwrap();
        let b = Box2::new(5.0, 5.0, 20.0, 20.0).unwrap();
        assert!((a.intersection_area(b) - 25.0).abs() < 1e-9);
        let c = Box2::new(50.0, 50.0, 60.0, 60.0).unwrap();
        assert!(a.intersection(c).is_none());
        assert!(a.intersection_area(c).abs() < 1e-9);
    }

    #[test]
    fn title_safe_scales_around_the_centre() {
        let canvas = Box2::new(0.0, 0.0, 1000.0, 1000.0).unwrap();
        let safe = canvas.scaled(0.8);
        assert!((safe.min_x - 100.0).abs() < 1e-9);
        assert!((safe.max_y - 900.0).abs() < 1e-9);
    }

    #[test]
    fn rotation_maps_to_the_corner_bounds() {
        let transform = Affine2::rotation_degrees(90.0);
        let local = Box2::new(-1.0, -2.0, 1.0, 2.0).unwrap();
        let mapped = local.transformed(transform).unwrap();
        assert!((mapped.width() - 4.0).abs() < 1e-9, "{mapped:?}");
        assert!((mapped.height() - 2.0).abs() < 1e-9, "{mapped:?}");
    }
}

//! 2D affine transforms in composition pixel space (y down).

use motioner_core::Vec2;

/// A 2D affine transform mapping `(x, y)` to
/// `(a*x + c*y + tx, b*x + d*y + ty)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Affine2 {
    /// First column, x component.
    pub a: f64,
    /// First column, y component.
    pub b: f64,
    /// Second column, x component.
    pub c: f64,
    /// Second column, y component.
    pub d: f64,
    /// Translation x.
    pub tx: f64,
    /// Translation y.
    pub ty: f64,
}

impl Affine2 {
    /// The identity transform.
    pub const IDENTITY: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        tx: 0.0,
        ty: 0.0,
    };

    /// A pure translation.
    #[must_use]
    pub const fn translation(offset: Vec2) -> Self {
        Self {
            tx: offset.x,
            ty: offset.y,
            ..Self::IDENTITY
        }
    }

    /// A pure scale.
    #[must_use]
    pub const fn scale(factors: Vec2) -> Self {
        Self {
            a: factors.x,
            d: factors.y,
            ..Self::IDENTITY
        }
    }

    /// A clockwise rotation in degrees (y points down, so a positive angle
    /// turns the x axis toward the y axis).
    #[must_use]
    pub fn rotation_degrees(degrees: f64) -> Self {
        let (sin, cos) = degrees.to_radians().sin_cos();
        Self {
            a: cos,
            b: sin,
            c: -sin,
            d: cos,
            ..Self::IDENTITY
        }
    }

    /// The After Effects layer transform:
    /// `translate(position) * rotate(rotation) * scale(scale) * translate(-anchor)`.
    #[must_use]
    pub fn layer(position: Vec2, rotation_degrees: f64, scale: Vec2, anchor: Vec2) -> Self {
        Self::translation(position)
            .then(Self::rotation_degrees(rotation_degrees))
            .then(Self::scale(scale))
            .then(Self::translation(Vec2::ZERO - anchor))
    }

    /// Returns `self * inner`: `inner` is applied first, then `self`.
    #[must_use]
    pub fn then(self, inner: Self) -> Self {
        Self {
            a: self.a * inner.a + self.c * inner.b,
            b: self.b * inner.a + self.d * inner.b,
            c: self.a * inner.c + self.c * inner.d,
            d: self.b * inner.c + self.d * inner.d,
            tx: self.a * inner.tx + self.c * inner.ty + self.tx,
            ty: self.b * inner.tx + self.d * inner.ty + self.ty,
        }
    }

    /// Transforms a point.
    #[must_use]
    pub fn apply(self, point: Vec2) -> Vec2 {
        Vec2::new(
            self.a * point.x + self.c * point.y + self.tx,
            self.b * point.x + self.d * point.y + self.ty,
        )
    }

    /// Transforms a vector, ignoring the translation.
    ///
    /// Path handles are vectors relative to their point, so this is what moves
    /// a curved path into another space.
    #[must_use]
    pub fn apply_vector(self, vector: Vec2) -> Vec2 {
        Vec2::new(
            self.a * vector.x + self.c * vector.y,
            self.b * vector.x + self.d * vector.y,
        )
    }

    /// The determinant; zero means the transform collapses area.
    #[must_use]
    pub fn determinant(self) -> f64 {
        self.a * self.d - self.b * self.c
    }

    /// Whether every matrix component is finite.
    ///
    /// A non-finite matrix cannot be rendered; the evaluator uses this to
    /// drop items produced by overflowing scale chains.
    #[must_use]
    pub fn is_finite(self) -> bool {
        self.a.is_finite()
            && self.b.is_finite()
            && self.c.is_finite()
            && self.d.is_finite()
            && self.tx.is_finite()
            && self.ty.is_finite()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Vec2, b: Vec2) -> bool {
        (a.x - b.x).abs() < 1e-9 && (a.y - b.y).abs() < 1e-9
    }

    #[test]
    fn layer_transform_follows_after_effects_order() {
        let m = Affine2::layer(
            Vec2::new(100.0, 50.0),
            90.0,
            Vec2::splat(2.0),
            Vec2::new(10.0, 0.0),
        );
        // The anchor maps to the position.
        assert!(close(m.apply(Vec2::new(10.0, 0.0)), Vec2::new(100.0, 50.0)));
        // One unit right of the anchor: scaled by 2, rotated clockwise to +y.
        assert!(close(m.apply(Vec2::new(11.0, 0.0)), Vec2::new(100.0, 52.0)));
    }

    #[test]
    fn composition_applies_inner_first() {
        let outer = Affine2::translation(Vec2::new(5.0, 0.0));
        let inner = Affine2::scale(Vec2::splat(3.0));
        assert!(close(
            outer.then(inner).apply(Vec2::new(1.0, 1.0)),
            Vec2::new(8.0, 3.0)
        ));
    }

    #[test]
    fn vectors_ignore_the_translation() {
        let matrix =
            Affine2::translation(Vec2::new(100.0, -40.0)).then(Affine2::scale(Vec2::new(2.0, 3.0)));
        assert!(close(
            matrix.apply_vector(Vec2::new(1.0, 2.0)),
            Vec2::new(2.0, 6.0)
        ));
    }
}

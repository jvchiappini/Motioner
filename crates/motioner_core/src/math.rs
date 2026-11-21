//! Minimal 2D vector math shared by the document and the evaluator.

use std::ops::{Add, Mul, Sub};

/// A 2D vector in composition pixels (x right, y down).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec2 {
    /// Horizontal component.
    pub x: f64,
    /// Vertical component (grows downward).
    pub y: f64,
}

impl Vec2 {
    /// The zero vector.
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };
    /// The vector `(1, 1)`, used as the identity scale.
    pub const ONE: Self = Self { x: 1.0, y: 1.0 };

    /// Creates a vector from its components.
    #[must_use]
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// Creates a vector with both components equal to `value`.
    #[must_use]
    pub const fn splat(value: f64) -> Self {
        Self { x: value, y: value }
    }

    /// The Euclidean length of the vector.
    #[must_use]
    pub fn length(self) -> f64 {
        self.x.hypot(self.y)
    }

    /// The squared length, cheaper when only comparisons are needed.
    #[must_use]
    pub fn length_squared(self) -> f64 {
        self.x * self.x + self.y * self.y
    }

    /// Whether both components are finite.
    #[must_use]
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }

    /// Linear interpolation towards `other` by `t`.
    #[must_use]
    pub fn lerp(self, other: Self, t: f64) -> Self {
        Self::new(
            self.x + (other.x - self.x) * t,
            self.y + (other.y - self.y) * t,
        )
    }
}

impl Add for Vec2 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl Sub for Vec2 {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl Mul<f64> for Vec2 {
    type Output = Self;
    fn mul(self, rhs: f64) -> Self {
        Self::new(self.x * rhs, self.y * rhs)
    }
}

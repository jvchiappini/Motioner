//! Expression drivers: a tiny, sandboxed IR the DSL compiles MDF functions
//! into (D-0052).
//!
//! A driver is a pure function of the rig's property values at one frame. It
//! cannot create nodes, read files, or run statements: the DSL rejects
//! anything outside this subset with a typed error naming the driver. The IR
//! is `Copy`-free but allocation-free once built, so evaluating it per frame
//! does not allocate.

use std::collections::BTreeMap;
use std::fmt;

use crate::PropValue;

/// Why a driver evaluation failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriverError {
    /// Driver name.
    pub driver: String,
    /// What went wrong.
    pub message: String,
}

impl fmt::Display for DriverError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "driver '{}': {}", self.driver, self.message)
    }
}

impl std::error::Error for DriverError {}

/// The part channels a driver may write.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Field {
    /// Horizontal position offset.
    PositionX,
    /// Vertical position offset.
    PositionY,
    /// Rotation offset in degrees.
    Rotation,
    /// Horizontal scale multiplier.
    ScaleX,
    /// Vertical scale multiplier.
    ScaleY,
    /// Absolute opacity.
    Opacity,
    /// Absolute stroke width.
    StrokeWidth,
}

impl Field {
    /// Parses an MDF field name (`position.y`, `y`, `scale.x`, ...).
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "position.x" | "pos.x" | "x" => Some(Self::PositionX),
            "position.y" | "pos.y" | "y" => Some(Self::PositionY),
            "rotation" | "rot" => Some(Self::Rotation),
            "scale.x" | "scale_x" => Some(Self::ScaleX),
            "scale.y" | "scale_y" => Some(Self::ScaleY),
            "opacity" => Some(Self::Opacity),
            "stroke_width" => Some(Self::StrokeWidth),
            _ => None,
        }
    }

    /// Every field name accepted by [`Self::from_name`], for diagnostics.
    pub const NAMES: [&'static str; 9] = [
        "position.x",
        "position.y",
        "rotation",
        "scale.x",
        "scale.y",
        "opacity",
        "stroke_width",
        "x",
        "y",
    ];
}

/// One write a driver performs.
#[derive(Debug, Clone, PartialEq)]
pub struct Write {
    /// Part index.
    pub part: usize,
    /// Channel to write.
    pub field: Field,
    /// Value expression.
    pub value: Expr,
}

/// The expression subset drivers may use.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// A literal.
    Number(f64),
    /// A property value as a number (`false` reads as 0, `true` as 1).
    Property(usize),
    /// A vector property's X component.
    VectorX(usize),
    /// A vector property's Y component.
    VectorY(usize),
    /// `a + b`.
    Add(Box<Expr>, Box<Expr>),
    /// `a - b`.
    Sub(Box<Expr>, Box<Expr>),
    /// `a * b`.
    Mul(Box<Expr>, Box<Expr>),
    /// `a / b`; division by zero yields 0.
    Div(Box<Expr>, Box<Expr>),
    /// `-a`.
    Neg(Box<Expr>),
    /// `min(a, b)`.
    Min(Box<Expr>, Box<Expr>),
    /// `max(a, b)`.
    Max(Box<Expr>, Box<Expr>),
    /// `abs(a)`.
    Abs(Box<Expr>),
    /// `sin(a)` (radians).
    Sin(Box<Expr>),
    /// `cos(a)` (radians).
    Cos(Box<Expr>),
    /// `clamp(value, low, high)`.
    Clamp(Box<Expr>, Box<Expr>, Box<Expr>),
    /// `lerp(a, b, t)`.
    Lerp(Box<Expr>, Box<Expr>, Box<Expr>),
    /// `a < b`.
    Less(Box<Expr>, Box<Expr>),
    /// `a > b`.
    Greater(Box<Expr>, Box<Expr>),
    /// `a and b`.
    And(Box<Expr>, Box<Expr>),
    /// `a or b`.
    Or(Box<Expr>, Box<Expr>),
    /// `not a`.
    Not(Box<Expr>),
}

/// Maximum expression depth a driver may use.
pub const MAX_DEPTH: usize = 64;
/// Maximum writes one driver may perform in one frame.
pub const MAX_WRITES: usize = 64;

impl Expr {
    /// Evaluates the expression against the property values at one frame.
    ///
    /// # Errors
    ///
    /// Returns [`DriverError`] when the expression is deeper than
    /// the maximum expression depth or evaluates a non-finite number.
    pub fn eval(
        &self,
        properties: &BTreeMap<usize, PropValue>,
        driver: &str,
    ) -> Result<f64, DriverError> {
        self.eval_depth(properties, driver, 0)
    }

    fn eval_depth(
        &self,
        properties: &BTreeMap<usize, PropValue>,
        driver: &str,
        depth: usize,
    ) -> Result<f64, DriverError> {
        if depth > MAX_DEPTH {
            return Err(DriverError {
                driver: driver.to_owned(),
                message: format!("expression is deeper than {MAX_DEPTH}"),
            });
        }
        let evaluate = |expression: &Expr| expression.eval_depth(properties, driver, depth + 1);
        let value = match self {
            Self::Number(value) => *value,
            Self::Property(index) => number_of(properties.get(index)),
            Self::VectorX(index) => properties
                .get(index)
                .and_then(PropValue::vector)
                .map_or(0.0, |vector| vector.x),
            Self::VectorY(index) => properties
                .get(index)
                .and_then(PropValue::vector)
                .map_or(0.0, |vector| vector.y),
            Self::Add(a, b) => evaluate(a)? + evaluate(b)?,
            Self::Sub(a, b) => evaluate(a)? - evaluate(b)?,
            Self::Mul(a, b) => evaluate(a)? * evaluate(b)?,
            Self::Div(a, b) => {
                let divisor = evaluate(b)?;
                if divisor == 0.0 {
                    0.0
                } else {
                    evaluate(a)? / divisor
                }
            }
            Self::Neg(a) => -evaluate(a)?,
            Self::Min(a, b) => evaluate(a)?.min(evaluate(b)?),
            Self::Max(a, b) => evaluate(a)?.max(evaluate(b)?),
            Self::Abs(a) => evaluate(a)?.abs(),
            Self::Sin(a) => evaluate(a)?.sin(),
            Self::Cos(a) => evaluate(a)?.cos(),
            Self::Clamp(value, low, high) => {
                let (value, low, high) = (evaluate(value)?, evaluate(low)?, evaluate(high)?);
                value.max(low).min(high.max(low))
            }
            Self::Lerp(a, b, t) => {
                let (a, b, t) = (evaluate(a)?, evaluate(b)?, evaluate(t)?);
                a + (b - a) * t
            }
            Self::Less(a, b) => f64::from(u8::from(evaluate(a)? < evaluate(b)?)),
            Self::Greater(a, b) => f64::from(u8::from(evaluate(a)? > evaluate(b)?)),
            Self::And(a, b) => f64::from(u8::from(evaluate(a)? != 0.0 && evaluate(b)? != 0.0)),
            Self::Or(a, b) => f64::from(u8::from(evaluate(a)? != 0.0 || evaluate(b)? != 0.0)),
            Self::Not(a) => f64::from(u8::from(evaluate(a)? == 0.0)),
        };
        if value.is_finite() {
            Ok(value)
        } else {
            Err(DriverError {
                driver: driver.to_owned(),
                message: "expression produced a non-finite value".to_owned(),
            })
        }
    }
}

fn number_of(value: Option<&PropValue>) -> f64 {
    match value {
        Some(PropValue::Number(value)) => *value,
        Some(PropValue::Bool(value)) => f64::from(u8::from(*value)),
        Some(PropValue::Text(_) | PropValue::Vector(_) | PropValue::Color(_)) | None => 0.0,
    }
}

/// A compiled driver.
#[derive(Debug, Clone, PartialEq)]
pub struct Driver {
    /// Function name, for errors and tools.
    pub name: String,
    /// Property indices bound to the function parameters, in order.
    pub params: Vec<usize>,
    /// Writes, in order.
    pub writes: Vec<Write>,
}

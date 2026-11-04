//! Shared plumbing for host calls: errors without spans, argument binding
//! and typed argument access.
//!
//! Builtins, scene constructors, node methods and user functions all bind
//! arguments through [`bind`], so positional/keyword rules and error
//! messages are identical everywhere.

use std::rc::Rc;

use motioner_core::{Color, Gradient, Vec2};

use crate::diagnostic::{ErrorCategory, Span};
use crate::origins::{ExpressionOrigin, Literal};
use crate::value::Value;

/// A paint argument: a solid colour or a gradient (D-0030).
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum PaintInput {
    Color(Color),
    Gradient(Rc<Gradient>),
}

/// A host-side error. The interpreter attaches the span and call stack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HostError {
    pub category: ErrorCategory,
    pub message: String,
}

impl HostError {
    pub fn new(category: ErrorCategory, message: impl Into<String>) -> Self {
        Self {
            category,
            message: message.into(),
        }
    }

    pub fn type_error(message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::Type, message)
    }

    pub fn value_error(message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::Value, message)
    }

    pub fn limit(message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::Limit, message)
    }
}

pub(crate) type HostResult<T> = Result<T, HostError>;

/// One evaluated call argument: its value and where it came from.
#[derive(Debug, Clone)]
pub(crate) struct CallValue {
    pub value: Value,
    /// Span of the argument expression in the source.
    pub span: Span,
    /// The literal shape of the expression, when it is a plain literal.
    pub literal: Option<Literal>,
    /// Literal, parameter reference, loop variable or computed.
    pub origin: ExpressionOrigin,
}

/// Evaluated call arguments.
#[derive(Debug, Default)]
pub(crate) struct CallArgs {
    pub positional: Vec<CallValue>,
    pub keywords: Vec<(Rc<str>, CallValue)>,
}

/// Arguments bound to a parameter list, with typed accessors that report
/// errors against the parameter name.
pub(crate) struct Bound<'a> {
    function: &'a str,
    params: &'a [&'a str],
    values: Vec<Option<CallValue>>,
}

/// Binds positional then keyword arguments to `params`. The first
/// `required` parameters must be provided.
pub(crate) fn bind<'a>(
    function: &'a str,
    params: &'a [&'a str],
    required: usize,
    args: CallArgs,
) -> HostResult<Bound<'a>> {
    if args.positional.len() > params.len() {
        return Err(HostError::type_error(format!(
            "{function}() takes at most {} positional arguments but {} were given",
            params.len(),
            args.positional.len()
        )));
    }
    let mut values: Vec<Option<CallValue>> = vec![None; params.len()];
    for (slot, value) in values.iter_mut().zip(args.positional) {
        *slot = Some(value);
    }
    for (name, value) in args.keywords {
        let Some(index) = params.iter().position(|p| **p == *name) else {
            return Err(HostError::type_error(format!(
                "{function}() got an unexpected keyword argument '{name}'"
            )));
        };
        if values[index].is_some() {
            return Err(HostError::type_error(format!(
                "{function}() got multiple values for argument '{name}'"
            )));
        }
        values[index] = Some(value);
    }
    if let Some(missing) = (0..required).find(|&i| values[i].is_none()) {
        return Err(HostError::type_error(format!(
            "{function}() is missing required argument '{}'",
            params[missing]
        )));
    }
    Ok(Bound {
        function,
        params,
        values,
    })
}

impl Bound<'_> {
    /// Takes every bound value in parameter order; `None` for omitted
    /// optional parameters.
    pub fn into_values(self) -> Vec<Option<Value>> {
        self.values
            .into_iter()
            .map(|value| value.map(|value| value.value))
            .collect()
    }

    pub fn get(&self, index: usize) -> Option<&Value> {
        self.values
            .get(index)
            .and_then(Option::as_ref)
            .map(|value| &value.value)
    }

    /// The source span and literal shape of parameter `index`, when given.
    pub fn source(&self, index: usize) -> Option<(Span, Option<Literal>)> {
        self.values
            .get(index)
            .and_then(Option::as_ref)
            .map(|value| (value.span, value.literal))
    }

    /// The evaluated argument bound to `index`, when given.
    pub fn argument(&self, index: usize) -> Option<&CallValue> {
        self.values.get(index).and_then(Option::as_ref)
    }

    fn mismatch(&self, index: usize, expected: &str, got: &Value) -> HostError {
        HostError::type_error(format!(
            "{}(): '{}' must be {expected}, got {}",
            self.function,
            self.params[index],
            got.type_name()
        ))
    }

    pub fn number_or(&self, index: usize, default: f64) -> HostResult<f64> {
        match self.get(index) {
            None => Ok(default),
            Some(_) => self.number(index),
        }
    }

    /// A required number. `bind` guarantees required arguments exist.
    pub fn number(&self, index: usize) -> HostResult<f64> {
        match self.get(index) {
            Some(Value::Number(n)) => Ok(*n),
            Some(other) => Err(self.mismatch(index, "a number", other)),
            None => Err(HostError::type_error(format!(
                "{}() is missing required argument '{}'",
                self.function, self.params[index]
            ))),
        }
    }

    pub fn string_or(&self, index: usize, default: &str) -> HostResult<Rc<str>> {
        match self.get(index) {
            None => Ok(Rc::from(default)),
            Some(Value::Str(s)) => Ok(s.clone()),
            Some(other) => Err(self.mismatch(index, "a string", other)),
        }
    }

    /// A color string, `None`, or the default when omitted.
    /// A colour, a gradient, or `None` for a paint argument (D-0030).
    pub fn optional_paint(
        &self,
        index: usize,
        default: Option<PaintInput>,
    ) -> HostResult<Option<PaintInput>> {
        match self.get(index) {
            None => Ok(default),
            Some(Value::None) => Ok(None),
            Some(Value::Str(s)) => Color::parse_hex(s)
                .map(|color| Some(PaintInput::Color(color)))
                .map_err(|e| HostError::value_error(format!("{}(): {e}", self.function))),
            Some(Value::Gradient(gradient)) => Ok(Some(PaintInput::Gradient(Rc::clone(gradient)))),
            Some(other) => Err(self.mismatch(index, "a colour, a gradient or None", other)),
        }
    }

    pub fn optional_color(
        &self,
        index: usize,
        default: Option<Color>,
    ) -> HostResult<Option<Color>> {
        match self.get(index) {
            None => Ok(default),
            Some(Value::None) => Ok(None),
            Some(Value::Str(s)) => Color::parse_hex(s)
                .map(Some)
                .map_err(|e| HostError::value_error(format!("{}(): {e}", self.function))),
            Some(other) => Err(self.mismatch(index, "a color string or None", other)),
        }
    }

    pub fn color_or(&self, index: usize, default: Color) -> HostResult<Color> {
        self.optional_color(index, Some(default))?.ok_or_else(|| {
            HostError::type_error(format!(
                "{}(): '{}' cannot be None",
                self.function, self.params[index]
            ))
        })
    }

    /// An integral number in `min..=max`.
    pub fn integer_in(&self, index: usize, default: i64, min: i64, max: i64) -> HostResult<i64> {
        let value = self.number_or(index, f64_from_i64(default))?;
        as_integer(value)
            .filter(|v| (min..=max).contains(v))
            .ok_or_else(|| {
                HostError::value_error(format!(
                    "{}(): '{}' must be a whole number from {min} to {max}, got {}",
                    self.function,
                    self.params[index],
                    crate::value::format_number(value)
                ))
            })
    }
}

/// Converts a value to a 2D vector: a 2-item tuple or list of numbers.
pub(crate) fn as_vec2(value: &Value) -> Option<Vec2> {
    let pair = |items: &[Value]| match items {
        [Value::Number(x), Value::Number(y)] => Some(Vec2::new(*x, *y)),
        _ => None,
    };
    match value {
        Value::Tuple(items) | Value::List(items) => pair(&items.borrow()),
        _ => None,
    }
}

/// Converts a 2D vector to an MDF tuple.
pub(crate) fn vec2_value(v: Vec2) -> Value {
    Value::tuple(vec![Value::Number(v.x), Value::Number(v.y)])
}

/// Largest integer that every `f64` below it represents exactly.
const MAX_EXACT_INTEGER: f64 = 9_007_199_254_740_992.0;

/// Returns the number as `i64` when it is integral and exactly
/// representable.
pub(crate) fn as_integer(value: f64) -> Option<i64> {
    if value.fract() == 0.0 && value.abs() <= MAX_EXACT_INTEGER {
        // Exact: integral and within +/- 2^53.
        #[allow(clippy::cast_possible_truncation)]
        let integer = value as i64;
        Some(integer)
    } else {
        None
    }
}

/// Converts a range-bounded integer to `f64`.
pub(crate) fn f64_from_i64(value: i64) -> f64 {
    // MDF integers stay within +/- 2^53 (see `as_integer`).
    #[allow(clippy::cast_precision_loss)]
    let float = value as f64;
    float
}

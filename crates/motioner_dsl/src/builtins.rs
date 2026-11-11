//! Pure built-in functions (spec section 6, v0 subset).
//!
//! Scene constructors are builtins too, but they mutate the document and
//! live in `scene.rs`.

use std::cmp::Ordering;
use std::sync::Arc;

use motioner_core::{Gradient, GradientKind, GradientStop, TextAlign, TextMetrics, TextRequest};

use crate::host::{as_integer, as_vec2, bind, f64_from_i64, CallArgs, HostError, HostResult};
use crate::limits::{Budget, Limits};
use crate::value::{Builtin, RangeValue, Value};

/// Calls a pure builtin. Scene builtins must not be passed here.
///
/// Bulk builtins charge one step per item processed and `str()` charges
/// the produced string bytes, so their work is covered by the budget.
///
/// # Errors
///
/// Returns a type, value or limit error from the builtin.
pub(crate) fn call(
    builtin: Builtin,
    args: CallArgs,
    limits: &Limits,
    budget: &mut Budget,
    metrics: &dyn TextMetrics,
) -> HostResult<Value> {
    match builtin {
        Builtin::TextWidth => text_measure("text_width", args, metrics, false),
        Builtin::TextBounds => text_measure("text_bounds", args, metrics, true),
        Builtin::Range => range(args),
        Builtin::Enumerate => enumerate(args, limits, budget),
        Builtin::Len => {
            let bound = bind("len", &["value"], 1, args)?;
            let length = length(bound.get(0).unwrap_or(&Value::None), budget)?;
            Ok(Value::Number(f64_from_i64(length)))
        }
        Builtin::Min => extreme("min", args, limits, budget, Ordering::Less),
        Builtin::Max => extreme("max", args, limits, budget, Ordering::Greater),
        Builtin::Abs => unary_math("abs", args, f64::abs),
        Builtin::Round => round(args),
        Builtin::Str => {
            let bound = bind("str", &["value"], 1, args)?;
            let text = match bound.get(0) {
                Some(value) => value.display(limits, budget)?,
                None => String::new(),
            };
            limits.check_string_len(text.len())?;
            budget.charge_string(text.len())?;
            Ok(Value::Str(Arc::from(text)))
        }
        Builtin::Int => convert_number("int", args, budget, f64::trunc),
        Builtin::Float => convert_number("float", args, budget, |v| v),
        Builtin::Sin => unary_math("sin", args, f64::sin),
        Builtin::Cos => unary_math("cos", args, f64::cos),
        Builtin::Lerp => {
            let bound = bind("lerp", &["a", "b", "t"], 3, args)?;
            let (a, b, t) = (bound.number(0)?, bound.number(1)?, bound.number(2)?);
            Ok(Value::Number(a + (b - a) * t))
        }
        Builtin::Clamp => {
            let bound = bind("clamp", &["value", "low", "high"], 3, args)?;
            let (value, low, high) = (bound.number(0)?, bound.number(1)?, bound.number(2)?);
            if low.is_nan() || high.is_nan() {
                return Err(HostError::value_error("clamp(): bounds must not be NaN"));
            }
            if low > high {
                return Err(HostError::value_error(
                    "clamp(): 'low' must not exceed 'high'",
                ));
            }
            Ok(Value::Number(value.clamp(low, high)))
        }
        Builtin::Bezier => {
            let bound = bind("bezier", &["x1", "y1", "x2", "y2"], 4, args)?;
            let (x1, y1, x2, y2) = (
                bound.number(0)?,
                bound.number(1)?,
                bound.number(2)?,
                bound.number(3)?,
            );
            let ease = motioner_core::Ease::bezier(x1, y1, x2, y2)
                .map_err(|error| HostError::value_error(error.to_string()))?;
            Ok(Value::Ease(ease))
        }
        Builtin::LinearGradient | Builtin::RadialGradient => gradient(builtin, args, budget),
        Builtin::Comp
        | Builtin::Group
        | Builtin::Rect
        | Builtin::Circle
        | Builtin::Text
        | Builtin::Path
        | Builtin::Polygon
        | Builtin::Star
        | Builtin::Arc
        | Builtin::Image
        | Builtin::Audio
        | Builtin::Video
        | Builtin::Stagger
        | Builtin::Theme
        | Builtin::UseTheme
        | Builtin::Token
        | Builtin::Beat
        | Builtin::Bar
        | Builtin::Scene
        | Builtin::Row
        | Builtin::Column
        | Builtin::Grid
        | Builtin::Stack
        | Builtin::CompSize => Err(HostError::type_error(format!(
            "{}() is a scene constructor",
            builtin.name()
        ))),
    }
}

/// `text_width(...)` / `text_bounds(...)` (D-0034).
///
/// Both take `(content, size, font="Inter", weight=400, tracking=0,
/// line_height=1, align="left")`.  `text_width` returns the widest line's
/// advance; `text_bounds` returns `(left, top, right, bottom)` in Motioner
/// local pixels (y down).
fn text_measure(
    name: &str,
    args: CallArgs,
    metrics: &dyn TextMetrics,
    bounds: bool,
) -> HostResult<Value> {
    let params = [
        "content",
        "size",
        "font",
        "weight",
        "tracking",
        "line_height",
        "align",
    ];
    let bound = bind(name, &params, 2, args)?;
    let content = match bound.get(0) {
        Some(Value::Str(text)) => text.clone(),
        Some(other) => {
            return Err(HostError::type_error(format!(
                "{name}(): 'content' must be a string, got {}",
                other.type_name()
            )))
        }
        None => {
            return Err(HostError::type_error(format!(
                "{name}() is missing 'content'"
            )))
        }
    };
    let size = bound.number(1)?;
    let font = bound.string_or(2, "Roboto")?;
    let weight = bound.integer_in(3, 400, 1, 1000)?;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let weight = weight.clamp(1, 1000) as u16;
    let tracking = bound.number_or(4, 0.0)?;
    let line_height = bound.number_or(5, 1.0)?;
    let align_name = bound.string_or(6, "left")?;
    let align = TextAlign::from_name(&align_name).ok_or_else(|| {
        HostError::value_error(format!(
            "{name}(): unknown align {align_name:?}; use \"left\", \"center\" or \"right\""
        ))
    })?;
    let request = TextRequest {
        text: &content,
        size,
        font: &font,
        weight,
        tracking,
        line_height,
        align,
    };
    let measurement = metrics.measure(&request).ok_or_else(|| {
        HostError::value_error(format!(
            "{name}(): no metrics for font {font:?} at weight {weight}"
        ))
    })?;
    if bounds {
        Ok(Value::tuple(vec![
            Value::Number(measurement.ink.left),
            Value::Number(measurement.ink.top),
            Value::Number(measurement.ink.right),
            Value::Number(measurement.ink.bottom),
        ]))
    } else {
        Ok(Value::Number(measurement.width))
    }
}

/// `linear_gradient(start, end, stops)` / `radial_gradient(center, radius,
/// stops)` (D-0030).
fn gradient(builtin: Builtin, args: CallArgs, budget: &mut Budget) -> HostResult<Value> {
    let linear = builtin == Builtin::LinearGradient;
    let (name, params) = if linear {
        ("linear_gradient", ["start", "end", "stops"])
    } else {
        ("radial_gradient", ["center", "radius", "stops"])
    };
    let bound = bind(name, &params, 3, args)?;
    let kind = if linear {
        let start = vec_or_default(bound.get(0), name, "start")?;
        let end = vec_or_default(bound.get(1), name, "end")?;
        GradientKind::Linear { start, end }
    } else {
        let center = vec_or_default(bound.get(0), name, "center")?;
        let radius = bound.number(1)?;
        GradientKind::Radial { center, radius }
    };
    let stops = gradient_stops(bound.get(2), name, budget)?;
    let gradient = Gradient::new(kind, stops)
        .map_err(|error| HostError::value_error(format!("{name}(): {error}")))?;
    Ok(Value::Gradient(Arc::new(gradient)))
}

fn vec_or_default(
    value: Option<&Value>,
    function: &str,
    param: &str,
) -> HostResult<motioner_core::Vec2> {
    let Some(value) = value else {
        return Err(HostError::type_error(format!(
            "{function}() is missing required argument '{param}'"
        )));
    };
    as_vec2(value).ok_or_else(|| {
        HostError::type_error(format!(
            "{function}(): '{param}' expects an (x, y) pair, got {}",
            value.type_name()
        ))
    })
}

/// Parses `[(offset, "#rrggbb"), ...]` stops.
fn gradient_stops(
    value: Option<&Value>,
    function: &str,
    budget: &mut Budget,
) -> HostResult<Vec<GradientStop>> {
    let Some(value) = value else {
        return Err(HostError::type_error(format!(
            "{function}() is missing required argument 'stops'"
        )));
    };
    let items = match value {
        Value::List(items) | Value::Tuple(items) => items.borrow().clone(),
        other => {
            return Err(HostError::type_error(format!(
                "{function}(): 'stops' must be a list of (offset, colour) pairs, got {}",
                other.type_name()
            )))
        }
    };
    budget.charge_items(items.len())?;
    items
        .iter()
        .map(|item| {
            let pair = match item {
                Value::List(pair) | Value::Tuple(pair) => pair.borrow().clone(),
                other => {
                    return Err(HostError::type_error(format!(
                        "{function}(): a stop must be an (offset, colour) pair, got {}",
                        other.type_name()
                    )))
                }
            };
            let [offset, color] = pair.as_slice() else {
                return Err(HostError::value_error(format!(
                    "{function}(): a stop must be an (offset, colour) pair"
                )));
            };
            let Value::Number(offset) = offset else {
                return Err(HostError::type_error(format!(
                    "{function}(): a stop offset must be a number"
                )));
            };
            let Value::Str(color) = color else {
                return Err(HostError::type_error(format!(
                    "{function}(): a stop colour must be a string"
                )));
            };
            let color = motioner_core::Color::parse_hex(color)
                .map_err(|error| HostError::value_error(format!("{function}(): {error}")))?;
            Ok(GradientStop::new(*offset, color))
        })
        .collect()
}

/// Number of items in a string (characters), list, tuple or range.
///
/// Counting the characters of a string scans its bytes, so that scan
/// charges one step per byte before it runs.
///
/// # Errors
///
/// Returns a type error for values without a length or a limit error when
/// the step budget cannot cover the scan.
pub(crate) fn length(value: &Value, budget: &mut Budget) -> HostResult<i64> {
    let count = match value {
        Value::Str(s) => {
            budget.charge_items(s.len())?;
            s.chars().count()
        }
        Value::List(items) => items.borrow().len(),
        Value::Tuple(items) => items.len(),
        Value::Range(range) => return Ok(range.len()),
        other => {
            return Err(HostError::type_error(format!(
                "len() of a {} is not defined",
                other.type_name()
            )))
        }
    };
    i64::try_from(count).map_err(|_| HostError::limit("collection is too large"))
}

/// Materializes an iterable into its items, in iteration order.
///
/// The number of items is charged before the vector is allocated.
///
/// # Errors
///
/// Returns a type error for non-iterables or a limit error when a cap or
/// the step budget is exceeded.
pub(crate) fn items(value: &Value, limits: &Limits, budget: &mut Budget) -> HostResult<Vec<Value>> {
    match value {
        Value::List(items) | Value::Tuple(items) => {
            let items = items.borrow();
            budget.charge_items(items.len())?;
            Ok(items.clone())
        }
        Value::Str(s) => {
            let count = s.chars().count();
            limits.check_list_len(count)?;
            budget.charge_items(count)?;
            Ok(s.chars()
                .map(|c| Value::Str(Arc::from(c.to_string())))
                .collect())
        }
        Value::Range(range) => {
            let len = range.len();
            let count = usize::try_from(len).unwrap_or(usize::MAX);
            limits.check_list_len(count)?;
            budget.charge_items(count)?;
            Ok((0..len)
                .map(|i| Value::Number(f64_from_i64(range.at(i))))
                .collect())
        }
        other => Err(HostError::type_error(format!(
            "a {} is not iterable",
            other.type_name()
        ))),
    }
}

/// `enumerate(iterable, start=0)`: pairs of `(index, item)`.
///
/// The iterable is materialized once; the number of produced pairs charges one
/// step each and is checked against the list cap.
fn enumerate(args: CallArgs, limits: &Limits, budget: &mut Budget) -> HostResult<Value> {
    let bound = bind("enumerate", &["iterable", "start"], 1, args)?;
    let start = match bound.get(1) {
        None | Some(Value::None) => 0,
        Some(_) => {
            let start = bound.number(1)?;
            as_integer(start).ok_or_else(|| {
                HostError::value_error("enumerate(): 'start' must be a whole number")
            })?
        }
    };
    let items = items(bound.get(0).unwrap_or(&Value::None), limits, budget)?;
    limits.check_list_len(items.len())?;
    budget.charge_items(items.len())?;
    let pairs = items
        .into_iter()
        .enumerate()
        .map(|(offset, item)| {
            let index = start.saturating_add(i64::try_from(offset).unwrap_or(i64::MAX));
            Value::tuple(vec![Value::Number(f64_from_i64(index)), item])
        })
        .collect();
    Ok(Value::list(pairs))
}

fn range(args: CallArgs) -> HostResult<Value> {
    if !args.keywords.is_empty() {
        return Err(HostError::type_error("range() takes no keyword arguments"));
    }
    let given = args.positional.len();
    let bound = bind("range", &["start", "stop", "step"], 1, args)?;
    let integer = |index: usize, default: i64| -> HostResult<i64> {
        let value = bound.number_or(index, f64_from_i64(default))?;
        as_integer(value)
            .ok_or_else(|| HostError::value_error("range() arguments must be whole numbers"))
    };
    let (start, stop) = if given == 1 {
        (0, integer(0, 0)?)
    } else {
        (integer(0, 0)?, integer(1, 0)?)
    };
    let stride = integer(2, 1)?;
    if stride == 0 {
        return Err(HostError::value_error("range() step must not be zero"));
    }
    Ok(Value::Range(RangeValue {
        start,
        stop,
        step: stride,
    }))
}

fn unary_math(name: &str, args: CallArgs, op: impl Fn(f64) -> f64) -> HostResult<Value> {
    let bound = bind(name, &["x"], 1, args)?;
    Ok(Value::Number(op(bound.number(0)?)))
}

fn round(args: CallArgs) -> HostResult<Value> {
    let bound = bind("round", &["x", "ndigits"], 1, args)?;
    let x = bound.number(0)?;
    let digits = match bound.get(1) {
        None | Some(Value::None) => 0,
        Some(_) => bound.integer_in(1, 0, -308, 308)?,
    };
    if digits == 0 {
        return Ok(Value::Number(x.round_ties_even()));
    }
    // `digits` is within +/-308, so the exponent conversion is exact.
    #[allow(clippy::cast_possible_truncation)]
    let scale = 10f64.powi(digits as i32);
    let scaled = x * scale;
    if !scaled.is_finite() {
        // Python returns the input unchanged when scaling overflows.
        return Ok(Value::Number(x));
    }
    Ok(Value::Number(scaled.round_ties_even() / scale))
}

/// `int()` and `float()`: numbers, booleans and numeric strings.
/// `int()` and `float()` conversion; parsing a string charges its bytes.
fn convert_number(
    name: &str,
    args: CallArgs,
    budget: &mut Budget,
    finish: impl Fn(f64) -> f64,
) -> HostResult<Value> {
    let bound = bind(name, &["value"], 1, args)?;
    let number = match bound.get(0) {
        Some(Value::Number(n)) => *n,
        Some(Value::Bool(b)) => f64::from(u8::from(*b)),
        Some(Value::Str(s)) => {
            budget.charge_string(s.len())?;
            s.trim()
                .parse::<f64>()
                .ok()
                .filter(|v| v.is_finite())
                .ok_or_else(|| HostError::value_error(format!("{name}(): {s:?} is not a number")))?
        }
        Some(other) => {
            return Err(HostError::type_error(format!(
                "{name}() cannot convert a {}",
                other.type_name()
            )))
        }
        None => 0.0,
    };
    Ok(Value::Number(finish(number)))
}

/// `min()` / `max()` over arguments or a single iterable.
///
/// Each candidate processed charges one step.
fn extreme(
    name: &str,
    args: CallArgs,
    limits: &Limits,
    budget: &mut Budget,
    wanted: Ordering,
) -> HostResult<Value> {
    if !args.keywords.is_empty() {
        return Err(HostError::type_error(format!(
            "{name}() takes no keyword arguments"
        )));
    }
    let candidates = if args.positional.len() == 1 {
        items(&args.positional[0].value, limits, budget)?
    } else {
        args.positional
            .into_iter()
            .map(|argument| argument.value)
            .collect()
    };
    let mut best: Option<Value> = None;
    for candidate in candidates {
        budget.charge(1)?;
        best = Some(match best {
            None => candidate,
            Some(current) => {
                if compare(&candidate, &current, budget)? == wanted {
                    candidate
                } else {
                    current
                }
            }
        });
    }
    best.ok_or_else(|| HostError::value_error(format!("{name}() of an empty sequence")))
}

/// Orders two numbers or two strings; comparing strings charges the
/// bytes it may scan.
pub(crate) fn compare(a: &Value, b: &Value, budget: &mut Budget) -> HostResult<Ordering> {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x
            .partial_cmp(y)
            .ok_or_else(|| HostError::value_error("cannot order NaN")),
        (Value::Str(x), Value::Str(y)) => {
            budget.charge_string(x.len().min(y.len()))?;
            Ok(x.cmp(y))
        }
        _ => Err(HostError::type_error(format!(
            "cannot compare {} with {}",
            a.type_name(),
            b.type_name()
        ))),
    }
}

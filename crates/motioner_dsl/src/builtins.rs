//! Pure built-in functions (spec section 6, v0 subset).
//!
//! Scene constructors are builtins too, but they mutate the document and
//! live in `scene.rs`.

use std::cmp::Ordering;
use std::rc::Rc;

use crate::host::{as_integer, bind, f64_from_i64, CallArgs, HostError, HostResult};
use crate::limits::Limits;
use crate::value::{Builtin, RangeValue, Value};

/// Calls a pure builtin. Scene builtins must not be passed here.
pub(crate) fn call(builtin: Builtin, args: CallArgs, limits: &Limits) -> HostResult<Value> {
    match builtin {
        Builtin::Range => range(args),
        Builtin::Len => {
            let bound = bind("len", &["value"], 1, args)?;
            let length = length(bound.get(0).unwrap_or(&Value::None))?;
            Ok(Value::Number(f64_from_i64(length)))
        }
        Builtin::Min => extreme("min", args, limits, Ordering::Less),
        Builtin::Max => extreme("max", args, limits, Ordering::Greater),
        Builtin::Abs => unary_math("abs", args, f64::abs),
        Builtin::Round => round(args),
        Builtin::Str => {
            let bound = bind("str", &["value"], 1, args)?;
            let text = bound.get(0).map_or_else(String::new, Value::display);
            check_string_len(text.len(), limits)?;
            Ok(Value::Str(Rc::from(text)))
        }
        Builtin::Int => convert_number("int", args, f64::trunc),
        Builtin::Float => convert_number("float", args, |v| v),
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
            if low > high {
                return Err(HostError::value_error(
                    "clamp(): 'low' must not exceed 'high'",
                ));
            }
            Ok(Value::Number(value.clamp(low, high)))
        }
        Builtin::Comp | Builtin::Group | Builtin::Rect | Builtin::Circle | Builtin::Text => Err(
            HostError::type_error(format!("{}() is a scene constructor", builtin.name())),
        ),
    }
}

/// Fails when a string would exceed the configured length limit.
pub(crate) fn check_string_len(bytes: usize, limits: &Limits) -> HostResult<()> {
    if bytes > limits.max_string_len {
        return Err(HostError::limit(format!(
            "string length limit of {} bytes exceeded",
            limits.max_string_len
        )));
    }
    Ok(())
}

/// Fails when a list would exceed the configured length limit.
pub(crate) fn check_list_len(items: usize, limits: &Limits) -> HostResult<()> {
    if items > limits.max_list_len {
        return Err(HostError::limit(format!(
            "list length limit of {} items exceeded",
            limits.max_list_len
        )));
    }
    Ok(())
}

/// Number of items in a string (characters), list, tuple or range.
pub(crate) fn length(value: &Value) -> HostResult<i64> {
    let count = match value {
        Value::Str(s) => s.chars().count(),
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
pub(crate) fn items(value: &Value, limits: &Limits) -> HostResult<Vec<Value>> {
    match value {
        Value::List(items) => Ok(items.borrow().clone()),
        Value::Tuple(items) => Ok(items.to_vec()),
        Value::Str(s) => Ok(s
            .chars()
            .map(|c| Value::Str(Rc::from(c.to_string())))
            .collect()),
        Value::Range(range) => {
            let len = range.len();
            check_list_len(usize::try_from(len).unwrap_or(usize::MAX), limits)?;
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
    Ok(Value::Number((x * scale).round_ties_even() / scale))
}

/// `int()` and `float()`: numbers, booleans and numeric strings.
fn convert_number(name: &str, args: CallArgs, finish: impl Fn(f64) -> f64) -> HostResult<Value> {
    let bound = bind(name, &["value"], 1, args)?;
    let number = match bound.get(0) {
        Some(Value::Number(n)) => *n,
        Some(Value::Bool(b)) => f64::from(u8::from(*b)),
        Some(Value::Str(s)) => s
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .ok_or_else(|| HostError::value_error(format!("{name}(): {s:?} is not a number")))?,
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
fn extreme(name: &str, args: CallArgs, limits: &Limits, wanted: Ordering) -> HostResult<Value> {
    if !args.keywords.is_empty() {
        return Err(HostError::type_error(format!(
            "{name}() takes no keyword arguments"
        )));
    }
    let candidates = match args.positional.as_slice() {
        [single] => items(single, limits)?,
        _ => args.positional,
    };
    let mut best: Option<Value> = None;
    for candidate in candidates {
        best = Some(match best {
            None => candidate,
            Some(current) => {
                if compare(&candidate, &current)? == wanted {
                    candidate
                } else {
                    current
                }
            }
        });
    }
    best.ok_or_else(|| HostError::value_error(format!("{name}() of an empty sequence")))
}

/// Orders two numbers or two strings.
pub(crate) fn compare(a: &Value, b: &Value) -> HostResult<Ordering> {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x
            .partial_cmp(y)
            .ok_or_else(|| HostError::value_error("cannot order NaN")),
        (Value::Str(x), Value::Str(y)) => Ok(x.cmp(y)),
        _ => Err(HostError::type_error(format!(
            "cannot compare {} with {}",
            a.type_name(),
            b.type_name()
        ))),
    }
}

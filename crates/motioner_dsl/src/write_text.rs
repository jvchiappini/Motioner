//! Shared MDF text for literals, values and key calls.
//!
//! The planner writes small snippets and bake writes the generated block;
//! both must use the same number, pair, quote and optional-`None` rules so
//! the produced program is byte-stable.

use motioner_core::{format_number, quote, value_text, OverrideValue, Vec2};

use crate::origins::{Literal, ValueOrigin};

/// The literal text for a static override.
pub(crate) fn set_replacement(
    origin: &ValueOrigin,
    property: &str,
    value: &OverrideValue,
) -> Option<String> {
    match (origin.literal?, value) {
        (Literal::Number, OverrideValue::Number(number)) => Some(format_number(*number)),
        (Literal::Text, OverrideValue::Text(text)) => Some(optional_none(property, text)),
        (Literal::None, OverrideValue::Text(text)) if is_optional(property) => {
            Some(optional_none(property, text))
        }
        (Literal::Pair, OverrideValue::Vec2(pair)) => Some(pair_text(*pair)),
        _ => None,
    }
}

/// The literal text for a key value.
pub(crate) fn key_replacement(origin: &ValueOrigin, value: &OverrideValue) -> Option<String> {
    match (origin.literal?, value) {
        (Literal::Number, OverrideValue::Number(number)) => Some(format_number(*number)),
        (Literal::Pair, OverrideValue::Vec2(pair)) => Some(pair_text(*pair)),
        (Literal::Text, OverrideValue::Text(text)) => Some(optional_none(&origin.property, text)),
        (Literal::None, OverrideValue::Text(text)) if is_optional(&origin.property) => {
            Some(optional_none(&origin.property, text))
        }
        _ => None,
    }
}

/// The assignment text for a value: same shapes as an override.
pub(crate) fn set_value_text(property: &str, value: &OverrideValue) -> String {
    if let OverrideValue::Text(text) = value {
        if is_optional(property) && text.eq_ignore_ascii_case("none") {
            return "None".to_owned();
        }
    }
    value_text(value)
}

/// `None` for an optional colour field cleared with `"none"`, else the quoted
/// string.
pub(crate) fn optional_none(property: &str, text: &str) -> String {
    if is_optional(property) && text.eq_ignore_ascii_case("none") {
        "None".to_owned()
    } else {
        quote(text)
    }
}

/// Whether a content field accepts `None`.
pub(crate) fn is_optional(property: &str) -> bool {
    matches!(property, "fill" | "stroke")
}

/// `(x, y)` with the canonical number format.
pub(crate) fn pair_text(pair: Vec2) -> String {
    format!("({}, {})", format_number(pair.x), format_number(pair.y))
}

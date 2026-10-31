//! Magnitude policy for authored property values.
//!
//! These are the only bounds the document enforces on transform values.
//! They are deliberately generous for real motion graphics while keeping
//! a single node's local matrix far from `f64` overflow; the evaluator
//! additionally guards against overflow caused by deep nesting.
//!
//! Every numeric write to a [`Property`] passes through `check_value`,
//! whether it sets a static value or adds a keyframe, so the policy is
//! applied in exactly one place.

use crate::error::CoreError;
use crate::node::Property;

/// Largest absolute component of `Position` and `Anchor`, in pixels.
pub const MAX_POSITION: f64 = 1.0e7;

/// Largest absolute component of `Scale`.
pub const MAX_SCALE: f64 = 1.0e4;

/// Largest absolute `Rotation`, in degrees.
pub const MAX_ROTATION: f64 = 1.0e7;

/// Smallest accepted `Opacity`.
pub const MIN_OPACITY: f64 = 0.0;

/// Largest accepted `Opacity`.
pub const MAX_OPACITY: f64 = 1.0;

/// Inclusive range of scalar values accepted for a property.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PropertyBounds {
    /// Smallest accepted value.
    pub min: f64,
    /// Largest accepted value.
    pub max: f64,
}

/// Returns the inclusive value range for `property`.
///
/// `Position` and `Anchor` accept `[-MAX_POSITION, MAX_POSITION]` per
/// component, `Scale` accepts `[-MAX_SCALE, MAX_SCALE]` per component,
/// `Rotation` accepts `[-MAX_ROTATION, MAX_ROTATION]` degrees, and
/// `Opacity` accepts `[MIN_OPACITY, MAX_OPACITY]`.
#[must_use]
pub const fn property_bounds(property: Property) -> PropertyBounds {
    match property {
        Property::Position | Property::Anchor => PropertyBounds {
            min: -MAX_POSITION,
            max: MAX_POSITION,
        },
        Property::Scale => PropertyBounds {
            min: -MAX_SCALE,
            max: MAX_SCALE,
        },
        Property::Rotation => PropertyBounds {
            min: -MAX_ROTATION,
            max: MAX_ROTATION,
        },
        Property::Opacity => PropertyBounds {
            min: MIN_OPACITY,
            max: MAX_OPACITY,
        },
    }
}

/// Validates one scalar written to `property`.
///
/// Returns the value unchanged when it is finite and inside the inclusive
/// [`property_bounds`] range; exact bounds are accepted.
///
/// # Errors
///
/// Returns [`CoreError::NonFiniteValue`] for NaN or an infinity, and
/// [`CoreError::ValueOutOfRange`] for a finite value outside the range.
pub(crate) fn check_value(property: Property, value: f64) -> Result<f64, CoreError> {
    if !value.is_finite() {
        return Err(CoreError::NonFiniteValue { property });
    }
    let bounds = property_bounds(property);
    if value < bounds.min || value > bounds.max {
        return Err(CoreError::ValueOutOfRange { property, value });
    }
    Ok(value)
}

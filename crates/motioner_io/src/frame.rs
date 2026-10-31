//! Frame-buffer size validation shared by PNG and video export.

use crate::error::ExportError;

/// Returns the byte length of one tightly packed RGBA8 frame.
///
/// # Errors
///
/// Returns [`ExportError::DimensionsTooLarge`] when the byte count does not
/// fit in a `usize`.
pub(crate) fn expected_len(width: u32, height: u32) -> Result<usize, ExportError> {
    u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|pixels| pixels.checked_mul(4))
        .and_then(|bytes| usize::try_from(bytes).ok())
        .ok_or(ExportError::DimensionsTooLarge { width, height })
}

/// Checks that `rgba` holds exactly one tightly packed RGBA8 frame.
///
/// # Errors
///
/// Returns [`ExportError::SizeMismatch`] when the length differs from
/// `width * height * 4`, or [`ExportError::DimensionsTooLarge`] when the
/// expected length overflows a `usize`.
pub(crate) fn validate_rgba(width: u32, height: u32, rgba: &[u8]) -> Result<(), ExportError> {
    let expected = expected_len(width, height)?;
    if rgba.len() == expected {
        Ok(())
    } else {
        Err(ExportError::SizeMismatch {
            expected,
            actual: rgba.len(),
        })
    }
}

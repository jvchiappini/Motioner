//! Minimal utility helpers for the DSL subsystem.
//!
//! Currently only a normalization/validation wrapper is required by the
//! rest of the application; all other functions have been removed pending a
//! future re‑implementation of the parser.

/// Validate DSL text and convert leading spaces to tabs.
///
/// This is a thin wrapper around the (stubbed) validator and the generator's
/// `normalize_tabs` function.  The returned diagnostics are later surfaced in
/// the UI or used to block saves.
pub fn validate_and_normalize(src: &mut String) -> Vec<super::validator::Diagnostic> {
    let diags = super::validate(src);
    let normalized = super::generator::normalize_tabs(src);
    if normalized != *src {
        *src = normalized;
    }
    diags
}

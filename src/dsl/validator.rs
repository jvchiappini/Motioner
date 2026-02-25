use serde::{Deserialize, Serialize};

/// Very small diagnostic type used by the editor when validating the DSL.
///
/// For now the validator always returns an empty vector, so the struct exists
/// purely to satisfy the public API and may grow again when real checking is
/// reinstated.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub message: String,
    pub line: usize,
    pub column: usize,
}

/// Perform a quick lint pass on DSL source.  Currently this is a no‑op.
///
/// The function is kept so callers such as the autosave logic and UI can
/// continue working even though the actual grammar is unimplemented.  When
/// the DSL support is restored this should evolve into a proper validator.
pub fn validate(_src: &str) -> Vec<Diagnostic> {
    Vec::new()
}

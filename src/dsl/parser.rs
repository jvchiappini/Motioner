//! Minimal stub implementation of the DSL parser.
//!
//! The original parser and its helpers have been removed because the grammar is
//! currently broken.  The public API still exists so that other parts of the
//! application compile, but all functions return empty/default values.  Once a
//! proper parser is restored this file can be rewritten accordingly.

use super::ast::{HeaderConfig, Statement};

/// Parse the supplied DSL source and return an empty statement list.
///
/// This stub ignores its input completely; it exists only to satisfy callers
/// such as `dsl::parse_dsl` and the runtime.  No shapes will ever be returned.
pub fn parse(_src: &str) -> Vec<Statement> {
    Vec::new()
}

/// Parse the project header configuration.  Always fails when the parser is
/// disabled so callers can treat the file as invalid.
pub fn parse_config(_src: &str) -> Result<HeaderConfig, String> {
    Err("DSL parser disabled".to_string())
}

// small helpers kept with allow(dead_code) so that they don't trigger lint
// errors; they may be resurrected when the parser is fixed.

#[allow(dead_code)]
pub fn method_color(_name: &str) -> Option<[u8; 4]> {
    None
}

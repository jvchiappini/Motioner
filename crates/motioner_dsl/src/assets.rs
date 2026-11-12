//! Asset and module resolution for `use()` (D-0051).
//!
//! `motioner_dsl` has no file-system access. The host injects a resolver
//! through [`crate::run_with_assets`]; `motioner_io` implements the real one
//! and tests use an in-memory one. `use()` of raster, audio, video and font
//! files only needs the path (the reader resolves it later), so those never
//! call the resolver.

use std::sync::Arc;

use motioner_core::Symbol;

/// What a resolver returns for a `use()` path it had to read.
#[derive(Debug, Clone)]
pub enum LoadedAsset {
    /// A `.motion` module: its source and the directory that contains it.
    Module {
        /// Canonical identity used for cycle detection and caching.
        key: String,
        /// Program text.
        source: String,
        /// Directory of the module, for relative `use()` inside it.
        dir: String,
    },
    /// A parsed SVG symbol.
    Symbol {
        /// Canonical identity used for diagnostics.
        key: String,
        /// The parsed symbol.
        symbol: Arc<Symbol>,
    },
}

/// Reads `use()` paths for the interpreter.
///
/// The resolver MUST reject absolute paths and parent escapes before any
/// I/O and MUST be deterministic for a given (path, base) pair.
pub trait AssetResolver: Sync {
    /// Loads `path` relative to `base` (empty for the main program).
    ///
    /// # Errors
    ///
    /// Returns a human-readable reason for a missing file, a sandbox escape
    /// or an unreadable document.
    fn load(&self, path: &str, base: &str) -> Result<LoadedAsset, String>;
}

/// The resolver used by [`crate::run`]: `use()` of a module or symbol fails
/// with a clear message, and media definitions still work.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoAssets;

impl AssetResolver for NoAssets {
    fn load(&self, path: &str, _base: &str) -> Result<LoadedAsset, String> {
        Err(format!(
            "use({path:?}) needs an asset resolver; call run_with_assets(...) from the application"
        ))
    }
}

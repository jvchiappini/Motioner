//! Transient state derived from the DSL source text.
//!
//! This type used to live inside `dsl::mod`, but since it represents in‑memory
//! bookkeeping rather than parsing logic, it now belongs in the `states`
//! hierarchy alongside related helpers such as `AutosaveState`.
//!
//! The struct is intentionally lightweight and `Clone` so it can be snapshotted
//! for the preview worker and other subsystems without locking the whole
//! `AppState`.

use crate::dsl::runtime::DslHandler;

/// Container for diagnostics and extracted handlers belonging to the current
/// DSL buffer.
#[derive(Debug, Default, Clone)]
pub struct DslState {
    /// Diagnostics produced by the most recent validation pass.
    pub diagnostics: Vec<crate::dsl::Diagnostic>,

    /// Event handlers (`on_time`, `on_click`, etc.) pulled from the DSL text.
    pub event_handlers: Vec<DslHandler>,
}

impl DslState {
    // Convenience helper removed; callers can clear fields directly if needed.
    // pub fn clear(&mut self) {
    //     self.diagnostics.clear();
    //     self.event_handlers.clear();
    // }
}

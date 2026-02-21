// Encapsulates the small collection of flags and timestamps used for
// the "autosave while typing" feature.  Moving this into a dedicated type
// keeps the many callsites in UI code from having to know about each field
// individually and provides a convenient place for helpers such as
// `on_change` and `tick`.

#[derive(Debug, Default)]
pub struct AutosaveState {
    /// The last wall-clock time (seconds) at which the user made a change that
    /// *might* need to be flushed to disk.  `None` means no edits have been
    /// recorded yet.
    pub last_edit_time: Option<f64>,
    /// True if an autosave is currently pending (a write may be in-flight or
    /// has been scheduled once the cooldown expires).
    pub pending: bool,
    /// Timestamp when the last successful autosave completed (for UI display).
    pub last_success_time: Option<f64>,
    /// Error message from the last failed autosave, if any.
    pub error: Option<String>,
    /// Debounce period (seconds) before attempting validation/write.
    pub cooldown_secs: f32,
}

/// Update state diagnostics and autosave flags based on the provided DSL
/// diagnostics.  This helper operates on the entire `AppState` to avoid
/// borrowing conflicts between the `autosave` field and other state members.
pub fn apply_diagnostics(
    state: &mut crate::app_state::AppState,
    diags: Vec<crate::dsl::Diagnostic>,
) {
    // borrow autosave separately to satisfy the borrow checker
    let autosave = &mut state.autosave;
    if diags.is_empty() {
        state.dsl.diagnostics.clear();
        autosave.error = None;
        // leave pending flag untouched; callers may adjust it separately
    } else {
            state.dsl.diagnostics = diags.clone();
        autosave.pending = false;
        autosave.error = Some(diags[0].message.clone());
    }
}

/// Drive the autosave debounce/validation/write sequence that used to be
/// implemented directly on `AppState`.  This helper mutates both the
/// autosave fields and other portions of the app state (`dsl_code`,
/// `dsl_diagnostics`, etc.) and is intended to be called once per frame with
/// the current wall‑clock time.
pub fn tick(state: &mut crate::app_state::AppState, now: f64) {
    if let Some(last_edit) = state.autosave.last_edit_time {
        if now - last_edit > state.autosave.cooldown_secs as f64 {
            let diagnostics = crate::dsl::utils::validate_and_normalize(&mut state.dsl_code);
            if !diagnostics.is_empty() {
                apply_diagnostics(state, diagnostics);
            } else {
                // attempt write
                match crate::events::element_properties_changed_event::write_dsl_to_project(
                    state,
                    false,
                ) {
                    Ok(_) => {
                        state.autosave.pending = false;
                        state.autosave.last_success_time = Some(now);
                        state.autosave.error = None;
                        state.dsl.diagnostics.clear();
                    }
                    Err(e) => {
                        state.autosave.pending = false;
                        state.autosave.error = Some(e.to_string());
                    }
                }
            }
        }
    }
}

impl AutosaveState {
    /// Helper invoked when the user makes an edit that should trigger
    /// autosave logic.  `diagnostics` may optionally contain the results of a
    /// quick validation pass; if any diagnostics are present the state will be
    /// marked "errored" and the pending flag will be cleared.
    pub fn on_change(&mut self, now: f64, diagnostics: Option<&[crate::dsl::Diagnostic]>) {
        self.last_edit_time = Some(now);
        if let Some(diags) = diagnostics {
            if diags.is_empty() {
                self.pending = true;
                self.error = None;
            } else {
                self.pending = false;
                self.error = Some(diags[0].message.clone());
            }
        } else {
            // no diagnostics provided, just mark dirty
            self.pending = true;
        }
    }

    /// Shortcut for marking the state dirty without running validations.
    /// Useful for UI actions that change state but do not produce DSL errors.
    pub fn mark_dirty(&mut self, now: f64) {
        self.last_edit_time = Some(now);
        self.pending = true;
    }

    // NOTE: autosave ticking is executed by `AppState::autosave_tick` so that
    // the method can borrow the surrounding state in one shot without
    // violating Rust's borrowing rules.  The logic has therefore been moved
    // back into `AppState` itself; this type simply provides the storage and
    // convenient mutation helpers above.
}

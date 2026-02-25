#[derive(Debug, Default)]
pub struct AutosaveState {
    pub last_edit_time: Option<f64>,
    pub pending: bool,
    pub last_success_time: Option<f64>,
    pub error: Option<String>,
    pub cooldown_secs: f32,
}

pub fn apply_diagnostics(
    state: &mut crate::app_state::AppState,
    diags: Vec<crate::dsl::Diagnostic>,
) {
    if diags.is_empty() {
        state.dsl.diagnostics.clear();
        state.autosave.error = None;
    } else {
        state.dsl.diagnostics = diags.clone();
        state.autosave.pending = false;
        state.autosave.error = Some(diags[0].message.clone());
    }
}

pub fn tick(state: &mut crate::app_state::AppState, now: f64) {
    if let Some(last_edit) = state.autosave.last_edit_time {
        if now - last_edit > state.autosave.cooldown_secs as f64 {
            let diagnostics = crate::dsl::utils::validate_and_normalize(&mut state.dsl_code);
            apply_diagnostics(state, diagnostics);

            // In factory state, "saving" is just clearing the dirty flag
            state.autosave.pending = false;
            state.autosave.last_success_time = Some(now);
            state.autosave.last_edit_time = None;
        }
    }
}

impl AutosaveState {
    pub fn mark_dirty(&mut self, now: f64) {
        self.last_edit_time = Some(now);
        self.pending = true;
    }
}

use std::fs;

use crate::app_state::{AppState, ToastType};

/// Low-level writer for `code.motioner`.
/// When `show_toast` is false the operation is silent (used for autosave).
pub fn write_dsl_to_project(state: &mut AppState, show_toast: bool) -> std::io::Result<()> {
    let project_dir = match state.project_path.as_ref() {
        Some(p) => p,
        None => return Ok(()),
    };

    // Validate DSL before writing — if there are diagnostics, do not save.
    let diags = crate::dsl::validate_dsl(&state.dsl_code);
    if !diags.is_empty() {
        state.dsl_diagnostics = diags.clone();
        if show_toast {
            state.toast_message = Some(format!("Save failed: DSL errors"));
            state.toast_type = crate::app_state::ToastType::Error;
            // We don't have access to egui::Context here; approximate deadline
            state.toast_deadline = state.last_update as f64 + 3.0;
        }
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            diags[0].message.clone(),
        ));
    }

    let dst = project_dir.join("code.motioner");
    fs::write(&dst, &state.dsl_code)?;
    state.last_export_path = Some(dst.clone());
    // After successful save, normalise indentation to use tabs and persist
    // the normalized version (silent autosave). Only overwrite if changes
    // are produced and the normalized source still validates.
    let normalized = crate::dsl::generator::normalize_tabs(&state.dsl_code);
    if normalized != state.dsl_code {
        // validate normalized before writing
        let diags = crate::dsl::validate_dsl(&normalized);
        if diags.is_empty() {
            fs::write(&dst, &normalized)?;
            state.dsl_code = normalized;
        }
    }
    // Clear diagnostics on successful save
    state.dsl_diagnostics.clear();
    if show_toast {
        state.toast_message = Some(format!(
            "Saved {}",
            dst.file_name().unwrap().to_string_lossy()
        ));
        state.toast_type = ToastType::Success;
    }
    Ok(())
}

/// Public event: call this when element properties change and you want the
/// user to be notified on success/failure. This delegates to the writer
/// with toast enabled.
pub fn on_element_properties_changed(state: &mut AppState) {
    let _ = write_dsl_to_project(state, true);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn write_dsl_fails_when_dsl_invalid_and_sets_diagnostics() {
        let mut state = crate::app_state::AppState::default();
        let td = tempdir().expect("tempdir");
        state.project_path = Some(td.path().to_path_buf());

        // invalid DSL: missing header and top-level move without element
        state.dsl_code = "move { to = (0.1, 0.2) }".to_string();

        let res = write_dsl_to_project(&mut state, false);
        assert!(res.is_err());
        assert!(!state.dsl_diagnostics.is_empty());

        // Ensure file was not written
        let dst = td.path().join("code.motioner");
        assert!(!dst.exists());
    }

    #[test]
    fn write_dsl_converts_leading_spaces_to_tabs_on_success() {
        let mut state = crate::app_state::AppState::default();
        let td = tempdir().expect("tempdir");
        state.project_path = Some(td.path().to_path_buf());

        // valid DSL (includes header) but indented with spaces
        state.dsl_code = "size(1280, 720)\ntimeline(fps = 60, duration = 5.00)\n\ncircle \"C\" {\n    x = 0.5,\n    y = 0.5,\n    radius = 0.1,\n    spawn = 0.0\n}\n".to_string();

        let res = write_dsl_to_project(&mut state, false);
        assert!(res.is_ok());

        let dst = td.path().join("code.motioner");
        let content = fs::read_to_string(&dst).expect("read written file");

        // saved file should contain tabs for indentation
        assert!(content.contains("\n\tx ="));
        // state.dsl_code should be updated to normalized version
        assert!(state.dsl_code.contains('\t'));
    }
}

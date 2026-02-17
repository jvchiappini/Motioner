use std::fs;

use crate::app_state::{AppState, ToastType};

/// Low-level writer for `code.motioner`.
/// When `show_toast` is false the operation is silent (used for autosave).
pub fn write_dsl_to_project(state: &mut AppState, show_toast: bool) -> std::io::Result<()> {
    let project_dir = match state.project_path.as_ref() {
        Some(p) => p,
        None => return Ok(()),
    };

    let dst = project_dir.join("code.motioner");
    fs::write(&dst, &state.dsl_code)?;
    state.last_export_path = Some(dst.clone());
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

use crate::app_state::AppState;

// Pull in the autocomplete helper now that it's part of this submodule.
// The file was moved from the crate root, so we declare it here and adjust
// the call sites accordingly.
mod autocomplete;
use eframe::egui;
use egui::scroll_area::ScrollAreaOutput;
use std::sync::Arc;

mod gutter;
mod highlighter;
mod minimap;
pub mod utils;

// re-export a few helpers from submodules so callers can continue using the
// existing `crate::code_panel::find_matching_paren` path without changing.
// this import is purely a re-export and may not be referenced locally,
// so silence the 'unused_imports' lint which triggers otherwise.
#[allow(unused_imports)]
pub use utils::find_matching_paren;

use highlighter::highlight_code;
use minimap::render_minimap;
use utils::handle_color_pickers;

// ---- Helpers -------------------------------------------------------------

/// Render the small header bar that contains the maximize/minimize button and
/// the autosave status indicator. This was previously inline inside
/// `show()`; pulling it out keeps the main UI function shorter and easier to
/// read.
fn render_header(ui: &mut egui::Ui, state: &mut AppState) {
    ui.horizontal(|ui| {
        // Save button removed — autosave will persist while editing.
        if ui
            .button(if state.code_fullscreen {
                "📉 Minimize"
            } else {
                "📈 Maximize"
            })
            .clicked()
        {
            state.code_fullscreen = !state.code_fullscreen;
        }

        // Autosave indicator (replaces toast for editor autosaves)
        let now = ui.ctx().input(|i| i.time);
        if state.autosave.pending {
            ui.label(egui::RichText::new("Autosaving…").weak());
        } else if let Some(err) = &state.autosave.error {
            ui.colored_label(egui::Color32::from_rgb(220, 100, 100), "Autosave failed")
                .on_hover_text(err);
        } else if let Some(t) = state.autosave.last_success_time {
            if now - t < 2.0 {
                ui.colored_label(egui::Color32::from_rgb(120, 200, 140), "Autosaved ✓");
            } else {
                ui.label(
                    egui::RichText::new("Edit code — autosave while typing")
                        .italics()
                        .weak(),
                );
            }
        } else {
            ui.label(
                egui::RichText::new("Edit code — autosave while typing")
                    .italics()
                    .weak(),
            );
        }
    });
}

/// Render the main text editor area inside a scrollable region.  Returns the
/// `ScrollAreaOutput` produced by `egui::ScrollArea::show` so that callers can
/// inspect the scroll offset (needed for the minimap) and other state.
///
/// The `layouter` argument is the same closure passed to the original
/// `TextEdit::layouter`.  We accept it by mutable reference so that the caller
/// can create it inline and still mutate it afterwards if necessary.
fn render_editor<F>(
    ui: &mut egui::Ui,
    state: &mut AppState,
    layouter: &mut F,
) -> ScrollAreaOutput<egui::Response>
where
    // the current egui version expects the layouter to return an `Arc<Galley>`
    F: FnMut(&egui::Ui, &str, f32) -> Arc<egui::text::Galley>,
{
    ui.set_min_width(ui.available_width());
    ui.set_min_height(ui.available_height()); // Ensure clickable area covers view

    let text_edit_id = ui.make_persistent_id("dsl_text_edit");

    // 1. Process Input BEFORE TextEdit (Consume keys)
    crate::code_panel::autocomplete::process_input(ui, text_edit_id, state);

    // Editor with a left gutter for line numbers (VSCode-style)
    let gutter_width = 56.0f32;

    // We'll capture the TextEdit response so it remains available after the
    // `ui.horizontal` closure (needed by autosave logic below).
    let mut editor_response: Option<egui::Response> = None;

    let output = egui::ScrollArea::vertical()
        .id_source("code_editor_scroll")
        .auto_shrink([false, false]) // Use full available space
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                // Main code text edit
                let rows = state.dsl_code.lines().count().max(6); // ensure a reasonable minimum height
                let text_edit = egui::TextEdit::multiline(&mut state.dsl_code)
                    .id(text_edit_id) // Explicit ID
                    .font(egui::TextStyle::Monospace) // for cursor height
                    .code_editor()
                    .desired_rows(rows)
                    .frame(false) // Transparent frame so background shows through
                    .desired_width(f32::INFINITY)
                    .lock_focus(true)
                    .layouter(layouter);

                let text_output = text_edit.show(ui);
                let _output_rect = text_output.response.rect; // capture rect so we can draw while still owning `output`

                // 2. Update State & Render Popup AFTER TextEdit
                crate::code_panel::autocomplete::handle_state_and_render(
                    ui,
                    &text_output.response,
                    state,
                );

                // 3. Render Color Pickers for hex color strings
                handle_color_pickers(ui, state, &text_output);

                // Render & handle gutter (extracted). Pass DSL diagnostics for inline markers.
                gutter::render_gutter(
                    ui,
                    &text_output,
                    text_edit_id,
                    &state.dsl_code,
                    &state.dsl_diagnostics,
                    gutter_width,
                );

                // finally store the response so outer scope can use it
                editor_response = Some(text_output.response);
            });

            //retrieve the TextEdit response we captured from the horizontal layout
            let output = editor_response.expect("text edit response");

            // Autosave behavior: on any editor change, persist DSL silently.
            // Run a *quick* validation immediately so we can block autosave
            // while the user types clearly-invalid content (prevents saving
            // arbitrary garbage). Full parsing/scene update remains debounced
            // in `ui::update`.
            if output.changed() {
                // On every keystroke we no longer run validation/normalization
                // immediately.  Doing so caused diagnostics to flash while the
                // user was typing and contributed to the perceived lag.
                //
                // Instead we simply mark the buffer dirty and allow the
                // debounced autosave logic to perform validation (and
                // normalization) once the user has been idle for the cooldown
                // period.  See `AppState::autosave_tick` for the implementation.
                let now = ui.ctx().input(|i| i.time);
                state.autosave.on_change(now, None);
            }

            output
        });

    output
}

/// Show a persistent banner at the bottom of the screen when there are DSL
/// diagnostics.  This used to live inline in `show()` but pulling it out cuts
/// another 40 lines from the main function and makes the layout intent more
/// apparent.
fn render_error_banner(ui: &mut egui::Ui, state: &mut AppState) {
    if state.dsl_diagnostics.is_empty() {
        return;
    }

    let diag = &state.dsl_diagnostics[0];
    egui::TopBottomPanel::bottom("code_error_banner")
        .frame(
            egui::Frame::none()
                .fill(egui::Color32::from_rgb(80, 20, 20))
                .inner_margin(8.0),
        )
        .show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("❌").size(14.0));
                ui.label(
                    egui::RichText::new(format!("Line {}: {}", diag.line, diag.message))
                        .color(egui::Color32::from_rgb(255, 180, 180))
                        .strong(),
                );
            });
        });
}

pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    render_header(ui, state);
    ui.separator();

    // Prepare non-borrowing pointers into `state` so we can reference the data
    // inside the layouter without holding on to a borrow of `state` itself.  The
    // pointers remain valid for the duration of `show()` because `state` is
    // borrowed mutably for the whole call.
    let scene_ptr = state.scene.as_ptr();
    let scene_len = state.scene.len();

    let handlers_ptr = state.dsl_event_handlers.as_ptr();
    let handlers_len = state.dsl_event_handlers.len();

    // Custom layouter that applies the DSL syntax highlighter to the editor's
    // layout job. Kept short and local so it can capture `defined_names` /
    // `handlers` without extra indirection.
    let mut layouter = move |ui: &egui::Ui, string: &str, wrap_width: f32| {
        let mut layout_job = egui::text::LayoutJob::default();

        // reconstruct borrowed slices from raw pointers; safe because the
        // original vectors live for the duration of this function call and
        // cannot be mutated while the editor is drawing.
        let handlers_slice: &[crate::dsl::runtime::DslHandler] =
            unsafe { std::slice::from_raw_parts(handlers_ptr, handlers_len) };

        // predicate that checks whether a word matches any scene element name.
        let is_defined = |word: &str| unsafe {
            std::slice::from_raw_parts(scene_ptr, scene_len)
                .iter()
                .any(|s| s.name == word)
        };

        highlight_code(&mut layout_job, string, is_defined, handlers_slice);
        layout_job.wrap.max_width = wrap_width;
        ui.fonts(|f| f.layout_job(layout_job))
    };

    let available_rect = ui.available_rect_before_wrap();
    // Paint the background for the whole scroll area to look like the editor
    ui.painter()
        .rect_filled(available_rect, 0.0, egui::Color32::from_rgb(10, 10, 10));

    let is_fullscreen = state.code_fullscreen;
    let minimap_width = if is_fullscreen { 120.0 } else { 0.0 };

    // Editor Area Rect (Full width or minus minimap)
    let editor_rect = egui::Rect::from_min_max(
        available_rect.min,
        egui::pos2(available_rect.max.x - minimap_width, available_rect.max.y),
    );

    // Render the editor contents and return the scroll output for later use
    // (minimap, etc.).
    let scroll_output = render_editor(
        &mut ui.child_ui(editor_rect, *ui.layout()),
        state,
        &mut layouter,
    );
    render_error_banner(ui, state);

    if state.completion_popup_open {
        // Keep a minimal placeholder so re-paint can occur when the
        // completion popup is requested; detailed rendering is handled
        // by `autocomplete::handle_state_and_render`.
    }

    if is_fullscreen {
        let minimap_rect = egui::Rect::from_min_max(
            egui::pos2(available_rect.max.x - minimap_width, available_rect.min.y),
            available_rect.max,
        );
        render_minimap(
            ui,
            minimap_rect,
            &state.dsl_code,
            scroll_output.state.offset,
            editor_rect.height(),
        );
    }
}

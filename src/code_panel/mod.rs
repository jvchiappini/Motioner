use crate::app_state::AppState;
use crate::autocomplete; // Added this
use eframe::egui;

mod gutter;
mod highlighter;
mod minimap;
pub mod utils;

use highlighter::highlight_code;
use minimap::render_minimap;
use utils::handle_color_pickers;

pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
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
        if state.autosave_pending {
            ui.label(egui::RichText::new("Autosaving…").weak());
        } else if let Some(err) = &state.autosave_error {
            ui.colored_label(egui::Color32::from_rgb(220, 100, 100), "Autosave failed")
                .on_hover_text(err);
        } else if let Some(t) = state.autosave_last_success_time {
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
    ui.separator();

    let defined_names: std::collections::HashSet<String> =
        state.scene.iter().map(|s| s.name().to_string()).collect();

    // Clone handler metadata so the highlighter closure doesn't borrow `state`
    // (avoids borrow-checker conflicts with other mutable UI closures).
    let handlers = state.dsl_event_handlers.clone();

    let mut layouter = |ui: &egui::Ui, string: &str, wrap_width: f32| {
        let mut layout_job = egui::text::LayoutJob::default();
        highlight_code(&mut layout_job, string, &defined_names, &handlers); // custom highlighter with handler colors
        layout_job.wrap.max_width = wrap_width; // no wrapping
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

    // Create a child UI for the editor to constrain it
    let mut editor_ui = ui.child_ui(editor_rect, *ui.layout());

    let scroll_output = egui::ScrollArea::vertical()
        .id_source("code_editor_scroll")
        .auto_shrink([false, false]) // Use full available space
        .show(&mut editor_ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.set_min_height(ui.available_height()); // Ensure clickable area covers view

            let text_edit_id = ui.make_persistent_id("dsl_text_edit");

            // 1. Process Input BEFORE TextEdit (Consume keys)
            autocomplete::process_input(ui, text_edit_id, state);

            // Editor with a left gutter for line numbers (VSCode-style)
            let gutter_width = 56.0f32;

            // We'll capture the TextEdit response so it remains available after the
            // `ui.horizontal` closure (needed by autosave logic below).
            let mut editor_response: Option<egui::Response> = None;

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
                    .layouter(&mut layouter);

                let output = text_edit.show(ui);
                let output_rect = output.response.rect; // capture rect so we can draw while still owning `output`

                // 2. Update State & Render Popup AFTER TextEdit
                autocomplete::handle_state_and_render(ui, &output.response, state);

                // 3. Render Color Pickers for hex color strings
                handle_color_pickers(ui, state, &output);

                // Render & handle gutter (extracted)
                gutter::render_gutter(ui, &output, text_edit_id, &state.dsl_code, gutter_width);

                // finally store the response so outer scope can use it
                editor_response = Some(output.response);
            });

            // retrieve the TextEdit response we captured from the horizontal layout
            let output = editor_response.expect("text edit response");

            // Autosave behavior: on any editor change, persist DSL silently,
            // attempt to parse/apply configuration and regenerate preview if parse succeeds.
            if output.changed() {
                // mark edit time so App::update will debounce the actual disk write
                state.last_code_edit_time = Some(ui.ctx().input(|i| i.time));
                state.autosave_pending = true;

                // Parsing and scene/preview updates are debounced and handled in `ui::update`
            }

            output
        });

    if state.completion_popup_open {
        // We still need a repaint for the popup to show up if it's new,
        // but it's better handled in handle_state_and_render now.
        // I'll keep it here just in case but without the spam.
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

/// Find the index of the matching `)` for the `(` at `open_pos` inside `s`.
/// Returns `Some(idx)` (byte index relative to `s`) or `None` if not found.
pub fn find_matching_paren(s: &str, open_pos: usize) -> Option<usize> {
    // Iterate starting at the byte index `open_pos` (don't use `Iterator::skip`)
    let mut depth: i32 = 0;
    let mut in_string = false;
    if open_pos >= s.len() {
        return None;
    }
    for (i, ch) in s[open_pos..].char_indices() {
        let idx = open_pos + i; // byte index relative to `s`
        if ch == '"' {
            in_string = !in_string;
            continue;
        }
        if in_string {
            continue;
        }
        if ch == '(' {
            depth += 1;
        } else if ch == ')' {
            depth -= 1;
            if depth == 0 {
                return Some(idx);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_matching_paren_handles_strings_and_nesting() {
        let s = "call(a, b(\"(ignored)\"), c) tail";
        // position of first '(' (the one after `call`)
        let open = s.find('(').unwrap();
        let matched = find_matching_paren(s, open).expect("matched");
        assert_eq!(&s[matched..matched + 1], ")");
        // ensure it matched the final closing paren before " tail"
        assert!(s[matched + 1..].starts_with(" tail"));
    }
}

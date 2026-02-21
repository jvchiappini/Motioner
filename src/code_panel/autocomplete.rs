use crate::app_state::AppState;
use eframe::egui;

/// Consumes events (Arrows, Tab, Enter) BEFORE TextEdit sees them,
/// preventing cursor movement or newline insertion during autocomplete navigation.
pub fn process_input(ui: &mut egui::Ui, text_edit_id: egui::Id, state: &mut AppState) {
    // If nothing to do (no popup and no active snippet), return early.
    if !state.completion_popup_open && !state.completion_snippet_active {
        return;
    }

    // Capture input state once to avoid nested input() calls which cause deadlocks
    let (tab_pressed, enter_pressed, esc_pressed, up_pressed, down_pressed) = ui.input(|i| {
        (
            i.key_pressed(egui::Key::Tab),
            i.key_pressed(egui::Key::Enter),
            i.key_pressed(egui::Key::Escape),
            i.key_pressed(egui::Key::ArrowUp),
            i.key_pressed(egui::Key::ArrowDown),
        )
    });

    // --- Snippet (tab-stop) navigation ---
    if state.completion_snippet_active {
        if tab_pressed {
            // consume the Tab so TextEdit doesn't insert a tab character
            ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Tab));
            // Recompute parameter ranges inside the snippet region (robust to edits)
            if let Some((s, e)) = state.completion_snippet_region {
                state.completion_snippet_params =
                    parse_snippet_param_ranges(&state.dsl_code[s..e], s);
                // move to next parameter
                let next = match state.completion_snippet_index {
                    None => Some(0),
                    Some(i) => Some(i + 1),
                };

                if let Some(idx) = next {
                    if idx < state.completion_snippet_params.len() {
                        state.completion_snippet_index = Some(idx);
                        let (ps_byte, pe_byte) = state.completion_snippet_params[idx];
                        if let Some(mut te_state) =
                            egui::TextEdit::load_state(ui.ctx(), text_edit_id)
                        {
                            // Convert byte index to char index for egui cursor
                            let ps_char = state.dsl_code[..ps_byte].chars().count();
                            let pe_char = state.dsl_code[..pe_byte].chars().count();

                            let start_cc = egui::text::CCursor::new(ps_char);
                            let end_cc = egui::text::CCursor::new(pe_char);
                            te_state
                                .cursor
                                .set_char_range(Some(egui::text::CCursorRange::two(
                                    start_cc, end_cc,
                                )));
                            egui::TextEdit::store_state(ui.ctx(), text_edit_id, te_state);
                        }
                        return;
                    }
                }

                // No more params — finish snippet mode
                state.completion_snippet_active = false;
                state.completion_snippet_region = None;
                state.completion_snippet_params.clear();
                state.completion_snippet_index = None;
                return;
            }
        }
        // If snippet is active but user pressed Escape -> cancel snippet mode
        if esc_pressed {
            ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
            state.completion_snippet_active = false;
            state.completion_snippet_region = None;
            state.completion_snippet_params.clear();
            state.completion_snippet_index = None;
            return;
        }
    }

    // --- Completion popup navigation / acceptance ---
    let mut action = None; // "consume" or "complete"

    if state.completion_popup_open && !state.completion_items.is_empty() {
        if down_pressed {
            state.completion_selected_index =
                (state.completion_selected_index + 1) % state.completion_items.len();
            action = Some("consume");
        } else if up_pressed {
            if state.completion_selected_index == 0 {
                state.completion_selected_index = state.completion_items.len() - 1;
            } else {
                state.completion_selected_index -= 1;
            }
            action = Some("consume");
        } else if tab_pressed || enter_pressed {
            action = Some("complete");
        } else if esc_pressed {
            state.completion_popup_open = false;
            state.completion_items.clear();
            action = Some("consume");
        }
    }

    if let Some(act) = action {
        ui.input_mut(|i| {
            i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown);
            i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp);
            i.consume_key(egui::Modifiers::NONE, egui::Key::Tab);
            i.consume_key(egui::Modifiers::NONE, egui::Key::Enter);
            i.consume_key(egui::Modifiers::NONE, egui::Key::Escape);
        });

        if act == "complete" {
            apply_completion_by_index(ui, text_edit_id, state, state.completion_selected_index);
        }
    }
}

// Helper: find the byte index of the start of the word before `cursor_byte_idx`.
fn find_word_start(text: &str, cursor_byte_idx: usize) -> usize {
    let mut start = cursor_byte_idx.min(text.len());
    while start > 0 {
        // Find the start of the previous character
        let mut prev = start - 1;
        while prev > 0 && !text.is_char_boundary(prev) {
            prev -= 1;
        }

        if let Some(c) = text[prev..start].chars().next() {
            if !c.is_alphanumeric() && c != '_' {
                break;
            }
            start = prev;
        } else {
            break;
        }
    }
    start
}

// Parse parameter value byte ranges inside a snippet region. Returns absolute ranges.
fn parse_snippet_param_ranges(snippet: &str, base: usize) -> Vec<(usize, usize)> {
    let mut params = Vec::new();

    // 1) Find quoted name: pattern `word "NAME"`
    if let Some(qstart) = snippet.find('"') {
        if let Some(qend_rel) = snippet[qstart + 1..].find('"') {
            let s = base + qstart + 1;
            let e = s + qend_rel;
            params.push((s, e));
        }
    }

    // 2) Find occurrences of `key = value` and capture `value` until comma or newline or '}'
    let mut idx = 0usize;
    while let Some(eq_pos) = snippet[idx..].find('=') {
        let eq_abs = idx + eq_pos;
        // value starts after '=' (skip whitespace)
        let mut val_start_rel = eq_abs + 1;
        while val_start_rel < snippet.len() {
            let ch = snippet.as_bytes()[val_start_rel];
            if !(ch == b' ' || ch == b'\t') {
                break;
            }
            val_start_rel += 1;
        }
        // find end (comma, newline or closing brace)
        let mut val_end_rel = val_start_rel;
        while val_end_rel < snippet.len() {
            let ch = snippet.as_bytes()[val_end_rel];
            if ch == b',' || ch == b'\n' || ch == b'}' {
                break;
            }
            val_end_rel += 1;
        }
        if val_start_rel < val_end_rel {
            params.push((base + val_start_rel, base + val_end_rel));
        }
        idx = val_end_rel + 1;
    }

    params
}

// Apply the completion (keyboard acceptance or mouse click) for a given index.
fn apply_completion_by_index(
    ui: &mut egui::Ui,
    text_edit_id: egui::Id,
    state: &mut AppState,
    index: usize,
) {
    if index >= state.completion_items.len() {
        return;
    }
    let cursor_byte_idx = state.completion_cursor_idx;
    let start_byte = find_word_start(&state.dsl_code, cursor_byte_idx);

    let item = &state.completion_items[index];

    // Replace current word range [start_byte..cursor_byte_idx] with insert_text
    state
        .dsl_code
        .replace_range(start_byte..cursor_byte_idx, &item.insert_text);

    let new_end_byte = start_byte + item.insert_text.len();

    // Convert new byte index back to char index for egui CCursor
    let new_end_char = state.dsl_code[..new_end_byte].chars().count();

    // Update egui TextEdit cursor to end of inserted text (or first param if snippet)
    if item.is_snippet {
        // mark snippet region and compute param ranges
        state.completion_snippet_active = true;
        state.completion_snippet_region = Some((start_byte, new_end_byte));
        state.completion_snippet_params =
            parse_snippet_param_ranges(&state.dsl_code[start_byte..new_end_byte], start_byte);
        state.completion_snippet_index = None; // will move to first on Tab

        // Select the first parameter immediately so user can start typing
        if let Some(&(ps_byte, pe_byte)) = state.completion_snippet_params.first() {
            state.completion_snippet_index = Some(0);

            if let Some(mut te_state) = egui::TextEdit::load_state(ui.ctx(), text_edit_id) {
                let ps_char = state.dsl_code[..ps_byte].chars().count();
                let pe_char = state.dsl_code[..pe_byte].chars().count();

                let start_cc = egui::text::CCursor::new(ps_char);
                let end_cc = egui::text::CCursor::new(pe_char);
                te_state
                    .cursor
                    .set_char_range(Some(egui::text::CCursorRange::two(start_cc, end_cc)));
                egui::TextEdit::store_state(ui.ctx(), text_edit_id, te_state);
            }
        } else {
            // fallback: put cursor at snippet end
            if let Some(mut te_state) = egui::TextEdit::load_state(ui.ctx(), text_edit_id) {
                let ccursor = egui::text::CCursor::new(new_end_char);
                te_state
                    .cursor
                    .set_char_range(Some(egui::text::CCursorRange::one(ccursor)));
                egui::TextEdit::store_state(ui.ctx(), text_edit_id, te_state);
            }
        }
    } else {
        // normal completion — place cursor after inserted text
        if let Some(mut te_state) = egui::TextEdit::load_state(ui.ctx(), text_edit_id) {
            let ccursor = egui::text::CCursor::new(new_end_char);
            te_state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::one(ccursor)));
            egui::TextEdit::store_state(ui.ctx(), text_edit_id, te_state);
        }
    }

    // Update our tracking cursor idx
    state.completion_cursor_idx = new_end_byte;

    // Close popup and clear suggestions
    state.completion_popup_open = false;
    state.completion_items.clear();
}

/// Updates state (opens popup based on typing) and renders the popup.
/// Call this AFTER TextEdit.
pub fn handle_state_and_render(ui: &mut egui::Ui, response: &egui::Response, state: &mut AppState) {
    if !response.has_focus() {
        return;
    }

    // Update cursor information from TextEdit state
    let mut cursor_byte_idx = 0;
    if let Some(text_edit_state) = egui::TextEdit::load_state(ui.ctx(), response.id) {
        if let Some(range) = text_edit_state.cursor.char_range() {
            let char_idx = range.primary.index;
            // Convert character index to byte index safely and efficiently
            cursor_byte_idx = state
                .dsl_code
                .char_indices()
                .nth(char_idx)
                .map(|(b, _)| b)
                .unwrap_or(state.dsl_code.len());

            state.completion_cursor_idx = cursor_byte_idx;
        }
    }

    let text = &state.dsl_code;
    if cursor_byte_idx > text.len() {
        return;
    }

    // Extract current word
    let start = find_word_start(text, cursor_byte_idx);
    let current_word = &text[start..cursor_byte_idx];

    // 1. Detect Typing (Open Popup) — debounce and require a small minimum length
    if response.changed() {
        let now = ui.ctx().input(|i| i.time);
        let min_trigger_len = 2;

        if current_word.is_empty() {
            state.completion_popup_open = false;
            state.completion_items.clear();
            state.last_completion_query = None;
            state.last_completion_query_time = now;
        } else if current_word.len() < min_trigger_len {
            state.completion_popup_open = false;
            state.completion_items.clear();
            state.last_completion_query = Some(current_word.to_string());
            state.last_completion_query_time = now;
        } else {
            let changed_query = match &state.last_completion_query {
                Some(prev) => prev != current_word,
                None => true,
            };

            if changed_query || (now - state.last_completion_query_time) > 0.05 {
                // Send request to worker
                if let Some(tx) = &state.completion_worker_tx {
                    let _ = tx.send(current_word.to_string());
                }

                state.last_completion_query = Some(current_word.to_string());
                state.last_completion_query_time = now;
            }
        }
    }

    // Check for worker results
    if let Some(rx) = &state.completion_worker_rx {
        while let Ok(items) = rx.try_recv() {
            state.completion_items = items;
            state.completion_popup_open = !state.completion_items.is_empty();
            state.completion_selected_index = 0;
            ui.ctx().request_repaint(); // Show results as soon as they arrive
        }
    }

    // 2. Render Popup
    if state.completion_popup_open && !state.completion_items.is_empty() {
        let popup_pos = if let Some(ptr) = ui.input(|i| i.pointer.hover_pos()) {
            ptr + egui::vec2(0.0, 20.0)
        } else {
            // Fallback: estimate from cursor position if possible
            response.rect.min + egui::vec2(50.0, 50.0)
        };

        let mut clicked_idx: Option<usize> = None;
        egui::Area::new("autocomplete_popup")
            .fixed_pos(popup_pos)
            .order(egui::Order::Foreground)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style())
                    .shadow(egui::epaint::Shadow::small_dark())
                    .show(ui, |ui| {
                        for (i, item) in state.completion_items.iter().enumerate() {
                            let selected = i == state.completion_selected_index;
                            let label = if item.is_snippet {
                                format!("{}  ⌁", item.label)
                            } else {
                                item.label.clone()
                            };

                            if ui.selectable_label(selected, label).clicked() {
                                clicked_idx = Some(i);
                            }
                        }
                    });
            });

        if let Some(i) = clicked_idx {
            apply_completion_by_index(ui, response.id, state, i);
        }
    }
}

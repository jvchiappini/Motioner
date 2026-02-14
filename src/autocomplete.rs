use crate::app_state::AppState;
use eframe::egui;

/// Consumes events (Arrows, Tab, Enter) BEFORE TextEdit sees them,
/// preventing cursor movement or newline insertion during autocomplete navigation.
pub fn process_input(ui: &mut egui::Ui, text_edit_id: egui::Id, state: &mut AppState) {
    if !state.completion_popup_open || state.completion_items.is_empty() { 
        return; 
    }

    let mut action = None; // "consume" or "complete"

    // Check keys
    if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
        state.completion_selected_index = (state.completion_selected_index + 1) % state.completion_items.len();
        action = Some("consume");
    } else if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
         if state.completion_selected_index == 0 {
             state.completion_selected_index = state.completion_items.len() - 1;
         } else {
             state.completion_selected_index -= 1;
         }
         action = Some("consume");
    } else if ui.input(|i| i.key_pressed(egui::Key::Tab) || i.key_pressed(egui::Key::Enter)) {
         action = Some("complete");
    } else if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        state.completion_popup_open = false;
        state.completion_items.clear();
        action = Some("consume");
    }

    // Apply Action
    if let Some(act) = action {
        // Consume ALL relevant keys to be safe so TextEdit doesn't get them
        ui.input_mut(|i| {
            i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown);
            i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp);
            i.consume_key(egui::Modifiers::NONE, egui::Key::Tab);
            i.consume_key(egui::Modifiers::NONE, egui::Key::Enter);
            i.consume_key(egui::Modifiers::NONE, egui::Key::Escape);
        });

        if act == "complete" {
            // Logic to insert text using the LAST KNOWN cursor index
            let cursor_idx = state.completion_cursor_idx;
            let text = &state.dsl_code;

            // Safe backward search for word start using known cursor_idx
            if cursor_idx <= text.len() {
                 let mut start = cursor_idx;
                 while start > 0 {
                     let slice = &text[..start];
                     if let Some(c) = slice.chars().next_back() {
                         if !c.is_alphanumeric() && c != '_' { break; }
                         start -= c.len_utf8();
                     } else { break; }
                 }
                 
                 let current_word = &text[start..cursor_idx];
                 if state.completion_selected_index < state.completion_items.len() {
                    let suggestion = &state.completion_items[state.completion_selected_index];
                    if suggestion.starts_with(current_word) {
                        let suffix = &suggestion[current_word.len()..];
                        state.dsl_code.insert_str(cursor_idx, suffix);

                        // Move Cursor to end of inserted word
                        let new_cursor_idx = cursor_idx + suffix.len();
                        
                        // Update EgUI TextEdit State
                        if let Some(mut text_edit_state) = egui::TextEdit::load_state(ui.ctx(), text_edit_id) {
                            let ccursor = egui::text::CCursor::new(new_cursor_idx);
                            text_edit_state.cursor.set_char_range(Some(egui::text::CCursorRange::one(ccursor)));
                            egui::TextEdit::store_state(ui.ctx(), text_edit_id, text_edit_state);
                        }
                        
                        // Update our tracking state too
                        state.completion_cursor_idx = new_cursor_idx;
                    }
                 }
            }
            state.completion_popup_open = false;
            state.completion_items.clear();
        }
    }
}

/// Updates state (opens popup based on typing) and renders the popup.
/// Call this AFTER TextEdit.
pub fn handle_state_and_render(ui: &mut egui::Ui, response: &egui::Response, state: &mut AppState) {
    if !response.has_focus() {
        return;
    }

    // Update Cursor Position for next frame's input handling
    // Only update if we have focus and valid range
    if let Some(text_edit_state) = egui::TextEdit::load_state(ui.ctx(), response.id) {
        if let Some(range) = text_edit_state.cursor.char_range() { 
            state.completion_cursor_idx = range.primary.index;
        }
    }

    // Re-acquire cursor info for logic
    let cursor_idx = state.completion_cursor_idx;
    let text = &state.dsl_code;
    if cursor_idx > text.len() { return; }

    let mut start = cursor_idx;
    while start > 0 {
        let slice = &text[..start];
        let last_char: Option<char> = slice.chars().next_back();
        if let Some(c) = last_char {
                if !c.is_alphanumeric() && c != '_' {
                    break;
                }
                start -= c.len_utf8();
        } else {
                break;
        }
    }
    let current_word = &text[start..cursor_idx];

    // 1. Detect Typing (Open Popup)
    if response.changed() {
        if !current_word.is_empty() {
            let keywords = ["project", "timeline", "layer", "fps", "duration", "size", "circle", "rect", "fill", "radius", "width", "height", "color"];
            state.completion_items = keywords.iter()
                .filter(|k| k.starts_with(current_word) && **k != current_word)
                .map(|k| k.to_string())
                .collect();
            
            state.completion_popup_open = !state.completion_items.is_empty();
            state.completion_selected_index = 0;
        } else {
            state.completion_popup_open = false;
        }
    }

    // 2. Render Popup
    if state.completion_popup_open && !state.completion_items.is_empty() {
        let popup_pos = if let Some(ptr) = ui.input(|i| i.pointer.hover_pos()) {
            ptr + egui::vec2(0.0, 20.0)
        } else {
            response.rect.min + egui::vec2(50.0, 50.0) 
        };
        
        egui::Area::new("autocomplete_popup")
            .fixed_pos(popup_pos)
            .order(egui::Order::Foreground)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style())
                    .shadow(egui::epaint::Shadow::small_dark())
                    .show(ui, |ui| {
                    for (i, item) in state.completion_items.iter().enumerate() {
                        let selected = i == state.completion_selected_index;
                        if ui.selectable_label(selected, item).clicked() {
                                // Handle click (TODO: Insert here too?)
                        }
                    }
                });
            });
    }
}

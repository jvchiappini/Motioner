use eframe::egui;

/// Render the left gutter (line numbers) and handle gutter clicks that move
/// the TextEdit cursor. This encapsulates all gutter drawing + interaction.
pub(crate) fn render_gutter(
    ui: &mut egui::Ui,
    output: &egui::text_edit::TextEditOutput,
    text_edit_id: egui::Id,
    dsl_code: &str,
    gutter_width: f32,
) {
    // Reserve gutter area (will scroll together with the TextEdit because
    // both are inside the same ScrollArea closure). Use a clickable rect
    // so we can detect gutter clicks via the returned Response.
    let gutter_response = ui.allocate_rect(
        egui::Rect::from_min_size(
            ui.cursor().min,
            egui::vec2(gutter_width, ui.available_height()),
        ),
        egui::Sense::click(),
    );
    let gutter_rect = gutter_response.rect;

    // --- GUTTER RENDERING ---
    // Match the font size used in highlight_code (14.0)
    let font_id = egui::FontId::monospace(14.0);

    // Determine active logical line efficiently
    let mut active_line_idx: usize = 0;
    if let Some(te_state) = egui::TextEdit::load_state(ui.ctx(), text_edit_id) {
        if let Some(range) = te_state.cursor.char_range() {
            let char_idx = range.primary.index;
            active_line_idx = dsl_code
                .chars()
                .take(char_idx)
                .filter(|&c| c == '\n')
                .count();
        }
    }

    // Draw gutter background
    let mut full_gutter_rect = gutter_rect;
    full_gutter_rect.set_bottom(ui.clip_rect().bottom().max(output.response.rect.bottom()));

    let gutter_painter = ui.painter().with_clip_rect(full_gutter_rect);
    gutter_painter.rect_filled(full_gutter_rect, 0.0, egui::Color32::from_rgb(24, 24, 24));

    let gutter_text_color = egui::Color32::from_gray(100);

    // Use the galley to accurately position line numbers, handling wrapping and different scales
    let galley = &output.galley;
    let galley_pos = output.galley_pos;

    let mut current_logical_line = 0;

    for row in &galley.rows {
        // Get character start index for this row using the galley's cursor system
        // We use the vertical center of the row to ensure we hit the right row
        let row_center_y = row.rect.center().y;
        let cursor = galley.cursor_from_pos(egui::vec2(0.0, row_center_y));
        let row_start_idx = cursor.ccursor.index;

        // A row is the start of a logical line if it starts at index 0
        // or if the character immediately preceding it is a newline.
        let is_start_of_logical_line = row_start_idx == 0 || {
            let prev_idx = row_start_idx - 1;
            dsl_code.chars().nth(prev_idx) == Some('\n')
        };

        if is_start_of_logical_line {
            let line_index = current_logical_line;
            current_logical_line += 1;

            // Calculate Y position based on the galley's layout
            let y = galley_pos.y + row.rect.top();

            // Optimization: Skip drawing if outside the visible clip rect
            if y + row.rect.height() < ui.clip_rect().top() {
                continue;
            }
            if y > ui.clip_rect().bottom() {
                break;
            }

            let num = format!("{}", line_index + 1);
            let is_active = line_index == active_line_idx;

            gutter_painter.text(
                egui::pos2(full_gutter_rect.right() - 8.0, y),
                egui::Align2::RIGHT_TOP,
                num,
                font_id.clone(),
                if is_active {
                    egui::Color32::from_rgb(220, 220, 220)
                } else {
                    gutter_text_color
                },
            );
        }
    }

    // Make gutter clickable: clicking a line will move cursor to line start
    if gutter_response.clicked() {
        if let Some(pos) = ui.ctx().pointer_interact_pos() {
            let galley_y = pos.y - galley_pos.y;

            // Use the galley to find the cursor at the clicked position
            let cursor = galley.cursor_from_pos(egui::vec2(0.0, galley_y));
            let char_idx = cursor.ccursor.index;
            let ccursor = egui::text::CCursor::new(char_idx);

            if let Some(mut te_state) = egui::TextEdit::load_state(ui.ctx(), text_edit_id) {
                te_state
                    .cursor
                    .set_char_range(Some(egui::text::CCursorRange::one(ccursor)));
                egui::TextEdit::store_state(ui.ctx(), text_edit_id, te_state);
            }
        }
    }
}

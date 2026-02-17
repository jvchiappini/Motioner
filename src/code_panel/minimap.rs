use eframe::egui;

pub fn render_minimap(
    ui: &mut egui::Ui,
    rect: egui::Rect,
    code: &str,
    scroll_offset: egui::Vec2,
    viewport_height: f32,
) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(18, 18, 18)); // Minimap bg

    // Minimap Rendering Parameters
    let code_line_height = 14.0; // Assuming monospace(14.0) from highlight_code
    let minimap_scale = 0.2; // 20% size
    let mm_line_height = code_line_height * minimap_scale;
    let mm_char_width = 8.0 * minimap_scale; // Approx char width

    // Draw Code Blocks
    // We can't draw every char as a rect efficiently if the file is huge,
    // but for DSL it's likely small efficiently.
    // Optimization: Draw lines as simplified blocks.

    let start_y = rect.top() + 4.0;

    let mut y = start_y;

    // Very simple highlighting for minimap (simplified parser)
    for line in code.lines() {
        // Skip rendering if out of bounds (optimization)
        if y > rect.bottom() {
            break;
        }

        let mut x = rect.left() + 4.0;

        // Simple "words" tokenizer
        // let mut word_color = egui::Color32::from_gray(100);

        // Heuristic: Color whole words based on first char or context?
        // Let's just iterate chars for "accurate" mini-view
        // If too slow, switch to block words.

        let mut chars = line.chars().peekable();
        while let Some(c) = chars.next() {
            if x > rect.right() {
                break;
            }

            let color = if c.is_whitespace() {
                None
            } else if c.is_ascii_digit() {
                Some(egui::Color32::from_rgb(181, 206, 168)) // Number green
            } else if "\"".contains(c) {
                Some(egui::Color32::from_rgb(206, 145, 120)) // String
            } else if "()[]{}".contains(c) {
                Some(egui::Color32::from_rgb(255, 200, 50)) // Bracket Gold
            } else if c == '/' && chars.peek() == Some(&'/') {
                // Comment - rest of line
                Some(egui::Color32::from_rgb(90, 120, 90)) // Comment Green
            } else if c.is_lowercase() {
                Some(egui::Color32::from_rgb(86, 156, 214)) // Keyword/Param Blue
            } else if c.is_uppercase() {
                Some(egui::Color32::from_rgb(78, 201, 176)) // Object Teal
            } else {
                Some(egui::Color32::from_gray(120))
            };

            if let Some(col) = color {
                // If it's a comment, draw a long bar
                if col == egui::Color32::from_rgb(80, 100, 80) {
                    let len = line.len() as f32 * mm_char_width; // Approx
                    painter.rect_filled(
                        egui::Rect::from_min_size(
                            egui::pos2(x, y),
                            egui::vec2(len.min(rect.right() - x), mm_line_height * 0.8),
                        ),
                        1.0,
                        col,
                    );
                    break; // Next line
                }

                painter.rect_filled(
                    egui::Rect::from_min_size(
                        egui::pos2(x, y),
                        egui::vec2(mm_char_width, mm_line_height * 0.8),
                    ),
                    0.5,
                    col,
                );
            }
            x += mm_char_width;
        }

        y += mm_line_height * 1.5; // Line spacing
    }

    // Draw Viewport Shadow Overlay
    // Map scroll_offset.y to minimap y
    let scroll_y = scroll_offset.y;
    let mm_scroll_y = scroll_y * minimap_scale;
    let mm_viewport_h = viewport_height * minimap_scale;

    let highlight_rect = egui::Rect::from_min_size(
        egui::pos2(rect.left(), start_y + mm_scroll_y),
        egui::vec2(rect.width(), mm_viewport_h),
    );

    // Draw semi-transparent overlay over the NON-visible parts?
    // Or highlight the visible part. VS Code highlights the visible part with a light hover effect.
    ui.painter().rect_filled(highlight_rect, 0.0, egui::Color32::from_white_alpha(15));
    ui.painter().rect_stroke(
        highlight_rect,
        0.0,
        egui::Stroke::new(1.0, egui::Color32::from_white_alpha(30)),
    );
}

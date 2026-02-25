use crate::app_state::AppState;
use eframe::egui;

pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    let top_bar_bg = egui::Color32::from_rgb(37, 37, 38);
    let top_bar_stroke = egui::Color32::from_rgb(51, 51, 51);
    let gutter_bg = egui::Color32::from_rgb(30, 30, 30);
    let gutter_fg = egui::Color32::from_rgb(133, 133, 133);

    // Header bar (similar to tabs in VSCode)
    egui::Frame::none()
        .fill(top_bar_bg)
        .inner_margin(egui::vec2(16.0, 8.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("📄 motion.dsl")
                        .color(egui::Color32::from_rgb(224, 224, 224))
                        .size(13.0),
                );
            });
        });

    // Divider line
    let rect = ui.max_rect();
    ui.painter().hline(
        rect.x_range(),
        ui.cursor().top(),
        egui::Stroke::new(1.0, top_bar_stroke),
    );
    ui.add_space(1.0);

    let theme = egui_extras::syntax_highlighting::CodeTheme::from_memory(ui.ctx());
    let mut layouter = |ui: &egui::Ui, string: &str, _wrap_width: f32| {
        let mut layout_job =
            egui_extras::syntax_highlighting::highlight(ui.ctx(), &theme, string, "rs");
        layout_job.wrap.max_width = f32::INFINITY; // Disable wrap to keep line numbers perfectly synced
        ui.fonts(|f| f.layout_job(layout_job))
    };

    let font_id = egui::TextStyle::Monospace.resolve(ui.style());
    let row_height = ui.fonts(|f| f.row_height(&font_id));

    let available_height = ui.available_height();

    // Use a ScrollArea with both vertical and horizontal scroll
    egui::ScrollArea::both()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                let spacing = ui.spacing_mut();
                spacing.item_spacing.x = 0.0; // no gap between gutter and code

                // Calculate the number of lines (at least 1, and count newlines)
                let num_lines = state.dsl_code.split('\n').count().max(1);

                // Gutter width based on digits
                let digits = num_lines.to_string().len().max(2);
                let gutter_width =
                    digits as f32 * ui.fonts(|f| f.glyph_width(&font_id, '0')) + 24.0;

                // Total height needed for gutter background and text editor empty space
                let content_height = (num_lines as f32 * row_height).max(available_height);

                // 1) Render the Line Gutter
                let (gutter_rect, _) = ui.allocate_exact_size(
                    egui::vec2(gutter_width, content_height),
                    egui::Sense::hover(),
                );

                // Draw background for gutter exactly to the allocated bounds
                ui.painter().rect_filled(gutter_rect, 0.0, gutter_bg);

                // Draw the line numbers
                for i in 1..=num_lines {
                    let y = gutter_rect.top() + (i - 1) as f32 * row_height;
                    let num_str = format!("{}", i);

                    let galley = ui.fonts(|f| {
                        f.layout(num_str, font_id.clone(), gutter_fg, gutter_width - 8.0)
                    });

                    // Right align text in the gutter
                    let x = gutter_rect.right() - 12.0 - galley.rect.width();

                    ui.painter()
                        .galley(egui::pos2(x, y), galley, egui::Color32::PLACEHOLDER);
                }

                // 2) Code Editor
                ui.add_space(4.0); // Margin from gutter to text

                // Let's modify the text edit to look beautiful and fill exact space
                let available_size = ui.available_size();
                let (text_rect, _text_response) = ui.allocate_exact_size(
                    egui::vec2(available_size.x, content_height),
                    egui::Sense::click(),
                );

                let mut text_output = None;

                // Draw TextEdit within this allocated rect so it doesn't get pushed to the right
                ui.allocate_ui_at_rect(text_rect, |ui| {
                    let output = egui::TextEdit::multiline(&mut state.dsl_code)
                        .font(egui::TextStyle::Monospace)
                        .frame(false)
                        .desired_width(f32::INFINITY)
                        .margin(egui::vec2(0.0, 0.0))
                        .lock_focus(true)
                        .layouter(&mut layouter);
                    
                    text_output = Some(output.show(ui));
                });

                if let Some(text_out) = text_output {
                    // If user clicks in the empty area below the text, focus the text editor
                    if ui.rect_contains_pointer(text_rect) && ui.input(|i| i.pointer.primary_clicked()) {
                        text_out.response.request_focus();
                    }

                    if text_out.response.changed() {
                        state.autosave.mark_dirty(ui.input(|i| i.time));
                    }
                }
            });
        });
}

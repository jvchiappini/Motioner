use crate::app_state::AppState;
use eframe::egui;

pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    ui.vertical(|ui| {
        ui.label(egui::RichText::new("DSL Code").strong());
        
        let theme = egui_extras::syntax_highlighting::CodeTheme::from_memory(ui.ctx());
        let mut layouter = |ui: &egui::Ui, string: &str, wrap_width: f32| {
            let mut layout_job = egui_extras::syntax_highlighting::highlight(ui.ctx(), &theme, string, "rs");
            layout_job.wrap.max_width = wrap_width;
            ui.fonts(|f| f.layout_job(layout_job))
        };

        egui::ScrollArea::vertical().show(ui, |ui| {
            if ui.add(
                egui::TextEdit::multiline(&mut state.dsl_code)
                    .font(egui::TextStyle::Monospace)
                    .code_editor()
                    .desired_width(f32::INFINITY)
                    .lock_focus(true)
                    .layouter(&mut layouter)
            ).changed() {
                state.autosave.mark_dirty(ui.input(|i| i.time));
            }
        });
    });
}

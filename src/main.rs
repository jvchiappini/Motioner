mod app_state;
mod autocomplete; // Added this
mod code_panel;
mod dsl;
mod project_settings;
mod renderer;
mod scene;
mod scene_graph;
mod timeline;
mod ui;
mod welcome_modal; // Added this

use anyhow::Result;

fn main() -> Result<()> {
    let native_options = eframe::NativeOptions::default();
    let _ = eframe::run_native(
        "Motioner UI",
        native_options,
        Box::new(|_cc| Box::new(ui::create_app())),
    );
    Ok(())
}

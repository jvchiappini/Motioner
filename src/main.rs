mod dsl;
mod renderer;
mod scene;
mod ui;
mod app_state;
mod scene_graph;
mod timeline;
mod code_panel;
mod project_settings; // Added this

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

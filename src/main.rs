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
use display_info::DisplayInfo;
use eframe::egui;

fn main() -> Result<()> {
    let mut native_options = eframe::NativeOptions::default();

    // Size the window to 80% of the primary monitor and center it (cross-platform + HiDPI aware).
    let mut win_w = 1280.0 * 0.85;
    let mut win_h = 720.0 * 0.85;
    let mut pos = None;

    if let Ok(displays) = DisplayInfo::all() {
        if let Some(primary) = displays.iter().find(|d| d.is_primary).or(displays.first()) {
            let scale = primary.scale_factor as f32;
            let mon_w = primary.width as f32 / scale;
            let mon_h = primary.height as f32 / scale;

            win_w = mon_w * 0.85;
            win_h = mon_h * 0.85;

            let center_x = (mon_w - win_w) / 2.0;
            let center_y = (mon_h - win_h) / 2.0;

            // Adjust for monitor position
            let mon_x = primary.x as f32 / scale;
            let mon_y = primary.y as f32 / scale;

            pos = Some(egui::pos2(mon_x + center_x, mon_y + center_y));
        }
    }

    let mut viewport = egui::ViewportBuilder::default().with_inner_size(egui::vec2(win_w, win_h));

    if let Some(p) = pos {
        viewport = viewport.with_position(p);
    }

    native_options.viewport = viewport;

    let _ = eframe::run_native(
        "Motioner UI",
        native_options,
        Box::new(|_cc| Box::new(ui::create_app())),
    );
    Ok(())
}

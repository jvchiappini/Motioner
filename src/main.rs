// Deny any unused code in the entire crate so that dead functions/structs
// are caught by the compiler. This forces us to remove or refactor any
// code that isn't referenced rather than hiding it behind `allow`.
#![deny(dead_code)]

mod animations;
mod app_state;
mod canvas;
mod code_panel;
mod dsl;
mod events;
mod logics;
mod logo;
mod modals;
mod project_settings;
mod renderer;
mod scene;
mod scene_graph;
mod shapes;
mod states; // ensure state-related modules are available early
mod timeline;
mod ui;
mod welcome_modal; // Added this

use anyhow::Result;
use display_info::DisplayInfo;
use eframe::egui;

#[allow(clippy::field_reassign_with_default)]
fn main() -> Result<()> {
    let mut native_options = eframe::NativeOptions::default();
    native_options.renderer = eframe::Renderer::Wgpu;
    native_options.wgpu_options.power_preference = wgpu::PowerPreference::HighPerformance;

    // Size the window to 80% of the primary monitor and center it (cross-platform + HiDPI aware).
    let mut win_w = 1280.0 * 0.85;
    let mut win_h = 720.0 * 0.85;
    let mut pos = None;

    if let Ok(displays) = DisplayInfo::all() {
        if let Some(primary) = displays.iter().find(|d| d.is_primary).or(displays.first()) {
            let scale = primary.scale_factor;
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

    // Sanity-check computed window size/position before applying an explicit
    // viewport. If values are invalid or out-of-range, fall back to the
    // platform-default placement (avoids off-screen windows).
    let mut set_viewport = false;
    if win_w.is_finite() && win_h.is_finite() && win_w > 200.0 && win_h > 200.0 {
        if let Some(p) = pos {
            if p.x.is_finite() && p.y.is_finite() {
                set_viewport = true;
            }
        }
    }

    let mut viewport = egui::ViewportBuilder::default().with_inner_size(egui::vec2(win_w, win_h));
    if let Some(icon) = logo::icon_data_from_svg(include_str!("../assets/logo.svg")) {
        viewport = viewport.with_icon(std::sync::Arc::new(icon));
    }

    if set_viewport {
        if let Some(p) = pos {
            viewport = viewport.with_position(p);
        }
        native_options.viewport = viewport;
        /*println!(
            "[motioner] explicit viewport applied - size={}x{} pos={:?}",
            win_w, win_h, pos
        );*/
    } else {
        native_options.viewport = viewport;
        /*println!(
            "[motioner] explicit viewport NOT applied - using OS placement (size={}x{} pos={:?})",
            win_w, win_h, pos
        );*/
    }

    // Run the native eframe app. If initialization fails (wgpu/device/etc.)
    // print the error and retry once with a simplified `NativeOptions` that
    // does not set an explicit viewport — this helps on systems where the
    // computed position/size would place the window off-screen or when
    // platform-specific viewport creation fails.
    match eframe::run_native(
        "Motioner UI",
        native_options,
        Box::new(|cc| Box::new(ui::create_app(cc))),
    ) {
        Ok(()) => Ok(()),
        Err(_err) => {
            /*eprintln!(
                "eframe::run_native failed: {:?}. Retrying with default options...",
                _err
            );*/
            // Retry with default options (no explicit viewport)
            let mut fallback = eframe::NativeOptions::default();
            fallback.renderer = eframe::Renderer::Wgpu;
            match eframe::run_native(
                "Motioner UI (fallback)",
                fallback,
                Box::new(|cc| Box::new(ui::create_app(cc))),
            ) {
                Ok(()) => Ok(()),
                Err(err2) => {
                    //eprintln!("Fallback start failed too: {:?}", err2);
                    Err(anyhow::anyhow!("eframe fallback start failed: {:#?}", err2))
                }
            }
        }
    }
}

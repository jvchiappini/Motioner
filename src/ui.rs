use crate::app_state::AppState;
use crate::{canvas, code_panel, timeline};
use eframe::egui;

pub struct MyApp {
    state: AppState,
}

pub fn create_app(cc: &eframe::CreationContext<'_>) -> MyApp {
    let mut state = AppState::default();

    // Initialize folder dialog channel
    let (tx, rx) = std::sync::mpsc::channel();
    state.folder_dialog_tx = Some(tx);
    state.folder_dialog_rx = Some(rx);

    // Initialize logo texture
    if let Some(logo_image) = crate::logo::color_image_from_svg(include_str!("../assets/logo.svg")) {
        state.logo_texture = Some(cc.egui_ctx.load_texture(
            "logo_texture",
            logo_image,
            Default::default()
        ));
    }

    MyApp { state }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let state = &mut self.state;
        let now = ctx.input(|i| i.time);
        let _ = state.tick(now);

        let is_modal_open = state.show_welcome;

        // Sidebar Toolbar (Far Left)
        egui::SidePanel::left("toolbar_panel")
            .resizable(false)
            .default_width(48.0)
            .frame(egui::Frame::none().fill(egui::Color32::from_rgb(20, 20, 22)))
            .show(ctx, |ui| {
                ui.set_enabled(!is_modal_open);
                canvas::ui::toolbar::show(ui, state);
            });

        egui::TopBottomPanel::bottom("timeline_panel")
            .resizable(true)
            .min_height(120.0)
            .show(ctx, |ui| {
                ui.set_enabled(!is_modal_open);
                timeline::show(ui, state);
            });

        egui::SidePanel::left("code_panel")
            .resizable(true)
            .default_width(400.0)
            .show(ctx, |ui| {
                ui.set_enabled(!is_modal_open);
                code_panel::show(ui, state);
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(egui::Color32::from_rgb(15, 15, 17)))
            .show(ctx, |ui| {
                ui.set_enabled(!is_modal_open);
                canvas::show(ui, state, true);
            });

        if state.playing && !is_modal_open {
            let dt = ctx.input(|i| i.stable_dt);
            state.set_time(state.time + dt);
            ctx.request_repaint();
        }

        if is_modal_open {
            crate::modals::welcome_modal::show(ctx, state);
        }
    }
}

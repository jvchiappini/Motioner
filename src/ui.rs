use crate::app_state::{AppState, PanelTab};
use crate::{canvas, code_panel, timeline};
use eframe::egui;

pub struct MyApp {
    state: AppState,
}

pub fn create_app(_cc: &eframe::CreationContext<'_>) -> MyApp {
    let state = AppState::default();
    MyApp { state }
}

impl eframe::App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let state = &mut self.state;
        let now = ctx.input(|i| i.time);
        let _ = state.tick(now);

        // Sidebar Toolbar (Far Left)
        egui::SidePanel::left("toolbar_panel")
            .resizable(false)
            .default_width(48.0)
            .frame(egui::Frame::none().fill(egui::Color32::from_rgb(20, 20, 22)))
            .show(ctx, |ui| {
                canvas::ui::toolbar::show(ui, state);
            });

        egui::TopBottomPanel::bottom("timeline_panel")
            .resizable(true)
            .min_height(120.0)
            .show(ctx, |ui| {
                timeline::show(ui, state);
            });

        egui::SidePanel::left("code_panel")
            .resizable(true)
            .default_width(400.0)
            .show(ctx, |ui| {
                code_panel::show(ui, state);
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(egui::Color32::from_rgb(15, 15, 17)))
            .show(ctx, |ui| {
                canvas::show(ui, state, true);
            });

        if state.playing {
            let dt = ctx.input(|i| i.stable_dt);
            state.set_time(state.time + dt);
            ctx.request_repaint();
        }
    }
}

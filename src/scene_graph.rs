use crate::app_state::AppState;
#[allow(unused_imports)]
use crate::scene::{Scene, Shape};
use eframe::egui;

pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    ui.add_space(4.0);
    // Heading handled by parent container
    egui::ScrollArea::vertical().show(ui, |ui| {
        let items: Vec<(usize, String)> = state
            .scene
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let icon = match s {
                    Shape::Circle { .. } => "●",
                    Shape::Rect { .. } => "▭",
                };
                let lbl = match s {
                    Shape::Circle { .. } => format!("Circle #{i}"),
                    Shape::Rect { .. } => format!("Rect #{i}"),
                };
                (i, format!("{} {}", icon, lbl))
            })
            .collect();

        for (i, label) in items {
            ui.horizontal(|ui| {
                let selected_here = Some(i) == state.selected;
                if ui.selectable_label(selected_here, label).clicked() {
                    state.selected = Some(i);
                }
                if ui.small_button("✖").clicked() {
                    state.scene.remove(i);
                    state.selected = if state.scene.is_empty() {
                        None
                    } else {
                        Some(0)
                    };
                }
            });
        }
    });

    ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
        ui.horizontal(|ui| {
            if ui.button("Add circle").clicked() {
                state.scene.push(Shape::Circle {
                    x: 0.2,
                    y: 0.5,
                    radius: 40.0,
                    color: [120, 200, 255, 255],
                });
                state.selected = Some(state.scene.len() - 1);
            }
            if ui.button("Add rect").clicked() {
                state.scene.push(Shape::Rect {
                    x: 0.5,
                    y: 0.5,
                    w: 120.0,
                    h: 60.0,
                    color: [200, 120, 120, 255],
                });
                state.selected = Some(state.scene.len() - 1);
            }
        });
    });
}

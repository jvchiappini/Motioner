use crate::app_state::AppState;
/// Renderiza la barra de herramientas rápida sobre el canvas.
use eframe::egui;

/// Muestra los controles de FPS, Multiplicador de previsualización y el cuentagotas.
pub fn show_toolbar(ui: &mut egui::Ui, state: &mut AppState, composition_rect: egui::Rect) {
    let menu_pos = ui.max_rect().min + egui::vec2(10.0, 10.0);

    egui::Area::new("canvas_quick_settings")
        .fixed_pos(menu_pos)
        .order(egui::Order::Foreground)
        .show(ui.ctx(), |ui| {
            egui::Frame::none()
                .fill(egui::Color32::from_black_alpha(150))
                .rounding(4.0)
                .inner_margin(4.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;

                        // Botón de Cuentagotas
                        let picker_btn = egui::Button::new(egui::RichText::new("📷").size(14.0))
                            .fill(if state.picker_active {
                                egui::Color32::from_rgb(255, 100, 0)
                            } else {
                                egui::Color32::TRANSPARENT
                            });

                        if ui
                            .add(picker_btn)
                            .on_hover_text("Selector de Color")
                            .clicked()
                        {
                            state.picker_active = !state.picker_active;
                        }

                        // Muestra el color seleccionado actualmente
                        let (r, _) =
                            ui.allocate_at_least(egui::vec2(14.0, 14.0), egui::Sense::hover());
                        ui.painter().rect_filled(
                            r.shrink(2.0),
                            2.0,
                            egui::Color32::from_rgb(
                                state.picker_color[0],
                                state.picker_color[1],
                                state.picker_color[2],
                            ),
                        );
                        ui.painter().rect_stroke(
                            r.shrink(2.0),
                            2.0,
                            egui::Stroke::new(1.0, egui::Color32::GRAY),
                        );

                        // Toggle resize mode directly on the mini toolbar
                        ui.separator();
                        if ui
                            .checkbox(&mut state.resize_mode, "Resize")
                            .on_hover_text(
                                "Drag edges of an element to resize instead of selecting",
                            )
                            .changed()
                        {
                            if state.resize_mode {
                                state.move_mode = false;
                            }
                            state.autosave_pending = true;
                        }

                        // Toggle move mode; display simple arrow icons to hint
                        // at the fact that the element can be moved vertically,
                        // horizontally or both.  When active the arrows are shown
                        // inside the checkbox label.
                        if ui
                            .checkbox(&mut state.move_mode, "↕↔ Move")
                            .on_hover_text("Drag an element anywhere to move it (autosaves)")
                            .changed()
                        {
                            if state.move_mode {
                                state.resize_mode = false;
                            }
                            state.autosave_pending = true;
                        }

                        ui.separator();

                        // Multiplicador de Previsualización
                        ui.menu_button(format!("Preview: {}x", state.preview_multiplier), |ui| {
                            for &m in &[0.125, 0.25, 0.5, 1.0, 1.125, 1.25, 1.5, 2.0] {
                                if ui
                                    .selectable_label(
                                        state.preview_multiplier == m,
                                        format!("{}x", m),
                                    )
                                    .clicked()
                                {
                                    state.preview_multiplier = m;
                                    ui.close_menu();
                                }
                            }
                        });

                        ui.separator();

                        // Input de FPS
                        ui.add(
                            egui::DragValue::new(&mut state.preview_fps)
                                .prefix("FPS: ")
                                .clamp_range(1..=240),
                        );

                        ui.separator();

                        // Coordenadas relativas del ratón
                        if let Some(mouse_pos) = ui.input(|i| i.pointer.hover_pos()) {
                            let pct_x =
                                (mouse_pos.x - composition_rect.min.x) / composition_rect.width();
                            let pct_y =
                                (mouse_pos.y - composition_rect.min.y) / composition_rect.height();
                            ui.label(
                                egui::RichText::new(format!(
                                    "X: {:.2}%, Y: {:.2}%",
                                    pct_x * 100.0,
                                    pct_y * 100.0
                                ))
                                .monospace()
                                .color(egui::Color32::LIGHT_BLUE),
                            );
                        } else {
                            ui.label(
                                egui::RichText::new("X: ---%, Y: ---%")
                                    .monospace()
                                    .color(egui::Color32::GRAY),
                            );
                        }
                    });
                });
        });
}

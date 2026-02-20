use crate::app_state::AppState;
use crate::shapes::element_store::FrameProps;
use eframe::egui;

pub mod easing_curve_editor;
pub mod move_animation_element_modifiers;

/// Fullscreen, non-draggable Element Modifiers modal.
pub fn show(ctx: &egui::Context, state: &mut AppState) {
    // Only render when active
    let path = match &state.modifier_active_path {
        Some(p) => p.clone(),
        None => return,
    };

    // Floating, draggable window centered on the full app window
    let screen_rect = ctx.input(|i| i.screen_rect());

    // Floating, draggable window centered on screen (keep initial position)
    let mut open = true;
    let center = screen_rect.center();
    let default_w = 420.0f32;
    let default_h = 480.0f32;
    let default_pos = egui::pos2(center.x - default_w / 2.0, center.y - default_h / 2.0);

    let window = egui::Window::new("🔧 Element Modifiers")
        .open(&mut open)
        .resizable(true)
        .collapsible(true)
        .default_width(default_w)
        .default_height(default_h)
        .default_pos(default_pos)
        .movable(true);

    window.show(ctx, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| {
            // Body: render the same controls that previously lived in ui::show_modifier_modal
            let mut changed = false;
            // We only support top-level paths for now (ElementKeyframes are flat)
            let Some(elem) = path.get(0).and_then(|&i| state.scene.get_mut(i)) else {
                return;
            };
            ui.add_space(4.0);

            let earliest_spawn = elem.spawn_frame as f32 / state.fps as f32;

            // Stable identifier for this element's modifier UI (used as id_source
            // for collapsing headers so they don't reset when labels change).
            let path_id = state
                .modifier_active_path
                .as_ref()
                .map(|p| {
                    p.iter()
                        .map(|n| n.to_string())
                        .collect::<Vec<_>>()
                        .join("-")
                })
                .unwrap_or_else(|| "root".to_string());

            match elem.kind.as_str() {
                "circle" => {
                    ui.group(|ui| {
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("⭕").size(18.0));
                                ui.label(
                                    egui::RichText::new("Circle Parameters").strong().size(14.0),
                                );
                            });
                            ui.separator();

                            egui::Grid::new("circle_grid")
                                .num_columns(2)
                                .spacing([12.0, 8.0])
                                .show(ui, |ui| {
                                    ui.label("Name:");
                                    if ui.text_edit_singleline(&mut elem.name).changed() {
                                        changed = true;
                                    }
                                    ui.end_row();

                                    ui.label("Visible:");
                                    let mut vis =
                                        elem.visible.first().map(|kf| kf.value).unwrap_or(true);
                                    if ui.checkbox(&mut vis, "").changed() {
                                        elem.visible.clear();
                                        elem.visible.push(crate::shapes::element_store::Keyframe {
                                            frame: elem.spawn_frame,
                                            value: vis,
                                            easing: crate::animations::easing::Easing::Linear,
                                        });
                                        changed = true;
                                    }
                                    ui.end_row();

                                    ui.label("Spawn Time:");
                                    let mut spawn_secs = elem.spawn_frame as f32 / state.fps as f32;
                                    if ui
                                        .add(
                                            egui::Slider::new(
                                                &mut spawn_secs,
                                                0.0..=state.duration_secs,
                                            )
                                            .suffix("s"),
                                        )
                                        .changed()
                                    {
                                        elem.spawn_frame =
                                            crate::shapes::element_store::seconds_to_frame(
                                                spawn_secs, state.fps,
                                            );
                                        changed = true;
                                    }
                                    ui.end_row();

                                    ui.label("Position X:");
                                    let mut val_x = elem
                                        .sample(elem.spawn_frame)
                                        .and_then(|p| p.x)
                                        .unwrap_or(0.5)
                                        * 100.0;
                                    if ui
                                        .add(
                                            egui::Slider::new(&mut val_x, 0.0..=100.0)
                                                .suffix("%")
                                                .clamp_to_range(false),
                                        )
                                        .changed()
                                    {
                                        elem.insert_frame(
                                            elem.spawn_frame,
                                            FrameProps {
                                                x: Some(val_x / 100.0),
                                                y: None,
                                                radius: None,
                                                w: None,
                                                h: None,
                                                size: None,
                                                value: None,
                                                color: None,
                                                visible: None,
                                                z_index: None,
                                            },
                                        );
                                        changed = true;
                                    }
                                    ui.end_row();

                                    ui.label("Position Y:");
                                    let mut val_y = elem
                                        .sample(elem.spawn_frame)
                                        .and_then(|p| p.y)
                                        .unwrap_or(0.5)
                                        * 100.0;
                                    if ui
                                        .add(
                                            egui::Slider::new(&mut val_y, 0.0..=100.0)
                                                .suffix("%")
                                                .clamp_to_range(false),
                                        )
                                        .changed()
                                    {
                                        elem.insert_frame(
                                            elem.spawn_frame,
                                            FrameProps {
                                                x: None,
                                                y: Some(val_y / 100.0),
                                                radius: None,
                                                w: None,
                                                h: None,
                                                size: None,
                                                value: None,
                                                color: None,
                                                visible: None,
                                                z_index: None,
                                            },
                                        );
                                        changed = true;
                                    }
                                    ui.end_row();

                                    ui.label("Radius:");
                                    let mut val_r = elem
                                        .sample(elem.spawn_frame)
                                        .and_then(|p| p.radius)
                                        .unwrap_or(0.1)
                                        * 100.0;
                                    if ui
                                        .add(
                                            egui::Slider::new(&mut val_r, 0.0..=100.0)
                                                .suffix("%")
                                                .clamp_to_range(false),
                                        )
                                        .changed()
                                    {
                                        elem.insert_frame(
                                            elem.spawn_frame,
                                            FrameProps {
                                                x: None,
                                                y: None,
                                                radius: Some(val_r / 100.0),
                                                w: None,
                                                h: None,
                                                size: None,
                                                value: None,
                                                color: None,
                                                visible: None,
                                                z_index: None,
                                            },
                                        );
                                        changed = true;
                                    }
                                    ui.end_row();

                                    ui.label("Color:");
                                    let mut color = elem
                                        .sample(elem.spawn_frame)
                                        .and_then(|p| p.color)
                                        .unwrap_or([120, 200, 255, 255]);
                                    if ui
                                        .color_edit_button_srgba_unmultiplied(&mut color)
                                        .changed()
                                    {
                                        elem.insert_frame(
                                            elem.spawn_frame,
                                            FrameProps {
                                                x: None,
                                                y: None,
                                                radius: None,
                                                w: None,
                                                h: None,
                                                size: None,
                                                value: None,
                                                color: Some(color),
                                                visible: None,
                                                z_index: None,
                                            },
                                        );
                                        changed = true;
                                    }
                                    ui.end_row();
                                });

                            ui.add_space(4.0);
                            // Animations are no longer stored on `ElementKeyframes`.
                            ui.label("Animations editing disabled — migrating to per-track storage");
                        });
                    });
                }
                "rect" => {
                    ui.group(|ui| {
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("⬜").size(18.0));
                                ui.label(
                                    egui::RichText::new("Rectangle Parameters")
                                        .strong()
                                        .size(14.0),
                                );
                            });
                            ui.separator();

                            egui::Grid::new("rect_grid")
                                .num_columns(2)
                                .spacing([12.0, 8.0])
                                .show(ui, |ui| {
                                    ui.label("Name:");
                                    if ui.text_edit_singleline(&mut elem.name).changed() {
                                        changed = true;
                                    }
                                    ui.end_row();

                                    ui.label("Visible:");
                                    let mut vis =
                                        elem.visible.first().map(|kf| kf.value).unwrap_or(true);
                                    if ui.checkbox(&mut vis, "").changed() {
                                        elem.visible.clear();
                                        elem.visible.push(crate::shapes::element_store::Keyframe {
                                            frame: elem.spawn_frame,
                                            value: vis,
                                            easing: crate::animations::easing::Easing::Linear,
                                        });
                                        changed = true;
                                    }
                                    ui.end_row();

                                    ui.label("Spawn Time:");
                                    let mut spawn_secs = elem.spawn_frame as f32 / state.fps as f32;
                                    if ui
                                        .add(
                                            egui::Slider::new(
                                                &mut spawn_secs,
                                                0.0..=state.duration_secs,
                                            )
                                            .suffix("s"),
                                        )
                                        .changed()
                                    {
                                        elem.spawn_frame =
                                            crate::shapes::element_store::seconds_to_frame(
                                                spawn_secs, state.fps,
                                            );
                                        changed = true;
                                    }
                                    ui.end_row();

                                    ui.label("Position X:");
                                    let mut val_x = elem
                                        .sample(elem.spawn_frame)
                                        .and_then(|p| p.x)
                                        .unwrap_or(0.5)
                                        * 100.0;
                                    if ui
                                        .add(
                                            egui::Slider::new(&mut val_x, 0.0..=100.0)
                                                .suffix("%")
                                                .clamp_to_range(false),
                                        )
                                        .changed()
                                    {
                                        elem.insert_frame(
                                            elem.spawn_frame,
                                            FrameProps {
                                                x: Some(val_x / 100.0),
                                                y: None,
                                                radius: None,
                                                w: None,
                                                h: None,
                                                size: None,
                                                value: None,
                                                color: None,
                                                visible: None,
                                                z_index: None,
                                            },
                                        );
                                        changed = true;
                                    }
                                    ui.end_row();

                                    ui.label("Position Y:");
                                    let mut val_y = elem
                                        .sample(elem.spawn_frame)
                                        .and_then(|p| p.y)
                                        .unwrap_or(0.5)
                                        * 100.0;
                                    if ui
                                        .add(
                                            egui::Slider::new(&mut val_y, 0.0..=100.0)
                                                .suffix("%")
                                                .clamp_to_range(false),
                                        )
                                        .changed()
                                    {
                                        elem.insert_frame(
                                            elem.spawn_frame,
                                            FrameProps {
                                                x: None,
                                                y: Some(val_y / 100.0),
                                                radius: None,
                                                w: None,
                                                h: None,
                                                size: None,
                                                value: None,
                                                color: None,
                                                visible: None,
                                                z_index: None,
                                            },
                                        );
                                        changed = true;
                                    }
                                    ui.end_row();

                                    ui.label("Width:");
                                    let mut val_w = elem
                                        .sample(elem.spawn_frame)
                                        .and_then(|p| p.w)
                                        .unwrap_or(0.3)
                                        * 100.0;
                                    if ui
                                        .add(
                                            egui::Slider::new(&mut val_w, 0.0..=100.0)
                                                .suffix("%")
                                                .clamp_to_range(false),
                                        )
                                        .changed()
                                    {
                                        elem.insert_frame(
                                            elem.spawn_frame,
                                            FrameProps {
                                                x: None,
                                                y: None,
                                                radius: None,
                                                w: Some(val_w / 100.0),
                                                h: None,
                                                size: None,
                                                value: None,
                                                color: None,
                                                visible: None,
                                                z_index: None,
                                            },
                                        );
                                        changed = true;
                                    }
                                    ui.end_row();

                                    ui.label("Height:");
                                    let mut val_h = elem
                                        .sample(elem.spawn_frame)
                                        .and_then(|p| p.h)
                                        .unwrap_or(0.2)
                                        * 100.0;
                                    if ui
                                        .add(
                                            egui::Slider::new(&mut val_h, 0.0..=100.0)
                                                .suffix("%")
                                                .clamp_to_range(false),
                                        )
                                        .changed()
                                    {
                                        elem.insert_frame(
                                            elem.spawn_frame,
                                            FrameProps {
                                                x: None,
                                                y: None,
                                                radius: None,
                                                w: None,
                                                h: Some(val_h / 100.0),
                                                size: None,
                                                value: None,
                                                color: None,
                                                visible: None,
                                                z_index: None,
                                            },
                                        );
                                        changed = true;
                                    }
                                    ui.end_row();

                                    ui.label("Color:");
                                    let mut color = elem
                                        .sample(elem.spawn_frame)
                                        .and_then(|p| p.color)
                                        .unwrap_or([255, 100, 100, 255]);
                                    if ui
                                        .color_edit_button_srgba_unmultiplied(&mut color)
                                        .changed()
                                    {
                                        elem.insert_frame(
                                            elem.spawn_frame,
                                            FrameProps {
                                                x: None,
                                                y: None,
                                                radius: None,
                                                w: None,
                                                h: None,
                                                size: None,
                                                value: None,
                                                color: Some(color),
                                                visible: None,
                                                z_index: None,
                                            },
                                        );
                                        changed = true;
                                    }
                                    ui.end_row();
                                });

                            ui.add_space(4.0);
                            ui.label("Animations editing disabled — migrating to per-track storage");
                        });
                    });
                }
                "text" => {
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("📝").size(24.0));
                            ui.vertical(|ui| {
                                ui.label(egui::RichText::new("Text Element").strong().size(16.0));
                                ui.label(
                                    egui::RichText::new(format!("Name: {}", elem.name))
                                        .small()
                                        .weak(),
                                );
                            });
                        });
                        ui.add_space(8.0);
                        ui.separator();
                        ui.add_space(8.0);

                        // --- SECTION: BASIC INFO ---
                        ui.label(egui::RichText::new("Identification").strong());
                        egui::Grid::new("text_basic_grid")
                            .num_columns(2)
                            .spacing([12.0, 8.0])
                            .show(ui, |ui| {
                                ui.label("Name:");
                                if ui.text_edit_singleline(&mut elem.name).changed() {
                                    changed = true;
                                }
                                ui.end_row();

                                ui.label("Visibility:");
                                let mut vis =
                                    elem.visible.first().map(|kf| kf.value).unwrap_or(true);
                                if ui.checkbox(&mut vis, "Visible").changed() {
                                    elem.visible.clear();
                                    elem.visible.push(crate::shapes::element_store::Keyframe {
                                        frame: elem.spawn_frame,
                                        value: vis,
                                        easing: crate::animations::easing::Easing::Linear,
                                    });
                                    changed = true;
                                }
                                ui.end_row();
                            });
                        ui.add_space(12.0);

                        // --- SECTION: TRANSFORM ---
                        ui.label(egui::RichText::new("Transform & Timing").strong());
                        egui::Grid::new("text_transform_grid")
                            .num_columns(2)
                            .spacing([12.0, 8.0])
                            .show(ui, |ui| {
                                ui.label("Position:");
                                ui.horizontal(|ui| {
                                    ui.label("X");
                                    let mut x = elem
                                        .sample(elem.spawn_frame)
                                        .and_then(|p| p.x)
                                        .unwrap_or(0.5);
                                    if ui
                                        .add(
                                            egui::DragValue::new(&mut x)
                                                .speed(0.01)
                                                .clamp_range(0.0..=1.0),
                                        )
                                        .changed()
                                    {
                                        elem.insert_frame(
                                            elem.spawn_frame,
                                            FrameProps {
                                                x: Some(x),
                                                y: None,
                                                radius: None,
                                                w: None,
                                                h: None,
                                                size: None,
                                                value: None,
                                                color: None,
                                                visible: None,
                                                z_index: None,
                                            },
                                        );
                                        changed = true;
                                    }
                                    ui.add_space(8.0);
                                    ui.label("Y");
                                    let mut y = elem
                                        .sample(elem.spawn_frame)
                                        .and_then(|p| p.y)
                                        .unwrap_or(0.5);
                                    if ui
                                        .add(
                                            egui::DragValue::new(&mut y)
                                                .speed(0.01)
                                                .clamp_range(0.0..=1.0),
                                        )
                                        .changed()
                                    {
                                        elem.insert_frame(
                                            elem.spawn_frame,
                                            FrameProps {
                                                x: None,
                                                y: Some(y),
                                                radius: None,
                                                w: None,
                                                h: None,
                                                size: None,
                                                value: None,
                                                color: None,
                                                visible: None,
                                                z_index: None,
                                            },
                                        );
                                        changed = true;
                                    }
                                });
                                ui.end_row();

                                ui.label("Spawn Time:");
                                    let mut spawn_secs = elem.spawn_frame as f32 / state.fps as f32;
                                if ui
                                    .add(
                                        egui::Slider::new(
                                            &mut spawn_secs,
                                            0.0..=state.duration_secs,
                                        )
                                        .suffix("s"),
                                    )
                                    .changed()
                                {
                                    elem.spawn_frame =
                                        crate::shapes::element_store::seconds_to_frame(
                                                spawn_secs, state.fps,
                                        );
                                    changed = true;
                                }
                                ui.end_row();
                            });
                        ui.add_space(12.0);

                        // --- SECTION: CONTENT & BASE STYLE ---
                        ui.label(egui::RichText::new("Content & Base Style").strong());
                        ui.group(|ui| {
                            egui::Grid::new("text_content_grid")
                                .num_columns(2)
                                .spacing([12.0, 8.0])
                                .show(ui, |ui| {
                                    ui.label("Text:");
                                    let mut val = elem
                                        .sample(elem.spawn_frame)
                                        .and_then(|p| p.value)
                                        .unwrap_or_else(|| "".to_string());
                                    if ui.text_edit_singleline(&mut val).changed() {
                                        elem.insert_frame(
                                            elem.spawn_frame,
                                            FrameProps {
                                                x: None,
                                                y: None,
                                                radius: None,
                                                w: None,
                                                h: None,
                                                size: None,
                                                value: Some(val.clone()),
                                                color: None,
                                                visible: None,
                                                z_index: None,
                                            },
                                        );
                                        changed = true;
                                    }
                                    ui.end_row();

                                    ui.label("Base Size:");
                                    let mut size_pct = elem
                                        .sample(elem.spawn_frame)
                                        .and_then(|p| p.size)
                                        .unwrap_or(24.0)
                                        * 100.0;
                                    if ui
                                        .add(
                                            egui::DragValue::new(&mut size_pct)
                                                .speed(0.1)
                                                .clamp_range(0.1..=50.0)
                                                .suffix("%"),
                                        )
                                        .changed()
                                    {
                                        elem.insert_frame(
                                            elem.spawn_frame,
                                            FrameProps {
                                                x: None,
                                                y: None,
                                                radius: None,
                                                w: None,
                                                h: None,
                                                size: Some(size_pct / 100.0),
                                                value: None,
                                                color: None,
                                                visible: None,
                                                z_index: None,
                                            },
                                        );
                                        changed = true;
                                    }
                                    ui.end_row();

                                    ui.label("Base Color:");
                                    let mut color = elem
                                        .sample(elem.spawn_frame)
                                        .and_then(|p| p.color)
                                        .unwrap_or([200, 255, 100, 255]);
                                    if ui
                                        .color_edit_button_srgba_unmultiplied(&mut color)
                                        .changed()
                                    {
                                        elem.insert_frame(
                                            elem.spawn_frame,
                                            FrameProps {
                                                x: None,
                                                y: None,
                                                radius: None,
                                                w: None,
                                                h: None,
                                                size: None,
                                                value: None,
                                                color: Some(color),
                                                visible: None,
                                                z_index: None,
                                            },
                                        );
                                        changed = true;
                                    }
                                    ui.end_row();
                                });
                        });
                        ui.add_space(12.0);

                        // --- SECTION: ANIMATIONS ---
                        let base_x = elem
                            .sample(elem.spawn_frame)
                            .and_then(|p| p.x)
                            .unwrap_or(0.5);
                        let base_y = elem
                            .sample(elem.spawn_frame)
                            .and_then(|p| p.y)
                            .unwrap_or(0.5);
                        ui.label("Animations editing disabled — migrating to per-track storage");
                    });
                }
                "group" => {
                    ui.group(|ui| {
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("📦").size(18.0));
                                ui.label(
                                    egui::RichText::new("Group Parameters").strong().size(14.0),
                                );
                            });
                            ui.separator();
                        }); // ui.vertical
                    }); // ui.group
                }
                _ => {
                    // unknown kind: no-op
                }
            } // match elem.kind

            if changed {
                state.request_dsl_update();
                // position cache removed — no-op
                crate::events::element_properties_changed_event::on_element_properties_changed(
                    state,
                );
            }
        }); // ScrollArea
    }); // window.show
}

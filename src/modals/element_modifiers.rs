use crate::app_state::AppState;
use crate::scene::{get_shape_mut, Shape};
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
            if let Some(shape) = get_shape_mut(&mut state.scene, &path) {
                ui.add_space(4.0);

                let earliest_spawn = shape.spawn_time();

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

                match shape {
                    Shape::Circle(c) => {
                        ui.group(|ui| {
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new("⭕").size(18.0));
                                    ui.label(
                                        egui::RichText::new("Circle Parameters")
                                            .strong()
                                            .size(14.0),
                                    );
                                });
                                ui.separator();

                                egui::Grid::new("circle_grid")
                                    .num_columns(2)
                                    .spacing([12.0, 8.0])
                                    .show(ui, |ui| {
                                        ui.label("Name:");
                                        if ui.text_edit_singleline(&mut c.name).changed() {
                                            changed = true;
                                        }
                                        ui.end_row();

                                        ui.label("Visible:");
                                        if ui.checkbox(&mut c.visible, "").changed() {
                                            changed = true;
                                        }
                                        ui.end_row();

                                        ui.label("Spawn Time:");
                                        if ui
                                            .add(
                                                egui::Slider::new(
                                                    &mut c.spawn_time,
                                                    0.0..=state.duration_secs,
                                                )
                                                .suffix("s"),
                                            )
                                            .changed()
                                        {
                                            changed = true;
                                        }
                                        ui.end_row();

                                        ui.label("Position X:");
                                        let mut val_x = c.x * 100.0;
                                        if ui
                                            .add(
                                                egui::Slider::new(&mut val_x, 0.0..=100.0)
                                                    .suffix("%")
                                                    .clamp_to_range(false),
                                            )
                                            .changed()
                                        {
                                            c.x = val_x / 100.0;
                                            changed = true;
                                        }
                                        ui.end_row();

                                        ui.label("Position Y:");
                                        let mut val_y = c.y * 100.0;
                                        if ui
                                            .add(
                                                egui::Slider::new(&mut val_y, 0.0..=100.0)
                                                    .suffix("%")
                                                    .clamp_to_range(false),
                                            )
                                            .changed()
                                        {
                                            c.y = val_y / 100.0;
                                            changed = true;
                                        }
                                        ui.end_row();

                                        ui.label("Radius:");
                                        let mut val_r = c.radius * 100.0;
                                        if ui
                                            .add(
                                                egui::Slider::new(&mut val_r, 0.0..=100.0)
                                                    .suffix("%")
                                                    .clamp_to_range(false),
                                            )
                                            .changed()
                                        {
                                            c.radius = val_r / 100.0;
                                            changed = true;
                                        }
                                        ui.end_row();

                                        ui.label("Color:");
                                        if ui.color_edit_button_srgba_unmultiplied(&mut c.color).changed()
                                        {
                                            changed = true;
                                        }
                                        ui.end_row();
                                    });

                                ui.add_space(4.0);
                                // Render Move animations using dedicated module
                                move_animation_element_modifiers::render_move_animation_modifiers(
                                    ui,
                                    ctx,
                                    &mut c.animations,
                                    c.spawn_time,
                                    state.duration_secs,
                                    c.x,
                                    c.y,
                                    &path_id,
                                    &mut changed,
                                );
                            });
                        });
                    }
                    Shape::Rect(r) => {
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
                                        if ui.text_edit_singleline(&mut r.name).changed() {
                                            changed = true;
                                        }
                                        ui.end_row();

                                        ui.label("Visible:");
                                        if ui.checkbox(&mut r.visible, "").changed() {
                                            changed = true;
                                        }
                                        ui.end_row();

                                        ui.label("Spawn Time:");
                                        if ui
                                            .add(
                                                egui::Slider::new(
                                                    &mut r.spawn_time,
                                                    0.0..=state.duration_secs,
                                                )
                                                .suffix("s"),
                                            )
                                            .changed()
                                        {
                                            changed = true;
                                        }
                                        ui.end_row();

                                        ui.label("Position X:");
                                        let mut val_x = r.x * 100.0;
                                        if ui
                                            .add(
                                                egui::Slider::new(&mut val_x, 0.0..=100.0)
                                                    .suffix("%")
                                                    .clamp_to_range(false),
                                            )
                                            .changed()
                                        {
                                            r.x = val_x / 100.0;
                                            changed = true;
                                        }
                                        ui.end_row();

                                        ui.label("Position Y:");
                                        let mut val_y = r.y * 100.0;
                                        if ui
                                            .add(
                                                egui::Slider::new(&mut val_y, 0.0..=100.0)
                                                    .suffix("%")
                                                    .clamp_to_range(false),
                                            )
                                            .changed()
                                        {
                                            r.y = val_y / 100.0;
                                            changed = true;
                                        }
                                        ui.end_row();

                                        ui.label("Width:");
                                        let mut val_w = r.w * 100.0;
                                        if ui
                                            .add(
                                                egui::Slider::new(&mut val_w, 0.0..=100.0)
                                                    .suffix("%")
                                                    .clamp_to_range(false),
                                            )
                                            .changed()
                                        {
                                            r.w = val_w / 100.0;
                                            changed = true;
                                        }
                                        ui.end_row();

                                        ui.label("Height:");
                                        let mut val_h = r.h * 100.0;
                                        if ui
                                            .add(
                                                egui::Slider::new(&mut val_h, 0.0..=100.0)
                                                    .suffix("%")
                                                    .clamp_to_range(false),
                                            )
                                            .changed()
                                        {
                                            r.h = val_h / 100.0;
                                            changed = true;
                                        }
                                        ui.end_row();

                                        ui.label("Color:");
                                        if ui.color_edit_button_srgba_unmultiplied(&mut r.color).changed()
                                        {
                                            changed = true;
                                        }
                                        ui.end_row();
                                    });

                                ui.add_space(4.0);
                                // Render Move animations using dedicated module
                                move_animation_element_modifiers::render_move_animation_modifiers(
                                    ui,
                                    ctx,
                                    &mut r.animations,
                                    r.spawn_time,
                                    state.duration_secs,
                                    r.x,
                                    r.y,
                                    &path_id,
                                    &mut changed,
                                );
                            });
                        });
                    }
                    Shape::Text(t) => {
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("📝").size(24.0));
                                ui.vertical(|ui| {
                                    ui.label(egui::RichText::new("Text Element").strong().size(16.0));
                                    ui.label(egui::RichText::new(format!("Name: {}", t.name)).small().weak());
                                });
                            });
                            ui.add_space(8.0);
                            ui.separator();
                            ui.add_space(8.0);

                            // --- SECTION: BASIC INFO ---
                            ui.label(egui::RichText::new("Identification").strong());
                            egui::Grid::new("text_basic_grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                                ui.label("Name:");
                                if ui.text_edit_singleline(&mut t.name).changed() { changed = true; }
                                ui.end_row();

                                ui.label("Visibility:");
                                if ui.checkbox(&mut t.visible, "Visible").changed() { changed = true; }
                                ui.end_row();
                            });
                            ui.add_space(12.0);

                            // --- SECTION: TRANSFORM ---
                            ui.label(egui::RichText::new("Transform & Timing").strong());
                            egui::Grid::new("text_transform_grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                                ui.label("Position:");
                                ui.horizontal(|ui| {
                                    ui.label("X");
                                    if ui.add(egui::DragValue::new(&mut t.x).speed(0.01).clamp_range(0.0..=1.0)).changed() { changed = true; }
                                    ui.add_space(8.0);
                                    ui.label("Y");
                                    if ui.add(egui::DragValue::new(&mut t.y).speed(0.01).clamp_range(0.0..=1.0)).changed() { changed = true; }
                                });
                                ui.end_row();

                                ui.label("Spawn Time:");
                                if ui.add(egui::Slider::new(&mut t.spawn_time, 0.0..=state.duration_secs).suffix("s")).changed() {
                                    changed = true;
                                }
                                ui.end_row();
                            });
                            ui.add_space(12.0);

                            // --- SECTION: CONTENT & BASE STYLE ---
                            ui.label(egui::RichText::new("Content & Base Style").strong());
                            ui.group(|ui| {
                                egui::Grid::new("text_content_grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                                    ui.label("Text:");
                                    if ui.text_edit_singleline(&mut t.value).changed() { changed = true; }
                                    ui.end_row();

                                    ui.label("Font Family:");
                                    let mut selected_font = t.font.clone();
                                    egui::ComboBox::from_id_source(format!("{}_font_combo", t.name))
                                        .selected_text(&selected_font)
                                        .show_ui(ui, |ui| {
                                            for font_name in &state.available_fonts {
                                                let f_fam = egui::FontFamily::Name(font_name.clone().into());
                                                let is_bound = ui.ctx().fonts(|f| f.families().iter().any(|fam| fam == &f_fam));
                                                let text = if is_bound {
                                                    egui::RichText::new(font_name).family(f_fam)
                                                } else {
                                                    egui::RichText::new(font_name)
                                                };
                                                if ui.selectable_value(&mut selected_font, font_name.clone(), text).changed() {
                                                    changed = true;
                                                }
                                            }
                                        });
                                    t.font = selected_font;
                                    ui.end_row();

                                    // Preview Area
                                    ui.label("Preview:");
                                    let preview_fam = if t.font == "System" || t.font.is_empty() {
                                        egui::FontFamily::Proportional
                                    } else {
                                        let f_name = egui::FontFamily::Name(t.font.clone().into());
                                        let is_bound = ui.ctx().fonts(|f| f.families().iter().any(|fam| fam == &f_name));
                                        if is_bound { f_name } else { egui::FontFamily::Proportional }
                                    };
                                    ui.horizontal(|ui| {
                                        ui.painter().rect_filled(ui.available_rect_before_wrap(), 4.0, egui::Color32::from_black_alpha(30));
                                        ui.add_space(10.0);
                                        ui.label(egui::RichText::new("AaBbCc 123").font(egui::FontId::new(24.0, preview_fam)));
                                    });
                                    ui.end_row();

                                    ui.label("Base Size:");
                                    let mut size_pct = t.size * 100.0;
                                    if ui.add(egui::DragValue::new(&mut size_pct).speed(0.1).clamp_range(0.1..=50.0).suffix("%")).changed() {
                                        t.size = size_pct / 100.0;
                                        changed = true;
                                    }
                                    ui.end_row();

                                    ui.label("Base Color:");
                                    if ui.color_edit_button_srgba_unmultiplied(&mut t.color).changed() {
                                        changed = true;
                                    }
                                    ui.end_row();
                                });
                            });
                            ui.add_space(12.0);

                            // --- SECTION: RICH TEXT SPANS ---
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("Rich Text Spans").strong());
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.button(egui::RichText::new("➕").color(egui::Color32::LIGHT_GREEN)).on_hover_text("Add new text span segment").clicked() {
                                        t.spans.push(crate::shapes::text::TextSpan {
                                            text: "New Segment".to_string(),
                                            font: t.font.clone(),
                                            size: t.size,
                                            color: t.color,
                                        });
                                        changed = true;
                                    }
                                });
                            });
                            
                            if t.spans.is_empty() {
                                ui.label(egui::RichText::new("No spans defined. Using base text and style.").small().weak());
                            } else {
                                let mut to_remove = None;
                                let mut to_move_up = None;
                                let mut to_move_down = None;

                                let num_spans = t.spans.len();
                                egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                                    for (i, span) in t.spans.iter_mut().enumerate() {
                                        ui.group(|ui| {
                                            ui.horizontal(|ui| {
                                                ui.label(egui::RichText::new(format!("#{}", i+1)).weak());
                                                if ui.text_edit_singleline(&mut span.text).changed() {
                                                    changed = true;
                                                }
                                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                    if ui.button("🗑").on_hover_text("Delete span").clicked() { to_remove = Some(i); }
                                                    if i < num_spans - 1 {
                                                        if ui.button("🔽").clicked() { to_move_down = Some(i); }
                                                    }
                                                    if i > 0 {
                                                        if ui.button("🔼").clicked() { to_move_up = Some(i); }
                                                    }
                                                });
                                            });
                                            
                                            ui.horizontal(|ui| {
                                                ui.label("Font:");
                                                let mut s_font = span.font.clone();
                                                egui::ComboBox::from_id_source(format!("font_{}_{}", t.name, i))
                                                    .selected_text(&s_font)
                                                    .width(120.0)
                                                    .show_ui(ui, |ui| {
                                                        for f in &state.available_fonts {
                                                            if ui.selectable_value(&mut s_font, f.clone(), f).changed() {
                                                                changed = true;
                                                            }
                                                        }
                                                    });
                                                span.font = s_font;
                                                
                                                ui.label("Size:");
                                                let mut span_pct = span.size * 100.0;
                                                if ui.add(egui::DragValue::new(&mut span_pct).speed(0.1).clamp_range(0.1..=50.0).suffix("%")).changed() {
                                                    span.size = span_pct / 100.0;
                                                    changed = true;
                                                }

                                                ui.label("Color:");
                                                if ui.color_edit_button_srgba_unmultiplied(&mut span.color).changed() {
                                                    changed = true;
                                                }
                                            });
                                        });
                                    }
                                });

                                if let Some(i) = to_remove { t.spans.remove(i); changed = true; }
                                if let Some(i) = to_move_up { t.spans.swap(i, i - 1); changed = true; }
                                if let Some(i) = to_move_down { t.spans.swap(i, i + 1); changed = true; }
                            }

                            ui.add_space(12.0);
                            ui.separator();
                            ui.add_space(12.0);

                            // --- SECTION: ANIMATIONS ---
                            move_animation_element_modifiers::render_move_animation_modifiers(
                                ui,
                                ctx,
                                &mut t.animations,
                                t.spawn_time,
                                state.duration_secs,
                                t.x,
                                t.y,
                                &path_id,
                                &mut changed,
                            );
                        });
                    }
                    Shape::Group {
                        name,
                        children: _,
                        visible,
                    } => {
                        ui.group(|ui| {
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new("📦").size(18.0));
                                    ui.label(
                                        egui::RichText::new("Group Parameters").strong().size(14.0),
                                    );
                                });
                                ui.separator();

                                egui::Grid::new("group_grid")
                                    .num_columns(2)
                                    .spacing([12.0, 8.0])
                                    .show(ui, |ui| {
                                        ui.label("Visible:");
                                        if ui.checkbox(visible, "").changed() {
                                            changed = true;
                                        }
                                        ui.end_row();

                                        ui.label("Name:");
                                        if ui.text_edit_singleline(name).changed() {
                                            changed = true;
                                        }
                                        ui.end_row();

                                        ui.label("Earliest Spawn:");
                                        ui.label(
                                            egui::RichText::new(format!("{:.2}s", earliest_spawn))
                                                .weak(),
                                        );
                                        ui.end_row();
                                    });

                                ui.add_space(4.0);
                            });
                        });
                    }
                }
            } else {
                ui.label("No element found at this path.");
                state.modifier_active_path = None;
            }

            // Persist changes immediately if any
            if changed {
                state.position_cache = None; // invalidate cache
                state.request_dsl_update();
            }
        });
    });

    // If the window was closed by the user, clear the active path
    if !open {
        state.modifier_active_path = None;
    }
}

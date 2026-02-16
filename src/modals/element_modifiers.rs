use crate::app_state::AppState;
use crate::scene::{get_shape_mut, Shape};
use crate::dsl;
use eframe::egui;

pub mod move_animation_element_modifiers;

/// Renders an interactive easing curve editor with draggable control points.
/// Returns true if the easing was modified.
pub(crate) fn render_easing_curve_editor(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    easing: &mut crate::scene::Easing,
    animation_index: usize,
    element_type: &str, // "circle" or "rect"
) -> bool {
    let mut changed = false;
    
    // Parameter editors for standard easings
    match easing {
        crate::scene::Easing::EaseIn { power } | 
        crate::scene::Easing::EaseOut { power } | 
        crate::scene::Easing::EaseInOut { power } => {
            ui.horizontal(|ui| {
                if ui.add(egui::Slider::new(power, 0.1..=5.0).text("Power")).changed() { 
                    changed = true; 
                }
            });
        }
        crate::scene::Easing::Bezier { p1, p2 } => {
            ui.horizontal(|ui| {
                ui.label("P1:");
                if ui.add(egui::DragValue::new(&mut p1.0).speed(0.01).clamp_range(0.0..=1.0)).changed() { changed = true; }
                if ui.add(egui::DragValue::new(&mut p1.1).speed(0.01)).changed() { changed = true; }
                ui.label("P2:");
                if ui.add(egui::DragValue::new(&mut p2.0).speed(0.01).clamp_range(0.0..=1.0)).changed() { changed = true; }
                if ui.add(egui::DragValue::new(&mut p2.1).speed(0.01)).changed() { changed = true; }
            });
        }
        crate::scene::Easing::Custom { .. } => {
            ui.label(egui::RichText::new("Left-click add/drag, Right-click remove").small().color(egui::Color32::GRAY));
        }
        _ => {}
    }

    // UNIFIED GRAPH EDITOR
    let size = egui::vec2(ui.available_width(), 200.0);
    let (response, painter) = ui.allocate_painter(size, egui::Sense::click_and_drag());
    let rect = response.rect;

    // Background & Grid
    painter.rect_filled(rect, 4.0, egui::Color32::from_gray(20));
    painter.rect_stroke(rect, 1.0, egui::Stroke::new(1.0, egui::Color32::from_gray(60)));
    
    let to_screen = |x: f32, y: f32| egui::pos2(
        rect.left() + x * rect.width(),
        rect.bottom() - y * rect.height()
    );
    let from_screen = |pos: egui::Pos2| (
        (pos.x - rect.left()) / rect.width(),
        (rect.bottom() - pos.y) / rect.height()
    );

    // Grid lines
    for i in 1..4 {
        let t = i as f32 / 4.0;
        let x = rect.left() + t * rect.width();
        let y = rect.bottom() - t * rect.height();
        painter.line_segment([egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())], egui::Stroke::new(1.0, egui::Color32::from_gray(40)));
        painter.line_segment([egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)], egui::Stroke::new(1.0, egui::Color32::from_gray(40)));
    }

    // Draw curve
    let mut curve_points = Vec::new();
    for i in 0..=100 {
        let t = i as f32 / 100.0;
        let v = match easing {
            crate::scene::Easing::Linear => t,
            crate::scene::Easing::EaseIn { power } => t.powf(*power),
            crate::scene::Easing::EaseOut { power } => 1.0 - (1.0 - t).powf(*power),
            crate::scene::Easing::EaseInOut { power } => {
                if t < 0.5 { 0.5 * (2.0 * t).powf(*power) } 
                else { 1.0 - 0.5 * (2.0 * (1.0 - t)).powf(*power) }
            },
            crate::scene::Easing::Bezier { p1, p2 } => {
                let u = 1.0 - t;
                3.0 * u * u * t * p1.1 + 3.0 * u * t * t * p2.1 + t * t * t
            },
            crate::scene::Easing::Custom { points } => {
                if points.is_empty() { t }
                else if points.len() == 1 { points[0].1 }
                else {
                    let mut sorted = points.clone();
                    sorted.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
                    if t <= sorted[0].0 { sorted[0].1 }
                    else if t >= sorted[sorted.len()-1].0 { sorted[sorted.len()-1].1 }
                    else {
                        let mut result = t;
                        for i in 0..sorted.len()-1 {
                            if t >= sorted[i].0 && t <= sorted[i+1].0 {
                                let alpha = (t - sorted[i].0) / (sorted[i+1].0 - sorted[i].0);
                                result = sorted[i].1 + alpha * (sorted[i+1].1 - sorted[i].1);
                                break;
                            }
                        }
                        result
                    }
                }
            },
        };
        curve_points.push(to_screen(t, v));
    }
    painter.add(egui::Shape::line(curve_points, egui::Stroke::new(2.0, egui::Color32::LIGHT_BLUE)));

    // INTERACTION & EDITORS
    match easing {
        crate::scene::Easing::Bezier { p1, p2 } => {
            // Bezier Handles
            let start = to_screen(0.0, 0.0);
            let end = to_screen(1.0, 1.0);
            let cp1 = to_screen(p1.0, p1.1);
            let cp2 = to_screen(p2.0, p2.1);
            
            painter.line_segment([start, cp1], egui::Stroke::new(1.0, egui::Color32::GRAY));
            painter.line_segment([end, cp2], egui::Stroke::new(1.0, egui::Color32::GRAY));
            painter.circle_filled(cp1, 4.0, egui::Color32::YELLOW);
            painter.circle_filled(cp2, 4.0, egui::Color32::YELLOW);

            let drag_id = ui.make_persistent_id(format!("bezier_drag_{}_{}", element_type, animation_index));
            let mut dragging: Option<usize> = ui.data(|d| d.get_temp(drag_id));
            let pointer_pos = ui.input(|i| i.pointer.hover_pos());
            let pointer_down = ui.input(|i| i.pointer.primary_down());
            let was_down_id = ui.make_persistent_id(format!("bezier_was_down_{}_{}", element_type, animation_index));
            let was_down = ui.data(|d| d.get_temp::<bool>(was_down_id)).unwrap_or(false);

            // Detectar inicio de drag
            if pointer_down && !was_down && dragging.is_none() {
                if let Some(pos) = pointer_pos {
                    if rect.contains(pos) {
                        if pos.distance(cp1) < 10.0 { dragging = Some(1); }
                        else if pos.distance(cp2) < 10.0 { dragging = Some(2); }
                        if dragging.is_some() {
                            ui.data_mut(|d| d.insert_temp(drag_id, dragging));
                        }
                    }
                }
            }
            
            // Actualizar durante el drag
            if let Some(idx) = dragging {
                if pointer_down {
                    if let Some(pos) = pointer_pos {
                        let (nx, ny) = from_screen(pos);
                        let new_val = (nx.clamp(0.0, 1.0), ny.clamp(-0.5, 1.5));
                        if idx == 1 { *p1 = new_val; } else { *p2 = new_val; }
                        ctx.request_repaint();
                    }
                } else {
                    // Mouse soltado
                    ui.data_mut(|d| d.remove::<Option<usize>>(drag_id));
                    changed = true;
                }
            }
            
            // Guardar estado del mouse
            ui.data_mut(|d| d.insert_temp(was_down_id, pointer_down));
        }
        crate::scene::Easing::Custom { points } => {
            // Points Editor
            let drag_id = ui.make_persistent_id(format!("custom_drag_{}_{}", element_type, animation_index));
            let mut dragging: Option<usize> = ui.data(|d| d.get_temp(drag_id));
            let pointer_pos = ui.input(|i| i.pointer.hover_pos());
            let pointer_down = ui.input(|i| i.pointer.primary_down());
            let was_down_id = ui.make_persistent_id(format!("custom_was_down_{}_{}", element_type, animation_index));
            let was_down = ui.data(|d| d.get_temp::<bool>(was_down_id)).unwrap_or(false);
            
            // Draw Points
            for (_idx, p) in points.iter().enumerate() {
                painter.circle_filled(to_screen(p.0, p.1), 5.0, egui::Color32::YELLOW);
            }

            // Detectar inicio de drag
            if pointer_down && !was_down && dragging.is_none() {
                if let Some(pos) = pointer_pos {
                    if rect.contains(pos) {
                        let mut best_dist = f32::MAX;
                        let mut best = None;
                        for (pt_idx, p) in points.iter().enumerate() {
                            let d = to_screen(p.0, p.1).distance(pos);
                            if d < 10.0 && d < best_dist { best_dist = d; best = Some(pt_idx); }
                        }
                        if let Some(pt_idx) = best { 
                            dragging = Some(pt_idx); 
                            ui.data_mut(|d| d.insert_temp(drag_id, dragging)); 
                        }
                    }
                }
            }
            
            // Actualizar durante el drag
            if let Some(idx) = dragging {
                if pointer_down {
                    if let Some(pos) = pointer_pos {
                        let (nx, ny) = from_screen(pos);
                        points[idx] = (nx.clamp(0.0, 1.0), ny.clamp(0.0, 1.0));
                        ctx.request_repaint();
                    }
                } else {
                    // Mouse soltado
                    ui.data_mut(|d| d.remove::<Option<usize>>(drag_id));
                    points.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
                    changed = true;
                }
            }
            
            // Guardar estado del mouse
            ui.data_mut(|d| d.insert_temp(was_down_id, pointer_down));
        }
        _ => {}
    }
    
    changed
}

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
        .collapsible(false)
        .default_width(default_w)
        .default_height(default_h)
        .default_pos(default_pos)
        .movable(true);

    window.show(ctx, |ui| {
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
                .map(|p| p.iter().map(|n| n.to_string()).collect::<Vec<_>>().join("-"))
                .unwrap_or_else(|| "root".to_string());

            match shape {
                                Shape::Circle {
                                    name,
                                    x,
                                    y,
                                    radius,
                                    color,
                                    spawn_time,
                                    visible,
                                    animations,
                                    ..
                                } => {
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
                                                    if ui.text_edit_singleline(name).changed() {
                                                        changed = true;
                                                    }
                                                    ui.end_row();

                                                    ui.label("Visible:");
                                                    if ui.checkbox(visible, "").changed() {
                                                        changed = true;
                                                    }
                                                    ui.end_row();

                                                    ui.label("Spawn Time:");
                                                    if ui
                                                        .add(
                                                            egui::Slider::new(
                                                                spawn_time,
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
                                                    let mut val_x = *x * 100.0;
                                                    if ui
                                                        .add(
                                                            egui::Slider::new(&mut val_x, 0.0..=100.0)
                                                                .suffix("%")
                                                                .clamp_to_range(false),
                                                        )
                                                        .changed()
                                                    {
                                                        *x = val_x / 100.0;
                                                        changed = true;
                                                    }
                                                    ui.end_row();

                                                    ui.label("Position Y:");
                                                    let mut val_y = *y * 100.0;
                                                    if ui
                                                        .add(
                                                            egui::Slider::new(&mut val_y, 0.0..=100.0)
                                                                .suffix("%")
                                                                .clamp_to_range(false),
                                                        )
                                                        .changed()
                                                    {
                                                        *y = val_y / 100.0;
                                                        changed = true;
                                                    }
                                                    ui.end_row();

                                                    ui.label("Radius:");
                                                    let mut val_r = *radius * 100.0;
                                                    if ui
                                                        .add(
                                                            egui::Slider::new(&mut val_r, 0.0..=100.0)
                                                                .suffix("%")
                                                                .clamp_to_range(false),
                                                        )
                                                        .changed()
                                                    {
                                                        *radius = val_r / 100.0;
                                                        changed = true;
                                                    }
                                                    ui.end_row();

                                                    ui.label("Color:");
                                                    if ui.color_edit_button_srgba_unmultiplied(color).changed()
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
                                                animations,
                                                *spawn_time,
                                                state.duration_secs,
                                                *x,
                                                *y,
                                                &path_id,
                                                &mut changed,
                                            );
                                        });
                                    });
                                }
                                Shape::Rect {
                                    name,
                                    x,
                                    y,
                                    w,
                                    h,
                                    color,
                                    spawn_time,
                                    visible,
                                    animations,
                                    ..
                                } => {
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
                                                    if ui.text_edit_singleline(name).changed() {
                                                        changed = true;
                                                    }
                                                    ui.end_row();

                                                    ui.label("Visible:");
                                                    if ui.checkbox(visible, "").changed() {
                                                        changed = true;
                                                    }
                                                    ui.end_row();

                                                    ui.label("Spawn Time:");
                                                    if ui
                                                        .add(
                                                            egui::Slider::new(
                                                                spawn_time,
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
                                                    let mut val_x = *x * 100.0;
                                                    if ui
                                                        .add(
                                                            egui::Slider::new(&mut val_x, 0.0..=100.0)
                                                                .suffix("%")
                                                                .clamp_to_range(false),
                                                        )
                                                        .changed()
                                                    {
                                                        *x = val_x / 100.0;
                                                        changed = true;
                                                    }
                                                    ui.end_row();

                                                    ui.label("Position Y:");
                                                    let mut val_y = *y * 100.0;
                                                    if ui
                                                        .add(
                                                            egui::Slider::new(&mut val_y, 0.0..=100.0)
                                                                .suffix("%")
                                                                .clamp_to_range(false),
                                                        )
                                                        .changed()
                                                    {
                                                        *y = val_y / 100.0;
                                                        changed = true;
                                                    }
                                                    ui.end_row();

                                                    ui.label("Width:");
                                                    let mut val_w = *w * 100.0;
                                                    if ui
                                                        .add(
                                                            egui::Slider::new(&mut val_w, 0.0..=100.0)
                                                                .suffix("%")
                                                                .clamp_to_range(false),
                                                        )
                                                        .changed()
                                                    {
                                                        *w = val_w / 100.0;
                                                        changed = true;
                                                    }
                                                    ui.end_row();

                                                    ui.label("Height:");
                                                    let mut val_h = *h * 100.0;
                                                    if ui
                                                        .add(
                                                            egui::Slider::new(&mut val_h, 0.0..=100.0)
                                                                .suffix("%")
                                                                .clamp_to_range(false),
                                                        )
                                                        .changed()
                                                    {
                                                        *h = val_h / 100.0;
                                                        changed = true;
                                                    }
                                                    ui.end_row();

                                                    ui.label("Color:");
                                                    if ui.color_edit_button_srgba_unmultiplied(color).changed()
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
                                                    animations,
                                                    *spawn_time,
                                                    state.duration_secs,
                                                    *x,
                                                    *y,
                                                    &path_id,
                                                    &mut changed,
                                                );
                                        });
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
                                                    egui::RichText::new("Group Parameters")
                                                        .strong()
                                                        .size(14.0),
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
                state.dsl_code = dsl::generate_dsl(
                    &state.scene,
                    state.render_width,
                    state.render_height,
                    state.fps,
                    state.duration_secs,
                );
                crate::events::element_properties_changed_event::on_element_properties_changed(state);
            }
        
    });

    // If the window was closed by the user, clear the active path
    if !open {
        state.modifier_active_path = None;
    }
}

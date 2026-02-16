use crate::app_state::AppState;
use crate::scene::{get_shape_mut, Shape};
use crate::dsl;
use eframe::egui;

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
                                            // --- Animations (Move) ---
                                            ui.collapsing("Animations", |ui| {
                                                // Allow adding a new Move animation
                                                ui.horizontal(|ui| {
                                                    // Add Move button with descriptive tooltip
                                                    if ui
                                                        .add(egui::Button::new("+ Add Move"))
                                                        .on_hover_text("Add a Move animation to this element.\n\nCreates a position animation that interpolates the element from its current position at Start toward (To X, To Y) over [Start, End]. Defaults: Start = element spawn (or 0), End = project duration, Easing = linear.")
                                                        .clicked()
                                                    {
                                                        // default: start at element spawn (or 0), end at project end
                                                        let start = (*spawn_time).max(0.0);
                                                        let end = state.duration_secs;
                                                        let to_x = (*x + 0.20).min(1.0);
                                                        let to_y = *y;
                                                        animations.push(crate::scene::Animation::Move {
                                                            to_x,
                                                            to_y,
                                                            start,
                                                            end,
                                                            easing: crate::scene::Easing::Linear,
                                                        });
                                                        changed = true;
                                                    }

                                                    // tooltip describing the Move animation and available easing types (info icon)
                                                    ui.add_space(6.0);
                                                        ui.label(egui::RichText::new("ⓘ").weak()).on_hover_text(
                                                        "Move animation — moves an element from its position at the animation Start to the specified target (To X, To Y) over [Start, End].\n\nBehavior:\n• Before Start: element stays at its base position.\n• During: interpolates from the element's position at Start toward the target.\n• After End: element remains at the target.\n\nParameters:\n• Start / End (seconds), To X / To Y (0.0..1.0).\n• Easing: `linear` = constant speed; `ease_in_out(power)` = symmetric ease-in/out (power controls curvature; 1.0 = linear).\n\nDSL example: `type = ease_in_out(power = 2.0)`.",
                                                    );
                                                });

                                                // List existing Move animations
                                                if animations.is_empty() {
                                                    ui.label("No animations");
                                                } else {
                                                    // iterate by index so we can remove safely after
                                                    let mut remove_idx: Option<usize> = None;

                                                    for i in 0..animations.len() {
                                                        // only show Move animations here
                                                        if let crate::scene::Animation::Move {
                                                            to_x,
                                                            to_y,
                                                            start,
                                                            end,
                                                            easing,
                                                        } = &mut animations[i]
                                                        {
                                                            let header_text = format!("Move #{} — {:.2}s → {:.2}s", i + 1, *start, *end);
                                                            let stable_id = format!("element_modifiers::{}::move::{}", path_id, i);
                                                            egui::CollapsingHeader::new(header_text)
                                                                .id_source(stable_id)
                                                                .show(ui, |ui| {
                                                                    ui.horizontal(|ui| {
                                                                        if ui.add(egui::Button::new("Remove").small()).clicked() {
                                                                            remove_idx = Some(i);
                                                                        }
                                                                           ui.add_space(6.0);
                                                                           ui.label(egui::RichText::new("ⓘ").weak()).on_hover_text(
                                                                               "Move animation — moves the element toward `To X, To Y` between `Start` and `End`.\n\nMultiple Move animations are applied in chronological order; each animation interpolates from the element's position at that animation's Start.\n\nEasing options: `linear` (constant speed) or `ease_in_out(power)` (symmetric ease-in/out). Use the `power` slider to control the curve (default 1.0).",
                                                                           );
                                                                    });

                                                                    ui.add_space(4.0);

                                                                    // Start / End times
                                                                    ui.horizontal(|ui| {
                                                                        ui.label("Start:");
                                                                        let mut s = *start;
                                                                        if ui.add(egui::Slider::new(&mut s, *spawn_time..=state.duration_secs).suffix("s")).changed() {
                                                                            *start = s.max(*spawn_time);
                                                                            // ensure end is not before start
                                                                            if *end < *start {
                                                                                *end = *start;
                                                                            }
                                                                            changed = true;
                                                                        }

                                                                        ui.label("End:");
                                                                        let mut e = *end;
                                                                        if ui.add(egui::Slider::new(&mut e, *start..=state.duration_secs).suffix("s")).changed() {
                                                                            *end = e.max(*start);
                                                                            changed = true;
                                                                        }
                                                                    });

                                                                    ui.add_space(4.0);

                                                                    // Target position (percent)
                                                                    ui.horizontal(|ui| {
                                                                        ui.label("To X:");
                                                                        let mut tx = *to_x * 100.0;
                                                                        if ui.add(egui::Slider::new(&mut tx, 0.0..=100.0).suffix("%")).changed() {
                                                                            *to_x = (tx / 100.0).clamp(0.0, 1.0);
                                                                            changed = true;
                                                                        }

                                                                        ui.label("To Y:");
                                                                        let mut ty = *to_y * 100.0;
                                                                        if ui.add(egui::Slider::new(&mut ty, 0.0..=100.0).suffix("%")).changed() {
                                                                            *to_y = (ty / 100.0).clamp(0.0, 1.0);
                                                                            changed = true;
                                                                        }
                                                                    });

                                                                    ui.add_space(4.0);

                                                                    // Easing selector (for now only Linear exists)
                                                                    ui.horizontal(|ui| {
                                                                        ui.label("Easing:");
                                                                        egui::ComboBox::from_label("")
                                                                            .selected_text(format!("{:?}", easing))
                                                                            .show_ui(ui, |ui| {
                                                                                if ui.selectable_label(matches!(easing, crate::scene::Easing::Linear), "Linear").on_hover_text("Linear — constant speed (uniform velocity)").clicked() {
                                                                                    *easing = crate::scene::Easing::Linear;
                                                                                    changed = true;
                                                                                }

                                                                                // `Lerp` removed — use `EaseInOut` instead (symmetric power curve)

                                                                                if ui.selectable_label(matches!(easing, crate::scene::Easing::EaseIn { .. }), "EaseIn").on_hover_text("EaseIn(power) — accelerate from zero; progress = t^power").clicked() {
                                                                                    *easing = crate::scene::Easing::EaseIn { power: 1.0 };
                                                                                    changed = true;
                                                                                }

                                                                                if ui.selectable_label(matches!(easing, crate::scene::Easing::EaseOut { .. }), "EaseOut").on_hover_text("EaseOut(power) — decelerate to stop; progress = 1 - (1-t)^power").clicked() {
                                                                                    *easing = crate::scene::Easing::EaseOut { power: 1.0 };
                                                                                    changed = true;
                                                                                }

                                                                                if ui.selectable_label(matches!(easing, crate::scene::Easing::EaseInOut { .. }), "EaseInOut").on_hover_text("EaseInOut(power) — symmetric ease-in/out (use for smooth start+end)").clicked() {
                                                                                    *easing = crate::scene::Easing::EaseInOut { power: 1.0 };
                                                                                    changed = true;
                                                                                }

                                                                                if ui.selectable_label(matches!(easing, crate::scene::Easing::Custom { .. }), "Custom").on_hover_text("Custom — define your own curve by adding points").clicked() {
                                                                                    *easing = crate::scene::Easing::Custom { points: vec![(0.0, 0.0), (1.0, 1.0)] };
                                                                                    changed = true;
                                                                                }
                                                                                if ui.selectable_label(matches!(easing, crate::scene::Easing::Bezier { .. }), "Bezier").on_hover_text("Bezier — smooth curve with 2 control points").clicked() {
                                                                                    // Default ease-in-out like
                                                                                    *easing = crate::scene::Easing::Bezier { p1: (0.42, 0.0), p2: (0.58, 1.0) };
                                                                                    changed = true;
                                                                                }
                                                                            });
                                                                        }); // Close the horizontal layout that contains the label and ComboBox

                                                                        // If a `power`-based easing is selected expose the `power` parameter
                                                                        ui.label("Easing Curve:");
                                                                        
                                                                        // Parameter sliders for standard easings
                                                                        match easing {
                                                                            crate::scene::Easing::EaseIn { power } | 
                                                                            crate::scene::Easing::EaseOut { power } | 
                                                                            crate::scene::Easing::EaseInOut { power } => {
                                                                                ui.horizontal(|ui| {
                                                                                    if ui.add(egui::Slider::new(power, 0.1..=5.0).text("Power")).changed() { changed = true; }
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

                                                                        // Draw Grid
                                                                        for i in 1..4 {
                                                                            let t = i as f32 / 4.0;
                                                                            let x = rect.left() + t * rect.width();
                                                                            let y = rect.bottom() - t * rect.height();
                                                                            painter.line_segment([egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())], egui::Stroke::new(1.0, egui::Color32::from_gray(40)));
                                                                            painter.line_segment([egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)], egui::Stroke::new(1.0, egui::Color32::from_gray(40)));
                                                                        }
                                                                        painter.text(rect.right_bottom() + egui::vec2(-4.0, -12.0), egui::Align2::RIGHT_BOTTOM, "Time", egui::FontId::proportional(10.0), egui::Color32::GRAY);
                                                                        painter.text(rect.left_top() + egui::vec2(6.0, 4.0), egui::Align2::LEFT_TOP, "Value", egui::FontId::proportional(10.0), egui::Color32::GRAY);

                                                                        // Draw Current Easing Curve
                                                                        let steps = 64;
                                                                        let mut curve_points = Vec::with_capacity(steps + 1);
                                                                        match easing {
                                                                            crate::scene::Easing::Linear => {
                                                                                curve_points.push(to_screen(0.0, 0.0));
                                                                                curve_points.push(to_screen(1.0, 1.0));
                                                                            }
                                                                            crate::scene::Easing::Custom { points } => {
                                                                                let mut sorted = points.clone();
                                                                                sorted.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
                                                                                for p in sorted {
                                                                                    curve_points.push(to_screen(p.0, p.1));
                                                                                }
                                                                            }
                                                                            crate::scene::Easing::Bezier { p1, p2 } => {
                                                                                for i in 0..=steps {
                                                                                    let t = i as f32 / steps as f32;
                                                                                    let u = 1.0 - t;
                                                                                    // Cubic Bezier Parametric
                                                                                    let cx = 3.0*u*u*t*p1.0 + 3.0*u*t*t*p2.0 + t*t*t;
                                                                                    let cy = 3.0*u*u*t*p1.1 + 3.0*u*t*t*p2.1 + t*t*t;
                                                                                    curve_points.push(to_screen(cx, cy));
                                                                                }
                                                                            }
                                                                            _ => {
                                                                                // Sample strict y(x) function
                                                                                for i in 0..=steps {
                                                                                    let t = i as f32 / steps as f32;
                                                                                    let v = match easing {
                                                                                        crate::scene::Easing::EaseIn { power } => t.powf(*power),
                                                                                        crate::scene::Easing::EaseOut { power } => 1.0 - (1.0 - t).powf(*power),
                                                                                        crate::scene::Easing::EaseInOut { power } => {
                                                                                            if t < 0.5 { 0.5 * (2.0 * t).powf(*power) } 
                                                                                            else { 1.0 - 0.5 * (2.0 * (1.0 - t)).powf(*power) }
                                                                                        },
                                                                                        _ => t // Fallback
                                                                                    };
                                                                                    curve_points.push(to_screen(t, v));
                                                                                }
                                                                            }
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

                                                                                let drag_id = ui.make_persistent_id("bezier_drag");
                                                                                let mut dragging: Option<usize> = ui.data(|d| d.get_temp(drag_id));

                                                                                if response.drag_started() {
                                                                                    if let Some(pos) = response.interact_pointer_pos() {
                                                                                        if pos.distance(cp1) < 10.0 { dragging = Some(1); }
                                                                                        else if pos.distance(cp2) < 10.0 { dragging = Some(2); }
                                                                                        ui.data_mut(|d| d.insert_temp(drag_id, dragging));
                                                                                    }
                                                                                }
                                                                                if let Some(idx) = dragging {
                                                                                    if let Some(pos) = response.interact_pointer_pos() {
                                                                                        let (nx, ny) = from_screen(pos);
                                                                                        let new_val = (nx.clamp(0.0, 1.0), ny.clamp(-0.5, 1.5));
                                                                                        if idx == 1 { *p1 = new_val; } else { *p2 = new_val; }
                                                                                        changed = true;
                                                                                    }
                                                                                    if response.drag_released() { ui.data_mut(|d| d.remove::<Option<usize>>(drag_id)); }
                                                                                }
                                                                            }
                                                                            crate::scene::Easing::Custom { points } => {
                                                                                // Points Editor
                                                                                let drag_id = ui.make_persistent_id("custom_drag");
                                                                                let mut dragging: Option<usize> = ui.data(|d| d.get_temp(drag_id));
                                                                                
                                                                                // Draw Points
                                                                                for (i, p) in points.iter().enumerate() {
                                                                                    painter.circle_filled(to_screen(p.0, p.1), 5.0, egui::Color32::YELLOW);
                                                                                }

                                                                                // Drag Logic
                                                                                if response.drag_started() {
                                                                                    if let Some(pos) = response.interact_pointer_pos() {
                                                                                        let mut best_dist = f32::MAX;
                                                                                        let mut best = None;
                                                                                        for (i, p) in points.iter().enumerate() {
                                                                                            let d = to_screen(p.0, p.1).distance(pos);
                                                                                            if d < 10.0 && d < best_dist { best_dist = d; best = Some(i); }
                                                                                        }
                                                                                        if let Some(i) = best { dragging = Some(i); ui.data_mut(|d| d.insert_temp(drag_id, dragging)); }
                                                                                    }
                                                                                }
                                                                                if let Some(idx) = dragging {
                                                                                    if let Some(pos) = response.interact_pointer_pos() {
                                                                                        let (nx, ny) = from_screen(pos);
                                                                                        points[idx] = (nx.clamp(0.0, 1.0), ny.clamp(0.0, 1.0));
                                                                                        changed = true;
                                                                                    }
                                                                                    if response.drag_released() { 
                                                                                        ui.data_mut(|d| d.remove::<Option<usize>>(drag_id)); 
                                                                                        points.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
                                                                                    }
                                                                                }
                                                                                // Add/Remove Logic
                                                                                if response.clicked() && dragging.is_none() {
                                                                                    if let Some(pos) = response.interact_pointer_pos() {
                                                                                        // Check close to point for remove?
                                                                                        let mut clicked_pt = None;
                                                                                        for (i, p) in points.iter().enumerate() {
                                                                                            if to_screen(p.0, p.1).distance(pos) < 10.0 { clicked_pt = Some(i); break; }
                                                                                        }
                                                                                        
                                                                                        if response.secondary_clicked() {
                                                                                            if let Some(i) = clicked_pt { 
                                                                                                if points.len() > 2 { points.remove(i); changed = true; }
                                                                                            }
                                                                                        } else if response.clicked_by(egui::PointerButton::Primary) {
                                                                                            if clicked_pt.is_none() {
                                                                                                let new_p = from_screen(pos);
                                                                                                points.push((new_p.0.clamp(0.0, 1.0), new_p.1.clamp(0.0, 1.0)));
                                                                                                points.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
                                                                                                changed = true;
                                                                                            }
                                                                                        }
                                                                                    }
                                                                                }
                                                                            }
                                                                            _ => {
                                                                                // Auto-convert to Custom on interaction
                                                                                if response.clicked() || response.drag_started() {
                                                                                     // Sample current curve to points
                                                                                     let mut new_points = Vec::new();
                                                                                     for i in 0..=4 {
                                                                                        let t = i as f32 / 4.0;
                                                                                        let v = match easing {
                                                                                             crate::scene::Easing::EaseIn { power } => t.powf(*power),
                                                                                             crate::scene::Easing::EaseOut { power } => 1.0 - (1.0 - t).powf(*power),
                                                                                             crate::scene::Easing::EaseInOut { power } => {
                                                                                                 if t < 0.5 { 0.5 * (2.0 * t).powf(*power) } 
                                                                                                 else { 1.0 - 0.5 * (2.0 * (1.0 - t)).powf(*power) }
                                                                                             },
                                                                                             _ => t
                                                                                         };
                                                                                         new_points.push((t, v));
                                                                                     }
                                                                                     // If clicked position is new, add it too? 
                                                                                     // Actually, just convert first, then let next frame handle drag/add.
                                                                                     // Or try to add point right away.
                                                                                     *easing = crate::scene::Easing::Custom { points: new_points };
                                                                                     changed = true;
                                                                                }
                                                                            }
                                                                        }
                                                                    });

                                                            ui.add_space(6.0);
                                                        }
                                                    }

                                                    // perform removal after iteration to avoid borrow issues
                                                    if let Some(idx) = remove_idx {
                                                        animations.remove(idx);
                                                        changed = true;
                                                    }
                                                }
                                            });
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

                                                // --- Animations (Move) ---
                                                ui.collapsing("Animations", |ui| {
                                                    ui.horizontal(|ui| {
                                                        if ui.button("+ Add Move").clicked() {
                                                                let start = (*spawn_time).max(0.0);
                                                            let end = state.duration_secs;
                                                            let to_x = (*x + 0.20).min(1.0);
                                                            let to_y = *y;
                                                            animations.push(crate::scene::Animation::Move {
                                                                to_x,
                                                                to_y,
                                                                start,
                                                                end,
                                                                easing: crate::scene::Easing::Linear,
                                                            });
                                                            changed = true;
                                                        }
                                                        // tooltip for the button
                                                        ui.add_space(6.0);
                                                        ui.label(egui::RichText::new("ⓘ").weak()).on_hover_text(
                                                            "Add Move — create a position animation for the selected element.\n\nDefaults: Start = element spawn or 0; End = project duration; Easing = linear.\n\nAfter adding, adjust Start/End, To X/To Y and select Easing (linear or ease_in_out(power)). Note: animation Start must be >= element spawn time.",
                                                        );
                                                    });

                                                    if animations.is_empty() {
                                                        ui.label("No animations");
                                                    } else {
                                                        let mut remove_idx: Option<usize> = None;
                                                        for i in 0..animations.len() {
                                                            if let crate::scene::Animation::Move {
                                                                to_x,
                                                                to_y,
                                                                start,
                                                                end,
                                                                easing,
                                                            } = &mut animations[i]
                                                            {
                                                                let header_text = format!("Move #{} — {:.2}s → {:.2}s", i + 1, *start, *end);
                                                                let stable_id = format!("element_modifiers::{}::move::{}", path_id, i);
                                                                egui::CollapsingHeader::new(header_text)
                                                                    .id_source(stable_id)
                                                                    .show(ui, |ui| {
                                                                        ui.horizontal(|ui| {
                                                                            if ui.add(egui::Button::new("Remove").small()).clicked() {
                                                                                remove_idx = Some(i);
                                                                            }
                                                                        });

                                                                        ui.add_space(4.0);

                                                                        ui.horizontal(|ui| {
                                                                            ui.label("Start:");
                                                                            let mut s = *start;
                                                                            if ui.add(egui::Slider::new(&mut s, *spawn_time..=state.duration_secs).suffix("s")).changed() {
                                                                                *start = s.max(*spawn_time);
                                                                                if *end < *start {
                                                                                    *end = *start;
                                                                                }
                                                                                changed = true;
                                                                            }

                                                                            ui.label("End:");
                                                                            let mut e = *end;
                                                                            if ui.add(egui::Slider::new(&mut e, *start..=state.duration_secs).suffix("s")).changed() {
                                                                                *end = e.max(*start);
                                                                                changed = true;
                                                                            }
                                                                        });

                                                                        ui.add_space(4.0);

                                                                        ui.horizontal(|ui| {
                                                                            ui.label("To X:");
                                                                            let mut tx = *to_x * 100.0;
                                                                            if ui.add(egui::Slider::new(&mut tx, 0.0..=100.0).suffix("%")).changed() {
                                                                                *to_x = (tx / 100.0).clamp(0.0, 1.0);
                                                                                changed = true;
                                                                            }

                                                                            ui.label("To Y:");
                                                                            let mut ty = *to_y * 100.0;
                                                                            if ui.add(egui::Slider::new(&mut ty, 0.0..=100.0).suffix("%")).changed() {
                                                                                *to_y = (ty / 100.0).clamp(0.0, 1.0);
                                                                                changed = true;
                                                                            }
                                                                        });

                                                                        ui.add_space(4.0);

                                                                        ui.horizontal(|ui| {
                                                                            ui.label("Easing:");
                                                                            egui::ComboBox::from_label("")
                                                                                .selected_text(format!("{:?}", easing))
                                                                                .show_ui(ui, |ui| {
                                                                                    if ui.selectable_label(matches!(easing, crate::scene::Easing::Linear), "Linear").on_hover_text("Linear — constant speed interpolation (uniform velocity)").clicked() {
                                                                                        *easing = crate::scene::Easing::Linear;
                                                                                        changed = true;
                                                                                    }
                                                                                    // `Lerp` option removed; use `EaseInOut(power)` for symmetric easing.
                                                                                    if ui.selectable_label(matches!(easing, crate::scene::Easing::EaseIn { .. }), "EaseIn").on_hover_text("EaseIn(power) — accelerate from zero; progress = t^power").clicked() {
                                                                                        *easing = crate::scene::Easing::EaseIn { power: 1.0 };
                                                                                        changed = true;
                                                                                    }
                                                                                    if ui.selectable_label(matches!(easing, crate::scene::Easing::EaseOut { .. }), "EaseOut").on_hover_text("EaseOut(power) — decelerate to zero; progress = 1 - (1-t)^power").clicked() {
                                                                                        *easing = crate::scene::Easing::EaseOut { power: 1.0 };
                                                                                        changed = true;
                                                                                    }
                                                                                    if ui.selectable_label(matches!(easing, crate::scene::Easing::EaseInOut { .. }), "EaseInOut").on_hover_text("EaseInOut(power) — symmetric ease-in/out").clicked() {
                                                                                        *easing = crate::scene::Easing::EaseInOut { power: 1.0 };
                                                                                        changed = true;
                                                                                    }

                                                                                    if ui.selectable_label(matches!(easing, crate::scene::Easing::Custom { .. }), "Custom").on_hover_text("Custom — define your own curve by adding points").clicked() {
                                                                                        // Initialize with just start and end points
                                                                                        *easing = crate::scene::Easing::Custom { points: vec![(0.0, 0.0), (1.0, 1.0)] };
                                                                                        changed = true;
                                                                                    }
                                                                                    if ui.selectable_label(matches!(easing, crate::scene::Easing::Bezier { .. }), "Bezier").on_hover_text("Bezier — smooth curve with 2 control points").clicked() {
                                                                                        // Default ease-in-out like
                                                                                        *easing = crate::scene::Easing::Bezier { p1: (0.42, 0.0), p2: (0.58, 1.0) };
                                                                                        changed = true;
                                                                                    }
                                                                                });
                                                                            }); // Close the horizontal layout that contains the label and ComboBox

                                                                            // If a `power`-based easing is selected expose the `power` parameter
                                                                            ui.label(""); // Spacing
                                                                            
                                                                            // --- UNIFIED GRAPH VISUALIZATION ---
                                                                            let graph_size = egui::vec2(ui.available_width(), 160.0);
                                                                            let (response, painter) = ui.allocate_painter(graph_size, egui::Sense::click_and_drag());
                                                                            let rect = response.rect;

                                                                            // Background
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

                                                                            // Grid
                                                                            for i in 1..4 {
                                                                                let t = i as f32 / 4.0;
                                                                                let x = rect.left() + t * rect.width();
                                                                                let y = rect.bottom() - t * rect.height();
                                                                                painter.line_segment([egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())], egui::Stroke::new(1.0, egui::Color32::from_gray(40)));
                                                                                painter.line_segment([egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)], egui::Stroke::new(1.0, egui::Color32::from_gray(40)));
                                                                            }
                                                                            painter.text(rect.right_bottom() + egui::vec2(-4.0, -12.0), egui::Align2::RIGHT_BOTTOM, "Time", egui::FontId::proportional(10.0), egui::Color32::GRAY);
                                                                            
                                                                            // Draw and Interact based on Type
                                                                            match easing {
                                                                                crate::scene::Easing::Bezier { p1, p2 } => {
                                                                                    // Handles
                                                                                    let start = to_screen(0.0, 0.0);
                                                                                    let end = to_screen(1.0, 1.0);
                                                                                    let cp1 = to_screen(p1.0, p1.1);
                                                                                    let cp2 = to_screen(p2.0, p2.1);
                                                                                    
                                                                                    painter.line_segment([start, cp1], egui::Stroke::new(1.0, egui::Color32::GRAY));
                                                                                    painter.line_segment([end, cp2], egui::Stroke::new(1.0, egui::Color32::GRAY));
                                                                                    
                                                                                    // Interaction
                                                                                    let drag_id = ui.make_persistent_id("bezier_drag");
                                                                                    let mut dragging: Option<usize> = ui.data(|d| d.get_temp(drag_id));
                                                                                    if response.drag_started() {
                                                                                        if let Some(pos) = response.interact_pointer_pos() {
                                                                                            if pos.distance(cp1) < 10.0 { dragging = Some(1); }
                                                                                            else if pos.distance(cp2) < 10.0 { dragging = Some(2); }
                                                                                            if dragging.is_some() { ui.data_mut(|d| d.insert_temp(drag_id, dragging)); }
                                                                                        }
                                                                                    }
                                                                                    if let Some(idx) = dragging {
                                                                                        if let Some(pos) = response.interact_pointer_pos() {
                                                                                            let (nx, ny) = from_screen(pos);
                                                                                            let new_val = (nx.clamp(0.0, 1.0), ny.clamp(-0.5, 1.5));
                                                                                            if idx == 1 { *p1 = new_val; } else { *p2 = new_val; }
                                                                                            changed = true;
                                                                                        }
                                                                                        if response.drag_released() { ui.data_mut(|d| d.remove::<Option<usize>>(drag_id)); }
                                                                                    }

                                                                                    // Draw Points
                                                                                    painter.circle_filled(cp1, 4.0, egui::Color32::YELLOW);
                                                                                    painter.circle_filled(cp2, 4.0, egui::Color32::YELLOW);

                                                                                    // Draw Curve
                                                                                    let steps = 64;
                                                                                    let mut curve = Vec::with_capacity(steps);
                                                                                    for i in 0..=steps {
                                                                                        let t = i as f32 / steps as f32;
                                                                                        let u = 1.0 - t;
                                                                                        let cx = 3.0*u*u*t*p1.0 + 3.0*u*t*t*p2.0 + t*t*t;
                                                                                        let cy = 3.0*u*u*t*p1.1 + 3.0*u*t*t*p2.1 + t*t*t;
                                                                                        curve.push(to_screen(cx, cy));
                                                                                    }
                                                                                    painter.add(egui::Shape::line(curve, egui::Stroke::new(2.0, egui::Color32::LIGHT_BLUE)));
                                                                                }
                                                                                crate::scene::Easing::Custom { points } => {
                                                                                    // Draw Points
                                                                                    for p in points.iter() {
                                                                                        painter.circle_filled(to_screen(p.0, p.1), 4.0, egui::Color32::YELLOW);
                                                                                    }
                                                                                    // Draw Lines
                                                                                    if points.len() >= 2 {
                                                                                        let mut sorted = points.clone();
                                                                                        sorted.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
                                                                                        let line_pts: Vec<egui::Pos2> = sorted.iter().map(|p| to_screen(p.0, p.1)).collect();
                                                                                        painter.add(egui::Shape::line(line_pts, egui::Stroke::new(2.0, egui::Color32::LIGHT_BLUE)));
                                                                                    }
                                                                                    
                                                                                    // Interaction Logic (Simplified for brevity as it was complex, but let's re-add basic drag)
                                                                                    let drag_id = ui.make_persistent_id("custom_drag");
                                                                                    let mut dragging: Option<usize> = ui.data(|d| d.get_temp(drag_id));
                                                                                    if response.drag_started() {
                                                                                        if let Some(pos) = response.interact_pointer_pos() {
                                                                                            let mut best = None;
                                                                                            let mut min_d = 10.0;
                                                                                            for (i, p) in points.iter().enumerate() {
                                                                                                let d = to_screen(p.0, p.1).distance(pos);
                                                                                                if d < min_d { min_d = d; best = Some(i); }
                                                                                            }
                                                                                            if let Some(i) = best { dragging = Some(i); ui.data_mut(|d| d.insert_temp(drag_id, dragging)); }
                                                                                            else if response.clicked_by(egui::PointerButton::Primary) {
                                                                                                // Add point
                                                                                                let (nx, ny) = from_screen(pos);
                                                                                                points.push((nx.clamp(0.0, 1.0), ny.clamp(0.0, 1.0)));
                                                                                                points.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
                                                                                                changed = true;
                                                                                            }
                                                                                        }
                                                                                    }
                                                                                    if let Some(idx) = dragging {
                                                                                        if let Some(pos) = response.interact_pointer_pos() {
                                                                                            let (nx, ny) = from_screen(pos);
                                                                                            if idx < points.len() {
                                                                                                points[idx] = (nx.clamp(0.0, 1.0), ny.clamp(0.0, 1.0));
                                                                                                changed = true;
                                                                                            }
                                                                                        }
                                                                                        if response.drag_released() { 
                                                                                            ui.data_mut(|d| d.remove::<Option<usize>>(drag_id)); 
                                                                                            points.sort_by(|a,b| a.0.partial_cmp(&b.0).unwrap());
                                                                                        }
                                                                                    }
                                                                                    // Right click remove
                                                                                    if response.secondary_clicked() {
                                                                                        if let Some(pos) = response.interact_pointer_pos() {
                                                                                             for (i, p) in points.iter().enumerate() {
                                                                                                 if to_screen(p.0, p.1).distance(pos) < 10.0 {
                                                                                                     if points.len() > 2 { points.remove(i); changed = true; }
                                                                                                     break;
                                                                                                 }
                                                                                             }
                                                                                        }
                                                                                    }
                                                                                }
                                                                                _ => {
                                                                                    // Draw standard easing
                                                                                    let steps = 64;
                                                                                    let mut curve = Vec::with_capacity(steps);
                                                                                    for i in 0..=steps {
                                                                                        let t = i as f32 / steps as f32;
                                                                                        let v = match easing {
                                                                                            crate::scene::Easing::EaseIn { power } => t.powf(*power),
                                                                                            crate::scene::Easing::EaseOut { power } => 1.0 - (1.0 - t).powf(*power),
                                                                                            crate::scene::Easing::EaseInOut { power } => {
                                                                                                if t < 0.5 { 0.5 * (2.0 * t).powf(*power) }
                                                                                                else { 1.0 - 0.5 * (2.0 * (1.0 - t)).powf(*power) }
                                                                                            },
                                                                                            _ => t
                                                                                        };
                                                                                        curve.push(to_screen(t, v));
                                                                                    }
                                                                                    painter.add(egui::Shape::line(curve, egui::Stroke::new(2.0, egui::Color32::LIGHT_BLUE)));
                                                                                    
                                                                                    // Convert to Custom on click
                                                                                    if response.clicked() {
                                                                                        // Sample to points
                                                                                        let mut new_pts = Vec::new();
                                                                                        for i in 0..=5 {
                                                                                            let t = i as f32 / 5.0;
                                                                                            let v = match easing {
                                                                                                crate::scene::Easing::EaseIn { power } => t.powf(*power),
                                                                                                crate::scene::Easing::EaseOut { power } => 1.0 - (1.0 - t).powf(*power),
                                                                                                crate::scene::Easing::EaseInOut { power } => {
                                                                                                    if t < 0.5 { 0.5 * (2.0 * t).powf(*power) }
                                                                                                    else { 1.0 - 0.5 * (2.0 * (1.0 - t)).powf(*power) }
                                                                                                },
                                                                                                _ => t
                                                                                            };
                                                                                            new_pts.push((t, v));
                                                                                        }
                                                                                        *easing = crate::scene::Easing::Custom { points: new_pts };
                                                                                        changed = true;
                                                                                    }
                                                                                }
                                                                            }

                                                                            // --- CONTROLS ---
                                                                            match easing {
                                                                                crate::scene::Easing::EaseIn { power } | 
                                                                                crate::scene::Easing::EaseOut { power } | 
                                                                                crate::scene::Easing::EaseInOut { power } => {
                                                                                    ui.horizontal(|ui| {
                                                                                        if ui.add(egui::Slider::new(power, 0.1..=5.0).text("Power")).changed() { changed = true; }
                                                                                    });
                                                                                }
                                                                                crate::scene::Easing::Bezier { p1, p2 } => {
                                                                                    ui.horizontal(|ui| {
                                                                                        ui.label("P1:");
                                                                                        if ui.add(egui::DragValue::new(&mut p1.0).speed(0.01)).changed() { changed = true; }
                                                                                        if ui.add(egui::DragValue::new(&mut p1.1).speed(0.01)).changed() { changed = true; }
                                                                                        ui.label("P2:");
                                                                                        if ui.add(egui::DragValue::new(&mut p2.0).speed(0.01)).changed() { changed = true; }
                                                                                        if ui.add(egui::DragValue::new(&mut p2.1).speed(0.01)).changed() { changed = true; }
                                                                                    });
                                                                                }
                                                                                _ => {}
                                                                            }
                                                                    });

                                                            ui.add_space(6.0);
                                                        }
                                                    }

                                                    // perform removal after iteration to avoid borrow issues
                                                    if let Some(idx) = remove_idx {
                                                        animations.remove(idx);
                                                        changed = true;
                                                    }
                                                }
                                            });
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

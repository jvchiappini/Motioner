use crate::app_state::AppState;
use eframe::egui;

/// Render and handle interactions for the central canvas area.
pub fn show(ui: &mut egui::Ui, state: &mut AppState, main_ui_enabled: bool) {
    egui::Frame::canvas(ui.style()).show(ui, |ui| {
        // Use Sense::drag() to handle panning and clicks
        let (rect, response) = ui.allocate_exact_size(
            ui.available_size(),
            egui::Sense::drag().union(egui::Sense::click()),
        );

        // --- Interaction ---
        if main_ui_enabled {
            // Panning: Right-click drag or Middle-click drag
            if response.dragged_by(egui::PointerButton::Secondary)
                || response.dragged_by(egui::PointerButton::Middle)
            {
                state.canvas_pan_x += response.drag_delta().x;
                state.canvas_pan_y += response.drag_delta().y;
            }

            // Zooming: Scroll wheel
            if response.hovered() {
                let scroll = ui.input(|i| i.smooth_scroll_delta.y);
                if scroll != 0.0 {
                    let zoom_delta = (scroll * 0.002).exp();

                    // Zoom towards mouse position
                    if let Some(mouse_pos) = ui.input(|i| i.pointer.hover_pos()) {
                        let zoom_before = state.canvas_zoom;
                        state.canvas_zoom *= zoom_delta;
                        state.canvas_zoom = state.canvas_zoom.clamp(0.01, 100.0);
                        let actual_delta = state.canvas_zoom / zoom_before;

                        // Adjust pan to keep mouse-over point stationary
                        let center = rect.center();
                        state.canvas_pan_x = (state.canvas_pan_x - (mouse_pos.x - center.x))
                            * actual_delta
                            + (mouse_pos.x - center.x);
                        state.canvas_pan_y = (state.canvas_pan_y - (mouse_pos.y - center.y))
                            * actual_delta
                            + (mouse_pos.y - center.y);
                    } else {
                        state.canvas_zoom *= zoom_delta;
                        state.canvas_zoom = state.canvas_zoom.clamp(0.01, 100.0);
                    }
                }
            }
        }

        let mut painter = ui.painter_at(rect);

        // Canvas bg: Gray
        painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(60, 60, 60));

        // --- Grid Rendering ---
        let zoom = state.canvas_zoom;
        let pan = egui::vec2(state.canvas_pan_x, state.canvas_pan_y);
        let center = rect.center();

        // Dynamic grid step (AutoCAD-like behavior: grid sub-divides)
        let mut base_step = 100.0;
        while base_step * zoom > 200.0 {
            base_step /= 10.0;
        }
        while base_step * zoom < 20.0 {
            base_step *= 10.0;
        }

        let step = base_step * zoom;

        // Calculate the starting position for the grid lines
        // We want origin to be at (center.x + pan.x, center.y + pan.y)
        let grid_origin = center + pan;

        let start_x = rect.left() + (grid_origin.x - rect.left()) % step - step;
        let start_y = rect.top() + (grid_origin.y - rect.top()) % step - step;

        let grid_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(40, 40, 40));
        let _major_grid_stroke = egui::Stroke::new(1.2, egui::Color32::BLACK);
        let origin_stroke_x = egui::Stroke::new(2.0, egui::Color32::from_rgb(150, 50, 50)); // Red-ish for X
        let origin_stroke_y = egui::Stroke::new(2.0, egui::Color32::from_rgb(50, 150, 50)); // Green-ish for Y

        // Vertical lines
        let mut x = start_x;
        while x <= rect.right() + step {
            if x >= rect.left() {
                painter.line_segment(
                    [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                    grid_stroke,
                );
            }
            x += step;
        }

        // Horizontal lines
        let mut y = start_y;
        while y <= rect.bottom() + step {
            if y >= rect.top() {
                painter.line_segment(
                    [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                    grid_stroke,
                );
            }
            y += step;
        }

        // Draw origin axes
        if grid_origin.x >= rect.left() && grid_origin.x <= rect.right() {
            painter.line_segment(
                [
                    egui::pos2(grid_origin.x, rect.top()),
                    egui::pos2(grid_origin.x, rect.bottom()),
                ],
                origin_stroke_y,
            );
        }
        if grid_origin.y >= rect.top() && grid_origin.y <= rect.bottom() {
            painter.line_segment(
                [
                    egui::pos2(rect.left(), grid_origin.y),
                    egui::pos2(rect.right(), grid_origin.y),
                ],
                origin_stroke_x,
            );
        }

        // --- Fictitious Composition Canvas (The "Paper" or "Main Viewport") ---
        // This is where the actual project elements will be drawn.
        // The size on screen only depends on the project resolution and zoom.
        let composition_size = egui::vec2(state.render_width as f32, state.render_height as f32) * zoom;
        let composition_min = grid_origin - composition_size / 2.0;
        let composition_rect = egui::Rect::from_min_size(composition_min, composition_size);

        // Draw shadows/border for the composition area
        let shadow_rect = composition_rect.expand(4.0 * zoom);
        painter.rect_filled(shadow_rect, 0.0, egui::Color32::from_black_alpha(100));

        // Draw the white paper (background)
        painter.rect_filled(composition_rect, 0.0, egui::Color32::WHITE);
        painter.rect_stroke(
            composition_rect,
            0.0,
            egui::Stroke::new(1.0, egui::Color32::BLACK),
        );
        // Draw a shadow or border for the "Paper" to make it pop against the gray
        painter.rect_stroke(composition_rect, 0.0, egui::Stroke::new(1.0, egui::Color32::BLACK));

        // --- Software Rasterizer Pass ---
        // This buffer has the actual "preview" resolution.
        // --- Software Rasterizer Pass ---
        // This calculates the EXACT resolution of the preview texture.
        // It must be a physical buffer that we then stretch on the screen.
        let target_res_w = state.render_width as f32;
        let target_res_h = state.render_height as f32;

        let buffer_width = (target_res_w * state.preview_multiplier).max(1.0).round() as usize;
        let buffer_height = (target_res_h * state.preview_multiplier).max(1.0).round() as usize;

        // Create an opaque white background for the rasterizer to make pixels more obvious
        let mut color_image = egui::ColorImage::new([buffer_width, buffer_height], egui::Color32::WHITE);

        for shape in &state.scene {
            match shape {
                crate::scene::Shape::Circle { x, y, radius, color } => {
                    // Coordinates in the buffer (normalized 0-1 mapped to buffer size)
                    let cx = *x * buffer_width as f32;
                    let cy = *y * buffer_height as f32;
                    
                    // The radius is relative to the project scale. 
                    // Since 'radius' in scene is stored as composition-space pixels (e.g. 50.0),
                    // we scale it by preview_multiplier to fit the buffer.
                    let r = *radius * state.preview_multiplier;
                    let r2 = r * r;
                    let c = egui::Color32::from_rgba_unmultiplied(color[0], color[1], color[2], color[3]);
                    
                    let min_x = (cx - r).max(0.0).floor() as usize;
                    let max_x = (cx + r).min(buffer_width as f32 - 1.0).ceil() as usize;
                    let min_y = (cy - r).max(0.0).floor() as usize;
                    let max_y = (cy + r).min(buffer_height as f32 - 1.0).ceil() as usize;

                    for py in min_y..=max_y {
                        let row_start = py * buffer_width;
                        for px in min_x..=max_x {
                            let dx = px as f32 - cx;
                            let dy = py as f32 - cy;
                            if dx * dx + dy * dy <= r2 {
                                color_image.pixels[row_start + px] = c;
                            }
                        }
                    }
                }
                crate::scene::Shape::Rect { x, y, w, h, color } => {
                    let rx = *x * buffer_width as f32;
                    let ry = *y * buffer_height as f32;
                    let rw = *w * state.preview_multiplier;
                    let rh = *h * state.preview_multiplier;
                    let c = egui::Color32::from_rgba_unmultiplied(color[0], color[1], color[2], color[3]);

                    let min_x = rx.max(0.0).floor() as usize;
                    let max_x = (rx + rw).min(buffer_width as f32 - 1.0).ceil() as usize;
                    let min_y = ry.max(0.0).floor() as usize;
                    let max_y = (ry + rh).min(buffer_height as f32 - 1.0).ceil() as usize;

                    for py in min_y..=max_y {
                        let row_start = py * buffer_width;
                        for px in min_x..=max_x {
                            color_image.pixels[row_start + px] = c;
                        }
                    }
                }
            }
        }

        // Upload/Update Texture with NEAREST filtering to keep pixels sharp
        let texture = state.preview_texture.get_or_insert_with(|| {
            ui.ctx().load_texture(
                "preview_raster",
                color_image.clone(),
                egui::TextureOptions::NEAREST,
            )
        });
        texture.set(color_image, egui::TextureOptions::NEAREST);

        // Draw the rasterized texture stretched to the full composition_rect
        // This rect represents the "Project Space" scaled by zoom.
        painter.image(
            texture.id(),
            composition_rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );

        // Interaction: clicks / selection relative to normalized coordinates
        if main_ui_enabled && response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                if composition_rect.contains(pos) {
                    let mut hit = None;
                    for (i, shape) in state.scene.iter().enumerate() {
                        match shape {
                            crate::scene::Shape::Circle { x, y, radius, .. } => {
                                let cw = *x * composition_rect.width();
                                let ch = *y * composition_rect.height();
                                let center = composition_rect.left_top() + egui::vec2(cw, ch);
                                let scaled_radius = radius * zoom; // Selection circle depends on zoom, not preview scale
                                if pos.distance(center) <= scaled_radius {
                                    hit = Some(i);
                                }
                            }
                            crate::scene::Shape::Rect { x, y, w, h, .. } => {
                                let cw = *x * composition_rect.width();
                                let ch = *y * composition_rect.height();
                                let min = composition_rect.left_top() + egui::vec2(cw, ch);
                                let size = egui::vec2(*w, *h) * zoom;
                                let rect = egui::Rect::from_min_size(min, size);
                                if rect.contains(pos) {
                                    hit = Some(i);
                                }
                            }
                        }
                    }
                    state.selected = hit;
                } else {
                    state.selected = None;
                }
            }
        }

        // Draw selection highlight (Vectorial, precise, over pixels)
        if let Some(selected_idx) = state.selected {
            if let Some(shape) = state.scene.get(selected_idx) {
                let stroke = egui::Stroke::new(2.0, egui::Color32::from_rgb(255, 165, 0));
                match shape {
                    crate::scene::Shape::Circle { x, y, radius, .. } => {
                        let cw = *x * composition_rect.width();
                        let ch = *y * composition_rect.height();
                        let center = composition_rect.left_top() + egui::vec2(cw, ch);
                        let scaled_radius = radius * zoom;
                        painter.circle_stroke(center, scaled_radius, stroke);
                    }
                    crate::scene::Shape::Rect { x, y, w, h, .. } => {
                        let cw = *x * composition_rect.width();
                        let ch = *y * composition_rect.height();
                        let min = composition_rect.left_top() + egui::vec2(cw, ch);
                        let size = egui::vec2(*w, *h) * zoom;
                        painter.rect_stroke(egui::Rect::from_min_size(min, size), 0.0, stroke);
                    }
                }
            }
        }

        // --- Floating Quick Settings (Top-Left of the Canvas) ---
        // We place this inside the closure to reuse 'grid_origin', 'zoom', and 'rect'
        let mut menu_pos = rect.min;
        menu_pos += egui::vec2(10.0, 10.0); // Margin from top-left

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

                            let current_label = format!("Preview: {}x", state.preview_multiplier);
                            ui.menu_button(current_label, |ui| {
                                ui.set_width(100.0);
                                let multipliers = [0.125, 0.25, 0.5, 1.0, 1.125, 1.25, 1.5, 2.0];
                                for &m in &multipliers {
                                    let label = format!("{}x", m);
                                    if ui
                                        .selectable_label(state.preview_multiplier == m, label)
                                        .clicked()
                                    {
                                        state.preview_multiplier = m;
                                        ui.close_menu();
                                    }
                                }
                            });

                            ui.separator();

                            ui.add(
                                egui::DragValue::new(&mut state.preview_fps)
                                    .prefix("FPS: ")
                                    .clamp_range(1..=240),
                            );

                            ui.separator();

                            // --- Mouse Coordinates relative to fictitious canvas (Normalized 0.0 - 1.0) ---
                            if let Some(mouse_pos) = ui.input(|i| i.pointer.hover_pos()) {
                                // Calculate normalized coordinates (0.0 to 1.0) relative to the top-left of the composition_rect
                                let pct_x = (mouse_pos.x - composition_rect.min.x) / composition_rect.width();
                                let pct_y = (mouse_pos.y - composition_rect.min.y) / composition_rect.height();

                                ui.label(
                                    egui::RichText::new(format!("X: {:.2}%, Y: {:.2}%", pct_x * 100.0, pct_y * 100.0))
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
    });
}

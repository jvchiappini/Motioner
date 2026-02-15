use crate::app_state::AppState;
use crate::scene::Shape;
use eframe::egui;

pub fn show(ui: &mut egui::Ui, state: &mut AppState) {
    // Dark background implicit in panel, but we can force a frame for the specific look
    egui::Frame::none()
        .fill(egui::Color32::from_rgb(40, 40, 42))
        .show(ui, |ui| {
            ui.set_min_size(ui.available_size());

            let (rect, response) =
                ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());

            let ruler_height = 24.0;
            let row_height = 24.0;

            // --- Input Handling (Pan & Zoom) ---
            if response.dragged_by(egui::PointerButton::Middle) {
                state.timeline_pan_x -= response.drag_delta().x;
                state.timeline_pan_y -= response.drag_delta().y;
            }

            let scroll_delta = ui.input(|i| i.raw_scroll_delta);
            if ui.input(|i| i.modifiers.ctrl) && scroll_delta.y != 0.0 {
                let zoom_factor = if scroll_delta.y > 0.0 { 1.1 } else { 0.9 };
                state.timeline_zoom *= zoom_factor;
                state.timeline_zoom = state.timeline_zoom.clamp(10.0, 1000.0);
            } else if scroll_delta.x != 0.0 || scroll_delta.y != 0.0 {
                state.timeline_pan_x -= scroll_delta.x;
                state.timeline_pan_y -= scroll_delta.y;
            }

            // Decide which scene paths are visible in the timeline (supports drilling into groups)
            let mut visible_paths: Vec<Vec<usize>> = Vec::new();
            if let Some(root_path) = state.timeline_root_path.as_ref() {
                match crate::scene::get_shape(&state.scene, root_path) {
                    Some(Shape::Group { children, .. }) => {
                        for j in 0..children.len() {
                            let mut p = root_path.clone();
                            p.push(j);
                            visible_paths.push(p);
                        }
                    }
                    // invalid root (not a group) — reset to top-level
                    _ => {
                        state.timeline_root_path = None;
                    }
                }
            }
            if visible_paths.is_empty() {
                for i in 0..state.scene.len() {
                    visible_paths.push(vec![i]);
                }
            }

            // Clamp pan to sensible limits (use visible rows count)
            let visible_track_height = rect.height() - ruler_height;
            let total_content_height = visible_paths.len() as f32 * row_height;
            let max_pan_y = (total_content_height - visible_track_height).max(0.0);
            if state.timeline_pan_x < 0.0 {
                state.timeline_pan_x = 0.0;
            }
            state.timeline_pan_y = state.timeline_pan_y.clamp(0.0, max_pan_y);

            let painter = ui.painter_at(rect);
            let font_id = egui::FontId::proportional(10.0);

            // sticky gutter width and time origin (0s) aligned to the track area
            let gutter_width = 140.0;
            let time_origin_x = rect.left() + gutter_width; // x coordinate that represents t=0

            // --- 1. Ruler ---
            let ruler_rect =
                egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), ruler_height));
            painter.rect_filled(ruler_rect, 0.0, egui::Color32::from_rgb(55, 55, 57));
            painter.line_segment(
                [ruler_rect.left_bottom(), ruler_rect.right_bottom()],
                egui::Stroke::new(1.0, egui::Color32::from_gray(80)),
            );

            let pixels_per_sec = state.timeline_zoom;
            let step_secs = if pixels_per_sec > 200.0 {
                0.1
            } else if pixels_per_sec > 50.0 {
                1.0
            } else {
                5.0
            };
            let start_sec = (state.timeline_pan_x / pixels_per_sec).floor() as i32;
            let visible_width = rect.width() - gutter_width; // only the track area is time-visible
            let end_sec = ((state.timeline_pan_x + visible_width) / pixels_per_sec).ceil() as i32;

            for s in start_sec..=end_sec {
                let sec_val = s as f32 * step_secs;
                if sec_val < 0.0 {
                    continue;
                }
                let x = time_origin_x + (sec_val * pixels_per_sec) - state.timeline_pan_x;
                if x < time_origin_x {
                    continue;
                }

                painter.line_segment(
                    [
                        egui::pos2(x, ruler_rect.bottom()),
                        egui::pos2(x, ruler_rect.bottom() - 10.0),
                    ],
                    egui::Stroke::new(1.0, egui::Color32::LIGHT_GRAY),
                );

                let time_text =
                    format!("{:02}:{:02}", (sec_val as u32) / 60, (sec_val as u32) % 60);
                painter.text(
                    egui::pos2(x + 2.0, ruler_rect.top() + 4.0),
                    egui::Align2::LEFT_TOP,
                    time_text,
                    font_id.clone(),
                    egui::Color32::GRAY,
                );

                let subticks = 4;
                for i in 1..subticks {
                    let sub_x = x + (pixels_per_sec * step_secs / (subticks as f32)) * (i as f32);
                    if sub_x > rect.right() {
                        break;
                    }
                    painter.line_segment(
                        [
                            egui::pos2(sub_x, ruler_rect.bottom()),
                            egui::pos2(sub_x, ruler_rect.bottom() - 4.0),
                        ],
                        egui::Stroke::new(1.0, egui::Color32::from_gray(90)),
                    );
                }
            }

            // --- 2. Tracks + Sticky Gutter ---
            let gutter_rect = egui::Rect::from_min_max(
                egui::pos2(rect.left(), rect.top() + ruler_height),
                egui::pos2(rect.left() + gutter_width, rect.bottom()),
            );
            let track_area_rect = egui::Rect::from_min_max(
                egui::pos2(rect.left() + gutter_width, rect.top() + ruler_height),
                rect.max,
            );

            // painters and clipping
            let mut gutter_painter = ui.painter_at(gutter_rect);
            gutter_painter.set_clip_rect(gutter_rect);
            let mut track_painter = ui.painter_at(track_area_rect);
            track_painter.set_clip_rect(track_area_rect);

            // gutter bg and separator
            painter.rect_filled(gutter_rect, 0.0, egui::Color32::from_rgb(36, 36, 38));
            painter.line_segment(
                [
                    egui::pos2(gutter_rect.right(), gutter_rect.top()),
                    egui::pos2(gutter_rect.right(), gutter_rect.bottom()),
                ],
                egui::Stroke::new(1.0, egui::Color32::from_gray(30)),
            );

            // If we're drilled into a group, reserve a small header in the gutter
            let gutter_header_h = if state.timeline_root_path.is_some() {
                26.0
            } else {
                0.0
            };

            let start_y = track_area_rect.top() + gutter_header_h - state.timeline_pan_y;

            // Draw gutter header when drilled into a group: show full breadcrumb (animated)
            if gutter_header_h > 0.0 {
                let header_rect = egui::Rect::from_min_size(
                    egui::pos2(gutter_rect.left(), gutter_rect.top()),
                    egui::vec2(gutter_width, gutter_header_h),
                );

                // Animation progress
                let dt = ui.input(|i| i.stable_dt);
                if state.timeline_breadcrumb_anim_t < 1.0 {
                    state.timeline_breadcrumb_anim_t =
                        (state.timeline_breadcrumb_anim_t + dt * 6.0).min(1.0);
                    ui.ctx().request_repaint();
                }
                let t = state.timeline_breadcrumb_anim_t.clamp(0.0, 1.0);

                // Helper: build crumbs from an optional path (None -> Root)
                let build_crumbs =
                    |opt_path: Option<&Vec<usize>>| -> Vec<(String, Option<Vec<usize>>)> {
                        let mut acc: Vec<(String, Option<Vec<usize>>)> = Vec::new();
                        acc.push(("Root".to_string(), None));
                        if let Some(p) = opt_path {
                            let mut prefix: Vec<usize> = Vec::new();
                            for &idx in p.iter() {
                                prefix.push(idx);
                                if let Some(s) = crate::scene::get_shape(&state.scene, &prefix) {
                                    acc.push((s.name().to_string(), Some(prefix.clone())));
                                } else {
                                    acc.push((format!("#{}", idx), Some(prefix.clone())));
                                }
                            }
                        }
                        acc
                    };

                let curr_crumbs = build_crumbs(state.timeline_root_path.as_ref());
                let prev_crumbs = build_crumbs(state.timeline_prev_root_path.as_ref());

                // draw previous crumbs sliding out (when animating)
                if state.timeline_breadcrumb_anim_t < 1.0 {
                    let alpha = ((1.0 - t) * 255.0) as u8;
                    let x_off = (1.0 - t) * -8.0;
                    let mut x = header_rect.left() + 8.0 + x_off;
                    for (i, (label, path)) in prev_crumbs.iter().enumerate() {
                        let mut job = egui::text::LayoutJob::default();
                        job.append(
                            label,
                            0.0,
                            egui::TextFormat {
                                font_id: font_id.clone(),
                                color: egui::Color32::from_rgba_premultiplied(200, 200, 200, alpha),
                                ..Default::default()
                            },
                        );
                        if i + 1 < prev_crumbs.len() {
                            job.append(
                                " ▶ ",
                                0.0,
                                egui::TextFormat {
                                    font_id: font_id.clone(),
                                    color: egui::Color32::from_rgba_premultiplied(
                                        120, 120, 120, alpha,
                                    ),
                                    ..Default::default()
                                },
                            );
                        }
                        let galley = ui.fonts(|f| f.layout_job(job.clone()));
                        let w = galley.size().x;
                        let seg_rect = egui::Rect::from_min_size(
                            egui::pos2(x, header_rect.top() + 6.0),
                            egui::vec2(w, header_rect.height() - 12.0),
                        );
                        painter.galley(
                            egui::pos2(x, header_rect.top() + 6.0),
                            galley,
                            egui::Color32::WHITE,
                        );
                        x += w + 6.0;
                    }
                }

                // draw current crumbs sliding in
                let alpha = (t * 255.0) as u8;
                let x_off = (1.0 - t) * 8.0; // slide from right
                let mut x = header_rect.left() + 8.0 + x_off;
                for (i, (label, path)) in curr_crumbs.iter().enumerate() {
                    let mut job = egui::text::LayoutJob::default();
                    job.append(
                        label,
                        0.0,
                        egui::TextFormat {
                            font_id: font_id.clone(),
                            color: egui::Color32::from_rgba_premultiplied(220, 220, 220, alpha),
                            ..Default::default()
                        },
                    );
                    if i + 1 < curr_crumbs.len() {
                        job.append(
                            " ▶ ",
                            0.0,
                            egui::TextFormat {
                                font_id: font_id.clone(),
                                color: egui::Color32::from_rgba_premultiplied(140, 140, 140, alpha),
                                ..Default::default()
                            },
                        );
                    }
                    let galley = ui.fonts(|f| f.layout_job(job.clone()));
                    let w = galley.size().x;
                    let seg_rect = egui::Rect::from_min_size(
                        egui::pos2(x, header_rect.top() + 6.0),
                        egui::vec2(w, header_rect.height() - 12.0),
                    );

                    // interactive segment: clicking navigates to that ancestor
                    let id = egui::Id::new("timeline_breadcrumb").with(i);
                    let resp = ui.interact(seg_rect, id, egui::Sense::click());
                    if resp.clicked() {
                        // compute new root path from segment
                        let new_root = match path {
                            None => None,
                            Some(p) => Some(p.clone()),
                        };
                        let old = state.timeline_root_path.clone();
                        state.timeline_root_path = new_root;
                        state.timeline_prev_root_path = old;
                        state.timeline_breadcrumb_anim_t = 0.0;
                        state.timeline_pan_x = 0.0;
                    }

                    painter.galley(
                        egui::pos2(x, header_rect.top() + 6.0),
                        galley,
                        egui::Color32::WHITE,
                    );
                    x += w + 6.0;
                }
            }

            for (i, path) in visible_paths.iter().enumerate() {
                let shape = match crate::scene::get_shape(&state.scene, path) {
                    Some(s) => s,
                    None => continue,
                };
                let y = start_y + (i as f32 * row_height);
                if y > track_area_rect.bottom() {
                    break;
                }
                if y + row_height < track_area_rect.top() {
                    continue;
                }
                let is_selected = state.selected_node_path.as_ref() == Some(path);
                let bg_color = if is_selected {
                    egui::Color32::from_rgb(60, 65, 80)
                } else {
                    egui::Color32::TRANSPARENT
                };

                // full-row background (gutter + content)
                let full_row_rect = egui::Rect::from_min_size(
                    egui::pos2(gutter_rect.left(), y),
                    egui::vec2(rect.width(), row_height),
                );
                painter.rect_filled(full_row_rect, 0.0, bg_color);
                painter.line_segment(
                    [full_row_rect.left_bottom(), full_row_rect.right_bottom()],
                    egui::Stroke::new(1.0, egui::Color32::from_gray(50)),
                );

                // sticky label in gutter (top-aligned) — use actual shape name
                let label = shape.name().to_string();
                gutter_painter.text(
                    egui::pos2(gutter_rect.left() + 8.0, y + 4.0),
                    egui::Align2::LEFT_TOP,
                    label,
                    font_id.clone(),
                    if is_selected {
                        egui::Color32::WHITE
                    } else {
                        egui::Color32::GRAY
                    },
                );

                // Row interaction (select / double-click to drill into groups)
                let row_id = egui::Id::new("timeline_row").with(path.clone());
                let row_resp = ui.interact(full_row_rect, row_id, egui::Sense::click());
                if row_resp.double_clicked() {
                    if let Shape::Group { .. } = shape {
                        let old = state.timeline_root_path.clone();
                        state.timeline_root_path = Some(path.clone());
                        state.timeline_prev_root_path = old;
                        state.timeline_breadcrumb_anim_t = 0.0;
                        // reset horizontal pan so user sees group content from start
                        state.timeline_pan_x = 0.0;
                    }
                    state.selected_node_path = Some(path.clone());
                    state.selected = Some(path[0]);
                } else if row_resp.clicked() {
                    state.selected_node_path = Some(path.clone());
                    state.selected = Some(path[0]);
                }

                // duration bar (spawn -> project end)
                let spawn = shape.spawn_time();
                let start_x = time_origin_x + (spawn * pixels_per_sec) - state.timeline_pan_x;
                let end_x =
                    time_origin_x + (state.duration_secs * pixels_per_sec) - state.timeline_pan_x;

                if end_x > start_x
                    && start_x < track_area_rect.right()
                    && end_x > track_area_rect.left()
                {
                    let bar_left = start_x.max(track_area_rect.left());
                    let bar_right = end_x.min(track_area_rect.right());

                    if bar_right > bar_left {
                        let pad = 4.0;
                        let bar_rect = egui::Rect::from_min_max(
                            egui::pos2(bar_left, y + pad),
                            egui::pos2(bar_right, y + row_height - pad),
                        );

                        let fill_color = match shape {
                            Shape::Circle { color, .. } | Shape::Rect { color, .. } => {
                                egui::Color32::from_rgba_premultiplied(
                                    color[0], color[1], color[2], color[3],
                                )
                            }
                            Shape::Group { .. } => egui::Color32::from_gray(120),
                        };

                        track_painter.rect_filled(bar_rect, 3.0, fill_color);
                        track_painter.rect_stroke(
                            bar_rect,
                            3.0,
                            egui::Stroke::new(1.0, egui::Color32::from_gray(40)),
                        );
                    }
                }
            }

            // --- 3. Playhead (Scrubber) ---
            let playhead_x = time_origin_x + (state.time * pixels_per_sec) - state.timeline_pan_x;
            if playhead_x >= rect.left() && playhead_x <= rect.right() {
                painter.line_segment(
                    [
                        egui::pos2(playhead_x, ruler_rect.bottom()),
                        egui::pos2(playhead_x, rect.bottom()),
                    ],
                    egui::Stroke::new(1.5, egui::Color32::from_rgb(255, 50, 50)),
                );

                painter.rect_filled(
                    egui::Rect::from_center_size(
                        egui::pos2(playhead_x, ruler_rect.bottom() - 6.0),
                        egui::vec2(12.0, 12.0),
                    ),
                    2.0,
                    egui::Color32::from_rgb(255, 50, 50),
                );
            }

            // Scrubbing interaction from ruler
            if response.hovered() && ui.input(|i| i.pointer.primary_down()) {
                if let Some(pos) = ui.input(|i| i.pointer.hover_pos()) {
                    if pos.y <= ruler_rect.bottom() + 10.0 {
                        // Convert pointer x → timeline time using the track area's time origin
                        let new_time =
                            (pos.x - time_origin_x + state.timeline_pan_x) / pixels_per_sec;
                        state.time = new_time.max(0.0);
                        // regenerate preview frames around the new playhead position
                        if state.preview_cache_center_time.map_or(true, |c| (c - state.time).abs() > 1e-4) {
                            // interactive scrubbing → request a *single* fast preview frame (debounced)
                            crate::canvas::request_preview_frames(state, state.time, crate::canvas::PreviewMode::Single);
                        }
                    }
                }
            }
        });
}

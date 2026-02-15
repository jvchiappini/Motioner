use crate::animations::animations_manager::animated_xy_for;
use crate::app_state::AppState;
use eframe::egui;
use std::sync::mpsc;
use std::thread;

/// Samples the color at a specific normalized (0..1) paper coordinate,
/// respecting the preview resolution and shape order. `time` is project time in seconds.
fn sample_color_at(state: &crate::app_state::AppState, paper_uv: egui::Pos2, time: f32) -> [u8; 4] {
    let preview_res = egui::vec2(
        state.render_width as f32 * state.preview_multiplier,
        state.render_height as f32 * state.preview_multiplier,
    );

    // Snap UV to the center of the preview pixel (same as shader floor(...) + 0.5)
    let snapped_uv = egui::pos2(
        (paper_uv.x * preview_res.x).floor() + 0.5,
        (paper_uv.y * preview_res.y).floor() + 0.5,
    );

    // Convert snapped logical pixel back to project pixel coordinates
    let pixel_pos = egui::pos2(
        snapped_uv.x * (state.render_width as f32 / preview_res.x),
        snapped_uv.y * (state.render_height as f32 / preview_res.y),
    );

    let mut final_color = [255.0, 255.0, 255.0, 255.0]; // Paper background

    // Recursively traverse the scene graph to find all visual primitives
    fn collect_primitives(
        shapes: &[crate::scene::Shape],
        parent_spawn: f32,
        out: &mut Vec<(crate::scene::Shape, f32)>,
    ) {
        for shape in shapes {
            let my_spawn = shape.spawn_time().max(parent_spawn);
            match shape {
                crate::scene::Shape::Group { children, .. } => {
                    collect_primitives(children, my_spawn, out);
                }
                _ => out.push((shape.clone(), my_spawn)),
            }
        }
    }

    let mut all_primitives = Vec::new();
    collect_primitives(&state.scene, 0.0, &mut all_primitives);

    for (shape, actual_spawn) in all_primitives {
        if time < actual_spawn {
            continue;
        }
        match shape {
            crate::scene::Shape::Circle {
                x: _x,
                y: _y,
                radius,
                color,
                ..
            } => {
                let width = state.render_width as f32;
                let height = state.render_height as f32;
                let (eval_x, eval_y) = animated_xy_for(&shape, time, state.duration_secs);
                let shape_pos = egui::pos2(eval_x * width, eval_y * height);
                let radius_px = radius * width; // Use width as reference
                let shape_color = [
                    color[0] as f32,
                    color[1] as f32,
                    color[2] as f32,
                    color[3] as f32,
                ];
                let dist = pixel_pos.distance(shape_pos);
                if dist <= radius_px {
                    let src_a = (shape_color[3]) / 255.0;
                    final_color[0] = final_color[0] * (1.0 - src_a) + shape_color[0] * src_a;
                    final_color[1] = final_color[1] * (1.0 - src_a) + shape_color[1] * src_a;
                    final_color[2] = final_color[2] * (1.0 - src_a) + shape_color[2] * src_a;
                }
            }
            crate::scene::Shape::Rect {
                x: _x,
                y: _y,
                w,
                h,
                color,
                ..
            } => {
                let width = state.render_width as f32;
                let height = state.render_height as f32;
                let (eval_x, eval_y) = animated_xy_for(&shape, time, state.duration_secs);
                let half_w = (w * width) / 2.0;
                let half_h = (h * height) / 2.0;
                let center_x = eval_x * width + half_w;
                let center_y = eval_y * height + half_h;
                let shape_pos = egui::pos2(center_x, center_y);
                let shape_size = egui::vec2(half_w, half_h);
                let shape_color = [
                    color[0] as f32,
                    color[1] as f32,
                    color[2] as f32,
                    color[3] as f32,
                ];
                let d_vec = egui::vec2(
                    (pixel_pos.x - shape_pos.x).abs() - shape_size.x,
                    (pixel_pos.y - shape_pos.y).abs() - shape_size.y,
                );
                if d_vec.x <= 0.0 && d_vec.y <= 0.0 {
                    let src_a = (shape_color[3]) / 255.0;
                    final_color[0] = final_color[0] * (1.0 - src_a) + shape_color[0] * src_a;
                    final_color[1] = final_color[1] * (1.0 - src_a) + shape_color[1] * src_a;
                    final_color[2] = final_color[2] * (1.0 - src_a) + shape_color[2] * src_a;
                }
            }
            _ => {}
        }
    }

    [
        final_color[0].round() as u8,
        final_color[1].round() as u8,
        final_color[2].round() as u8,
        255,
    ]
}

// animation sampling is implemented in `src/animations/animations_manager.rs`

/// Render a single preview frame into an egui::ColorImage at the current preview resolution.
fn render_frame_color_image(state: &crate::app_state::AppState, time: f32) -> egui::ColorImage {
    let preview_w = (state.render_width as f32 * state.preview_multiplier)
        .round()
        .max(1.0) as usize;
    let preview_h = (state.render_height as f32 * state.preview_multiplier)
        .round()
        .max(1.0) as usize;
    let mut pixels: Vec<u8> = Vec::with_capacity(preview_w * preview_h * 4);

    for y in 0..preview_h {
        for x in 0..preview_w {
            // sample at pixel center in normalized paper coords
            let uv = egui::pos2(
                (x as f32 + 0.5) / (preview_w as f32),
                (y as f32 + 0.5) / (preview_h as f32),
            );
            let col = sample_color_at(state, uv, time);
            pixels.push(col[0]);
            pixels.push(col[1]);
            pixels.push(col[2]);
            pixels.push(col[3]);
        }
    }

    egui::ColorImage::from_rgba_unmultiplied([preview_w, preview_h], &pixels)
}

/// Generate cached preview frames around `center_time` (10 before + 10 after) and
/// store them in `state.preview_frame_cache`. Also update `state.preview_texture` to the center frame.
pub fn generate_preview_frames(state: &mut AppState, center_time: f32, ctx: &egui::Context) {
    // Backward-compat / direct-call fallback: delegate to request_preview_frames (buffered)
    request_preview_frames(state, center_time, PreviewMode::Buffered);
    // poll results immediately so UI updates if worker already finished (non-blocking)
    poll_preview_results(state, ctx);
}

/// Modes for preview generation requests
#[derive(Clone, Copy, Debug)]
pub enum PreviewMode {
    Buffered, // 10 frames before/after center
    Single,   // single center frame
}

/// Job sent to the background preview worker
pub enum PreviewJob {
    Generate {
        center_time: f32,
        mode: PreviewMode,
        snapshot: RenderSnapshot,
    },
}

/// Result returned from background worker
pub enum PreviewResult {
    Buffered(Vec<(f32, egui::ColorImage)>),
    Single(f32, egui::ColorImage),
}

/// Lightweight snapshot of rendering inputs that can be sent to worker threads.
#[derive(Clone)]
pub struct RenderSnapshot {
    pub scene: Vec<crate::scene::Shape>,
    pub render_width: u32,
    pub render_height: u32,
    pub preview_multiplier: f32,
    pub duration_secs: f32,
    pub preview_fps: u32,
}

fn render_frame_color_image_snapshot(snap: &RenderSnapshot, time: f32) -> egui::ColorImage {
    let preview_w = (snap.render_width as f32 * snap.preview_multiplier)
        .round()
        .max(1.0) as usize;
    let preview_h = (snap.render_height as f32 * snap.preview_multiplier)
        .round()
        .max(1.0) as usize;
    let mut pixels: Vec<u8> = Vec::with_capacity(preview_w * preview_h * 4);

    // reuse a minimal sampling loop similar to `sample_color_at` but operating on the snapshot
    for y in 0..preview_h {
        for x in 0..preview_w {
            let uv = egui::pos2(
                (x as f32 + 0.5) / (preview_w as f32),
                (y as f32 + 0.5) / (preview_h as f32),
            );
            // simple sampling: iterate scene primitives and composite (no sub-pixel antialiasing)
            let mut final_color = [255.0, 255.0, 255.0, 255.0];

            // collect primitives
            fn collect_primitives(
                shapes: &[crate::scene::Shape],
                parent_spawn: f32,
                out: &mut Vec<(crate::scene::Shape, f32)>,
            ) {
                for shape in shapes {
                    let my_spawn = shape.spawn_time().max(parent_spawn);
                    match shape {
                        crate::scene::Shape::Group { children, .. } => {
                            collect_primitives(children, my_spawn, out)
                        }
                        _ => out.push((shape.clone(), my_spawn)),
                    }
                }
            }

            let mut all_prims = Vec::new();
            collect_primitives(&snap.scene, 0.0, &mut all_prims);

            // produce pixel in project coordinates
            let pixel_pos = egui::pos2(
                uv.x * snap.render_width as f32,
                uv.y * snap.render_height as f32,
            );

            for (shape, actual_spawn) in all_prims {
                if time < actual_spawn {
                    continue;
                }
                match shape {
                    crate::scene::Shape::Circle {
                        x,
                        y,
                        radius,
                        color,
                        ..
                    } => {
                        let (eval_x, eval_y) = animated_xy_for(&shape, time, snap.duration_secs);
                        let shape_pos = egui::pos2(
                            eval_x * snap.render_width as f32,
                            eval_y * snap.render_height as f32,
                        );
                        let radius_px = radius * snap.render_width as f32;
                        let dist = pixel_pos.distance(shape_pos);
                        if dist <= radius_px {
                            let src_a = (color[3] as f32) / 255.0;
                            final_color[0] =
                                final_color[0] * (1.0 - src_a) + (color[0] as f32) * src_a;
                            final_color[1] =
                                final_color[1] * (1.0 - src_a) + (color[1] as f32) * src_a;
                            final_color[2] =
                                final_color[2] * (1.0 - src_a) + (color[2] as f32) * src_a;
                        }
                    }
                    crate::scene::Shape::Rect {
                        x, y, w, h, color, ..
                    } => {
                        let (eval_x, eval_y) = animated_xy_for(&shape, time, snap.duration_secs);
                        let half_w = (w * snap.render_width as f32) / 2.0;
                        let half_h = (h * snap.render_height as f32) / 2.0;
                        let center_x = eval_x * snap.render_width as f32 + half_w;
                        let center_y = eval_y * snap.render_height as f32 + half_h;
                        let d_vec = egui::vec2(
                            (pixel_pos.x - center_x).abs() - half_w,
                            (pixel_pos.y - center_y).abs() - half_h,
                        );
                        if d_vec.x <= 0.0 && d_vec.y <= 0.0 {
                            let src_a = (color[3] as f32) / 255.0;
                            final_color[0] =
                                final_color[0] * (1.0 - src_a) + (color[0] as f32) * src_a;
                            final_color[1] =
                                final_color[1] * (1.0 - src_a) + (color[1] as f32) * src_a;
                            final_color[2] =
                                final_color[2] * (1.0 - src_a) + (color[2] as f32) * src_a;
                        }
                    }
                    _ => {}
                }
            }

            pixels.push(final_color[0].round() as u8);
            pixels.push(final_color[1].round() as u8);
            pixels.push(final_color[2].round() as u8);
            pixels.push(255);
        }
    }

    egui::ColorImage::from_rgba_unmultiplied([preview_w, preview_h], &pixels)
}

/// Ensure background preview worker is running; if not, spawn it and store channels in `state`.
fn ensure_preview_worker(state: &mut AppState) {
    if state.preview_worker_tx.is_some() && state.preview_worker_rx.is_some() {
        return;
    }

    let (job_tx, job_rx) = mpsc::channel::<PreviewJob>();
    let (res_tx, res_rx) = mpsc::channel::<PreviewResult>();

    // Spawn worker
    thread::spawn(move || {
        while let Ok(job) = job_rx.recv() {
            match job {
                PreviewJob::Generate {
                    center_time,
                    mode,
                    snapshot,
                } => {
                    match mode {
                        PreviewMode::Single => {
                            let img = render_frame_color_image_snapshot(&snapshot, center_time);
                            let _ = res_tx.send(PreviewResult::Single(center_time, img));
                        }
                        PreviewMode::Buffered => {
                            let frames_each_side = 10i32;
                            let frame_step = 1.0 / (snapshot.preview_fps as f32);
                            let mut frames =
                                Vec::with_capacity((frames_each_side * 2 + 1) as usize);
                            for i in -frames_each_side..=frames_each_side {
                                let t = (center_time + (i as f32) * frame_step)
                                    .clamp(0.0, snapshot.duration_secs);
                                let img = render_frame_color_image_snapshot(&snapshot, t);
                                frames.push((t, img));
                                // send intermediate single-frame updates for smoother UX
                                let _ = res_tx.send(PreviewResult::Single(
                                    t,
                                    frames.last().unwrap().1.clone(),
                                ));
                            }
                            let _ = res_tx.send(PreviewResult::Buffered(frames));
                        }
                    }
                }
            }
        }
    });

    state.preview_worker_tx = Some(job_tx);
    state.preview_worker_rx = Some(res_rx);
}

/// Request preview frames (delegates to background worker). Non-blocking.
pub fn request_preview_frames(state: &mut AppState, center_time: f32, mode: PreviewMode) {
    ensure_preview_worker(state);
    if let Some(tx) = &state.preview_worker_tx {
        let snap = RenderSnapshot {
            scene: state.scene.clone(),
            render_width: state.render_width,
            render_height: state.render_height,
            preview_multiplier: state.preview_multiplier,
            duration_secs: state.duration_secs,
            preview_fps: state.preview_fps,
        };
        let job = PreviewJob::Generate {
            center_time,
            mode,
            snapshot: snap,
        };
        let _ = tx.send(job);
    }
}

/// Poll for preview results from the worker and integrate them into `state` (must be called on UI thread).
pub fn poll_preview_results(state: &mut AppState, ctx: &egui::Context) {
    if let Some(rx) = &state.preview_worker_rx {
        while let Ok(result) = rx.try_recv() {
            match result {
                PreviewResult::Single(t, img) => {
                    // update/insert single frame into cache
                    // replace center frame if times are close
                    state
                        .preview_frame_cache
                        .retain(|(tt, _)| (tt - t).abs() > 1e-6);
                    state.preview_frame_cache.push((t, img.clone()));
                    // if this is near the cache center, update texture too
                    if state
                        .preview_cache_center_time
                        .map_or(true, |c| (c - t).abs() < 1e-3)
                    {
                        let handle = ctx.load_texture(
                            "preview_center",
                            img.clone(),
                            egui::TextureOptions::NEAREST,
                        );
                        state.preview_texture = Some(handle);
                        state.preview_cache_center_time = Some(t);
                    }
                }
                PreviewResult::Buffered(frames) => {
                    state.preview_frame_cache = frames.clone();
                    if let Some((_, center_img)) = state.preview_frame_cache.get((10) as usize) {
                        let handle = ctx.load_texture(
                            "preview_center",
                            center_img.clone(),
                            egui::TextureOptions::NEAREST,
                        );
                        state.preview_texture = Some(handle);
                        state.preview_cache_center_time = Some(state.preview_frame_cache[10].0);
                    }
                }
            }
        }
    }
}

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

        let painter = ui.painter_at(rect);

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
        let composition_size =
            egui::vec2(state.render_width as f32, state.render_height as f32) * zoom;
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
        painter.rect_stroke(
            composition_rect,
            0.0,
            egui::Stroke::new(1.0, egui::Color32::BLACK),
        );

        // --- Software Rasterizer Pass ---
        // This buffer has the actual "preview" resolution.
        // --- GPU / WGPU Rasterizer ---
        #[cfg(feature = "wgpu")]
        {
            let mut gpu_shapes = Vec::new();

            fn fill_gpu_shapes(
                shapes: &[crate::scene::Shape],
                gpu_shapes: &mut Vec<GpuShape>,
                _render_width: f32,
                _render_height: f32,
                parent_spawn: f32,
                current_time: f32,
                project_duration: f32,
            ) {
                for shape in shapes {
                    let my_spawn = shape.spawn_time().max(parent_spawn);
                    match shape {
                        crate::scene::Shape::Circle {
                            x,
                            y,
                            radius,
                            color,
                            ..
                        } => {
                            // Apply animated position when available (GPU path must match software rasterizer)
                            let (eval_x, eval_y) =
                                crate::animations::animations_manager::animated_xy_for(
                                    shape,
                                    current_time,
                                    project_duration,
                                );
                            gpu_shapes.push(GpuShape {
                                pos: [eval_x, eval_y],
                                size: [*radius, 0.0],
                                color: [
                                    color[0] as f32 / 255.0,
                                    color[1] as f32 / 255.0,
                                    color[2] as f32 / 255.0,
                                    color[3] as f32 / 255.0,
                                ],
                                shape_type: 0,
                                spawn_time: my_spawn,
                                p1: 0,
                                p2: 0,
                            });
                        }
                        crate::scene::Shape::Rect {
                            x, y, w, h, color, ..
                        } => {
                            // Use animated center for rects as well
                            let (eval_x, eval_y) =
                                crate::animations::animations_manager::animated_xy_for(
                                    shape,
                                    current_time,
                                    project_duration,
                                );
                            gpu_shapes.push(GpuShape {
                                pos: [eval_x + *w / 2.0, eval_y + *h / 2.0],
                                size: [*w / 2.0, *h / 2.0],
                                color: [
                                    color[0] as f32 / 255.0,
                                    color[1] as f32 / 255.0,
                                    color[2] as f32 / 255.0,
                                    color[3] as f32 / 255.0,
                                ],
                                shape_type: 1,
                                spawn_time: my_spawn,
                                p1: 0,
                                p2: 0,
                            });
                        }
                        crate::scene::Shape::Group { children, .. } => {
                            fill_gpu_shapes(
                                children,
                                gpu_shapes,
                                _render_width,
                                _render_height,
                                my_spawn,
                                current_time,
                                project_duration,
                            );
                        }
                    }
                }
            }

            fill_gpu_shapes(
                &state.scene,
                &mut gpu_shapes,
                state.render_width as f32,
                state.render_height as f32,
                0.0,
                state.time,
                state.duration_secs,
            );

            // Important: use the FULL canvas rect for the callback to avoid coordinate distortion
            let magnifier_pos = if state.picker_active {
                ui.input(|i| i.pointer.hover_pos())
            } else {
                None
            };

            let cb = egui_wgpu::Callback::new_paint_callback(
                rect, // Use full viewport rect, not just the paper rect
                CompositionCallback {
                    shapes: gpu_shapes,
                    render_width: state.render_width as f32,
                    render_height: state.render_height as f32,
                    preview_multiplier: state.preview_multiplier,
                    paper_rect: composition_rect,
                    viewport_rect: rect,
                    magnifier_pos,
                    time: state.time,
                },
            );

            painter.add(cb);
        }

        #[cfg(not(feature = "wgpu"))]
        {
            fn draw_shapes_recursive(
                ui_painter: &egui::Painter,
                shapes: &[crate::scene::Shape],
                composition_rect: egui::Rect,
                zoom: f32,
                current_time: f32,
                parent_spawn: f32,
                project_duration: f32,
            ) {
                for shape in shapes {
                    let actual_spawn = shape.spawn_time().max(parent_spawn);
                    if current_time < actual_spawn {
                        continue;
                    }
                    match shape {
                        crate::scene::Shape::Circle {
                            x: _x,
                            y: _y,
                            radius,
                            color,
                            ..
                        } => {
                            let (eval_x, eval_y) =
                                animated_xy_for(shape, current_time, project_duration);
                            let pos = composition_rect.min
                                + egui::vec2(
                                    eval_x * composition_rect.width(),
                                    eval_y * composition_rect.height(),
                                );
                            let scaled_radius = radius * composition_rect.width();
                            let c = egui::Color32::from_rgba_unmultiplied(
                                color[0], color[1], color[2], color[3],
                            );
                            ui_painter.circle_filled(pos, scaled_radius, c);
                        }
                        crate::scene::Shape::Rect {
                            x: _x,
                            y: _y,
                            w,
                            h,
                            color,
                            ..
                        } => {
                            let (eval_x, eval_y) =
                                animated_xy_for(shape, current_time, project_duration);
                            let min = composition_rect.min
                                + egui::vec2(
                                    eval_x * composition_rect.width(),
                                    eval_y * composition_rect.height(),
                                );
                            let size = egui::vec2(
                                w * composition_rect.width(),
                                h * composition_rect.height(),
                            );
                            let rect = egui::Rect::from_min_size(min, size);
                            let c = egui::Color32::from_rgba_unmultiplied(
                                color[0], color[1], color[2], color[3],
                            );
                            ui_painter.rect_filled(rect, 0.0, c);
                        }
                        crate::scene::Shape::Group { children, .. } => {
                            draw_shapes_recursive(
                                ui_painter,
                                children,
                                composition_rect,
                                zoom,
                                current_time,
                                actual_spawn,
                                project_duration,
                            );
                        }
                    }
                }
            }

            draw_shapes_recursive(
                &painter,
                &state.scene,
                composition_rect,
                zoom,
                state.time,
                0.0,
                state.duration_secs,
            );
        }

        // Interaction: clicks / selection relative to normalized coordinates
        if main_ui_enabled && response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                if composition_rect.contains(pos) {
                    let paper_uv = egui::pos2(
                        (pos.x - composition_rect.min.x) / composition_rect.width(),
                        (pos.y - composition_rect.min.y) / composition_rect.height(),
                    );

                    if state.picker_active {
                        // COLOR PICKER LOGIC
                        let color = sample_color_at(state, paper_uv, state.time);
                        let hex = format!("#{:02x}{:02x}{:02x}", color[0], color[1], color[2]);

                        // Copy to clipboard
                        ui.output_mut(|o| o.copied_text = hex.clone());

                        // Show Toast
                        state.picker_color = color;
                        state.toast_message = Some(format!("Color {} copied to clipboard!", hex));
                        state.toast_type = crate::app_state::ToastType::Success;
                        state.toast_deadline = ui.input(|i| i.time) + 3.0;

                        // Auto-disable picker after pick
                        state.picker_active = false;
                    } else {
                        // SELECTION LOGIC
                        // Determine hit path recursively
                        fn find_hit_path(
                            shapes: &[crate::scene::Shape],
                            pos: egui::Pos2,
                            composition_rect: egui::Rect,
                            zoom: f32,
                            current_path: Vec<usize>,
                            current_time: f32,
                            parent_spawn: f32,
                        ) -> Option<Vec<usize>> {
                            // Iterate in reverse order to prefer picking top-most elements
                            for (i, shape) in shapes.iter().enumerate().rev() {
                                let actual_spawn = shape.spawn_time().max(parent_spawn);
                                if current_time < actual_spawn {
                                    continue;
                                }

                                let mut path = current_path.clone();
                                path.push(i);

                                match shape {
                                    crate::scene::Shape::Circle { x, y, radius, .. } => {
                                        let cw = *x * composition_rect.width();
                                        let ch = *y * composition_rect.height();
                                        let center =
                                            composition_rect.left_top() + egui::vec2(cw, ch);
                                        let scaled_radius = radius * composition_rect.width();
                                        if pos.distance(center) <= scaled_radius {
                                            return Some(path);
                                        }
                                    }
                                    crate::scene::Shape::Rect { x, y, w, h, .. } => {
                                        let cw = *x * composition_rect.width();
                                        let ch = *y * composition_rect.height();
                                        let min = composition_rect.left_top() + egui::vec2(cw, ch);
                                        let size = egui::vec2(
                                            w * composition_rect.width(),
                                            h * composition_rect.height(),
                                        );
                                        let rect = egui::Rect::from_min_size(min, size);
                                        if rect.contains(pos) {
                                            return Some(path);
                                        }
                                    }
                                    crate::scene::Shape::Group { children, .. } => {
                                        // Try to hit children first
                                        if let Some(child_path) = find_hit_path(
                                            children,
                                            pos,
                                            composition_rect,
                                            zoom,
                                            path.clone(),
                                            current_time,
                                            actual_spawn,
                                        ) {
                                            return Some(child_path);
                                        }

                                        // If no child hit, but we want the group to be pickable as a whole,
                                        // we'd need a bounding box for the group. For now, groups aren't picked directly
                                        // unless we are picking their children.
                                    }
                                }
                            }
                            None
                        }

                        let hit_path = find_hit_path(
                            &state.scene,
                            pos,
                            composition_rect,
                            zoom,
                            Vec::new(),
                            state.time,
                            0.0,
                        );

                        // Update both the top-level selection and the selected node path
                        if let Some(p) = hit_path {
                            state.selected = Some(p[0]);
                            state.selected_node_path = Some(p);
                        } else {
                            state.selected = None;
                            state.selected_node_path = None;
                        }
                    }
                } else {
                    state.selected = None;
                }
            }
        }

        // Draw selection highlight (supports nested selection paths)
        if let Some(path) = &state.selected_node_path {
            let stroke = egui::Stroke::new(2.0, egui::Color32::from_rgb(255, 165, 0));

            // Helper to draw highlights recursively for a shape
            fn draw_highlight_recursive(
                painter: &egui::Painter,
                shape: &crate::scene::Shape,
                composition_rect: egui::Rect,
                _zoom: f32,
                stroke: egui::Stroke,
                current_time: f32,
                parent_spawn: f32,
            ) {
                let actual_spawn = shape.spawn_time().max(parent_spawn);
                if current_time < actual_spawn {
                    return;
                }
                match shape {
                    crate::scene::Shape::Circle { x, y, radius, .. } => {
                        let cw = *x * composition_rect.width();
                        let ch = *y * composition_rect.height();
                        let center = composition_rect.left_top() + egui::vec2(cw, ch);
                        let scaled_radius = radius * composition_rect.width();
                        painter.circle_stroke(center, scaled_radius, stroke);
                    }
                    crate::scene::Shape::Rect { x, y, w, h, .. } => {
                        let cw = *x * composition_rect.width();
                        let ch = *y * composition_rect.height();
                        let min = composition_rect.left_top() + egui::vec2(cw, ch);
                        let size =
                            egui::vec2(w * composition_rect.width(), h * composition_rect.height());
                        painter.rect_stroke(egui::Rect::from_min_size(min, size), 0.0, stroke);
                    }
                    crate::scene::Shape::Group { children, .. } => {
                        for child in children {
                            draw_highlight_recursive(
                                painter,
                                child,
                                composition_rect,
                                _zoom,
                                stroke,
                                current_time,
                                actual_spawn,
                            );
                        }
                    }
                }
            }

            // Find the selected node
            let mut current_node: Option<&crate::scene::Shape> = state.scene.get(path[0]);
            for &idx in &path[1..] {
                current_node = match current_node {
                    Some(crate::scene::Shape::Group { children, .. }) => children.get(idx),
                    _ => None,
                };
            }

            if let Some(node) = current_node {
                draw_highlight_recursive(
                    &painter,
                    node,
                    composition_rect,
                    zoom,
                    stroke,
                    state.time,
                    0.0,
                );
            }
        } else if let Some(selected_idx) = state.selected {
            // Backwards-compatible fallback (should rarely be used since we set `selected_node_path` everywhere)
            if let Some(shape) = state.scene.get(selected_idx) {
                if state.time >= shape.spawn_time() {
                    let stroke = egui::Stroke::new(2.0, egui::Color32::from_rgb(255, 165, 0));
                    match shape {
                        crate::scene::Shape::Circle { x, y, radius, .. } => {
                            let cw = *x * composition_rect.width();
                            let ch = *y * composition_rect.height();
                            let center = composition_rect.left_top() + egui::vec2(cw, ch);
                            let scaled_radius = radius * composition_rect.width();
                            painter.circle_stroke(center, scaled_radius, stroke);
                        }
                        crate::scene::Shape::Rect { x, y, w, h, .. } => {
                            let cw = *x * composition_rect.width();
                            let ch = *y * composition_rect.height();
                            let min = composition_rect.left_top() + egui::vec2(cw, ch);
                            let size = egui::vec2(
                                w * composition_rect.width(),
                                h * composition_rect.height(),
                            );
                            painter.rect_stroke(egui::Rect::from_min_size(min, size), 0.0, stroke);
                        }
                        _ => {}
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

                            // Color Picker Button
                            let picker_btn = egui::Button::new(
                                egui::RichText::new("📷").size(14.0),
                            )
                            .fill(if state.picker_active {
                                egui::Color32::from_rgb(255, 100, 0)
                            } else {
                                egui::Color32::TRANSPARENT
                            });

                            if ui
                                .add(picker_btn)
                                .on_hover_text("Color Picker & Magnifier")
                                .clicked()
                            {
                                state.picker_active = !state.picker_active;
                            }

                            // Show current picked color
                            let (rect, _response) =
                                ui.allocate_at_least(egui::vec2(14.0, 14.0), egui::Sense::hover());
                            ui.painter().rect_filled(
                                rect.shrink(2.0),
                                2.0,
                                egui::Color32::from_rgb(
                                    state.picker_color[0],
                                    state.picker_color[1],
                                    state.picker_color[2],
                                ),
                            );
                            ui.painter().rect_stroke(
                                rect.shrink(2.0),
                                2.0,
                                egui::Stroke::new(1.0, egui::Color32::GRAY),
                            );

                            ui.separator();

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
                                let pct_x = (mouse_pos.x - composition_rect.min.x)
                                    / composition_rect.width();
                                let pct_y = (mouse_pos.y - composition_rect.min.y)
                                    / composition_rect.height();

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
    });
}

#[cfg(feature = "wgpu")]
use eframe::egui_wgpu;
#[cfg(feature = "wgpu")]
use eframe::wgpu;

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuShape {
    pos: [f32; 2],
    size: [f32; 2],
    color: [f32; 4],
    shape_type: i32,
    spawn_time: f32,
    p1: i32,
    p2: i32,
}

#[cfg(feature = "wgpu")]
struct CompositionCallback {
    shapes: Vec<GpuShape>,
    render_width: f32,
    render_height: f32,
    preview_multiplier: f32,
    paper_rect: egui::Rect,
    viewport_rect: egui::Rect,
    // Magnifier / Picker
    magnifier_pos: Option<egui::Pos2>,
    time: f32,
}

#[cfg(feature = "wgpu")]
impl egui_wgpu::CallbackTrait for CompositionCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _screen_descriptor: &egui_wgpu::ScreenDescriptor,
        _egui_encoder: &mut wgpu::CommandEncoder,
        callback_resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let resources: &mut GpuResources = callback_resources.get_mut().unwrap();

        let shape_data = bytemuck::cast_slice(&self.shapes);
        if shape_data.len() > resources.shape_buffer.size() as usize {
            resources.shape_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("shape_buffer"),
                size: (shape_data.len() * 2 + 1024) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            // Rebuild bind group
            resources.bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("composition_bind_group"),
                layout: &resources.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: resources.shape_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: resources.uniform_buffer.as_entire_binding(),
                    },
                ],
            });
        }

        if !self.shapes.is_empty() {
            queue.write_buffer(&resources.shape_buffer, 0, shape_data);
        }

        // Layout:
        // vec4 resolution (w, h, prev_w, prev_h)
        // vec4 paper_rect (min_x, min_y, max_x, max_y)
        // vec4 viewport_rect (min_x, min_y, max_x, max_y)
        // vec4 count_mag_pos (count, mag_x, mag_y, mag_active)
        let mag_active = if self.magnifier_pos.is_some() {
            1.0
        } else {
            0.0
        };
        let m_pos = self.magnifier_pos.unwrap_or(egui::Pos2::ZERO);

        let mut uniforms: [f32; 20] = [0.0; 20];
        uniforms[0] = self.render_width;
        uniforms[1] = self.render_height;
        uniforms[2] = self.render_width * self.preview_multiplier;
        uniforms[3] = self.render_height * self.preview_multiplier;
        uniforms[4] = self.paper_rect.min.x;
        uniforms[5] = self.paper_rect.min.y;
        uniforms[6] = self.paper_rect.max.x;
        uniforms[7] = self.paper_rect.max.y;
        uniforms[8] = self.viewport_rect.min.x;
        uniforms[9] = self.viewport_rect.min.y;
        uniforms[10] = self.viewport_rect.max.x;
        uniforms[11] = self.viewport_rect.max.y;
        uniforms[12] = self.shapes.len() as f32;
        uniforms[13] = m_pos.x;
        uniforms[14] = m_pos.y;
        uniforms[15] = mag_active;
        uniforms[16] = self.time; // Pass the current time
                                  // 17, 18, 19 remain 0.0 for padding

        queue.write_buffer(
            &resources.uniform_buffer,
            0,
            bytemuck::cast_slice(&uniforms),
        );

        Vec::new()
    }

    fn paint<'a>(
        &'a self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'a>,
        callback_resources: &'a egui_wgpu::CallbackResources,
    ) {
        let resources: &GpuResources = callback_resources.get().unwrap();
        render_pass.set_pipeline(&resources.pipeline);
        render_pass.set_bind_group(0, &resources.bind_group, &[]);
        render_pass.draw(0..6, 0..1); // Draw 2 triangles covering the quad
    }
}

#[cfg(feature = "wgpu")]
pub struct GpuResources {
    pub pipeline: wgpu::RenderPipeline,
    pub shape_buffer: wgpu::Buffer,
    pub uniform_buffer: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
    pub bind_group_layout: wgpu::BindGroupLayout,
}

#[cfg(feature = "wgpu")]
impl GpuResources {
    pub fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("composition_shader"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(include_str!(
                "composition.wgsl"
            ))),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("composition_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("composition_layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("composition_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });

        let shape_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shape_buffer"),
            size: 1024, // Start small
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("uniform_buffer"),
            size: 80, // 20 * f32
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("composition_bind_group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: shape_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: uniform_buffer.as_entire_binding(),
                },
            ],
        });

        Self {
            pipeline,
            shape_buffer,
            uniform_buffer,
            bind_group,
            bind_group_layout,
        }
    }
}

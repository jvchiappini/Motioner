use crate::app_state::AppState;
use eframe::egui;

/// Messages sent from the ffmpeg background thread to the UI thread.
pub enum FfmpegMsg {
    Log(String),
    /// (frames_done, total_frames)
    Progress(u32, u32),
    Done,
    Error(String),
}

pub fn show(ctx: &egui::Context, state: &mut AppState) {
    if !state.show_export_modal {
        return;
    }

    // Poll ffmpeg background thread messages
    let mut pending_msgs: Vec<FfmpegMsg> = Vec::new();
    if let Some(rx) = &state.export_ffmpeg_rx {
        while let Ok(msg) = rx.try_recv() {
            pending_msgs.push(msg);
        }
        if !state.export_ffmpeg_done {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
    for msg in pending_msgs {
        match msg {
            FfmpegMsg::Log(line) => state.export_ffmpeg_log.push(line),
            FfmpegMsg::Progress(done, total) => {
                state.export_frames_done = done;
                state.export_frames_total = total;
            }
            FfmpegMsg::Done => {
                state.export_ffmpeg_done = true;
                state
                    .export_ffmpeg_log
                    .push("✅ Export finished successfully.".to_string());
                state.export_ffmpeg_rx = None;
            }
            FfmpegMsg::Error(err) => {
                state.export_ffmpeg_done = true;
                state.export_ffmpeg_error = Some(err.clone());
                state.export_ffmpeg_log.push(format!("❌ Error: {}", err));
                state.export_ffmpeg_rx = None;
            }
        }
    }

    // Dimmed overlay – blocks interaction with everything behind
    let screen_rect = ctx.input(|i| i.screen_rect());
    let fade_color = egui::Color32::from_black_alpha(210);

    egui::Area::new("export_modal_overlay")
        .fixed_pos(egui::pos2(0.0, 0.0))
        .interactable(true)
        .order(egui::Order::Tooltip)
        .show(ctx, |ui| {
            // Full-screen blocker
            let blocker = ui.allocate_rect(screen_rect, egui::Sense::click_and_drag());
            ui.painter().rect_filled(screen_rect, 0.0, fade_color);
            let _ = blocker; // absorbs all clicks

            match state.export_modal_step {
                0 => draw_config_step(ui, state, screen_rect),
                1 => draw_export_step(ui, ctx, state, screen_rect),
                _ => {}
            }
        });
}

// ─── Step 0: Configuration ────────────────────────────────────────────────────

fn draw_config_step(ui: &mut egui::Ui, state: &mut AppState, screen_rect: egui::Rect) {
    let width = 480.0;
    let height = 380.0;
    let center = screen_rect.center();
    let rect = egui::Rect::from_center_size(center, egui::vec2(width, height));

    ui.allocate_ui_at_rect(rect, |ui| {
        ui.push_id("export_config_modal", |ui| {
            egui::Frame::window(ui.style())
                .fill(egui::Color32::from_rgb(28, 28, 32))
                .inner_margin(28.0)
                .rounding(14.0)
                .shadow(egui::epaint::Shadow {
                    extrusion: 40.0,
                    color: egui::Color32::BLACK,
                })
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_white_alpha(25)))
                .show(ui, |ui| {
                    // Title
                    ui.vertical_centered(|ui| {
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new("🎬  Export Video")
                                .size(22.0)
                                .strong()
                                .color(egui::Color32::WHITE),
                        );
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new("Review and adjust your export settings")
                                .size(13.0)
                                .color(egui::Color32::from_white_alpha(140)),
                        );
                    });

                    ui.add_space(20.0);
                    ui.separator();
                    ui.add_space(14.0);

                    // Grid of settings
                    egui::Grid::new("export_config_grid")
                        .num_columns(2)
                        .spacing(egui::vec2(16.0, 10.0))
                        .show(ui, |ui| {
                            // Width
                            ui.label(
                                egui::RichText::new("Width (px)")
                                    .color(egui::Color32::from_white_alpha(200)),
                            );
                            ui.add(
                                egui::DragValue::new(&mut state.export_modal_width)
                                    .speed(1.0)
                                    .clamp_range(16u32..=7680u32),
                            );
                            ui.end_row();

                            // Height
                            ui.label(
                                egui::RichText::new("Height (px)")
                                    .color(egui::Color32::from_white_alpha(200)),
                            );
                            ui.add(
                                egui::DragValue::new(&mut state.export_modal_height)
                                    .speed(1.0)
                                    .clamp_range(16u32..=4320u32),
                            );
                            ui.end_row();

                            // FPS
                            ui.label(
                                egui::RichText::new("Frame Rate (fps)")
                                    .color(egui::Color32::from_white_alpha(200)),
                            );
                            ui.add(
                                egui::DragValue::new(&mut state.export_modal_fps)
                                    .speed(1.0)
                                    .clamp_range(1u32..=240u32),
                            );
                            ui.end_row();

                            // Duration
                            ui.label(
                                egui::RichText::new("Duration (s)")
                                    .color(egui::Color32::from_white_alpha(200)),
                            );
                            ui.add(
                                egui::DragValue::new(&mut state.export_modal_duration)
                                    .speed(0.1)
                                    .clamp_range(0.1f32..=3600.0f32),
                            );
                            ui.end_row();
                        });

                    // Summary line
                    ui.add_space(10.0);
                    let total_frames =
                        (state.export_modal_duration * state.export_modal_fps as f32).ceil() as u32;
                    ui.label(
                        egui::RichText::new(format!(
                            "→  {} frames  ·  {}×{}  ·  {:.1}s",
                            total_frames,
                            state.export_modal_width,
                            state.export_modal_height,
                            state.export_modal_duration,
                        ))
                        .size(12.0)
                        .color(egui::Color32::from_white_alpha(120)),
                    );

                    ui.add_space(24.0);

                    // Buttons
                    ui.horizontal(|ui| {
                        let cancel_btn =
                            egui::Button::new(egui::RichText::new("Cancel").size(14.0))
                                .min_size(egui::vec2(100.0, 36.0));

                        if ui.add(cancel_btn).clicked() {
                            close_modal(state);
                        }

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let export_btn = egui::Button::new(
                                egui::RichText::new("Export  →").size(14.0).strong(),
                            )
                            .min_size(egui::vec2(130.0, 36.0))
                            .fill(egui::Color32::from_rgb(0, 120, 215));

                            if ui.add(export_btn).clicked() {
                                // Let user pick output file
                                let dialog = rfd::FileDialog::new()
                                    .set_title("Save exported video")
                                    .add_filter("MP4 Video", &["mp4"])
                                    .add_filter("WebM Video", &["webm"])
                                    .add_filter("GIF Animation", &["gif"])
                                    .set_file_name("output.mp4");

                                if let Some(path) = dialog.save_file() {
                                    state.export_output_path = Some(path);
                                    start_export(state);
                                    state.export_modal_step = 1;
                                }
                            }
                        });
                    });
                });
        });
    });
}

// ─── Step 1: Export progress ─────────────────────────────────────────────────

fn draw_export_step(
    ui: &mut egui::Ui,
    _ctx: &egui::Context,
    state: &mut AppState,
    screen_rect: egui::Rect,
) {
    let width = 560.0;
    let height = 440.0;
    let center = screen_rect.center();
    let rect = egui::Rect::from_center_size(center, egui::vec2(width, height));

    ui.allocate_ui_at_rect(rect, |ui| {
        ui.push_id("export_progress_modal", |ui| {
            egui::Frame::window(ui.style())
                .fill(egui::Color32::from_rgb(28, 28, 32))
                .inner_margin(28.0)
                .rounding(14.0)
                .shadow(egui::epaint::Shadow {
                    extrusion: 40.0,
                    color: egui::Color32::BLACK,
                })
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_white_alpha(25)))
                .show(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(4.0);
                        let title = if state.export_ffmpeg_done {
                            if state.export_ffmpeg_error.is_some() {
                                "❌  Export Failed"
                            } else {
                                "✅  Export Complete"
                            }
                        } else {
                            "⏳  Exporting…"
                        };
                        ui.label(
                            egui::RichText::new(title)
                                .size(20.0)
                                .strong()
                                .color(egui::Color32::WHITE),
                        );
                        if !state.export_ffmpeg_done {
                            ui.add_space(6.0);
                            ui.label(
                                egui::RichText::new("Rendering frames and encoding with ffmpeg…")
                                    .size(13.0)
                                    .color(egui::Color32::from_white_alpha(140)),
                            );
                        }
                    });

                    ui.add_space(14.0);
                    ui.separator();
                    ui.add_space(8.0);

                    // Progress bar
                    if !state.export_ffmpeg_done {
                        let progress = if state.export_frames_total > 0 {
                            state.export_frames_done as f32 / state.export_frames_total as f32
                        } else {
                            0.0
                        };
                        let label = if state.export_frames_total > 0
                            && state.export_frames_done < state.export_frames_total
                        {
                            format!(
                                "Rendering frame {}/{}",
                                state.export_frames_done, state.export_frames_total
                            )
                        } else if state.export_frames_done >= state.export_frames_total
                            && state.export_frames_total > 0
                        {
                            "Encoding with ffmpeg…".to_string()
                        } else {
                            "Starting…".to_string()
                        };
                        ui.add(
                            egui::ProgressBar::new(progress)
                                .text(label)
                                .animate(true)
                                .desired_width(ui.available_width()),
                        );
                        ui.add_space(8.0);
                    }

                    // Log output
                    egui::ScrollArea::vertical()
                        .max_height(240.0)
                        .auto_shrink([false; 2])
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            for line in &state.export_ffmpeg_log {
                                let color = if line.starts_with("❌") {
                                    egui::Color32::from_rgb(255, 100, 100)
                                } else if line.starts_with("✅") {
                                    egui::Color32::from_rgb(100, 220, 100)
                                } else {
                                    egui::Color32::from_white_alpha(180)
                                };
                                ui.label(
                                    egui::RichText::new(line)
                                        .size(11.5)
                                        .color(color)
                                        .monospace(),
                                );
                            }
                        });

                    ui.add_space(16.0);

                    // Bottom buttons
                    ui.horizontal(|ui| {
                        if state.export_ffmpeg_done {
                            let label = if state.export_ffmpeg_error.is_some() {
                                "Close"
                            } else {
                                "Done"
                            };

                            if ui
                                .add(
                                    egui::Button::new(egui::RichText::new(label).size(14.0))
                                        .min_size(egui::vec2(100.0, 36.0)),
                                )
                                .clicked()
                            {
                                close_modal(state);
                            }

                            // "Open folder" shortcut when successful
                            if state.export_ffmpeg_error.is_none() {
                                if let Some(out_path) = &state.export_output_path.clone() {
                                    if let Some(parent) = out_path.parent() {
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                if ui
                                                    .add(
                                                        egui::Button::new(
                                                            egui::RichText::new("📂  Open folder")
                                                                .size(13.0),
                                                        )
                                                        .min_size(egui::vec2(130.0, 36.0)),
                                                    )
                                                    .clicked()
                                                {
                                                    let _ = open_folder(parent);
                                                }
                                            },
                                        );
                                    }
                                }
                            }
                        } else {
                            // Cancel (not yet supported mid-flight — just closes UI)
                            if ui
                                .add(
                                    egui::Button::new(egui::RichText::new("Cancel").size(14.0))
                                        .min_size(egui::vec2(100.0, 36.0)),
                                )
                                .clicked()
                            {
                                // Drop the receiver — background thread will finish but we won't wait.
                                state.export_ffmpeg_rx = None;
                                close_modal(state);
                            }
                        }
                    });
                });
        });
    });
}

// ─── Export logic ─────────────────────────────────────────────────────────────

fn start_export(state: &mut AppState) {
    // Reset state
    state.export_ffmpeg_log.clear();
    state.export_ffmpeg_done = false;
    state.export_ffmpeg_error = None;
    state.export_frames_done = 0;
    state.export_frames_total = 0;

    let output_path = match &state.export_output_path {
        Some(p) => p.clone(),
        None => return,
    };

    // Temp dir lives next to the project so it uses the same drive (fast I/O)
    let project_dir = state
        .project_path
        .clone()
        .unwrap_or_else(|| std::env::temp_dir());
    let frames_dir = project_dir.join("tempdir");

    let fps = state.export_modal_fps;
    let width = state.export_modal_width;
    let height = state.export_modal_height;
    let duration = state.export_modal_duration;
    let scene = state.scene.clone();
    let dsl_handlers = state.dsl_event_handlers.clone();
    let font_arc_cache = state.font_arc_cache.clone();
    let font_map = state.font_map.clone();

    let (tx, rx) = std::sync::mpsc::channel::<FfmpegMsg>();
    state.export_ffmpeg_rx = Some(rx);

    std::thread::spawn(move || {
        let total_frames = (duration * fps as f32).ceil() as u32;
        let _ = tx.send(FfmpegMsg::Log(format!(
            "Starting export: {}×{} @ {}fps — {} frames",
            width, height, fps, total_frames
        )));
        let _ = tx.send(FfmpegMsg::Progress(0, total_frames));

        // ── Create / clean temp frames directory ───────────────────────────
        if frames_dir.exists() {
            // Remove old frames from a previous export
            let _ = std::fs::remove_dir_all(&frames_dir);
        }
        if let Err(e) = std::fs::create_dir_all(&frames_dir) {
            let _ = tx.send(FfmpegMsg::Error(format!(
                "Failed to create frames dir {}: {}",
                frames_dir.display(),
                e
            )));
            return;
        }
        let _ = tx.send(FfmpegMsg::Log(format!(
            "Frames dir: {}",
            frames_dir.display()
        )));

        // ── Initialise headless wgpu device for GPU rendering ──────────────
        #[cfg(feature = "wgpu")]
        let gpu = {
            let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
            let adapter =
                pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    ..Default::default()
                }));
            match adapter {
                Some(a) => {
                    match pollster::block_on(a.request_device(
                        &wgpu::DeviceDescriptor {
                            label: Some("export_device"),
                            required_features: wgpu::Features::empty(),
                            required_limits: wgpu::Limits::default(),
                        },
                        None,
                    )) {
                        Ok((device, queue)) => {
                            let target_format = wgpu::TextureFormat::Rgba8UnormSrgb;
                            let resources =
                                crate::canvas::gpu::GpuResources::new(&device, target_format);
                            let _ =
                                tx.send(FfmpegMsg::Log("GPU renderer initialised.".to_string()));
                            Some((
                                std::sync::Arc::new(device),
                                std::sync::Arc::new(queue),
                                resources,
                            ))
                        }
                        Err(e) => {
                            let _ = tx.send(FfmpegMsg::Log(format!(
                                "GPU init failed ({}), falling back to CPU.",
                                e
                            )));
                            None
                        }
                    }
                }
                None => {
                    let _ = tx.send(FfmpegMsg::Log(
                        "No GPU adapter found, falling back to CPU.".to_string(),
                    ));
                    None
                }
            }
        };
        #[cfg(not(feature = "wgpu"))]
        let gpu: Option<()> = None;

        // ── Render each frame ──────────────────────────────────────────────
        let snap_base = crate::canvas::preview_worker::RenderSnapshot {
            scene: scene.clone(),
            dsl_event_handlers: dsl_handlers.clone(),
            render_width: width,
            render_height: height,
            preview_multiplier: 1.0,
            duration_secs: duration,
            preview_fps: fps,
            use_gpu: false,
            font_arc_cache: font_arc_cache.clone(),
            #[cfg(feature = "wgpu")]
            wgpu_render_state: None,
        };

        #[cfg(feature = "wgpu")]
        let mut gpu_resources = gpu;

        for frame_idx in 0..total_frames {
            let t = frame_idx as f32 / fps as f32;

            // Render frame via GPU or CPU fallback
            #[cfg(feature = "wgpu")]
            let img_result: Result<egui::ColorImage, String> = {
                if let Some((ref device, ref queue, ref mut resources)) = gpu_resources {
                    // GPU path: renders circles/rects on GPU, text via CPU text layer, then composite
                    let gpu_img = crate::canvas::gpu::render_frame_color_image_gpu_snapshot(
                        device, queue, resources, &snap_base, t,
                    );

                    // Composite text layer on top (CPU → blend into GPU pixels)
                    match gpu_img {
                        Ok(mut img) => {
                            let mut font_arc_cache_local = font_arc_cache.clone();
                            if let Some(text_layer) =
                                crate::canvas::text_rasterizer::rasterize_text_layer(
                                    &scene,
                                    width,
                                    height,
                                    t,
                                    duration,
                                    &mut font_arc_cache_local,
                                    &font_map,
                                    &dsl_handlers,
                                    0.0,
                                )
                            {
                                composite_text_layer(&mut img, &text_layer);
                            }
                            Ok(img)
                        }
                        Err(e) => Err(e),
                    }
                } else {
                    Ok(render_frame_cpu(
                        &snap_base,
                        t,
                        &font_arc_cache,
                        &font_map,
                        &dsl_handlers,
                    ))
                }
            };
            #[cfg(not(feature = "wgpu"))]
            let img_result: Result<egui::ColorImage, String> = Ok(render_frame_cpu(
                &snap_base,
                t,
                &font_arc_cache,
                &font_map,
                &dsl_handlers,
            ));

            let img = match img_result {
                Ok(i) => i,
                Err(e) => {
                    let _ = tx.send(FfmpegMsg::Error(format!(
                        "GPU render failed at frame {}: {}",
                        frame_idx, e
                    )));
                    return;
                }
            };

            // Save as PNG
            let frame_path = frames_dir.join(format!("frame_{:06}.png", frame_idx));
            if let Err(e) = save_png(&img, &frame_path) {
                let _ = tx.send(FfmpegMsg::Error(format!(
                    "Failed to save frame {}: {}",
                    frame_idx, e
                )));
                return;
            }

            let done = frame_idx + 1;
            let _ = tx.send(FfmpegMsg::Progress(done, total_frames));
            // Log every ~10% of frames
            let log_interval = (total_frames / 10).max(1);
            if done % log_interval == 0 || done == total_frames {
                let _ = tx.send(FfmpegMsg::Log(format!(
                    "  Frame {}/{} ({:.1}s)",
                    done, total_frames, t
                )));
            }
        }

        let _ = tx.send(FfmpegMsg::Log(
            "All frames rendered. Running ffmpeg…".to_string(),
        ));
        let _ = tx.send(FfmpegMsg::Progress(total_frames, total_frames));

        // ── Run ffmpeg ──────────────────────────────────────────────────────
        let input_pattern = frames_dir.join("frame_%06d.png");

        let ext = output_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("mp4")
            .to_lowercase();

        let mut cmd = std::process::Command::new("ffmpeg");
        cmd.arg("-y")
            .arg("-framerate")
            .arg(fps.to_string())
            .arg("-i")
            .arg(&input_pattern);

        match ext.as_str() {
            "gif" => {
                cmd.args([
                    "-vf",
                    &format!(
                        "fps={},scale={}:{}:flags=lanczos,split[s0][s1];[s0]palettegen[p];[s1][p]paletteuse",
                        fps, width, height
                    ),
                    "-loop", "0",
                ]);
            }
            "webm" => {
                cmd.args([
                    "-c:v",
                    "libvpx-vp9",
                    "-b:v",
                    "0",
                    "-crf",
                    "30",
                    "-pix_fmt",
                    "yuva420p",
                ]);
            }
            _ => {
                // mp4 default
                cmd.args([
                    "-c:v",
                    "libx264",
                    "-pix_fmt",
                    "yuv420p",
                    "-crf",
                    "18",
                    "-preset",
                    "fast",
                    "-movflags",
                    "+faststart",
                ]);
                let vf = format!("scale={}:{}", round_even(width), round_even(height));
                cmd.arg("-vf").arg(&vf);
            }
        }

        cmd.arg(&output_path);
        cmd.stderr(std::process::Stdio::piped());
        cmd.stdout(std::process::Stdio::piped());

        let _ = tx.send(FfmpegMsg::Log(format!("ffmpeg: {:?}", cmd)));

        match cmd.output() {
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                for line in stderr.lines().take(80) {
                    let _ = tx.send(FfmpegMsg::Log(line.to_string()));
                }
                if output.status.success() {
                    let _ = tx.send(FfmpegMsg::Log(format!(
                        "Output saved to: {}",
                        output_path.display()
                    )));
                    // Clean up temp frames
                    let _ = std::fs::remove_dir_all(&frames_dir);
                    let _ = tx.send(FfmpegMsg::Done);
                } else {
                    let code = output.status.code().unwrap_or(-1);
                    let _ = tx.send(FfmpegMsg::Error(format!(
                        "ffmpeg exited with code {}. Check ffmpeg is installed and in PATH.",
                        code
                    )));
                }
            }
            Err(e) => {
                let _ = tx.send(FfmpegMsg::Error(format!(
                    "Failed to launch ffmpeg: {}. Make sure ffmpeg is installed and in PATH.",
                    e
                )));
            }
        }
    });
}

// ─── Text compositing helper ──────────────────────────────────────────────────

/// Blends a CPU-rasterized text layer (RGBA) on top of a GPU-rendered ColorImage.
fn composite_text_layer(
    base: &mut egui::ColorImage,
    text: &crate::canvas::text_rasterizer::TextRasterResult,
) {
    let w = base.size[0];
    let h = base.size[1];
    // text.pixels is RGBA flat vec, same resolution
    let pixel_count = (w * h) as usize;
    for i in 0..pixel_count.min(text.pixels.len() / 4) {
        let ta = text.pixels[i * 4 + 3];
        if ta == 0 {
            continue;
        }
        let tr = text.pixels[i * 4];
        let tg = text.pixels[i * 4 + 1];
        let tb = text.pixels[i * 4 + 2];
        let alpha = ta as f32 / 255.0;
        let inv = 1.0 - alpha;
        let dst = base.pixels[i].to_array();
        base.pixels[i] = egui::Color32::from_rgba_premultiplied(
            (tr as f32 * alpha + dst[0] as f32 * inv) as u8,
            (tg as f32 * alpha + dst[1] as f32 * inv) as u8,
            (tb as f32 * alpha + dst[2] as f32 * inv) as u8,
            255,
        );
    }
}

// ─── CPU frame renderer ───────────────────────────────────────────────────────

fn render_frame_cpu(
    snap: &crate::canvas::preview_worker::RenderSnapshot,
    time: f32,
    font_arc_cache: &std::collections::HashMap<String, ab_glyph::FontArc>,
    font_map: &std::collections::HashMap<String, std::path::PathBuf>,
    dsl_handlers: &[crate::dsl::runtime::DslHandler],
) -> egui::ColorImage {
    use crate::animations::animations_manager::animated_xy_for;
    use crate::scene::Shape;

    let w = snap.render_width as usize;
    let h = snap.render_height as usize;
    let mut pixels = vec![[255u8, 255, 255, 255]; w * h];

    fn collect_prims(shapes: &[Shape], parent_spawn: f32, out: &mut Vec<(Shape, f32)>) {
        for s in shapes {
            let my_spawn = s.spawn_time().max(parent_spawn);
            match s {
                Shape::Group { children, .. } => collect_prims(children, my_spawn, out),
                _ => out.push((s.clone(), my_spawn)),
            }
        }
    }

    let mut all = Vec::new();
    collect_prims(&snap.scene, 0.0, &mut all);
    // Reverse so that scene index 0 (top of scene graph) paints last = on top.
    all.reverse();

    let width_f = w as f32;
    let height_f = h as f32;

    for (shape, spawn) in &all {
        if time < *spawn {
            continue;
        }

        let (ax, ay) = animated_xy_for(shape, time, snap.duration_secs);

        match shape {
            Shape::Circle(c) => {
                let cx = ax * width_f;
                let cy = ay * height_f;
                let r = c.radius * width_f;
                let r2 = r * r;
                let min_x = ((cx - r).floor() as isize).max(0) as usize;
                let max_x = ((cx + r).ceil() as isize).min(w as isize - 1) as usize;
                let min_y = ((cy - r).floor() as isize).max(0) as usize;
                let max_y = ((cy + r).ceil() as isize).min(h as isize - 1) as usize;

                for py in min_y..=max_y {
                    for px in min_x..=max_x {
                        let dx = px as f32 + 0.5 - cx;
                        let dy = py as f32 + 0.5 - cy;
                        if dx * dx + dy * dy <= r2 {
                            let alpha = c.color[3] as f32 / 255.0;
                            let dst = &mut pixels[py * w + px];
                            blend(dst, &c.color, alpha);
                        }
                    }
                }
            }
            Shape::Rect(rc) => {
                let cx = ax * width_f;
                let cy = ay * height_f;
                let rw = rc.w * width_f;
                let rh = rc.h * height_f;
                let x0 = ((cx - rw * 0.5).floor() as isize).max(0) as usize;
                let x1 = ((cx + rw * 0.5).ceil() as isize).min(w as isize - 1) as usize;
                let y0 = ((cy - rh * 0.5).floor() as isize).max(0) as usize;
                let y1 = ((cy + rh * 0.5).ceil() as isize).min(h as isize - 1) as usize;

                let alpha = rc.color[3] as f32 / 255.0;
                for py in y0..=y1 {
                    for px in x0..=x1 {
                        let dst = &mut pixels[py * w + px];
                        blend(dst, &rc.color, alpha);
                    }
                }
            }
            Shape::Text(..) => {
                // Text is handled via the rasterize_text_layer pass below
            }
            Shape::Group { .. } => {} // already flattened
        }
    }

    // Composite text layer (uses the same rasterizer as the live preview)
    let mut font_arc_cache_local = font_arc_cache.clone();
    if let Some(text_layer) = crate::canvas::text_rasterizer::rasterize_text_layer(
        &snap.scene,
        snap.render_width,
        snap.render_height,
        time,
        snap.duration_secs,
        &mut font_arc_cache_local,
        font_map,
        dsl_handlers,
        0.0,
    ) {
        // Blend text pixels (RGBA) on top
        let pixel_count = w * h;
        for i in 0..pixel_count.min(text_layer.pixels.len() / 4) {
            let ta = text_layer.pixels[i * 4 + 3];
            if ta == 0 {
                continue;
            }
            let src = [
                text_layer.pixels[i * 4],
                text_layer.pixels[i * 4 + 1],
                text_layer.pixels[i * 4 + 2],
                ta,
            ];
            let alpha = ta as f32 / 255.0;
            blend(&mut pixels[i], &src, alpha);
        }
    }

    let flat: Vec<u8> = pixels.iter().flat_map(|p| *p).collect();
    egui::ColorImage::from_rgba_unmultiplied([w, h], &flat)
}

fn blend(dst: &mut [u8; 4], src: &[u8; 4], alpha: f32) {
    let inv = 1.0 - alpha;
    dst[0] = (src[0] as f32 * alpha + dst[0] as f32 * inv) as u8;
    dst[1] = (src[1] as f32 * alpha + dst[1] as f32 * inv) as u8;
    dst[2] = (src[2] as f32 * alpha + dst[2] as f32 * inv) as u8;
    dst[3] = 255;
}

// ─── PNG save ─────────────────────────────────────────────────────────────────

fn save_png(
    img: &egui::ColorImage,
    path: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let w = img.size[0] as u32;
    let h = img.size[1] as u32;
    let flat: Vec<u8> = img.pixels.iter().flat_map(|p| p.to_array()).collect();
    image::save_buffer(path, &flat, w, h, image::ColorType::Rgba8)?;
    Ok(())
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn round_even(n: u32) -> u32 {
    if n % 2 == 0 {
        n
    } else {
        n + 1
    }
}

fn open_folder(path: &std::path::Path) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer").arg(path).spawn()?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(path).spawn()?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open").arg(path).spawn()?;
    }
    Ok(())
}

fn close_modal(state: &mut AppState) {
    state.show_export_modal = false;
    state.export_modal_step = 0;
    state.export_ffmpeg_log.clear();
    state.export_ffmpeg_rx = None;
    state.export_ffmpeg_done = false;
    state.export_ffmpeg_error = None;
    state.export_output_path = None;
    state.export_frames_done = 0;
    state.export_frames_total = 0;
}

//! Gestiona hilos de trabajo en segundo plano para generar previsualizaciones.
//! Se encarga de la generación de snapshots (CPU o GPU) sin bloquear el hilo principal.

use super::cache_management::{
    compress_color_image_to_png, enforce_preview_cache_limits, preview_cache_vram_bytes,
};
#[cfg(feature = "wgpu")]
use super::gpu::{
    render_frame_color_image_gpu_snapshot, // used to produce CPU previews when necessary
    render_frame_native_texture,
    GpuResources,
};
use crate::app_state::AppState;
use eframe::egui;
use std::sync::mpsc;
use std::thread;

// no buffering logic remains; remove unused constant

// `PreviewMode` has been retired; we now only ever request single-frame
// previews.  The type remains in history for documentation but is no longer
// referenced by compiled code.

pub enum PreviewJob {
    Generate {
        center_time: f32,
        snapshot: RenderSnapshot,
    },
}

pub enum PreviewResult {
    Single(f32, egui::ColorImage),
    Native(f32, wgpu::Texture),
    // Buffered mode removed; only single-frame results are produced now.
}

#[derive(Clone)]
pub struct RenderSnapshot {
    pub scene: Vec<crate::shapes::element_store::ElementKeyframes>,
    pub dsl: crate::states::dslstate::DslState,
    pub render_width: u32,
    pub render_height: u32,
    pub preview_multiplier: f32,
    pub duration_secs: f32,
    #[cfg(feature = "wgpu")]
    pub wgpu_render_state: Option<eframe::egui_wgpu::RenderState>,
    pub preview_fps: u32,
    pub font_arc_cache: std::collections::HashMap<String, ab_glyph::FontArc>,
    pub scene_version: u32,
}

pub fn request_preview_frames(state: &mut AppState, center_time: f32) {
    ensure_preview_worker(state);
    if state.preview_job_pending {
        return;
    }

    if let Some(tx) = &state.preview_worker_tx {
        let snap = RenderSnapshot {
            scene: state.scene.clone(),
            dsl: state.dsl.clone(),
            render_width: state.render_width,
            render_height: state.render_height,
            preview_multiplier: state.preview_multiplier,
            duration_secs: state.duration_secs,
            preview_fps: state.preview_fps,
            font_arc_cache: state.font_arc_cache.clone(),
            scene_version: state.scene_version,
            #[cfg(feature = "wgpu")]
            wgpu_render_state: state.wgpu_render_state.clone(),
        };
        let job = PreviewJob::Generate {
            center_time,
            snapshot: snap,
        };
        state.preview_job_pending = true;
        let _ = tx.send(job);
    }
}

pub fn poll_preview_results(state: &mut AppState, ctx: &egui::Context) {
    if let Some(rx) = &state.preview_worker_rx {
        let mut needs_enforce = false;

        let vram_limit_bytes = if state.prefer_vram_cache && state.estimated_vram_bytes > 0 {
            (state.estimated_vram_bytes as f32 * state.vram_cache_max_percent) as usize
        } else {
            usize::MAX
        };

        let current_vram_usage = preview_cache_vram_bytes(state);
        let vram_available = vram_limit_bytes.saturating_sub(current_vram_usage);

        while let Ok(result) = rx.try_recv() {
            state.preview_job_pending = false;
            match result {
                PreviewResult::Native(t, tex) => {
                    #[cfg(feature = "wgpu")]
                    if let Some(render_state) = &state.wgpu_render_state {
                        // Free previous texture if any
                        if let Some(old_id) = state.preview_native_texture_id {
                            render_state.renderer.write().free_texture(&old_id);
                        }

                        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
                        let id = render_state.renderer.write().register_native_texture(
                            &render_state.device,
                            &view,
                            wgpu::FilterMode::Nearest,
                        );
                        state.preview_native_texture_id = Some(id);
                        state.preview_native_texture_resource = Some(tex);
                        state.preview_texture = None; // Clear CPU texture to signal usage of native one
                        state.preview_cache_center_time = Some(t);
                    }
                }
                PreviewResult::Single(t, img) => {
                    // If we already have a cached preview for the same center time,
                    // avoid replacing the texture (prevents an unnecessary swap/flicker).
                    if state
                        .preview_cache_center_time
                        .is_some_and(|c| (c - t).abs() < 1e-6)
                    {
                        // still ensure cache limits are correct but skip reload
                        continue;
                    }

                    let img_size = img.size[0] * img.size[1] * 4;
                    let use_vram = state.prefer_vram_cache
                        && (vram_available >= img_size || state.preview_worker_use_gpu);

                    // Load new center texture and update caches atomically where possible
                    let handle = ctx.load_texture(
                        "preview_center",
                        img.clone(),
                        egui::TextureOptions::NEAREST,
                    );
                    state.preview_texture = Some(handle);
                    state.preview_cache_center_time = Some(t);

                    if use_vram {
                        let tex_name = format!("preview_cached_{:.6}", t);
                        let th =
                            ctx.load_texture(&tex_name, img.clone(), egui::TextureOptions::NEAREST);
                        state
                            .preview_texture_cache
                            .retain(|(tt, _h, _s)| (tt - t).abs() > 1e-6);
                        state.preview_texture_cache.push((t, th, img_size));
                    } else if state.compress_preview_cache {
                        if let Some(bytes) = compress_color_image_to_png(&img) {
                            state
                                .preview_compressed_cache
                                .retain(|(tt, _b, _s)| (tt - t).abs() > 1e-6);
                            state.preview_compressed_cache.push((
                                t,
                                bytes,
                                (img.size[0], img.size[1]),
                            ));
                        } else {
                            state
                                .preview_frame_cache
                                .retain(|(tt, _)| (tt - t).abs() > 1e-6);
                            state.preview_frame_cache.push((t, img.clone()));
                        }
                    } else {
                        state
                            .preview_frame_cache
                            .retain(|(tt, _)| (tt - t).abs() > 1e-6);
                        state.preview_frame_cache.push((t, img.clone()));
                    }

                    needs_enforce = true;
                } // Buffered mode removed; we only ever produce a single frame now.
            }
            // Request repaint after state changes have been applied (reduces mid-update flashes)
            ctx.request_repaint();
        }
        if needs_enforce {
            enforce_preview_cache_limits(state, ctx);
        }
    }
}

pub fn ensure_preview_worker(state: &mut AppState) {
    if state.preview_worker_tx.is_some() && state.preview_worker_rx.is_some() {
        return;
    }

    let (job_tx, job_rx) = mpsc::channel::<PreviewJob>();
    let (res_tx, res_rx) = mpsc::channel::<PreviewResult>();

    thread::spawn(move || {
        #[cfg(feature = "wgpu")]
        let mut gpu_renderer: Option<(
            std::sync::Arc<wgpu::Device>,
            std::sync::Arc<wgpu::Queue>,
            GpuResources,
        )> = None;

        while let Ok(job) = job_rx.recv() {
            // `PreviewJob` currently has only the `Generate` variant, so this
            // pattern is guaranteed to match.  silence the clippy warning.
            #[allow(irrefutable_let_patterns)]
            if let PreviewJob::Generate {
                center_time,
                snapshot,
                ..
            } = job
            {
                #[cfg(feature = "wgpu")]
                {
                    // CAPTURAMOS EL ESTADO DE LA UI SI EXISTE
                    if let Some(render_state) = &snapshot.wgpu_render_state {
                        let device = &render_state.device;
                        let queue = &render_state.queue;

                        // Reutilizamos o creamos recursos localmente
                        if gpu_renderer.is_none() {
                            gpu_renderer = Some((
                                device.clone(),
                                queue.clone(),
                                GpuResources::new(device, render_state.target_format),
                            ));
                        }

                        if let Some((ref device, ref queue, ref mut resources)) = gpu_renderer {
                            // ¡NATIVO! Renderizamos a textura sin bajar a RAM
                            if let Ok(tex) = render_frame_native_texture(
                                device,
                                queue,
                                resources,
                                &snapshot,
                                center_time,
                            ) {
                                let _ = res_tx.send(PreviewResult::Native(center_time, tex));
                            }
                        }
                    } else {
                        // Fallback a descarga (antiguo método) si no hay estado compartido
                        // (Opcional: podríamos quitarlo si confiamos 100% en el compartido)
                    }
                }
                // also always attempt to render a colour image snapshot so that the
                // `PreviewResult::Single` case is exercised.  This keeps the existing
                // texture caching logic in `poll_preview_results` working even when
                // only CPU images are available.
                #[cfg(feature = "wgpu")]
                if let Some((ref device, ref queue, ref mut resources)) = gpu_renderer {
                    if let Ok(img) = render_frame_color_image_gpu_snapshot(
                        device,
                        queue,
                        resources,
                        &snapshot,
                        center_time,
                    ) {
                        let _ = res_tx.send(PreviewResult::Single(center_time, img.clone()));
                    }
                }
            }
        }
    });

    state.preview_worker_tx = Some(job_tx);
    state.preview_worker_rx = Some(res_rx);
}

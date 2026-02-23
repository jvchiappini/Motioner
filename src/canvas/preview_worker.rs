//! Gestiona hilos de trabajo en segundo plano para generar previsualizaciones.
//! Se encarga de la generación de snapshots (CPU o GPU) sin bloquear el hilo principal.

// cache_management is no longer needed for GPU-only previews; remove import
#[cfg(feature = "wgpu")]
use super::gpu::{render_frame_native_texture, GpuResources};
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
    /// GPU-native texture result.  CPU snapshots are no longer produced for
    /// previews; we render directly to a wgpu::Texture and send that back.
    Native(f32, wgpu::Texture),
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
        // CPU caching and VRAM accounting are no longer relevant with GPU-only
        // previews.  We drop the old bookkeeping variables entirely.

        while let Ok(result) = rx.try_recv() {
            state.preview_job_pending = false;
            // the enum now has only one variant; destructure directly
            let PreviewResult::Native(_t, tex) = result;
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
                // CPU texture field is left untouched; we never write it.
            }
            // Request repaint after state changes have been applied (reduces mid-update flashes)
            ctx.request_repaint();
        }
        // no cache enforcement needed for GPU-only previews
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
                // Previously we also generated a CPU snapshot and sent
                // `PreviewResult::Single`.  That path has been removed – previews are
                // always GPU-native now, so we don't waste cycles copying data back to
                // the CPU.
            }
        }
    });

    state.preview_worker_tx = Some(job_tx);
    state.preview_worker_rx = Some(res_rx);
}

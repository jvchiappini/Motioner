//! Gestiona hilos de trabajo en segundo plano para generar previsualizaciones.
//! Se encarga de la generación de snapshots (CPU o GPU) sin bloquear el hilo principal.

// cache_management is no longer needed for GPU-only previews; remove import
#[cfg(feature = "wgpu")]
use super::gpu::{render_frame_native_texture, GpuResources};
use crate::app_state::AppState;
use eframe::egui;
use std::sync::mpsc;
use std::thread;

// previously we only generated a single preview frame; now we render a small
// batch of frames around the request and keep them cached in VRAM.

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
    pub render_width: u32,
    pub render_height: u32,
    pub preview_multiplier: f32,
    pub duration_secs: f32,
    #[cfg(feature = "wgpu")]
    pub wgpu_render_state: Option<eframe::egui_wgpu::RenderState>,
    pub preview_fps: u32,
    pub font_arc_cache: std::collections::HashMap<String, ab_glyph::FontArc>,
    pub font_map: std::collections::HashMap<String, std::path::PathBuf>,
    pub scene_version: u32,
}

pub fn request_preview_frames(state: &mut AppState, center_time: f32) {
    ensure_preview_worker(state);
    // if we already have a cached texture for this time, reuse it immediately
    if let Some((_, id, tex_arc)) = state
        .preview_gpu_cache
        .iter()
        .find(|(tt, _, _)| (tt - center_time).abs() < 1e-6)
    {
        // bump to the cache head by swapping into native_texture fields
        state.preview_native_texture_id = Some(*id);
        #[cfg(feature = "wgpu")]
        {
            state.preview_native_texture_resource = Some(tex_arc.clone());
        }
        return;
    }

    if state.preview_job_pending {
        return;
    }

    if let Some(tx) = &state.preview_worker_tx {
        let snap = RenderSnapshot {
            scene: state.scene.clone(),
            render_width: state.render_width,
            render_height: state.render_height,
            preview_multiplier: state.preview_multiplier,
            duration_secs: state.duration_secs,
            preview_fps: state.preview_fps,
            font_arc_cache: state.font_arc_cache.clone(),
            font_map: state.font_map.clone(),
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
            let PreviewResult::Native(t, tex) = result;
            #[cfg(feature = "wgpu")]
            if let Some(render_state) = &state.wgpu_render_state {
                // Free previous texture if any
                if let Some(old_id) = state.preview_native_texture_id {
                    render_state.renderer.write().free_texture(&old_id);
                }

                // wrap texture in Arc so we can keep multiple copies alive
                let tex_arc = std::sync::Arc::new(tex);
                let view = tex_arc.create_view(&wgpu::TextureViewDescriptor::default());
                let id = render_state.renderer.write().register_native_texture(
                    &render_state.device,
                    &view,
                    wgpu::FilterMode::Nearest,
                );
                state.preview_native_texture_id = Some(id);
                state.preview_native_texture_resource = Some(tex_arc.clone());

                // insert into GPU cache if not already present
                if !state
                    .preview_gpu_cache
                    .iter()
                    .any(|(tt, _, _)| (tt - t).abs() < 1e-6)
                {
                    state.preview_gpu_cache.push((t, id, tex_arc.clone()));
                }
                // simple eviction policy: keep at most 10 entries
                const MAX_GPU_CACHE_FRAMES: usize = 10;
                if state.preview_gpu_cache.len() > MAX_GPU_CACHE_FRAMES {
                    // remove entry farthest from current time
                    if let Some(idx) = state
                        .preview_gpu_cache
                        .iter()
                        .enumerate()
                        .map(|(i, (tt, _, _))| (i, (tt - t).abs()))
                        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
                        .map(|(i, _)| i)
                    {
                        let (_tt, old_id, _old_tex) = state.preview_gpu_cache.remove(idx);
                        render_state.renderer.write().free_texture(&old_id);
                    }
                }
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
                            // We pre‑render a small batch of frames around the
                            // requested time so that scrubbing forward/backward
                            // can hit the GPU cache.  The radius is measured in
                            // preview frames; e.g. 2 means centre±2 frames.
                            let radius: i32 = 2;
                            let frame_idx = crate::shapes::element_store::seconds_to_frame(
                                center_time,
                                snapshot.preview_fps,
                            );
                            for off in -radius..=radius {
                                let idx = frame_idx as i32 + off;
                                if idx < 0 {
                                    continue;
                                }
                                let t = (idx as f32) / snapshot.preview_fps as f32;
                                if t < 0.0 || t > snapshot.duration_secs {
                                    continue;
                                }
                                if let Ok(tex) = render_frame_native_texture(
                                    device, queue, resources, &snapshot, t,
                                ) {
                                    let _ = res_tx.send(PreviewResult::Native(t, tex));
                                }
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

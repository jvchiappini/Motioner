use super::resources::GpuResources;
use super::types::*;
use ab_glyph::FontArc;
/// Implementa la lógica de computación para interpolar keyframes directamente en la GPU.
/// Esto evita el muestreo excesivo en la CPU y mejora el rendimiento drásticamente.
#[cfg(feature = "wgpu")]
use eframe::wgpu;

/// El código WGSL del shader de computación para interpolar keyframes.
pub const COMPUTE_WGSL: &str = include_str!("../../shaders/compute_keyframes.wgsl");

#[cfg(feature = "wgpu")]
impl GpuResources {
    /// Sube los keyframes de la escena a los buffers de computación y dispara el dispatch.
    /// Al finalizar, `shape_buffer` contendrá los datos actualizados para el frame solicitado.
    #[allow(clippy::too_many_arguments)]
    pub fn dispatch_compute(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        scene: &[crate::shapes::element_store::ElementKeyframes],
        current_frame: u32,
        fps: u32,
        render_width: u32,
        render_height: u32,
        upload_keyframes: bool,
        text_overrides: Option<&std::collections::HashMap<usize, [f32; 4]>>,
        font_map: &std::collections::HashMap<String, std::path::PathBuf>,
        font_arc_cache: &mut std::collections::HashMap<String, FontArc>,
    ) {
        let element_count = scene.len() as u32;
        if element_count == 0 {
            return;
        }

        if upload_keyframes {
            let estimated_kf = scene.len() * 5 * 4;
            let mut all_keyframes: Vec<GpuKeyframe> = Vec::with_capacity(estimated_kf);
            let mut descs: Vec<GpuElementDesc> = Vec::with_capacity(scene.len());
            // glyph buffer for text shapes
            let mut all_glyphs: Vec<GpuGlyph> = Vec::new();

            // track the final combined atlas dimensions after processing all
            // text elements; both width and height are needed to normalise UVs
            // in their respective axes.
            let mut final_combined_w: u32 = 0;
            let mut final_combined_h: u32 = 0;
            // Construir descriptores en orden de dibujado (inverso)
            for (scene_idx, ek) in scene.iter().enumerate().rev() {
                let mut desc = GpuElementDesc {
                    x_offset: 0,
                    x_len: 0,
                    y_offset: 0,
                    y_len: 0,
                    radius_offset: 0,
                    radius_len: 0,
                    w_offset: 0,
                    w_len: 0,
                    h_offset: 0,
                    h_len: 0,
                    shape_type: match ek.kind.as_str() {
                        "circle" => 0,
                        "rect" => 1,
                        "text" => 2,
                        _ => 0,
                    },
                    spawn_frame: ek.spawn_frame as u32,
                    kill_frame: ek.kill_frame.map(|f| f as u32).unwrap_or(0xFFFF_FFFF),
                    r_offset: 0,
                    g_offset: 0,
                    b_offset: 0,
                    a_offset: 0,
                    r_len: 0,
                    g_len: 0,
                    b_len: 0,
                    a_len: 0,
                    glyph_offset: 0,
                    glyph_len: 0,
                    _pad: 0,
                    base_size: [0.0, 0.0],
                    uv0: [0.0, 0.0],
                    uv1: [0.0, 0.0],
                };

                // Handle text glyphs: compute sequence of glyph metadata and
                // record offsets in desc.p1/p2 (reusing these fields).
                if ek.kind == "text" {
                    // recover current shape to access text value, font, size
                    if let Some(shape) =
                        ek.to_shape_at_frame(current_frame.try_into().unwrap(), fps)
                    {
                        if let crate::scene::Shape::Text(t) = shape {
                            // convert the text element's `size` field into a
                            // pixel extent so that the compute shader can size
                            // the quad correctly.  This mirrors the CPU path
                            // where the quad was square with side = size_px.
                            let actual_size_px = if t.size > 1.0 {
                                t.size
                            } else {
                                t.size * render_height as f32
                            };
                            let mut total_string_width = 0.0_f32;
                            // Ensure atlas(es) for this text (and spans) exist and
                            // upload combined atlas if newly created.
                            let mut new_atlas_pixels: Option<Vec<u8>> = None;
                            let mut combined_w = 0;
                            let mut combined_h = 0;
                            // list of (offset_x, metrics_map) per span or main text
                            let mut glyph_runs: Vec<(
                                u32,
                                Vec<crate::canvas::gpu::types::GpuGlyph>,
                            )> = Vec::new();

                            // helper to process a string with given font/size/color
                            let mut process_run =
                                |text: &str,
                                 font_name: Option<&str>,
                                 size_frac: f32,
                                 color: [u8; 4],
                                 total_string_width: &mut f32| {
                                    // The DSL exposes `size` as either a fraction of the
                                    // canvas height (preferred) or an absolute pixel
                                    // value.  Historically this confusion caused the
                                    // extremely large atlas dimensions observed by the
                                    // user when they wrote `size = 24.0` instead of
                                    // `size = 0.024`.  To be forgiving we interpret any
                                    // `size_frac > 1.0` as a pixel count.
                                    let real_size_px = if size_frac > 1.0 {
                                        size_frac
                                    } else {
                                        size_frac * render_height as f32
                                    };
                                    let mut size_px = real_size_px;
                                    // Determine a conservative upper bound for glyph size
                                    // such that an ASCII atlas (16 columns, 8 rows)
                                    // will always fit within the maximum texture
                                    // dimension supported by the GPU.  We derived this by
                                    // observing that `combined_w = cols * cell_w` and
                                    // `combined_h = rows * glyph_h`; choosing cols = 16 and
                                    // rows = 8 gives the tightest width constraint,
                                    // therefore we limit `size_px` accordingly.
                                    let max_glyph_px =
                                        (super::utils::MAX_GPU_TEXTURE_SIZE as f32) / 16.0;
                                    if size_px > max_glyph_px {
                                        size_px = max_glyph_px;
                                    }
                                    // If the evaluated pixel size is zero (this can
                                    // happen when a shape's size track contains a
                                    // zero value or when the caller mistakenly sets
                                    // `size = 0`), skip the run entirely rather than
                                    // generating spurious empty glyph lists.  A text
                                    // element with zero size should not render anyway.
                                    if size_px <= 0.0 {
                                        return;
                                    }
                                    let res =
                                        crate::canvas::text_rasterizer::ensure_glyph_atlas_gpu(
                                            font_name,
                                            size_px,
                                            font_map,
                                            font_arc_cache,
                                            self,
                                            device,
                                            queue,
                                        );
                                    // always update combined dims; upload pixels only if new
                                    combined_w = res.combined_width;
                                    combined_h = res.combined_height;
                                    if res.is_new {
                                        if let Some(pix) = &res.combined_pixels {
                                            new_atlas_pixels = Some(pix.clone());
                                            /*eprintln!(
                                                "[gpu::compute] uploaded new atlas {}x{} for font {:?}"
                                                , combined_w, combined_h, font_name
                                            );*/
                                        }
                                    }
                                    // compute normalized advances & uv coords with offset_x
                                    // compute un‑normalised UVs in *pixel* coordinates and
                                    // collect glyph entries. Normalisation will be applied
                                    // below once the final atlas width is known.
                                    let mut run: Vec<crate::canvas::gpu::types::GpuGlyph> =
                                        Vec::new();
                                    let mut total_adv = 0.0f32;
                                    for ch in text.chars() {
                                        if let Some(m) = res.atlas.metrics.get(&ch) {
                                            total_adv += m.advance;
                                        }
                                    }
                                    *total_string_width += total_adv * (real_size_px / size_px);
                                    if total_adv > 0.0 {
                                        let color_f = [
                                            super::utils::srgb_to_linear(color[0]),
                                            super::utils::srgb_to_linear(color[1]),
                                            super::utils::srgb_to_linear(color[2]),
                                            color[3] as f32 / 255.0,
                                        ];
                                        for ch in text.chars() {
                                            if let Some(m) = res.atlas.metrics.get(&ch) {
                                                // compute uv0/uv1 in atlas pixel coordinates
                                                // (horizontal offset added, vertical scaled by
                                                // the atlas height).  We'll normalise both
                                                // axes by the final combined dimensions later.
                                                let uv0_px = [
                                                    m.uv0[0] * (res.atlas.width as f32)
                                                        + res.offset_x as f32,
                                                    m.uv0[1] * (res.atlas.height as f32),
                                                ];
                                                let uv1_px = [
                                                    m.uv1[0] * (res.atlas.width as f32)
                                                        + res.offset_x as f32,
                                                    m.uv1[1] * (res.atlas.height as f32),
                                                ];
                                                run.push(crate::canvas::gpu::types::GpuGlyph {
                                                    uv0: uv0_px,
                                                    uv1: uv1_px,
                                                    advance: m.advance / total_adv,
                                                    _pad_align_0: 0.0,
                                                    _pad_align_1: 0.0,
                                                    _pad_align_2: 0.0,
                                                    color: color_f,
                                                    _pad: [0.0; 4],
                                                });
                                            }
                                        }
                                    }
                                    glyph_runs.push((res.offset_x, run));
                                };

                            if t.spans.is_empty() {
                                process_run(
                                    &t.value,
                                    if t.font == "System" || t.font.is_empty() {
                                        None
                                    } else {
                                        Some(t.font.as_str())
                                    },
                                    t.size,
                                    t.color,
                                    &mut total_string_width,
                                );
                            } else {
                                for span in &t.spans {
                                    process_run(
                                        &span.text,
                                        if span.font == "System" || span.font.is_empty() {
                                            None
                                        } else {
                                            Some(span.font.as_str())
                                        },
                                        span.size,
                                        span.color,
                                        &mut total_string_width,
                                    );
                                }
                            }

                            desc.base_size = [total_string_width / 2.0, actual_size_px / 2.0];

                            // remember latest combined dimensions
                            final_combined_w = combined_w;
                            final_combined_h = combined_h;
                            // if atlas updated, push to GPU
                            if let Some(pix) = new_atlas_pixels {
                                self.update_text_atlas(device, queue, &pix, combined_w, combined_h);
                            }

                            // flatten runs into metrics vector
                            let mut metrics: Vec<crate::canvas::gpu::types::GpuGlyph> = Vec::new();
                            for (_off, mut run) in glyph_runs {
                                if run.is_empty() {
                                    /*eprintln!(
                                        "[gpu::compute] glyph run empty for text {:?}, font {:?} size_frac {}",
                                        t.value, t.font, t.size
                                    );*/
                                }
                                metrics.append(&mut run);
                            }
                            let offset = all_glyphs.len() as u32;
                            let len = metrics.len() as u32;
                            desc.glyph_offset = offset;
                            desc.glyph_len = len;
                            all_glyphs.extend(metrics);
                        }
                    }
                }

                if let Some(overrides) = text_overrides {
                    if let Some(uvs) = overrides.get(&scene_idx) {
                        desc.uv0 = [uvs[0], uvs[1]];
                        desc.uv1 = [uvs[2], uvs[3]];
                    }
                }

                (desc.x_offset, desc.x_len) =
                    self.push_track_helper(&mut all_keyframes, &ek.x, 1.0);
                (desc.y_offset, desc.y_len) =
                    self.push_track_helper(&mut all_keyframes, &ek.y, 1.0);
                (desc.radius_offset, desc.radius_len) =
                    self.push_track_helper(&mut all_keyframes, &ek.radius, 1.0);
                (desc.w_offset, desc.w_len) =
                    self.push_track_helper(&mut all_keyframes, &ek.w, 1.0);
                (desc.h_offset, desc.h_len) =
                    self.push_track_helper(&mut all_keyframes, &ek.h, 1.0);

                // Tracks de color (convertidos a lineal 0..1)
                (desc.r_offset, desc.r_len) =
                    self.push_color_track_helper(&mut all_keyframes, &ek.color, 0);
                (desc.g_offset, desc.g_len) =
                    self.push_color_track_helper(&mut all_keyframes, &ek.color, 1);
                (desc.b_offset, desc.b_len) =
                    self.push_color_track_helper(&mut all_keyframes, &ek.color, 2);
                (desc.a_offset, desc.a_len) =
                    self.push_color_track_helper(&mut all_keyframes, &ek.color, 3);

                // Comandos de Movimiento (100% GPU)
                // move commands are no longer stored – x/y tracks already
                // contain all positional keyframes, so there is nothing special to
                // push here.

                descs.push(desc);
            }

            // Subida de keyframes
            let kf_bytes = bytemuck::cast_slice::<GpuKeyframe, u8>(&all_keyframes);
            if kf_bytes.len() as u64 > self.keyframe_buffer.size() {
                self.keyframe_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("keyframe_buffer"),
                    size: (kf_bytes.len() * 2 + 64) as u64,
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                self.rebuild_compute_bind_group(device);
            }
            queue.write_buffer(&self.keyframe_buffer, 0, kf_bytes);

            // no move buffer upload required

            // Subida de descriptores
            let desc_bytes = bytemuck::cast_slice::<GpuElementDesc, u8>(&descs);
            if desc_bytes.len() as u64 > self.element_desc_buffer.size() {
                self.element_desc_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("element_desc_buffer"),
                    size: (desc_bytes.len() * 2 + 64) as u64,
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                self.rebuild_compute_bind_group(device);
            }
            queue.write_buffer(&self.element_desc_buffer, 0, desc_bytes);

            // before we convert the glyph vector to bytes we need to normalize
            // the accumulated UV coordinates using the final atlas width.
            if final_combined_w > 0 {
                // normalise both axes by the final combined dimensions
                for g in &mut all_glyphs {
                    g.uv0[0] /= final_combined_w as f32;
                    g.uv1[0] /= final_combined_w as f32;
                    g.uv0[1] /= final_combined_h as f32;
                    g.uv1[1] /= final_combined_h as f32;
                }
                // debug: show first few glyphs UVs (should be normalized 0..1)
                if !all_glyphs.is_empty() {
                    /*eprintln!(
                        "[gpu::compute] first glyphs after normalisation: {:?}",
                        &all_glyphs[..all_glyphs.len().min(5)]
                    );*/
                }
            }

            // upload glyph buffer
            let glyph_bytes = bytemuck::cast_slice::<GpuGlyph, u8>(&all_glyphs);
            if glyph_bytes.len() as u64 > self.glyph_buffer.size() {
                /*eprintln!(
                    "[gpu::compute] resizing glyph_buffer from {} to {} entries",
                    self.glyph_buffer.size() / std::mem::size_of::<GpuGlyph>() as u64,
                    glyph_bytes.len()
                );*/
                // the glyph buffer has to grow.  we were only rebuilding the
                // *compute* bind group previously, but the same buffer is also
                // consumed by the render pipeline (binding 4).  if we leave the
                // old bind group alive the fragment shader will continue to
                // point at the stale / undersized buffer and the glyph metadata
                // will be garbage, which is exactly what manifested as “text
                // shapes rendered as empty rectangles” in the UI.  rebuilding
                // both groups ensures the new buffer is visible everywhere.
                self.glyph_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("glyph_buffer"),
                    size: (glyph_bytes.len() * 2 + 64) as u64,
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                self.rebuild_compute_bind_group(device);
                // also update the render bind group so the fragment shader sees
                // the resized buffer.
                self.rebuild_render_bind_group(device);
            }
            queue.write_buffer(&self.glyph_buffer, 0, glyph_bytes);
            // dump the first few glyph entries so we can verify that the
            // colour/uv data is sane.  This log is cheap and will help
            // diagnose cases where the buffer ends up empty or all-white.
            if !all_glyphs.is_empty() {
                /*eprintln!(
                    "[gpu::compute] first glyphs uploaded: {:?}",
                    &all_glyphs[..all_glyphs.len().min(5)]
                );*/
            }

            // NOTE: we normalise the UVs *before* uploading above. the
            // previous version of this code contained a second identical
            // normalisation block here which mutated `all_glyphs` after the
            // write.  that was harmless in the short term, but it confused
            // future readers and risked double-normalisation if the vector
            // were reused later.  remove the duplicate logic to keep things
            // simple.
        }

        // Redimensionar shape_buffer si es necesario
        let shape_bytes_needed = (element_count as usize * std::mem::size_of::<GpuShape>()) as u64;
        if shape_bytes_needed > self.shape_buffer.size() {
            self.shape_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("shape_buffer"),
                size: shape_bytes_needed * 2 + 1024,
                usage: wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_DST
                    | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });
            self.rebuild_compute_bind_group(device);
            self.rebuild_render_bind_group(device);
        }

        // Uniforms de computación
        let cu_u32: [u32; 4] = [current_frame, fps, element_count, 0];
        queue.write_buffer(&self.compute_uniform_buffer, 0, bytemuck::bytes_of(&cu_u32));
        let res_f32: [f32; 2] = [render_width as f32, render_height as f32];
        queue.write_buffer(
            &self.compute_uniform_buffer,
            16,
            bytemuck::bytes_of(&res_f32),
        );

        // Dispatch
        let workgroups = element_count.div_ceil(64);
        {
            let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("keyframe_interpolation"),
                timestamp_writes: None,
            });
            cpass.set_pipeline(&self.compute_pipeline);
            cpass.set_bind_group(0, &self.compute_bind_group, &[]);
            cpass.dispatch_workgroups(workgroups, 1, 1);
        }
    }

    fn push_track_helper(
        &self,
        all: &mut Vec<GpuKeyframe>,
        track: &[crate::shapes::element_store::Keyframe<f32>],
        scale: f32,
    ) -> (u32, u32) {
        let offset = all.len() as u32;
        for kf in track {
            all.push(GpuKeyframe {
                frame: kf.frame as u32,
                value: kf.value * scale,
                easing: super::utils::easing_to_gpu(&kf.easing),
                _pad: 0,
            });
        }
        (offset, track.len() as u32)
    }

    fn push_color_track_helper(
        &self,
        all: &mut Vec<GpuKeyframe>,
        track: &[crate::shapes::element_store::Keyframe<[u8; 4]>],
        channel_idx: usize,
    ) -> (u32, u32) {
        let offset = all.len() as u32;
        for kf in track {
            let val = if channel_idx < 3 {
                super::utils::srgb_to_linear(kf.value[channel_idx])
            } else {
                kf.value[channel_idx] as f32 / 255.0
            };
            all.push(GpuKeyframe {
                frame: kf.frame as u32,
                value: val,
                easing: super::utils::easing_to_gpu(&kf.easing),
                _pad: 0,
            });
        }
        (offset, track.len() as u32)
    }
}

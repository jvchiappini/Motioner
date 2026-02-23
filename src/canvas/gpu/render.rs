//! Implementa la integración del renderizado GPU con egui y el sistema de previsualización.
//! Contiene el callback de pintura y funciones para generar snapshots.

use super::resources::GpuResources;
use super::types::*;
use super::utils::MAX_GPU_TEXTURE_SIZE;
use ab_glyph::FontArc;
#[cfg(feature = "wgpu")]
use eframe::{egui, egui_wgpu, wgpu};

#[cfg(feature = "wgpu")]
pub struct CompositionCallback {
    pub render_width: f32,
    pub render_height: f32,
    pub preview_multiplier: f32,
    pub paper_rect: egui::Rect,
    pub viewport_rect: egui::Rect,
    pub magnifier_pos: Option<egui::Pos2>,
    pub time: f32,
    pub elements: Option<Vec<crate::shapes::element_store::ElementKeyframes>>,
    pub font_map: std::collections::HashMap<String, std::path::PathBuf>,
    pub font_arc_cache: std::collections::HashMap<String, FontArc>,
    pub current_frame: u32,
    pub fps: u32,
    pub scene_version: u32,
}

#[cfg(feature = "wgpu")]
impl egui_wgpu::CallbackTrait for CompositionCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        screen_descriptor: &egui_wgpu::ScreenDescriptor,
        _egui_encoder: &mut wgpu::CommandEncoder,
        callback_resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let resources: &mut GpuResources = callback_resources.get_mut().unwrap();

        if let Some(ref elements) = self.elements {
            let dirty = self.scene_version > resources.current_scene_version;

            resources.dispatch_compute(
                device,
                queue,
                _egui_encoder,
                elements,
                self.current_frame,
                self.fps,
                self.render_width as u32,
                self.render_height as u32,
                dirty,
                None,
                &self.font_map,
                &mut self.font_arc_cache.clone(),
            );
            if dirty {
                resources.current_scene_version = self.scene_version;
            }
        }

        let mag_active = if self.magnifier_pos.is_some() {
            1.0
        } else {
            0.0
        };
        let m_pos = self.magnifier_pos.unwrap_or(egui::Pos2::ZERO);
        let count = self.elements.as_ref().map(|el| el.len()).unwrap_or(0);

        let uniforms = Uniforms {
            resolution: [self.render_width, self.render_height],
            preview_res: [
                self.render_width * self.preview_multiplier,
                self.render_height * self.preview_multiplier,
            ],
            paper_rect: [
                self.paper_rect.min.x,
                self.paper_rect.min.y,
                self.paper_rect.max.x,
                self.paper_rect.max.y,
            ],
            viewport_rect: [
                self.viewport_rect.min.x,
                self.viewport_rect.min.y,
                self.viewport_rect.max.x,
                self.viewport_rect.max.y,
            ],
            count: count as f32,
            mag_x: m_pos.x,
            mag_y: m_pos.y,
            mag_active,
            time: self.time,
            pixels_per_point: screen_descriptor.pixels_per_point,
            gamma_correction: if format!("{:?}", resources.target_format).contains("Srgb") {
                0.0
            } else {
                1.0
            },
            _pad: 0.0,
        };

        queue.write_buffer(&resources.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
        Vec::new()
    }

    fn paint<'a>(
        &'a self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'a>,
        callback_resources: &'a egui_wgpu::CallbackResources,
    ) {
        let resources: &GpuResources = callback_resources.get().unwrap();
        let count = self.elements.as_ref().map(|el| el.len()).unwrap_or(0) as u32;

        if count > 0 {
            render_pass.set_pipeline(&resources.pipeline);
            render_pass.set_bind_group(0, &resources.bind_group, &[]);
            render_pass.draw(0..6, 0..count);
        }
    }
}

/// Renderiza un frame y lo devuelve como egui::ColorImage (readback de GPU a CPU).
#[cfg(feature = "wgpu")]
pub fn render_frame_color_image_gpu_snapshot(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    resources: &mut GpuResources,
    snap: &crate::canvas::preview_worker::RenderSnapshot,
    time: f32,
    clear_color: wgpu::Color,
) -> Result<egui::ColorImage, String> {
    let mut preview_w = (snap.render_width as f32 * snap.preview_multiplier)
        .round()
        .max(1.0) as u32;
    let mut preview_h = (snap.render_height as f32 * snap.preview_multiplier)
        .round()
        .max(1.0) as u32;

    if preview_w > MAX_GPU_TEXTURE_SIZE || preview_h > MAX_GPU_TEXTURE_SIZE {
        let scale = (MAX_GPU_TEXTURE_SIZE as f32 / preview_w as f32)
            .min(MAX_GPU_TEXTURE_SIZE as f32 / preview_h as f32);
        preview_w = (preview_w as f32 * scale).round() as u32;
        preview_h = (preview_h as f32 * scale).round() as u32;
    }

    let frame_idx = crate::shapes::element_store::seconds_to_frame(time, snap.preview_fps);
    let mut compute_encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("compute_keyframes"),
    });
    let dirty = snap.scene_version > resources.current_scene_version;

    // dispatch compute; it will handle glyph atlases internally
    resources.dispatch_compute(
        device,
        queue,
        &mut compute_encoder,
        &snap.scene,
        frame_idx as u32,
        snap.preview_fps,
        snap.render_width,
        snap.render_height,
        dirty,
        None,
        &snap.font_map,
        &mut snap.font_arc_cache.clone(),
    );
    if dirty {
        resources.current_scene_version = snap.scene_version;
    }
    queue.submit(Some(compute_encoder.finish()));

    let render_w = snap.render_width;
    let render_h = snap.render_height;

    // Guard against absurdly large dimensions coming from the caller (for
    // example, a corrupted project file or an unbounded export request).  The
    // GPU has hard limits on texture sizes and attempting to create a texture
    // beyond those limits results in a validation error that crashes the app
    // (see panic above).  Instead we report an error back to the caller so it
    // can decide how to handle it.
    if render_w == 0 || render_h == 0 {
        return Err("requested snapshot has zero width or height".to_string());
    }
    if render_w > MAX_GPU_TEXTURE_SIZE || render_h > MAX_GPU_TEXTURE_SIZE {
        return Err(format!(
            "requested snapshot size {}x{} exceeds GPU limit {}",
            render_w, render_h, MAX_GPU_TEXTURE_SIZE
        ));
    }
    let uniforms = Uniforms {
        resolution: [render_w as f32, render_h as f32],
        preview_res: [preview_w as f32, preview_h as f32],
        paper_rect: [0.0, 0.0, render_w as f32, render_h as f32],
        viewport_rect: [0.0, 0.0, render_w as f32, render_h as f32],
        count: snap.scene.len() as f32,
        mag_x: 0.0,
        mag_y: 0.0,
        mag_active: 0.0,
        time,
        pixels_per_point: 1.0,
        gamma_correction: if format!("{:?}", resources.target_format).contains("Srgb") {
            0.0
        } else {
            1.0
        },
        _pad: 0.0,
    };
    queue.write_buffer(&resources.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

    // Compute the number of bytes per row required for the readback.  The
    // original implementation worked purely in `u32`, which could overflow in
    // debug builds when the user requested a very large snapshot (for
    // example, exporting a 100k×100k canvas).  We convert to `u64` and use
    // checked arithmetic to avoid panics; if we do overflow we simply abort
    // the snapshot with a descriptive error rather than crashing the whole
    // application.

    let bytes_per_pixel = 4u64;
    let unpadded_bpr = (render_w as u64).checked_mul(bytes_per_pixel).ok_or_else(|| {
        "render width too large when calculating bytes per row".to_string()
    })?;
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as u64;
    // align up to the copy bytes-per-row requirement
    let padded_bpr = ((unpadded_bpr + align - 1) / align) * align;
    let staging_size = padded_bpr
        .checked_mul(render_h as u64)
        .ok_or_else(|| "render height too large when computing staging buffer".to_string())?;

    if resources.readback_size != [render_w, render_h] {
        // The readback texture must use the same format as the pipeline we created
        // earlier (stored in `resources.target_format`).  Before this change we
        // hard-coded `Rgba8UnormSrgb` which could differ from the swapchain
        // format and lead to a validation error when the pipeline was used
        // against the render pass (see panic reported by users).
        //
        // Using `resources.target_format` here ensures the render pass and
        // pipeline remain compatible regardless of the surface format chosen by
        // wgpu/egui on the host platform.
        resources.readback_render_texture = Some(device.create_texture(&wgpu::TextureDescriptor {
            label: Some("preview_readback_texture"),
            size: wgpu::Extent3d {
                width: render_w,
                height: render_h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: resources.target_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        }));
        resources.readback_staging_buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("preview_staging_buffer"),
            size: staging_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        }));
        resources.readback_size = [render_w, render_h];
        resources.readback_pixel_buf = Vec::with_capacity((render_w * render_h * 4) as usize);
    }

    let render_texture = resources.readback_render_texture.as_ref().unwrap();
    let staging_buffer = resources.readback_staging_buffer.as_ref().unwrap();
    let render_view = render_texture.create_view(&wgpu::TextureViewDescriptor::default());

    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    {
        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &render_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(clear_color),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        });
        rpass.set_pipeline(&resources.pipeline);
        rpass.set_bind_group(0, &resources.bind_group, &[]);
        if !snap.scene.is_empty() {
            rpass.draw(0..6, 0..snap.scene.len() as u32);
        }
    }
    // `padded_bpr` must be passed back to wgpu as a `u32`.  We already
    // performed overflow checking above, so this conversion should never
    // fail; however, do a checked cast just to keep the compiler happy and
    // produce a reasonable error message in the unlikely event we hit the
    // 32‑bit limit.
    let padded_bpr_u32: u32 = padded_bpr
        .try_into()
        .map_err(|_| "bytes-per-row exceeds 32-bit limit".to_string())?;

    encoder.copy_texture_to_buffer(
        wgpu::ImageCopyTexture {
            texture: render_texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::ImageCopyBuffer {
            buffer: staging_buffer,
            layout: wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(padded_bpr_u32),
                rows_per_image: Some(render_h),
            },
        },
        wgpu::Extent3d {
            width: render_w,
            height: render_h,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(Some(encoder.finish()));

    let slice = staging_buffer.slice(..);
    // We'll capture the mapping result so we can detect failures instead of
    // blindly calling `get_mapped_range` which panics when the slice isn't
    // actually mapped.
    let map_result: std::sync::Arc<std::sync::Mutex<Option<Result<(), wgpu::BufferAsyncError>>>> =
        std::sync::Arc::new(std::sync::Mutex::new(None));
    {
        let caller_thread = std::thread::current();
        let map_result = map_result.clone();
        slice.map_async(wgpu::MapMode::Read, move |res| {
            *map_result.lock().unwrap() = Some(res);
            caller_thread.unpark();
        });
    }
    device.poll(wgpu::Maintain::Poll);
    std::thread::park_timeout(std::time::Duration::from_secs(5));

    // check mapping outcome
    let mapped_ok = map_result.lock().unwrap().take();
    if mapped_ok != Some(Ok(())) {
        return Err("failed to map staging buffer for readback".to_string());
    }

    {
        let mapped = slice.get_mapped_range();
        resources.readback_pixel_buf.clear();
        for row in 0..render_h {
            // perform the row arithmetic in u64 to avoid overflow
            let start = (row as u64 * padded_bpr) as usize;
            let end = start + (unpadded_bpr as usize);
            resources
                .readback_pixel_buf
                .extend_from_slice(&mapped[start..end]);
        }
    }
    staging_buffer.unmap();

    Ok(egui::ColorImage::from_rgba_unmultiplied(
        [render_w as usize, render_h as usize],
        &resources.readback_pixel_buf,
    ))
}

/// Renderiza un frame directamente a una wgpu::Texture.
#[cfg(feature = "wgpu")]
pub fn render_frame_native_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    resources: &mut GpuResources,
    snap: &crate::canvas::preview_worker::RenderSnapshot,
    time: f32,
) -> anyhow::Result<wgpu::Texture> {
    let preview_w = (snap.render_width as f32 * snap.preview_multiplier).round() as u32;
    let preview_h = (snap.render_height as f32 * snap.preview_multiplier).round() as u32;
    let frame_idx = crate::shapes::element_store::seconds_to_frame(time, snap.preview_fps);

    // simple compute dispatch; glyph atlas updates are handled internally
    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    let dirty = snap.scene_version > resources.current_scene_version;
    resources.dispatch_compute(
        device,
        queue,
        &mut encoder,
        &snap.scene,
        frame_idx as u32,
        snap.preview_fps,
        snap.render_width,
        snap.render_height,
        dirty,
        None,
        &snap.font_map,
        &mut snap.font_arc_cache.clone(),
    );
    if dirty {
        resources.current_scene_version = snap.scene_version;
    }
    queue.submit(Some(encoder.finish()));

    let uniforms = Uniforms {
        resolution: [snap.render_width as f32, snap.render_height as f32],
        preview_res: [preview_w as f32, preview_h as f32],
        paper_rect: [0.0, 0.0, preview_w as f32, preview_h as f32],
        viewport_rect: [0.0, 0.0, preview_w as f32, preview_h as f32],
        count: snap.scene.len() as f32,
        mag_x: 0.0,
        mag_y: 0.0,
        mag_active: 0.0,
        time,
        pixels_per_point: 1.0,
        gamma_correction: if format!("{:?}", resources.target_format).contains("Srgb") {
            0.0
        } else {
            1.0
        },
        _pad: 0.0,
    };
    queue.write_buffer(&resources.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("gpu_cache_texture"),
        size: wgpu::Extent3d {
            width: preview_w,
            height: preview_h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: resources.target_format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    {
        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        });
        rpass.set_pipeline(&resources.pipeline);
        rpass.set_bind_group(0, &resources.bind_group, &[]);
        if !snap.scene.is_empty() {
            rpass.draw(0..6, 0..snap.scene.len() as u32);
        }
    }
    queue.submit(Some(encoder.finish()));
    Ok(texture)
}

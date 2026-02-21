/// Implementa la integración del renderizado GPU con egui y el sistema de previsualización.
/// Contiene el callback de pintura y funciones para generar snapshots.

#[cfg(feature = "wgpu")]
use eframe::{egui, egui_wgpu, wgpu};
use super::types::*;
use super::resources::GpuResources;
use super::utils::MAX_GPU_TEXTURE_SIZE;

#[cfg(feature = "wgpu")]
pub struct CompositionCallback {
    pub render_width: f32,
    pub render_height: f32,
    pub preview_multiplier: f32,
    pub paper_rect: egui::Rect,
    pub viewport_rect: egui::Rect,
    pub magnifier_pos: Option<egui::Pos2>,
    pub time: f32,
    pub shared_device: Option<std::sync::Arc<wgpu::Device>>,
    pub shared_queue: Option<std::sync::Arc<wgpu::Queue>>,
    pub text_pixels: Option<(Vec<u8>, u32, u32)>,
    pub elements: Option<Vec<crate::shapes::element_store::ElementKeyframes>>,
    pub current_frame: u32,
    pub fps: u32,
    pub scene_version: u32,
    pub text_overrides: Option<Vec<(usize, [f32; 4])>>,
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
            
            // Convertir overrides de Vec a HashMap para el despachador de computación
            let mut overrides_map = std::collections::HashMap::new();
            if let Some(ref overrides) = self.text_overrides {
                for (idx, uvs) in overrides {
                    overrides_map.insert(*idx, *uvs);
                }
            }

            resources.dispatch_compute(
                device, queue, _egui_encoder, elements,
                self.current_frame, self.fps,
                self.render_width as u32, self.render_height as u32,
                dirty,
                Some(&overrides_map),
            );
            if dirty { resources.current_scene_version = self.scene_version; }

            if let Some((ref pixels, w, h)) = self.text_pixels {
                resources.update_text_atlas(device, queue, pixels, w, h);
            }
        }

        let mag_active = if self.magnifier_pos.is_some() { 1.0 } else { 0.0 };
        let m_pos = self.magnifier_pos.unwrap_or(egui::Pos2::ZERO);
        let count = self.elements.as_ref().map(|el| el.len()).unwrap_or(0);
        
        let uniforms = Uniforms {
            resolution: [self.render_width, self.render_height],
            preview_res: [self.render_width * self.preview_multiplier, self.render_height * self.preview_multiplier],
            paper_rect: [self.paper_rect.min.x, self.paper_rect.min.y, self.paper_rect.max.x, self.paper_rect.max.y],
            viewport_rect: [self.viewport_rect.min.x, self.viewport_rect.min.y, self.viewport_rect.max.x, self.viewport_rect.max.y],
            count: count as f32,
            mag_x: m_pos.x, mag_y: m_pos.y, mag_active,
            time: self.time,
            pixels_per_point: screen_descriptor.pixels_per_point,
            gamma_correction: if format!("{:?}", resources.target_format).contains("Srgb") { 0.0 } else { 1.0 },
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
) -> Result<egui::ColorImage, String> {
    let mut preview_w = (snap.render_width as f32 * snap.preview_multiplier).round().max(1.0) as u32;
    let mut preview_h = (snap.render_height as f32 * snap.preview_multiplier).round().max(1.0) as u32;

    if preview_w > MAX_GPU_TEXTURE_SIZE || preview_h > MAX_GPU_TEXTURE_SIZE {
        let scale = (MAX_GPU_TEXTURE_SIZE as f32 / preview_w as f32).min(MAX_GPU_TEXTURE_SIZE as f32 / preview_h as f32);
        preview_w = (preview_w as f32 * scale).round() as u32;
        preview_h = (preview_h as f32 * scale).round() as u32;
    }

    let frame_idx = crate::shapes::element_store::seconds_to_frame(time, snap.preview_fps);
    let mut text_entries_local: Vec<(usize, crate::scene::Shape, f32)> = Vec::new();
    for (scene_idx, ek) in snap.scene.iter().enumerate() {
        if frame_idx < ek.spawn_frame { continue; }
        if let Some(kf) = ek.kill_frame { if frame_idx >= kf { continue; } }

        if let Some(shape) = ek.to_shape_at_frame(frame_idx, snap.preview_fps) {
            if shape.descriptor().map_or(false, |d| d.dsl_keyword() == "text") {
                text_entries_local.push((scene_idx, shape.clone(), (ek.spawn_frame as f32 / snap.preview_fps as f32).max(0.0)));
            }
        }
    }

    let mut compute_encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("compute_keyframes") });
    let dirty = snap.scene_version > resources.current_scene_version;
    
    // Primero preparamos los overrides de texto para el compute shader
    let mut overrides_map = std::collections::HashMap::new();
    let rw = snap.render_width;
    let rh = snap.render_height;
    let mut atlas_buf = Vec::new();

    if !text_entries_local.is_empty() {
        let atlas_h = rh * text_entries_local.len() as u32;
        atlas_buf = vec![0u8; (rw * atlas_h * 4) as usize];

        for (tile_idx, (scene_idx, shape, parent_spawn)) in text_entries_local.iter().enumerate() {
            if let Some(result) = crate::canvas::text_rasterizer::rasterize_single_text(
                shape, rw, rh, time, snap.duration_secs,
                &mut snap.font_arc_cache.clone(), &std::collections::HashMap::new(),
                &snap.dsl.event_handlers, *parent_spawn,
            ) {
                let row_offset = (tile_idx as u32 * rh * rw * 4) as usize;
                atlas_buf[row_offset..row_offset + (rw * rh * 4) as usize].copy_from_slice(&result.pixels);

                let uv0_y = tile_idx as f32 / text_entries_local.len() as f32;
                let uv1_y = (tile_idx + 1) as f32 / text_entries_local.len() as f32;
                overrides_map.insert(*scene_idx, [0.0, uv0_y, 1.0, uv1_y]);
            }
        }
        resources.update_text_atlas(device, queue, &atlas_buf, rw, atlas_h);
    }

    // Ahora ejecutamos el compute shader con los UVs ya calculados
    resources.dispatch_compute(
        device, queue, &mut compute_encoder, &snap.scene,
        frame_idx as u32, snap.preview_fps, snap.render_width, snap.render_height,
        dirty,
        Some(&overrides_map),
    );
    if dirty { resources.current_scene_version = snap.scene_version; }
    queue.submit(Some(compute_encoder.finish()));

    let render_w = snap.render_width;
    let render_h = snap.render_height;
    let uniforms = Uniforms {
        resolution: [render_w as f32, render_h as f32],
        preview_res: [preview_w as f32, preview_h as f32],
        paper_rect: [0.0, 0.0, render_w as f32, render_h as f32],
        viewport_rect: [0.0, 0.0, render_w as f32, render_h as f32],
        count: snap.scene.len() as f32,
        mag_x: 0.0, mag_y: 0.0, mag_active: 0.0,
        time,
        pixels_per_point: 1.0,
        gamma_correction: if format!("{:?}", resources.target_format).contains("Srgb") { 0.0 } else { 1.0 },
        _pad: 0.0,
    };
    queue.write_buffer(&resources.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

    let bytes_per_pixel = 4u32;
    let unpadded_bpr = render_w * bytes_per_pixel;
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let padded_bpr = (unpadded_bpr + align - 1) / align * align;
    let staging_size = (padded_bpr * render_h) as u64;

    if resources.readback_size != [render_w, render_h] {
        resources.readback_render_texture = Some(device.create_texture(&wgpu::TextureDescriptor {
            label: Some("preview_readback_texture"),
            size: wgpu::Extent3d { width: render_w, height: render_h, depth_or_array_layers: 1 },
            mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
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

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    {
        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &render_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None, occlusion_query_set: None, timestamp_writes: None,
        });
        rpass.set_pipeline(&resources.pipeline);
        rpass.set_bind_group(0, &resources.bind_group, &[]);
        if !snap.scene.is_empty() { rpass.draw(0..6, 0..snap.scene.len() as u32); }
    }
    encoder.copy_texture_to_buffer(
        wgpu::ImageCopyTexture { texture: &render_texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        wgpu::ImageCopyBuffer { buffer: &staging_buffer, layout: wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(padded_bpr), rows_per_image: Some(render_h) } },
        wgpu::Extent3d { width: render_w, height: render_h, depth_or_array_layers: 1 },
    );
    queue.submit(Some(encoder.finish()));

    let slice = staging_buffer.slice(..);
    let caller_thread = std::thread::current();
    slice.map_async(wgpu::MapMode::Read, move |_r| { caller_thread.unpark(); });
    device.poll(wgpu::Maintain::Poll);
    std::thread::park_timeout(std::time::Duration::from_secs(5));

    {
        let mapped = slice.get_mapped_range();
        resources.readback_pixel_buf.clear();
        for row in 0..render_h {
            let start = (row * padded_bpr) as usize;
            let end = start + unpadded_bpr as usize;
            resources.readback_pixel_buf.extend_from_slice(&mapped[start..end]);
        }
    }
    staging_buffer.unmap();

    Ok(egui::ColorImage::from_rgba_unmultiplied([render_w as usize, render_h as usize], &resources.readback_pixel_buf))
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

    let mut text_entries_local: Vec<(usize, crate::scene::Shape, f32)> = Vec::new();
    for (scene_idx, ek) in snap.scene.iter().enumerate() {
        if frame_idx < ek.spawn_frame { continue; }
        if let Some(kf) = ek.kill_frame { if frame_idx >= kf { continue; } }
        if let Some(shape) = ek.to_shape_at_frame(frame_idx, snap.preview_fps) {
            if shape.descriptor().map_or(false, |d| d.dsl_keyword() == "text") {
                text_entries_local.push((scene_idx, shape.clone(), (ek.spawn_frame as f32 / snap.preview_fps as f32).max(0.0)));
            }
        }
    }

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    let dirty = snap.scene_version > resources.current_scene_version;
    
    // Preparar UVs para el compute shader
    let mut overrides_map = std::collections::HashMap::new();
    let rw = snap.render_width;
    let rh = snap.render_height;

    if !text_entries_local.is_empty() {
        let atlas_h = rh * text_entries_local.len() as u32;
        let mut atlas = vec![0u8; (rw * atlas_h * 4) as usize];

        for (tile_idx, (scene_idx, shape, parent_spawn)) in text_entries_local.iter().enumerate() {
            if let Some(result) = crate::canvas::text_rasterizer::rasterize_single_text(
                shape, rw, rh, time, snap.duration_secs,
                &mut snap.font_arc_cache.clone(), &std::collections::HashMap::new(),
                &snap.dsl.event_handlers, *parent_spawn,
            ) {
                let row_offset = (tile_idx as u32 * rh * rw * 4) as usize;
                atlas[row_offset..row_offset + (rw * rh * 4) as usize].copy_from_slice(&result.pixels);

                let uv0_y = tile_idx as f32 / text_entries_local.len() as f32;
                let uv1_y = (tile_idx + 1) as f32 / text_entries_local.len() as f32;
                overrides_map.insert(*scene_idx, [0.0, uv0_y, 1.0, uv1_y]);
            }
        }
        resources.update_text_atlas(device, queue, &atlas, rw, atlas_h);
    }

    resources.dispatch_compute(
        device, queue, &mut encoder, &snap.scene,
        frame_idx as u32, snap.preview_fps, snap.render_width, snap.render_height,
        dirty,
        Some(&overrides_map),
    );
    if dirty { resources.current_scene_version = snap.scene_version; }
    queue.submit(Some(encoder.finish()));

    let uniforms = Uniforms {
        resolution: [snap.render_width as f32, snap.render_height as f32],
        preview_res: [preview_w as f32, preview_h as f32],
        paper_rect: [0.0, 0.0, preview_w as f32, preview_h as f32],
        viewport_rect: [0.0, 0.0, preview_w as f32, preview_h as f32],
        count: snap.scene.len() as f32,
        mag_x: 0.0, mag_y: 0.0, mag_active: 0.0,
        time,
        pixels_per_point: 1.0,
        gamma_correction: if format!("{:?}", resources.target_format).contains("Srgb") { 0.0 } else { 1.0 },
        _pad: 0.0,
    };
    queue.write_buffer(&resources.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("gpu_cache_texture"),
        size: wgpu::Extent3d { width: preview_w, height: preview_h, depth_or_array_layers: 1 },
        mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2,
        format: resources.target_format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
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
            depth_stencil_attachment: None, occlusion_query_set: None, timestamp_writes: None,
        });
        rpass.set_pipeline(&resources.pipeline);
        rpass.set_bind_group(0, &resources.bind_group, &[]);
        if !snap.scene.is_empty() { rpass.draw(0..6, 0..snap.scene.len() as u32); }
    }
    queue.submit(Some(encoder.finish()));
    Ok(texture)
}

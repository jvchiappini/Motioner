#[cfg(feature = "wgpu")]
use crate::events::time_changed_event::apply_on_time_handlers;
use eframe::egui;
#[cfg(feature = "wgpu")]
use eframe::egui_wgpu;
#[cfg(feature = "wgpu")]
use eframe::wgpu;

pub const MAX_GPU_TEXTURE_SIZE: u32 = 2048;

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuShape {
    pub pos: [f32; 2],
    pub size: [f32; 2],
    pub color: [f32; 4],
    pub shape_type: i32,
    pub spawn_time: f32,
    pub p1: i32,
    pub p2: i32,
}

#[cfg(feature = "wgpu")]
pub struct CompositionCallback {
    pub shapes: Vec<GpuShape>,
    pub render_width: f32,
    pub render_height: f32,
    pub preview_multiplier: f32,
    pub paper_rect: egui::Rect,
    pub viewport_rect: egui::Rect,
    pub magnifier_pos: Option<egui::Pos2>,
    pub time: f32,
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
        uniforms[16] = self.time;

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
        render_pass.draw(0..6, 0..1);
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
                "../composition.wgsl"
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
            size: 1024,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("uniform_buffer"),
            size: 80,
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

pub fn detect_vram_size(adapter_info: &wgpu::AdapterInfo) -> usize {
    let estimated_vram = match adapter_info.device_type {
        wgpu::DeviceType::DiscreteGpu => 6 * 1024 * 1024 * 1024,
        wgpu::DeviceType::IntegratedGpu => 2 * 1024 * 1024 * 1024,
        wgpu::DeviceType::VirtualGpu => 512 * 1024 * 1024,
        _ => 1024 * 1024 * 1024,
    };

    eprintln!(
        "[VRAM] Detected GPU: {} ({:?}) - Estimated VRAM: {} MB",
        adapter_info.name,
        adapter_info.device_type,
        estimated_vram / (1024 * 1024)
    );

    estimated_vram
}

#[cfg(feature = "wgpu")]
pub fn render_frame_color_image_gpu_snapshot(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    resources: &mut GpuResources,
    snap: &crate::canvas::preview_worker::RenderSnapshot,
    time: f32,
) -> Result<egui::ColorImage, String> {
    let mut working_scene = snap.scene.clone();
    let frame_idx = (time * snap.preview_fps as f32).round() as u32;
    apply_on_time_handlers(
        &mut working_scene,
        &snap.dsl_event_handlers,
        time,
        frame_idx,
    );

    let mut preview_w = (snap.render_width as f32 * snap.preview_multiplier)
        .round()
        .max(1.0) as u32;
    let mut preview_h = (snap.render_height as f32 * snap.preview_multiplier)
        .round()
        .max(1.0) as u32;

    if preview_w > MAX_GPU_TEXTURE_SIZE || preview_h > MAX_GPU_TEXTURE_SIZE {
        let scale_w = if preview_w > MAX_GPU_TEXTURE_SIZE {
            MAX_GPU_TEXTURE_SIZE as f32 / preview_w as f32
        } else {
            1.0
        };
        let scale_h = if preview_h > MAX_GPU_TEXTURE_SIZE {
            MAX_GPU_TEXTURE_SIZE as f32 / preview_h as f32
        } else {
            1.0
        };
        let scale = scale_w.min(scale_h);
        preview_w = (preview_w as f32 * scale).round() as u32;
        preview_h = (preview_h as f32 * scale).round() as u32;
    }

    let mut gpu_shapes: Vec<GpuShape> = Vec::new();

    fn collect_prims(
        shapes: &[crate::scene::Shape],
        parent_spawn: f32,
        out: &mut Vec<(crate::scene::Shape, f32)>,
    ) {
        for s in shapes {
            let my_spawn = s.spawn_time().max(parent_spawn);
            match s {
                crate::scene::Shape::Group { children, .. } => {
                    collect_prims(children, my_spawn, out)
                }
                _ => out.push((s.clone(), my_spawn)),
            }
        }
    }

    let mut all = Vec::new();
    collect_prims(&working_scene, 0.0, &mut all);

    for (shape, actual_spawn) in all.iter() {
        if time < *actual_spawn {
            continue;
        }
        match shape {
            crate::scene::Shape::Circle { radius, color, .. } => {
                let (eval_x, eval_y) = crate::animations::animations_manager::animated_xy_for(
                    shape,
                    time,
                    snap.duration_secs,
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
                    spawn_time: *actual_spawn,
                    p1: 0,
                    p2: 0,
                });
            }
            crate::scene::Shape::Rect { w, h, color, .. } => {
                let (eval_x, eval_y) = crate::animations::animations_manager::animated_xy_for(
                    shape,
                    time,
                    snap.duration_secs,
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
                    spawn_time: *actual_spawn,
                    p1: 0,
                    p2: 0,
                });
            }
            _ => {}
        }
    }

    let shape_data = bytemuck::cast_slice(&gpu_shapes);
    if shape_data.len() > resources.shape_buffer.size() as usize {
        resources.shape_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shape_buffer_worker"),
            size: (shape_data.len() * 2 + 1024) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        resources.bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("composition_bind_group_worker"),
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

    if !gpu_shapes.is_empty() {
        queue.write_buffer(&resources.shape_buffer, 0, shape_data);
    }

    let mut uniforms: [f32; 20] = [0.0; 20];
    uniforms[0] = snap.render_width as f32;
    uniforms[1] = snap.render_height as f32;
    uniforms[2] = snap.render_width as f32 * snap.preview_multiplier;
    uniforms[3] = snap.render_height as f32 * snap.preview_multiplier;
    uniforms[4] = 0.0;
    uniforms[5] = 0.0;
    uniforms[6] = snap.render_width as f32;
    uniforms[7] = snap.render_height as f32;
    uniforms[8] = 0.0;
    uniforms[9] = 0.0;
    uniforms[10] = snap.render_width as f32;
    uniforms[11] = snap.render_height as f32;
    uniforms[12] = gpu_shapes.len() as f32;
    uniforms[13] = 0.0;
    uniforms[14] = 0.0;
    uniforms[15] = 0.0;
    uniforms[16] = time;
    queue.write_buffer(
        &resources.uniform_buffer,
        0,
        bytemuck::cast_slice(&uniforms),
    );

    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("preview_offscreen"),
        size: wgpu::Extent3d {
            width: preview_w,
            height: preview_h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("preview_encoder"),
    });
    {
        let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("preview_renderpass"),
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
        rpass.draw(0..6, 0..1);
    }

    let bytes_per_pixel = 4u32;
    let bytes_per_row_unpadded = bytes_per_pixel * preview_w;
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let padded_bytes_per_row = ((bytes_per_row_unpadded + align - 1) / align) * align;
    let output_buffer_size = padded_bytes_per_row as u64 * preview_h as u64;

    let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("preview_readback_buffer"),
        size: output_buffer_size,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    encoder.copy_texture_to_buffer(
        wgpu::ImageCopyTexture {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::ImageCopyBuffer {
            buffer: &output_buffer,
            layout: wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(padded_bytes_per_row),
                rows_per_image: Some(preview_h),
            },
        },
        wgpu::Extent3d {
            width: preview_w,
            height: preview_h,
            depth_or_array_layers: 1,
        },
    );

    queue.submit(Some(encoder.finish()));

    let buffer_slice = output_buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer_slice.map_async(wgpu::MapMode::Read, move |res| {
        let _ = tx.send(res);
    });
    device.poll(wgpu::Maintain::Wait);
    match rx.recv() {
        Ok(Ok(())) => {
            let data = buffer_slice.get_mapped_range();
            let mut pixels: Vec<u8> = Vec::with_capacity((preview_w * preview_h * 4) as usize);
            for row in 0..preview_h as usize {
                let start = row * padded_bytes_per_row as usize;
                let end = start + bytes_per_row_unpadded as usize;
                pixels.extend_from_slice(&data[start..end]);
            }
            drop(data);
            output_buffer.unmap();
            Ok(egui::ColorImage::from_rgba_unmultiplied(
                [preview_w as usize, preview_h as usize],
                &pixels,
            ))
        }
        _ => Err("wgpu readback failed".to_string()),
    }
}

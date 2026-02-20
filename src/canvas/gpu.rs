use eframe::egui;
#[cfg(feature = "wgpu")]
use eframe::egui_wgpu;
#[cfg(feature = "wgpu")]
use eframe::wgpu;

// Maximum texture size for GPU rendering. Modern GPUs support 16384+, but we
// set a conservative limit here. This can be increased for export rendering.
// Preview uses a lower effective limit via preview_multiplier to stay responsive.
pub const MAX_GPU_TEXTURE_SIZE: u32 = 8192;

use super::preview_worker::RenderSnapshot;

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Uniforms {
    pub resolution: [f32; 2],
    pub preview_res: [f32; 2],
    pub paper_rect: [f32; 4],
    pub viewport_rect: [f32; 4],
    pub count: f32,
    pub mag_x: f32,
    pub mag_y: f32,
    pub mag_active: f32,
    pub time: f32,
    pub pixels_per_point: f32,
    pub _padding: [f32; 2],
}

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
    pub uv0: [f32; 2], // UV min en el atlas de texto
    pub uv1: [f32; 2], // UV max en el atlas de texto
}

/// A single keyframe for one property track, as uploaded to the GPU.
/// `easing` matches the WGSL constants: 0=Linear, 1=EaseIn, 2=EaseOut,
/// 3=EaseInOut, 4=Sine, 5=Expo, 6=Circ.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuKeyframe {
    pub frame:  u32,
    pub value:  f32,
    pub easing: u32,
    pub _pad:   u32,
}

/// Per-element descriptor sent to the compute shader.
/// Offsets point into the flat `keyframe_buffer`.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuElementDesc {
    pub x_offset:      u32,
    pub x_len:         u32,
    pub y_offset:      u32,
    pub y_len:         u32,
    pub radius_offset: u32,
    pub radius_len:    u32,
    pub w_offset:      u32,
    pub w_len:         u32,
    pub h_offset:      u32,
    pub h_len:         u32,
    pub shape_type:    i32,
    pub spawn_frame:   u32,
    pub kill_frame:    u32,  // 0xFFFFFFFF = no kill
    pub _pad0:         u32,
    pub _pad1:         u32,
    pub _pad2:         u32,
    pub color:         [f32; 4],
    pub base_size:     [f32; 2],
    pub _pad3:         [f32; 2],
}

/// Map a `crate::animations::easing::Easing` to the GPU easing constant used
/// in `compute_keyframes.wgsl`.
pub fn easing_to_gpu(e: &crate::animations::easing::Easing) -> u32 {
    use crate::animations::easing::Easing;
    match e {
        Easing::Linear                  => 0,
        Easing::EaseIn { .. }           => 1,
        Easing::EaseOut { .. }          => 2,
        Easing::EaseInOut { .. }        => 3,
        Easing::Sine                    => 4,
        Easing::Expo                    => 5,
        Easing::Circ                    => 6,
        // All other curves fall back to linear until GPU support is added.
        _                               => 0,
    }
}

/// The WGSL source for the keyframe compute shader.
pub const COMPUTE_WGSL: &str = include_str!("../shaders/compute_keyframes.wgsl");

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
    pub shared_device: Option<std::sync::Arc<wgpu::Device>>, // Para el caché GPU-to-GPU
    pub shared_queue: Option<std::sync::Arc<wgpu::Queue>>,
    /// Píxeles RGBA del texto rasterizado en CPU (tamaño render_width * render_height)
    pub text_pixels: Option<(Vec<u8>, u32, u32)>, // (data, w, h)
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
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&resources.text_atlas_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Sampler(&resources.text_sampler),
                    },
                ],
            });
        }

        // Actualizar atlas de texto si se proporcionaron píxeles nuevos
        if let Some((ref pixels, w, h)) = self.text_pixels {
            resources.update_text_atlas(device, queue, pixels, w, h);
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
        uniforms[16] = self.time;
        uniforms[17] = screen_descriptor.pixels_per_point;

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
        if !self.shapes.is_empty() {
            render_pass.set_pipeline(&resources.pipeline);
            render_pass.set_bind_group(0, &resources.bind_group, &[]);
            render_pass.draw(0..6, 0..self.shapes.len() as u32);
        }
    }
}

#[cfg(feature = "wgpu")]
pub struct GpuResources {
    // ── Render pipeline ───────────────────────────────────────────────────────
    pub pipeline: wgpu::RenderPipeline,
    pub shape_buffer: wgpu::Buffer,
    pub uniform_buffer: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub target_format: wgpu::TextureFormat,
    // Text atlas
    pub text_atlas_texture: wgpu::Texture,
    pub text_atlas_view: wgpu::TextureView,
    pub text_sampler: wgpu::Sampler,
    pub text_atlas_size: [u32; 2],

    // ── Compute pipeline (keyframe interpolation) ─────────────────────────────
    /// The compute pipeline that reads keyframe tracks and writes GpuShape positions.
    pub compute_pipeline: wgpu::ComputePipeline,
    /// Flat array of all GpuKeyframe values for every element/track (read-only).
    pub keyframe_buffer: wgpu::Buffer,
    /// Per-element descriptors: track offsets + static data (read-only).
    pub element_desc_buffer: wgpu::Buffer,
    /// Compute uniforms: current_frame, fps, element_count.
    pub compute_uniform_buffer: wgpu::Buffer,
    pub compute_bind_group_layout: wgpu::BindGroupLayout,
    pub compute_bind_group: wgpu::BindGroup,
}

#[cfg(feature = "wgpu")]
impl GpuResources {
    pub fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
        // Use the combined WGSL provided by shapes_manager so each shape can
        // supply its own WGSL snippet (one file per shape).
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("composition_shader"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(
                crate::shapes::shapes_manager::COMBINED_WGSL,
            )),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("composition_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
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
                // Binding 2: text atlas texture
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // Binding 3: text sampler (NEAREST = pixelated)
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
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
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING), // USAMOS BLENDING POR HARDWARE
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
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

        // Atlas inicial de texto: 1x1 transparente
        let initial_atlas_size = [1u32, 1u32];
        let text_atlas_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("text_atlas"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let text_atlas_view =
            text_atlas_texture.create_view(&wgpu::TextureViewDescriptor::default());
        // Sampler NEAREST para texto pixelado
        let text_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("text_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
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
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&text_atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&text_sampler),
                },
            ],
        });

        // ── Compute pipeline ──────────────────────────────────────────────────
        let compute_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("compute_keyframes"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(COMPUTE_WGSL)),
        });

        let compute_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("compute_keyframes_bgl"),
            entries: &[
                // binding 0: ComputeUniforms
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // binding 1: keyframe_buffer (read-only storage)
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // binding 2: element_descs (read-only storage)
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // binding 3: output_shapes (read-write storage)
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let compute_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("compute_keyframes_layout"),
                bind_group_layouts: &[&compute_bgl],
                push_constant_ranges: &[],
            });

        let compute_pipeline =
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("compute_keyframes_pipeline"),
                layout: Some(&compute_pipeline_layout),
                module: &compute_shader,
                entry_point: "cs_main",
            });

        // Initial stub buffers (1-element minimum so bind group is valid).
        let keyframe_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("keyframe_buffer"),
            size: std::mem::size_of::<GpuKeyframe>() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let element_desc_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("element_desc_buffer"),
            size: std::mem::size_of::<GpuElementDesc>() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let compute_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("compute_uniform_buffer"),
            size: 16, // ComputeUniforms: 4× u32
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let compute_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("compute_keyframes_bg"),
            layout: &compute_bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: compute_uniform_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: keyframe_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: element_desc_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: shape_buffer.as_entire_binding() },
            ],
        });

        Self {
            pipeline,
            shape_buffer,
            uniform_buffer,
            bind_group,
            bind_group_layout,
            target_format,
            text_atlas_texture,
            text_atlas_view,
            text_sampler,
            text_atlas_size: initial_atlas_size,
            compute_pipeline,
            keyframe_buffer,
            element_desc_buffer,
            compute_uniform_buffer,
            compute_bind_group_layout: compute_bgl,
            compute_bind_group,
        }
    }

    /// Actualiza la textura de atlas de texto con nuevos píxeles RGBA.
    /// Si el tamaño cambia, recrea la textura y el bind_group.
    pub fn update_text_atlas(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pixels: &[u8], // RGBA8, tamaño: w*h*4
        w: u32,
        h: u32,
    ) {
        if w == 0 || h == 0 {
            return;
        }
        // Si el tamaño cambió, recrear la textura
        if self.text_atlas_size[0] != w || self.text_atlas_size[1] != h {
            self.text_atlas_texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("text_atlas"),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            self.text_atlas_view = self
                .text_atlas_texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            self.text_atlas_size = [w, h];
            // Recrear bind_group con la nueva view
            self.bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("composition_bind_group"),
                layout: &self.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.shape_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: self.uniform_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&self.text_atlas_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Sampler(&self.text_sampler),
                    },
                ],
            });
        }
        // Subir los píxeles
        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &self.text_atlas_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            pixels,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(w * 4),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
    }

    /// Upload all `ElementKeyframes` from the scene to the compute buffers and
    /// dispatch the keyframe interpolation compute pass.
    ///
    /// After this call `shape_buffer` contains up-to-date `GpuShape` data for
    /// the requested `current_frame`, ready to be consumed by the render pass.
    pub fn dispatch_compute(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        scene: &[crate::shapes::element_store::ElementKeyframes],
        current_frame: u32,
        fps: u32,
    ) {
        use crate::shapes::element_store::Keyframe;

        let element_count = scene.len() as u32;
        if element_count == 0 {
            return;
        }

        // ── Build flat keyframe array + element descriptors ───────────────────
        let mut all_keyframes: Vec<GpuKeyframe> = Vec::new();
        let mut descs: Vec<GpuElementDesc> = Vec::new();

        for ek in scene {
            let mut desc = GpuElementDesc {
                x_offset: 0, x_len: 0,
                y_offset: 0, y_len: 0,
                radius_offset: 0, radius_len: 0,
                w_offset: 0, w_len: 0,
                h_offset: 0, h_len: 0,
                shape_type: match ek.kind.as_str() {
                    "circle" => 0,
                    "rect"   => 1,
                    "text"   => 2,
                    _        => 0,
                },
                spawn_frame: ek.spawn_frame as u32,
                kill_frame:  ek.kill_frame.map(|f| f as u32).unwrap_or(0xFFFF_FFFF),
                _pad0: 0, _pad1: 0, _pad2: 0,
                color: {
                    // Sample color at current frame, fall back to white.
                    let c = ek.color.iter().rev()
                        .find(|kf| kf.frame <= current_frame as usize)
                        .map(|kf| kf.value)
                        .unwrap_or([255, 255, 255, 255]);
                    [
                        crate::canvas::gpu::srgb_to_linear(c[0]),
                        crate::canvas::gpu::srgb_to_linear(c[1]),
                        crate::canvas::gpu::srgb_to_linear(c[2]),
                        c[3] as f32 / 255.0,
                    ]
                },
                base_size: [0.0, 0.0],
                _pad3: [0.0, 0.0],
            };

            // Helper: append a f32 track to the flat array and record offset/len.
            fn push_track(
                all: &mut Vec<GpuKeyframe>,
                track: &[Keyframe<f32>],
            ) -> (u32, u32) {
                let offset = all.len() as u32;
                for kf in track {
                    all.push(GpuKeyframe {
                        frame:  kf.frame as u32,
                        value:  kf.value,
                        easing: easing_to_gpu(&kf.easing),
                        _pad:   0,
                    });
                }
                (offset, track.len() as u32)
            }

            (desc.x_offset,      desc.x_len)      = push_track(&mut all_keyframes, &ek.x);
            (desc.y_offset,      desc.y_len)       = push_track(&mut all_keyframes, &ek.y);
            (desc.radius_offset, desc.radius_len)  = push_track(&mut all_keyframes, &ek.radius);
            (desc.w_offset,      desc.w_len)       = push_track(&mut all_keyframes, &ek.w);
            (desc.h_offset,      desc.h_len)       = push_track(&mut all_keyframes, &ek.h);

            descs.push(desc);
        }

        // ── Upload buffers (recreate if too small) ────────────────────────────
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

        let shape_bytes_needed =
            (element_count as usize * std::mem::size_of::<GpuShape>()) as u64;
        if shape_bytes_needed > self.shape_buffer.size() {
            self.shape_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("shape_buffer"),
                size: shape_bytes_needed * 2 + 1024,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.rebuild_compute_bind_group(device);
            self.rebuild_render_bind_group(device);
        }

        // Compute uniforms: [current_frame, fps, element_count, 0]
        let cu: [u32; 4] = [current_frame, fps, element_count, 0];
        queue.write_buffer(&self.compute_uniform_buffer, 0, bytemuck::bytes_of(&cu));

        // ── Dispatch ──────────────────────────────────────────────────────────
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

    fn rebuild_compute_bind_group(&mut self, device: &wgpu::Device) {
        self.compute_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("compute_keyframes_bg"),
            layout: &self.compute_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: self.compute_uniform_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: self.keyframe_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: self.element_desc_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: self.shape_buffer.as_entire_binding() },
            ],
        });
    }

    fn rebuild_render_bind_group(&mut self, device: &wgpu::Device) {
        self.bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("composition_bind_group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: self.shape_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: self.uniform_buffer.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&self.text_atlas_view) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::Sampler(&self.text_sampler) },
            ],
        });
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

pub(crate) fn srgb_to_linear(u: u8) -> f32 {
    let x = u as f32 / 255.0;
    if x <= 0.04045 {
        x / 12.92
    } else {
        ((x + 0.055) / 1.055).powf(2.4)
    }
}

#[cfg(feature = "wgpu")]
pub fn render_frame_color_image_gpu_snapshot(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    resources: &mut GpuResources,
    snap: &crate::canvas::preview_worker::RenderSnapshot,
    time: f32,
) -> Result<egui::ColorImage, String> {
    // Render a real frame and read it back to CPU for the buffered preview cache.
    // The texture is created with COPY_SRC so we can stage it down.
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

    // Build GpuShape list from ElementKeyframes, same as render_frame_native_texture.
    let frame_idx = crate::shapes::element_store::seconds_to_frame(time, snap.preview_fps);
    let mut gpu_shapes: Vec<GpuShape> = Vec::new();
    for ek in &snap.scene {
        if frame_idx < ek.spawn_frame { continue; }
        if let Some(kf) = ek.kill_frame { if frame_idx >= kf { continue; } }
        if let Some(shape) = ek.to_shape_at_frame(frame_idx, snap.preview_fps) {
            if let Some(desc) = shape.descriptor() {
                let spawn_secs = ek.spawn_frame as f32 / snap.preview_fps as f32;
                desc.append_gpu_shapes(
                    &shape, &mut gpu_shapes, time, snap.duration_secs, spawn_secs,
                    snap.render_width as f32, snap.render_height as f32,
                );
            }
        }
    }

    if !gpu_shapes.is_empty() {
        queue.write_buffer(&resources.shape_buffer, 0, bytemuck::cast_slice(&gpu_shapes));
    }

    // For the buffered preview we render into a full-resolution (render_w × render_h)
    // texture so all elements are visible at their correct positions, then
    // we return it as a ColorImage (egui will scale it to fit the canvas).
    let render_w = snap.render_width;
    let render_h = snap.render_height;

    let uniforms = Uniforms {
        resolution:   [render_w as f32, render_h as f32],
        preview_res:  [preview_w as f32, preview_h as f32],
        paper_rect:   [0.0, 0.0, render_w as f32, render_h as f32],
        viewport_rect:[0.0, 0.0, render_w as f32, render_h as f32],
        count:        gpu_shapes.len() as f32,
        mag_x: 0.0, mag_y: 0.0, mag_active: 0.0,
        time,
        pixels_per_point: 1.0,
        _padding: [0.0; 2],
    };
    queue.write_buffer(&resources.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

    // Create a render texture with COPY_SRC so we can read the pixels back.
    let render_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("preview_readback_texture"),
        size: wgpu::Extent3d { width: render_w, height: render_h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let render_view = render_texture.create_view(&wgpu::TextureViewDescriptor::default());

    // Bytes-per-row must be a multiple of 256 (wgpu requirement).
    let bytes_per_pixel = 4u32;
    let unpadded_bpr = render_w * bytes_per_pixel;
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let padded_bpr = (unpadded_bpr + align - 1) / align * align;

    let staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("preview_staging_buffer"),
        size: (padded_bpr * render_h) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

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
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
        });
        rpass.set_pipeline(&resources.pipeline);
        rpass.set_bind_group(0, &resources.bind_group, &[]);
        if !gpu_shapes.is_empty() {
            rpass.draw(0..6, 0..gpu_shapes.len() as u32);
        }
    }
    // Copy texture → staging buffer.
    encoder.copy_texture_to_buffer(
        wgpu::ImageCopyTexture {
            texture: &render_texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::ImageCopyBuffer {
            buffer: &staging_buffer,
            layout: wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(padded_bpr),
                rows_per_image: Some(render_h),
            },
        },
        wgpu::Extent3d { width: render_w, height: render_h, depth_or_array_layers: 1 },
    );
    queue.submit(Some(encoder.finish()));

    // Map the staging buffer synchronously (preview worker thread — blocking is fine).
    let slice = staging_buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| { let _ = tx.send(r); });
    device.poll(wgpu::Maintain::Wait);
    rx.recv().map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;

    // De-pad rows and build the ColorImage.
    let mapped = slice.get_mapped_range();
    let mut pixels = Vec::with_capacity((render_w * render_h * 4) as usize);
    for row in 0..render_h {
        let start = (row * padded_bpr) as usize;
        let end = start + unpadded_bpr as usize;
        pixels.extend_from_slice(&mapped[start..end]);
    }
    drop(mapped);
    staging_buffer.unmap();

    Ok(egui::ColorImage::from_rgba_unmultiplied(
        [render_w as usize, render_h as usize],
        &pixels,
    ))
}

pub fn render_frame_native_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    resources: &GpuResources,
    snap: &RenderSnapshot,
    time: f32,
) -> anyhow::Result<wgpu::Texture> {
    let preview_w = (snap.render_width as f32 * snap.preview_multiplier).round() as u32;
    let preview_h = (snap.render_height as f32 * snap.preview_multiplier).round() as u32;

    let mut gpu_shapes: Vec<GpuShape> = Vec::new();

    // Populate GPU instance data from the canonical ElementKeyframes store
    // by sampling each element at the current preview frame. This keeps the
    // GPU composition in sync with the ElementKeyframes-driven runtime.
    let frame_idx = crate::shapes::element_store::seconds_to_frame(time, snap.preview_fps);
    for ek in &snap.scene {
        // skip elements not yet spawned / already killed
        if frame_idx < ek.spawn_frame {
            continue;
        }
        if let Some(kf) = ek.kill_frame {
            if frame_idx >= kf {
                continue;
            }
        }

        if let Some(shape) = ek.to_shape_at_frame(frame_idx, snap.preview_fps) {
            if let Some(desc) = shape.descriptor() {
                let spawn_secs = ek.spawn_frame as f32 / snap.preview_fps as f32;
                desc.append_gpu_shapes(
                    &shape,
                    &mut gpu_shapes,
                    time,
                    snap.duration_secs,
                    spawn_secs,
                    snap.render_width as f32,
                    snap.render_height as f32,
                );
            }
        }
    }

    if !gpu_shapes.is_empty() {
        queue.write_buffer(
            &resources.shape_buffer,
            0,
            bytemuck::cast_slice(&gpu_shapes),
        );
    }

    let uniforms = Uniforms {
        resolution: [snap.render_width as f32, snap.render_height as f32],
        preview_res: [preview_w as f32, preview_h as f32],
        paper_rect: [0.0, 0.0, preview_w as f32, preview_h as f32],
        viewport_rect: [0.0, 0.0, preview_w as f32, preview_h as f32],
        count: gpu_shapes.len() as f32,
        mag_x: 0.0,
        mag_y: 0.0,
        mag_active: 0.0,
        time,
        pixels_per_point: 1.0,
        _padding: [0.0; 2],
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
        if !gpu_shapes.is_empty() {
            rpass.draw(0..6, 0..gpu_shapes.len() as u32);
        }
    }
    queue.submit(Some(encoder.finish()));
    Ok(texture)
}

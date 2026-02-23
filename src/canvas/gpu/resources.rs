//! Maneja los recursos de bajo nivel de WGPU: buffers, texturas y pipelines.
//! Responsable de la inicialización y actualización de recursos globales.

use super::types::*;
#[cfg(feature = "wgpu")]
use eframe::wgpu;

#[cfg(feature = "wgpu")]
pub struct GpuResources {
    // Pipeline de Renderizado
    pub pipeline: wgpu::RenderPipeline,
    pub shape_buffer: wgpu::Buffer,
    pub uniform_buffer: wgpu::Buffer,
    pub bind_group: wgpu::BindGroup,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub target_format: wgpu::TextureFormat,

    // Atlas de texto
    pub text_atlas_texture: wgpu::Texture,
    pub text_atlas_view: wgpu::TextureView,
    pub text_sampler: wgpu::Sampler,
    pub text_atlas_size: [u32; 2],

    /// Buffer containing glyph metadata for text shapes.
    pub glyph_buffer: wgpu::Buffer,

    // Pipeline de Computación (interpolación de keyframes)
    pub compute_pipeline: wgpu::ComputePipeline,
    pub keyframe_buffer: wgpu::Buffer,
    pub element_desc_buffer: wgpu::Buffer,
    pub compute_uniform_buffer: wgpu::Buffer,
    pub compute_bind_group_layout: wgpu::BindGroupLayout,
    pub compute_bind_group: wgpu::BindGroup,

    // Caché de lectura (para evitar re-preparar texturas cada frame en el worker)
    pub readback_staging_buffer: Option<wgpu::Buffer>,
    pub readback_render_texture: Option<wgpu::Texture>,
    pub readback_size: [u32; 2],
    pub readback_pixel_buf: Vec<u8>,
    pub current_scene_version: u32,
}

#[cfg(feature = "wgpu")]
impl GpuResources {
    pub fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
        // Cargar shader combinado de todas las formas
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("composition_shader"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(
                crate::shapes::shapes_manager::COMBINED_WGSL,
            )),
        });

        // Layout de renderizado
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
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                // glyph metadata buffer
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
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
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
        });

        let shape_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("shape_buffer"),
            size: 1024,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("uniform_buffer"),
            size: 80,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // create an initially-empty glyph buffer; it will be resized/uploaded
        // on demand when text shapes are encountered.
        let glyph_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("glyph_buffer"),
            size: std::mem::size_of::<GpuGlyph>() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Configuración inicial del atlas
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
            // Use Rgba8Unorm (NOT sRGB): ab_glyph produces linear RGBA pixels
            // and the glyph color is already stored linear in the GpuGlyph buffer.
            // Rgba8UnormSrgb would make wgpu perform an automatic sRGB decode on
            // every sample, corrupting coverage-alpha values and causing text to
            // appear invisible.
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let text_atlas_view =
            text_atlas_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let text_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("text_sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
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
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: glyph_buffer.as_entire_binding(),
                },
            ],
        });

        // Preparación del pipeline de computación
        let compute_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("compute_keyframes"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(
                super::compute::COMPUTE_WGSL,
            )),
        });

        let compute_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("compute_keyframes_bgl"),
            entries: &[
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
                // glyph metadata buffer for compute (read-only)
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("compute_keyframes_pipeline"),
            layout: Some(
                &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("compute_keyframes_layout"),
                    bind_group_layouts: &[&compute_bgl],
                    push_constant_ranges: &[],
                }),
            ),
            module: &compute_shader,
            entry_point: "cs_main",
        });

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
            size: 32,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let compute_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("compute_keyframes_bg"),
            layout: &compute_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: compute_uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: keyframe_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: element_desc_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: shape_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: glyph_buffer.as_entire_binding(),
                },
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
            glyph_buffer,
            compute_pipeline,
            keyframe_buffer,
            element_desc_buffer,
            compute_uniform_buffer,
            compute_bind_group_layout: compute_bgl,
            compute_bind_group,
            readback_staging_buffer: None,
            readback_render_texture: None,
            readback_size: [0, 0],
            readback_pixel_buf: Vec::new(),
            current_scene_version: 0,
        }
    }

    pub fn update_text_atlas(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pixels: &[u8],
        w: u32,
        h: u32,
    ) {
        if w == 0 || h == 0 {
            return;
        }
        // Reject atlas updates that would require textures larger than the
        // GPU can support.  This is a defensive check: callers should already
        // avoid producing such atlases (see `merge_all_atlases`), but a
        // validation error from wgpu would crash the app if we allowed the
        // call to proceed.
        if w > super::utils::MAX_GPU_TEXTURE_SIZE || h > super::utils::MAX_GPU_TEXTURE_SIZE {
            panic!(
                "attempted to update text atlas to {}x{}, which exceeds GPU limit {}",
                w,
                h,
                super::utils::MAX_GPU_TEXTURE_SIZE
            );
        }
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
                // Rgba8Unorm — same reasoning as the initial allocation above
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            self.text_atlas_view = self
                .text_atlas_texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            self.text_atlas_size = [w, h];
            self.rebuild_render_bind_group(device);
        }
        // compute padding and prepare data buffer before issuing write
        // (see comment below for rationale)
        let bytes_per_pixel = 4;
        let unpadded_bytes_per_row = bytes_per_pixel * w;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as u32;
        let padded_bytes_per_row = ((unpadded_bytes_per_row + align - 1) / align) * align;

        let data_to_upload = if padded_bytes_per_row == unpadded_bytes_per_row {
            pixels.to_vec()
        } else {
            let mut buf = Vec::with_capacity((padded_bytes_per_row * h) as usize);
            for row in 0..h {
                let start = (row * unpadded_bytes_per_row) as usize;
                let end = start + unpadded_bytes_per_row as usize;
                buf.extend_from_slice(&pixels[start..end]);
                buf.extend(
                    std::iter::repeat(0)
                        .take((padded_bytes_per_row - unpadded_bytes_per_row) as usize),
                );
            }
            buf
        };

        // debug log to help diagnose mysterious blank atlases
        /*eprintln!(
            "[gpu::resources] writing atlas {}x{} (unpadded row {} bytes, padded {} bytes)",
            w, h, unpadded_bytes_per_row, padded_bytes_per_row
        );*/

        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &self.text_atlas_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &data_to_upload,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(padded_bytes_per_row),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
    }

    pub fn rebuild_compute_bind_group(&mut self, device: &wgpu::Device) {
        self.compute_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("compute_keyframes_bg"),
            layout: &self.compute_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.compute_uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.keyframe_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.element_desc_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.shape_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: self.glyph_buffer.as_entire_binding(),
                },
            ],
        });
    }

    pub fn rebuild_render_bind_group(&mut self, device: &wgpu::Device) {
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
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: self.glyph_buffer.as_entire_binding(),
                },
            ],
        });
    }
}

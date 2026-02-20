use super::resources::GpuResources;
use super::types::*;
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
    ) {
        let element_count = scene.len() as u32;
        if element_count == 0 {
            return;
        }

        if upload_keyframes {
            let estimated_kf = scene.len() * 5 * 4;
            let mut all_keyframes: Vec<GpuKeyframe> = Vec::with_capacity(estimated_kf);
            let mut all_moves: Vec<GpuMove> = Vec::new();
            let mut descs: Vec<GpuElementDesc> = Vec::with_capacity(scene.len());

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
                    move_offset: 0,
                    move_len: 0,
                    r_offset: 0,
                    g_offset: 0,
                    b_offset: 0,
                    a_offset: 0,
                    r_len: 0,
                    g_len: 0,
                    b_len: 0,
                    a_len: 0,
                    _pad: 0,
                    base_size: [0.0, 0.0],
                    uv0: [0.0, 0.0],
                    uv1: [0.0, 0.0],
                };

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
                desc.move_offset = all_moves.len() as u32;
                desc.move_len = ek.move_commands.len() as u32;
                for ma in &ek.move_commands {
                    // delegate conversion to the helper in `MoveAnimation` now that
                    // the logic lives in the animations module.  keeps GPU code
                    // simpler and avoids duplication.
                    all_moves.push(ma.to_gpu_move(fps));
                }

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

            // Subida de movimientos
            let move_bytes = bytemuck::cast_slice::<GpuMove, u8>(&all_moves);
            if move_bytes.len() as u64 > self.move_buffer.size() {
                self.move_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("move_buffer"),
                    size: (move_bytes.len() * 2 + 64) as u64,
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                self.rebuild_compute_bind_group(device);
            }
            queue.write_buffer(&self.move_buffer, 0, move_bytes);

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

/// Define las estructuras de datos que se envían a la GPU.
/// Estas estructuras deben cumplir con el layout de WGSL (std140/std430).

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

/// Un único keyframe para una propiedad, listo para subirse a la GPU.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuKeyframe {
    pub frame: u32,
    pub value: f32,
    pub easing: u32,
    pub _pad: u32,
}

/// Descriptor por elemento enviado al shader de computación.
/// Contiene offsets y longitudes para indexar el `keyframe_buffer`.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuElementDesc {
    pub x_offset: u32,
    pub x_len: u32,
    pub y_offset: u32,
    pub y_len: u32,
    pub radius_offset: u32,
    pub radius_len: u32,
    pub w_offset: u32,
    pub w_len: u32,
    pub h_offset: u32,
    pub h_len: u32,
    pub shape_type: i32,
    pub spawn_frame: u32,
    pub kill_frame: u32, // 0xFFFFFFFF = no kill
    pub r_offset: u32,
    pub r_len: u32,
    pub g_offset: u32,
    pub g_len: u32,
    pub b_offset: u32,
    pub b_len: u32,
    pub a_offset: u32,
    pub a_len: u32,
    pub _pad_color: u32, // Padding para alinear base_size a 8 bytes
    pub base_size: [f32; 2],
    pub uv0: [f32; 2],
    pub uv1: [f32; 2],
}

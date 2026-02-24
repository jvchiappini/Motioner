struct VertexInput {
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) instance_index: u32,
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) local_uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) @interpolate(flat) shape_type: i32,
    @location(3) @interpolate(flat) size: vec2<f32>,
    @location(4) tex_uv: vec2<f32>,
    // glyph sequence offset/length (for text shapes)
    @location(5) @interpolate(flat) p1: i32,
    @location(6) @interpolate(flat) p2: i32,
    @location(7) reveal: f32,
    @location(8) @interpolate(flat) both_sides: f32,
    @location(9) raw_local_uv: vec2<f32>,
};

struct Shape {
    pos: vec2<f32>,
    size: vec2<f32>,
    color: vec4<f32>,
    shape_type: i32,
    spawn_time: f32,
    p1: i32,
    p2: i32,
    reveal: f32,
    both_sides: f32,
    _pad: vec2<f32>,
    uv0: vec2<f32>,
    uv1: vec2<f32>,
};

struct Uniforms {
    resolution: vec2<f32>,
    preview_res: vec2<f32>,
    paper_rect: vec4<f32>,
    viewport_rect: vec4<f32>,
    count: f32,
    mag_x: f32,
    mag_y: f32,
    mag_active: f32,
    time: f32,
    pixels_per_point: f32,
    gamma_correction: f32,
    _pad: f32,
};

struct Glyph {
    uv0: vec2<f32>,
    uv1: vec2<f32>,
    advance: f32,
    _pad_align_0: f32,
    _pad_align_1: f32,
    _pad_align_2: f32,
    color: vec4<f32>,
    _pad: vec4<f32>,
};

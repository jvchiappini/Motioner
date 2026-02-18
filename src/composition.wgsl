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
};

struct Shape {
    pos: vec2<f32>,
    size: vec2<f32>,
    color: vec4<f32>,
    shape_type: i32,
    spawn_time: f32,
    p1: i32,
    p2: i32,
    uv0: vec2<f32>,   // UV min en el atlas de texto
    uv1: vec2<f32>,   // UV max en el atlas de texto
}

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
}

@group(0) @binding(0) var<storage, read> shapes: array<Shape>;
@group(0) @binding(1) var<uniform> uniforms: Uniforms;
@group(0) @binding(2) var text_atlas: texture_2d<f32>;
@group(0) @binding(3) var text_sampler: sampler;

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    let s = shapes[in.instance_index];
    var out: VertexOutput;

    // Coordenadas del Quad (-1..1)
    var quad_pos = vec2<f32>(0.0, 0.0);
    var quad_uv01 = vec2<f32>(0.0, 0.0); // UV local 0..1 del quad
    if (in.vertex_index == 0u) { quad_pos = vec2<f32>(-1.0, -1.0); quad_uv01 = vec2<f32>(0.0, 1.0); }
    else if (in.vertex_index == 1u) { quad_pos = vec2<f32>(1.0, -1.0); quad_uv01 = vec2<f32>(1.0, 1.0); }
    else if (in.vertex_index == 2u) { quad_pos = vec2<f32>(-1.0, 1.0); quad_uv01 = vec2<f32>(0.0, 0.0); }
    else if (in.vertex_index == 3u) { quad_pos = vec2<f32>(-1.0, 1.0); quad_uv01 = vec2<f32>(0.0, 0.0); }
    else if (in.vertex_index == 4u) { quad_pos = vec2<f32>(1.0, -1.0); quad_uv01 = vec2<f32>(1.0, 1.0); }
    else { quad_pos = vec2<f32>(1.0, 1.0); quad_uv01 = vec2<f32>(1.0, 0.0); }

    out.local_uv = quad_pos; // Para calcular el SDF en el fragment shader
    out.color = s.color;
    out.shape_type = s.shape_type;
    out.size = s.size;

    // UV interpolada en el rango [uv0, uv1] del atlas de texto.
    // Invertimos Y porque el NDC tiene Y invertido respecto al buffer CPU (Y=0 arriba en imagen).
    let quad_uv01_flipped = vec2<f32>(quad_uv01.x, 1.0 - quad_uv01.y);
    out.tex_uv = mix(s.uv0, s.uv1, quad_uv01_flipped);

    // Transformación: De coordenadas normalizadas de escena (0..1) a coordenadas de clip (-1..1)
    // El tamaño del quad depende del tamaño del objeto
    let world_pos_px = s.pos + (quad_pos * s.size);
    let world_pos_norm = world_pos_px / uniforms.resolution;
    
    // Map normalized (0..1) to paper area, then to screen
    let screen_pos = mix(uniforms.paper_rect.xy, uniforms.paper_rect.zw, world_pos_norm);
    
    // Convert screen pixel position to NDC (-1..1)
    let ndc_pos = (screen_pos - uniforms.viewport_rect.xy) / (uniforms.viewport_rect.zw - uniforms.viewport_rect.xy) * 2.0 - 1.0;
    
    // Invertimos Y porque WGPU e imagen van al revés
    out.position = vec4<f32>(ndc_pos.x, -ndc_pos.y, 0.0, 1.0);
    
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {

    // -- Pixelation Logic --
    // Convert Screen Coords -> Normalized Paper Coords
    // NOTE: in.position is in PHYSICAL pixels. paper_rect is in LOGICAL points.
    // We must scale physical pixels to logical points using pixels_per_point.
    let logical_pos = in.position.xy / uniforms.pixels_per_point;

    let paper_w = uniforms.paper_rect.z - uniforms.paper_rect.x;
    let paper_h = uniforms.paper_rect.w - uniforms.paper_rect.y;
    let rel_x = logical_pos.x - uniforms.paper_rect.x;
    let rel_y = logical_pos.y - uniforms.paper_rect.y;
    
    // Prevent division by zero if paper is unreasonably small
    let safe_pw = max(paper_w, 1.0);
    let safe_ph = max(paper_h, 1.0);

    let norm_x = rel_x / safe_pw;
    let norm_y = rel_y / safe_ph;
    
    // Snap to Virtual Grid (Project Pixels)
    let grid_w = uniforms.preview_res.x;
    let grid_h = uniforms.preview_res.y;
    
    // Center of the virtual pixel
    let snapped_x = floor(norm_x * grid_w) / grid_w + (0.5 / grid_w);
    let snapped_y = floor(norm_y * grid_h) / grid_h + (0.5 / grid_h);

    let dx = snapped_x - norm_x;
    let dy = snapped_y - norm_y;
    
    // Convert offset strictly to UV space
    // 1.0 UV unit = size pixels (half-extent)
    // delta in normalized paper space * resolution = delta in project pixels
    // delta in project pixels / size = delta in UV
    
    // We use a small epsilon to avoid div-by-zero for 0-size shapes
    let sz = max(in.size, vec2<f32>(0.001));
    
    let du = (dx * uniforms.resolution.x) / sz.x;
    let dv = (dy * uniforms.resolution.y) / sz.y;
    
    let effective_uv = in.local_uv + vec2<f32>(du, dv);

    // -- SDF Evaluation --
    var alpha = 1.0;
    
    if (in.shape_type == 0) { // Circle
        let d = length(effective_uv) - 1.0;
        // Hard edge for pixelated look
        if (d > 0.0) { alpha = 0.0; } else { alpha = 1.0; }
    } else if (in.shape_type == 1) { // Rect
        // Rect implicitly from -1 to 1. Check if snapped UV is outside.
        let d = max(abs(effective_uv.x), abs(effective_uv.y)) - 1.0;
        if (d > 0.0) { alpha = 0.0; } else { alpha = 1.0; }
    } else if (in.shape_type == 2) { // Texto: atlas snapeado al mismo grid que los demás elementos
        let atlas_uv = vec2<f32>(snapped_x, snapped_y);
        return textureSample(text_atlas, text_sampler, atlas_uv);
    }

    return vec4<f32>(in.color.rgb, in.color.a * alpha);
}

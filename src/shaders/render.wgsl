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
    out.p1 = s.p1;
    out.p2 = s.p2;
    out.reveal = s.reveal;
    out.both_sides = s.both_sides;

    // UV interpolada en el rango [uv0, uv1] del atlas de texto.
    // Invertimos Y porque el NDC tiene Y invertido respecto al buffer CPU (Y=0 arriba en imagen).
    let quad_uv01_flipped = vec2<f32>(quad_uv01.x, 1.0 - quad_uv01.y);
    out.tex_uv = mix(s.uv0, s.uv1, quad_uv01_flipped);

    // Transformación: De coordenadas normalizadas de escena (0..1) a coordenadas de clip (-1..1)
    // NOTE: `s.pos` is interpreted as the *center of mass* of the shape.  The
    // GPU-side `size` still holds the radius for circles or the half-dim for
    // rects, so multiplying by `quad_pos` (which runs from -1..1) produces an
    // offset from the centre to the current quad vertex.  This restores the
    // familiar behaviour where the DSL coordinates specify the centre of the
    // element rather than its top-left corner.  Consumers should be aware that
    // placing a shape at (0,0) will once again position it half off the paper
    // if its size is non‑zero.
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

    // Delegate actual per-shape rendering to shape-specific helpers.
    // Per-shape helpers are provided from separate WGSL snippets (one file per shape)
    // and are concatenated at compile time by the Rust side.
    let final_color = eval_shape(in, effective_uv);
    if (uniforms.gamma_correction > 0.5) {
        return vec4<f32>(pow(final_color.rgb, vec3<f32>(1.0/2.2)), final_color.a);
    }
    return final_color;
}

// Dispatcher: call the per-shape helper function according to `shape_type`.
// When adding a new shape: implement `shape_<name>` in a WGSL file and
// add the Rust-side include so it becomes part of the shader module.
fn eval_shape(in: VertexOutput, effective_uv: vec2<f32>) -> vec4<f32> {
    if (in.shape_type == 0) {
        return shape_circle(in, effective_uv);
    } else if (in.shape_type == 1) {
        return shape_rect(in, effective_uv);
    } else if (in.shape_type == 2) {
        return shape_text(in, effective_uv);
    }
    // fallback: opaque solid color
    return vec4<f32>(in.color.rgb, in.color.a);
}

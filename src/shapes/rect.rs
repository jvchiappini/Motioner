/// Rect-specific helpers and constants.
#[allow(dead_code)]
pub fn default_color() -> [u8; 4] {
    [200, 120, 120, 255]
}

// Future: rect-specific utilities can live here

/// Return the DSL snippet line for a rectangle (without animations).
#[allow(clippy::too_many_arguments)]
pub fn to_dsl_snippet(
    name: &str,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    color: [u8; 4],
    spawn_time: f32,
    indent: &str,
) -> String {
    format!(
        "{}rect \"{}\" {{\n{}    x = {:.3},\n{}    y = {:.3},\n{}    width = {:.3},\n{}    height = {:.3},\n{}    fill = \"#{:02x}{:02x}{:02x}\",\n{}    spawn = {:.2}\n{}}}\n",
        indent, name, indent, x, indent, y, indent, w, indent, h, indent, color[0], color[1], color[2], indent, spawn_time, indent
    )
}

/// Produce the full DSL snippet for a rectangle.
#[allow(clippy::too_many_arguments)]
pub fn to_dsl_with_animations(
    name: &str,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    color: [u8; 4],
    spawn_time: f32,
    _animations: &[crate::scene::Animation],
    indent: &str,
) -> String {
    // We no longer nest animations inside the rect block.
    // Animations are now top-level entities generated in dsl.rs.
    to_dsl_snippet(name, x, y, w, h, color, spawn_time, indent)
}

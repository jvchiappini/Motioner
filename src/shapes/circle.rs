/// Circle-specific helpers and constants.
#[allow(dead_code)]
pub fn default_color() -> [u8; 4] {
    [120, 200, 255, 255]
}

// Future: circle-specific utilities can live here (hit-tests, editors, etc.)

/// Return the DSL snippet line for a circle (without animations).
pub fn to_dsl_snippet(
    name: &str,
    x: f32,
    y: f32,
    radius: f32,
    color: [u8; 4],
    spawn_time: f32,
    indent: &str,
) -> String {
    format!(
        "{}circle \"{}\" {{\n{}    x = {:.3},\n{}    y = {:.3},\n{}    radius = {:.3},\n{}    fill = \"#{:02x}{:02x}{:02x}\",\n{}    spawn = {:.2}\n{}}}\n",
        indent, name, indent, x, indent, y, indent, radius, indent, color[0], color[1], color[2], indent, spawn_time, indent
    )
}

/// Produce the full DSL snippet for a circle.
#[allow(clippy::too_many_arguments)]
pub fn to_dsl_with_animations(
    name: &str,
    x: f32,
    y: f32,
    radius: f32,
    color: [u8; 4],
    spawn_time: f32,
    _animations: &[crate::scene::Animation],
    indent: &str,
) -> String {
    #[allow(clippy::too_many_arguments)]
    to_dsl_snippet(name, x, y, radius, color, spawn_time, indent)
}

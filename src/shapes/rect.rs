/// Rect-specific helpers and constants.
pub fn default_color() -> [u8; 4] {
    [200, 120, 120, 255]
}

// Future: rect-specific utilities can live here

/// Return the DSL snippet line for a rectangle (without animations).
pub fn to_dsl_snippet(name: &str, x: f32, y: f32, w: f32, h: f32, color: [u8; 4], spawn_time: f32, indent: &str) -> String {
    format!(
        "{}rect \"{}\" {{\n{}    x = {:.3},\n{}    y = {:.3},\n{}    width = {:.3},\n{}    height = {:.3},\n{}    fill = \"#{:02x}{:02x}{:02x}\",\n{}    spawn = {:.2}\n{}}}\n",
        indent, name, indent, x, indent, y, indent, w, indent, h, indent, color[0], color[1], color[2], indent, spawn_time, indent
    )
}

/// Produce the full DSL snippet for a rectangle.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_dsl_snippet_contains_values() {
        let s = to_dsl_snippet("R", 0.1, 0.2, 0.3, 0.4, [10,20,30,255], 1.0, "");
        assert!(s.contains("rect \"R\""));
        assert!(s.contains("width = 0.300"));
        assert!(s.contains("height = 0.400"));
        assert!(s.contains("fill = \"#0a141e\""));
    }

    #[test]
    fn rect_includes_move_animation_snippet() {
        let anim = crate::scene::Animation::Move { to_x: 0.7, to_y: 0.5, start: 0.0, end: 5.0, easing: crate::scene::Easing::Linear };
        let out = to_dsl_with_animations("R", 0.1, 0.2, 0.3, 0.4, [10,20,30,255], 0.0, &[anim], "");
        assert!(out.contains("move {"));
        assert!(out.contains("during = 0.000 -> 5.000"));
    }
}

/// Circle-specific helpers and constants.
pub fn default_color() -> [u8; 4] {
    [120, 200, 255, 255]
}

// Future: circle-specific utilities can live here (hit-tests, editors, etc.)

/// Return the DSL snippet line for a circle (without animations).
pub fn to_dsl_snippet(name: &str, x: f32, y: f32, radius: f32, color: [u8; 4], spawn_time: f32, indent: &str) -> String {
    format!(
        "{}circle(name = \"{}\", x = {:.3}, y = {:.3}, radius = {:.3}, fill = \"#{:02x}{:02x}{:02x}\", spawn = {:.2})",
        indent, name, x, y, radius, color[0], color[1], color[2], spawn_time
    )
}

/// Produce the full DSL snippet for a circle including its animations.
pub fn to_dsl_with_animations(
    name: &str,
    x: f32,
    y: f32,
    radius: f32,
    color: [u8; 4],
    spawn_time: f32,
    animations: &[crate::scene::Animation],
    indent: &str,
) -> String {
    let mut out = to_dsl_snippet(name, x, y, radius, color, spawn_time, indent);
    if !animations.is_empty() {
        for a in animations {
            // currently we only support `Move` — delegate to MoveAnimation's formatter
            if let Some(ma) = crate::animations::move_animation::MoveAnimation::from_scene(a) {
                out.push_str(&ma.to_dsl_snippet(name, indent));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn circle_dsl_snippet_contains_values() {
        let s = to_dsl_snippet("C", 0.5, 0.25, 0.1, [1,2,3,255], 0.0, "    ");
        assert!(s.contains("circle(name = \"C\""));
        assert!(s.contains("x = 0.500"));
        assert!(s.contains("y = 0.250"));
        assert!(s.contains("radius = 0.100"));
        assert!(s.contains("fill = \"#010203\""));
    }

    #[test]
    fn circle_includes_move_animation_snippet() {
        use crate::animations::move_animation::MoveAnimation;
        let anim = crate::scene::Animation::Move { to_x: 0.7, to_y: 0.5, start: 0.0, end: 5.0, easing: crate::scene::Easing::Linear };
        let out = to_dsl_with_animations("C", 0.5, 0.5, 0.1, [1,2,3,255], 0.0, &[anim], "    ");
        assert!(out.contains("move {"), "missing move block: {}", out);
        assert!(out.contains("startAt = 0.000"));
        assert!(out.contains("time = 5.000"));
    }
}

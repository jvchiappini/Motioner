use crate::scene::Shape;

/// Generate a simple DSL string for the given scene.
pub fn generate_dsl(scene: &Vec<Shape>, fps: u32, duration: f32) -> String {
    let mut out = String::new();
    out.push_str(&format!("project \"Demo\" {{\n  size(1280, 720)\n  timeline {{ fps = {}; duration = {} }}\n\n  layer \"scene\" {{\n", fps, duration));
    for s in scene.iter() { out.push_str("    "); out.push_str(&s.to_dsl()); out.push_str("\n"); }
    out.push_str("  }\n}\n");
    out
}

/// Stub parser: in the future this will parse DSL -> Scene (AST).
/// For now returns an empty vec (placeholder).
pub fn parse_dsl(_src: &str) -> Vec<Shape> {
    // TODO: implement parser that produces Shape instances
    Vec::new()
}

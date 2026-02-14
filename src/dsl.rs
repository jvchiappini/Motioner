use crate::scene::Shape;

/// Generate a simple DSL string for the given scene.
pub fn generate_dsl(scene: &[Shape], width: u32, height: u32, fps: u32, duration: f32) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "project \"Demo\" {{\n  size({}, {})\n  timeline {{ fps = {}; duration = {} }}\n\n  layer \"scene\" {{\n",
        width, height, fps, duration
    ));
    for s in scene.iter() {
        out.push_str("    ");
        out.push_str(&s.to_dsl());
        out.push_str("\n");
    }
    out.push_str("  }\n}\n");
    out
}

/// Simple parser struct to hold extracted values
pub struct ProjectConfig {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub duration: f32,
}

/// Validates and parses just the header configuration (size, fps, duration).
/// Returns error string if validation fails.
pub fn parse_config(code: &str) -> Result<ProjectConfig, String> {
    // Simple line-based scanning
    let mut width = None;
    let mut height = None;
    let mut fps = None;
    let mut duration = None;

    for line in code.lines() {
        let line = line.trim();
        
        // Parse size(w, h)
        if line.starts_with("size(") && line.ends_with(")") {
            let content = &line[5..line.len()-1];
            let parts: Vec<&str> = content.split(',').collect();
            if parts.len() == 2 {
                if let (Ok(w), Ok(h)) = (parts[0].trim().parse::<u32>(), parts[1].trim().parse::<u32>()) {
                    width = Some(w);
                    height = Some(h);
                } else {
                    return Err(format!("Invalid size parameters: {}", content));
                }
            }
        }

        // Parse timeline { ... }
        if line.starts_with("timeline {") && line.contains("}") {
            // Very naive: extract content inside {}
            if let Some(start) = line.find('{') {
                if let Some(end) = line.rfind('}') {
                    let content = &line[start+1..end];
                    // Split by semicolon
                    for part in content.split(';') {
                        let part = part.trim();
                        if part.starts_with("fps =") {
                            if let Ok(val) = part.replace("fps =", "").trim().parse::<u32>() {
                                fps = Some(val);
                            }
                        }
                        if part.starts_with("duration =") {
                            if let Ok(val) = part.replace("duration =", "").trim().parse::<f32>() {
                                duration = Some(val);
                            }
                        }
                    }
                }
            }
        }
    }

    // Validation
    if width.is_none() || height.is_none() {
        return Err("Missing 'size(width, height)' configuration".to_string());
    }
    if fps.is_none() {
        return Err("Missing 'timeline { fps = ... }' configuration".to_string());
    }
    if duration.is_none() {
         return Err("Missing 'timeline { duration = ... }' configuration".to_string());
    }

    Ok(ProjectConfig {
        width: width.unwrap(),
        height: height.unwrap(),
        fps: fps.unwrap(),
        duration: duration.unwrap(),
    })
}

/// Stub parser: in the future this will parse DSL -> Scene (AST).
/// For now returns an empty vec (placeholder).
pub fn parse_dsl(_src: &str) -> Vec<Shape> {
    // TODO: implement parser that produces Shape instances
    Vec::new()
}

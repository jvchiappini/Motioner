use crate::scene::Shape;

/// Generate a simple DSL string for the given scene.
pub fn generate_dsl(scene: &[Shape], width: u32, height: u32, fps: u32, duration: f32) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "size({}, {})\ntimeline {{ fps = {}; duration = {} }}\n\n",
        width, height, fps, duration
    ));
    for s in scene.iter() {
        out.push_str(&s.to_dsl());
        // Validate animations vs spawn_time and emit a warning comment when detected
        match s {
            Shape::Circle {
                spawn_time,
                animations,
                ..
            }
            | Shape::Rect {
                spawn_time,
                animations,
                ..
            } => {
                for a in animations {
                    if let crate::scene::Animation::Move { start, .. } = a {
                        let start_secs = *start; // start is already in seconds
                        if start_secs < *spawn_time {
                            out.push_str(&format!("\n# WARNING: animation starts at {:.3}s before element spawn at {:.3}s\n", start_secs, spawn_time));
                        }
                    }
                }
            }
            _ => {}
        }
        out.push_str("\n");
    }
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
            let content = &line[5..line.len() - 1];
            let parts: Vec<&str> = content.split(',').collect();
            if parts.len() == 2 {
                if let (Ok(w), Ok(h)) = (
                    parts[0].trim().parse::<u32>(),
                    parts[1].trim().parse::<u32>(),
                ) {
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
                    let content = &line[start + 1..end];
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

    #[test]
    fn parse_top_level_move_with_lerp() {
        let src = r##"size(1280, 720)
timeline { fps = 60; duration = 5 }

circle(name = "Circle", x = 0.500, y = 0.500, radius = 0.100, fill = "#78c8ff", spawn = 0.00)
move {
    element = "Circle"
    type = lerp
    startAt = 0.000
    end {
        time = 5.000
        x = 0.700
        y = 0.500
    }
}
"##;

        let shapes = parse_dsl(src);
        assert_eq!(shapes.len(), 1);
        match &shapes[0] {
            crate::scene::Shape::Circle { animations, .. } => {
                assert_eq!(animations.len(), 1);
                if let crate::scene::Animation::Move { easing, .. } = animations[0] {
                    assert!(matches!(easing, crate::scene::Easing::Lerp { .. }));
                } else {
                    panic!("expected Move animation");
                }
            }
            _ => panic!("expected a circle"),
        }
    }

    // Validate
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
    let src = _src;
    let mut shapes: Vec<Shape> = Vec::new();
    let mut lines = src.lines().map(|l| l.trim()).peekable();

    fn parse_kv_list(s: &str) -> std::collections::HashMap<String, String> {
        let mut map = std::collections::HashMap::new();
        for part in s.split(',') {
            let p = part.trim();
            if p.is_empty() {
                continue;
            }
            if let Some(eq) = p.find('=') {
                let key = p[..eq].trim().to_string();
                let val = p[eq + 1..].trim().trim_matches('"').to_string();
                map.insert(key, val);
            }
        }
        map
    }

    // collect top-level move blocks that reference elements (to support move blocks before/after shapes)
    // tuple: (element, end_time, to_x, to_y, start_at, easing)
    let mut pending_moves: Vec<(String, f32, f32, f32, f32, crate::scene::Easing)> = Vec::new();

    while let Some(line) = lines.next() {
        if line.starts_with("circle(") && line.ends_with(")") {
            let inner = &line[7..line.len() - 1];
            let kv = parse_kv_list(inner);
            let name = kv
                .get("name")
                .cloned()
                .unwrap_or_else(|| format!("Circle_{}", shapes.len()));
            let x = kv
                .get("x")
                .and_then(|v| v.parse::<f32>().ok())
                .unwrap_or(0.5);
            let y = kv
                .get("y")
                .and_then(|v| v.parse::<f32>().ok())
                .unwrap_or(0.5);
            let radius = kv
                .get("radius")
                .and_then(|v| v.parse::<f32>().ok())
                .unwrap_or(0.1);
            let spawn = kv
                .get("spawn")
                .and_then(|v| v.parse::<f32>().ok())
                .unwrap_or(0.0);
            let color = if let Some(fill) = kv.get("fill") {
                if fill.starts_with('#') && fill.len() >= 7 {
                    let r = u8::from_str_radix(&fill[1..3], 16).unwrap_or(120);
                    let g = u8::from_str_radix(&fill[3..5], 16).unwrap_or(200);
                    let b = u8::from_str_radix(&fill[5..7], 16).unwrap_or(255);
                    [r, g, b, 255]
                } else {
                    [120, 200, 255, 255]
                }
            } else {
                [120, 200, 255, 255]
            };

            shapes.push(Shape::Circle {
                name,
                x,
                y,
                radius,
                color,
                spawn_time: spawn,
                animations: Vec::new(),
                visible: true,
            });

            // Peek for an anim block immediately after
            if let Some(&next) = lines.peek() {
                if next.starts_with("anim") {
                    // consume 'anim {' line
                    lines.next();
                    // collect keyframes
                    let mut keyframes: Vec<(f32, Option<f32>, Option<f32>)> = Vec::new();
                    while let Some(kline) = lines.next() {
                        let k = kline.trim();
                        if k == "}" {
                            break;
                        }
                        if k.starts_with("at") {
                            // at <time> { ... }
                            if let Some(open) = k.find('{') {
                                let time_part = k[2..open].trim();
                                if let Ok(t) = time_part.parse::<f32>() {
                                    let inner = k[open + 1..].trim_end_matches('}').trim();
                                    let mut kx: Option<f32> = None;
                                    let mut ky: Option<f32> = None;
                                    for stmt in inner.split(';') {
                                        let s = stmt.trim();
                                        if s.contains(".x") && s.contains('=') {
                                            if let Some(eq) = s.find('=') {
                                                let val = s[eq + 1..].trim();
                                                if let Ok(v) = val.parse::<f32>() {
                                                    kx = Some(v);
                                                }
                                            }
                                        }
                                        if s.contains(".y") && s.contains('=') {
                                            if let Some(eq) = s.find('=') {
                                                let val = s[eq + 1..].trim();
                                                if let Ok(v) = val.parse::<f32>() {
                                                    ky = Some(v);
                                                }
                                            }
                                        }
                                    }
                                    keyframes.push((t, kx, ky));
                                }
                            }
                        }
                    }

                    if !keyframes.is_empty() {
                        // associate with last shape
                        if let Some(last) = shapes.last_mut() {
                            // pick first and last keyframes with x/y defined
                            let start = keyframes.first().unwrap().0;
                            let end = keyframes.last().unwrap().0;
                            let last_x = keyframes.last().unwrap().1.unwrap_or(match last {
                                Shape::Circle { x, .. } => *x,
                                Shape::Rect { x, .. } => *x,
                                _ => 0.5,
                            });
                            let last_y = keyframes.last().unwrap().2.unwrap_or(match last {
                                Shape::Circle { y, .. } => *y,
                                Shape::Rect { y, .. } => *y,
                                _ => 0.5,
                            });
                            match last {
                                Shape::Circle { animations, .. }
                                | Shape::Rect { animations, .. } => {
                                    animations.push(crate::scene::Animation::Move {
                                        to_x: last_x,
                                        to_y: last_y,
                                        start,
                                        end,
                                        easing: crate::scene::Easing::Linear,
                                    });
                                }
                                _ => {}
                            }
                        }
                    }
                } else if next.starts_with("move") {
                    // consume 'move {', collect inner lines and delegate parsing to shared helper
                    lines.next();
                    let mut inner: Vec<&str> = Vec::new();
                    while let Some(bline) = lines.next() {
                        let b = bline.trim();
                        if b == "}" {
                            break;
                        }
                        inner.push(b);
                    }
                    if let Some(parsed) =
                        crate::animations::move_animation::parse_move_block(&inner)
                    {
                        if let Some(last) = shapes.last_mut() {
                            match last {
                                Shape::Circle { animations, .. }
                                | Shape::Rect { animations, .. } => {
                                    animations.push(crate::scene::Animation::Move {
                                        to_x: parsed.to_x,
                                        to_y: parsed.to_y,
                                        start: parsed.start,
                                        end: parsed.end,
                                        easing: parsed.easing,
                                    });
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        } else if line.starts_with("rect(") && line.ends_with(")") {
            let inner = &line[5..line.len() - 1];
            let kv = parse_kv_list(inner);
            let name = kv
                .get("name")
                .cloned()
                .unwrap_or_else(|| format!("Rect_{}", shapes.len()));
            let x = kv
                .get("x")
                .and_then(|v| v.parse::<f32>().ok())
                .unwrap_or(0.4);
            let y = kv
                .get("y")
                .and_then(|v| v.parse::<f32>().ok())
                .unwrap_or(0.4);
            let w = kv
                .get("width")
                .or_else(|| kv.get("w"))
                .and_then(|v| v.parse::<f32>().ok())
                .unwrap_or(0.2);
            let h = kv
                .get("height")
                .or_else(|| kv.get("h"))
                .and_then(|v| v.parse::<f32>().ok())
                .unwrap_or(0.2);
            let spawn = kv
                .get("spawn")
                .and_then(|v| v.parse::<f32>().ok())
                .unwrap_or(0.0);
            let color = if let Some(fill) = kv.get("fill") {
                if fill.starts_with('#') && fill.len() >= 7 {
                    let r = u8::from_str_radix(&fill[1..3], 16).unwrap_or(200);
                    let g = u8::from_str_radix(&fill[3..5], 16).unwrap_or(120);
                    let b = u8::from_str_radix(&fill[5..7], 16).unwrap_or(120);
                    [r, g, b, 255]
                } else {
                    [200, 120, 120, 255]
                }
            } else {
                [200, 120, 120, 255]
            };

            shapes.push(Shape::Rect {
                name,
                x,
                y,
                w,
                h,
                color,
                spawn_time: spawn,
                animations: Vec::new(),
                visible: true,
            });

            // optional anim handling (same as circle)
            if let Some(&next) = lines.peek() {
                if next.starts_with("anim") {
                    lines.next();
                    let mut keyframes: Vec<(f32, Option<f32>, Option<f32>)> = Vec::new();
                    while let Some(kline) = lines.next() {
                        let k = kline.trim();
                        if k == "}" {
                            break;
                        }
                        if k.starts_with("at") {
                            if let Some(open) = k.find('{') {
                                let time_part = k[2..open].trim();
                                if let Ok(t) = time_part.parse::<f32>() {
                                    let inner = k[open + 1..].trim_end_matches('}').trim();
                                    let mut kx: Option<f32> = None;
                                    let mut ky: Option<f32> = None;
                                    for stmt in inner.split(';') {
                                        let s = stmt.trim();
                                        if s.contains(".x") && s.contains('=') {
                                            if let Some(eq) = s.find('=') {
                                                let val = s[eq + 1..].trim();
                                                if let Ok(v) = val.parse::<f32>() {
                                                    kx = Some(v);
                                                }
                                            }
                                        }
                                        if s.contains(".y") && s.contains('=') {
                                            if let Some(eq) = s.find('=') {
                                                let val = s[eq + 1..].trim();
                                                if let Ok(v) = val.parse::<f32>() {
                                                    ky = Some(v);
                                                }
                                            }
                                        }
                                    }
                                    keyframes.push((t, kx, ky));
                                }
                            }
                        }
                    }
                    if !keyframes.is_empty() {
                        if let Some(last) = shapes.last_mut() {
                            let start = keyframes.first().unwrap().0;
                            let end = keyframes.last().unwrap().0;
                            let last_x = keyframes.last().unwrap().1.unwrap_or(match last {
                                Shape::Circle { x, .. } => *x,
                                Shape::Rect { x, .. } => *x,
                                _ => 0.5,
                            });
                            let last_y = keyframes.last().unwrap().2.unwrap_or(match last {
                                Shape::Circle { y, .. } => *y,
                                Shape::Rect { y, .. } => *y,
                                _ => 0.5,
                            });
                            match last {
                                Shape::Circle { animations, .. }
                                | Shape::Rect { animations, .. } => {
                                    animations.push(crate::scene::Animation::Move {
                                        to_x: last_x,
                                        to_y: last_y,
                                        start,
                                        end,
                                        easing: crate::scene::Easing::Linear,
                                    });
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        } else if line.starts_with("move") && line.contains('{') {
            // parse a top-level move { ... } block (delegate inner parsing to animations module)
            // consume the opening 'move {' and gather inner lines
            let mut inner: Vec<&str> = Vec::new();
            // consume first line's remainder already matched by `line` — but we still need to advance iterator
            while let Some(bline) = lines.next() {
                let b = bline.trim();
                if b == "}" {
                    break;
                }
                inner.push(b);
            }

            if let Some(parsed) = crate::animations::move_animation::parse_move_block(&inner) {
                if let Some(el) = parsed.element.clone() {
                    pending_moves.push((
                        el,
                        parsed.end,
                        parsed.to_x,
                        parsed.to_y,
                        parsed.start,
                        parsed.easing,
                    ));
                }
            }
        } else {
            // ignore other lines for now
            continue;
        }
    }

    // attach pending moves to matching shapes by name
    for (el, end_t, ex, ey, start_at, easing_kind) in pending_moves {
        if let Some(s) = shapes.iter_mut().find(|sh| sh.name() == el) {
            match s {
                Shape::Circle { animations, .. } | Shape::Rect { animations, .. } => {
                    animations.push(crate::scene::Animation::Move {
                        to_x: ex,
                        to_y: ey,
                        start: start_at,
                        end: end_t,
                        easing: easing_kind,
                    });
                }
                _ => {}
            }
        }
    }

    shapes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_top_level_move_and_attach() {
        let src = r##"size(1280, 720)
timeline { fps = 60; duration = 5 }

circle(name = "Circle", x = 0.500, y = 0.500, radius = 0.100, fill = "#78c8ff", spawn = 0.00)
move {
    element = "Circle"
    type = linear
    startAt = 0.000
    end {
        time = 5.000
        x = 0.700
        y = 0.500
    }
}
"##;

        let shapes = parse_dsl(src);
        assert_eq!(shapes.len(), 1);
        match &shapes[0] {
            crate::scene::Shape::Circle {
                animations, x, y, ..
            } => {
                assert_eq!(*x, 0.5);
                assert_eq!(*y, 0.5);
                assert_eq!(animations.len(), 1);
                if let crate::scene::Animation::Move {
                    to_x,
                    to_y,
                    start,
                    end,
                    ..
                } = animations[0]
                {
                    assert_eq!(start, 0.0);
                    assert_eq!(end, 5.0);
                    assert!((to_x - 0.7).abs() < 1e-6);
                    assert!((to_y - 0.5).abs() < 1e-6);
                } else {
                    panic!("expected Move animation");
                }
            }
            _ => panic!("expected a circle"),
        }
    }

    #[test]
    fn animated_xy_interpolates_for_move() {
        let src = r##"size(1280, 720)
timeline { fps = 60; duration = 5 }

circle(name = "Circle", x = 0.500, y = 0.500, radius = 0.100, fill = "#78c8ff", spawn = 0.00)
move {
    element = "Circle"
    type = linear
    startAt = 0.000
    end {
        time = 5.000
        x = 0.700
        y = 0.500
    }
}
"##;

        let shapes = parse_dsl(src);
        // sample at t = 2.5 (halfway) => x should be 0.6
        let (x, y) = crate::animations::animations_manager::animated_xy_for(&shapes[0], 2.5, 5.0);
        assert!((x - 0.6).abs() < 1e-5, "expected x ~= 0.6 but got {}", x);
        assert!((y - 0.5).abs() < 1e-5, "expected y ~= 0.5 but got {}", y);
    }
}

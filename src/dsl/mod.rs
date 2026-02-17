pub mod evaluator;
pub mod runtime;

use runtime::DslHandler;

use crate::scene::Shape;

/// Generate a simple DSL string for the given scene.
pub fn generate_dsl(scene: &[Shape], width: u32, height: u32, fps: u32, duration: f32) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "size({}, {})\ntimeline(fps = {}, duration = {:.2})\n\n",
        width, height, fps, duration
    ));
    // First, generate all shapes
    for s in scene.iter() {
        out.push_str(&s.to_dsl());
        out.push_str("\n");
    }

    // Then, generate all animations as top-level blocks
    for s in scene.iter() {
        let name = s.name();
        let animations = match s {
            Shape::Circle { animations, .. } | Shape::Rect { animations, .. } => animations,
            _ => continue,
        };

        for a in animations {
            match a {
                crate::scene::Animation::Move { .. } => {
                    if let Some(ma) =
                        crate::animations::move_animation::MoveAnimation::from_scene(a)
                    {
                        out.push_str(&ma.to_dsl_block(Some(&name), ""));
                        out.push_str("\n");
                    }
                }
            }
        }
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
    let mut width = None;
    let mut height = None;
    let mut fps = None;
    let mut duration = None;

    for line in code.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if line.starts_with("size") {
            let content = line
                .trim_start_matches("size")
                .trim_start_matches('(')
                .trim_end_matches(')')
                .trim_start_matches(':')
                .trim();
            let parts: Vec<&str> = content.split(',').collect();
            if parts.len() == 2 {
                width = parts[0].trim().parse().ok();
                height = parts[1].trim().parse().ok();
            }
        }

        if line.starts_with("timeline") {
            let content = line
                .trim_start_matches("timeline")
                .trim_matches(|c| c == '{' || c == '}' || c == '(' || c == ')')
                .trim();
            let stmts = if content.contains(';') {
                content.split(';')
            } else {
                content.split(',')
            };
            for part in stmts {
                let s = part.trim();
                if s.starts_with("fps") {
                    if let Some(eq) = s.find('=') {
                        fps = s[eq + 1..].trim().parse().ok();
                    } else {
                        fps = s
                            .trim_start_matches("fps")
                            .trim_start_matches(':')
                            .trim()
                            .parse()
                            .ok();
                    }
                }
                if s.starts_with("duration") {
                    if let Some(eq) = s.find('=') {
                        duration = s[eq + 1..].trim().parse().ok();
                    } else {
                        duration = s
                            .trim_start_matches("duration")
                            .trim_start_matches(':')
                            .trim()
                            .parse()
                            .ok();
                    }
                }
            }
        }
    }

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
    let mut shapes: Vec<Shape> = Vec::new();
    let mut lines = _src.lines().map(|l| l.trim()).peekable();

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

    // collect top-level move blocks that reference elements
    let mut pending_moves: Vec<(String, f32, f32, f32, f32, crate::scene::Easing)> = Vec::new();

    while let Some(line) = lines.next() {
        if line.is_empty() {
            continue;
        }

        // Event handler lines (e.g. `on_time { ... }`) are NOT parsed here —
        // we only extract/collect top-level handler *blocks* elsewhere so
        // individual event modules can keep their parsing/execution logic
        // inside their own files.

        // Handle shape definitions: circle "name" { ... } or circle(...) legacy
        if line.starts_with("circle") || line.starts_with("rect") {
            let is_circle = line.starts_with("circle");
            let mut name = String::new();

            // Extract name from circle "name" or circle(name="name")
            if let Some(quote_start) = line.find('"') {
                if let Some(quote_end) = line[quote_start + 1..].find('"') {
                    name = line[quote_start + 1..quote_start + 1 + quote_end].to_string();
                }
            } else if let Some(open_paren) = line.find('(') {
                let end_paren = line.rfind(')').unwrap_or(line.len());
                let inner = &line[open_paren + 1..end_paren];
                let kv = parse_kv_list(inner);
                if let Some(n) = kv.get("name") {
                    name = n.clone();
                }
            }

            if name.is_empty() {
                name = format!(
                    "{}_{}",
                    if is_circle { "Circle" } else { "Rect" },
                    shapes.len()
                );
            }

            let mut current_shape = if is_circle {
                Shape::Circle {
                    name,
                    x: 0.5,
                    y: 0.5,
                    radius: 0.1,
                    color: [120, 200, 255, 255],
                    spawn_time: 0.0,
                    animations: Vec::new(),
                    visible: true,
                }
            } else {
                Shape::Rect {
                    name,
                    x: 0.4,
                    y: 0.4,
                    w: 0.2,
                    h: 0.2,
                    color: [200, 120, 120, 255],
                    spawn_time: 0.0,
                    animations: Vec::new(),
                    visible: true,
                }
            };

            // If legacy format circle(x=1, y=2), apply values
            if let Some(open_paren) = line.find('(') {
                let end_paren = line.rfind(')').unwrap_or(line.len());
                let inner = &line[open_paren + 1..end_paren];
                let kv = parse_kv_list(inner);
                update_shape_from_kv(&mut current_shape, &kv);
            }

            // Check if block follows
            let mut has_block = line.contains('{');
            if !has_block {
                if let Some(next) = lines.peek() {
                    if next.starts_with('{') {
                        lines.next();
                        has_block = true;
                    }
                }
            }

            if has_block {
                while let Some(inner_line) = lines.next() {
                    let b = inner_line.trim();
                    if b == "}" {
                        break;
                    }
                    if b.is_empty() {
                        continue;
                    }

                    if b.starts_with("move") && (b.contains('{') || b.contains('(')) {
                        let mut move_lines = Vec::new();
                        if b.contains('{') {
                            // Block-based move
                            while let Some(m_line) = lines.next() {
                                let m = m_line.trim();
                                if m == "}" {
                                    break;
                                }
                                move_lines.push(m);
                            }
                        } else {
                            // Single line move(to=..., ...)
                            move_lines.push(b);
                        }

                        if let Some(parsed) =
                            crate::animations::move_animation::parse_move_block(&move_lines)
                        {
                            add_anim_to_shape(&mut current_shape, parsed);
                        }
                    } else if b.contains('=') {
                        let kv = parse_kv_list(b);
                        update_shape_from_kv(&mut current_shape, &kv);
                    }
                }
            }
            shapes.push(current_shape);
        } else if line.starts_with("move") && line.contains('{') {
            // top-level move block
            let mut move_lines = Vec::new();
            while let Some(m_line) = lines.next() {
                let m = m_line.trim();
                if m == "}" {
                    break;
                }
                move_lines.push(m);
            }
            if let Some(parsed) = crate::animations::move_animation::parse_move_block(&move_lines) {
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
        }
    }

    // Attach pending moves
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

/// Extract top-level event handler blocks from DSL source as structured objects.
pub fn extract_event_handlers_structured(src: &str) -> Vec<DslHandler> {
    let mut out = Vec::new();
    let mut chars = src.chars().enumerate().peekable();

    while let Some((i, c)) = chars.peek().cloned() {
        if c.is_whitespace() {
            chars.next();
            continue;
        }

        let remainder = &src[i..];
        let mut ident = String::new();
        for ch in remainder.chars() {
            if ch.is_alphanumeric() || ch == '_' {
                ident.push(ch);
            } else {
                break;
            }
        }

        if ident.is_empty() {
            chars.next();
            continue;
        }

        let id_len = ident.len();
        let mut found_block = false;
        let mut block_body = String::new();

        // Check if this identifier is followed by a '{'
        let after_ident = &remainder[id_len..];
        if let Some(brace_start) = after_ident.find('{') {
            // Verify there's only whitespace between ident and {
            if after_ident[..brace_start].trim().is_empty() {
                // Find matching closing brace
                let mut brace_count = 0;
                let abs_start = i + id_len + brace_start;
                let mut end_idx = 0;

                for (j, b) in src[abs_start..].chars().enumerate() {
                    if b == '{' {
                        brace_count += 1;
                    } else if b == '}' {
                        brace_count -= 1;
                        if brace_count == 0 {
                            end_idx = abs_start + j + 1;
                            block_body = src[abs_start + 1..abs_start + j].to_string();
                            found_block = true;
                            break;
                        }
                    }
                }

                if found_block {
                    // Always advance the outer iterator past the block so we don't
                    // re-parse the block body as top-level identifiers.
                    for _ in 0..(end_idx - i) {
                        chars.next();
                    }

                    // Only treat *recognized* event handler names as handlers.
                    // Currently we accept only `on_time` as a top-level event.
                    if let Some(col) = event_color(&ident) {
                        out.push(DslHandler {
                            name: ident,
                            body: block_body,
                            color: col,
                        });
                    } else {
                        // Unknown top-level block — skip registering it as a handler.
                    }

                    continue;
                }
            }
        }
        chars.next();
    }
    out
}

/// Return the display color for a known top-level event name (RGBA), or None
/// if the name is not a recognized event. Add new events here to whitelist
/// them for handler extraction + coloring in the editor.
pub fn event_color(name: &str) -> Option<[u8; 4]> {
    match name {
        "on_time" => Some([200, 100, 255, 255]), // purple for events
        _ => None,
    }
}

/// Return the display color for a known DSL method (e.g. `move_element`).
/// Methods listed here will get a distinct color in the editor; callers may
/// still allow per-call color override (e.g. `move_element(color = "#rrggbb")`).
pub fn method_color(name: &str) -> Option<[u8; 4]> {
    match name {
        "move_element" => Some([255, 160, 80, 255]), // orange for methods
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_on_time_is_extracted_as_handler() {
        let src = r#"
            on_time {
                move_element(name = "C", x = 0.1, y = 0.2)
            }

            asghasgbag {
                move_element(name = "C", x = 0.3, y = 0.4)
            }
        "#;

        let handlers = extract_event_handlers_structured(src);
        assert_eq!(handlers.len(), 1);
        assert_eq!(handlers[0].name, "on_time");
        assert_eq!(handlers[0].color, [200, 100, 255, 255]);
    }

    #[test]
    fn method_color_registry_contains_move_element() {
        let c = super::method_color("move_element").expect("move_element color");
        assert_eq!(c, [255, 160, 80, 255]);
    }
}

fn update_shape_from_kv(shape: &mut Shape, kv: &std::collections::HashMap<String, String>) {
    match shape {
        Shape::Circle {
            x,
            y,
            radius,
            color,
            spawn_time,
            ..
        } => {
            if let Some(v) = kv.get("x").and_then(|s| s.parse().ok()) {
                *x = v;
            }
            if let Some(v) = kv.get("y").and_then(|s| s.parse().ok()) {
                *y = v;
            }
            if let Some(v) = kv.get("radius").and_then(|s| s.parse().ok()) {
                *radius = v;
            }
            if let Some(v) = kv.get("spawn").and_then(|s| s.parse().ok()) {
                *spawn_time = v;
            }
            if let Some(fill) = kv.get("fill") {
                if fill.starts_with('#') && fill.len() >= 7 {
                    let r = u8::from_str_radix(&fill[1..3], 16).unwrap_or(120);
                    let g = u8::from_str_radix(&fill[3..5], 16).unwrap_or(200);
                    let b = u8::from_str_radix(&fill[5..7], 16).unwrap_or(255);
                    *color = [r, g, b, 255];
                }
            }
        }
        Shape::Rect {
            x,
            y,
            w,
            h,
            color,
            spawn_time,
            ..
        } => {
            if let Some(v) = kv.get("x").and_then(|s| s.parse().ok()) {
                *x = v;
            }
            if let Some(v) = kv.get("y").and_then(|s| s.parse().ok()) {
                *y = v;
            }
            if let Some(v) = kv.get("width").or(kv.get("w")).and_then(|s| s.parse().ok()) {
                *w = v;
            }
            if let Some(v) = kv
                .get("height")
                .or(kv.get("h"))
                .and_then(|s| s.parse().ok())
            {
                *h = v;
            }
            if let Some(v) = kv.get("spawn").and_then(|s| s.parse().ok()) {
                *spawn_time = v;
            }
            if let Some(fill) = kv.get("fill") {
                if fill.starts_with('#') && fill.len() >= 7 {
                    let r = u8::from_str_radix(&fill[1..3], 16).unwrap_or(200);
                    let g = u8::from_str_radix(&fill[3..5], 16).unwrap_or(120);
                    let b = u8::from_str_radix(&fill[5..7], 16).unwrap_or(120);
                    *color = [r, g, b, 255];
                }
            }
        }
        _ => {}
    }
}

fn add_anim_to_shape(shape: &mut Shape, parsed: crate::animations::move_animation::ParsedMove) {
    match shape {
        Shape::Circle { animations, .. } | Shape::Rect { animations, .. } => {
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

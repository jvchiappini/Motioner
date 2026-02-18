/// Parser for the Motioner DSL.
///
/// Converts raw DSL source text into structured AST nodes defined in
/// [`crate::dsl::ast`].  This module owns all parsing logic: shape blocks,
/// header directives, move animations, and event handlers.
///
/// The entry points are:
/// - [`parse`]  — full document parse (returns `Vec<Statement>`)
/// - [`parse_config`] — header-only parse (used by quick-validation path)
use std::collections::HashMap;

use super::ast::{
    BezierPoint, CircleNode, Color, EasingKind, EventHandlerNode, HeaderConfig, MoveBlock, Point2,
    RectNode, Statement, TextNode, TextSpan,
};
use super::lexer::extract_balanced;

// ─── Public entry points ──────────────────────────────────────────────────────

/// Parse a full DSL document and return the statement list.
///
/// Unknown or malformed constructs are silently skipped so the editor can
/// continue to display a partial scene while the user is still typing.
pub fn parse(src: &str) -> Vec<Statement> {
    let mut stmts = Vec::new();
    let mut lines = src.lines().map(str::trim).peekable();

    // Collect top-level move blocks that reference elements; we attach them
    // to the corresponding shape after all shapes have been parsed.
    let mut pending_moves: Vec<MoveBlock> = Vec::new();

    while let Some(line) = lines.next() {
        if line.is_empty() || line.starts_with("//") {
            continue;
        }

        if line.starts_with("size") {
            // Header is parsed once from the full source string.
            if let Ok(cfg) = parse_config(src) {
                stmts.push(Statement::Header(cfg));
            }
            // Skip remaining header lines inside the iterator.
            continue;
        }

        if line.starts_with("timeline") {
            // Already consumed as part of parse_config above.
            // Skip the block body if it spans multiple lines.
            if line.contains('{') && !line.contains('}') {
                for l in lines.by_ref() {
                    if l.trim() == "}" {
                        break;
                    }
                }
            }
            continue;
        }

        if line.starts_with("circle") {
            let block = collect_block(line, &mut lines);
            if let Some(node) = parse_circle(&block) {
                stmts.push(Statement::Circle(node));
            }
            continue;
        }

        if line.starts_with("rect") {
            let block = collect_block(line, &mut lines);
            if let Some(node) = parse_rect(&block) {
                stmts.push(Statement::Rect(node));
            }
            continue;
        }

        if line.starts_with("text") {
            let block = collect_block(line, &mut lines);
            if let Some(node) = parse_text(&block) {
                stmts.push(Statement::Text(node));
            }
            continue;
        }

        if line.starts_with("move") && line.contains('{') {
            let block = collect_block(line, &mut lines);
            if let Some(mv) = parse_move_block_lines(&block) {
                if mv.element.is_some() {
                    pending_moves.push(mv);
                }
            }
            continue;
        }

        // Event handlers: `on_time { ... }`
        let ident = first_ident(line);
        if !ident.is_empty() && line.contains('{') {
            if let Some(color) = event_handler_color(&ident) {
                let block = collect_block(line, &mut lines);
                let body = block_body_str(&block);
                stmts.push(Statement::EventHandler(EventHandlerNode {
                    event: ident,
                    body,
                    color,
                }));
            }
        }
    }

    // Attach top-level move blocks as `Statement::Move`.
    for mv in pending_moves {
        stmts.push(Statement::Move(mv));
    }

    stmts
}

/// Parse only the header section of a DSL document.
///
/// Returns [`HeaderConfig`] on success or a human-readable error string.
/// This is used by the quick-validation path in the editor so it stays fast.
pub fn parse_config(src: &str) -> Result<HeaderConfig, String> {
    let mut width: Option<u32> = None;
    let mut height: Option<u32> = None;
    let mut fps: Option<u32> = None;
    let mut duration: Option<f32> = None;

    // `size(w, h)`
    if let Some(pos) = src.find("size") {
        if let Some(inner) = extract_balanced(src, pos, '(', ')') {
            let parts: Vec<&str> = inner.split(',').collect();
            if parts.len() == 2 {
                width = parts[0].trim().parse().ok();
                height = parts[1].trim().parse().ok();
            }
        }
    }

    // `timeline(fps = N, duration = N)` or `timeline { fps = N, duration = N }`
    if let Some(pos) = src.find("timeline") {
        let inner = extract_balanced(src, pos, '(', ')')
            .or_else(|| extract_balanced(src, pos, '{', '}'))
            .unwrap_or_default();

        let sep: &[_] = if inner.contains(';') { &[';'] } else { &[','] };
        for part in inner.split(sep[0]) {
            let s = part.trim();
            if let Some(v) = parse_named_value(s, "fps") {
                fps = v.parse().ok();
            }
            if let Some(v) = parse_named_value(s, "duration") {
                duration = v.parse().ok();
            }
        }
    }

    match (width, height, fps, duration) {
        (Some(w), Some(h), Some(f), Some(d)) => Ok(HeaderConfig {
            width: w,
            height: h,
            fps: f,
            duration: d,
        }),
        (None, _, _, _) | (_, None, _, _) => {
            Err("Missing 'size(width, height)' configuration".to_string())
        }
        (_, _, None, _) => Err("Missing 'timeline { fps = ... }' configuration".to_string()),
        (_, _, _, None) => Err("Missing 'timeline { duration = ... }' configuration".to_string()),
    }
}

// ─── Shape parsers ────────────────────────────────────────────────────────────

fn parse_circle(block: &[String]) -> Option<CircleNode> {
    let header = block.first()?;
    let name = extract_name(header).unwrap_or_else(|| format!("Circle_{}", fastrand_usize()));

    let mut node = CircleNode {
        name,
        x: 0.5,
        y: 0.5,
        radius: 0.05,
        fill: None,
        spawn: 0.0,
        z_index: 0,
        animations: Vec::new(),
    };

    let body_lines = body_lines(block);
    let mut iter = body_lines.iter().peekable();

    while let Some(line) = iter.next() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }

        if line.starts_with("move") && line.contains('{') {
            let sub: Vec<String> = collect_sub_block(line, &mut iter);
            if let Some(mv) = parse_move_block_lines(&sub) {
                node.animations.push(mv);
            }
            continue;
        }

        if let Some((key, val)) = split_kv(line) {
            apply_circle_kv(&mut node, &key, &val);
        }
    }

    Some(node)
}

fn parse_rect(block: &[String]) -> Option<RectNode> {
    let header = block.first()?;
    let name = extract_name(header).unwrap_or_else(|| format!("Rect_{}", fastrand_usize()));

    let mut node = RectNode {
        name,
        x: 0.5,
        y: 0.5,
        w: 0.1,
        h: 0.1,
        fill: None,
        spawn: 0.0,
        z_index: 0,
        animations: Vec::new(),
    };

    let body_lines = body_lines(block);
    let mut iter = body_lines.iter().peekable();

    while let Some(line) = iter.next() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }

        if line.starts_with("move") && line.contains('{') {
            let sub = collect_sub_block(line, &mut iter);
            if let Some(mv) = parse_move_block_lines(&sub) {
                node.animations.push(mv);
            }
            continue;
        }

        if let Some((key, val)) = split_kv(line) {
            apply_rect_kv(&mut node, &key, &val);
        }
    }

    Some(node)
}

fn parse_text(block: &[String]) -> Option<TextNode> {
    let header = block.first()?;
    let name = extract_name(header).unwrap_or_else(|| format!("Text_{}", fastrand_usize()));

    let mut node = TextNode {
        name,
        x: 0.5,
        y: 0.5,
        size: 0.05,
        font: "System".to_string(),
        value: String::new(),
        fill: None,
        spawn: 0.0,
        z_index: 0,
        spans: Vec::new(),
        animations: Vec::new(),
    };

    let body_lines = body_lines(block);
    let mut iter = body_lines.iter().peekable();

    while let Some(line) = iter.next() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }

        if line.starts_with("move") && line.contains('{') {
            let sub = collect_sub_block(line, &mut iter);
            if let Some(mv) = parse_move_block_lines(&sub) {
                node.animations.push(mv);
            }
            continue;
        }

        if line.starts_with("spans") && line.contains('[') {
            // Collect span list: lines until the closing `]`.
            let mut span_lines = Vec::new();
            for sl in iter.by_ref() {
                let s = sl.trim();
                if s.starts_with(']') {
                    break;
                }
                span_lines.push(s.to_string());
            }
            node.spans = parse_spans(&span_lines);
            continue;
        }

        if let Some((key, val)) = split_kv(line) {
            apply_text_kv(&mut node, &key, &val);
        }
    }

    Some(node)
}

// ─── Move block parser ────────────────────────────────────────────────────────

/// Parse the inner lines of a `move { ... }` block.
///
/// This is also the canonical parser used by [`crate::animations::move_animation`].
pub fn parse_move_block_lines(block: &[String]) -> Option<MoveBlock> {
    let mut element: Option<String> = None;
    let mut to: Option<Point2> = None;
    let mut start_time: Option<f32> = None;
    let mut end_time: Option<f32> = None;
    let mut easing = EasingKind::Linear;

    // Skip the header line ("move {"), iterate body only.
    let body = body_lines(block);

    for line in &body {
        let line = line.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }

        let Some((key, val)) = split_kv(line) else {
            continue;
        };

        match key.as_str() {
            "element" => element = Some(val.trim_matches('"').to_string()),
            "to" => to = parse_point(&val),
            "during" => {
                if let Some(arrow) = val.find("->") {
                    start_time = val[..arrow].trim().parse().ok();
                    end_time = val[arrow + 2..].trim().parse().ok();
                }
            }
            "ease" | "easing" => easing = parse_easing(&val),
            _ => {}
        }
    }

    let to = to?;
    let start = start_time?;
    let end = end_time?;

    Some(MoveBlock {
        element,
        to,
        during: (start, end),
        easing,
    })
}

// ─── Easing parser ────────────────────────────────────────────────────────────

/// Parse a DSL easing string (right-hand side of `ease = ...`).
pub fn parse_easing(s: &str) -> EasingKind {
    let s = s.trim().trim_end_matches(',');

    if s == "linear" {
        return EasingKind::Linear;
    }
    if s == "sine" {
        return EasingKind::Sine;
    }
    if s == "expo" {
        return EasingKind::Expo;
    }
    if s == "circ" {
        return EasingKind::Circ;
    }

    // ease_in(power = N)
    if s.starts_with("ease_in_out") {
        let power = extract_f32_param(s, "power").unwrap_or(2.0);
        return EasingKind::EaseInOut { power };
    }
    if s.starts_with("ease_in") {
        let power = extract_f32_param(s, "power").unwrap_or(2.0);
        return EasingKind::EaseIn { power };
    }
    if s.starts_with("ease_out") {
        let power = extract_f32_param(s, "power").unwrap_or(2.0);
        return EasingKind::EaseOut { power };
    }

    // spring(damping = N, stiffness = N, mass = N)
    if s.starts_with("spring") {
        let damping = extract_f32_param(s, "damping").unwrap_or(10.0);
        let stiffness = extract_f32_param(s, "stiffness").unwrap_or(100.0);
        let mass = extract_f32_param(s, "mass").unwrap_or(1.0);
        return EasingKind::Spring {
            damping,
            stiffness,
            mass,
        };
    }

    // elastic(amplitude = N, period = N)
    if s.starts_with("elastic") {
        let amplitude = extract_f32_param(s, "amplitude").unwrap_or(1.0);
        let period = extract_f32_param(s, "period").unwrap_or(0.3);
        return EasingKind::Elastic { amplitude, period };
    }

    // bounce(bounciness = N)
    if s.starts_with("bounce") {
        let bounciness = extract_f32_param(s, "bounciness").unwrap_or(0.5);
        return EasingKind::Bounce { bounciness };
    }

    // bezier(p1 = (x, y), p2 = (x, y))
    if s.starts_with("bezier") && !s.starts_with("custom_bezier") {
        if let (Some(p1), Some(p2)) = (extract_point_param(s, "p1"), extract_point_param(s, "p2")) {
            return EasingKind::Bezier { p1, p2 };
        }
    }

    // custom_bezier(points = [...])
    if s.starts_with("custom_bezier") {
        let pts = parse_bezier_points(s);
        return EasingKind::CustomBezier { points: pts };
    }

    // custom(points = [(t, v), ...])
    if s.starts_with("custom") {
        let pts = parse_custom_points(s);
        return EasingKind::Custom { points: pts };
    }

    EasingKind::Linear
}

// ─── KV applicators ───────────────────────────────────────────────────────────

fn apply_circle_kv(node: &mut CircleNode, key: &str, val: &str) {
    match key {
        "x" => node.x = val.parse().unwrap_or(node.x),
        "y" => node.y = val.parse().unwrap_or(node.y),
        "radius" => node.radius = val.parse().unwrap_or(node.radius),
        "spawn" => node.spawn = val.parse().unwrap_or(node.spawn),
        "z" | "z_index" => node.z_index = val.parse().unwrap_or(node.z_index),
        "fill" => node.fill = Color::from_hex(val),
        _ => {}
    }
}

fn apply_rect_kv(node: &mut RectNode, key: &str, val: &str) {
    match key {
        "x" => node.x = val.parse().unwrap_or(node.x),
        "y" => node.y = val.parse().unwrap_or(node.y),
        "width" | "w" => node.w = val.parse().unwrap_or(node.w),
        "height" | "h" => node.h = val.parse().unwrap_or(node.h),
        "spawn" => node.spawn = val.parse().unwrap_or(node.spawn),
        "z" | "z_index" => node.z_index = val.parse().unwrap_or(node.z_index),
        "fill" => node.fill = Color::from_hex(val),
        _ => {}
    }
}

fn apply_text_kv(node: &mut TextNode, key: &str, val: &str) {
    match key {
        "x" => node.x = val.parse().unwrap_or(node.x),
        "y" => node.y = val.parse().unwrap_or(node.y),
        "size" => node.size = val.parse().unwrap_or(node.size),
        "spawn" => node.spawn = val.parse().unwrap_or(node.spawn),
        "z" | "z_index" => node.z_index = val.parse().unwrap_or(node.z_index),
        "value" => node.value = val.trim_matches('"').to_string(),
        "font" => node.font = val.trim_matches('"').to_string(),
        "fill" => node.fill = Color::from_hex(val),
        _ => {}
    }
}

// ─── Span parser ──────────────────────────────────────────────────────────────

fn parse_spans(lines: &[String]) -> Vec<TextSpan> {
    lines
        .iter()
        .filter_map(|l| {
            let l = l.trim();
            if !l.starts_with("span(") {
                return None;
            }
            let inner = l
                .trim_start_matches("span(")
                .trim_end_matches(')')
                .trim_end_matches(',');
            let kv = parse_kv_map(inner);
            let text = kv
                .get("text")
                .map(|s| s.trim_matches('"').to_string())
                .unwrap_or_default();
            let font = kv
                .get("font")
                .cloned()
                .unwrap_or_else(|| "System".to_string());
            let size = kv
                .get("size")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.033_f32);
            let color = kv
                .get("fill")
                .and_then(|s| Color::from_hex(s))
                .unwrap_or(Color {
                    r: 255,
                    g: 255,
                    b: 255,
                    a: 255,
                });
            Some(TextSpan {
                text,
                font,
                size,
                color,
            })
        })
        .collect()
}

// ─── Small utilities ─────────────────────────────────────────────────────────

/// Collect a block starting at `header` until the matching `}`.
/// Returns lines including the header and the closing `}`.
fn collect_block<'a, I>(header: &str, lines: &mut std::iter::Peekable<I>) -> Vec<String>
where
    I: Iterator<Item = &'a str>,
{
    let mut block = vec![header.to_string()];
    if !header.contains('{') {
        // Opening brace might be on the next line.
        if let Some(next) = lines.next() {
            block.push(next.to_string());
            if !next.contains('{') {
                return block;
            }
        }
    }

    let mut depth: i32 = header.chars().filter(|&c| c == '{').count() as i32
        - header.chars().filter(|&c| c == '}').count() as i32;

    while depth > 0 {
        if let Some(line) = lines.next() {
            depth += line.chars().filter(|&c| c == '{').count() as i32;
            depth -= line.chars().filter(|&c| c == '}').count() as i32;
            block.push(line.to_string());
        } else {
            break;
        }
    }
    block
}

/// Collect a nested sub-block from an already-collected body iterator.
fn collect_sub_block<'a, I>(header: &str, iter: &mut std::iter::Peekable<I>) -> Vec<String>
where
    I: Iterator<Item = &'a String>,
{
    let mut block = vec![header.to_string()];
    let mut depth: i32 = header.chars().filter(|&c| c == '{').count() as i32
        - header.chars().filter(|&c| c == '}').count() as i32;

    while depth > 0 {
        if let Some(line) = iter.next() {
            depth += line.chars().filter(|&c| c == '{').count() as i32;
            depth -= line.chars().filter(|&c| c == '}').count() as i32;
            block.push(line.clone());
        } else {
            break;
        }
    }
    block
}

/// Return the body lines of a collected block (everything between `{` and `}`).
fn body_lines(block: &[String]) -> Vec<String> {
    let mut in_body = false;
    let mut depth = 0i32;
    let mut result = Vec::new();

    for line in block {
        let trimmed = line.trim();
        for ch in trimmed.chars() {
            match ch {
                '{' => {
                    if !in_body {
                        in_body = true;
                    }
                    depth += 1;
                }
                '}' => {
                    depth -= 1;
                }
                _ => {}
            }
        }
        if in_body && depth >= 1 {
            result.push(trimmed.to_string());
        }
    }
    // Strip the opening brace line if it is the sole content of the first line
    result.retain(|l| l != "{");
    result
}

/// Return the raw body string from a collected block (for event handlers).
fn block_body_str(block: &[String]) -> String {
    body_lines(block).join("\n")
}

/// Extract the quoted name from a shape header line.
/// e.g. `circle "MyCircle" {` → `Some("MyCircle")`
fn extract_name(header: &str) -> Option<String> {
    let start = header.find('"')?;
    let rest = &header[start + 1..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// Return the first identifier in a line (letters, digits, `_`).
fn first_ident(s: &str) -> String {
    s.chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect()
}

/// Split `key = value` (strips trailing commas from the value).
fn split_kv(s: &str) -> Option<(String, String)> {
    let eq = s.find('=')?;
    let key = s[..eq].trim().to_string();
    let val = s[eq + 1..].trim().trim_end_matches(',').trim().to_string();
    if key.is_empty() {
        None
    } else {
        Some((key, val))
    }
}

/// Parse `key = val` or `key: val` from a short string fragment.
fn parse_named_value<'a>(s: &'a str, key: &str) -> Option<&'a str> {
    if s.starts_with(key) {
        if let Some(eq) = s.find('=').or_else(|| s.find(':')) {
            return Some(s[eq + 1..].trim().trim_end_matches(','));
        }
    }
    None
}

/// Parse `(x, y)` → `Point2`.
fn parse_point(s: &str) -> Option<Point2> {
    let s = s.trim();
    if s.starts_with('(') && s.ends_with(')') {
        let inner = &s[1..s.len() - 1];
        let parts: Vec<&str> = inner.split(',').collect();
        if parts.len() == 2 {
            let x: f32 = parts[0].trim().parse().ok()?;
            let y: f32 = parts[1].trim().parse().ok()?;
            return Some((x, y));
        }
    }
    None
}

/// Parse all `key = value` pairs from a comma-separated string.
fn parse_kv_map(s: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for part in s.split(',') {
        let p = part.trim();
        if let Some(eq) = p.find('=') {
            let key = p[..eq].trim().to_string();
            let val = p[eq + 1..].trim().trim_matches('"').to_string();
            map.insert(key, val);
        }
    }
    map
}

/// Extract a named f32 parameter from an easing string, e.g. `power = 3.0`.
fn extract_f32_param(s: &str, name: &str) -> Option<f32> {
    let needle = format!("{} =", name);
    let pos = s.find(needle.as_str())?;
    let rest = s[pos + needle.len()..].trim();
    let end = rest
        .find(|c: char| c == ',' || c == ')' || c == ' ')
        .unwrap_or(rest.len());
    rest[..end].trim().parse().ok()
}

/// Extract a named `(x, y)` parameter from an easing string.
fn extract_point_param(s: &str, name: &str) -> Option<Point2> {
    let needle = format!("{} =", name);
    let pos = s.find(needle.as_str())?;
    let rest = s[pos + needle.len()..].trim();
    parse_point(rest)
}

/// Parse `custom_bezier` point list from the full easing string.
fn parse_bezier_points(s: &str) -> Vec<BezierPoint> {
    let start = match s.find('[') {
        Some(i) => i,
        None => return Vec::new(),
    };
    let end = match s.rfind(']') {
        Some(i) => i,
        None => return Vec::new(),
    };
    let inner = &s[start + 1..end];

    // Each point: `((px, py), (lx, ly), (rx, ry))`
    let mut pts = Vec::new();
    let mut depth = 0i32;
    let mut current = String::new();
    for ch in inner.chars() {
        match ch {
            '(' => {
                depth += 1;
                current.push(ch);
            }
            ')' => {
                depth -= 1;
                current.push(ch);
                if depth == 0 {
                    // parse triple of points
                    let s = current.trim();
                    let inner = &s[1..s.len() - 1]; // strip outer parens
                    let sub: Vec<&str> = inner.splitn(3, "),").collect();
                    if sub.len() == 3 {
                        if let (Some(pos), Some(hl), Some(hr)) = (
                            parse_point(&format!("{})", sub[0].trim())),
                            parse_point(&format!("{})", sub[1].trim())),
                            parse_point(&format!("{})", sub[2].trim().trim_end_matches(')'))),
                        ) {
                            pts.push(BezierPoint {
                                pos,
                                handle_left: hl,
                                handle_right: hr,
                            });
                        }
                    }
                    current.clear();
                }
            }
            ',' if depth == 0 => {} // separator between points
            _ => {
                if depth > 0 {
                    current.push(ch);
                }
            }
        }
    }
    pts
}

/// Parse `custom` easing point list: `[(t, v), ...]`.
fn parse_custom_points(s: &str) -> Vec<Point2> {
    let start = match s.find('[') {
        Some(i) => i,
        None => return Vec::new(),
    };
    let end = match s.rfind(']') {
        Some(i) => i,
        None => return Vec::new(),
    };
    let inner = &s[start + 1..end];
    inner
        .split(',')
        .collect::<Vec<_>>()
        .chunks(2)
        .filter_map(|pair| {
            if pair.len() == 2 {
                let t: f32 = pair[0].trim().trim_matches('(').parse().ok()?;
                let v: f32 = pair[1].trim().trim_matches(')').parse().ok()?;
                Some((t, v))
            } else {
                None
            }
        })
        .collect()
}

// ─── Event handler registry ───────────────────────────────────────────────────

/// Return the editor display color (RGBA) for a recognized event name, or
/// `None` if it is not a known event kind.
///
/// **Add new events here** to make them recognized by the parser and highlighter.
pub fn event_handler_color(name: &str) -> Option<[u8; 4]> {
    match name {
        "on_time" => Some([200, 100, 255, 255]),
        _ => None,
    }
}

/// Return the editor display color for a known DSL method name, or `None`.
///
/// **Add new methods here** to make them highlighted in the editor.
pub fn method_color(name: &str) -> Option<[u8; 4]> {
    match name {
        "move_element" => Some([255, 160, 80, 255]),
        _ => None,
    }
}

// ─── Tiny helpers ─────────────────────────────────────────────────────────────

/// Deterministic "random" u32 for default names (avoids importing rand).
fn fastrand_usize() -> usize {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static CTR: AtomicUsize = AtomicUsize::new(0);
    CTR.fetch_add(1, Ordering::Relaxed)
}

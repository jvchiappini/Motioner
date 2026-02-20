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
    BezierPoint, EasingKind, EventHandlerNode, HeaderConfig, MoveBlock, Point2, Statement,
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

        // For any identified top-level block try delegating to the
        // registered shape parsers (kept in `shapes_manager`). This lets
        // each shape module own its parsing logic.
        if line.contains('{') {
            let first = first_ident(line);
            if !first.is_empty() {
                let block = collect_block(line, &mut lines);
                if let Some(shape) = crate::shapes::shapes_manager::parse_shape_block(&block) {
                    stmts.push(Statement::Shape(shape));
                    continue;
                }
            }
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

// per-shape parsing has been moved into each shape module and is invoked
// via the `ShapeParserFactory` registry in `shapes_manager`.

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

// shape-specific KV applicators are implemented inside each shape module now.

// ─── Span parser ──────────────────────────────────────────────────────────────

// span parsing lives in `src/shapes/text.rs` now.

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
pub(crate) fn collect_sub_block<'a, I>(
    header: &str,
    iter: &mut std::iter::Peekable<I>,
) -> Vec<String>
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
pub(crate) fn body_lines(block: &[String]) -> Vec<String> {
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

// (removed wrapper) use `body_lines` directly

/// Return the raw body string from a collected block (for event handlers).
fn block_body_str(block: &[String]) -> String {
    body_lines(block).join("\n")
}

/// Extract the quoted name from a shape header line.
/// e.g. `circle "MyCircle" {` → `Some("MyCircle")`
pub(crate) fn extract_name(header: &str) -> Option<String> {
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
pub(crate) fn split_kv(s: &str) -> Option<(String, String)> {
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
pub(crate) fn parse_point(s: &str) -> Option<Point2> {
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
pub(crate) fn parse_kv_map(s: &str) -> HashMap<String, String> {
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
pub(crate) fn fastrand_usize() -> usize {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static CTR: AtomicUsize = AtomicUsize::new(0);
    CTR.fetch_add(1, Ordering::Relaxed)
}

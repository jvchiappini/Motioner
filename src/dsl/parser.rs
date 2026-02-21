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

use super::ast::{EasingKind, EventHandlerNode, HeaderConfig, MoveBlock, Statement};
use super::lexer::extract_balanced;
use crate::dsl::utils;

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

        if line.starts_with("move") && line.contains('{') {
            let block = collect_block(line, &mut lines);
            if let Some(mv) = parse_move_block_lines(&block) {
                if mv.element.is_some() {
                    pending_moves.push(mv);
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
            if let Some(v) = utils::parse_named_value(s, "fps") {
                fps = v.parse::<u32>().ok();
            }
            if let Some(v) = utils::parse_named_value(s, "duration") {
                duration = v.parse::<f32>().ok();
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
    let mut to: Option<utils::Point2> = None; // use alias via utils or define separate
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
            "to" => to = utils::parse_point(&val),
            "during" => {
                if let Some(arrow) = val.find("->") {
                    start_time = val[..arrow].trim().parse().ok();
                    end_time = val[arrow + 2..].trim().parse().ok();
                }
            }
            "ease" | "easing" => easing = utils::parse_easing(&val),
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

pub(crate) fn split_kv(s: &str) -> Option<(String, String)> {
    utils::split_kv(s)
}

/// Parse all `key = value` pairs from a comma-separated string.
pub(crate) fn parse_kv_map(s: &str) -> HashMap<String, String> {
    utils::parse_kv_map(s)
}

// helpers above moved to utils; parser delegates there where needed.

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

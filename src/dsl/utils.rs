// Common helpers used by multiple DSL modules

use std::collections::HashMap;

// convenient alias re-exported for parser consumers
pub type Point2 = super::ast::Point2;

/// Collect a block starting at `header` until the matching `}`.
/// Returns lines including the header and the closing `}`.

/// Return the body lines of a collected block (everything between `{` and `}`).
/// Parse a DSL easing string into the AST easing kind.
///
/// The implementation mirrors the previous logic in `parser.rs` but lives
/// here so both grammar and evaluation layers can share it (animations can
/// also call it if needed).  Returning `Linear` on failure ensures the
/// surrounding parser remains resilient to malformed input.
pub fn parse_easing(s: &str) -> super::ast::EasingKind {
    let s = s.trim().trim_end_matches(',');

    // helper to parse named float parameters
    fn param(s: &str, name: &str) -> Option<f32> {
        let needle = format!("{} =", name);
        let pos = s.find(&needle)?;
        let rest = s[pos + needle.len()..].trim();
        let end = rest
            .find(|c: char| c == ',' || c == ')' || c == ' ')
            .unwrap_or(rest.len());
        rest[..end].trim().parse().ok()
    }

    if s == "linear" {
        return super::ast::EasingKind::Linear;
    }
    if s == "sine" {
        return super::ast::EasingKind::Sine;
    }
    if s == "expo" {
        return super::ast::EasingKind::Expo;
    }
    if s == "circ" {
        return super::ast::EasingKind::Circ;
    }

    if s.starts_with("ease_in_out") {
        let power = param(s, "power").unwrap_or(2.0);
        return super::ast::EasingKind::EaseInOut { power };
    }
    if s.starts_with("ease_in") {
        let power = param(s, "power").unwrap_or(2.0);
        return super::ast::EasingKind::EaseIn { power };
    }
    if s.starts_with("ease_out") {
        let power = param(s, "power").unwrap_or(2.0);
        return super::ast::EasingKind::EaseOut { power };
    }

    if s.starts_with("spring") {
        let damping = param(s, "damping").unwrap_or(10.0);
        let stiffness = param(s, "stiffness").unwrap_or(100.0);
        let mass = param(s, "mass").unwrap_or(1.0);
        return super::ast::EasingKind::Spring {
            damping,
            stiffness,
            mass,
        };
    }

    if s.starts_with("elastic") {
        let amplitude = param(s, "amplitude").unwrap_or(1.0);
        let period = param(s, "period").unwrap_or(0.3);
        return super::ast::EasingKind::Elastic { amplitude, period };
    }

    if s.starts_with("bounce") {
        let bounciness = param(s, "bounciness").unwrap_or(0.5);
        return super::ast::EasingKind::Bounce { bounciness };
    }

    // bezier(p1 = (x, y), p2 = (x, y))
    if s.starts_with("bezier") && !s.starts_with("custom_bezier") {
        if let (Some(p1), Some(p2)) = (extract_point_param(s, "p1"), extract_point_param(s, "p2")) {
            return super::ast::EasingKind::Bezier { p1, p2 };
        }
    }

    if s.starts_with("custom_bezier") {
        let pts = parse_bezier_points(s);
        return super::ast::EasingKind::CustomBezier { points: pts };
    }
    if s.starts_with("custom") {
        let pts = parse_custom_points(s);
        return super::ast::EasingKind::Custom { points: pts };
    }

    super::ast::EasingKind::Linear
}

/// Helper used by `parse_easing` to read a named `(x, y)` parameter.
pub fn extract_point_param(s: &str, name: &str) -> Option<(f32, f32)> {
    let needle = format!("{} =", name);
    let pos = s.find(&needle)?;
    let rest = s[pos + needle.len()..].trim();
    parse_point(rest)
}

/// Parse `custom_bezier` point list from the full easing string.
pub fn parse_bezier_points(s: &str) -> Vec<super::ast::BezierPoint> {
    let start = match s.find('[') {
        Some(i) => i,
        None => return Vec::new(),
    };
    let end = match s.rfind(']') {
        Some(i) => i,
        None => return Vec::new(),
    };
    let inner = &s[start + 1..end];

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
                    let inner = &s[1..s.len() - 1];
                    let sub: Vec<&str> = inner.splitn(3, "),").collect();
                    if sub.len() == 3 {
                        if let (Some(pos), Some(hl), Some(hr)) = (
                            parse_point(&format!("{})", sub[0].trim())),
                            parse_point(&format!("{})", sub[1].trim())),
                            parse_point(&format!("{})", sub[2].trim().trim_end_matches(')'))),
                        ) {
                            pts.push(super::ast::BezierPoint {
                                pos,
                                handle_left: hl,
                                handle_right: hr,
                            });
                        }
                    }
                    current.clear();
                }
            }
            ',' if depth == 0 => {}
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
pub fn parse_custom_points(s: &str) -> Vec<(f32, f32)> {
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
/// Split a top-level string of comma-separated key/value fragments, ignoring
/// commas inside parentheses/brackets/braces. Used by runtime when evaluating
/// handler shape blocks.
pub fn split_top_level_kvs(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut depth = 0i32;
    for ch in s.chars() {
        match ch {
            '(' | '{' | '[' => {
                depth += 1;
                cur.push(ch);
            }
            ')' | '}' | ']' => {
                depth = (depth - 1).max(0);
                cur.push(ch);
            }
            ',' if depth == 0 => {
                if !cur.trim().is_empty() {
                    out.push(cur.trim().to_string());
                }
                cur.clear();
            }
            _ => cur.push(ch),
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

/// Perform a quick validation of DSL source and, if the text is syntactically
/// valid, apply the canonical tab normalization in‑place.
/// The return value is the list of diagnostics produced by the validator.  The
/// caller is responsible for handling the case where the vector is non‑empty
/// (typically by blocking autosave or showing an error).
///
/// In either case we also apply the canonical tab normalization to the string
/// itself.  Previously the helper only rewrote the source when it parsed
/// successfully; callers that wanted normalization even when the DSL contained
/// errors had to duplicate `normalize_tabs` themselves.  Centralizing the
/// behaviour avoids that duplication and guarantees the editor always shows a
/// consistent, tab‑based representation no matter what the user types.
///
/// This helper now encapsulates the combination of `validate_dsl` and
/// `generator::normalize_tabs` that was scattered around the codebase.
pub fn validate_and_normalize(src: &mut String) -> Vec<super::validator::Diagnostic> {
    // Note: use the public facade functions so callers don't need to import
    // inner modules.
    let mut diags = super::validate_dsl(src);

    // Always normalise the indentation; this is a harmless transformation that
    // makes it easier for the UI to present a canonical view.  Doing it
    // unconditionally also simplifies callers, as they no longer need to
    // remember to perform a second normalization step when validation fails.
    let normalized = super::generator::normalize_tabs(src);
    if normalized != *src {
        *src = normalized;
    }

    diags
}

/// Return the top-level lines of a block body string (ignoring nested braces).
pub fn top_level_lines(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth: i32 = 0;
    let mut cur = String::new();
    for ch in body.chars() {
        if ch == '{' {
            depth += 1;
            cur.push(ch);
            continue;
        }
        if ch == '}' {
            depth -= 1;
            cur.push(ch);
            continue;
        }
        if ch == '\n' && depth == 0 {
            if !cur.trim().is_empty() {
                out.push(cur.trim().to_string());
            }
            cur.clear();
            continue;
        }
        cur.push(ch);
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

/// Parse all `key = value` pairs from a comma-separated string.
pub fn parse_kv_map(s: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for frag in split_top_level_kvs(s) {
        if let Some((k, v)) = split_kv(&frag) {
            map.insert(k, v);
        }
    }
    map
}
/// Return the raw body string from a collected block (for event handlers).

/// Extract the quoted name from a shape header line.
/// e.g. `circle "MyCircle" {` → `Some("MyCircle")`

/// Return the first identifier in a line (letters, digits, `_`).

/// Split `key = value` (strips trailing commas from the value).
pub fn split_kv(s: &str) -> Option<(String, String)> {
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
pub fn parse_named_value<'a>(s: &'a str, key: &str) -> Option<&'a str> {
    if s.starts_with(key) {
        if let Some(eq) = s.find('=').or_else(|| s.find(':')) {
            return Some(s[eq + 1..].trim().trim_end_matches(','));
        }
    }
    None
}

/// Parse `(x, y)` → `Point2`.
pub fn parse_point(s: &str) -> Option<(f32, f32)> {
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

// NOTE: `parse_kv_map` lives in shapes/utilities/common now (see that file).

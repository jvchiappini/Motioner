// Common helpers used by multiple DSL modules
use std::collections::HashMap;

pub type Point2 = super::ast::Point2;

pub fn parse_easing(s: &str) -> crate::scene::Easing {
    let s = s.trim().trim_end_matches(',');

    fn param(s: &str, name: &str) -> Option<f32> {
        let needle = format!("{} =", name);
        let pos = s.find(&needle)?;
        let rest = s[pos + needle.len()..].trim();
        let end = rest.find([',', ')', ' ']).unwrap_or(rest.len());
        rest[..end].trim().parse().ok()
    }

    if s == "linear" {
        return crate::scene::Easing::Linear;
    }
    if s == "step" {
        return crate::scene::Easing::Step;
    }

    if s.starts_with("ease_in_out") {
        let power = param(s, "power").unwrap_or(2.0);
        return crate::scene::Easing::EaseInOut { power };
    }
    if s.starts_with("ease_in") {
        let power = param(s, "power").unwrap_or(2.0);
        return crate::scene::Easing::EaseIn { power };
    }
    if s.starts_with("ease_out") {
        let power = param(s, "power").unwrap_or(2.0);
        return crate::scene::Easing::EaseOut { power };
    }

    crate::scene::Easing::Linear
}

pub fn split_top_level_kvs(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut depth = 0;
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

pub fn validate_and_normalize(src: &mut String) -> Vec<super::validator::Diagnostic> {
    let diags = super::validate(src);
    let normalized = super::generator::normalize_tabs(src);
    if normalized != *src {
        *src = normalized;
    }
    diags
}

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

pub fn parse_kv_map(s: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for frag in split_top_level_kvs(s) {
        if let Some((k, v)) = split_kv(&frag) {
            map.insert(k, v);
        }
    }
    map
}

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

pub fn parse_named_value<'a>(s: &'a str, key: &str) -> Option<&'a str> {
    if s.starts_with(key) {
        if let Some(eq) = s.find('=').or_else(|| s.find(':')) {
            return Some(s[eq + 1..].trim().trim_end_matches(','));
        }
    }
    None
}

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

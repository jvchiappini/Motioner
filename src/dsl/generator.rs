/// DSL code generator: converts a scene back into DSL source text.
///
/// This is the write direction of the DSL pipeline:
///   `Scene → DSL string`
///
/// The read direction lives in [`crate::dsl::parser`].

use super::parser;
use crate::scene::{Animation, Shape};
use crate::animations::move_animation::MoveAnimation;

// ─── Public entry points ──────────────────────────────────────────────────────

/// Generate the full DSL string for the given scene configuration.
pub fn generate(scene: &[Shape], width: u32, height: u32, fps: u32, duration: f32) -> String {
    let mut out = String::new();

    // Header
    out.push_str(&format!(
        "size({}, {})\ntimeline(fps = {}, duration = {:.2})\n\n",
        width, height, fps, duration
    ));

    // Shape definitions (without inline animations)
    for shape in scene {
        out.push_str(&shape.to_dsl(""));
        out.push('\n');
    }

    // Top-level move blocks referencing elements by name
    for shape in scene {
        let name = shape.name();
        let animations = shape_animations(shape);

        for anim in animations {
            if let Some(ma) = MoveAnimation::from_scene(anim) {
                out.push_str(&ma.to_dsl_block(Some(name), ""));
                out.push('\n');
            }
        }
    }

    out
}

/// Extract event handlers from DSL source as structured objects.
///
/// Only recognized event names (see [`event_handler_color`]) are returned;
/// unknown top-level blocks are silently ignored.
pub fn extract_event_handlers(src: &str) -> Vec<crate::dsl::runtime::DslHandler> {
    use crate::dsl::runtime::DslHandler;

    let mut out = Vec::new();
    let mut chars = src.chars().enumerate().peekable();

    while let Some((i, c)) = chars.peek().cloned() {
        if c.is_whitespace() {
            chars.next();
            continue;
        }

        let remainder = &src[i..];
        let ident: String = remainder
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();

        if ident.is_empty() {
            chars.next();
            continue;
        }

        let id_len = ident.len();
        let after_ident = &remainder[id_len..];

        if let Some(brace_offset) = after_ident.find('{') {
            // Ensure only whitespace sits between the identifier and `{`.
            if after_ident[..brace_offset].trim().is_empty() {
                let abs_start = i + id_len + brace_offset;
                let mut depth = 0i32;
                let mut end_idx = 0usize;
                let mut body = String::new();
                let mut found = false;

                for (j, b) in src[abs_start..].char_indices() {
                    match b {
                        '{' => depth += 1,
                        '}' => {
                            depth -= 1;
                            if depth == 0 {
                                body = src[abs_start + 1..abs_start + j].to_string();
                                end_idx = abs_start + j + 1;
                                found = true;
                                break;
                            }
                        }
                        _ => {}
                    }
                }

                if found {
                    // Advance the outer iterator past the consumed block.
                    for _ in 0..(end_idx - i) {
                        chars.next();
                    }

                    if let Some(color) = parser::event_handler_color(&ident) {
                        out.push(DslHandler { name: ident, body, color });
                    }
                    continue;
                }
            }
        }

        chars.next();
    }

    out
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn shape_animations(shape: &Shape) -> &[Animation] {
    match shape {
        Shape::Circle(c) => &c.animations,
        Shape::Rect(r) => &r.animations,
        Shape::Text(t) => &t.animations,
        _ => &[],
    }
}

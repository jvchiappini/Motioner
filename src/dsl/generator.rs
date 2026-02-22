/// DSL code generator: converts a scene back into DSL source text.
///
/// This is the write direction of the DSL pipeline:
///   `Scene → DSL string`
///
/// The read direction lives in [`crate::dsl::parser`].
use super::parser;
use crate::animations::move_animation::MoveAnimation;
use crate::scene::{Animation, Shape};
use crate::shapes::element_store::ElementKeyframes;

// ─── Public entry points ──────────────────────────────────────────────────────

/// Generate DSL directly from `ElementKeyframes` — the canonical scene store.
///
/// This is the preferred code-gen path: no intermediate `Vec<Shape>` clone
/// is needed.  Each element is materialized at its spawn frame only to obtain
/// its DSL serialization, so allocations are minimal and proportional to the
/// number of shapes, not the number of keyframes.
pub fn generate_from_elements(
    elements: &[ElementKeyframes],
    width: u32,
    height: u32,
    fps: u32,
    duration: f32,
) -> String {
    let mut out = String::new();

    // Header
    out.push_str(&format!(
        "size({}, {})\ntimeline(fps = {}, duration = {:.2})\n\n",
        width, height, fps, duration
    ));

    // Shape definitions — materialize each element at spawn frame just for DSL output.
    for elem in elements {
        if elem.ephemeral {
            continue;
        }
        if let Some(shape) = elem.to_shape_at_frame(elem.spawn_frame, fps) {
            out.push_str(&shape.to_dsl(""));
            out.push('\n');
        }
    }

    // NOTE: `ElementKeyframes` no longer carries top-level `Animation` lists.
    // Generating top-level animation blocks is handled when producing DSL
    // from `Shape` values (see `generate`) or from a future per-track store.

    out
}

/// Generate the full DSL string for the given scene configuration.
/// Prefer [`generate_from_elements`] when the scene is stored as `ElementKeyframes`.

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
                        out.push(DslHandler {
                            name: ident,
                            body,
                            color,
                        });
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

// legacy generator paths removed; only `generate_from_elements` is used.
// fn generate(scene: &[Shape], width: u32, height: u32, fps: u32, duration: f32) -> String { ... }
// fn shape_animations(shape: &Shape) -> &[Animation] { ... }

/// Convert leading groups of 4 spaces into tab characters for every line.
/// Only affects leading indentation — interior spaces are preserved.
pub fn normalize_tabs(src: &str) -> String {
    // Earlier versions of this helper used `src.lines()` followed by
    // `join("\n")`.  That approach discards any trailing newline(s) and
    // collapses multiple blank lines at the end, which in turn made the
    // editor strip the empty line the user had just created.  To faithfully
    // round-trip every character we now iterate over `split_inclusive('\n')`
    // so each segment retains its terminating newline (if present).  The
    // conversion logic is applied only to the line itself; the newline is
    // simply appended afterwards.

    let mut out = String::with_capacity(src.len());

    for segment in src.split_inclusive('\n') {
        // segment is either "...\n" or (for the final piece when the string
        // doesn't end with newline) "...".  A bare "\n" corresponds to an
        // empty line and we can push it directly.
        if segment == "\n" {
            out.push('\n');
            continue;
        }

        let has_newline = segment.ends_with('\n');
        let line = if has_newline {
            &segment[..segment.len() - 1]
        } else {
            segment
        };

        // perform the original indentation conversion on `line`
        let mut i = 0usize;
        let bytes = line.as_bytes();
        let mut leading = String::new();
        while i < bytes.len() {
            let c = bytes[i] as char;
            if c == '\t' {
                leading.push('\t');
                i += 1;
            } else if c == ' ' {
                let mut count = 0usize;
                while i + count < bytes.len() && bytes[i + count] == b' ' {
                    count += 1;
                }
                let tabs = count / 4;
                let rem = count % 4;
                for _ in 0..tabs {
                    leading.push('\t');
                }
                for _ in 0..rem {
                    leading.push(' ');
                }
                i += count;
            } else {
                break;
            }
        }
        out.push_str(&leading);
        out.push_str(&line[i..]);
        if has_newline {
            out.push('\n');
        }
    }

    out
}

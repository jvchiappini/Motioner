//! Minimal support for the Motioner DSL parser.
//!
//! Only a single helper (see [`extract_balanced`]) is exposed; the full lexer
//! implementation was removed when DSL parsing was disabled.

// Helper for parsers that need to pull out a balanced region of text.

/// Extracts a balanced region of text (e.g. `( ... )` or `{ ... }`) that
/// begins at `ident_pos` and uses the specified `open`/`close` characters.
/// Returns `None` if no matching close is found.
pub fn extract_balanced(src: &str, ident_pos: usize, open: char, close: char) -> Option<String> {
    let mut depth = 0;
    let mut out = String::new();
    for (_i, c) in src[ident_pos..].char_indices() {
        if c == open {
            depth += 1;
            if depth > 1 {
                out.push(c);
            }
        } else if c == close {
            depth -= 1;
            if depth == 0 {
                return Some(out);
            } else {
                out.push(c);
            }
        } else if depth >= 1 {
            out.push(c);
        }
    }
    None
}

// Only `extract_balanced` is exported; other helpers were deleted.

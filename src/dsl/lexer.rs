/// Lexer for the Motioner DSL.
///
/// Converts raw source text into a flat sequence of [`Token`]s.
/// The parser consumes this token stream to build the AST.

// This module used to contain a full lexer implementation with token
// definitions and span tracking.  The parser now operates directly on raw
// lines, so the only remaining helper is `extract_balanced` below.  The old
// types have been removed to satisfy `#![deny(dead_code)]`.

// ─── Lexer ────────────────────────────────────────────────────────────────────

/// Tokenises a DSL source string into a `Vec<SpannedToken>`.
///
/// Comments (`// …`) are silently skipped.
/// Errors (e.g. unterminated strings) produce a diagnostic but continue
/// tokenising so the caller can report all problems at once.
// The lexer used by the parser only needs a simple helper to extract a
// balanced parenthesized or brace-delimited substring.  The original token
// definitions and tokenizer logic are retained in history but removed from
// compilation because they are not referenced anywhere else.

/// Extracts a balanced region of text (e.g. `( ... )` or `{ ... }`) that
/// begins at `ident_pos` and uses the specified `open`/`close` characters.
/// Returns `None` if no matching close is found.
pub fn extract_balanced(src: &str, ident_pos: usize, open: char, close: char) -> Option<String> {
    let mut depth = 0;
    let mut out = String::new();
    for (i, c) in src[ident_pos..].char_indices() {
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

// ─── Helpers ─────────────────────────────────────────────────────────────────
// Only `extract_balanced` remains; previous helper functions were trimmed
// during the dead-code elimination.

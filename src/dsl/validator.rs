/// DSL validator: produces editor diagnostics from a source string.
///
/// Checks performed in order:
/// 1. Unterminated string literals
/// 2. Unbalanced / mismatched delimiters (`()`, `{}`, `[]`)
/// 3. Header config via [`crate::dsl::parser::parse_config`]
/// 4. Unknown top-level blocks
/// 5. Empty blocks
/// 6. Top-level `move {}` missing `element`
/// 7. Stray top-level assignments
use super::parser;

// ─── Diagnostic ───────────────────────────────────────────────────────────────

/// An editor-friendly diagnostic produced during DSL validation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub message: String,
    /// 1-based line number.
    pub line: usize,
    /// 1-based column number.
    pub column: usize,
}

// ─── Public entry point ───────────────────────────────────────────────────────

/// Validate DSL source and return all diagnostics.
///
/// An empty result means the source is valid.
pub fn validate(src: &str) -> Vec<Diagnostic> {
    let mut diags: Vec<Diagnostic> = Vec::new();

    check_unterminated_string(src, &mut diags);
    if !diags.is_empty() {
        // Unterminated string makes all further checks unreliable.
        return diags;
    }

    check_balanced_delimiters(src, &mut diags);
    check_header_config(src, &mut diags);
    check_top_level_blocks(src, &mut diags);

    diags
}

// ─── Checks ───────────────────────────────────────────────────────────────────

fn check_unterminated_string(src: &str, diags: &mut Vec<Diagnostic>) {
    let mut in_string = false;
    let mut string_start: Option<usize> = None;

    for (i, ch) in src.char_indices() {
        if ch == '"' {
            if !in_string {
                in_string = true;
                string_start = Some(i);
            } else {
                in_string = false;
                string_start = None;
            }
        }
    }

    if in_string {
        if let Some(pos) = string_start {
            let (ln, col) = byte_to_line_col(src, pos);
            diags.push(Diagnostic {
                message: "Unterminated string literal".to_string(),
                line: ln,
                column: col,
            });
        }
    }
}

fn check_balanced_delimiters(src: &str, diags: &mut Vec<Diagnostic>) {
    let mut stack: Vec<(char, usize)> = Vec::new();
    let mut in_string = false;

    for (i, ch) in src.char_indices() {
        if ch == '"' {
            in_string = !in_string;
            continue;
        }
        if in_string {
            continue;
        }
        match ch {
            '(' | '{' | '[' => stack.push((ch, i)),
            ')' | '}' | ']' => {
                if let Some((open, _)) = stack.pop() {
                    let expected = matching_close(open);
                    if ch != expected {
                        let (ln, col) = byte_to_line_col(src, i);
                        diags.push(Diagnostic {
                            message: format!("Mismatched delimiter — expected '{}'", expected),
                            line: ln,
                            column: col,
                        });
                    }
                } else {
                    let (ln, col) = byte_to_line_col(src, i);
                    diags.push(Diagnostic {
                        message: "Unmatched closing delimiter".to_string(),
                        line: ln,
                        column: col,
                    });
                }
            }
            _ => {}
        }
    }

    if let Some((open, pos)) = stack.first().cloned() {
        let (ln, col) = byte_to_line_col(src, pos);
        diags.push(Diagnostic {
            message: format!("Unclosed delimiter '{}'", open),
            line: ln,
            column: col,
        });
    }
}

fn check_header_config(src: &str, diags: &mut Vec<Diagnostic>) {
    if let Err(msg) = parser::parse_config(src) {
        let pos = src
            .find("size")
            .or_else(|| src.find("timeline"))
            .unwrap_or(0);
        let (ln, col) = byte_to_line_col(src, pos);
        diags.push(Diagnostic {
            message: msg,
            line: ln,
            column: col,
        });
    }
}

fn check_top_level_blocks(src: &str, diags: &mut Vec<Diagnostic>) {
    // Non-shape top-level keywords that are always allowed.
    const ALWAYS_ALLOWED: &[&str] = &["move", "write_text", "group", "on_time", "size", "timeline"];

    // Shape keywords come from the live registry — no hard-coded list needed.
    // Adding a new shape automatically makes it recognised here.
    let shape_keywords = crate::shapes::shapes_manager::registered_shape_keywords();

    let is_allowed = |tok: &str| ALWAYS_ALLOWED.contains(&tok) || shape_keywords.contains(&tok);

    let mut brace_depth: i32 = 0;
    let mut byte_offset: usize = 0;

    for line in src.lines() {
        let trimmed = line.trim();

        if brace_depth == 0 && !trimmed.is_empty() && !trimmed.starts_with("//") {
            let first_tok = trimmed
                .split(|c: char| c.is_whitespace() || c == '{' || c == '(' || c == '"')
                .next()
                .unwrap_or("");

            // Decide which kind of top-level content we're looking at.  The
            // previous implementation only handled blocks (with `{`) and
            // assignments (`=`); any other text was silently ignored.  That
            // allowed stray words such as `asdf` to slip through without
            // diagnostics even though they are not valid DSL.  We now treat
            // bare tokens as unexpected content too, which makes the editor
            // flag the line and prevents confusion.
            if trimmed.contains('{') {
                let open_pos = byte_offset + line.find('{').unwrap();
                if let Some(block_end) = find_matching_brace(src, open_pos) {
                    let body = &src[open_pos + 1..block_end - 1];

                    if body.trim().is_empty() {
                        let (ln, col) = byte_to_line_col(
                            src,
                            byte_offset + trimmed.find(first_tok).unwrap_or(0),
                        );
                        diags.push(Diagnostic {
                            message: format!("Empty block '{}' is not allowed", first_tok),
                            line: ln,
                            column: col,
                        });
                    }

                    if !is_allowed(first_tok) {
                        let (ln, col) = byte_to_line_col(
                            src,
                            byte_offset + trimmed.find(first_tok).unwrap_or(0),
                        );
                        diags.push(Diagnostic {
                            message: format!("Unknown top-level block '{}'", first_tok),
                            line: ln,
                            column: col,
                        });
                    }

                    if first_tok == "move" && !body.contains("element") {
                        let (ln, col) = byte_to_line_col(
                            src,
                            byte_offset + trimmed.find(first_tok).unwrap_or(0),
                        );
                        diags.push(Diagnostic {
                            message: "Top-level `move` block missing `element = \"Name\"`"
                                .to_string(),
                            line: ln,
                            column: col,
                        });
                    }
                }
            } else if trimmed.contains('=') {
                // Stray top-level assignment
                if !is_allowed(first_tok) {
                    let (ln, col) = byte_to_line_col(src, byte_offset);
                    diags.push(Diagnostic {
                        message: format!("Unexpected top-level content: '{}'", trimmed),
                        line: ln,
                        column: col,
                    });
                }
            } else {
                // No braces and no assignment -- e.g. a lone word or text.
                if !is_allowed(first_tok) {
                    let (ln, col) = byte_to_line_col(src, byte_offset);
                    diags.push(Diagnostic {
                        message: format!("Unexpected top-level content: '{}'", trimmed),
                        line: ln,
                        column: col,
                    });
                }
            }
        }

        // Track brace depth (ignore braces inside strings).
        let mut in_str = false;
        for ch in line.chars() {
            if ch == '"' {
                in_str = !in_str;
                continue;
            }
            if in_str {
                continue;
            }
            match ch {
                '{' => brace_depth += 1,
                '}' => {
                    if brace_depth > 0 {
                        brace_depth -= 1;
                    }
                }
                _ => {}
            }
        }

        byte_offset += line.len();
        if byte_offset < src.len() {
            byte_offset += 1; // newline
        }
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Convert a byte index into a 1-based (line, column) pair.
pub fn byte_to_line_col(src: &str, byte_idx: usize) -> (usize, usize) {
    let mut line = 1usize;
    let mut col = 1usize;
    let mut seen = 0usize;
    for ch in src.chars() {
        if seen >= byte_idx {
            break;
        }
        seen += ch.len_utf8();
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

/// Return the expected closing delimiter for the given opener.
fn matching_close(open: char) -> char {
    match open {
        '(' => ')',
        '{' => '}',
        '[' => ']',
        _ => '?',
    }
}

/// Find the byte index just past the `}` that closes the `{` at `open_pos`.
pub fn find_matching_brace(s: &str, open_pos: usize) -> Option<usize> {
    let mut depth: i32 = 0;
    let mut in_string = false;
    for (i, ch) in s[open_pos..].char_indices() {
        if ch == '"' {
            in_string = !in_string;
            continue;
        }
        if in_string {
            continue;
        }
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open_pos + i + 1);
                }
            }
            _ => {}
        }
    }
    None
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Run `validate` on `src`, returning just the diagnostic messages.
    ///
    /// Most of our unit tests are concerned with top-level content and not
    /// the mandatory header configuration, so we prepend a minimal header
    /// to the source before validating.  This avoids diagnostics such as
    /// "Missing 'size(width, height)' configuration" or missing timeline
    /// entries that the validator always emits when the header is absent.
    fn diag_msgs(src: &str) -> Vec<String> {
        let header = "size(1,1)\ntimeline(fps = 30, duration = 1.0)\n";
        let prefixed = format!("{}{}", header, src);
        validate(&prefixed).into_iter().map(|d| d.message).collect()
    }

    #[test]
    fn bare_unknown_word_produces_error() {
        let src = "asdf";
        let msgs = diag_msgs(src);
        assert_eq!(
            msgs,
            vec!["Unexpected top-level content: 'asdf'".to_string()]
        );
    }

    #[test]
    fn comment_alone_is_ok() {
        let src = "// just a comment";
        assert!(diag_msgs(src).is_empty());
    }

    #[test]
    fn known_keyword_without_block_is_ok() {
        let src = "size";
        // 'size' alone isn't particularly useful, but it's a recognised token
        // so the validator doesn't complain.  Existing behaviour preserved.
        assert!(diag_msgs(src).is_empty());
    }

    #[test]
    fn stray_assignment_with_unknown_key_errors() {
        let src = "foo = 123";
        let msgs = diag_msgs(src);
        assert_eq!(
            msgs,
            vec!["Unexpected top-level content: 'foo = 123'".to_string()]
        );
    }

    #[test]
    fn unknown_block_with_braces_errors() {
        let src = "foobar {\n}\n";
        let msgs = diag_msgs(src);
        // we expect at least the unknown-block diagnostic; an empty-block
        // warning may also be produced, which is harmless, so just verify the
        // presence of the primary message.
        assert!(msgs.contains(&"Unknown top-level block 'foobar'".to_string()));
    }
}

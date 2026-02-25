use serde::{Deserialize, Serialize};
use super::parser;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub message: String,
    pub line: usize,
    pub column: usize,
}

pub fn validate(src: &str) -> Vec<Diagnostic> {
    let mut diags: Vec<Diagnostic> = Vec::new();

    check_unterminated_string(src, &mut diags);
    if !diags.is_empty() {
        return diags;
    }

    check_balanced_delimiters(src, &mut diags);
    check_header_config(src, &mut diags);
    check_top_level_blocks(src, &mut diags);

    diags
}

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
                    let expected = match open {
                        '(' => ')',
                        '{' => '}',
                        '[' => ']',
                        _ => '?',
                    };
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
        let pos = src.find("size").or_else(|| src.find("timeline")).unwrap_or(0);
        let (ln, col) = byte_to_line_col(src, pos);
        diags.push(Diagnostic {
            message: msg,
            line: ln,
            column: col,
        });
    }
}

fn check_top_level_blocks(src: &str, diags: &mut Vec<Diagnostic>) {
    const ALWAYS_ALLOWED: &[&str] = &["move", "rect", "size", "timeline"];

    let mut brace_depth = 0;
    let mut byte_offset = 0;

    for line in src.lines() {
        let trimmed = line.trim();
        if brace_depth == 0 && !trimmed.is_empty() && !trimmed.starts_with("//") {
            let first_tok = trimmed
                .split(|c: char| c.is_whitespace() || c == '{' || c == '(' || c == '"')
                .next()
                .unwrap_or("");

            if !ALWAYS_ALLOWED.contains(&first_tok) {
                let (ln, col) = byte_to_line_col(src, byte_offset);
                diags.push(Diagnostic {
                    message: format!("Unknown top-level keyword '{}'", first_tok),
                    line: ln,
                    column: col,
                });
            }
        }

        let mut in_str = false;
        for ch in line.chars() {
            if ch == '"' { in_str = !in_str; continue; }
            if in_str { continue; }
            match ch {
                '{' => brace_depth += 1,
                '}' => if brace_depth > 0 { brace_depth -= 1; },
                _ => {}
            }
        }
        byte_offset += line.len() + 1; // approx
    }
}

pub fn byte_to_line_col(src: &str, byte_idx: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    let mut seen = 0;
    for ch in src.chars() {
        if seen >= byte_idx { break; }
        seen += ch.len_utf8();
        if ch == '\n' { line += 1; col = 1; } else { col += 1; }
    }
    (line, col)
}

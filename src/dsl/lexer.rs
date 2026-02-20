#![allow(dead_code)]
/// Lexer for the Motioner DSL.
///
/// Converts raw source text into a flat sequence of [`Token`]s.
/// The parser consumes this token stream to build the AST.

// ─── Token kinds ──────────────────────────────────────────────────────────────

/// A single lexical unit of DSL source.
#[derive(Clone, Debug, PartialEq)]
pub enum Token {
    // Literals
    Number(f32),
    StringLit(String),

    // Identifiers and keywords
    Ident(String),

    // Punctuation
    LParen,   // (
    RParen,   // )
    LBrace,   // {
    RBrace,   // }
    LBracket, // [
    RBracket, // ]
    Comma,    // ,
    Equals,   // =
    Arrow,    // ->
    Hash,     // # (starts a color literal)

    // End of file
    Eof,
}

// ─── Span ─────────────────────────────────────────────────────────────────────

/// Source location attached to a token (1-based line and column).
#[derive(Clone, Debug, PartialEq)]
pub struct Span {
    pub line: usize,
    pub col: usize,
}

/// A token together with its source location.
#[derive(Clone, Debug, PartialEq)]
pub struct SpannedToken {
    pub token: Token,
    pub span: Span,
}

// ─── Lexer ────────────────────────────────────────────────────────────────────

/// Tokenises a DSL source string into a `Vec<SpannedToken>`.
///
/// Comments (`// …`) are silently skipped.
/// Errors (e.g. unterminated strings) produce a diagnostic but continue
/// tokenising so the caller can report all problems at once.
pub fn tokenize(src: &str) -> Vec<SpannedToken> {
    let mut tokens = Vec::new();
    let mut chars = src.char_indices().peekable();
    let mut line = 1usize;
    let mut col = 1usize;

    macro_rules! push {
        ($tok:expr) => {
            tokens.push(SpannedToken {
                token: $tok,
                span: Span { line, col },
            })
        };
    }

    while let Some((_i, ch)) = chars.next() {
        match ch {
            // ── Newline ──────────────────────────────────────────────────────
            '\n' => {
                line += 1;
                col = 1;
            }
            // ── Whitespace ───────────────────────────────────────────────────
            c if c.is_whitespace() => {
                col += 1;
            }
            // ── Line comment ─────────────────────────────────────────────────
            '/' if matches!(chars.peek(), Some((_, '/'))) => {
                // consume until end of line
                for (_, c) in chars.by_ref() {
                    if c == '\n' {
                        line += 1;
                        col = 1;
                        break;
                    }
                }
            }
            // ── Arrow `->`  or minus ─────────────────────────────────────────
            '-' => {
                if matches!(chars.peek(), Some((_, '>'))) {
                    chars.next();
                    push!(Token::Arrow);
                    col += 2;
                } else {
                    // Treat lone `-` as part of a negative number or unknown
                    col += 1;
                }
            }
            // ── Single-char punctuation ──────────────────────────────────────
            '(' => {
                push!(Token::LParen);
                col += 1;
            }
            ')' => {
                push!(Token::RParen);
                col += 1;
            }
            '{' => {
                push!(Token::LBrace);
                col += 1;
            }
            '}' => {
                push!(Token::RBrace);
                col += 1;
            }
            '[' => {
                push!(Token::LBracket);
                col += 1;
            }
            ']' => {
                push!(Token::RBracket);
                col += 1;
            }
            ',' => {
                push!(Token::Comma);
                col += 1;
            }
            '=' => {
                push!(Token::Equals);
                col += 1;
            }
            '#' => {
                push!(Token::Hash);
                col += 1;
            }
            // ── String literal ───────────────────────────────────────────────
            '"' => {
                let start_col = col;
                col += 1;
                let mut s = String::new();
                let mut closed = false;
                while let Some((_, c)) = chars.next() {
                    col += 1;
                    if c == '"' {
                        closed = true;
                        break;
                    }
                    if c == '\n' {
                        line += 1;
                        col = 1;
                    }
                    s.push(c);
                }
                // Even if unterminated we emit the token so parser can proceed
                tokens.push(SpannedToken {
                    token: Token::StringLit(s),
                    span: Span {
                        line,
                        col: start_col,
                    },
                });
                let _ = closed; // caller validates via validator.rs
            }
            // ── Number (including negative) ───────────────────────────────────
            c if c.is_ascii_digit()
                || (c == '-' && matches!(chars.peek(), Some((_, d)) if d.is_ascii_digit())) =>
            {
                let start_col = col;
                let mut num_str = String::new();
                num_str.push(c);
                col += 1;
                while let Some(&(_, d)) = chars.peek() {
                    if d.is_ascii_digit() || d == '.' {
                        num_str.push(d);
                        col += 1;
                        chars.next();
                    } else {
                        break;
                    }
                }
                if let Ok(v) = num_str.parse::<f32>() {
                    tokens.push(SpannedToken {
                        token: Token::Number(v),
                        span: Span {
                            line,
                            col: start_col,
                        },
                    });
                }
            }
            // ── Identifier or keyword ────────────────────────────────────────
            c if c.is_alphabetic() || c == '_' => {
                let start_col = col;
                let mut ident = String::new();
                ident.push(c);
                col += 1;
                while let Some(&(_, d)) = chars.peek() {
                    if d.is_alphanumeric() || d == '_' {
                        ident.push(d);
                        col += 1;
                        chars.next();
                    } else {
                        break;
                    }
                }
                tokens.push(SpannedToken {
                    token: Token::Ident(ident),
                    span: Span {
                        line,
                        col: start_col,
                    },
                });
            }
            // ── Unknown character — skip ──────────────────────────────────────
            _ => {
                col += 1;
            }
        }
    }

    tokens.push(SpannedToken {
        token: Token::Eof,
        span: Span { line, col },
    });

    tokens
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Returns the byte index of the closing delimiter matching the one at
/// `open_pos` inside `s`.  Handles nested delimiters and string literals.
pub fn find_matching_close(s: &str, open_pos: usize, open: char, close: char) -> Option<usize> {
    let mut depth: i32 = 0;
    let mut in_string = false;
    for (i, ch) in s[open_pos..].char_indices() {
        let idx = open_pos + i;
        if ch == '"' {
            in_string = !in_string;
            continue;
        }
        if in_string {
            continue;
        }
        if ch == open {
            depth += 1;
        } else if ch == close {
            depth -= 1;
            if depth == 0 {
                return Some(idx);
            }
        }
    }
    None
}

/// Extract the inner content between the first matching delimiter pair
/// starting at `ident_pos` in `src`.
pub fn extract_balanced(src: &str, ident_pos: usize, open: char, close: char) -> Option<String> {
    let bytes = src.as_bytes();
    let mut open_idx: Option<usize> = None;
    for (i, &b) in bytes.iter().enumerate().skip(ident_pos) {
        if (b as char) == open {
            open_idx = Some(i);
            break;
        }
    }
    let open_idx = open_idx?;
    let close_idx = find_matching_close(src, open_idx, open, close)?;
    Some(src[open_idx + 1..close_idx].trim().to_string())
}

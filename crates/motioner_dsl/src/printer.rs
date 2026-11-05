//! Canonical MDF printer (prompt 12 phase 7).
//!
//! The printer is **source based**, not AST based: the lexer deliberately
//! discards `#` comments, so a formatter built on the AST alone would silently
//! drop them. Instead this module groups physical lines into logical
//! statements (a statement continues while a bracket is open), canonicalises
//! the tokens of every statement with the shared [`tokenize`] scanner, and
//! keeps comment-only lines and one trailing inline comment per statement.
//!
//! Canonical style:
//!
//! - four spaces per indentation level;
//! - one logical statement per line (calls and `def` headers are collapsed,
//!   even when the input spread them over several lines);
//! - spaces around arithmetic, comparison and assignment operators, one space
//!   after a comma and none inside brackets or around `.`;
//! - keyword arguments and default parameters keep the compact `name=value`
//!   form used by the examples;
//! - strings use double quotes; numbers go through
//!   [`motioner_core::format_number`] when that text reads back as exactly the
//!   same `f64` (otherwise the original literal is kept so formatting never
//!   changes the program's meaning or the rendered pixels);
//! - at most one blank line inside a block and two blank lines around a
//!   top-level `def`; a trailing newline; the input line ending is preserved
//!   (`CRLF` in, `CRLF` out).
//!
//! `format` is idempotent: formatting its own output changes nothing.
//!
//! A comment inside a multi-line bracketed construct cannot be preserved
//! verbatim without changing what it comments out, so the printer refuses that
//! one shape with a [`Diagnostic`] instead of dropping or moving the comment.
//! Every other comment (leading, comment-only, trailing inline) is kept.

use motioner_core::{format_number, quote};

use crate::diagnostic::{Diagnostic, ErrorCategory, Span};
use crate::lexer::tokenize;
use crate::parser::parse;
use crate::token::{Keyword, Op, Token, TokenKind};

/// Spaces per indentation level.
const INDENT: &str = "    ";

/// Formats `source` into the canonical MDF style, preserving comments.
///
/// # Errors
///
/// Returns the first lexical or parse [`Diagnostic`] when `source` is not a
/// valid MDF program, and an [`ErrorCategory::Unsupported`] diagnostic when a
/// comment sits inside a multi-line bracketed construct (the only construct
/// the printer refuses to rewrite).
pub fn format(source: &str) -> Result<String, Diagnostic> {
    parse(source)?;
    let eol = line_ending(source);
    let items = collect_items(source, eol)?;
    Ok(render_items(&items, eol))
}

/// Reports whether `source` is already in canonical form.
///
/// # Errors
///
/// Returns the first lexical or parse [`Diagnostic`] when `source` is not a
/// valid MDF program; otherwise returns one [`ErrorCategory::Value`]
/// diagnostic pointing at the first byte that differs from the canonical
/// output.
pub fn format_check(source: &str) -> Result<(), Vec<Diagnostic>> {
    let formatted = format(source).map_err(|diagnostic| vec![diagnostic])?;
    if formatted == source {
        Ok(())
    } else {
        Err(vec![not_canonical(source, &formatted)])
    }
}

/// Normalises one inserted snippet (a statement or expression) to the
/// canonical style, keeping it on one line and without indentation or a
/// trailing newline. Used by the planner for every generated statement.
#[must_use]
pub fn format_snippet(snippet: &str) -> String {
    let trimmed = snippet.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.starts_with('#') {
        return join_lines(trimmed);
    }
    let joined = join_lines(trimmed);
    if scan_line(&joined).comment.is_some() {
        // A comment cannot be reflowed token-by-token without deciding what it
        // comments out; keep the text intact instead of inventing a layout.
        return joined;
    }
    match tokenize(&joined) {
        Ok(tokens) => {
            let rendered = render_tokens(&tokens, &joined);
            if rendered.is_empty() {
                joined
            } else {
                rendered
            }
        }
        Err(_) => joined,
    }
}

/// Collapses the whitespace and newlines of a `def` header (or any call or
/// parameter list) into one canonical line.
///
/// This is the helper the write-through integration needs to promote a
/// literal to a parameter when the function header was written over several
/// lines. The result carries no indentation and no trailing newline, like
/// [`format_snippet`].
#[must_use]
pub fn format_signature(signature: &str) -> String {
    format_snippet(signature)
}

/// A significant output line: a code statement or a comment.
struct Item {
    kind: ItemKind,
    indent: usize,
    blank_before: bool,
    /// Whether this line belongs to the body of the most recent top-level
    /// `def` (the header itself included).
    top_def: bool,
}

impl Item {
    fn is_def(&self) -> bool {
        matches!(self.kind, ItemKind::Code { is_def: true, .. })
    }

    fn is_comment(&self) -> bool {
        matches!(self.kind, ItemKind::Comment(_))
    }
}

enum ItemKind {
    Code {
        text: String,
        is_def: bool,
        inline_comment: Option<String>,
    },
    Comment(String),
}

struct Builder {
    fragments: Vec<String>,
    indent: usize,
    blank_before: bool,
}

struct LineScan {
    /// Byte offset where the code ends (the comment starts, or end of line).
    code_end: usize,
    /// Byte offset of the `#` that starts a comment, when there is one.
    comment: Option<usize>,
    /// Net bracket depth change of the code part of the line.
    delta: i32,
}

fn line_ending(source: &str) -> &'static str {
    if source.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

/// Scans one physical line for the comment start and the bracket delta.
///
/// Comments and brackets inside string literals are ignored. UTF-8 bytes are
/// safe to inspect one by one: every ASCII delimiter is a single byte and
/// UTF-8 continuation bytes are never ASCII.
fn scan_line(line: &str) -> LineScan {
    let bytes = line.as_bytes();
    let mut index = 0;
    let mut quote: Option<u8> = None;
    let mut escaped = false;
    let mut delta = 0_i32;
    while index < bytes.len() {
        let byte = bytes[index];
        if let Some(open) = quote {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == open {
                quote = None;
            }
            index += 1;
            continue;
        }
        match byte {
            b'#' => {
                return LineScan {
                    code_end: index,
                    comment: Some(index),
                    delta,
                }
            }
            b'"' | b'\'' => quote = Some(byte),
            b'(' | b'[' | b'{' => delta += 1,
            b')' | b']' | b'}' => delta -= 1,
            _ => {}
        }
        index += 1;
    }
    LineScan {
        code_end: bytes.len(),
        comment: None,
        delta,
    }
}

fn collect_items(source: &str, eol: &str) -> Result<Vec<Item>, Diagnostic> {
    let mut items = Vec::new();
    let mut pending_blank = false;
    let mut depth = 0_i32;
    let mut current: Option<Builder> = None;
    let mut line_start = 0_usize;
    let eol_bytes = eol.len();
    for (index, line) in source.lines().enumerate() {
        let line_number = u32::try_from(index + 1).unwrap_or(u32::MAX);
        let scan = scan_line(line);
        let code = &line[..scan.code_end];
        let trimmed = code.trim();
        if let Some(builder) = current.as_mut() {
            if trimmed.is_empty() {
                if let Some(position) = scan.comment {
                    return Err(unsupported_comment(line_number, position, line_start, line));
                }
                // Blank lines inside brackets are layout only.
            } else {
                builder.fragments.push(trimmed.to_owned());
                depth += scan.delta;
                if depth <= 0 {
                    depth = 0;
                    let builder = current.take().expect("a builder exists in this branch");
                    let inline = scan.comment.map(|at| line[at..].trim_end().to_owned());
                    items.push(finish_code(&builder, inline)?);
                } else if scan.comment.is_some() {
                    return Err(unsupported_comment(
                        line_number,
                        scan.comment.unwrap_or(0),
                        line_start,
                        line,
                    ));
                }
            }
        } else if trimmed.is_empty() {
            if let Some(position) = scan.comment {
                let leading = line.len() - line.trim_start_matches(' ').len();
                items.push(Item {
                    kind: ItemKind::Comment(line[position..].trim_end().to_owned()),
                    indent: leading / INDENT.len(),
                    blank_before: pending_blank,
                    top_def: false,
                });
                pending_blank = false;
            } else {
                pending_blank = true;
            }
        } else {
            let leading = code.len() - code.trim_start_matches(' ').len();
            let builder = Builder {
                fragments: vec![trimmed.to_owned()],
                indent: leading / INDENT.len(),
                blank_before: pending_blank,
            };
            pending_blank = false;
            depth += scan.delta;
            if depth <= 0 {
                depth = 0;
                let inline = scan.comment.map(|at| line[at..].trim_end().to_owned());
                items.push(finish_code(&builder, inline)?);
            } else if scan.comment.is_some() {
                return Err(unsupported_comment(
                    line_number,
                    scan.comment.unwrap_or(0),
                    line_start,
                    line,
                ));
            } else {
                current = Some(builder);
            }
        }
        line_start += line.len() + eol_bytes;
    }
    // `parse` already rejects unclosed brackets; reaching here with an open
    // builder would mean the scanner and the lexer disagree.
    if current.is_some() {
        return Err(Diagnostic::new(
            ErrorCategory::Lexical,
            Span::default(),
            "this bracketed statement is never closed",
        ));
    }
    assign_top_def(&mut items);
    Ok(items)
}

/// Marks the lines that belong to the most recent top-level `def` block.
///
/// `blank_lines` needs this to put two blank lines after a definition body
/// (the last item of a def is indented, not the `def` header itself).
fn assign_top_def(items: &mut [Item]) {
    let mut inside = false;
    for item in items {
        if item.indent == 0 {
            inside = item.is_def();
            item.top_def = inside;
        } else {
            item.top_def = inside;
        }
    }
}

fn unsupported_comment(line: u32, column: usize, line_start: usize, text: &str) -> Diagnostic {
    Diagnostic::new(
        ErrorCategory::Unsupported,
        Span {
            start: line_start + column,
            end: line_start + column,
            line,
            column: u32::try_from(text[..column].chars().count() + 1).unwrap_or(u32::MAX),
        },
        "a comment inside a multi-line call or header cannot be formatted safely; put it on its own line",
    )
}

fn finish_code(builder: &Builder, inline_comment: Option<String>) -> Result<Item, Diagnostic> {
    let joined = builder.fragments.join(" ");
    let tokens = tokenize(&joined)?;
    let text = render_tokens(&tokens, &joined);
    let is_def = tokens
        .iter()
        .find(|token| !is_trivia(&token.kind))
        .is_some_and(|token| token.kind == TokenKind::Keyword(Keyword::Def));
    Ok(Item {
        kind: ItemKind::Code {
            text,
            is_def,
            inline_comment,
        },
        indent: builder.indent,
        blank_before: builder.blank_before,
        top_def: false,
    })
}

fn render_items(items: &[Item], eol: &str) -> String {
    let mut out = String::new();
    let mut previous: Option<&Item> = None;
    for item in items {
        if let Some(previous) = previous {
            for _ in 0..blank_lines(previous, item) {
                out.push_str(eol);
            }
        }
        for _ in 0..item.indent {
            out.push_str(INDENT);
        }
        match &item.kind {
            ItemKind::Code {
                text,
                inline_comment,
                ..
            } => {
                out.push_str(text);
                if let Some(comment) = inline_comment {
                    out.push_str("  ");
                    out.push_str(comment);
                }
            }
            ItemKind::Comment(text) => out.push_str(text),
        }
        out.push_str(eol);
        previous = Some(item);
    }
    out
}

/// How many blank lines go between two significant items.
///
/// The input's blank lines are collapsed to one, except around a top-level
/// `def`: two blank lines follow a definition body and precede a new top-level
/// definition that is not already introduced by a comment block.
fn blank_lines(previous: &Item, current: &Item) -> usize {
    let mut blanks = usize::from(current.blank_before);
    let leaves_top_def = previous.top_def && current.indent == 0;
    let enters_top_def = current.indent == 0 && current.is_def() && !previous.is_comment();
    if leaves_top_def || enters_top_def {
        blanks = 2;
    }
    blanks.min(2)
}

/// Renders a token stream with canonical spacing.
fn render_tokens(tokens: &[Token], source: &str) -> String {
    let significant: Vec<&Token> = tokens
        .iter()
        .filter(|token| !is_trivia(&token.kind))
        .collect();
    let skip = trailing_comma_skips(&significant);
    let mut out = String::new();
    let mut previous: Option<&Token> = None;
    let mut previous_unary = false;
    let mut depth = 0_i32;
    for (index, token) in significant.iter().copied().enumerate() {
        if skip[index] {
            continue;
        }
        if is_closing(&token.kind) {
            depth -= 1;
        }
        let unary = starts_expression(previous)
            && matches!(token.kind, TokenKind::Op(Op::Minus | Op::Plus));
        if let Some(previous) = previous {
            if !previous_unary && pair_space(previous, token, depth) {
                out.push(' ');
            }
        }
        out.push_str(&token_text(token, source));
        if is_opening(&token.kind) {
            depth += 1;
        }
        previous_unary = unary;
        previous = Some(token);
    }
    out
}

/// Marks trailing commas that can be removed.
///
/// A trailing comma is required only for a one-element tuple such as `(1,)`;
/// removing it would turn the tuple into a grouping and change the program's
/// meaning. Calls (`f(1,)`), parameter lists (`def f(a,)`) and lists
/// (`[1,]`) lose it.
fn trailing_comma_skips(tokens: &[&Token]) -> Vec<bool> {
    let mut skip = vec![false; tokens.len()];
    let mut stack: Vec<usize> = Vec::new();
    let mut close_to_open: Vec<Option<usize>> = vec![None; tokens.len()];
    let mut direct_commas: Vec<usize> = vec![0; tokens.len()];
    for (index, token) in tokens.iter().enumerate() {
        if is_opening(&token.kind) {
            stack.push(index);
        } else if is_closing(&token.kind) {
            if let Some(open) = stack.pop() {
                close_to_open[index] = Some(open);
            }
        } else if matches!(token.kind, TokenKind::Op(Op::Comma)) {
            if let Some(&open) = stack.last() {
                direct_commas[open] += 1;
            }
        }
    }
    for (index, token) in tokens.iter().enumerate() {
        if !matches!(token.kind, TokenKind::Op(Op::Comma)) {
            continue;
        }
        let Some(next) = tokens.get(index + 1) else {
            continue;
        };
        if !is_closing(&next.kind) {
            continue;
        }
        let Some(open) = close_to_open[index + 1] else {
            continue;
        };
        let call_like = open > 0 && is_value_like(&tokens[open - 1].kind);
        let required = matches!(tokens[open].kind, TokenKind::Op(Op::LParen))
            && !call_like
            && direct_commas[open] == 1;
        if !required {
            skip[index] = true;
        }
    }
    skip
}

fn is_trivia(kind: &TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Newline | TokenKind::Indent | TokenKind::Dedent | TokenKind::Eof
    )
}

fn is_opening(kind: &TokenKind) -> bool {
    matches!(kind, TokenKind::Op(Op::LParen | Op::LBracket | Op::LBrace))
}

fn is_closing(kind: &TokenKind) -> bool {
    matches!(kind, TokenKind::Op(Op::RParen | Op::RBracket | Op::RBrace))
}

fn is_assignment(kind: &TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Op(
            Op::Assign | Op::PlusAssign | Op::MinusAssign | Op::StarAssign | Op::SlashAssign
        )
    )
}

fn is_value_like(kind: &TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Name(_)
            | TokenKind::Number(_)
            | TokenKind::Str(_)
            | TokenKind::Op(Op::RParen | Op::RBracket | Op::RBrace)
    )
}

fn starts_expression(previous: Option<&Token>) -> bool {
    match previous {
        None => true,
        Some(token) => match &token.kind {
            TokenKind::Name(_) | TokenKind::Number(_) | TokenKind::Str(_) => false,
            TokenKind::Op(op) => !matches!(op, Op::RParen | Op::RBracket | Op::RBrace),
            TokenKind::Keyword(keyword) => {
                !matches!(keyword, Keyword::True | Keyword::False | Keyword::None)
            }
            TokenKind::Newline | TokenKind::Indent | TokenKind::Dedent | TokenKind::Eof => true,
        },
    }
}

fn pair_space(previous: &Token, current: &Token, depth: i32) -> bool {
    if matches!(previous.kind, TokenKind::Op(Op::Dot))
        || matches!(current.kind, TokenKind::Op(Op::Dot))
    {
        return false;
    }
    if matches!(current.kind, TokenKind::Op(Op::Comma | Op::Colon)) {
        return false;
    }
    if is_opening(&previous.kind) || is_closing(&current.kind) {
        return false;
    }
    if matches!(previous.kind, TokenKind::Op(Op::Comma | Op::Colon)) {
        return true;
    }
    if matches!(current.kind, TokenKind::Op(Op::LParen | Op::LBracket)) {
        // A keyword argument or default keeps the compact `name=(...)` form
        // inside a parameter/argument list; a top-level assignment keeps its
        // spaces (`x = (1, 2)`).
        if depth > 0 && is_assignment(&previous.kind) {
            return false;
        }
        return !is_value_like(&previous.kind);
    }
    if matches!(current.kind, TokenKind::Op(Op::LBrace)) {
        return true;
    }
    if is_assignment(&previous.kind) || is_assignment(&current.kind) {
        return depth == 0;
    }
    true
}

fn token_text(token: &Token, source: &str) -> String {
    match &token.kind {
        TokenKind::Name(name) => name.to_string(),
        TokenKind::Number(value) => {
            let original = source
                .get(token.span.start..token.span.end)
                .unwrap_or_default();
            number_text(*value, original)
        }
        TokenKind::Str(value) => quote(value),
        TokenKind::Keyword(keyword) => keyword_text(*keyword).to_owned(),
        TokenKind::Op(op) => op_text(*op).to_owned(),
        TokenKind::Newline | TokenKind::Indent | TokenKind::Dedent | TokenKind::Eof => {
            String::new()
        }
    }
}

/// `format_number` when it round-trips exactly, else the original literal.
fn number_text(value: f64, original: &str) -> String {
    let candidate = format_number(value);
    match candidate.parse::<f64>() {
        Ok(parsed) if parsed.to_bits() == value.to_bits() => candidate,
        _ => original.to_owned(),
    }
}

fn keyword_text(keyword: Keyword) -> &'static str {
    match keyword {
        Keyword::Def => "def",
        Keyword::Return => "return",
        Keyword::If => "if",
        Keyword::Elif => "elif",
        Keyword::Else => "else",
        Keyword::For => "for",
        Keyword::In => "in",
        Keyword::While => "while",
        Keyword::Break => "break",
        Keyword::Continue => "continue",
        Keyword::Pass => "pass",
        Keyword::True => "True",
        Keyword::False => "False",
        Keyword::None => "None",
        Keyword::And => "and",
        Keyword::Or => "or",
        Keyword::Not => "not",
        Keyword::Use => "use",
    }
}

fn op_text(op: Op) -> &'static str {
    match op {
        Op::Plus => "+",
        Op::Minus => "-",
        Op::Star => "*",
        Op::Slash => "/",
        Op::SlashSlash => "//",
        Op::Percent => "%",
        Op::Assign => "=",
        Op::PlusAssign => "+=",
        Op::MinusAssign => "-=",
        Op::StarAssign => "*=",
        Op::SlashAssign => "/=",
        Op::EqEq => "==",
        Op::NotEq => "!=",
        Op::Lt => "<",
        Op::Le => "<=",
        Op::Gt => ">",
        Op::Ge => ">=",
        Op::LParen => "(",
        Op::RParen => ")",
        Op::LBracket => "[",
        Op::RBracket => "]",
        Op::LBrace => "{",
        Op::RBrace => "}",
        Op::Comma => ",",
        Op::Colon => ":",
        Op::Dot => ".",
    }
}

fn join_lines(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn not_canonical(source: &str, formatted: &str) -> Diagnostic {
    let mut first = 0_usize;
    for (left, right) in source.bytes().zip(formatted.bytes()) {
        if left != right {
            break;
        }
        first += 1;
    }
    let mut start = first.min(source.len());
    while start > 0 && !source.is_char_boundary(start) {
        start -= 1;
    }
    let prefix = &source[..start];
    let line =
        u32::try_from(prefix.bytes().filter(|byte| *byte == b'\n').count() + 1).unwrap_or(u32::MAX);
    let column = u32::try_from(
        prefix
            .rsplit('\n')
            .next()
            .unwrap_or_default()
            .chars()
            .count()
            + 1,
    )
    .unwrap_or(u32::MAX);
    Diagnostic::new(
        ErrorCategory::Value,
        Span {
            start,
            end: start,
            line,
            column,
        },
        "file is not in canonical MDF style; run `motioner fmt`",
    )
}

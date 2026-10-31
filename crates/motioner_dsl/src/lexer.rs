//! Hand-written lexer with Python-style indentation tokens.
//!
//! Rules (spec section 3): spaces only, four per indentation level; blank
//! and comment-only lines do not affect indentation; newlines inside
//! brackets join lines.

use std::rc::Rc;

use crate::diagnostic::{Diagnostic, ErrorCategory, Span};
use crate::token::{Keyword, Op, Token, TokenKind};

/// Spaces per indentation level.
const INDENT_WIDTH: u32 = 4;

/// Splits MDF source into tokens, ending with [`TokenKind::Eof`].
///
/// # Errors
///
/// Returns a [`ErrorCategory::Lexical`] diagnostic for tabs, bad
/// indentation, unknown characters, malformed literals and unclosed
/// brackets or strings.
pub fn tokenize(source: &str) -> Result<Vec<Token>, Diagnostic> {
    Lexer::new(source).run()
}

struct Lexer<'s> {
    source: &'s str,
    chars: Vec<(usize, char)>,
    pos: usize,
    line: u32,
    column: u32,
    tokens: Vec<Token>,
    indents: Vec<u32>,
    open_brackets: Vec<Span>,
    at_line_start: bool,
}

/// Start position of a token being scanned.
#[derive(Clone, Copy)]
struct Mark {
    byte: usize,
    line: u32,
    column: u32,
}

impl<'s> Lexer<'s> {
    fn new(source: &'s str) -> Self {
        Self {
            source,
            chars: source.char_indices().collect(),
            pos: 0,
            line: 1,
            column: 1,
            tokens: Vec::new(),
            indents: vec![0],
            open_brackets: Vec::new(),
            at_line_start: true,
        }
    }

    fn run(mut self) -> Result<Vec<Token>, Diagnostic> {
        while self.pos < self.chars.len() {
            if self.at_line_start && self.open_brackets.is_empty() {
                self.at_line_start = false;
                self.indentation()?;
                continue;
            }
            self.token()?;
        }
        self.finish()
    }

    fn peek(&self, offset: usize) -> Option<char> {
        self.chars.get(self.pos + offset).map(|&(_, c)| c)
    }

    fn byte_offset(&self) -> usize {
        self.chars
            .get(self.pos)
            .map_or(self.source.len(), |&(byte, _)| byte)
    }

    fn mark(&self) -> Mark {
        Mark {
            byte: self.byte_offset(),
            line: self.line,
            column: self.column,
        }
    }

    fn span_from(&self, mark: Mark) -> Span {
        Span {
            start: mark.byte,
            end: self.byte_offset(),
            line: mark.line,
            column: mark.column,
        }
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek(0)?;
        self.pos += 1;
        if c == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        Some(c)
    }

    fn error(&self, mark: Mark, message: impl Into<String>) -> Diagnostic {
        Diagnostic::new(ErrorCategory::Lexical, self.span_from(mark), message)
    }

    fn push(&mut self, kind: TokenKind, mark: Mark) {
        let span = self.span_from(mark);
        self.tokens.push(Token { kind, span });
    }

    fn last_is_newline(&self) -> bool {
        self.tokens
            .last()
            .is_none_or(|t| t.kind == TokenKind::Newline)
    }

    /// Measures leading spaces and emits `Indent`/`Dedent` tokens.
    fn indentation(&mut self) -> Result<(), Diagnostic> {
        let mark = self.mark();
        let mut width = 0u32;
        while let Some(c) = self.peek(0) {
            match c {
                ' ' => width += 1,
                '\t' => {
                    return Err(self.error(self.mark(), "tabs are not allowed; indent with spaces"))
                }
                _ => break,
            }
            self.bump();
        }
        if matches!(self.peek(0), None | Some('\n' | '\r' | '#')) {
            return Ok(()); // Blank or comment-only line.
        }
        let current = self.indents.last().copied().unwrap_or(0);
        if width > current {
            if width != current + INDENT_WIDTH {
                return Err(self.error(mark, "indentation must increase by exactly 4 spaces"));
            }
            self.indents.push(width);
            self.push(TokenKind::Indent, mark);
        } else {
            while self.indents.last().copied().unwrap_or(0) > width {
                self.indents.pop();
                self.push(TokenKind::Dedent, mark);
            }
            if self.indents.last().copied().unwrap_or(0) != width {
                return Err(self.error(mark, "unindent does not match any outer indentation level"));
            }
        }
        Ok(())
    }

    fn token(&mut self) -> Result<(), Diagnostic> {
        let mark = self.mark();
        let Some(c) = self.peek(0) else {
            return Ok(());
        };
        match c {
            '\n' => {
                self.bump();
                if self.open_brackets.is_empty() {
                    if !self.last_is_newline() {
                        self.push(TokenKind::Newline, mark);
                    }
                    self.at_line_start = true;
                }
            }
            ' ' | '\r' => {
                self.bump();
            }
            '\t' => return Err(self.error(mark, "tabs are not allowed; use spaces")),
            '#' => {
                while self.peek(0).is_some_and(|c| c != '\n') {
                    self.bump();
                }
            }
            '0'..='9' => self.number(mark)?,
            '"' | '\'' => self.string(mark, false)?,
            c if c == '_' || c.is_ascii_alphabetic() => self.word(mark)?,
            _ => self.operator(mark)?,
        }
        Ok(())
    }

    fn number(&mut self, mark: Mark) -> Result<(), Diagnostic> {
        self.digits();
        let mut is_float = false;
        if self.peek(0) == Some('.') && self.peek(1).is_some_and(|c| c.is_ascii_digit()) {
            is_float = true;
            self.bump();
            self.digits();
        }
        if matches!(self.peek(0), Some('e' | 'E')) {
            let sign = usize::from(matches!(self.peek(1), Some('+' | '-')));
            if self.peek(1 + sign).is_some_and(|c| c.is_ascii_digit()) {
                is_float = true;
                for _ in 0..=sign {
                    self.bump();
                }
                self.digits();
            }
        }
        let text = &self.source[mark.byte..self.byte_offset()];
        let value = if is_float {
            text.parse::<f64>().ok().filter(|v| v.is_finite())
        } else {
            // Integers above 2^53 cannot be represented exactly; reject
            // them rather than silently rounding.
            text.parse::<i64>()
                .ok()
                .filter(|v| v.unsigned_abs() <= 1 << 53)
                .map(|v| {
                    #[allow(clippy::cast_precision_loss)]
                    let exact = v as f64;
                    exact
                })
        };
        let value = value
            .ok_or_else(|| self.error(mark, format!("number literal {text} is out of range")))?;
        self.push(TokenKind::Number(value), mark);
        Ok(())
    }

    fn digits(&mut self) {
        while self.peek(0).is_some_and(|c| c.is_ascii_digit()) {
            self.bump();
        }
    }

    fn word(&mut self, mark: Mark) -> Result<(), Diagnostic> {
        if self.peek(0) == Some('r') && matches!(self.peek(1), Some('"' | '\'')) {
            self.bump();
            return self.string(mark, true);
        }
        while self
            .peek(0)
            .is_some_and(|c| c == '_' || c.is_ascii_alphanumeric())
        {
            self.bump();
        }
        let text = &self.source[mark.byte..self.byte_offset()];
        let kind = match Keyword::from_ident(text) {
            Some(keyword) => TokenKind::Keyword(keyword),
            None => TokenKind::Name(Rc::from(text)),
        };
        self.push(kind, mark);
        Ok(())
    }

    fn string(&mut self, mark: Mark, raw: bool) -> Result<(), Diagnostic> {
        let Some(quote) = self.bump() else {
            return Ok(());
        };
        let mut value = String::new();
        loop {
            let char_mark = self.mark();
            match self.bump() {
                None | Some('\n') => return Err(self.error(mark, "unterminated string literal")),
                Some(c) if c == quote => break,
                Some('\\') if raw => {
                    value.push('\\');
                    if self.peek(0) == Some(quote) {
                        value.push(quote);
                        self.bump();
                    }
                }
                Some('\\') => value.push(self.escape(char_mark)?),
                Some(c) => value.push(c),
            }
        }
        self.push(TokenKind::Str(Rc::from(value)), mark);
        Ok(())
    }

    /// Resolves the escape after a backslash; `mark` is the backslash.
    fn escape(&mut self, mark: Mark) -> Result<char, Diagnostic> {
        let escaped = match self.bump() {
            Some('n') => '\n',
            Some('t') => '\t',
            Some('\\') => '\\',
            Some('"') => '"',
            Some('\'') => '\'',
            Some('u') => {
                let mut code = 0u32;
                for _ in 0..4 {
                    let digit = self.peek(0).and_then(|c| c.to_digit(16)).ok_or_else(|| {
                        self.error(mark, "\\u escape needs exactly four hex digits")
                    })?;
                    self.bump();
                    code = code * 16 + digit;
                }
                char::from_u32(code).ok_or_else(|| {
                    self.error(mark, format!("\\u{code:04x} is not a valid character"))
                })?
            }
            _ => return Err(self.error(mark, "unknown escape sequence")),
        };
        Ok(escaped)
    }

    fn operator(&mut self, mark: Mark) -> Result<(), Diagnostic> {
        let first = self.peek(0).unwrap_or('\0');
        let second = self.peek(1);
        let (op, len) = match (first, second) {
            ('/', Some('/')) => (Op::SlashSlash, 2),
            ('=', Some('=')) => (Op::EqEq, 2),
            ('!', Some('=')) => (Op::NotEq, 2),
            ('<', Some('=')) => (Op::Le, 2),
            ('>', Some('=')) => (Op::Ge, 2),
            ('+', Some('=')) => (Op::PlusAssign, 2),
            ('-', Some('=')) => (Op::MinusAssign, 2),
            ('*', Some('=')) => (Op::StarAssign, 2),
            ('/', Some('=')) => (Op::SlashAssign, 2),
            ('+', _) => (Op::Plus, 1),
            ('-', _) => (Op::Minus, 1),
            ('*', _) => (Op::Star, 1),
            ('/', _) => (Op::Slash, 1),
            ('%', _) => (Op::Percent, 1),
            ('=', _) => (Op::Assign, 1),
            ('<', _) => (Op::Lt, 1),
            ('>', _) => (Op::Gt, 1),
            ('(', _) => (Op::LParen, 1),
            (')', _) => (Op::RParen, 1),
            ('[', _) => (Op::LBracket, 1),
            (']', _) => (Op::RBracket, 1),
            ('{', _) => (Op::LBrace, 1),
            ('}', _) => (Op::RBrace, 1),
            (',', _) => (Op::Comma, 1),
            (':', _) => (Op::Colon, 1),
            ('.', _) => (Op::Dot, 1),
            (c, _) => {
                self.bump();
                return Err(self.error(mark, format!("unexpected character {c:?}")));
            }
        };
        for _ in 0..len {
            self.bump();
        }
        match op {
            Op::LParen | Op::LBracket | Op::LBrace => self.open_brackets.push(self.span_from(mark)),
            Op::RParen | Op::RBracket | Op::RBrace => {
                self.open_brackets.pop();
            }
            _ => {}
        }
        self.push(TokenKind::Op(op), mark);
        Ok(())
    }

    fn finish(mut self) -> Result<Vec<Token>, Diagnostic> {
        if let Some(&span) = self.open_brackets.last() {
            return Err(Diagnostic::new(
                ErrorCategory::Lexical,
                span,
                "this bracket is never closed",
            ));
        }
        let mark = self.mark();
        if !self.last_is_newline() {
            self.push(TokenKind::Newline, mark);
        }
        while self.indents.len() > 1 {
            self.indents.pop();
            self.push(TokenKind::Dedent, mark);
        }
        self.push(TokenKind::Eof, mark);
        Ok(self.tokens)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(source: &str) -> Vec<TokenKind> {
        tokenize(source)
            .unwrap()
            .into_iter()
            .map(|t| t.kind)
            .collect()
    }

    fn name(text: &str) -> TokenKind {
        TokenKind::Name(Rc::from(text))
    }

    #[test]
    fn emits_indent_and_dedent() {
        let source = "if x:\n    y = 1\n\n    # note\nz\n";
        assert_eq!(
            kinds(source),
            vec![
                TokenKind::Keyword(Keyword::If),
                name("x"),
                TokenKind::Op(Op::Colon),
                TokenKind::Newline,
                TokenKind::Indent,
                name("y"),
                TokenKind::Op(Op::Assign),
                TokenKind::Number(1.0),
                TokenKind::Newline,
                TokenKind::Dedent,
                name("z"),
                TokenKind::Newline,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn joins_lines_inside_brackets() {
        let source = "f(1,\n      2)\n";
        assert!(!kinds(source)[..6].contains(&TokenKind::Newline));
    }

    #[test]
    fn closes_open_blocks_at_eof() {
        let k = kinds("for i in x:\n    pass");
        assert_eq!(
            &k[k.len() - 3..],
            &[TokenKind::Newline, TokenKind::Dedent, TokenKind::Eof]
        );
    }

    #[test]
    fn lexes_numbers_strings_and_operators() {
        assert_eq!(
            kinds("1 2.5 3e2 'a\\n' r\"\\d\" // != <= +="),
            vec![
                TokenKind::Number(1.0),
                TokenKind::Number(2.5),
                TokenKind::Number(300.0),
                TokenKind::Str(Rc::from("a\n")),
                TokenKind::Str(Rc::from("\\d")),
                TokenKind::Op(Op::SlashSlash),
                TokenKind::Op(Op::NotEq),
                TokenKind::Op(Op::Le),
                TokenKind::Op(Op::PlusAssign),
                TokenKind::Newline,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn spans_point_at_tokens() {
        let tokens = tokenize("a = 1\n  \nbb").unwrap();
        let bb = tokens.iter().find(|t| t.kind == name("bb")).unwrap();
        assert_eq!((bb.span.line, bb.span.column), (3, 1));
        assert_eq!((bb.span.start, bb.span.end), (9, 11));
    }

    #[test]
    fn reports_lexical_errors_with_positions() {
        for (source, line, column) in [
            ("x = 1\n\ty = 2", 2, 1),
            ("if x:\n   y", 2, 1),
            ("x = 'abc", 1, 5),
            ("x = (1,\n2", 1, 5),
            ("x = 1 $ 2", 1, 7),
            ("x = '\\q'", 1, 6),
            ("x = 99999999999999999999", 1, 5),
        ] {
            let error = tokenize(source).unwrap_err();
            assert_eq!(error.category, ErrorCategory::Lexical, "{source}");
            assert_eq!(
                (error.span.line, error.span.column),
                (line, column),
                "{source}"
            );
        }
    }
}

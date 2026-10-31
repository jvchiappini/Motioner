//! Source spans and diagnostics shared by the lexer, parser and runtime.

use std::fmt;
use std::fmt::Write as _;

/// A region of source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Span {
    /// Byte offset of the first character.
    pub start: usize,
    /// Byte offset one past the last character.
    pub end: usize,
    /// 1-based line of `start`.
    pub line: u32,
    /// 1-based column of `start`, counted in characters.
    pub column: u32,
}

impl Span {
    /// Returns a span covering `self` through `other`, keeping `self`'s
    /// line and column.
    #[must_use]
    pub fn to(self, other: Self) -> Self {
        Self {
            end: other.end.max(self.end),
            ..self
        }
    }
}

/// The kind of problem a diagnostic reports (spec section 13).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorCategory {
    /// Invalid characters, literals or indentation.
    Lexical,
    /// Invalid syntax.
    Parse,
    /// An undefined name or attribute.
    Name,
    /// An operation on values of the wrong type.
    Type,
    /// A value of the right type but invalid content.
    Value,
    /// A resource limit was exceeded.
    Limit,
    /// A valid MDF construct that this version does not implement.
    Unsupported,
}

impl ErrorCategory {
    /// Lower-case name used in messages.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Lexical => "lexical",
            Self::Parse => "parse",
            Self::Name => "name",
            Self::Type => "type",
            Self::Value => "value",
            Self::Limit => "limit",
            Self::Unsupported => "unsupported",
        }
    }
}

/// One active function call when a runtime error happened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StackFrame {
    /// Name of the called function.
    pub function: String,
    /// Where the call happened.
    pub call_span: Span,
}

/// An error with category, message, source span and call stack.
///
/// The same diagnostic is shown to users and sent to the AI so either can
/// repair the program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// What kind of problem this is.
    pub category: ErrorCategory,
    /// Human-readable description.
    pub message: String,
    /// Where the problem is.
    pub span: Span,
    /// Active calls, outermost first. Empty for lexical and parse errors.
    pub stack: Vec<StackFrame>,
}

impl Diagnostic {
    /// Creates a diagnostic without a call stack.
    #[must_use]
    pub fn new(category: ErrorCategory, span: Span, message: impl Into<String>) -> Self {
        Self {
            category,
            message: message.into(),
            span,
            stack: Vec::new(),
        }
    }

    /// Formats the diagnostic with the offending source line and a caret,
    /// for terminal output.
    #[must_use]
    pub fn render(&self, source: &str, path: &str) -> String {
        let mut out = format!("{path}:{self}");
        let line_index = self.span.line.saturating_sub(1) as usize;
        if let Some(line) = source.lines().nth(line_index) {
            let caret_pad = " ".repeat(self.span.column.saturating_sub(1) as usize);
            write!(out, "\n    {line}\n    {caret_pad}^").expect("writing to a String cannot fail");
        }
        out
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}: {} error: {}",
            self.span.line,
            self.span.column,
            self.category.name(),
            self.message
        )?;
        for frame in self.stack.iter().rev() {
            write!(
                f,
                "\n  in {}() called at {}:{}",
                frame.function, frame.call_span.line, frame.call_span.column
            )?;
        }
        Ok(())
    }
}

impl std::error::Error for Diagnostic {}

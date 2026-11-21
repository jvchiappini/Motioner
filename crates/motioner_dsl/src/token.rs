//! Tokens produced by the lexer.

use std::sync::Arc;

use crate::diagnostic::Span;

/// A lexical token kind.
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    /// An identifier.
    Name(Arc<str>),
    /// An integer or float literal (MDF has one number type).
    Number(f64),
    /// A string literal with escapes resolved.
    Str(Arc<str>),
    /// A reserved word.
    Keyword(Keyword),
    /// An operator or delimiter.
    Op(Op),
    /// End of a logical line.
    Newline,
    /// Indentation increased by one level.
    Indent,
    /// Indentation decreased by one level.
    Dedent,
    /// End of input.
    Eof,
}

/// Reserved words (spec section 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub enum Keyword {
    Def,
    Return,
    If,
    Elif,
    Else,
    For,
    In,
    While,
    Break,
    Continue,
    Pass,
    True,
    False,
    None,
    And,
    Or,
    Not,
    Use,
}

impl Keyword {
    /// Maps an identifier to a keyword.
    #[must_use]
    pub fn from_ident(ident: &str) -> Option<Self> {
        Some(match ident {
            "def" => Self::Def,
            "return" => Self::Return,
            "if" => Self::If,
            "elif" => Self::Elif,
            "else" => Self::Else,
            "for" => Self::For,
            "in" => Self::In,
            "while" => Self::While,
            "break" => Self::Break,
            "continue" => Self::Continue,
            "pass" => Self::Pass,
            "True" => Self::True,
            "False" => Self::False,
            "None" => Self::None,
            "and" => Self::And,
            "or" => Self::Or,
            "not" => Self::Not,
            "use" => Self::Use,
            _ => return None,
        })
    }
}

/// Operators and delimiters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub enum Op {
    Plus,
    Minus,
    Star,
    Slash,
    SlashSlash,
    Percent,
    Assign,
    PlusAssign,
    MinusAssign,
    StarAssign,
    SlashAssign,
    EqEq,
    NotEq,
    Lt,
    Le,
    Gt,
    Ge,
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Comma,
    Colon,
    Dot,
}

/// A token with its source span.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    /// What the token is.
    pub kind: TokenKind,
    /// Where it is.
    pub span: Span,
}

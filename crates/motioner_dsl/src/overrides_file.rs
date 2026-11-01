//! Parser for `<name>.overrides.motion` files (D-0022).
//!
//! The file is a restricted MDF subset: a sequence of `override`,
//! `override_key`, `override_unkey`, `override_visible` and `override_locked`
//! statements, one per line. It reuses the MDF lexer, so comments, string
//! escapes and number literals behave exactly like the program. Everything
//! else is a diagnostic at the offending statement.

use motioner_core::{
    Ease, FrameRate, OverrideAction, OverrideEntry, OverrideSet, OverrideValue, Vec2,
};

use crate::diagnostic::{Diagnostic, ErrorCategory, Span};
use crate::lexer::tokenize;
use crate::token::{Keyword, Op, Token, TokenKind};

/// The result of parsing an overrides file.
#[derive(Debug)]
pub(crate) struct ParsedOverrides {
    /// The overrides in file order (duplicates are kept; last write wins on
    /// apply).
    pub set: OverrideSet,
    /// Span of each entry's statement, aligned with `set.entries()`.
    pub spans: Vec<Span>,
}

/// A diagnostic in the overrides file, prefixed so the user can tell it apart
/// from a program diagnostic.
fn parse_error(span: Span, message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(
        ErrorCategory::Value,
        span,
        format!("overrides: {}", message.into()),
    )
}

/// Every statement name the overrides file accepts.
pub(crate) const STATEMENTS: [&str; 5] = [
    "override",
    "override_key",
    "override_unkey",
    "override_visible",
    "override_locked",
];

/// Parses an overrides file.
///
/// # Errors
///
/// Returns a value diagnostic prefixed `overrides:` with the span of the first
/// malformed statement, unknown call or invalid time.
pub(crate) fn parse_overrides(
    source: &str,
    rate: FrameRate,
) -> Result<ParsedOverrides, Diagnostic> {
    let tokens = tokenize(source)?;
    Parser {
        tokens,
        pos: 0,
        rate,
        set: OverrideSet::new(),
        spans: Vec::new(),
    }
    .program()
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    rate: FrameRate,
    set: OverrideSet,
    spans: Vec<Span>,
}

impl Parser {
    fn peek(&self) -> &Token {
        &self.tokens[self.pos.min(self.tokens.len() - 1)]
    }

    fn advance(&mut self) -> Token {
        let token = self.peek().clone();
        if self.pos < self.tokens.len() - 1 {
            self.pos += 1;
        }
        token
    }

    fn program(mut self) -> Result<ParsedOverrides, Diagnostic> {
        loop {
            while matches!(self.peek().kind, TokenKind::Newline) {
                self.advance();
            }
            if matches!(self.peek().kind, TokenKind::Eof) {
                break;
            }
            if matches!(self.peek().kind, TokenKind::Indent | TokenKind::Dedent) {
                return Err(parse_error(
                    self.peek().span,
                    "statements must start at the beginning of a line",
                ));
            }
            let (name, start_span) = match self.peek().kind.clone() {
                TokenKind::Name(name) => {
                    let span = self.peek().span;
                    self.advance();
                    (name.to_string(), span)
                }
                other => {
                    return Err(parse_error(
                        self.peek().span,
                        format!("expected an override statement, found {other:?}"),
                    ))
                }
            };
            self.expect_op(Op::LParen)?;
            match name.as_str() {
                "override" => self.set_statement(start_span, false)?,
                "override_key" => self.set_statement(start_span, true)?,
                "override_unkey" => self.unkey_statement(start_span)?,
                "override_visible" => self.bool_statement(start_span, true)?,
                "override_locked" => self.bool_statement(start_span, false)?,
                other => {
                    return Err(parse_error(
                        start_span,
                        format!(
                            "unknown statement '{other}'; the file accepts {}",
                            STATEMENTS.join(", ")
                        ),
                    ))
                }
            }
            if !matches!(self.peek().kind, TokenKind::Newline | TokenKind::Eof) {
                return Err(parse_error(
                    self.peek().span,
                    "expected the end of the line",
                ));
            }
        }
        Ok(ParsedOverrides {
            set: self.set,
            spans: self.spans,
        })
    }

    fn expect_op(&mut self, expected: Op) -> Result<Span, Diagnostic> {
        let token = self.advance();
        if token.kind == TokenKind::Op(expected) {
            Ok(token.span)
        } else {
            Err(parse_error(
                token.span,
                format!("expected {expected:?}, found {:?}", token.kind),
            ))
        }
    }

    fn name(&mut self, what: &str) -> Result<String, Diagnostic> {
        let token = self.advance();
        match token.kind {
            TokenKind::Name(name) => Ok(name.to_string()),
            other => Err(parse_error(
                token.span,
                format!("{what} must be a name, found {other:?}"),
            )),
        }
    }

    fn string(&mut self, what: &str) -> Result<String, Diagnostic> {
        let token = self.advance();
        match token.kind {
            TokenKind::Str(text) => Ok(text.to_string()),
            other => Err(parse_error(
                token.span,
                format!("{what} must be a string, found {other:?}"),
            )),
        }
    }

    /// A number, with an optional leading minus.
    fn number(&mut self, what: &str) -> Result<f64, Diagnostic> {
        let mut negative = false;
        if matches!(self.peek().kind, TokenKind::Op(Op::Minus)) {
            negative = true;
            self.advance();
        }
        let token = self.advance();
        match token.kind {
            TokenKind::Number(value) => Ok(if negative { -value } else { value }),
            other => Err(parse_error(
                token.span,
                format!("{what} must be a number, found {other:?}"),
            )),
        }
    }

    /// Accepts the lower-case true/false of D-0022 and the capitalised
    /// True/False of MDF; both mean the same.
    fn boolean(&mut self) -> Result<bool, Diagnostic> {
        let token = self.advance();
        match token.kind {
            TokenKind::Keyword(Keyword::True) => Ok(true),
            TokenKind::Keyword(Keyword::False) => Ok(false),
            TokenKind::Name(name) if name.as_ref() == "true" => Ok(true),
            TokenKind::Name(name) if name.as_ref() == "false" => Ok(false),
            other => Err(parse_error(
                token.span,
                format!("expected true or false, found {other:?}"),
            )),
        }
    }

    /// `(x, y)`, a number or a string.
    fn value(&mut self) -> Result<OverrideValue, Diagnostic> {
        if matches!(self.peek().kind, TokenKind::Op(Op::LParen)) {
            self.advance();
            let x = self.number("the first component")?;
            self.expect_op(Op::Comma)?;
            let y = self.number("the second component")?;
            self.expect_op(Op::RParen)?;
            return Ok(OverrideValue::Vec2(Vec2::new(x, y)));
        }
        if matches!(self.peek().kind, TokenKind::Str(_)) {
            return Ok(OverrideValue::Text(self.string("a value")?));
        }
        Ok(OverrideValue::Number(self.number("a value")?))
    }

    fn time_frame(&self, seconds: f64, span: Span) -> Result<i64, Diagnostic> {
        self.rate
            .seconds_to_frame(seconds)
            .map_err(|error| parse_error(span, error.to_string()))
    }

    fn id_and_property(&mut self) -> Result<(String, String), Diagnostic> {
        let id = self.string("the id")?;
        self.expect_op(Op::Comma)?;
        let property = self.string("the property")?;
        self.expect_op(Op::Comma)?;
        Ok((id, property))
    }

    fn push(&mut self, entry: OverrideEntry, span: Span) {
        self.set.push(entry);
        self.spans.push(span);
    }

    fn set_statement(&mut self, start: Span, keyed: bool) -> Result<(), Diagnostic> {
        let (id, property) = self.id_and_property()?;
        if keyed {
            let time_span = self.peek().span;
            let seconds = self.number("the time")?;
            self.expect_op(Op::Comma)?;
            let value = self.value()?;
            let mut ease = Ease::Linear;
            if matches!(self.peek().kind, TokenKind::Op(Op::Comma)) {
                self.advance();
                let name = self.name("'ease'")?;
                if name != "ease" {
                    return Err(parse_error(
                        start,
                        "the only keyword after the value is ease=\"...\"",
                    ));
                }
                self.expect_op(Op::Assign)?;
                let ease_name = self.string("the ease name")?;
                ease = Ease::from_name(&ease_name).ok_or_else(|| {
                    parse_error(
                        start,
                        format!("unknown ease {ease_name:?}; use one of the MDF ease names"),
                    )
                })?;
            }
            let end = self.expect_op(Op::RParen)?;
            let frame = self.time_frame(seconds, time_span)?;
            self.push(
                OverrideEntry::new(id, property, OverrideAction::Key { frame, value, ease }),
                start.to(end),
            );
        } else {
            let value = self.value()?;
            let end = self.expect_op(Op::RParen)?;
            self.push(
                OverrideEntry::new(id, property, OverrideAction::Set(value)),
                start.to(end),
            );
        }
        Ok(())
    }

    fn unkey_statement(&mut self, start: Span) -> Result<(), Diagnostic> {
        let (id, property) = self.id_and_property()?;
        let time_span = self.peek().span;
        let seconds = self.number("the time")?;
        let end = self.expect_op(Op::RParen)?;
        let frame = self.time_frame(seconds, time_span)?;
        self.push(
            OverrideEntry::new(id, property, OverrideAction::Unkey { frame }),
            start.to(end),
        );
        Ok(())
    }

    fn bool_statement(&mut self, start: Span, visible: bool) -> Result<(), Diagnostic> {
        let id = self.string("the id")?;
        self.expect_op(Op::Comma)?;
        let flag = self.boolean()?;
        let end = self.expect_op(Op::RParen)?;
        let action = if visible {
            OverrideAction::Visible(flag)
        } else {
            OverrideAction::Locked(flag)
        };
        let property = if visible {
            motioner_core::VISIBLE
        } else {
            motioner_core::LOCKED
        };
        self.push(OverrideEntry::new(id, property, action), start.to(end));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rate() -> FrameRate {
        FrameRate::new(60, 1).unwrap()
    }

    fn parse(text: &str) -> ParsedOverrides {
        parse_overrides(text, rate()).expect("valid overrides")
    }

    #[test]
    fn parses_every_statement() {
        let parsed = parse(
            "# comment\n\
             override(\"title\", \"pos\", (960, 480))\n\
             override(\"title\", \"fill\", \"#ff3366\")\n\
             override_key(\"title\", \"opacity\", 0.8, 1, ease=\"quad_out\")\n\
             override_unkey(\"title\", \"rotation\", 1.5)\n\
             override_visible(\"dot\", false)\n\
             override_locked(\"spinner\", true)\n",
        );
        assert_eq!(parsed.set.len(), 6);
        assert_eq!(parsed.spans.len(), 6);
        let entries = parsed.set.entries();
        assert_eq!(
            entries[0].action,
            OverrideAction::Set(OverrideValue::Vec2(Vec2::new(960.0, 480.0)))
        );
        assert_eq!(
            entries[1].action,
            OverrideAction::Set(OverrideValue::Text("#ff3366".to_owned()))
        );
        assert_eq!(
            entries[2].action,
            OverrideAction::Key {
                frame: 48,
                value: OverrideValue::Number(1.0),
                ease: Ease::QuadOut,
            }
        );
        assert_eq!(entries[3].action, OverrideAction::Unkey { frame: 90 });
        assert_eq!(entries[4].action, OverrideAction::Visible(false));
        assert_eq!(entries[4].property, "visible");
        assert_eq!(entries[5].action, OverrideAction::Locked(true));
        assert_eq!(entries[5].property, "locked");
    }

    #[test]
    fn negative_numbers_and_escaped_strings() {
        let parsed =
            parse("override(\"a\", \"pos\", (-1.5, -20))\noverride(\"a\", \"text\", \"x\\\"y\")\n");
        assert_eq!(
            parsed.set.entries()[0].action,
            OverrideAction::Set(OverrideValue::Vec2(Vec2::new(-1.5, -20.0)))
        );
        assert_eq!(
            parsed.set.entries()[1].action,
            OverrideAction::Set(OverrideValue::Text("x\"y".to_owned()))
        );
    }

    #[test]
    fn a_program_statement_is_rejected_with_its_line() {
        let error = parse_overrides("comp(name=\"x\")\n", rate()).unwrap_err();
        assert_eq!(error.span.line, 1);
        assert!(
            error.message.starts_with("overrides: "),
            "{}",
            error.message
        );
        assert!(error.is_error());
    }

    #[test]
    fn malformed_statements_point_at_the_problem() {
        for text in [
            "override(\"a\", \"pos\")\n",
            "override(\"a\", \"pos\", (1))\n",
            "override_visible(\"a\", 1)\n",
            "override_key(\"a\", \"pos\", 0, (1, 2), ease=\"bounce\")\n",
            "override(\"a\", \"pos\", (1, 2)) extra\n",
            "  override(\"a\", \"pos\", (1, 2))\n",
        ] {
            let error = parse_overrides(text, rate()).unwrap_err();
            assert_eq!(error.severity, crate::diagnostic::Severity::Error);
            // Indentation is rejected by the shared lexer, which does not
            // know about overrides; everything else is an overrides message.
            assert!(
                error.message.starts_with("overrides:") || error.message.contains("indentation"),
                "{text:?} -> {}",
                error.message
            );
        }
    }

    #[test]
    fn times_convert_with_the_composition_rate() {
        let parsed = parse("override_key(\"a\", \"opacity\", 0.5, 1)\n");
        assert_eq!(
            parsed.set.entries()[0].action,
            OverrideAction::Key {
                frame: 30,
                value: OverrideValue::Number(1.0),
                ease: Ease::Linear,
            }
        );
    }
}

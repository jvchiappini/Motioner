//! Value origins: where each node property value came from in the program.
//!
//! The hybrid bake (T-DSL-0006) edits a literal argument in place when that is
//! exact. To decide, the interpreter records, for every property write it
//! performs, the source span of the expression that produced the value and
//! whether that expression is a plain literal. A literal is **private** to a
//! node when it is evaluated exactly once during the run: inside a loop or a
//! function called twice the same span appears several times, so editing it
//! would change every node it fed.
//!
//! Recording is deterministic (the interpreter has no clock or randomness)
//! and never changes the document.

use motioner_core::{Frame, NodeId};

use crate::ast::{Expr, ExprKind};
use crate::diagnostic::Span;

/// The MDF shape of a plain literal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Literal {
    /// A number, possibly signed: `160`, `-24`.
    Number,
    /// A string literal.
    Text,
    /// A two-number pair: `(960, 480)`.
    Pair,
    /// `True` or `False`.
    Bool,
    /// `None`.
    None,
}

/// What produced a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OriginKind {
    /// A static write: a constructor argument or `node.property = value`.
    Set,
    /// A `node.key(...)` call at a frame.
    Key(Frame),
}

/// An explicit `ease="..."` argument of a `.key()` call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EaseOrigin {
    /// Span of the ease expression (the string literal).
    pub span: Span,
    /// The ease name it names.
    pub name: String,
}

/// One recorded property value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueOrigin {
    /// The node the value belongs to.
    pub node: NodeId,
    /// The property as written (`pos`, `pos.x`, `radius`, `visible`...).
    pub property: String,
    /// Span of the expression that produced the value.
    pub span: Span,
    /// The literal shape, when the expression is a plain literal.
    pub literal: Option<Literal>,
    /// Whether the value is static or a keyframe.
    pub kind: OriginKind,
    /// For a key, its explicit ease argument.
    pub ease: Option<EaseOrigin>,
}

/// Every value origin of one run.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ValueOrigins {
    entries: Vec<ValueOrigin>,
}

impl ValueOrigins {
    /// An empty set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The recorded origins, in write order.
    #[must_use]
    pub fn entries(&self) -> &[ValueOrigin] {
        &self.entries
    }

    /// Records `origin`, replacing an earlier static value or key of the same
    /// node and property (the later write wins, as in the document).
    pub fn record(&mut self, origin: ValueOrigin) {
        self.entries.retain(|existing| {
            let same_write = match (existing.kind, origin.kind) {
                (OriginKind::Set, OriginKind::Set) => true,
                (OriginKind::Key(a), OriginKind::Key(b)) => a == b,
                _ => false,
            };
            !(same_write && existing.node == origin.node && existing.property == origin.property)
        });
        self.entries.push(origin);
    }

    /// How many times the value expression of `origin` was evaluated in this
    /// run. `1` means the literal is private to its node.
    #[must_use]
    pub fn count(&self, origin: &ValueOrigin) -> usize {
        self.entries
            .iter()
            .filter(|entry| entry.span.start == origin.span.start)
            .count()
    }

    /// The static origin of `node`'s `property`, if recorded.
    #[must_use]
    pub fn set_origin(&self, node: NodeId, property: &str) -> Option<&ValueOrigin> {
        self.entries.iter().rev().find(|entry| {
            entry.node == node && entry.property == property && entry.kind == OriginKind::Set
        })
    }

    /// The key origin of `node`'s `property` at `frame`, if recorded.
    #[must_use]
    pub fn key_origin(&self, node: NodeId, property: &str, frame: Frame) -> Option<&ValueOrigin> {
        self.entries.iter().rev().find(|entry| {
            entry.node == node && entry.property == property && entry.kind == OriginKind::Key(frame)
        })
    }

    /// Whether every origin with this span start is private to one node.
    #[must_use]
    pub fn is_private(&self, origin: &ValueOrigin) -> bool {
        self.count(origin) == 1
    }
}

/// The literal shape of `expr`, if it is a plain literal.
///
/// Strings must not be interpolated (MDF has no f-strings), numbers may be
/// signed, and a pair is a tuple or list of exactly two plain numbers.
#[must_use]
pub fn literal_of(expr: &Expr) -> Option<Literal> {
    match &expr.kind {
        ExprKind::Number(_) => Some(Literal::Number),
        ExprKind::Str(_) => Some(Literal::Text),
        ExprKind::Bool(_) => Some(Literal::Bool),
        ExprKind::None => Some(Literal::None),
        ExprKind::Unary { operand, .. } => {
            matches!(operand.kind, ExprKind::Number(_)).then_some(Literal::Number)
        }
        ExprKind::Tuple(items) | ExprKind::List(items) => {
            let plain = |item: &Expr| match &item.kind {
                ExprKind::Number(_) => true,
                ExprKind::Unary { operand, .. } => matches!(operand.kind, ExprKind::Number(_)),
                _ => false,
            };
            (items.len() == 2 && items.iter().all(plain)).then_some(Literal::Pair)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    fn first_value(source: &str) -> Option<Literal> {
        let program = parse(source).unwrap();
        let statement = program.body.first()?;
        let crate::ast::StmtKind::Expr(expr) = &statement.kind else {
            return None;
        };
        literal_of(expr)
    }

    #[test]
    fn literals_are_recognised() {
        assert_eq!(first_value("160\n"), Some(Literal::Number));
        assert_eq!(first_value("-24\n"), Some(Literal::Number));
        assert_eq!(first_value("\"hello\"\n"), Some(Literal::Text));
        assert_eq!(first_value("True\n"), Some(Literal::Bool));
        assert_eq!(first_value("None\n"), Some(Literal::None));
        assert_eq!(first_value("(960, -480)\n"), Some(Literal::Pair));
        assert_eq!(first_value("[1, 2]\n"), Some(Literal::Pair));
    }

    #[test]
    fn expressions_and_shared_values_are_not_literals() {
        assert_eq!(first_value("480 + i * 240\n"), None);
        assert_eq!(first_value("size\n"), None);
        assert_eq!(first_value("(960, y)\n"), None);
        assert_eq!(first_value("f()\n"), None);
    }
}

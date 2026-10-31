//! Runtime values of the MDF interpreter.

use std::cell::RefCell;
use std::rc::Rc;

use motioner_core::NodeId;

use crate::ast::FunctionDef;

/// A lazily iterated integer range produced by `range()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RangeValue {
    pub start: i64,
    pub stop: i64,
    pub step: i64,
}

impl RangeValue {
    /// Number of elements, saturating at zero.
    pub fn len(self) -> i64 {
        let span = if self.step > 0 {
            self.stop - self.start
        } else {
            self.start - self.stop
        };
        if span <= 0 {
            0
        } else {
            (span + self.step.abs() - 1) / self.step.abs()
        }
    }

    /// The element at `index`, which must be in `0..len`.
    pub fn at(self, index: i64) -> i64 {
        self.start + index * self.step
    }
}

/// Built-in functions and scene constructors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Builtin {
    Range,
    Len,
    Min,
    Max,
    Abs,
    Round,
    Str,
    Int,
    Float,
    Sin,
    Cos,
    Lerp,
    Clamp,
    Comp,
    Group,
    Rect,
    Circle,
    Text,
}

impl Builtin {
    /// Every builtin with its MDF name.
    pub const ALL: [(Self, &'static str); 18] = [
        (Self::Range, "range"),
        (Self::Len, "len"),
        (Self::Min, "min"),
        (Self::Max, "max"),
        (Self::Abs, "abs"),
        (Self::Round, "round"),
        (Self::Str, "str"),
        (Self::Int, "int"),
        (Self::Float, "float"),
        (Self::Sin, "sin"),
        (Self::Cos, "cos"),
        (Self::Lerp, "lerp"),
        (Self::Clamp, "clamp"),
        (Self::Comp, "comp"),
        (Self::Group, "group"),
        (Self::Rect, "rect"),
        (Self::Circle, "circle"),
        (Self::Text, "text"),
    ];

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .find(|(_, candidate)| *candidate == name)
            .map(|(builtin, _)| *builtin)
    }

    pub fn name(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(builtin, _)| *builtin == self)
            .map_or("?", |(_, name)| name)
    }
}

/// Methods callable on scene nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NodeMethod {
    Key,
    Add,
}

impl NodeMethod {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "key" => Some(Self::Key),
            "add" => Some(Self::Add),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Key => "key",
            Self::Add => "add",
        }
    }
}

/// A dynamically typed MDF value. Cloning is cheap: aggregates share
/// their storage.
#[derive(Debug, Clone)]
pub(crate) enum Value {
    None,
    Bool(bool),
    Number(f64),
    Str(Rc<str>),
    List(Rc<RefCell<Vec<Value>>>),
    Tuple(Rc<[Value]>),
    Range(RangeValue),
    Function(Rc<FunctionDef>),
    Builtin(Builtin),
    Node(NodeId),
    Method(NodeId, NodeMethod),
}

impl Value {
    pub fn list(items: Vec<Value>) -> Self {
        Self::List(Rc::new(RefCell::new(items)))
    }

    /// The MDF type name used in error messages.
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Bool(_) => "bool",
            Self::Number(_) => "number",
            Self::Str(_) => "string",
            Self::List(_) => "list",
            Self::Tuple(_) => "tuple",
            Self::Range(_) => "range",
            Self::Function(_) | Self::Builtin(_) | Self::Method(..) => "function",
            Self::Node(_) => "node",
        }
    }

    /// Python truthiness.
    pub fn is_truthy(&self) -> bool {
        match self {
            Self::None => false,
            Self::Bool(b) => *b,
            Self::Number(n) => *n != 0.0,
            Self::Str(s) => !s.is_empty(),
            Self::List(items) => !items.borrow().is_empty(),
            Self::Tuple(items) => !items.is_empty(),
            Self::Range(range) => range.len() > 0,
            Self::Function(_) | Self::Builtin(_) | Self::Node(_) | Self::Method(..) => true,
        }
    }

    /// Structural equality; values of different types are unequal.
    pub fn equals(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::None, Self::None) => true,
            (Self::Bool(a), Self::Bool(b)) => a == b,
            // MDF `==` is IEEE-754 equality by design (spec/mdf-v0.md section 4).
            #[allow(clippy::float_cmp)]
            (Self::Number(a), Self::Number(b)) => a == b,
            (Self::Str(a), Self::Str(b)) => a == b,
            (Self::List(a), Self::List(b)) => {
                Rc::ptr_eq(a, b) || items_equal(&a.borrow(), &b.borrow())
            }
            (Self::Tuple(a), Self::Tuple(b)) => items_equal(a, b),
            (Self::Range(a), Self::Range(b)) => a == b,
            (Self::Function(a), Self::Function(b)) => Rc::ptr_eq(a, b),
            (Self::Builtin(a), Self::Builtin(b)) => a == b,
            (Self::Node(a), Self::Node(b)) => a == b,
            (Self::Method(a, m), Self::Method(b, n)) => a == b && m == n,
            _ => false,
        }
    }

    /// The `str()` form of the value.
    pub fn display(&self) -> String {
        match self {
            Self::None => "None".to_owned(),
            Self::Bool(true) => "True".to_owned(),
            Self::Bool(false) => "False".to_owned(),
            Self::Number(n) => format_number(*n),
            Self::Str(s) => s.to_string(),
            Self::List(items) => format!("[{}]", join_repr(&items.borrow())),
            Self::Tuple(items) if items.len() == 1 => format!("({},)", items[0].repr()),
            Self::Tuple(items) => format!("({})", join_repr(items)),
            Self::Range(r) => format!("range({}, {}, {})", r.start, r.stop, r.step),
            Self::Function(f) => format!("<function {}>", f.name),
            Self::Builtin(b) => format!("<builtin {}>", b.name()),
            Self::Node(id) => format!("<node {id}>"),
            Self::Method(id, m) => format!("<method {} of node {id}>", m.name()),
        }
    }

    /// The form used inside containers: strings are quoted.
    fn repr(&self) -> String {
        match self {
            Self::Str(s) => format!("{s:?}"),
            other => other.display(),
        }
    }
}

fn items_equal(a: &[Value], b: &[Value]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.equals(y))
}

fn join_repr(items: &[Value]) -> String {
    items.iter().map(Value::repr).collect::<Vec<_>>().join(", ")
}

/// Formats a number like Python: integral values without a fraction.
pub(crate) fn format_number(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e16 {
        format!("{value:.0}")
    } else {
        format!("{value}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn range_lengths_match_python() {
        let len = |start, stop, step| RangeValue { start, stop, step }.len();
        assert_eq!(len(0, 5, 1), 5);
        assert_eq!(len(0, 5, 2), 3);
        assert_eq!(len(5, 0, -2), 3);
        assert_eq!(len(5, 0, 1), 0);
    }

    #[test]
    fn displays_like_python() {
        let tuple = Value::Tuple(Rc::from(vec![
            Value::Number(1.0),
            Value::Number(2.5),
            Value::Str(Rc::from("a")),
        ]));
        assert_eq!(tuple.display(), "(1, 2.5, \"a\")");
        assert_eq!(Value::Number(-0.0).display(), "-0");
        assert_eq!(Value::Bool(true).display(), "True");
    }
}

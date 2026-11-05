//! Abstract syntax tree for the MDF v0 subset.
//!
//! Every node carries the [`Span`] it was parsed from so runtime errors
//! and provenance can point back at the source.

use std::rc::Rc;

use crate::diagnostic::Span;

/// A parsed program.
#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    /// Top-level statements in source order.
    pub body: Vec<Stmt>,
}

/// A statement with its span.
#[derive(Debug, Clone, PartialEq)]
pub struct Stmt {
    /// What the statement does.
    pub kind: StmtKind,
    /// Where it is.
    pub span: Span,
}

/// Statement kinds.
#[derive(Debug, Clone, PartialEq)]
pub enum StmtKind {
    /// An expression evaluated for its effect (usually a call).
    Expr(Expr),
    /// `target op value`, for `=`, `+=`, `-=`, `*=` and `/=`.
    Assign {
        /// What is assigned.
        target: Target,
        /// Plain or augmented assignment.
        op: AssignOp,
        /// The assigned expression.
        value: Expr,
    },
    /// `def name(params):` and its body.
    Def(Rc<FunctionDef>),
    /// `return [value]`.
    Return(Option<Expr>),
    /// `if` / `elif` chain with an optional `else` body.
    If {
        /// Conditions and bodies, in order.
        branches: Vec<(Expr, Vec<Stmt>)>,
        /// The `else` body; empty when absent.
        otherwise: Vec<Stmt>,
    },
    /// `for variable(s) in iterable:` and its body.
    For {
        /// Loop variables, in order; more than one unpacks a tuple.
        variables: Vec<Rc<str>>,
        /// The iterated expression.
        iterable: Expr,
        /// Loop body.
        body: Vec<Stmt>,
    },
    /// `break`.
    Break,
    /// `continue`.
    Continue,
    /// `pass`.
    Pass,
}

/// Assignment operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOp {
    /// `=`
    Set,
    /// `+=`
    Add,
    /// `-=`
    Sub,
    /// `*=`
    Mul,
    /// `/=`
    Div,
}

/// The left-hand side of an assignment.
#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    /// A variable.
    Name(Rc<str>),
    /// `object.name`, for node properties.
    Attribute {
        /// The object expression.
        object: Expr,
        /// The attribute name.
        name: Rc<str>,
    },
    /// `object[index]`, for list items.
    Index {
        /// The container expression.
        object: Expr,
        /// The index expression.
        index: Expr,
    },
}

/// A parameter of a user-defined function.
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    /// Parameter name.
    pub name: Rc<str>,
    /// Default expression, when the parameter is optional.
    pub default: Option<Expr>,
}

/// A user-defined function.
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionDef {
    /// Function name.
    pub name: Rc<str>,
    /// Parameters in definition order; optional ones come last.
    pub params: Vec<Param>,
    /// Function body.
    pub body: Vec<Stmt>,
    /// Span of the `def` header.
    pub span: Span,
}

/// An expression with its span.
#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    /// What the expression computes.
    pub kind: ExprKind,
    /// Where it is.
    pub span: Span,
}

/// Expression kinds.
#[derive(Debug, Clone, PartialEq)]
pub enum ExprKind {
    /// A number literal.
    Number(f64),
    /// A string literal.
    Str(Rc<str>),
    /// `True` or `False`.
    Bool(bool),
    /// `None`.
    None,
    /// A variable reference.
    Name(Rc<str>),
    /// `[a, b, ...]`.
    List(Vec<Expr>),
    /// `(a, b, ...)` or `()`.
    Tuple(Vec<Expr>),
    /// `-x` or `+x`.
    Unary {
        /// The operator.
        op: UnaryOp,
        /// The operand.
        operand: Box<Expr>,
    },
    /// Arithmetic.
    Binary {
        /// The operator.
        op: BinaryOp,
        /// Left operand.
        left: Box<Expr>,
        /// Right operand.
        right: Box<Expr>,
    },
    /// A single (non-chained) comparison.
    Compare {
        /// The operator.
        op: CompareOp,
        /// Left operand.
        left: Box<Expr>,
        /// Right operand.
        right: Box<Expr>,
    },
    /// Short-circuit `and` / `or`, returning an operand like Python.
    Logical {
        /// The operator.
        op: LogicalOp,
        /// Left operand.
        left: Box<Expr>,
        /// Right operand.
        right: Box<Expr>,
    },
    /// `not x`.
    Not(Box<Expr>),
    /// `callee(args)`.
    Call {
        /// The called expression.
        callee: Box<Expr>,
        /// Positional arguments first, then keyword arguments.
        args: Vec<Argument>,
    },
    /// `object.name`.
    Attribute {
        /// The object expression.
        object: Box<Expr>,
        /// The attribute name.
        name: Rc<str>,
    },
    /// `object[index]`.
    Index {
        /// The container expression.
        object: Box<Expr>,
        /// The index expression.
        index: Box<Expr>,
    },
}

/// A call argument.
#[derive(Debug, Clone, PartialEq)]
pub struct Argument {
    /// Keyword name, or `None` for a positional argument.
    pub name: Option<Rc<str>>,
    /// The argument value.
    pub value: Expr,
}

/// Unary operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    /// `-`
    Neg,
    /// `+`
    Pos,
}

/// Arithmetic operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    /// `+`
    Add,
    /// `-`
    Sub,
    /// `*`
    Mul,
    /// `/`
    Div,
    /// `//`
    FloorDiv,
    /// `%`
    Mod,
}

impl BinaryOp {
    /// The operator symbol.
    #[must_use]
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Add => "+",
            Self::Sub => "-",
            Self::Mul => "*",
            Self::Div => "/",
            Self::FloorDiv => "//",
            Self::Mod => "%",
        }
    }
}

/// Comparison operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareOp {
    /// `==`
    Eq,
    /// `!=`
    NotEq,
    /// `<`
    Lt,
    /// `<=`
    Le,
    /// `>`
    Gt,
    /// `>=`
    Ge,
    /// `in`
    In,
    /// `not in`
    NotIn,
}

/// Boolean operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogicalOp {
    /// `and`
    And,
    /// `or`
    Or,
}

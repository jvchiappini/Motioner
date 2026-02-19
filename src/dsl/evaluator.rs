/// Expression evaluator for the Motioner DSL.
///
/// Evaluates simple mathematical expressions (e.g. `seconds * 0.1 + 0.5`)
/// against a variable context.  Used by the runtime to resolve dynamic
/// values inside event handler actions.
use std::collections::HashMap;

// ─── Context ─────────────────────────────────────────────────────────────────

/// Generic value used by the DSL runtime (numbers, strings, lists).
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Number(f32),
    Str(String),
    List(Vec<Value>),
}

/// Variables available during expression evaluation (e.g. `seconds`, `frame`).
pub struct EvalContext {
    pub variables: HashMap<String, Value>,
    /// Shapes requested by runtime handlers (e.g. full `circle {}` /
    /// `rect {}` blocks declared inside `on_time`), collected while a
    /// handler runs. Caller should append them to the real scene after
    /// the handler finishes.
    pub spawned_shapes: Vec<crate::scene::Shape>,
}

impl EvalContext {
    pub fn new() -> Self {
        Self {
            variables: HashMap::new(),
            spawned_shapes: Vec::new(),
        }
    }

    /// Builder-style helper: add a numeric variable and return `self`.
    pub fn with_var(mut self, name: &str, val: f32) -> Self {
        self.variables.insert(name.to_string(), Value::Number(val));
        self
    }

    /// Set a variable to a Value (overwrites any existing variable).
    pub fn set_var(&mut self, name: &str, val: Value) {
        self.variables.insert(name.to_string(), val);
    }

    /// Convenience getters for common types.
    pub fn get_number(&self, name: &str) -> Option<f32> {
        match self.variables.get(name) {
            Some(Value::Number(n)) => Some(*n),
            _ => None,
        }
    }

    pub fn get_str(&self, name: &str) -> Option<&str> {
        match self.variables.get(name) {
            Some(Value::Str(s)) => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn get_list(&self, name: &str) -> Option<&[Value]> {
        match self.variables.get(name) {
            Some(Value::List(v)) => Some(v.as_slice()),
            _ => None,
        }
    }

    /// Add a spawned shape to the evaluation context (collected by caller).
    pub fn push_spawned_shape(&mut self, s: crate::scene::Shape) {
        self.spawned_shapes.push(s);
    }

    /// Drain and return spawned shapes collected during evaluation.
    pub fn take_spawned_shapes(&mut self) -> Vec<crate::scene::Shape> {
        std::mem::take(&mut self.spawned_shapes)
    }
}

impl Default for EvalContext {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Public entry point ───────────────────────────────────────────────────────

/// The result type for expression evaluation.
pub type EvalResult = Result<f32, String>;

/// Evaluate a mathematical expression against a variable context.
///
/// Supported operators: `+`, `-`, `*`, `/`.
/// Supported atoms: numeric literals and variable names from `ctx`.
pub fn evaluate(expr: &str, ctx: &EvalContext) -> EvalResult {
    let tokens = tokenize(expr);
    if tokens.is_empty() {
        return Err("Empty expression".to_string());
    }
    let mut it = tokens.iter().peekable();
    parse_expr(&mut it, ctx).ok_or_else(|| format!("Failed to evaluate: '{}'", expr))
}

// ─── Tokens ───────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
enum Tok<'a> {
    Num(f32),
    Ident(&'a str),
    Op(char),
    LParen,
    RParen,
}

fn tokenize(s: &str) -> Vec<Tok<'_>> {
    let mut out = Vec::new();
    let mut i = 0usize;
    let bytes = s.as_bytes();

    while i < s.len() {
        let c = bytes[i] as char;

        if c.is_whitespace() {
            i += 1;
            continue;
        }

        match c {
            '(' => {
                out.push(Tok::LParen);
                i += 1;
            }
            ')' => {
                out.push(Tok::RParen);
                i += 1;
            }
            '+' | '-' | '*' | '/' => {
                out.push(Tok::Op(c));
                i += 1;
            }
            _ if c.is_ascii_digit() || c == '.' => {
                let start = i;
                while i < s.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                    i += 1;
                }
                if let Ok(v) = s[start..i].parse::<f32>() {
                    out.push(Tok::Num(v));
                }
            }
            _ if c.is_alphabetic() => {
                let start = i;
                while i < s.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                out.push(Tok::Ident(&s[start..i]));
            }
            _ => {
                i += 1;
            }
        }
    }

    out
}

// ─── Recursive-descent parser ─────────────────────────────────────────────────

use std::iter::Peekable;
use std::slice::Iter;

fn parse_expr<'a>(it: &mut Peekable<Iter<'a, Tok<'a>>>, ctx: &EvalContext) -> Option<f32> {
    parse_addsub(it, ctx)
}

fn parse_addsub<'a>(it: &mut Peekable<Iter<'a, Tok<'a>>>, ctx: &EvalContext) -> Option<f32> {
    let mut v = parse_muldiv(it, ctx)?;
    while let Some(Tok::Op(op @ ('+' | '-'))) = it.peek() {
        let op = *op;
        it.next();
        let r = parse_muldiv(it, ctx)?;
        v = if op == '+' { v + r } else { v - r };
    }
    Some(v)
}

fn parse_muldiv<'a>(it: &mut Peekable<Iter<'a, Tok<'a>>>, ctx: &EvalContext) -> Option<f32> {
    let mut v = parse_primary(it, ctx)?;
    while let Some(Tok::Op(op @ ('*' | '/'))) = it.peek() {
        let op = *op;
        it.next();
        let r = parse_primary(it, ctx)?;
        v = if op == '*' { v * r } else { v / r };
    }
    Some(v)
}

fn parse_primary<'a>(it: &mut Peekable<Iter<'a, Tok<'a>>>, ctx: &EvalContext) -> Option<f32> {
    match it.next() {
        Some(Tok::Num(n)) => Some(*n),
        Some(Tok::Ident(id)) => ctx.get_number(*id),
        Some(Tok::LParen) => {
            let v = parse_expr(it, ctx)?;
            if matches!(it.next(), Some(Tok::RParen)) {
                Some(v)
            } else {
                None
            }
        }
        _ => None,
    }
}

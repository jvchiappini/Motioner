use std::collections::HashMap;

/// Result of evaluating a DSL expression.
pub type EvalResult = Result<f32, String>;

/// Context containing variables available for expression evaluation (e.g., "seconds", "frame").
pub struct EvalContext {
    pub variables: HashMap<String, f32>,
}

impl EvalContext {
    pub fn new() -> Self {
        Self {
            variables: HashMap::new(),
        }
    }

    pub fn with_var(mut self, name: &str, val: f32) -> Self {
        self.variables.insert(name.to_string(), val);
        self
    }
}

/// Evaluates a mathematical expression string using the provided context.
pub fn evaluate(expr: &str, ctx: &EvalContext) -> EvalResult {
    let tokens = tokenize(expr);
    if tokens.is_empty() {
        return Err("Empty expression".to_string());
    }
    let mut it = tokens.iter().peekable();
    parse_expr(&mut it, ctx).ok_or_else(|| "Failed to parse expression".to_string())
}

#[derive(Clone, Debug)]
enum Tok<'a> {
    Num(f32),
    Ident(&'a str),
    Op(char),
    LPar,
    RPar,
}

fn tokenize<'a>(s: &'a str) -> Vec<Tok<'a>> {
    let mut out = Vec::new();
    let mut i = 0usize;
    let bytes = s.as_bytes();
    while i < s.len() {
        let c = bytes[i] as char;
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c == '(' {
            out.push(Tok::LPar);
            i += 1;
            continue;
        }
        if c == ')' {
            out.push(Tok::RPar);
            i += 1;
            continue;
        }
        if "+-*/".contains(c) {
            out.push(Tok::Op(c));
            i += 1;
            continue;
        }
        if c.is_ascii_digit() || c == '.' {
            let start = i;
            while i < s.len() && ((bytes[i] as char).is_ascii_digit() || (bytes[i] as char) == '.') {
                i += 1;
            }
            if let Ok(v) = s[start..i].parse::<f32>() {
                out.push(Tok::Num(v));
            }
            continue;
        }
        if c.is_alphabetic() {
            let start = i;
            while i < s.len() && ((bytes[i] as char).is_alphanumeric() || (bytes[i] as char) == '_') {
                i += 1;
            }
            out.push(Tok::Ident(&s[start..i]));
            continue;
        }
        i += 1;
    }
    out
}

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
        if op == '+' {
            v += r;
        } else {
            v -= r;
        }
    }
    Some(v)
}

fn parse_muldiv<'a>(it: &mut Peekable<Iter<'a, Tok<'a>>>, ctx: &EvalContext) -> Option<f32> {
    let mut v = parse_primary(it, ctx)?;
    while let Some(Tok::Op(op @ ('*' | '/'))) = it.peek() {
        let op = *op;
        it.next();
        let r = parse_primary(it, ctx)?;
        if op == '*' {
            v *= r;
        } else {
            v /= r;
        }
    }
    Some(v)
}

fn parse_primary<'a>(it: &mut Peekable<Iter<'a, Tok<'a>>>, ctx: &EvalContext) -> Option<f32> {
    match it.next() {
        Some(Tok::Num(n)) => Some(*n),
        Some(Tok::Ident(id)) => ctx.variables.get(*id).copied(),
        Some(Tok::LPar) => {
            let v = parse_expr(it, ctx)?;
            if matches!(it.next(), Some(Tok::RPar)) {
                Some(v)
            } else {
                None
            }
        }
        _ => None,
    }
}

use crate::dsl::evaluator::{self, EvalContext, Value};
use crate::scene::Shape;

/// Parse a list literal like `[1, 2, 3]` or `["a", "b"]` and store it in a var.
/// Usage: `let items = [1, 2, 3]` or `let names = ["A","B"]`.
pub fn exec(_shapes: &mut [Shape], line: &str, ctx: &mut EvalContext) -> Result<bool, String> {
    let rest = if line.starts_with("let ") { &line[4..] } else if line.starts_with("set ") { &line[4..] } else { line };
    let parts: Vec<&str> = rest.splitn(2, '=').collect();
    if parts.len() != 2 {
        return Err("invalid list assignment".to_string());
    }
    let name = parts[0].trim();
    let rhs = parts[1].trim();
    if !rhs.starts_with('[') || !rhs.ends_with(']') {
        return Err("list must be a literal enclosed in [ ]".to_string());
    }
    let inner = &rhs[1..rhs.len() - 1];
    let mut items: Vec<Value> = Vec::new();
    if !inner.trim().is_empty() {
        for part in inner.split(',') {
            let p = part.trim();
            if p.starts_with('"') && p.ends_with('"') {
                items.push(Value::Str(p[1..p.len() - 1].to_string()));
            } else {
                // numeric expression
                let n = evaluator::evaluate(p, ctx)?;
                items.push(Value::Number(n));
            }
        }
    }
    ctx.set_var(name, Value::List(items));
    Ok(false)
}

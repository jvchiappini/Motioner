use crate::dsl::evaluator::{EvalContext, Value};
use crate::scene::Shape;

/// Execute string `let`/`set` assignments, e.g. `let s = "hello"`.
pub fn exec(_shapes: &mut [Shape], line: &str, ctx: &mut EvalContext) -> Result<bool, String> {
    let rest = if line.starts_with("let ") { &line[4..] } else if line.starts_with("set ") { &line[4..] } else { line };
    let parts: Vec<&str> = rest.splitn(2, '=').collect();
    if parts.len() != 2 {
        return Err("invalid string assignment".to_string());
    }
    let name = parts[0].trim();
    let rhs = parts[1].trim();
    // simple quoted-string parsing
    if !(rhs.starts_with('"') && rhs.ends_with('"')) {
        return Err("string value must be quoted".to_string());
    }
    let inner = &rhs[1..rhs.len() - 1];
    ctx.set_var(name, Value::Str(inner.to_string()));
    Ok(false)
}

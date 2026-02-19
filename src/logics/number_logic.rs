use crate::dsl::evaluator::{self, EvalContext, Value};
use crate::scene::Shape;

/// Execute numeric `let`/`set` assignments, e.g. `let x = seconds * 0.1` or
/// `set a = a + 1`.
pub fn exec(_shapes: &mut [Shape], line: &str, ctx: &mut EvalContext) -> Result<bool, String> {
    // accept both `let name = expr` and `set name = expr`
    let rest = if line.starts_with("let ") { &line[4..] } else if line.starts_with("set ") { &line[4..] } else { line };
    let parts: Vec<&str> = rest.splitn(2, '=').collect();
    if parts.len() != 2 {
        return Err("invalid assignment".to_string());
    }
    let name = parts[0].trim();
    if name.is_empty() {
        return Err("missing variable name".to_string());
    }
    let expr = parts[1].trim();
    let v = evaluator::evaluate(expr, ctx)?;
    ctx.set_var(name, Value::Number(v));
    Ok(false)
}

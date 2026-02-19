use crate::dsl::evaluator::{self, EvalContext, Value};
use crate::scene::Shape;

fn eval_condition(cond: &str, ctx: &EvalContext) -> Result<bool, String> {
    let s = cond.trim();
    // comparison operators (check two-char operators first)
    if let Some(idx) = s.find("<=") {
        let (l, r) = s.split_at(idx);
        let rv = evaluator::evaluate(r.trim_start_matches("<=").trim(), ctx)?;
        let lv = evaluator::evaluate(l.trim(), ctx)?;
        return Ok(lv <= rv);
    }
    if let Some(idx) = s.find(">=") {
        let (l, r) = s.split_at(idx);
        let rv = evaluator::evaluate(r.trim_start_matches(">=").trim(), ctx)?;
        let lv = evaluator::evaluate(l.trim(), ctx)?;
        return Ok(lv >= rv);
    }
    if let Some(idx) = s.find("==") {
        let (l, r) = s.split_at(idx);
        let lhs = l.trim();
        let rhs = r.trim_start_matches("==").trim();
        // try numeric compare first
        if let (Ok(ln), Ok(rn)) = (evaluator::evaluate(lhs, ctx), evaluator::evaluate(rhs, ctx)) {
            return Ok((ln - rn).abs() < 1e-6);
        }
        // fallback to string compare
        if let (Some(ls), Some(rs)) = (ctx.get_str(lhs), ctx.get_str(rhs)) {
            return Ok(ls == rs);
        }
        return Ok(false);
    }
    if let Some(idx) = s.find("!=") {
        let (l, r) = s.split_at(idx);
        let lhs = l.trim();
        let rhs = r.trim_start_matches("!=").trim();
        if let (Ok(ln), Ok(rn)) = (evaluator::evaluate(lhs, ctx), evaluator::evaluate(rhs, ctx)) {
            return Ok((ln - rn).abs() >= 1e-6);
        }
        if let (Some(ls), Some(rs)) = (ctx.get_str(lhs), ctx.get_str(rhs)) {
            return Ok(ls != rs);
        }
        return Ok(true);
    }
    if let Some(idx) = s.find('<') {
        let (l, r) = s.split_at(idx);
        let rv = evaluator::evaluate(r.trim_start_matches('<').trim(), ctx)?;
        let lv = evaluator::evaluate(l.trim(), ctx)?;
        return Ok(lv < rv);
    }
    if let Some(idx) = s.find('>') {
        let (l, r) = s.split_at(idx);
        let rv = evaluator::evaluate(r.trim_start_matches('>').trim(), ctx)?;
        let lv = evaluator::evaluate(l.trim(), ctx)?;
        return Ok(lv > rv);
    }

    // plain identifier or expression: numeric truthiness, string/list emptiness
    if let Some(v) = ctx.variables.get(s) {
        match v {
            Value::Number(n) => return Ok(*n != 0.0),
            Value::Str(st) => return Ok(!st.is_empty()),
            Value::List(l) => return Ok(!l.is_empty()),
        }
    }

    // try numeric expression
    if let Ok(n) = evaluator::evaluate(s, ctx) {
        return Ok(n != 0.0);
    }

    Ok(false)
}

/// Execute `if` / `if not` blocks. Syntax:
/// - `if <cond> { ... }`
/// - `if not <cond> { ... }`
pub fn exec(shapes: &mut [Shape], block: &str, ctx: &mut EvalContext) -> Result<bool, String> {
    let brace = block.find('{').ok_or("if: missing '{' in block")?;
    let end_brace = block.rfind('}').ok_or("if: missing '}' in block")?;
    let header = block[..brace].trim();
    let body = &block[brace + 1..end_brace];

    let mut invert = false;
    let mut cond = header
        .strip_prefix("if")
        .ok_or("if: invalid header")?
        .trim();
    if cond.starts_with("not ") {
        invert = true;
        cond = cond.trim_start_matches("not ").trim();
    }

    let res = eval_condition(cond, ctx)?;
    let cond_true = if invert { !res } else { res };
    if cond_true {
        let modified = crate::dsl::runtime::exec_block(shapes, body, ctx)?;
        return Ok(modified);
    }
    Ok(false)
}

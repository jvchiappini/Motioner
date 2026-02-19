use crate::dsl::evaluator::{self, EvalContext, Value};
use crate::scene::Shape;

/// Execute `for` blocks. Supported forms:
/// - `for i in 0..N { ... }`  (numeric range, end exclusive)
/// - `for x in some_list { ... }` (iterate list variable or literal list)
pub fn exec(shapes: &mut [Shape], block: &str, ctx: &mut EvalContext) -> Result<bool, String> {
    // split header and body
    let brace = block.find('{').ok_or("for: missing '{' in block")?;
    let end_brace = block.rfind('}').ok_or("for: missing '}' in block")?;
    let header = block[..brace].trim();
    let body = &block[brace + 1..end_brace];

    // header looks like: for <ident> in <expr>
    let header = header.trim();
    let after_for = header
        .strip_prefix("for")
        .ok_or("for: invalid header")?
        .trim();
    let parts: Vec<&str> = after_for.splitn(3, ' ').collect();
    if parts.len() < 3 || parts[1] != "in" {
        return Err("for: expected 'for <var> in <iterable>'".to_string());
    }
    let var_name = parts[0].trim();
    let iterable = after_for
        .splitn(2, "in")
        .nth(1)
        .ok_or("for: missing iterable")?
        .trim();

    // Range form?
    if iterable.contains("..") {
        let rng: Vec<&str> = iterable.split("..").collect();
        if rng.len() != 2 {
            return Err("for: invalid range".to_string());
        }
        let start = if rng[0].trim().is_empty() {
            0.0
        } else {
            evaluator::evaluate(rng[0].trim(), ctx)?
        };
        let end = evaluator::evaluate(rng[1].trim(), ctx)?;
        let start_i = start as i32;
        let end_i = end as i32;
        let mut modified = false;
        for ii in start_i..end_i {
            ctx.set_var(var_name, Value::Number(ii as f32));
            if crate::dsl::runtime::exec_block(shapes, body, ctx)? {
                modified = true;
            }
        }
        return Ok(modified);
    }

    // Iterable is either a variable name (list) or a literal list
    if iterable.starts_with('[') && iterable.ends_with(']') {
        // parse literal list quickly (reuse list logic parsing rules)
        let inner = &iterable[1..iterable.len() - 1];
        let mut items: Vec<Value> = Vec::new();
        if !inner.trim().is_empty() {
            for part in inner.split(',') {
                let p = part.trim();
                if p.starts_with('"') && p.ends_with('"') {
                    items.push(Value::Str(p[1..p.len() - 1].to_string()));
                } else {
                    let n = evaluator::evaluate(p, ctx)?;
                    items.push(Value::Number(n));
                }
            }
        }
        let mut modified = false;
        for item in items {
            ctx.set_var(var_name, item.clone());
            if crate::dsl::runtime::exec_block(shapes, body, ctx)? {
                modified = true;
            }
        }
        return Ok(modified);
    }

    // variable name — must be a list
    if let Some(list) = ctx.get_list(iterable) {
        // clone out the list so we don't hold an immutable borrow while mutating `ctx`
        let items: Vec<_> = list.iter().cloned().collect();
        let mut modified = false;
        for item in items {
            ctx.set_var(var_name, item);
            if crate::dsl::runtime::exec_block(shapes, body, ctx)? {
                modified = true;
            }
        }
        return Ok(modified);
    }

    Err(format!(
        "for: iterable '{}' not found or not a list",
        iterable
    ))
}

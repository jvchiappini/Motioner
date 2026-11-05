//! Call-site specialisation (D-0036 section 2.3, prompt 12 phase 4).
//!
//! When the value a user edits lives inside a user function, changing the
//! literal would change every invocation. The planner instead specialises the
//! edited call site:
//!
//! - the value is already a parameter: pass (or edit) the keyword argument;
//! - the function returns the node: bind the call result when needed and
//!   assign right after the call;
//! - otherwise: promote the literal to a keyword parameter with the old
//!   literal as default and pass the new value at this call only.
//!
//! Nodes created or written inside a function that is called from a loop are
//! handled by [`crate::loop_guard`], which adds the loop guard after the call.
//! Every edit is verified by [`crate::plan::verify`]; anything the planner
//! cannot make exact falls back with a reason.

use std::collections::HashSet;

use motioner_core::{FrameRate, NodeId};

use crate::ast::{Expr, ExprKind, FunctionDef, Program, Stmt, StmtKind, Target};
use crate::code_edit::{CodeEdit, Replacement, Strategy};
use crate::origins::{
    BindingOrigin, BlockOrigin, CallOrigin, ExpressionOrigin, NodeOrigin, OriginKind, ValueOrigin,
    ValueOrigins,
};
use crate::plan::{Action, Resolved};
use crate::Span;

/// Plans a call-site edit for `node`, or `Ok(None)` when the node has no
/// enclosing user-function call (the caller tries the next strategy).
pub(crate) fn plan_call(
    program: &str,
    origins: &ValueOrigins,
    resolved: &Resolved,
    node: NodeId,
    rate: FrameRate,
) -> Result<Option<CodeEdit>, String> {
    let Some(origin) = origins.node_origin(node) else {
        return Ok(None);
    };
    if !origin.site.loops.is_empty() {
        // Loop guards insert the overriding write inside the loop body.
        return Ok(None);
    }
    let write = super::plan::value_origin(resolved, origins, node);
    let creation_call = origin.site.calls.last();
    let write_call = write.and_then(|write| write.site.calls.last());
    let Some(call) = write_call.or(creation_call) else {
        return Ok(None);
    };
    let depth = write.map_or(origin.site.calls.len(), |write| write.site.calls.len());
    if depth > 1 {
        return Err(
            "the node crosses more than one function call; nested calls are not supported yet"
                .to_owned(),
        );
    }

    let ast = crate::parse(program).map_err(|error| error.to_string())?;
    let Some(call_expr) = find_call(&ast, call.call_site.start) else {
        return Err("the call expression could not be located".to_owned());
    };
    let ExprKind::Call { args, .. } = &call_expr.kind else {
        return Err("the recorded call site is not a call".to_owned());
    };
    let new_value = crate::loop_guard::value_text(resolved)?;

    // The edited value is already a parameter of the function that writes it.
    if let Some(write) = write {
        if let ExpressionOrigin::Parameter { name } = write.expression {
            let parameter = origins.name(name);
            let index = call
                .parameters
                .iter()
                .position(|origin| origins.name(origin.name) == parameter);
            let replacements =
                pass_argument(program, call_expr, args, parameter, index, &new_value)?;
            return Ok(Some(
                CodeEdit::new(
                    replacements,
                    format!(
                        "{} {} by passing {parameter} at the call site",
                        resolved.entry.property,
                        action_label(&resolved.action)
                    ),
                    Strategy::CallArgument,
                )
                .map_err(|error| error.to_string())?,
            ));
        }
    }

    // A function that returns the node gets an assignment right after the
    // call, which is exact for this instance and needs no new parameter.
    if call.returns_node {
        return assign_after_call(
            program, &ast, origins, resolved, node, origin, call, call_expr, rate,
        );
    }

    // Otherwise promote the literal to a keyword parameter.
    let Some(write) = write else {
        return Err("the node is not written by the function's call site".to_owned());
    };
    promote_literal(
        program, &ast, resolved, write, call, call_expr, args, &new_value,
    )
}

/// Binds the call result when needed and writes the assignment after the
/// node's last write in the caller block.
#[allow(clippy::too_many_arguments)] // explicit call-site context
fn assign_after_call(
    program: &str,
    ast: &Program,
    origins: &ValueOrigins,
    resolved: &Resolved,
    node: NodeId,
    origin: &NodeOrigin,
    call: &CallOrigin,
    call_expr: &Expr,
    rate: FrameRate,
) -> Result<Option<CodeEdit>, String> {
    let statement = statement_containing(&ast.body, call.call_site.start)
        .ok_or_else(|| "the call statement could not be located".to_owned())?;
    let binding = caller_binding(origin, call.call_site, statement.span);
    let (binding_name, mut replacements) = match binding {
        Some(binding) if !binding.reassigned_later => {
            (origins.name(binding.name).to_owned(), Vec::new())
        }
        Some(_) => {
            return Err("the variable bound to the call result is reassigned later".to_owned())
        }
        None => {
            if !is_bare_call_statement(statement, call_expr) {
                return Err(
                    "the call result is not bound; assign it to a variable before editing"
                        .to_owned(),
                );
            }
            let mut used = crate::loop_guard::program_names(program);
            let name = crate::loop_guard::fresh_node_name(program, node, &mut used);
            let replacements = vec![Replacement::insert(
                call_expr.span.start,
                format!("{name} = "),
            )];
            (name, replacements)
        }
    };

    let block = binding.map_or(
        BlockOrigin {
            span: statement.span,
            indent: 0,
        },
        |binding| binding.block,
    );
    if let Some(existing) = existing_assignment(origins, node, resolved, &block, statement) {
        replacements.push(Replacement::new(
            existing.span.start,
            existing.span.end,
            crate::loop_guard::value_text(resolved)?,
        ));
        if let Action::Key { ease, .. } = resolved.action {
            if let Some(current) = &existing.ease {
                if current.name != ease.mdf_value() {
                    replacements.push(Replacement::new(
                        current.span.start,
                        current.span.end,
                        ease.mdf_literal(),
                    ));
                }
            } else if ease != motioner_core::Ease::Linear {
                return Err("the existing key has no ease argument to edit".to_owned());
            }
        }
    } else {
        let end = last_write_end(origins, node, &block, statement.span);
        replacements.push(insert_after(
            program,
            end,
            &assignment_text(&binding_name, resolved, rate)?,
        ));
    }
    CodeEdit::new(
        replacements,
        format!(
            "{} {} after the call result {binding_name}",
            resolved.entry.property,
            action_label(&resolved.action)
        ),
        Strategy::CallResult,
    )
    .map(Some)
    .map_err(|error| error.to_string())
}

/// Adds a keyword parameter to the definition, replaces the literal with it
/// and passes the new value at this call site.
#[allow(clippy::too_many_arguments)] // explicit call-site context
fn promote_literal(
    program: &str,
    ast: &Program,
    resolved: &Resolved,
    write: &ValueOrigin,
    call: &CallOrigin,
    call_expr: &Expr,
    args: &[crate::ast::Argument],
    new_value: &str,
) -> Result<Option<CodeEdit>, String> {
    if write.literal.is_none() {
        return Err(
            "the value written in the function is computed, so it cannot become a parameter"
                .to_owned(),
        );
    }
    let Some(function) = find_function(ast, call.definition.start) else {
        return Err("the function definition could not be located".to_owned());
    };
    if program[function.span.start..function.span.end].contains('\n') {
        return Err(
            "the function parameter list spans several lines; put it on one line first".to_owned(),
        );
    }
    let property = resolved.entry.property.as_str();
    let parameter = parameter_name(property, function);
    let old = program[write.span.start..write.span.end].trim().to_owned();
    let definition = append_to_definition(program, function.span, &format!("{parameter}={old}"))
        .ok_or_else(|| "the function parameter list could not be located".to_owned())?;
    let mut replacements = vec![
        Replacement::new(write.span.start, write.span.end, parameter.clone()),
        definition,
    ];
    replacements.extend(append_argument(
        program,
        call_expr,
        args,
        &format!("{parameter}={new_value}"),
    )?);
    CodeEdit::new(
        replacements,
        format!(
            "{} {} by promoting it to the parameter {parameter}",
            resolved.entry.property,
            action_label(&resolved.action)
        ),
        Strategy::CallPromotion,
    )
    .map(Some)
    .map_err(|error| error.to_string())
}

/// The replacements for passing `parameter` at this call: edit the existing
/// positional/keyword value, or append the keyword argument.
fn pass_argument(
    program: &str,
    call_expr: &Expr,
    args: &[crate::ast::Argument],
    parameter: &str,
    index: Option<usize>,
    value: &str,
) -> Result<Vec<Replacement>, String> {
    if let Some(argument) = args
        .iter()
        .find(|argument| argument.name.as_deref() == Some(parameter))
    {
        return Ok(vec![Replacement::new(
            argument.value.span.start,
            argument.value.span.end,
            value.to_owned(),
        )]);
    }
    if let Some(index) = index {
        if let Some(argument) = args.get(index).filter(|argument| argument.name.is_none()) {
            return Ok(vec![Replacement::new(
                argument.value.span.start,
                argument.value.span.end,
                value.to_owned(),
            )]);
        }
    }
    append_argument(program, call_expr, args, &format!("{parameter}={value}"))
}

fn assignment_text(binding: &str, resolved: &Resolved, rate: FrameRate) -> Result<String, String> {
    let value = crate::loop_guard::value_text(resolved)?;
    Ok(crate::loop_guard::write_statement(
        resolved,
        &resolved.entry.property,
        &value,
        binding,
        rate,
    ))
}

fn action_label(action: &Action) -> &'static str {
    match action {
        Action::Set | Action::Visible(_) => "set",
        Action::Key { .. } => "key",
    }
}

/// The caller variable bound to the call result, if any.
fn caller_binding(origin: &NodeOrigin, call_site: Span, statement: Span) -> Option<&BindingOrigin> {
    origin
        .bindings
        .iter()
        .find(|binding| {
            binding.statement.start <= call_site.start && call_site.end <= binding.statement.end
        })
        .or_else(|| {
            origin.bindings.iter().find(|binding| {
                binding.block.span.start <= statement.start
                    && statement.end <= binding.block.span.end
            })
        })
}

/// The last write of `node` in `block`, starting at `creation_statement`.
fn last_write_end(
    origins: &ValueOrigins,
    node: NodeId,
    block: &BlockOrigin,
    creation_statement: Span,
) -> usize {
    let mut end = creation_statement.end;
    for entry in origins.entries().iter().filter(|entry| entry.node == node) {
        if entry.block.span == block.span && entry.statement.end > end {
            end = entry.statement.end;
        }
    }
    end
}

/// An existing assignment for the same property after the call.
fn existing_assignment<'a>(
    origins: &'a ValueOrigins,
    node: NodeId,
    resolved: &Resolved,
    block: &BlockOrigin,
    call_statement: &Stmt,
) -> Option<&'a ValueOrigin> {
    origins.entries().iter().rev().find(|entry| {
        entry.node == node
            && entry.property == resolved.entry.property
            && entry.block.span == block.span
            && entry.statement.start >= call_statement.span.end
            && kind_matches(resolved, &entry.kind)
    })
}

fn kind_matches(resolved: &Resolved, kind: &OriginKind) -> bool {
    match (&resolved.action, kind) {
        (Action::Key { frame, .. }, OriginKind::Key(existing)) => *existing == *frame,
        (Action::Set | Action::Visible(_), OriginKind::Set) => true,
        (Action::Key { .. }, OriginKind::Set)
        | (Action::Set | Action::Visible(_), OriginKind::Key(_)) => false,
    }
}

fn is_bare_call_statement(statement: &Stmt, call: &Expr) -> bool {
    matches!(&statement.kind, StmtKind::Expr(expr) if expr.span.start == call.span.start)
}

/// Inserts a statement after `statement_end`, matching the line's indent and
/// ending.
fn insert_after(program: &str, statement_end: usize, statement: &str) -> Replacement {
    let eol = crate::loop_guard::line_ending(program);
    let offset = crate::loop_guard::line_end(program, statement_end);
    let indent = crate::loop_guard::line_indent_at(program, statement_end);
    let mut text = String::new();
    if offset == program.len() && !program.ends_with('\n') {
        text.push_str(eol);
    }
    text.push_str(&indent);
    text.push_str(statement);
    text.push_str(eol);
    Replacement::insert(offset, text)
}

/// The innermost statement at any block depth that contains `offset`.
fn statement_containing(body: &[Stmt], offset: usize) -> Option<&Stmt> {
    for statement in body {
        if !(statement.span.start <= offset && offset < statement.span.end) {
            continue;
        }
        match &statement.kind {
            StmtKind::If {
                branches,
                otherwise,
            } => {
                for (_, branch) in branches {
                    if let Some(found) = statement_containing(branch, offset) {
                        return Some(found);
                    }
                }
                return statement_containing(otherwise, offset).or(Some(statement));
            }
            StmtKind::For { body, .. } => {
                return statement_containing(body, offset).or(Some(statement));
            }
            _ => return Some(statement),
        }
    }
    None
}

/// Finds the call expression whose span starts at `start`.
fn find_call(program: &Program, start: usize) -> Option<&Expr> {
    fn in_expr(expr: &Expr, start: usize) -> Option<&Expr> {
        if expr.span.start == start && matches!(expr.kind, ExprKind::Call { .. }) {
            return Some(expr);
        }
        match &expr.kind {
            ExprKind::List(items) | ExprKind::Tuple(items) => {
                items.iter().find_map(|item| in_expr(item, start))
            }
            ExprKind::Unary { operand, .. } | ExprKind::Not(operand) => in_expr(operand, start),
            ExprKind::Binary { left, right, .. }
            | ExprKind::Compare { left, right, .. }
            | ExprKind::Logical { left, right, .. } => {
                in_expr(left, start).or_else(|| in_expr(right, start))
            }
            ExprKind::Call { callee, args } => in_expr(callee, start)
                .or_else(|| args.iter().find_map(|arg| in_expr(&arg.value, start))),
            ExprKind::Attribute { object, .. } => in_expr(object, start),
            ExprKind::Index { object, index } => {
                in_expr(object, start).or_else(|| in_expr(index, start))
            }
            ExprKind::Number(_)
            | ExprKind::Str(_)
            | ExprKind::Bool(_)
            | ExprKind::None
            | ExprKind::Name(_) => None,
        }
    }
    fn in_body(body: &[Stmt], start: usize) -> Option<&Expr> {
        for statement in body {
            let found = match &statement.kind {
                StmtKind::Expr(expr)
                | StmtKind::Return(Some(expr))
                | StmtKind::Assign { value: expr, .. } => in_expr(expr, start),
                StmtKind::Def(function) => in_body(&function.body, start),
                StmtKind::If {
                    branches,
                    otherwise,
                } => branches
                    .iter()
                    .find_map(|(condition, body)| {
                        in_expr(condition, start).or_else(|| in_body(body, start))
                    })
                    .or_else(|| in_body(otherwise, start)),
                StmtKind::For { iterable, body, .. } => {
                    in_expr(iterable, start).or_else(|| in_body(body, start))
                }
                StmtKind::Return(None) | StmtKind::Break | StmtKind::Continue | StmtKind::Pass => {
                    None
                }
            };
            if found.is_some() {
                return found;
            }
        }
        None
    }
    in_body(&program.body, start)
}

/// Finds the function whose `def` header starts at `start`.
fn find_function(program: &Program, start: usize) -> Option<&FunctionDef> {
    fn in_body(body: &[Stmt], start: usize) -> Option<&FunctionDef> {
        for statement in body {
            match &statement.kind {
                StmtKind::Def(function) => {
                    if statement.span.start == start {
                        return Some(function);
                    }
                    if let Some(found) = in_body(&function.body, start) {
                        return Some(found);
                    }
                }
                StmtKind::If {
                    branches,
                    otherwise,
                } => {
                    for (_, branch) in branches {
                        if let Some(found) = in_body(branch, start) {
                            return Some(found);
                        }
                    }
                    if let Some(found) = in_body(otherwise, start) {
                        return Some(found);
                    }
                }
                StmtKind::For { body, .. } => {
                    if let Some(found) = in_body(body, start) {
                        return Some(found);
                    }
                }
                _ => {}
            }
        }
        None
    }
    in_body(&program.body, start)
}

/// A readable, unique parameter name for `property`.
fn parameter_name(property: &str, function: &FunctionDef) -> String {
    let mut used: HashSet<String> = function
        .params
        .iter()
        .map(|param| param.name.to_string())
        .collect();
    used.extend(body_names(&function.body));
    let base: String = property
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect();
    let base = if base.is_empty() {
        "value".to_owned()
    } else if base.starts_with(|character: char| character.is_ascii_digit()) {
        format!("value_{base}")
    } else {
        base
    };
    if used.insert(base.clone()) {
        return base;
    }
    for suffix in 2.. {
        let candidate = format!("{base}_{suffix}");
        if used.insert(candidate.clone()) {
            return candidate;
        }
    }
    unreachable!("the suffix loop always finds a free name")
}

/// Every name read or assigned in `body`, recursively.
fn body_names(body: &[Stmt]) -> HashSet<String> {
    let mut names = HashSet::new();
    for statement in body {
        match &statement.kind {
            StmtKind::Expr(expr) | StmtKind::Return(Some(expr)) => collect_expr(expr, &mut names),
            StmtKind::Assign { target, value, .. } => {
                if let Target::Name(name) = target {
                    names.insert(name.to_string());
                }
                collect_expr(value, &mut names);
            }
            StmtKind::Def(function) => {
                names.extend(body_names(&function.body));
            }
            StmtKind::If {
                branches,
                otherwise,
            } => {
                for (condition, branch) in branches {
                    collect_expr(condition, &mut names);
                    names.extend(body_names(branch));
                }
                names.extend(body_names(otherwise));
            }
            StmtKind::For { iterable, body, .. } => {
                collect_expr(iterable, &mut names);
                names.extend(body_names(body));
            }
            StmtKind::Return(None) | StmtKind::Break | StmtKind::Continue | StmtKind::Pass => {}
        }
    }
    names
}

fn collect_expr(expr: &Expr, names: &mut HashSet<String>) {
    match &expr.kind {
        ExprKind::Name(name) => {
            names.insert(name.to_string());
        }
        ExprKind::List(items) | ExprKind::Tuple(items) => {
            for item in items {
                collect_expr(item, names);
            }
        }
        ExprKind::Unary { operand, .. } | ExprKind::Not(operand) => collect_expr(operand, names),
        ExprKind::Binary { left, right, .. }
        | ExprKind::Compare { left, right, .. }
        | ExprKind::Logical { left, right, .. } => {
            collect_expr(left, names);
            collect_expr(right, names);
        }
        ExprKind::Call { callee, args } => {
            collect_expr(callee, names);
            for argument in args {
                collect_expr(&argument.value, names);
            }
        }
        ExprKind::Attribute { object, .. } => collect_expr(object, names),
        ExprKind::Index { object, index } => {
            collect_expr(object, names);
            collect_expr(index, names);
        }
        ExprKind::Number(_) | ExprKind::Str(_) | ExprKind::Bool(_) | ExprKind::None => {}
    }
}

/// Appends `argument` before the closing parenthesis of `call`, preserving the
/// list layout for single-line and multiline calls.
fn append_argument(
    program: &str,
    call: &Expr,
    args: &[crate::ast::Argument],
    argument: &str,
) -> Result<Vec<Replacement>, String> {
    let rparen = call.span.end - 1;
    if program.as_bytes().get(rparen) != Some(&b')') {
        return Err("the call argument list could not be located".to_owned());
    }
    let text = &program[call.span.start..call.span.end];
    if text.contains('\n') {
        let line_start = program[..rparen]
            .rfind('\n')
            .map_or(call.span.start, |index| index + 1);
        let close_indent = &program[line_start..rparen];
        if close_indent.chars().all(char::is_whitespace) {
            let Some(last) = program[..line_start]
                .char_indices()
                .rev()
                .find(|(_, character)| !character.is_whitespace())
                .map(|(index, _)| index)
            else {
                return Err("the call argument list could not be located".to_owned());
            };
            let argument_line = program[..=last]
                .rfind('\n')
                .map_or(call.span.start, |index| index + 1);
            let argument_indent: String = program[argument_line..]
                .chars()
                .take_while(|character| character.is_whitespace() && *character != '\n')
                .collect();
            let mut replacements = Vec::new();
            if program.as_bytes().get(last) != Some(&b',') {
                replacements.push(Replacement::insert(last + 1, ",".to_owned()));
            }
            replacements.push(Replacement::insert(
                line_start,
                format!(
                    "{argument_indent}{argument},{}",
                    crate::loop_guard::line_ending(program)
                ),
            ));
            return Ok(replacements);
        }
    }
    if args.is_empty() {
        return Ok(vec![Replacement::insert(rparen, argument.to_owned())]);
    }
    let last = program[..rparen]
        .chars()
        .rev()
        .find(|character| !character.is_whitespace())
        .ok_or_else(|| "the call argument list could not be located".to_owned())?;
    if last == ',' {
        Ok(vec![Replacement::insert(rparen, format!(" {argument}"))])
    } else {
        Ok(vec![Replacement::insert(rparen, format!(", {argument}"))])
    }
}

/// Appends `argument` to a single-line definition header.
fn append_to_definition(program: &str, header: Span, argument: &str) -> Option<Replacement> {
    let text = program.get(header.start..header.end)?;
    let close = text.rfind(')')?;
    if text[..close].trim_end().ends_with(',') {
        Some(Replacement::insert(
            header.start + close,
            format!(" {argument}"),
        ))
    } else {
        Some(Replacement::insert(
            header.start + close,
            format!(", {argument}"),
        ))
    }
}

//! Advisory linter for generated MDF code (prompt 12 phase 7).
//!
//! The linter reports the idioms that make editor-generated code hard to edit
//! later, as warnings with real source spans. It never changes the program and
//! never blocks rendering: [`lint`] returns an empty vector for a program
//! without smells and only returns `Err` when the program does not parse.
//!
//! Rules (all heuristics are deliberately conservative):
//!
//! - **Unbound inline node**: a scene constructor (`rect`, `circle`, `group`,
//!   `text`, ...) used inside a larger expression instead of being bound to a
//!   name (`parent.add(circle(10))`). The planner can only rewrite a node it
//!   can name, so generated code should always bind it.
//! - **Empty guard**: an `if`/`elif` branch whose whole body is `pass`.
//! - **Dead guard**: an assignment inside an `if` whose target was already
//!   given the same literal value earlier in the same block. Only plain
//!   literals (numbers, strings, booleans, `None`, pairs of literals) count,
//!   which is the simple single-value case. Constructor keyword arguments
//!   count as the unguarded value too, so `c = circle(10, fill="#fff")`
//!   followed by `if i == 3: c.fill = "#fff"` is reported.
//! - **Duplicate assignment**: the same target property assigned twice
//!   unconditionally in one block; one of the writes is redundant.
//! - **Underscore guard**: an `if` condition that reads `_`, the usual result
//!   of a `for _ in ...` loop that later grew a generated guard. Rename the
//!   loop variable so the guard names one iteration.
//! - **Unused parameter**: a function parameter that the body never reads.
//!   Parameter assignment targets do not count as reads.
//! - **Magic number**: the same non-trivial numeric literal assigned to a
//!   name or property more than three times in one block. Numbers passed to
//!   calls (layout, sizes, keyframe times) are not counted: repeating a
//!   coordinate is normal, repeating a bare constant is a missing variable.
//!
//! Diagnostics are sorted by source position so repeated runs and machines
//! produce identical output.

use std::collections::{BTreeMap, BTreeSet};

use motioner_core::format_number;

use crate::ast::{AssignOp, Expr, ExprKind, FunctionDef, Stmt, StmtKind, Target, UnaryOp};
use crate::diagnostic::{Diagnostic, ErrorCategory, Span};
use crate::parser::parse;

/// A numeric literal repeated more often than this in one block is reported.
const MAGIC_REPEAT_LIMIT: usize = 3;

/// Lints `source` and returns its advisory diagnostics.
///
/// # Errors
///
/// Returns the first lexical or parse [`Diagnostic`] when `source` is not a
/// valid MDF program.
pub fn lint(source: &str) -> Result<Vec<Diagnostic>, Diagnostic> {
    let program = parse(source)?;
    let mut diagnostics = Vec::new();
    check_block(&program.body, &mut diagnostics);
    check_inline_constructors(&program.body, &mut diagnostics);
    check_prelude_shadowing(&program.body, &mut diagnostics);
    diagnostics.sort_by(|left, right| {
        left.span
            .start
            .cmp(&right.span.start)
            .then_with(|| left.message.cmp(&right.message))
    });
    diagnostics.dedup();
    Ok(diagnostics)
}

/// Warns when a top-level definition shadows a standard-library name.
fn check_prelude_shadowing(stmts: &[Stmt], out: &mut Vec<Diagnostic>) {
    let names = crate::prelude::exported_names();
    if names.is_empty() {
        return;
    }
    for statement in stmts {
        if let StmtKind::Def(function) = &statement.kind {
            if names.iter().any(|name| name == &*function.name) {
                out.push(warning(
                    function.span,
                    format!(
                        "function '{}' shadows a standard-library name; rename it to keep the \
                         program portable",
                        function.name
                    ),
                ));
            }
        }
    }
}

// ---- block walks -----------------------------------------------------------

fn check_block(stmts: &[Stmt], out: &mut Vec<Diagnostic>) {
    let mut known: BTreeMap<String, &Expr> = BTreeMap::new();
    let mut seen: BTreeMap<String, Span> = BTreeMap::new();
    let mut numbers: BTreeMap<u64, Vec<Span>> = BTreeMap::new();
    for stmt in stmts {
        check_statement(stmt, &mut known, &mut seen, &mut numbers, out);
    }
    report_magic_numbers(numbers, out);
}

fn check_statement<'a>(
    stmt: &'a Stmt,
    known: &mut BTreeMap<String, &'a Expr>,
    seen: &mut BTreeMap<String, Span>,
    numbers: &mut BTreeMap<u64, Vec<Span>>,
    out: &mut Vec<Diagnostic>,
) {
    match &stmt.kind {
        StmtKind::Assign { target, op, value } => {
            if *op == AssignOp::Set {
                record_assignment(stmt.span, target, value, known, seen, numbers, out);
            }
        }
        StmtKind::If {
            branches,
            otherwise,
        } => {
            for (condition, body) in branches {
                if let Some(span) = find_name(condition, "_") {
                    out.push(warning(
                        span,
                        "guard reads `_`; name the loop variable so generated guards can target one iteration",
                    ));
                }
                if body.len() == 1 && matches!(&body[0].kind, StmtKind::Pass) {
                    out.push(warning(
                        body[0].span,
                        "empty guard: this branch only contains `pass`",
                    ));
                }
                for branch_stmt in body {
                    check_dead_assignment(branch_stmt, known, out);
                }
                check_block(body, out);
            }
            check_block(otherwise, out);
        }
        StmtKind::For { body, .. } => check_block(body, out),
        StmtKind::Def(def) => {
            check_unused_parameters(def, out);
            check_block(&def.body, out);
        }
        StmtKind::Expr(_)
        | StmtKind::Return(_)
        | StmtKind::Break
        | StmtKind::Continue
        | StmtKind::Pass => {}
    }
}

fn record_assignment<'a>(
    span: Span,
    target: &'a Target,
    value: &'a Expr,
    known: &mut BTreeMap<String, &'a Expr>,
    seen: &mut BTreeMap<String, Span>,
    numbers: &mut BTreeMap<u64, Vec<Span>>,
    out: &mut Vec<Diagnostic>,
) {
    if let Some(key) = target_key(target) {
        if matches!(target, Target::Attribute { .. }) && seen.insert(key.clone(), span).is_some() {
            out.push(warning(
                span,
                format!("duplicate assignment to `{key}` in the same block; keep one write"),
            ));
        }
        known.insert(key, value);
    }
    if let Some(number) = scalar_number(value) {
        if !is_trivial_number(number) {
            numbers
                .entry(number.to_bits())
                .or_default()
                .push(value.span);
        }
    }
    record_constructor_bindings(target, value, known);
}

/// Remembers constructor keyword arguments as the node's initial values, so a
/// guard that repeats one of them is recognised as dead.
fn record_constructor_bindings<'a>(
    target: &'a Target,
    value: &'a Expr,
    known: &mut BTreeMap<String, &'a Expr>,
) {
    let Target::Name(name) = target else {
        return;
    };
    let ExprKind::Call { callee, args } = &value.kind else {
        return;
    };
    let ExprKind::Name(callee) = &callee.kind else {
        return;
    };
    if !is_constructor(callee) {
        return;
    }
    for argument in args {
        if let Some(argument_name) = &argument.name {
            known.insert(format!("{name}.{argument_name}"), &argument.value);
        }
    }
}

fn check_dead_assignment(stmt: &Stmt, known: &BTreeMap<String, &Expr>, out: &mut Vec<Diagnostic>) {
    let StmtKind::Assign {
        target,
        op: AssignOp::Set,
        value,
    } = &stmt.kind
    else {
        return;
    };
    let Some(key) = target_key(target) else {
        return;
    };
    let Some(unguarded) = known.get(&key) else {
        return;
    };
    if is_literal(value) && is_literal(unguarded) && same_literal(value, unguarded) {
        out.push(warning(
            stmt.span,
            format!(
                "dead guard: `{key}` is already set to this value by the unguarded write above"
            ),
        ));
    }
}

fn report_magic_numbers(numbers: BTreeMap<u64, Vec<Span>>, out: &mut Vec<Diagnostic>) {
    for (bits, spans) in numbers {
        if spans.len() > MAGIC_REPEAT_LIMIT {
            let value = format_number(f64::from_bits(bits));
            let count = spans.len();
            out.push(warning(
                spans[0],
                format!(
                    "the number {value} appears {count} times in this block; assign it to a named variable"
                ),
            ));
        }
    }
}

// ---- unused parameters -----------------------------------------------------

fn check_unused_parameters(def: &FunctionDef, out: &mut Vec<Diagnostic>) {
    let mut used = BTreeSet::new();
    for stmt in &def.body {
        collect_used_names(stmt, &mut used);
    }
    for parameter in &def.params {
        if !used.contains(parameter.name.as_ref()) {
            let name = &parameter.name;
            let function = &def.name;
            out.push(warning(
                def.span,
                format!("parameter `{name}` of `{function}` is never used; remove it or use it"),
            ));
        }
    }
}

fn collect_used_names(stmt: &Stmt, names: &mut BTreeSet<String>) {
    match &stmt.kind {
        StmtKind::Expr(expression)
        | StmtKind::Assign {
            value: expression, ..
        }
        | StmtKind::Return(Some(expression)) => collect_expr_names(expression, names),
        StmtKind::Def(def) => {
            for parameter in &def.params {
                if let Some(default) = &parameter.default {
                    collect_expr_names(default, names);
                }
            }
            for nested in &def.body {
                collect_used_names(nested, names);
            }
        }
        StmtKind::If {
            branches,
            otherwise,
        } => {
            for (condition, body) in branches {
                collect_expr_names(condition, names);
                for nested in body {
                    collect_used_names(nested, names);
                }
            }
            for nested in otherwise {
                collect_used_names(nested, names);
            }
        }
        StmtKind::For { iterable, body, .. } => {
            collect_expr_names(iterable, names);
            for nested in body {
                collect_used_names(nested, names);
            }
        }
        StmtKind::Return(None) | StmtKind::Break | StmtKind::Continue | StmtKind::Pass => {}
    }
}

fn collect_expr_names(expression: &Expr, names: &mut BTreeSet<String>) {
    let mut stack = vec![expression];
    while let Some(current) = stack.pop() {
        match &current.kind {
            ExprKind::Name(name) => {
                names.insert(name.to_string());
            }
            ExprKind::List(items) | ExprKind::Tuple(items) => {
                stack.extend(items.iter());
            }
            ExprKind::Unary { operand, .. } | ExprKind::Not(operand) => stack.push(operand),
            ExprKind::Binary { left, right, .. }
            | ExprKind::Compare { left, right, .. }
            | ExprKind::Logical { left, right, .. } => {
                stack.push(left);
                stack.push(right);
            }
            ExprKind::Call { callee, args } => {
                stack.push(callee);
                for argument in args {
                    stack.push(&argument.value);
                }
            }
            ExprKind::Attribute { object, .. } => stack.push(object),
            ExprKind::Index { object, index } => {
                stack.push(object);
                stack.push(index);
            }
            ExprKind::Number(_) | ExprKind::Str(_) | ExprKind::Bool(_) | ExprKind::None => {}
        }
    }
}

// ---- unbound inline constructors ------------------------------------------

fn check_inline_constructors(stmts: &[Stmt], out: &mut Vec<Diagnostic>) {
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::Assign { target, value, .. } => {
                let bound = matches!(target, Target::Name(_)) && is_constructor_call(value);
                scan_inline(value, bound, out);
            }
            StmtKind::Expr(expression) | StmtKind::Return(Some(expression)) => {
                scan_inline(expression, true, out);
            }
            StmtKind::Def(def) => {
                for parameter in &def.params {
                    if let Some(default) = &parameter.default {
                        scan_inline(default, true, out);
                    }
                }
                check_inline_constructors(&def.body, out);
            }
            StmtKind::If {
                branches,
                otherwise,
            } => {
                for (condition, body) in branches {
                    scan_inline(condition, true, out);
                    check_inline_constructors(body, out);
                }
                check_inline_constructors(otherwise, out);
            }
            StmtKind::For { iterable, body, .. } => {
                scan_inline(iterable, true, out);
                check_inline_constructors(body, out);
            }
            StmtKind::Return(None) | StmtKind::Break | StmtKind::Continue | StmtKind::Pass => {}
        }
    }
}

fn scan_inline(expression: &Expr, root_is_bound: bool, out: &mut Vec<Diagnostic>) {
    let mut stack = vec![(expression, root_is_bound)];
    while let Some((current, bound)) = stack.pop() {
        match &current.kind {
            ExprKind::Call { callee, args } => {
                if let ExprKind::Name(name) = &callee.kind {
                    if is_constructor(name) && !bound {
                        out.push(warning(
                            current.span,
                            format!(
                                "node constructor `{name}(...)` is used without a binding; bind it to a name so generated edits can target it"
                            ),
                        ));
                    }
                }
                stack.push((callee, false));
                for argument in args {
                    stack.push((&argument.value, false));
                }
            }
            ExprKind::List(items) | ExprKind::Tuple(items) => {
                for item in items {
                    stack.push((item, false));
                }
            }
            ExprKind::Unary { operand, .. } | ExprKind::Not(operand) => {
                stack.push((operand, false));
            }
            ExprKind::Binary { left, right, .. }
            | ExprKind::Compare { left, right, .. }
            | ExprKind::Logical { left, right, .. } => {
                stack.push((left, false));
                stack.push((right, false));
            }
            ExprKind::Attribute { object, .. } => stack.push((object, false)),
            ExprKind::Index { object, index } => {
                stack.push((object, false));
                stack.push((index, false));
            }
            ExprKind::Number(_)
            | ExprKind::Str(_)
            | ExprKind::Bool(_)
            | ExprKind::None
            | ExprKind::Name(_) => {}
        }
    }
}

// ---- helpers ---------------------------------------------------------------

fn warning(span: Span, message: impl Into<String>) -> Diagnostic {
    Diagnostic::warning(ErrorCategory::Value, span, message)
}

fn target_key(target: &Target) -> Option<String> {
    match target {
        Target::Name(name) => Some(name.to_string()),
        Target::Attribute { object, name } => {
            let parent = expr_path(object)?;
            Some(format!("{parent}.{name}"))
        }
        Target::Index { .. } => None,
    }
}

fn expr_path(expression: &Expr) -> Option<String> {
    match &expression.kind {
        ExprKind::Name(name) => Some(name.to_string()),
        ExprKind::Attribute { object, name } => {
            let parent = expr_path(object)?;
            Some(format!("{parent}.{name}"))
        }
        _ => None,
    }
}

fn is_constructor(name: &str) -> bool {
    matches!(
        name,
        "comp"
            | "group"
            | "rect"
            | "circle"
            | "path"
            | "polygon"
            | "star"
            | "arc"
            | "text"
            | "latex"
            | "image"
            | "video"
            | "audio"
            | "camera"
            | "light"
            | "mesh"
            | "particles"
    )
}

fn is_constructor_call(expression: &Expr) -> bool {
    let ExprKind::Call { callee, .. } = &expression.kind else {
        return false;
    };
    let ExprKind::Name(name) = &callee.kind else {
        return false;
    };
    is_constructor(name)
}

fn find_name(expression: &Expr, wanted: &str) -> Option<Span> {
    let mut found: Option<Span> = None;
    let mut stack = vec![expression];
    while let Some(current) = stack.pop() {
        if let ExprKind::Name(name) = &current.kind {
            if name.as_ref() == wanted {
                found = Some(found.map_or(current.span, |span: Span| {
                    if current.span.start < span.start {
                        current.span
                    } else {
                        span
                    }
                }));
            }
        }
        match &current.kind {
            ExprKind::List(items) | ExprKind::Tuple(items) => stack.extend(items.iter()),
            ExprKind::Unary { operand, .. } | ExprKind::Not(operand) => stack.push(operand),
            ExprKind::Binary { left, right, .. }
            | ExprKind::Compare { left, right, .. }
            | ExprKind::Logical { left, right, .. } => {
                stack.push(left);
                stack.push(right);
            }
            ExprKind::Call { callee, args } => {
                stack.push(callee);
                for argument in args {
                    stack.push(&argument.value);
                }
            }
            ExprKind::Attribute { object, .. } => stack.push(object),
            ExprKind::Index { object, index } => {
                stack.push(object);
                stack.push(index);
            }
            ExprKind::Number(_)
            | ExprKind::Str(_)
            | ExprKind::Bool(_)
            | ExprKind::None
            | ExprKind::Name(_) => {}
        }
    }
    found
}

/// The value of a scalar number literal, including `-42` and `+42`.
fn scalar_number(expression: &Expr) -> Option<f64> {
    match &expression.kind {
        ExprKind::Number(value) => Some(*value),
        ExprKind::Unary { op, operand } => match (&operand.kind, op) {
            (ExprKind::Number(value), UnaryOp::Neg) => Some(-*value),
            (ExprKind::Number(value), UnaryOp::Pos) => Some(*value),
            _ => None,
        },
        _ => None,
    }
}

fn is_trivial_number(value: f64) -> bool {
    value.to_bits() == 0.0_f64.to_bits() || value.to_bits() == 1.0_f64.to_bits()
}

fn is_literal(expression: &Expr) -> bool {
    let mut stack = vec![expression];
    while let Some(current) = stack.pop() {
        match &current.kind {
            ExprKind::Number(_) | ExprKind::Str(_) | ExprKind::Bool(_) | ExprKind::None => {}
            ExprKind::Tuple(items) | ExprKind::List(items) => stack.extend(items.iter()),
            ExprKind::Unary {
                op: UnaryOp::Neg | UnaryOp::Pos,
                operand,
            } => stack.push(operand),
            _ => return false,
        }
    }
    true
}

fn same_literal(left: &Expr, right: &Expr) -> bool {
    let mut stack = vec![(left, right)];
    while let Some((left, right)) = stack.pop() {
        match (&left.kind, &right.kind) {
            (ExprKind::Number(left), ExprKind::Number(right)) => {
                if left.to_bits() != right.to_bits() {
                    return false;
                }
            }
            (ExprKind::Str(left), ExprKind::Str(right)) => {
                if left != right {
                    return false;
                }
            }
            (ExprKind::Bool(left), ExprKind::Bool(right)) => {
                if left != right {
                    return false;
                }
            }
            (ExprKind::None, ExprKind::None) => {}
            (ExprKind::Tuple(left), ExprKind::Tuple(right))
            | (ExprKind::List(left), ExprKind::List(right)) => {
                if left.len() != right.len() {
                    return false;
                }
                stack.extend(left.iter().zip(right));
            }
            (
                ExprKind::Unary {
                    op: left_op,
                    operand: left_operand,
                },
                ExprKind::Unary {
                    op: right_op,
                    operand: right_operand,
                },
            ) => {
                if left_op != right_op {
                    return false;
                }
                stack.push((left_operand, right_operand));
            }
            _ => return false,
        }
    }
    true
}

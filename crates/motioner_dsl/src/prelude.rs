//! The MDF standard-library prelude (D-0039).
//!
//! `std/*.motion` ships inside the binary with [`include_str!`] and is parsed
//! as a prelude before the user program. Prelude definitions share the global
//! scope, so a user definition shadows one and `lint` warns about it. Node
//! creation inside the prelude keeps its own spans, shifted past
//! [`LIBRARY_OFFSET`]; the write-through planner treats those spans as
//! read-only and resolves edits to the user's call arguments instead.
//!
//! The prelude carries [`VERSION`] with the language: breaking changes need a
//! migration because trained models depend on it.

use crate::ast::{Argument, Expr, ExprKind, FunctionDef, Param, Program, Stmt, StmtKind, Target};
use crate::diagnostic::Span;

/// The standard-library version, reported by `project_summary`/`query` and by
/// `std_version()`.
pub const VERSION: u32 = 1;

/// Byte-offset base for every span that comes from the prelude.
///
/// User programs are far below it; the planner uses
/// [`is_library_span`] instead of comparing against a real file.
pub const LIBRARY_OFFSET: usize = 100_000_000;

/// Line-number base for spans that come from the prelude.
const LIBRARY_LINE_OFFSET: u32 = 1_000_000;

/// Every prelude file, concatenated in load order.
#[must_use]
pub fn source() -> &'static str {
    concat!(
        include_str!("../std/std.motion"),
        "\n",
        include_str!("../std/motion.motion"),
        "\n",
        include_str!("../std/components.motion"),
    )
}

/// Whether a span belongs to the standard library.
#[must_use]
pub fn is_library_span(span: Span) -> bool {
    span.start >= LIBRARY_OFFSET
}

/// Maps a library span back to the prelude's own coordinates.
#[must_use]
pub(crate) fn normalize_span(mut span: Span) -> Span {
    span.start = span.start.saturating_sub(LIBRARY_OFFSET);
    span.end = span.end.saturating_sub(LIBRARY_OFFSET);
    span.line = span.line.saturating_sub(LIBRARY_LINE_OFFSET);
    span
}

/// Parses the prelude and shifts every span into the library range.
///
/// # Errors
///
/// Returns a diagnostic when the shipped prelude does not parse, which is a
/// build-time bug rather than a user error.
pub(crate) fn parse_library() -> Result<Program, crate::Diagnostic> {
    let mut program = crate::parse(source())?;
    shift_program(&mut program, LIBRARY_OFFSET, LIBRARY_LINE_OFFSET);
    Ok(program)
}

/// The names the prelude defines at top level, in source order.
///
/// Used by `lint` to warn about shadowing.
#[must_use]
pub fn exported_names() -> Vec<String> {
    let Ok(program) = crate::parse(source()) else {
        return Vec::new();
    };
    program
        .body
        .iter()
        .filter_map(|statement| match &statement.kind {
            StmtKind::Def(function) => Some(function.name.to_string()),
            _ => None,
        })
        .collect()
}

/// Builds the program the interpreter runs: prelude statements followed by
/// the user's statements.
#[must_use]
pub(crate) fn with_user(mut library: Program, user: Program) -> Program {
    library.body.extend(user.body);
    library
}

fn shift_span(span: &mut Span, byte_offset: usize, line_offset: u32) {
    span.start = span.start.saturating_add(byte_offset);
    span.end = span.end.saturating_add(byte_offset);
    span.line = span.line.saturating_add(line_offset);
}

fn shift_program(program: &mut Program, byte_offset: usize, line_offset: u32) {
    for statement in &mut program.body {
        shift_statement(statement, byte_offset, line_offset);
    }
}

fn shift_statement(statement: &mut Stmt, byte_offset: usize, line_offset: u32) {
    shift_span(&mut statement.span, byte_offset, line_offset);
    match &mut statement.kind {
        StmtKind::Expr(expr) => shift_expr(expr, byte_offset, line_offset),
        StmtKind::Assign { target, value, .. } => {
            shift_target(target, byte_offset, line_offset);
            shift_expr(value, byte_offset, line_offset);
        }
        StmtKind::Def(function) => {
            shift_function(std::rc::Rc::make_mut(function), byte_offset, line_offset);
        }
        StmtKind::Return(value) => {
            if let Some(value) = value {
                shift_expr(value, byte_offset, line_offset);
            }
        }
        StmtKind::If {
            branches,
            otherwise,
        } => {
            for (condition, body) in branches {
                shift_expr(condition, byte_offset, line_offset);
                for statement in body {
                    shift_statement(statement, byte_offset, line_offset);
                }
            }
            for statement in otherwise {
                shift_statement(statement, byte_offset, line_offset);
            }
        }
        StmtKind::For { iterable, body, .. } => {
            shift_expr(iterable, byte_offset, line_offset);
            for statement in body {
                shift_statement(statement, byte_offset, line_offset);
            }
        }
        StmtKind::Break | StmtKind::Continue | StmtKind::Pass => {}
    }
}

fn shift_function(function: &mut FunctionDef, byte_offset: usize, line_offset: u32) {
    shift_span(&mut function.span, byte_offset, line_offset);
    for Param { default, .. } in &mut function.params {
        if let Some(default) = default {
            shift_expr(default, byte_offset, line_offset);
        }
    }
    for statement in &mut function.body {
        shift_statement(statement, byte_offset, line_offset);
    }
}

fn shift_target(target: &mut Target, byte_offset: usize, line_offset: u32) {
    match target {
        Target::Name(_) => {}
        Target::Attribute { object, .. } => shift_expr(object, byte_offset, line_offset),
        Target::Index { object, index } => {
            shift_expr(object, byte_offset, line_offset);
            shift_expr(index, byte_offset, line_offset);
        }
    }
}

fn shift_expr(expr: &mut Expr, byte_offset: usize, line_offset: u32) {
    shift_span(&mut expr.span, byte_offset, line_offset);
    match &mut expr.kind {
        ExprKind::Number(_)
        | ExprKind::Str(_)
        | ExprKind::Bool(_)
        | ExprKind::None
        | ExprKind::Name(_) => {}
        ExprKind::List(items) | ExprKind::Tuple(items) => {
            for item in items {
                shift_expr(item, byte_offset, line_offset);
            }
        }
        ExprKind::Unary { operand, .. } | ExprKind::Not(operand) => {
            shift_expr(operand, byte_offset, line_offset);
        }
        ExprKind::Binary { left, right, .. }
        | ExprKind::Compare { left, right, .. }
        | ExprKind::Logical { left, right, .. } => {
            shift_expr(left, byte_offset, line_offset);
            shift_expr(right, byte_offset, line_offset);
        }
        ExprKind::Call { callee, args } => {
            shift_expr(callee, byte_offset, line_offset);
            for Argument { value, .. } in args {
                shift_expr(value, byte_offset, line_offset);
            }
        }
        ExprKind::Attribute { object, .. } => shift_expr(object, byte_offset, line_offset),
        ExprKind::Index { object, index } => {
            shift_expr(object, byte_offset, line_offset);
            shift_expr(index, byte_offset, line_offset);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_prelude_parses_and_has_a_version() {
        let program = parse_library().unwrap();
        assert!(!program.body.is_empty());
        for statement in &program.body {
            assert!(
                is_library_span(statement.span),
                "prelude statement did not get a library span"
            );
        }
    }

    #[test]
    fn exported_names_are_unique() {
        let mut names = exported_names();
        let count = names.len();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), count, "the prelude defines a name twice");
    }
}

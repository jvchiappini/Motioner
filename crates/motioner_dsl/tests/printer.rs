//! Canonical printer regressions (prompt 12 phase 7).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use motioner_dsl::ast::{Expr, ExprKind, FunctionDef, Program, Stmt, StmtKind, Target};
use motioner_dsl::printer::{format, format_check, format_signature, format_snippet};
use motioner_dsl::{parse, Span};

fn examples_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}

fn example_sources() -> Vec<(String, String)> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(examples_dir())
        .expect("examples directory")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "motion")
        })
        .collect();
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let name = path
                .file_name()
                .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
            let source = std::fs::read_to_string(&path).expect("read example");
            (name, source)
        })
        .collect()
}

fn comments(source: &str) -> Vec<String> {
    source
        .lines()
        .filter_map(|line| comment_start(line).map(|at| line[at..].trim_end().to_owned()))
        .collect()
}

fn comment_start(line: &str) -> Option<usize> {
    let mut quote: Option<u8> = None;
    let mut escaped = false;
    for (index, byte) in line.bytes().enumerate() {
        if let Some(open) = quote {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == open {
                quote = None;
            }
            continue;
        }
        match byte {
            b'#' => return Some(index),
            b'"' | b'\'' => quote = Some(byte),
            _ => {}
        }
    }
    None
}

#[test]
fn every_example_formats_idempotently_and_keeps_its_line_endings() {
    for (name, source) in example_sources() {
        let once = format(&source).unwrap_or_else(|error| panic!("{name}: {error}"));
        let twice = format(&once).unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(once, twice, "{name} is not idempotent");
        assert!(once.ends_with('\n'), "{name} has no trailing newline");
        assert_eq!(
            once.contains("\r\n"),
            source.contains("\r\n"),
            "{name} changed its line endings"
        );
        parse(&once).unwrap_or_else(|error| panic!("{name} is not re-parsable: {error}"));
    }
}

#[test]
fn formatting_preserves_the_ast_semantics() {
    for (name, source) in example_sources() {
        let formatted = format(&source).unwrap_or_else(|error| panic!("{name}: {error}"));
        let before = normalize(parse(&source).unwrap());
        let after = normalize(parse(&formatted).unwrap());
        assert_eq!(before, after, "{name} changed its AST");
    }
}

#[test]
fn comments_are_kept_in_order_with_sensible_indentation() {
    let source = "\
# leading comment
# second leading

c = circle(10)  # inline circle
# block comment between statements
# second line of the block

if c.radius == 10:  # header comment
    # inside the block
    c.fill = '#ff0000'  # value comment

d = rect(
    20,
    30,
)  # trailing after a multiline call
";
    let formatted = format(source).unwrap();
    assert_eq!(comments(&formatted), comments(source));
    assert!(formatted.contains("c = circle(10)  # inline circle"));
    assert!(formatted.contains("if c.radius == 10:  # header comment"));
    assert!(formatted.contains("    # inside the block\n"));
    assert!(formatted.contains("    c.fill = \"#ff0000\"  # value comment"));
    assert!(formatted.contains("d = rect(20, 30)  # trailing after a multiline call"));
    assert!(format_check(&formatted).is_ok());
}

#[test]
fn a_comment_inside_a_multiline_call_is_refused_instead_of_dropped() {
    let source = "x = f(1,\n    # note\n    2)\n";
    assert!(format(source).is_err());
}

#[test]
fn spacing_quotes_indentation_and_numbers_are_canonical() {
    let source = "\
c=circle( 10 ,fill = '#FF0000' )
if c . radius==10 :
    c . radius=10.5000
x=1e3
";
    let formatted = format(source).unwrap();
    assert_eq!(
        formatted,
        "c = circle(10, fill=\"#FF0000\")\nif c.radius == 10:\n    c.radius = 10.5\nx = 1000\n"
    );
}

#[test]
fn long_numbers_keep_their_exact_value() {
    let source = "PI = 3.141592653589793\ny = 0.5000\nz = 1e-9\n";
    let formatted = format(source).unwrap();
    assert!(formatted.contains("PI = 3.141592653589793"), "{formatted}");
    assert!(formatted.contains("y = 0.5"), "{formatted}");
    assert!(formatted.contains("z = 1e-9"), "{formatted}");
}

#[test]
fn top_level_definitions_use_two_blank_lines() {
    let source = "def f():\n    return 1\ndef g():\n    return 2\nx = f()\n";
    let formatted = format(source).unwrap();
    assert_eq!(
        formatted,
        "def f():\n    return 1\n\n\ndef g():\n    return 2\n\n\nx = f()\n"
    );
    let commented = format("x = 1\n\n# helper\ndef f():\n    return 1\n").unwrap();
    assert_eq!(commented, "x = 1\n\n# helper\ndef f():\n    return 1\n");
}

#[test]
fn only_a_one_element_tuple_keeps_a_trailing_comma() {
    let formatted = format("x = (1,)\ny = [1,]\nf(1,)\n").unwrap();
    assert!(formatted.contains("x = (1,)"), "{formatted}");
    assert!(formatted.contains("y = [1]"), "{formatted}");
    assert!(formatted.contains("f(1)"), "{formatted}");
}

#[test]
fn crlf_input_produces_crlf_output() {
    let source = "c = circle(10)\r\nc.pos = (1, 2)\r\n";
    let formatted = format(source).unwrap();
    assert_eq!(formatted, "c = circle(10)\r\nc.pos = (1, 2)\r\n");
    assert_eq!(format(&formatted).unwrap(), formatted);
    assert!(!formatted.replace("\r\n", "").contains('\n'));
}

#[test]
fn format_check_rejects_non_canonical_input() {
    assert!(format_check("c = circle(10)\n").is_ok());
    let diagnostics = format_check("c=circle(10)\n").unwrap_err();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].span.line, 1);
    assert!(diagnostics[0].message.contains("canonical"));
}

#[test]
fn format_signature_collapses_a_multiline_def_header() {
    let signature = format_signature("def badge(\n    x,\n    fill = '#ffffff'\n):");
    assert_eq!(signature, "def badge(x, fill=\"#ffffff\"):");
    assert!(!signature.contains('\n'));
    assert_eq!(
        format_signature("badge(\n    i,\n    fill='#fff'\n)"),
        "badge(i, fill=\"#fff\")"
    );
}

#[test]
fn format_snippet_canonicalises_one_line_without_indentation() {
    assert_eq!(
        format_snippet("c . fill = '#FF0000'"),
        "c.fill = \"#FF0000\""
    );
    assert_eq!(
        format_snippet("circle( 10 , fill = '#fff' )"),
        "circle(10, fill=\"#fff\")"
    );
    assert_eq!(format_snippet("\n    c.pos = ( 1,2 )\n"), "c.pos = (1, 2)");
    assert_eq!(format_snippet(""), "");
}

fn normalize(mut program: Program) -> Program {
    for stmt in &mut program.body {
        clear_stmt(stmt);
    }
    program
}

fn clear_stmt(stmt: &mut Stmt) {
    stmt.span = Span::default();
    match &mut stmt.kind {
        StmtKind::Expr(expression) | StmtKind::Return(Some(expression)) => clear_expr(expression),
        StmtKind::Assign { target, value, .. } => {
            clear_target(target);
            clear_expr(value);
        }
        StmtKind::Def(def) => clear_def(Arc::make_mut(def)),
        StmtKind::If {
            branches,
            otherwise,
        } => {
            for (condition, body) in branches {
                clear_expr(condition);
                for stmt in body {
                    clear_stmt(stmt);
                }
            }
            for stmt in otherwise {
                clear_stmt(stmt);
            }
        }
        StmtKind::For { iterable, body, .. } => {
            clear_expr(iterable);
            for stmt in body {
                clear_stmt(stmt);
            }
        }
        StmtKind::Return(None) | StmtKind::Break | StmtKind::Continue | StmtKind::Pass => {}
    }
}

fn clear_def(def: &mut FunctionDef) {
    def.span = Span::default();
    for parameter in &mut def.params {
        if let Some(default) = &mut parameter.default {
            clear_expr(default);
        }
    }
    for stmt in &mut def.body {
        clear_stmt(stmt);
    }
}

fn clear_target(target: &mut Target) {
    match target {
        Target::Name(_) => {}
        Target::Attribute { object, .. } => clear_expr(object),
        Target::Index { object, index } => {
            clear_expr(object);
            clear_expr(index);
        }
    }
}

fn clear_expr(expression: &mut Expr) {
    expression.span = Span::default();
    match &mut expression.kind {
        ExprKind::List(items) | ExprKind::Tuple(items) => {
            for item in items {
                clear_expr(item);
            }
        }
        ExprKind::Unary { operand, .. } | ExprKind::Not(operand) => clear_expr(operand),
        ExprKind::Binary { left, right, .. }
        | ExprKind::Compare { left, right, .. }
        | ExprKind::Logical { left, right, .. } => {
            clear_expr(left);
            clear_expr(right);
        }
        ExprKind::Call { callee, args } => {
            clear_expr(callee);
            for argument in args {
                clear_expr(&mut argument.value);
            }
        }
        ExprKind::Attribute { object, .. } => clear_expr(object),
        ExprKind::Index { object, index } => {
            clear_expr(object);
            clear_expr(index);
        }
        ExprKind::Number(_)
        | ExprKind::Str(_)
        | ExprKind::Bool(_)
        | ExprKind::None
        | ExprKind::Name(_) => {}
    }
}

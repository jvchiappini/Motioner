//! Stack-safety tests for deeply nested expression syntax.
//!
//! Left-associative operator loops and postfix chains build deep trees
//! without recursing in the parser, so these tests exercise the
//! [`MAX_EXPR_DEPTH`] guard through the public API and verify that exactly
//! at the limit a program still parses, executes and drops on the
//! interpreter thread without overflowing.

use std::time::Instant;

use motioner_dsl::{execute, parse, run, Diagnostic, ErrorCategory, Limits, MAX_EXPR_DEPTH};

const HEADER: &str = "comp(name=\"deep\", width=16, height=16, fps=60, duration=1.0)\n";
const LARGE_STACK_BYTES: usize = 64 * 1024 * 1024;

fn on_large_stack<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(LARGE_STACK_BYTES)
        .spawn(work)
        .expect("could not spawn the test worker")
        .join()
        .expect("the test worker panicked")
}

fn flat_add(terms: u32) -> String {
    let mut source = String::with_capacity(4 * terms as usize);
    source.push('1');
    for _ in 1..terms {
        source.push_str(" + 1");
    }
    source
}

fn joined(term: &str, separator: &str, terms: u32) -> String {
    let mut source = String::new();
    for index in 0..terms {
        if index > 0 {
            source.push_str(separator);
        }
        source.push_str(term);
    }
    source
}

fn assert_depth_limit(error: &Diagnostic) {
    assert_eq!(error.category, ErrorCategory::Limit, "{error}");
    assert!(error.message.contains("too deeply nested"), "{error}");
    assert!(
        error.span.line >= 1 && error.span.column >= 1,
        "limit diagnostic must carry a real span: {error}"
    );
}

fn parses_on_large_stack(source: &str) -> bool {
    let source = source.to_owned();
    on_large_stack(move || parse(&source).is_ok())
}

fn execute_on_large_stack(source: &str) -> Result<(), Diagnostic> {
    let source = source.to_owned();
    on_large_stack(move || {
        let program = parse(&source).expect("the at-limit program should parse");
        execute(&program, &Limits::default()).map(|_| ())
    })
}

#[test]
fn flat_add_chain_at_the_limit_parses_and_runs() {
    let source = format!("{HEADER}x = {}\n", flat_add(MAX_EXPR_DEPTH));
    run(&source, &Limits::default())
        .unwrap_or_else(|error| panic!("{}", error.render(&source, "test")));
}

#[test]
fn flat_add_chain_past_the_limit_is_rejected() {
    let source = format!("x = {}\n", flat_add(MAX_EXPR_DEPTH + 1));
    let error = parse(&source).expect_err("one term past the limit must fail");
    assert_depth_limit(&error);
}

#[test]
fn logical_chains_at_the_limit_run() {
    let and_source = format!("{HEADER}x = {}\n", joined("1", " and ", MAX_EXPR_DEPTH));
    run(&and_source, &Limits::default())
        .unwrap_or_else(|error| panic!("{}", error.render(&and_source, "test")));

    let or_source = format!("{HEADER}x = {}\n", joined("0", " or ", MAX_EXPR_DEPTH));
    run(&or_source, &Limits::default())
        .unwrap_or_else(|error| panic!("{}", error.render(&or_source, "test")));
}

#[test]
fn logical_chains_past_the_limit_are_rejected() {
    let and_source = format!("x = {}\n", joined("1", " and ", MAX_EXPR_DEPTH + 1));
    assert_depth_limit(&parse(&and_source).expect_err("past the limit must fail"));

    let or_source = format!("x = {}\n", joined("0", " or ", MAX_EXPR_DEPTH + 1));
    assert_depth_limit(&parse(&or_source).expect_err("past the limit must fail"));
}

#[test]
fn long_chained_comparisons_are_rejected_quickly() {
    let source = format!("x = {}\n", joined("1", " < ", 100_000));
    let error = parse(&source).expect_err("chained comparisons must be unsupported");
    assert_eq!(error.category, ErrorCategory::Unsupported, "{error}");
    assert!(error.message.contains("chained comparisons"), "{error}");
}

#[test]
fn comparisons_joined_by_and_respect_the_limit() {
    // Each comparison has depth 2, so `k` comparisons joined by `and` have
    // depth `k + 1`; `k = MAX_EXPR_DEPTH - 1` sits exactly at the limit.
    let at_limit = format!(
        "{HEADER}x = {}\n",
        joined("1 < 2", " and ", MAX_EXPR_DEPTH - 1)
    );
    run(&at_limit, &Limits::default())
        .unwrap_or_else(|error| panic!("{}", error.render(&at_limit, "test")));

    let past_limit = format!("x = {}\n", joined("1 < 2", " and ", MAX_EXPR_DEPTH));
    assert_depth_limit(&parse(&past_limit).expect_err("past the limit must fail"));
}

fn postfix_forms(links: u32) -> [(&'static str, String); 3] {
    [
        ("call", format!("f{}", "()".repeat(links as usize))),
        ("attribute", format!("a{}", ".b".repeat(links as usize))),
        ("index", format!("x{}", "[0]".repeat(links as usize))),
    ]
}

#[test]
fn postfix_chains_at_the_limit_parse_and_fail_cleanly() {
    // `links` postfix operators add `links` nodes on top of the depth-1
    // primary, so this is exactly `MAX_EXPR_DEPTH`.
    for (label, expression) in postfix_forms(MAX_EXPR_DEPTH - 1) {
        let source = format!("{HEADER}y = {expression}\n");
        assert!(parses_on_large_stack(&source), "{label} chain must parse");
        let error = execute_on_large_stack(&source)
            .expect_err("the undefined base name must be a semantic error");
        assert_eq!(error.category, ErrorCategory::Name, "{label}: {error}");
    }
}

#[test]
fn postfix_chains_past_the_limit_are_rejected() {
    for (label, expression) in postfix_forms(MAX_EXPR_DEPTH) {
        let source = format!("y = {expression}\n");
        let error = parse(&source).expect_err("one postfix link past the limit must fail");
        assert_eq!(error.category, ErrorCategory::Limit, "{label}: {error}");
        assert!(
            error.message.contains("too deeply nested"),
            "{label}: {error}"
        );
        assert!(
            error.span.line >= 1 && error.span.column >= 1,
            "{label}: limit diagnostic must carry a real span: {error}"
        );
    }
}

#[test]
fn two_million_term_expression_is_rejected_without_crashing() {
    let source = format!("x = {}\n", flat_add(2_000_000));
    let started = Instant::now();
    let error = parse(&source).expect_err("the depth limit must reject this input");
    let elapsed = started.elapsed();
    assert_depth_limit(&error);
    if !cfg!(debug_assertions) {
        assert!(
            elapsed.as_secs() < 10,
            "rejecting two million terms took {elapsed:?}"
        );
    }
}

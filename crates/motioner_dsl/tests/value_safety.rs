//! Regression tests for hostile runtime values: NaN bounds, value cycles,
//! deep nesting, literal limits and overflow-safe builtins.
//!
//! Every test goes through the public `run` API, because `Value` is private.

use std::time::{Duration, Instant};

use motioner_dsl::{run, ErrorCategory, Limits};

const HEADER: &str = "comp(name=\"T\", width=200, height=100, fps=60, duration=2.0)\n";

fn source(body: &str) -> String {
    format!("{HEADER}{body}")
}

fn failing(body: &str, limits: &Limits) -> motioner_dsl::Diagnostic {
    let source = source(body);
    run(&source, limits).expect_err("program should fail")
}

/// Runs `body` and returns the static x position of the first node.
fn first_x(body: &str) -> f64 {
    let source = source(body);
    let document = run(&source, &Limits::default())
        .unwrap_or_else(|e| panic!("{}", e.render(&source, "test")));
    let first = document.nodes().next().expect("a node");
    first.transform().position.value().x
}

#[test]
fn clamp_rejects_nan_bounds_without_panicking() {
    let nan = "(1e308 * 1e308) - (1e308 * 1e308)";
    for body in [
        format!("x = clamp(0, {nan}, 1)\n"),
        format!("x = clamp(0, 0, {nan})\n"),
        format!("x = clamp(0, {nan}, {nan})\n"),
    ] {
        let error = failing(&body, &Limits::default());
        assert_eq!(error.category, ErrorCategory::Value, "{body}: {error}");
        assert!(error.message.contains("NaN"), "{body}: {error}");
    }
}

#[test]
fn indexed_assignment_rejects_cycles() {
    for body in [
        // Direct self-reference.
        "a = [0]\na[0] = a\n",
        // Indirect through a tuple and another list.
        "a = [0]\nb = (a,)\nc = [b]\na[0] = c\n",
        // Augmented assignment builds the same cycle.
        "a = [[0]]\na[0] += [a]\n",
    ] {
        let error = failing(body, &Limits::default());
        assert_eq!(error.category, ErrorCategory::Value, "{body}: {error}");
        assert_eq!(
            error.message, "a list cannot contain itself",
            "{body}: {error}"
        );
    }
}

#[test]
fn million_deep_list_builds_and_drops_iteratively() {
    let source = source("l = [0]\nfor i in range(1000000):\n    l = [l]\n");
    let document = run(&source, &Limits::default())
        .unwrap_or_else(|e| panic!("{}", e.render(&source, "test")));
    assert_eq!(document.len(), 0);
}

#[test]
fn million_deep_tuple_builds_and_drops_iteratively() {
    let source = source("t = (0,)\nfor i in range(1000000):\n    t = (t,)\n");
    let document = run(&source, &Limits::default())
        .unwrap_or_else(|e| panic!("{}", e.render(&source, "test")));
    assert_eq!(document.len(), 0);
}

#[test]
fn deep_values_fail_display_and_equality_with_a_limit() {
    let limits = Limits::default();

    let display = "l = [0]\nfor i in range(200):\n    l = [l]\ns = str(l)\n";
    let error = failing(display, &limits);
    assert_eq!(error.category, ErrorCategory::Limit, "{error}");
    assert!(error.message.contains("nesting limit"), "{error}");

    let equality = "\
a = [0]
b = [0]
for i in range(200):
    a = [a]
    b = [b]
x = a == b
";
    let error = failing(equality, &limits);
    assert_eq!(error.category, ErrorCategory::Limit, "{error}");
    assert!(error.message.contains("nesting limit"), "{error}");

    // Identity short-circuits structural comparison, but the depth limit is
    // still enforced before it, so a deep value compared with itself fails.
    let identity = "l = [0]\nfor i in range(200):\n    l = [l]\nx = l == l\n";
    let error = failing(identity, &limits);
    assert_eq!(error.category, ErrorCategory::Limit, "{error}");
    assert!(error.message.contains("nesting limit"), "{error}");

    // Exactly `max_value_depth` containers are allowed; one more is not.
    let exact = "l = [0]\nfor i in range(99):\n    l = [l]\ns = str(l)\n";
    run(&source(exact), &limits).expect("depth exactly at the limit is allowed");
    let too_deep = "l = [0]\nfor i in range(100):\n    l = [l]\ns = str(l)\n";
    let error = failing(too_deep, &limits);
    assert_eq!(error.category, ErrorCategory::Limit, "{error}");
}

#[test]
fn string_iteration_respects_the_list_limit() {
    let limits = Limits {
        max_list_len: 3,
        ..Limits::default()
    };
    let error = failing("for c in \"abcd\":\n    x = c\n", &limits);
    assert_eq!(error.category, ErrorCategory::Limit, "{error}");
    assert!(error.message.contains("list length"), "{error}");
}

#[test]
fn concatenation_respects_the_list_limit() {
    let limits = Limits {
        max_list_len: 5,
        ..Limits::default()
    };
    for body in [
        "a = [0, 0, 0, 0]\nb = [0, 0, 0, 0]\nc = a + b\n",
        "a = (0, 0, 0, 0)\nb = (0, 0, 0, 0)\nc = a + b\n",
    ] {
        let error = failing(body, &limits);
        assert_eq!(error.category, ErrorCategory::Limit, "{body}: {error}");
        assert!(error.message.contains("list length"), "{body}: {error}");
    }
}

#[test]
fn literals_respect_string_and_list_limits() {
    let limits = Limits {
        max_string_len: 3,
        max_list_len: 2,
        ..Limits::default()
    };
    let error = failing("s = \"abcdef\"\n", &limits);
    assert_eq!(error.category, ErrorCategory::Limit, "{error}");
    assert!(error.message.contains("string length"), "{error}");

    let error = failing("t = (1, 2, 3)\n", &limits);
    assert_eq!(error.category, ErrorCategory::Limit, "{error}");
    assert!(error.message.contains("list length"), "{error}");
}

#[test]
fn shared_values_cannot_blow_up_display() {
    let body = "a = [0]\nfor i in range(64):\n    a = [a, a]\ns = str(a)\n";
    let limits = Limits::default();
    let started = Instant::now();
    let error = failing(body, &limits);
    let elapsed = started.elapsed();
    assert_eq!(error.category, ErrorCategory::Limit, "{error}");
    assert!(
        error.message.contains("step limit") || error.message.contains("string length"),
        "{error}"
    );
    assert!(elapsed < Duration::from_secs(2), "display took {elapsed:?}");
}

#[test]
fn shared_values_cannot_blow_up_equality() {
    let body = "\
a = [0]
b = [0]
for i in range(64):
    a = [a, a]
    b = [b, b]
x = a == b
";
    let limits = Limits::default();
    let started = Instant::now();
    let error = failing(body, &limits);
    let elapsed = started.elapsed();
    assert_eq!(error.category, ErrorCategory::Limit, "{error}");
    assert!(error.message.contains("step limit"), "{error}");
    assert!(
        elapsed < Duration::from_secs(2),
        "equality took {elapsed:?}"
    );
}

#[test]
fn repeated_bulk_operations_hit_the_step_limit() {
    let zeros = vec!["0"; 1000].join(", ");
    let body = format!("l = [{zeros}]\nfor i in range(1000):\n    x = l == l\n");
    let limits = Limits {
        max_steps: 100_000,
        ..Limits::default()
    };
    let error = failing(&body, &limits);
    assert_eq!(error.category, ErrorCategory::Limit, "{error}");
    assert!(
        error
            .message
            .contains("execution step limit of 100000 exceeded"),
        "{error}"
    );
}

#[test]
fn step_limit_counts_concatenation_and_iteration() {
    let concat_limits = Limits {
        max_steps: 12,
        ..Limits::default()
    };
    let error = failing(
        "a = [0, 0, 0, 0]\nb = [0, 0, 0, 0]\nc = a + b\n",
        &concat_limits,
    );
    assert_eq!(error.category, ErrorCategory::Limit, "{error}");
    assert!(error.message.contains("step limit"), "{error}");

    let word = "x".repeat(100);
    let body = format!("for c in \"{word}\":\n    pass\n");
    let iteration_limits = Limits {
        max_steps: 150,
        ..Limits::default()
    };
    let error = failing(&body, &iteration_limits);
    assert_eq!(error.category, ErrorCategory::Limit, "{error}");
    assert!(error.message.contains("step limit"), "{error}");
}

// Exact comparison is intended: 100 and 0 are exactly representable.
#[allow(clippy::float_cmp)]
#[test]
fn round_overflow_returns_the_input_unchanged() {
    let x = first_x("n = rect(1, 1)\nn.pos = (round(100, 308) + round(1.5, -308), 0)\n");
    assert_eq!(x, 100.0);
}

#[test]
fn string_comparison_and_parsing_are_charged() {
    let limits = Limits {
        max_steps: 10_000,
        ..Limits::default()
    };
    // A 65,536-byte string costs about 1,024 steps per full scan, so twenty
    // scans exceed the budget; without byte charging they cost 40 steps.
    let build = "s = \"0\"\nfor i in range(16):\n    s = s + s\nt = s + \"\"\n";
    for scan in ["x = s == t", "x = s < t", "x = float(s)"] {
        let body = format!("{build}for i in range(20):\n    {scan}\n");
        let error = failing(&body, &limits);
        assert_eq!(error.category, ErrorCategory::Limit, "{scan}: {error}");
        assert!(error.message.contains("step limit"), "{scan}: {error}");
    }
}

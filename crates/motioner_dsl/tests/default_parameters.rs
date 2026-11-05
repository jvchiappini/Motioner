//! Default parameter values in MDF v0 (prompt 12 phase 4).

use motioner_core::NodeContent;
use motioner_dsl::{parse, run, Limits};

fn widths(source: &str) -> Vec<f64> {
    run(source, &Limits::default())
        .unwrap()
        .nodes()
        .map(|node| match node.content() {
            NodeContent::Rect { width, .. } => width.value(),
            other => panic!("expected a rect, got {other:?}"),
        })
        .collect()
}

#[test]
fn a_missing_defaulted_parameter_uses_its_default() {
    let source = r#"comp(name="D", duration=1.0)
def bar(width, extra=5):
    return rect(width + extra, 1)
a = bar(10)
b = bar(10, extra=2)
"#;
    assert_eq!(widths(source), vec![15.0, 12.0]);
}

#[test]
fn a_default_expression_is_evaluated_at_the_call() {
    let source = r#"comp(name="D", duration=1.0)
pad = 4
def bar(width, extra=pad):
    return rect(width + extra, 1)
a = bar(10)
pad = 1
b = bar(10)
"#;
    assert_eq!(widths(source), vec![14.0, 11.0]);
}

#[test]
fn a_required_parameter_cannot_follow_a_default() {
    let error = parse("def f(a=1, b):\n    pass\n").unwrap_err();
    assert!(error.message.contains("default"), "{}", error.message);
}

#[test]
fn a_missing_required_parameter_is_still_an_error() {
    let error = run(
        "comp(name=\"D\", duration=1.0)\ndef f(a, b):\n    return rect(a, b)\nf(1)\n",
        &Limits::default(),
    )
    .unwrap_err();
    assert!(
        error.message.contains("missing required"),
        "{}",
        error.message
    );
}

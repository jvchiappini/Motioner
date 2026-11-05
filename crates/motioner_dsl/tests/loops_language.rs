//! MDF v0 language additions needed by the loop guards (prompt 12 phase 3):
//! membership, `enumerate` and tuple unpacking in `for`.

use motioner_core::NodeContent;
use motioner_dsl::{run, Limits};

fn rect_widths(source: &str) -> Vec<f64> {
    let document = run(source, &Limits::default()).unwrap();
    document
        .nodes()
        .map(|node| match node.content() {
            NodeContent::Rect { width, .. } => width.value(),
            other => panic!("expected a rect, got {other:?}"),
        })
        .collect()
}

#[test]
fn membership_works_on_lists_tuples_and_ranges() {
    let source = r#"comp(name="M", duration=1.0)
for i in [1, 3, 5]:
    if i in (1, 3):
        r = rect(i, 1)
"#;
    assert_eq!(rect_widths(source), vec![1.0, 3.0]);
    let source = r#"comp(name="M", duration=1.0)
for i in range(4):
    if i not in (1, 3):
        r = rect(i, 1)
"#;
    assert_eq!(rect_widths(source), vec![0.0, 2.0]);
}

#[test]
fn enumerate_unpacks_the_index_and_the_value() {
    let source = r#"comp(name="E", duration=1.0)
for i, name in enumerate(["a", "b"]):
    if i == 1:
        r = rect(len(name), 1)
"#;
    assert_eq!(rect_widths(source), vec![1.0]);
    let source = r#"comp(name="E", duration=1.0)
for i, name in enumerate(["a", "b"], start=10):
    if i == 11:
        r = rect(len(name), 1)
"#;
    assert_eq!(rect_widths(source), vec![1.0]);
}

#[test]
fn nested_guards_combine_with_and() {
    let source = r#"comp(name="N", duration=1.0)
for row in range(2):
    for col in range(3):
        if row == 1 and col == 2:
            r = rect(row, col)
"#;
    assert_eq!(rect_widths(source), vec![1.0]);
}

#[test]
fn tuple_unpacking_rejects_a_wrong_length() {
    let error = run(
        "comp(name=\"T\", duration=1.0)\nfor a, b in [(1, 2, 3)]:\n    pass\n",
        &Limits::default(),
    )
    .unwrap_err();
    assert!(error.message.contains("expected 2 values"), "{error}");
}

#[test]
fn a_guarded_assignment_is_the_document_value() {
    let source = r#"comp(name="G", duration=1.0)
for i in range(3):
    c = circle(10)
    if i == 1:
        c.radius = 20
"#;
    let document = run(source, &Limits::default()).unwrap();
    let radii: Vec<f64> = document
        .nodes()
        .map(|node| match node.content() {
            NodeContent::Circle { radius, .. } => radius.value(),
            other => panic!("expected a circle, got {other:?}"),
        })
        .collect();
    assert_eq!(radii, vec![10.0, 20.0, 10.0]);
}

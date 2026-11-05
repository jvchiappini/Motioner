//! The write-through planner: private literals, local assignments and refusals.

use motioner_core::{Ease, EditCommand, OverrideValue};
use motioner_dsl::{execute_detailed, parse, plan, verify, Execution, Limits, Plan, Strategy};

fn compiled(source: &str) -> Execution {
    execute_detailed(&parse(source).unwrap(), &Limits::default()).unwrap()
}

fn node_id(execution: &Execution, index: usize) -> String {
    execution
        .document
        .nodes()
        .nth(index)
        .unwrap()
        .identity()
        .as_str()
        .to_owned()
}

fn set_value(
    execution: &Execution,
    index: usize,
    property: &str,
    value: OverrideValue,
) -> EditCommand {
    EditCommand::SetValue {
        id: node_id(execution, index),
        property: property.to_owned(),
        value,
    }
}

const COMPUTED_RADIUS: &str = r##"comp(name="P", duration=1.0)
r = 40
c = circle(r, fill="#ffffff")
"##;

#[test]
fn a_private_literal_is_edited_in_place() {
    let source = r##"comp(name="P", duration=1.0)
c = circle(40, fill="#ffffff")
"##;
    let execution = compiled(source);
    let plan = plan(
        source,
        &execution.origins,
        &execution.document,
        &set_value(
            &execution,
            0,
            "fill",
            OverrideValue::Text("#ff4d4d".to_owned()),
        ),
    );
    let edit = plan.code().expect("a private literal is exact");
    assert_eq!(edit.strategy(), Strategy::PrivateLiteral);
    let after = edit.apply(source).unwrap();
    assert!(after.contains("#ff4d4d"));
    assert!(!after.contains("circle(40, fill=\"#ffffff\")"));
}

#[test]
fn a_computed_value_becomes_a_local_assignment_after_the_creation() {
    let execution = compiled(COMPUTED_RADIUS);
    let plan = plan(
        COMPUTED_RADIUS,
        &execution.origins,
        &execution.document,
        &set_value(&execution, 0, "radius", OverrideValue::Number(80.0)),
    );
    assert!(plan.code().is_some(), "computed radius: {plan:?}");
    let edit = plan.code().unwrap();
    assert_eq!(edit.strategy(), Strategy::LocalAssignment);
    let after = edit.apply(COMPUTED_RADIUS).unwrap();
    assert!(
        after.contains("\nc.radius = 80\n"),
        "the assignment goes after the node, not in a trailing block: {after}"
    );
    assert!(!after.contains("baked by motioner"));
    let reparsed = motioner_dsl::run(&after, &Limits::default()).unwrap();
    let id = node_id(&execution, 0);
    let node_id = reparsed.find(&id).unwrap();
    let node = reparsed.node(node_id).unwrap();
    assert!(matches!(
        node.content()
            .static_value(motioner_core::ContentField::Radius),
        Some(motioner_core::ContentValue::Number(80.0))
    ));
}

#[test]
fn a_shared_literal_is_replaced_by_a_local_assignment() {
    let source = r##"comp(name="P", duration=1.0)
colour = "#ffffff"
a = circle(10, fill=colour)
b = circle(10, fill=colour)
"##;
    let execution = compiled(source);
    let plan = plan(
        source,
        &execution.origins,
        &execution.document,
        &set_value(
            &execution,
            0,
            "fill",
            OverrideValue::Text("#ff4d4d".to_owned()),
        ),
    );
    assert!(plan.code().is_some(), "shared fill: {plan:?}");
    let edit = plan.code().unwrap();
    assert_eq!(edit.strategy(), Strategy::LocalAssignment);
    let after = edit.apply(source).unwrap();
    assert!(after.contains("a.fill = \"#ff4d4d\""), "{after}");
    assert!(!after.contains("b.fill"), "the other node stays untouched");
    let reparsed = motioner_dsl::run(&after, &Limits::default()).unwrap();
    let fill = |index: usize| {
        let node = reparsed.nodes().nth(index).unwrap();
        node.content()
            .static_value(motioner_core::ContentField::Fill)
    };
    assert_ne!(fill(0), fill(1));
}

#[test]
fn an_existing_assignment_is_edited_instead_of_duplicated() {
    let source = r#"comp(name="P", duration=1.0)
o = 0.5
c = circle(10)
c.opacity = o
"#;
    let execution = compiled(source);
    let plan = plan(
        source,
        &execution.origins,
        &execution.document,
        &set_value(&execution, 0, "opacity", OverrideValue::Number(0.9)),
    );
    let edit = plan.code().expect("the existing assignment is exact");
    let after = edit.apply(source).unwrap();
    assert_eq!(after.matches("c.opacity =").count(), 1, "{after}");
    assert!(after.contains("c.opacity = 0.9"));
}

#[test]
fn insertion_follows_the_block_indentation_and_crlf() {
    let source = "comp(name=\"P\", duration=1.0)\r\nr = 40\r\nif True:\r\n    c = circle(r)\r\n";
    let execution = compiled(source);
    let plan = plan(
        source,
        &execution.origins,
        &execution.document,
        &set_value(&execution, 0, "radius", OverrideValue::Number(80.0)),
    );
    assert!(plan.code().is_some(), "indent radius: {plan:?}");
    let edit = plan.code().unwrap();
    let after = edit.apply(source).unwrap();
    assert!(after.contains("\r\n    c.radius = 80\r\n"), "{after:?}");
}

#[test]
fn a_computed_key_value_is_edited_in_place() {
    let source = r#"comp(name="P", duration=1.0)
o = 0.0
c = circle(10)
c.key("opacity", 0.0, o)
"#;
    let execution = compiled(source);
    let plan = plan(
        source,
        &execution.origins,
        &execution.document,
        &EditCommand::SetKey {
            id: node_id(&execution, 0),
            property: "opacity".to_owned(),
            axis: None,
            frame: 0,
            value: OverrideValue::Number(0.5),
            ease: Ease::Linear,
        },
    );
    let edit = plan.code().expect("a bound computed key is exact");
    assert_eq!(edit.strategy(), Strategy::LocalAssignment);
    let after = edit.apply(source).unwrap();
    assert!(after.contains("c.key(\"opacity\", 0.0, 0.5)"), "{after}");
}

#[test]
fn a_missing_key_is_inserted_after_the_node() {
    let source = r#"comp(name="P", duration=1.0)
c = circle(10)
"#;
    let execution = compiled(source);
    let plan = plan(
        source,
        &execution.origins,
        &execution.document,
        &EditCommand::SetKey {
            id: node_id(&execution, 0),
            property: "opacity".to_owned(),
            axis: None,
            frame: 0,
            value: OverrideValue::Number(0.5),
            ease: Ease::Linear,
        },
    );
    let edit = plan.code().expect("a key on a bound node is exact");
    assert_eq!(edit.strategy(), Strategy::LocalAssignment);
    let after = edit.apply(source).unwrap();
    assert!(after.contains("\nc.key(\"opacity\", 0, 0.5)\n"), "{after}");
}

#[test]
fn a_loop_created_node_gets_a_guard() {
    let source = r#"comp(name="P", duration=1.0)
for i in range(2):
    c = circle(40)
"#;
    let execution = compiled(source);
    let plan = plan(
        source,
        &execution.origins,
        &execution.document,
        &set_value(&execution, 0, "radius", OverrideValue::Number(80.0)),
    );
    let edit = plan.code().expect("a loop instance is exact");
    assert_eq!(edit.strategy(), Strategy::LoopGuard);
    let after = edit.apply(source).unwrap();
    assert!(
        after.contains("    if i == 0:\n        c.radius = 80\n"),
        "{after}"
    );
}

#[test]
fn unsupported_commands_fall_back_with_a_reason() {
    let source = r#"comp(name="P", duration=1.0)
c = circle(40)
"#;
    let execution = compiled(source);
    let plan = plan(
        source,
        &execution.origins,
        &execution.document,
        &EditCommand::SetName {
            id: node_id(&execution, 0),
            name: "Title".to_owned(),
        },
    );
    assert!(
        plan.reason()
            .is_some_and(|reason| reason.contains("does not handle")),
        "{plan:?}"
    );
}

#[test]
fn a_code_plan_verifies_against_the_expected_document() {
    let source = r#"comp(name="P", duration=1.0)
c = circle(40)
"#;
    let execution = compiled(source);
    let plan = plan(
        source,
        &execution.origins,
        &execution.document,
        &set_value(&execution, 0, "radius", OverrideValue::Number(80.0)),
    );
    let Plan::Code(edit) = plan else {
        panic!("expected a code plan, got {plan:?}");
    };
    let after = edit.apply(source).unwrap();
    // The unchanged program verifies against its own document.
    assert!(verify(source, None, &execution.document));
    // The edited program does not verify against the old document.
    assert!(!verify(&after, None, &execution.document));
}

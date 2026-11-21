//! Loop specialisation: guards, `in` merges, enumerate rewrites, scope.

use motioner_core::{Ease, EditCommand, OverrideValue};
use motioner_dsl::{
    execute_detailed, parse, plan, plan_with_scope, verify, Execution, Limits, Plan, Scope,
    Strategy,
};

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

fn applied(source: &str, plan: Plan) -> (String, Strategy) {
    let Plan::Code(edit) = plan else {
        panic!("expected a code plan, got {plan:?}");
    };
    (edit.apply(source).unwrap(), edit.strategy())
}

fn assert_verified(after: &str, execution: &Execution, change: &EditCommand) {
    let mut expected = execution.document.clone();
    let mut set = motioner_core::OverrideSet::new();
    if let Some(entry) = entry_of(change) {
        set.insert(entry);
    }
    assert!(set.apply(&mut expected).is_empty());
    assert!(
        verify(after, None, &expected),
        "the edited program reproduces the document:\n{after}"
    );
}

fn entry_of(change: &EditCommand) -> Option<motioner_core::OverrideEntry> {
    match change {
        EditCommand::SetValue {
            id,
            property,
            value,
        } => Some(motioner_core::OverrideEntry::new(
            id.clone(),
            property.clone(),
            motioner_core::OverrideAction::Set(value.clone()),
        )),
        EditCommand::SetKey {
            id,
            property,
            frame,
            value,
            ease,
            ..
        } => Some(motioner_core::OverrideEntry::new(
            id.clone(),
            property.clone(),
            motioner_core::OverrideAction::Key {
                frame: *frame,
                value: value.clone(),
                ease: *ease,
            },
        )),
        _ => None,
    }
}

const LOOP: &str = r##"comp(name="P", duration=1.0)
for i in range(5):
    c = circle(40, fill="#ffffff")
    c.pos = (200 + i * 120, 540)
"##;

#[test]
fn a_loop_instance_gets_a_guarded_assignment_after_its_last_write() {
    let execution = compiled(LOOP);
    let change = set_value(
        &execution,
        3,
        "fill",
        OverrideValue::Text("#ff4d4d".to_owned()),
    );
    let (after, strategy) = applied(
        LOOP,
        plan(LOOP, &execution.origins, &execution.document, &change),
    );
    assert_eq!(strategy, Strategy::LoopGuard);
    assert!(
        after.contains(
            "    c.pos = (200 + i * 120, 540)\n    if i == 3:\n        c.fill = \"#ff4d4d\"\n"
        ),
        "{after}"
    );
    assert_verified(&after, &execution, &change);
}

#[test]
fn nested_loops_combine_their_guards() {
    let source = "comp(name=\"P\", duration=1.0)\n\
                  for row in range(2):\n\
                  \x20   for col in range(3):\n\
                  \x20       d = circle(8)\n";
    let execution = compiled(source);
    let change = set_value(&execution, 5, "radius", OverrideValue::Number(16.0));
    let (after, _) = applied(
        source,
        plan(source, &execution.origins, &execution.document, &change),
    );
    assert!(
        after.contains("        if row == 1 and col == 2:\n            d.radius = 16\n"),
        "{after}"
    );
    assert_verified(&after, &execution, &change);
}

#[test]
fn an_existing_guard_with_the_same_condition_is_extended() {
    let source = "comp(name=\"P\", duration=1.0)\n\
                  for i in range(4):\n\
                  \x20   c = circle(10)\n\
                  \x20   c.opacity = 0.5\n\
                  \x20   if i == 2:\n\
                  \x20       c.opacity = 0.9\n";
    let execution = compiled(source);
    let change = set_value(&execution, 2, "radius", OverrideValue::Number(20.0));
    let (after, _) = applied(
        source,
        plan(source, &execution.origins, &execution.document, &change),
    );
    assert_eq!(after.matches("if i == 2:").count(), 1, "{after}");
    assert!(
        after.contains("        c.opacity = 0.9\n        c.radius = 20\n"),
        "{after}"
    );
    assert_verified(&after, &execution, &change);
}

#[test]
fn the_same_value_on_two_iterations_merges_into_an_in_tuple() {
    let source = "comp(name=\"P\", duration=1.0)\n\
                  for i in range(5):\n\
                  \x20   c = circle(10)\n\
                  \x20   c.opacity = 0.5\n\
                  \x20   if i == 1:\n\
                  \x20       c.opacity = 0.9\n";
    let execution = compiled(source);
    let change = set_value(&execution, 3, "opacity", OverrideValue::Number(0.9));
    let (after, _) = applied(
        source,
        plan(source, &execution.origins, &execution.document, &change),
    );
    assert!(after.contains("if i in (1, 3):"), "{after}");
    assert_eq!(after.matches("c.opacity = 0.9").count(), 1, "{after}");
    assert_verified(&after, &execution, &change);
}

#[test]
fn an_unused_loop_variable_is_renamed() {
    let source = "comp(name=\"P\", duration=1.0)\nfor _ in range(3):\n    c = circle(10)\n";
    let execution = compiled(source);
    let change = set_value(&execution, 1, "radius", OverrideValue::Number(20.0));
    let (after, _) = applied(
        source,
        plan(source, &execution.origins, &execution.document, &change),
    );
    assert!(after.contains("for i in range(3):"), "{after}");
    assert!(
        after.contains("if i == 1:\n        c.radius = 20"),
        "{after}"
    );
    assert_verified(&after, &execution, &change);
}

#[test]
fn a_unique_string_value_guards_on_the_value() {
    let source = "comp(name=\"P\", duration=1.0)\n\
                  for name in [\"a\", \"b\", \"c\"]:\n\
                  \x20   c = circle(10)\n";
    let execution = compiled(source);
    let change = set_value(&execution, 1, "radius", OverrideValue::Number(20.0));
    let (after, _) = applied(
        source,
        plan(source, &execution.origins, &execution.document, &change),
    );
    assert!(after.contains("if name == \"b\":"), "{after}");
    assert_verified(&after, &execution, &change);
}

#[test]
fn a_repeated_string_value_is_rewritten_to_enumerate() {
    let source = "comp(name=\"P\", duration=1.0)\n\
                  for name in [\"a\", \"b\", \"a\"]:\n\
                  \x20   c = circle(10)\n";
    let execution = compiled(source);
    let change = set_value(&execution, 0, "radius", OverrideValue::Number(20.0));
    let (after, _) = applied(
        source,
        plan(source, &execution.origins, &execution.document, &change),
    );
    assert!(
        after.contains("for i, name in enumerate([\"a\", \"b\", \"a\"]):"),
        "{after}"
    );
    assert!(after.contains("if i == 0:"), "{after}");
    assert_verified(&after, &execution, &change);
}

#[test]
fn an_inline_node_gets_a_readable_binding() {
    let source = "comp(name=\"P\", duration=1.0)\nfor i in range(3):\n    circle(10)\n";
    let execution = compiled(source);
    let change = set_value(&execution, 1, "radius", OverrideValue::Number(20.0));
    let (after, _) = applied(
        source,
        plan(source, &execution.origins, &execution.document, &change),
    );
    assert!(after.contains("circle_2 = circle(10)"), "{after}");
    assert!(
        after.contains("if i == 1:\n        circle_2.radius = 20"),
        "{after}"
    );
    assert_verified(&after, &execution, &change);
}

#[test]
fn a_loop_key_is_guarded() {
    let source = "comp(name=\"P\", duration=1.0)\nfor i in range(3):\n    c = circle(10)\n";
    let execution = compiled(source);
    let change = EditCommand::SetKey {
        id: node_id(&execution, 1),
        property: "opacity".to_owned(),
        axis: None,
        frame: 0,
        value: OverrideValue::Number(0.5),
        ease: Ease::Linear,
    };
    let (after, _) = applied(
        source,
        plan(source, &execution.origins, &execution.document, &change),
    );
    assert!(
        after.contains("if i == 1:\n        c.key(\"opacity\", 0, 0.5)"),
        "{after}"
    );
    assert_verified(&after, &execution, &change);
}

#[test]
fn all_scope_edits_the_shared_literal_without_a_guard() {
    let execution = compiled(LOOP);
    let change = set_value(
        &execution,
        3,
        "fill",
        OverrideValue::Text("#ff4d4d".to_owned()),
    );
    let (after, strategy) = applied(
        LOOP,
        plan_with_scope(
            LOOP,
            &execution.origins,
            &execution.document,
            &change,
            Scope::All,
        ),
    );
    assert_eq!(strategy, Strategy::LoopSharedValue);
    assert!(after.contains("fill=\"#ff4d4d\""), "{after}");
    assert!(!after.contains("if i =="), "{after}");
}

#[test]
fn a_redundant_guard_is_removed_when_the_value_returns_to_the_shared_one() {
    let source = "comp(name=\"P\", duration=1.0)\n\
                  for i in range(3):\n\
                  \x20   c = circle(10)\n\
                  \x20   if i == 1:\n\
                  \x20       c.radius = 20\n";
    let execution = compiled(source);
    let change = set_value(&execution, 1, "radius", OverrideValue::Number(10.0));
    let (after, _) = applied(
        source,
        plan(source, &execution.origins, &execution.document, &change),
    );
    assert!(!after.contains("if i == 1:"), "{after}");
    assert!(!after.contains("c.radius = 20"), "{after}");
    assert_verified(&after, &execution, &change);
}

#[test]
fn all_scope_removes_the_guard_it_makes_redundant() {
    let source = "comp(name=\"P\", duration=1.0)\n\
                  for i in range(3):\n\
                  \x20   c = circle(10)\n\
                  \x20   if i == 1:\n\
                  \x20       c.radius = 20\n";
    let execution = compiled(source);
    let change = set_value(&execution, 1, "radius", OverrideValue::Number(30.0));
    let (after, strategy) = applied(
        source,
        plan_with_scope(
            source,
            &execution.origins,
            &execution.document,
            &change,
            Scope::All,
        ),
    );
    assert_eq!(strategy, Strategy::LoopSharedValue);
    assert!(after.contains("circle(30)"), "{after}");
    assert!(!after.contains("if i == 1:"), "{after}");
    let actual = motioner_dsl::run(&after, &Limits::default()).unwrap();
    let radii: Vec<f64> = actual
        .nodes()
        .map(|node| match node.content() {
            motioner_core::NodeContent::Circle { radius, .. } => radius.value(),
            other => panic!("expected a circle, got {other:?}"),
        })
        .collect();
    assert_eq!(radii, vec![30.0, 30.0, 30.0]);
}

#[test]
fn the_last_iteration_edit_rewrites_the_shared_literal() {
    let source = "comp(name=\"P\", duration=1.0)\n\
                  for i in range(4):\n\
                  \x20   c = circle(10)\n\
                  \x20   if i == 0:\n\
                  \x20       c.radius = 20\n\
                  \x20   if i == 1:\n\
                  \x20       c.radius = 20\n\
                  \x20   if i == 2:\n\
                  \x20       c.radius = 20\n";
    let execution = compiled(source);
    let change = set_value(&execution, 3, "radius", OverrideValue::Number(20.0));
    let (after, strategy) = applied(
        source,
        plan(source, &execution.origins, &execution.document, &change),
    );
    assert_eq!(strategy, Strategy::LoopSharedValue);
    assert!(after.contains("circle(20)"), "{after}");
    assert!(!after.contains("if i =="), "{after}");
    let actual = motioner_dsl::run(&after, &Limits::default()).unwrap();
    let radii: Vec<f64> = actual
        .nodes()
        .map(|node| match node.content() {
            motioner_core::NodeContent::Circle { radius, .. } => radius.value(),
            other => panic!("expected a circle, got {other:?}"),
        })
        .collect();
    assert_eq!(radii, vec![20.0, 20.0, 20.0, 20.0]);
}

#[test]
fn a_loop_guard_keeps_the_file_line_endings_and_indentation() {
    let source = "comp(name=\"P\", duration=1.0)\r\nfor i in range(2):\r\n    c = circle(10)\r\n";
    let execution = compiled(source);
    let change = set_value(&execution, 1, "radius", OverrideValue::Number(20.0));
    let (after, _) = applied(
        source,
        plan(source, &execution.origins, &execution.document, &change),
    );
    assert!(
        after.contains("\r\n    if i == 1:\r\n        c.radius = 20\r\n"),
        "{after:?}"
    );
    assert_verified(&after, &execution, &change);
}

#[test]
fn merging_conditions_keeps_the_tuple_sorted() {
    let source = "comp(name=\"P\", duration=1.0)\n\
                  for i in range(5):\n\
                  \x20   c = circle(10)\n\
                  \x20   c.opacity = 0.5\n\
                  \x20   if i == 3:\n\
                  \x20       c.opacity = 0.9\n";
    let execution = compiled(source);
    let change = set_value(&execution, 1, "opacity", OverrideValue::Number(0.9));
    let (after, _) = applied(
        source,
        plan(source, &execution.origins, &execution.document, &change),
    );
    assert!(after.contains("if i in (1, 3):"), "{after}");
    assert_verified(&after, &execution, &change);
}

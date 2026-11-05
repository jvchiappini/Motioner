//! Call-site specialisation: parameters, returned nodes, literal promotion,
//! loop guards around calls and the refusals (prompt 12 phase 4).

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

fn applied(source: &str, plan: Plan) -> (String, Strategy) {
    let Plan::Code(edit) = plan else {
        panic!("expected a code plan, got {plan:?}");
    };
    (edit.apply(source).unwrap(), edit.strategy())
}

fn assert_verified(after: &str, execution: &Execution, change: &EditCommand) {
    let mut expected = execution.document.clone();
    let mut set = motioner_core::OverrideSet::new();
    set.insert(entry_of(change));
    assert!(set.apply(&mut expected).is_empty());
    assert!(
        verify(after, None, &expected),
        "the edited program reproduces the document:\n{after}"
    );
}

fn entry_of(change: &EditCommand) -> motioner_core::OverrideEntry {
    match change {
        EditCommand::SetValue {
            id,
            property,
            value,
        } => motioner_core::OverrideEntry::new(
            id.clone(),
            property.clone(),
            motioner_core::OverrideAction::Set(value.clone()),
        ),
        other => panic!("unexpected command {other:?}"),
    }
}

fn plan_value(source: &str, index: usize, property: &str, value: OverrideValue) -> Plan {
    let execution = compiled(source);
    let change = set_value(&execution, index, property, value);
    let plan = plan(source, &execution.origins, &execution.document, &change);
    if let Plan::Code(edit) = &plan {
        let after = edit.apply(source).unwrap();
        assert_verified(&after, &execution, &change);
    }
    plan
}

const BADGE: &str = "def badge(x):\n    return circle(x, fill=\"#ffffff\")\n";

#[test]
fn a_value_that_is_a_parameter_is_passed_at_the_call() {
    let source = "comp(name=\"C\", duration=1.0)\n\
                  def badge(x, fill=\"#ffffff\"):\n\
                  \x20   return circle(x, fill=fill)\n\
                  a = badge(10)\n\
                  b = badge(20, fill=\"#00ff00\")\n";
    let (after, strategy) = applied(
        source,
        plan_value(source, 0, "fill", OverrideValue::Text("#ff0000".into())),
    );
    assert_eq!(strategy, Strategy::CallArgument);
    assert!(after.contains("a = badge(10, fill=\"#ff0000\")"), "{after}");
    assert!(after.contains("b = badge(20, fill=\"#00ff00\")"), "{after}");
}

#[test]
fn an_existing_keyword_argument_is_edited() {
    let source = "comp(name=\"C\", duration=1.0)\n\
                  def badge(x, fill=\"#ffffff\"):\n\
                  \x20   return circle(x, fill=fill)\n\
                  a = badge(10, fill=\"#00ff00\")\n";
    let (after, strategy) = applied(
        source,
        plan_value(source, 0, "fill", OverrideValue::Text("#ff0000".into())),
    );
    assert_eq!(strategy, Strategy::CallArgument);
    assert!(after.contains("a = badge(10, fill=\"#ff0000\")"), "{after}");
}

#[test]
fn a_positional_parameter_is_edited_in_place() {
    let source = "comp(name=\"C\", duration=1.0)\n\
                  def dot(fill):\n\
                  \x20   return circle(10, fill=fill)\n\
                  a = dot(\"#00ff00\")\n";
    let (after, strategy) = applied(
        source,
        plan_value(source, 0, "fill", OverrideValue::Text("#ff0000".into())),
    );
    assert_eq!(strategy, Strategy::CallArgument);
    assert!(after.contains("a = dot(\"#ff0000\")"), "{after}");
}

#[test]
fn a_returned_node_gets_an_assignment_after_the_call() {
    let source = format!("comp(name=\"C\", duration=1.0)\n{BADGE}a = badge(10)\nb = badge(20)\n");
    let (after, strategy) = applied(
        &source,
        plan_value(&source, 0, "fill", OverrideValue::Text("#ff0000".into())),
    );
    assert_eq!(strategy, Strategy::CallResult);
    assert!(
        after.contains("a = badge(10)\na.fill = \"#ff0000\""),
        "{after}"
    );
    assert!(after.contains("b = badge(20)\n"), "{after}");
    assert!(!after.contains("b.fill"), "{after}");
}

#[test]
fn an_unbound_call_result_is_bound_before_the_assignment() {
    let source = format!("comp(name=\"C\", duration=1.0)\n{BADGE}badge(10)\nbadge(20)\n");
    let (after, strategy) = applied(
        &source,
        plan_value(&source, 0, "fill", OverrideValue::Text("#ff0000".into())),
    );
    assert_eq!(strategy, Strategy::CallResult);
    assert!(after.contains(" = badge(10)\n"), "{after}");
    let binding = after
        .lines()
        .find_map(|line| line.strip_suffix(" = badge(10)"))
        .expect("the first call is bound");
    assert!(
        after.contains(&format!("{binding}.fill = \"#ff0000\"")),
        "{after}"
    );
    assert!(after.contains("\nbadge(20)\n"), "{after}");
}

#[test]
fn repeating_the_edit_replaces_the_call_assignment() {
    let source = format!(
        "comp(name=\"C\", duration=1.0)\n{BADGE}a = badge(10)\nshade = \"#00ff00\"\na.fill = shade\n"
    );
    let (after, strategy) = applied(
        &source,
        plan_value(&source, 0, "fill", OverrideValue::Text("#ff0000".into())),
    );
    assert_eq!(strategy, Strategy::CallResult);
    assert!(after.contains("a.fill = \"#ff0000\""), "{after}");
    assert_eq!(after.matches("a.fill").count(), 1, "{after}");
}

#[test]
fn a_literal_in_a_non_returning_function_becomes_a_parameter() {
    let source = "comp(name=\"C\", duration=1.0)\n\
                  def tint(target):\n\
                  \x20   target.fill = \"#ffffff\"\n\
                  a = circle(10)\n\
                  b = circle(20)\n\
                  tint(a)\n\
                  tint(b)\n";
    let (after, strategy) = applied(
        source,
        plan_value(source, 0, "fill", OverrideValue::Text("#ff0000".into())),
    );
    assert_eq!(strategy, Strategy::CallPromotion);
    assert!(
        after.contains("def tint(target, fill=\"#ffffff\"):"),
        "{after}"
    );
    assert!(after.contains("target.fill = fill"), "{after}");
    assert!(after.contains("tint(a, fill=\"#ff0000\")"), "{after}");
    assert!(after.contains("tint(b)\n"), "{after}");
}

#[test]
fn a_pair_literal_is_promoted_with_its_default() {
    let source = "comp(name=\"C\", duration=1.0)\n\
                  def place(target):\n\
                  \x20   target.pos = (10, 20)\n\
                  a = circle(10)\n\
                  b = circle(20)\n\
                  place(a)\n\
                  place(b)\n";
    let (after, strategy) = applied(
        source,
        plan_value(
            source,
            0,
            "pos",
            OverrideValue::Vec2(motioner_core::Vec2::new(30.0, 40.0)),
        ),
    );
    assert_eq!(strategy, Strategy::CallPromotion);
    assert!(
        after.contains("def place(target, pos=(10, 20)):"),
        "{after}"
    );
    assert!(after.contains("place(a, pos=(30, 40))"), "{after}");
}

#[test]
fn a_loop_call_binds_and_guards_after_the_call() {
    let source = "comp(name=\"C\", duration=1.0)\n\
                  def badge(i):\n\
                  \x20   return circle(10, fill=\"#ffffff\")\n\
                  for i in range(4):\n\
                  \x20   b = badge(i)\n";
    let (after, strategy) = applied(
        source,
        plan_value(source, 2, "fill", OverrideValue::Text("#ff0000".into())),
    );
    assert_eq!(strategy, Strategy::LoopGuard);
    assert!(
        after.contains("    b = badge(i)\n    if i == 2:\n        b.fill = \"#ff0000\""),
        "{after}"
    );
    assert_eq!(after.matches("b = badge(i)").count(), 1, "{after}");
}

#[test]
fn an_unbound_loop_call_gets_a_binding_before_the_guard() {
    let source = "comp(name=\"C\", duration=1.0)\n\
                  def badge(i):\n\
                  \x20   return circle(10, fill=\"#ffffff\")\n\
                  for i in range(4):\n\
                  \x20   badge(i)\n";
    let (after, strategy) = applied(
        source,
        plan_value(source, 2, "fill", OverrideValue::Text("#ff0000".into())),
    );
    assert_eq!(strategy, Strategy::LoopGuard);
    assert!(after.contains("    circle_2 = badge(i)"), "{after}");
    assert!(
        after.contains("    if i == 2:\n        circle_2.fill = \"#ff0000\""),
        "{after}"
    );
}

#[test]
fn a_value_written_by_a_function_inside_a_loop_is_guarded_after_the_call() {
    let source = "comp(name=\"C\", duration=1.0)\n\
                  def tint(target):\n\
                  \x20   target.fill = \"#ffffff\"\n\
                  for i in range(4):\n\
                  \x20   c = circle(10)\n\
                  \x20   tint(c)\n";
    let (after, strategy) = applied(
        source,
        plan_value(source, 2, "fill", OverrideValue::Text("#ff0000".into())),
    );
    assert_eq!(strategy, Strategy::LoopGuard);
    assert!(
        after.contains("    tint(c)\n    if i == 2:\n        c.fill = \"#ff0000\""),
        "{after}"
    );
}

#[test]
fn a_nested_call_falls_back_with_a_reason() {
    let source = "comp(name=\"C\", duration=1.0)\n\
                  def inner():\n\
                  \x20   return circle(1, fill=\"#ffffff\")\n\
                  def outer():\n\
                  \x20   return inner()\n\
                  a = outer()\n\
                  b = outer()\n";
    let execution = compiled(source);
    let change = set_value(&execution, 0, "fill", OverrideValue::Text("#ff0000".into()));
    let plan = plan(source, &execution.origins, &execution.document, &change);
    assert!(
        matches!(&plan, Plan::Fallback(reason) if reason.contains("nested")),
        "{plan:?}"
    );
}

#[test]
fn a_call_inside_another_expression_falls_back_with_a_reason() {
    let source = format!(
        "comp(name=\"C\", duration=1.0)\n{BADGE}g = group()\ng.add(badge(10))\ng.add(badge(20))\n"
    );
    let execution = compiled(&source);
    let change = set_value(&execution, 1, "fill", OverrideValue::Text("#ff0000".into()));
    let plan = plan(&source, &execution.origins, &execution.document, &change);
    assert!(
        matches!(&plan, Plan::Fallback(reason) if reason.contains("not bound")),
        "{plan:?}"
    );
}

#[test]
fn promotion_keeps_crlf_and_the_call_layout() {
    let source = "comp(name=\"C\", duration=1.0)\r\n\
                  def tint(target):\r\n\
                  \x20   target.fill = \"#ffffff\"\r\n\
                  a = circle(10)\r\n\
                  b = circle(20)\r\n\
                  tint(a)\r\n\
                  tint(b)\r\n";
    let (after, strategy) = applied(
        source,
        plan_value(source, 0, "fill", OverrideValue::Text("#ff0000".into())),
    );
    assert_eq!(strategy, Strategy::CallPromotion);
    assert!(
        after.contains("def tint(target, fill=\"#ffffff\"):\r\n"),
        "{after:?}"
    );
    assert!(!after.replace("\r\n", "").contains('\n'));
}

#[test]
fn a_multiline_call_appends_its_argument_on_a_new_line() {
    let source = "comp(name=\"C\", duration=1.0)\n\
                  def badge(\n\
                  \x20   x,\n\
                  \x20   fill=\"#ffffff\",\n\
                  ):\n\
                  \x20   return circle(x, fill=fill)\n\
                  a = badge(\n\
                  \x20   10,\n\
                  )\n";
    let (after, strategy) = applied(
        source,
        plan_value(source, 0, "fill", OverrideValue::Text("#ff0000".into())),
    );
    assert_eq!(strategy, Strategy::CallArgument);
    assert!(
        after.contains("a = badge(\n    10,\n    fill=\"#ff0000\",\n)"),
        "{after}"
    );
}

#[test]
fn a_key_value_is_passed_at_the_call() {
    let source = "comp(name=\"C\", duration=1.0)\n\
                  def pulse(x, opacity):\n\
                  \x20   c = circle(x)\n\
                  \x20   c.key(\"opacity\", 0.5, opacity)\n\
                  \x20   return c\n\
                  a = pulse(10, 0.2)\n\
                  b = pulse(20, 0.8)\n";
    let execution = compiled(source);
    let change = EditCommand::SetKey {
        id: node_id(&execution, 0),
        property: "opacity".to_owned(),
        axis: None,
        frame: 30,
        value: OverrideValue::Number(0.5),
        ease: Ease::Linear,
    };
    let plan = plan(source, &execution.origins, &execution.document, &change);
    let (after, strategy) = applied(source, plan);
    assert_eq!(strategy, Strategy::CallArgument);
    assert!(after.contains("a = pulse(10, 0.5)"), "{after}");
}

#[test]
fn a_returning_function_reuses_the_existing_binding() {
    let source = "comp(name=\"C\", duration=1.0)\n\
                  def tint(target):\n\
                  \x20   target.fill = \"#ffffff\"\n\
                  \x20   return target\n\
                  a = circle(10)\n\
                  tint(a)\n\
                  tint(a)\n";
    let (after, strategy) = applied(
        source,
        plan_value(source, 0, "fill", OverrideValue::Text("#ff0000".into())),
    );
    assert_eq!(strategy, Strategy::CallResult);
    assert!(after.contains("a.fill = \"#ff0000\""), "{after}");
    assert_eq!(after.matches("tint(a)").count(), 2, "{after}");
    assert!(!after.contains("circle_2"), "{after}");
}

#[test]
fn a_computed_value_in_a_function_falls_back_with_a_reason() {
    let source = "comp(name=\"C\", duration=1.0)\n\
                  def pick(shade):\n\
                  \x20   return shade\n\
                  def tint(target, shade):\n\
                  \x20   target.fill = pick(shade)\n\
                  a = circle(10)\n\
                  tint(a, \"#ffffff\")\n\
                  tint(a, \"#eeeeee\")\n";
    let execution = compiled(source);
    let change = set_value(&execution, 0, "fill", OverrideValue::Text("#ff0000".into()));
    let plan = plan(source, &execution.origins, &execution.document, &change);
    assert!(
        matches!(&plan, Plan::Fallback(reason) if reason.contains("computed")),
        "{plan:?}"
    );
}

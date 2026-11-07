//! Structural edits as code: add, delete, duplicate, rename, reorder, parent,
//! asset placement and key maintenance (prompt 12 phase 5).

use motioner_core::{
    Document, Ease, EditCommand, MoveKey, OverrideAction, OverrideEntry, OverrideSet, OverrideValue,
};
use motioner_dsl::structural::{plan_structural, LayerKind, StructuralCommand};
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

fn plan_and_apply(
    source: &str,
    execution: &Execution,
    command: &StructuralCommand,
) -> (String, Strategy) {
    let plan = plan_structural(source, &execution.origins, &execution.document, command);
    let Plan::Code(edit) = plan else {
        panic!("expected a code plan, got {plan:?}");
    };
    #[allow(clippy::let_and_return)] // keep the panic branch next to the match
    let strategy = edit.strategy();
    (edit.apply(source).unwrap(), strategy)
}

fn entry(id: &str, property: &str, action: OverrideAction) -> OverrideEntry {
    OverrideEntry::new(id.to_owned(), property.to_owned(), action)
}

fn expected_with(execution: &Execution, entries: Vec<OverrideEntry>) -> Document {
    let mut expected = execution.document.clone();
    let mut set = OverrideSet::new();
    for entry in entries {
        set.insert(entry);
    }
    assert!(set.apply(&mut expected).is_empty());
    expected
}

const BASIC: &str = "comp(name=\"P\", duration=1.0)\n\
                     a = circle(10)\n\
                     a.pos = (10, 20)\n\
                     b = circle(20)\n";

#[test]
fn add_layer_after_the_selected_node() {
    let execution = compiled(BASIC);
    let change = StructuralCommand::AddLayer {
        kind: LayerKind::Rect {
            width: 120.0,
            height: 60.0,
        },
        after: Some(node_id(&execution, 0)),
        name: None,
    };
    let (after, strategy) = plan_and_apply(BASIC, &execution, &change);
    assert_eq!(strategy, Strategy::StructuralEdit);
    assert_eq!(
        after,
        "comp(name=\"P\", duration=1.0)\n\
         a = circle(10)\n\
         a.pos = (10, 20)\n\
         rect = rect(120, 60)\n\
         b = circle(20)\n"
    );
    let actual = motioner_dsl::run(&after, &Limits::default()).unwrap();
    assert_eq!(actual.len(), execution.document.len() + 1);
    assert!(matches!(
        actual.nodes().nth(1).unwrap().content(),
        motioner_core::NodeContent::Rect { .. }
    ));
}

#[test]
fn add_layer_at_the_end_of_the_body() {
    let execution = compiled(BASIC);
    let change = StructuralCommand::AddLayer {
        kind: LayerKind::Group,
        after: None,
        name: None,
    };
    let (after, _) = plan_and_apply(BASIC, &execution, &change);
    assert!(
        after.ends_with("b = circle(20)\ngroup = group()\n"),
        "{after}"
    );
}

#[test]
fn delete_removes_the_creation_and_dependent_statements() {
    let execution = compiled(BASIC);
    let change = StructuralCommand::Delete {
        id: node_id(&execution, 0),
    };
    let (after, _) = plan_and_apply(BASIC, &execution, &change);
    assert_eq!(after, "comp(name=\"P\", duration=1.0)\nb = circle(20)\n");
    assert_eq!(
        motioner_dsl::run(&after, &Limits::default()).unwrap().len(),
        1
    );
}

#[test]
fn delete_falls_back_when_a_statement_also_writes_another_layer() {
    let source = "comp(name=\"P\", duration=1.0)\n\
                  a = circle(10)\n\
                  b = circle(20)\n\
                  a.pos = (b.radius, 10)\n";
    let execution = compiled(source);
    let plan = plan_structural(
        source,
        &execution.origins,
        &execution.document,
        &StructuralCommand::Delete {
            id: node_id(&execution, 0),
        },
    );
    assert!(
        matches!(&plan, Plan::Fallback(reason) if reason.contains("shared")),
        "{plan:?}"
    );
}

#[test]
fn delete_removes_the_group_add_call_too() {
    let source = "comp(name=\"P\", duration=1.0)\n\
                  g = group()\n\
                  a = circle(10)\n\
                  g.add(a)\n\
                  b = circle(20)\n";
    let execution = compiled(source);
    let (after, _) = plan_and_apply(
        source,
        &execution,
        &StructuralCommand::Delete {
            id: node_id(&execution, 1),
        },
    );
    assert_eq!(
        after,
        "comp(name=\"P\", duration=1.0)\ng = group()\nb = circle(20)\n"
    );
}

#[test]
fn delete_falls_back_when_the_add_call_is_shared() {
    let source = "comp(name=\"P\", duration=1.0)\n\
                  g = group()\n\
                  a = circle(10)\n\
                  b = circle(20)\n\
                  g.add(a, b)\n";
    let execution = compiled(source);
    let plan = plan_structural(
        source,
        &execution.origins,
        &execution.document,
        &StructuralCommand::Delete {
            id: node_id(&execution, 1),
        },
    );
    assert!(
        matches!(&plan, Plan::Fallback(reason) if reason.contains("add()")),
        "{plan:?}"
    );
}

#[test]
fn deleting_one_loop_instance_writes_a_continue_guard() {
    let source = "comp(name=\"P\", duration=1.0)\n\
                  for i in range(5):\n\
                  \x20   c = circle(10)\n\
                  \x20   c.pos = (i * 10, 20)\n";
    let execution = compiled(source);
    let target = node_id(&execution, 3);
    let (after, strategy) = plan_and_apply(
        source,
        &execution,
        &StructuralCommand::Delete { id: target.clone() },
    );
    assert_eq!(strategy, Strategy::StructuralLoopContinue);
    assert_eq!(
        after,
        "comp(name=\"P\", duration=1.0)\n\
         for i in range(5):\n\
         \x20   if i == 3:\n\
         \x20       continue\n\
         \x20   c = circle(10)\n\
         \x20   c.pos = (i * 10, 20)\n"
    );
    assert_eq!(
        motioner_dsl::run(&after, &Limits::default()).unwrap().len(),
        4
    );
}

#[test]
fn deleting_one_loop_instance_falls_back_when_the_iteration_builds_more_layers() {
    let source = "comp(name=\"P\", duration=1.0)\n\
                  for i in range(3):\n\
                  \x20   c = circle(10)\n\
                  \x20   d = rect(10, 10)\n";
    let execution = compiled(source);
    let plan = plan_structural(
        source,
        &execution.origins,
        &execution.document,
        &StructuralCommand::Delete {
            id: node_id(&execution, 0),
        },
    );
    assert!(
        matches!(&plan, Plan::Fallback(reason) if reason.contains("more than one layer")),
        "{plan:?}"
    );
}

#[test]
fn duplicate_copies_the_build_statements_under_a_fresh_binding() {
    let source = "comp(name=\"P\", duration=1.0)\n\
                  a = circle(10)\n\
                  a.pos = (10, 20)\n\
                  b = circle(20)\n";
    let execution = compiled(source);
    let (after, _) = plan_and_apply(
        source,
        &execution,
        &StructuralCommand::Duplicate {
            id: node_id(&execution, 0),
        },
    );
    assert_eq!(
        after,
        "comp(name=\"P\", duration=1.0)\n\
         a = circle(10)\n\
         a.pos = (10, 20)\n\
         a_2 = circle(10)\n\
         a_2.pos = (10, 20)\n\
         b = circle(20)\n"
    );
    assert_eq!(
        motioner_dsl::run(&after, &Limits::default()).unwrap().len(),
        3
    );
}

#[test]
fn rename_edits_an_existing_name_assignment() {
    let source = "comp(name=\"P\", duration=1.0)\na = circle(10)\na.name = \"old\"\n";
    let execution = compiled(source);
    let (after, _) = plan_and_apply(
        source,
        &execution,
        &StructuralCommand::Rename {
            id: node_id(&execution, 0),
            name: "New".to_owned(),
        },
    );
    assert_eq!(
        after,
        "comp(name=\"P\", duration=1.0)\na = circle(10)\na.name = \"New\"\n"
    );
}

#[test]
fn rename_inserts_a_name_assignment() {
    let source = "comp(name=\"P\", duration=1.0)\na = circle(10)\nb = circle(20)\n";
    let execution = compiled(source);
    let (after, _) = plan_and_apply(
        source,
        &execution,
        &StructuralCommand::Rename {
            id: node_id(&execution, 0),
            name: "Title".to_owned(),
        },
    );
    assert_eq!(
        after,
        "comp(name=\"P\", duration=1.0)\n\
         a = circle(10)\n\
         a.name = \"Title\"\n\
         b = circle(20)\n"
    );
}

#[test]
fn an_empty_name_removes_the_private_assignment() {
    let source = "comp(name=\"P\", duration=1.0)\na = circle(10)\na.name = \"old\"\n";
    let execution = compiled(source);
    let (after, _) = plan_and_apply(
        source,
        &execution,
        &StructuralCommand::Rename {
            id: node_id(&execution, 0),
            name: String::new(),
        },
    );
    assert_eq!(after, "comp(name=\"P\", duration=1.0)\na = circle(10)\n");
}

#[test]
fn reorder_moves_the_whole_creation_block() {
    let source = "comp(name=\"P\", duration=1.0)\n\
                  a = circle(10)\n\
                  a.pos = (10, 20)\n\
                  b = circle(20)\n\
                  c = circle(30)\n";
    let execution = compiled(source);
    let (after, _) = plan_and_apply(
        source,
        &execution,
        &StructuralCommand::Reorder {
            id: node_id(&execution, 0),
            to: 2,
        },
    );
    assert_eq!(
        after,
        "comp(name=\"P\", duration=1.0)\n\
         b = circle(20)\n\
         c = circle(30)\n\
         a = circle(10)\n\
         a.pos = (10, 20)\n"
    );
}

#[test]
fn reorder_falls_back_when_the_creation_block_is_not_contiguous() {
    let source = "comp(name=\"P\", duration=1.0)\n\
                  a = circle(10)\n\
                  b = circle(20)\n\
                  a.pos = (10, 20)\n\
                  c = circle(30)\n";
    let execution = compiled(source);
    let plan = plan_structural(
        source,
        &execution.origins,
        &execution.document,
        &StructuralCommand::Reorder {
            id: node_id(&execution, 0),
            to: 2,
        },
    );
    assert!(
        matches!(&plan, Plan::Fallback(reason) if reason.contains("contiguous")),
        "{plan:?}"
    );
}

#[test]
fn parent_adds_a_group_add_call_and_unparent_removes_it() {
    let source = "comp(name=\"P\", duration=1.0)\n\
                  g = group()\n\
                  a = circle(10)\n\
                  b = circle(20)\n";
    let execution = compiled(source);
    let group = node_id(&execution, 0);
    let child = node_id(&execution, 1);
    let (parented, _) = plan_and_apply(
        source,
        &execution,
        &StructuralCommand::Reparent {
            id: child.clone(),
            parent: Some(group),
        },
    );
    assert!(parented.contains("g.add(a)"), "{parented}");
    let parented_execution = compiled(&parented);
    let parented_document = &parented_execution.document;
    assert_eq!(
        parented_document.nodes().nth(1).unwrap().parent(),
        Some(parented_document.nodes().next().unwrap().id())
    );
    let (unparented, _) = plan_and_apply(
        &parented,
        &parented_execution,
        &StructuralCommand::Reparent {
            id: node_id(&parented_execution, 1),
            parent: None,
        },
    );
    assert!(!unparented.contains("g.add(a)"), "{unparented}");
}

#[test]
fn place_asset_inserts_image_and_audio_statements() {
    let execution = compiled(BASIC);
    let image = StructuralCommand::PlaceAsset {
        kind: LayerKind::Image {
            source: String::new(),
        },
        source: "pics/card.png".to_owned(),
        frame: None,
        pos: Some((100.0, 50.0)),
    };
    let (after, _) = plan_and_apply(BASIC, &execution, &image);
    assert!(
        after
            .ends_with("b = circle(20)\nimage = image(\"pics/card.png\")\nimage.pos = (100, 50)\n"),
        "{after}"
    );
    let audio = StructuralCommand::PlaceAsset {
        kind: LayerKind::Audio {
            source: String::new(),
        },
        source: "music/loop.wav".to_owned(),
        frame: Some(30),
        pos: None,
    };
    let (after, _) = plan_and_apply(BASIC, &execution, &audio);
    assert!(
        after.ends_with("b = circle(20)\naudio = audio(\"music/loop.wav\", start=30)\n"),
        "{after}"
    );
}

#[test]
fn locked_is_editor_state_and_falls_back() {
    let execution = compiled(BASIC);
    let plan = plan(
        BASIC,
        &execution.origins,
        &execution.document,
        &EditCommand::SetLocked {
            id: node_id(&execution, 0),
            locked: true,
        },
    );
    assert!(
        matches!(&plan, Plan::Fallback(reason) if reason.contains("editor.motion")),
        "{plan:?}"
    );
}

// ---- key maintenance -----------------------------------------------------

const KEYS: &str = "comp(name=\"P\", duration=1.0)\n\
                    a = circle(10)\n\
                    a.key(\"opacity\", 0, 1)\n\
                    a.key(\"opacity\", 0.5, 0.5, ease=\"cubic_out\")\n";

fn keys_entry(id: &str, action: OverrideAction) -> OverrideEntry {
    entry(id, "opacity", action)
}

#[test]
fn remove_key_deletes_the_key_call() {
    let execution = compiled(KEYS);
    let plan = plan(
        KEYS,
        &execution.origins,
        &execution.document,
        &EditCommand::RemoveKey {
            id: node_id(&execution, 0),
            property: "opacity".to_owned(),
            axis: None,
            frame: 30,
        },
    );
    let Plan::Code(edit) = plan else {
        panic!("expected code, got {plan:?}");
    };
    let after = edit.apply(KEYS).unwrap();
    assert!(!after.contains("cubic_out"), "{after}");
    assert!(after.contains("a.key(\"opacity\", 0, 1)"), "{after}");
    let expected = expected_with(
        &execution,
        vec![keys_entry(
            &node_id(&execution, 0),
            OverrideAction::Unkey { frame: 30 },
        )],
    );
    assert!(verify(&after, None, &expected));
}

#[test]
fn move_key_edits_the_frame_argument() {
    let execution = compiled(KEYS);
    let id = node_id(&execution, 0);
    let plan = plan(
        KEYS,
        &execution.origins,
        &execution.document,
        &EditCommand::MoveKeys(vec![MoveKey {
            key: motioner_core::KeyRef {
                id: id.clone(),
                property: "opacity".to_owned(),
                frame: 0,
            },
            to: 15,
            value: OverrideValue::Number(1.0),
            ease: Ease::Linear,
        }]),
    );
    let Plan::Code(edit) = plan else {
        panic!("expected code, got {plan:?}");
    };
    let after = edit.apply(KEYS).unwrap();
    assert!(after.contains("a.key(\"opacity\", 0.25, 1)"), "{after}");
    let expected = expected_with(
        &execution,
        vec![
            keys_entry(&id, OverrideAction::Unkey { frame: 0 }),
            keys_entry(
                &id,
                OverrideAction::Key {
                    frame: 15,
                    value: OverrideValue::Number(1.0),
                    ease: Ease::Linear,
                },
            ),
        ],
    );
    assert!(verify(&after, None, &expected));
}

#[test]
fn ease_change_edits_the_ease_argument() {
    let execution = compiled(KEYS);
    let id = node_id(&execution, 0);
    let plan = plan(
        KEYS,
        &execution.origins,
        &execution.document,
        &EditCommand::MoveKeys(vec![MoveKey {
            key: motioner_core::KeyRef {
                id: id.clone(),
                property: "opacity".to_owned(),
                frame: 30,
            },
            to: 30,
            value: OverrideValue::Number(0.5),
            ease: Ease::QuadIn,
        }]),
    );
    let Plan::Code(edit) = plan else {
        panic!("expected code, got {plan:?}");
    };
    let after = edit.apply(KEYS).unwrap();
    assert!(after.contains("ease=\"quad_in\""), "{after}");
}

#[test]
fn stopwatch_on_adds_the_first_key() {
    let execution = compiled(BASIC);
    let id = node_id(&execution, 0);
    let plan = plan(
        BASIC,
        &execution.origins,
        &execution.document,
        &EditCommand::ToggleStopwatch {
            id: id.clone(),
            property: "opacity".to_owned(),
            axis: None,
            frame: 60,
            value: OverrideValue::Number(0.3),
            existing_keys: Vec::new(),
        },
    );
    let Plan::Code(edit) = plan else {
        panic!("expected code, got {plan:?}");
    };
    let after = edit.apply(BASIC).unwrap();
    assert!(after.contains("a.key(\"opacity\", 1, 0.3)"), "{after}");
}

#[test]
fn stopwatch_off_removes_keys_and_writes_the_static_value() {
    let execution = compiled(KEYS);
    let id = node_id(&execution, 0);
    let plan = plan(
        KEYS,
        &execution.origins,
        &execution.document,
        &EditCommand::ToggleStopwatch {
            id: id.clone(),
            property: "opacity".to_owned(),
            axis: None,
            frame: 60,
            value: OverrideValue::Number(0.7),
            existing_keys: vec![0, 30],
        },
    );
    let Plan::Code(edit) = plan else {
        panic!("expected code, got {plan:?}");
    };
    let after = edit.apply(KEYS).unwrap();
    assert!(!after.contains(".key("), "{after}");
    assert!(after.contains("a.opacity = 0.7"), "{after}");
    let expected = expected_with(
        &execution,
        vec![
            keys_entry(&id, OverrideAction::Unkey { frame: 0 }),
            keys_entry(&id, OverrideAction::Unkey { frame: 30 }),
            keys_entry(&id, OverrideAction::Set(OverrideValue::Number(0.7))),
        ],
    );
    assert!(verify(&after, None, &expected));
}

#[test]
fn removing_a_loop_key_excludes_the_selected_iteration() {
    let source = "comp(name=\"P\", duration=1.0)\n\
                  for i in range(3):\n\
                  \x20   c = circle(10)\n\
                  \x20   c.key(\"opacity\", 0, 1)\n";
    let execution = compiled(source);
    let id = node_id(&execution, 1);
    let plan = plan(
        source,
        &execution.origins,
        &execution.document,
        &EditCommand::RemoveKey {
            id: id.clone(),
            property: "opacity".to_owned(),
            axis: None,
            frame: 0,
        },
    );
    let Plan::Code(edit) = plan else {
        panic!("expected code, got {plan:?}");
    };
    let after = edit.apply(source).unwrap();
    assert!(after.contains("if i != 1:"), "{after}");
    let expected = expected_with(
        &execution,
        vec![keys_entry(&id, OverrideAction::Unkey { frame: 0 })],
    );
    assert!(verify(&after, None, &expected));
}

#[test]
fn moving_a_loop_key_guards_both_the_old_and_the_new_frame() {
    let source = "comp(name=\"P\", duration=1.0)\n\
                  for i in range(3):\n\
                  \x20   c = circle(10)\n\
                  \x20   c.key(\"opacity\", 0, 1)\n";
    let execution = compiled(source);
    let id = node_id(&execution, 1);
    let plan = plan(
        source,
        &execution.origins,
        &execution.document,
        &EditCommand::MoveKeys(vec![MoveKey {
            key: motioner_core::KeyRef {
                id: id.clone(),
                property: "opacity".to_owned(),
                frame: 0,
            },
            to: 30,
            value: OverrideValue::Number(1.0),
            ease: Ease::Linear,
        }]),
    );
    let Plan::Code(edit) = plan else {
        panic!("expected code, got {plan:?}");
    };
    let after = edit.apply(source).unwrap();
    assert!(after.contains("if i != 1:"), "{after}");
    assert!(after.contains("if i == 1:"), "{after}");
    assert!(after.contains("c.key(\"opacity\", 0.5, 1)"), "{after}");
    let expected = expected_with(
        &execution,
        vec![
            keys_entry(&id, OverrideAction::Unkey { frame: 0 }),
            keys_entry(
                &id,
                OverrideAction::Key {
                    frame: 30,
                    value: OverrideValue::Number(1.0),
                    ease: Ease::Linear,
                },
            ),
        ],
    );
    assert!(verify(&after, None, &expected));
}

#[test]
fn removing_a_guarded_loop_key_removes_the_guard_too() {
    let source = "comp(name=\"P\", duration=1.0)\n\
                  for i in range(3):\n\
                  \x20   c = circle(10)\n\
                  \x20   if i == 1:\n\
                  \x20       c.key(\"opacity\", 0, 1)\n";
    let execution = compiled(source);
    let id = node_id(&execution, 1);
    let plan = plan(
        source,
        &execution.origins,
        &execution.document,
        &EditCommand::RemoveKey {
            id: id.clone(),
            property: "opacity".to_owned(),
            axis: None,
            frame: 0,
        },
    );
    let Plan::Code(edit) = plan else {
        panic!("expected code, got {plan:?}");
    };
    let after = edit.apply(source).unwrap();
    assert!(!after.contains("if i == 1:"), "{after}");
    assert!(!after.contains(".key("), "{after}");
}

#[test]
fn a_shared_call_key_move_falls_back() {
    let source = "comp(name=\"P\", duration=1.0)\n\
                  def pulse(x):\n\
                  \x20   c = circle(x)\n\
                  \x20   c.key(\"opacity\", 0, 0.5)\n\
                  \x20   return c\n\
                  a = pulse(10)\n\
                  b = pulse(20)\n";
    let execution = compiled(source);
    let id = node_id(&execution, 0);
    let plan = plan(
        source,
        &execution.origins,
        &execution.document,
        &EditCommand::MoveKeys(vec![MoveKey {
            key: motioner_core::KeyRef {
                id,
                property: "opacity".to_owned(),
                frame: 0,
            },
            to: 15,
            value: OverrideValue::Number(0.5),
            ease: Ease::Linear,
        }]),
    );
    assert!(
        matches!(&plan, Plan::Fallback(reason) if reason.contains("function")),
        "{plan:?}"
    );
}

#[test]
fn a_loop_key_remove_for_nested_loops_uses_both_guards() {
    let source = "comp(name=\"P\", duration=1.0)\n\
                  for row in range(2):\n\
                  \x20   for col in range(3):\n\
                  \x20       d = circle(8)\n\
                  \x20       d.key(\"opacity\", 0, 1)\n";
    let execution = compiled(source);
    let id = node_id(&execution, 5);
    let plan = plan(
        source,
        &execution.origins,
        &execution.document,
        &EditCommand::RemoveKey {
            id: id.clone(),
            property: "opacity".to_owned(),
            axis: None,
            frame: 0,
        },
    );
    let Plan::Code(edit) = plan else {
        panic!("expected code, got {plan:?}");
    };
    let after = edit.apply(source).unwrap();
    assert!(after.contains("if not (row == 1 and col == 2):"), "{after}");
}

#[test]
fn set_name_and_visible_are_planned_as_code_through_the_command_layer() {
    let execution = compiled(BASIC);
    let id = node_id(&execution, 0);
    let renamed = plan(
        BASIC,
        &execution.origins,
        &execution.document,
        &EditCommand::SetName {
            id: id.clone(),
            name: "Title".to_owned(),
        },
    );
    let Plan::Code(edit) = renamed else {
        panic!("expected code, got {renamed:?}");
    };
    assert!(edit.apply(BASIC).unwrap().contains("a.name = \"Title\""));
    let hidden = plan(
        BASIC,
        &execution.origins,
        &execution.document,
        &EditCommand::SetVisible { id, visible: false },
    );
    let Plan::Code(edit) = hidden else {
        panic!("expected code, got {hidden:?}");
    };
    assert!(edit.apply(BASIC).unwrap().contains("a.visible = False"));
}

#[test]
fn add_layer_works_after_an_inline_layer() {
    let source = "comp(name=\"P\", duration=1.0)\ncircle(10)\n";
    let execution = compiled(source);
    let (after, _) = plan_and_apply(
        source,
        &execution,
        &StructuralCommand::AddLayer {
            kind: LayerKind::Circle { radius: 5.0 },
            after: Some(node_id(&execution, 0)),
            name: None,
        },
    );
    assert_eq!(
        after,
        "comp(name=\"P\", duration=1.0)\ncircle(10)\ncircle_2 = circle(5)\n"
    );
}

#[test]
fn add_layer_after_a_loop_created_layer_falls_back_with_a_reason() {
    let source = "comp(name=\"P\", duration=1.0)\nfor i in range(2):\n    c = circle(10)\n";
    let execution = compiled(source);
    let plan = plan_structural(
        source,
        &execution.origins,
        &execution.document,
        &StructuralCommand::AddLayer {
            kind: LayerKind::Group,
            after: Some(node_id(&execution, 0)),
            name: None,
        },
    );
    assert!(
        matches!(&plan, Plan::Fallback(reason) if reason.contains("loop")),
        "{plan:?}"
    );
}

#[test]
fn duplicate_preserves_comments_with_non_ascii_text() {
    let source = "comp(name=\"P\", duration=1.0)\n\
                  a = circle(10)\n\
                  # café: keep the accent\n\
                  a.pos = (10, 20)\n";
    let execution = compiled(source);
    let (after, _) = plan_and_apply(
        source,
        &execution,
        &StructuralCommand::Duplicate {
            id: node_id(&execution, 0),
        },
    );
    assert!(after.contains("# café: keep the accent"), "{after}");
    assert!(after.contains("a_2 = circle(10)"), "{after}");
    assert!(after.contains("a_2.pos = (10, 20)"), "{after}");
}

#[test]
fn a_loop_stopwatch_on_uses_the_loop_guard() {
    let source = "comp(name=\"P\", duration=1.0)\nfor i in range(3):\n    c = circle(10)\n";
    let execution = compiled(source);
    let id = node_id(&execution, 1);
    let plan = plan(
        source,
        &execution.origins,
        &execution.document,
        &EditCommand::ToggleStopwatch {
            id,
            property: "opacity".to_owned(),
            axis: None,
            frame: 0,
            value: OverrideValue::Number(0.4),
            existing_keys: Vec::new(),
        },
    );
    let Plan::Code(edit) = plan else {
        panic!("expected code, got {plan:?}");
    };
    let after = edit.apply(source).unwrap();
    assert!(
        after.contains("if i == 1:\n        c.key(\"opacity\", 0, 0.4)"),
        "{after}"
    );
}

#[test]
fn place_asset_supports_video() {
    let execution = compiled(BASIC);
    let (after, _) = plan_and_apply(
        BASIC,
        &execution,
        &StructuralCommand::PlaceAsset {
            kind: LayerKind::Video {
                source: String::new(),
            },
            source: "clips/intro.mp4".to_owned(),
            frame: Some(0),
            pos: None,
        },
    );
    assert!(
        after.ends_with("b = circle(20)\nvideo = video(\"clips/intro.mp4\", start=0)\n"),
        "{after}"
    );
}

#[test]
fn a_comment_and_the_file_line_endings_survive_structural_edits() {
    let source = "comp(name=\"P\", duration=1.0)\r\n\
                  # keep me\r\n\
                  a = circle(10)\r\n";
    let execution = compiled(source);
    let (after, _) = plan_and_apply(
        source,
        &execution,
        &StructuralCommand::AddLayer {
            kind: LayerKind::Circle { radius: 5.0 },
            after: Some(node_id(&execution, 0)),
            name: Some("dot".to_owned()),
        },
    );
    assert!(after.contains("# keep me\r\n"), "{after:?}");
    assert!(after.contains("dot = circle(5)\r\n"), "{after:?}");
}

#[test]
fn group_wraps_the_selection_in_one_edit() {
    let execution = compiled(BASIC);
    let ids = vec![node_id(&execution, 0), node_id(&execution, 1)];
    let change = StructuralCommand::Group {
        ids: ids.clone(),
        name: Some("pair".to_owned()),
    };
    let (after, strategy) = plan_and_apply(BASIC, &execution, &change);
    assert_eq!(strategy, Strategy::StructuralEdit);
    assert!(after.contains("group"), "{after}");
    let grouped = compiled(&after);
    assert_eq!(grouped.document.len(), 3, "{after}");
    let group = grouped
        .document
        .nodes()
        .find(|node| node.name() == Some("pair"))
        .expect("the group keeps its readable name");
    for id in &ids {
        let node = grouped
            .document
            .find(id)
            .expect("the original layer survives");
        assert_eq!(
            grouped
                .document
                .node(node)
                .and_then(motioner_core::Node::parent),
            Some(group.id()),
            "{after}"
        );
    }
}

#[test]
fn ungroup_returns_children_and_deletes_the_group() {
    let execution = compiled(BASIC);
    let ids = vec![node_id(&execution, 0), node_id(&execution, 1)];
    let change = StructuralCommand::Group {
        ids,
        name: Some("pair".to_owned()),
    };
    let (grouped, _) = plan_and_apply(BASIC, &execution, &change);
    let group_execution = compiled(&grouped);
    let group = group_execution
        .document
        .nodes()
        .find(|node| node.name() == Some("pair"))
        .expect("a group");
    let ungroup = StructuralCommand::Ungroup {
        id: group.identity().as_str().to_owned(),
    };
    let (after, strategy) = plan_and_apply(&grouped, &group_execution, &ungroup);
    assert_eq!(strategy, Strategy::StructuralEdit);
    let result = compiled(&after);
    assert_eq!(result.document.len(), 2, "{after}");
    assert!(
        result.document.nodes().all(|node| node.parent().is_none()),
        "{after}"
    );
}

#[test]
fn grouping_in_a_loop_falls_back() {
    let source = "comp(name=\"P\", duration=1.0)\nfor i in range(2):\n    c = circle(10 + i)\n";
    let execution = compiled(source);
    let id = node_id(&execution, 0);
    let change = StructuralCommand::Group {
        ids: vec![id],
        name: None,
    };
    let plan = plan_structural(source, &execution.origins, &execution.document, &change);
    assert!(matches!(plan, Plan::Fallback(_)), "{plan:?}");
}

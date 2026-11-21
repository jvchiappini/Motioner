//! Provenance, `.id()` and the overrides file at the DSL boundary (D-0022).

use motioner_core::{OverrideEntry, OverrideSet};
use motioner_dsl::{run, run_with_overrides, ErrorCategory, Limits, Severity};

const HELLO: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../examples/hello.motion"
));

fn limits() -> Limits {
    Limits::default()
}

fn identities(source: &str) -> Vec<String> {
    let document = run(source, &limits()).unwrap();
    document
        .nodes()
        .map(|node| node.identity().as_str().to_owned())
        .collect()
}

#[test]
fn provenance_keys_are_unique_and_stable() {
    let first = identities(HELLO);
    let second = identities(HELLO);
    assert_eq!(first, second, "the same program yields the same keys");
    let mut sorted = first.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), first.len(), "every node has its own key");
    // Loop-created dots carry their iteration index; function-created nodes
    // carry the call site of the user frame.
    assert!(
        first.iter().filter(|key| key.contains("/i")).count() >= 5,
        "{first:?}"
    );
    assert!(
        first.iter().any(|key| key.contains("/c")),
        "a node created inside spinner() records the call site: {first:?}"
    );
    assert!(first.iter().all(|key| key.starts_with('@')), "{first:?}");
}

#[test]
fn editing_unrelated_lines_shifts_keys_but_the_program_stays_self_consistent() {
    let shifted = format!("# a new comment line\n{HELLO}");
    let original = identities(HELLO);
    let moved = identities(&shifted);
    assert_ne!(original, moved, "byte offsets moved, as D-0022 documents");
    // Still one key per node and no collisions.
    let mut sorted = moved.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), moved.len());
}

#[test]
fn explicit_ids_win_are_unique_and_chainable() {
    let source = "comp(name=\"Ids\", duration=1.0)\n\
                  card = rect(10, 10).id(\"card\")\n\
                  card.pos = (5, 5)\n";
    let document = run(source, &limits()).unwrap();
    let node = document.node(document.roots()[0]).unwrap();
    assert_eq!(node.identity().as_str(), "card");
    assert!(node.identity().is_explicit());
    assert_eq!(document.find("card"), Some(node.id()));
    assert_eq!(
        node.transform().position.value(),
        motioner_core::Vec2::new(5.0, 5.0)
    );

    let duplicate = "comp(name=\"Ids\", duration=1.0)\n\
                     a = rect(1, 1).id(\"same\")\n\
                     b = rect(1, 1).id(\"same\")\n";
    let error = run(duplicate, &limits()).unwrap_err();
    assert_eq!(error.category, ErrorCategory::Value);
    assert!(error.message.contains("already used"), "{error}");

    let invalid = "comp(name=\"Ids\", duration=1.0)\nrect(1, 1).id(\"9bad\")\n";
    let error = run(invalid, &limits()).unwrap_err();
    assert!(error.message.contains("[A-Za-z_]"), "{error}");
    assert_eq!(error.span.line, 2, "the diagnostic points at the call");
}

#[test]
fn an_override_by_provenance_key_applies_and_leaves_run_unchanged() {
    let document = run(HELLO, &limits()).unwrap();
    let title = document.roots()[0];
    let key = document.node(title).unwrap().identity().as_str().to_owned();
    let overrides = format!("override({key:?}, \"rotation\", 45)\n");
    let compiled = run_with_overrides(HELLO, Some(&overrides), &limits()).unwrap();
    assert!(compiled.warnings.is_empty(), "{:?}", compiled.warnings);
    assert_eq!(compiled.overrides.len(), 1);
    let rotation = compiled
        .document
        .node(title)
        .unwrap()
        .transform()
        .rotation
        .value();
    assert!((rotation - 45.0).abs() < 1e-9);
    // Without the overrides file the program is unchanged.
    let plain = run(HELLO, &limits()).unwrap();
    assert!((plain.node(title).unwrap().transform().rotation.value()).abs() < 1e-9);
}

#[test]
fn orphans_are_warnings_with_the_statement_span() {
    let overrides = "override(\"ghost\", \"opacity\", 0.5)\n";
    let compiled = run_with_overrides(HELLO, Some(overrides), &limits()).unwrap();
    assert_eq!(compiled.warnings.len(), 1);
    let warning = &compiled.warnings[0];
    assert_eq!(warning.severity, Severity::Warning);
    assert_eq!(warning.span.line, 1);
    assert!(warning.message.contains("ghost"), "{warning}");
    assert!(!warning.is_error());
    assert_eq!(compiled.overrides.len(), 1, "the orphan is kept");
}

#[test]
fn kind_mismatches_are_warnings_too() {
    let document = run(HELLO, &limits()).unwrap();
    let title = document.roots()[0];
    let key = document.node(title).unwrap().identity().as_str().to_owned();
    let overrides = format!("override({key:?}, \"width\", 10)\n");
    let compiled = run_with_overrides(HELLO, Some(&overrides), &limits()).unwrap();
    assert_eq!(compiled.warnings.len(), 1);
    assert!(compiled.warnings[0].message.contains("width"));
}

#[test]
fn a_malformed_overrides_file_is_an_error_with_its_own_span() {
    let error = run_with_overrides(HELLO, Some("nope(\"a\")\n"), &limits()).unwrap_err();
    assert_eq!(error.severity, Severity::Error);
    assert_eq!(error.span.line, 1);
    assert!(error.message.starts_with("overrides:"));
}

#[test]
fn axis_keys_from_the_program_and_the_overrides_file() {
    // The program can key one axis; the other stays unkeyed.
    let source = "comp(name=\"Axis\", duration=2.0)\n\
                  card = rect(10, 10).id(\"card\")\n\
                  card.key(\"pos.x\", 0.5, 100)\n";
    let document = run(source, &limits()).unwrap();
    let node = document.find("card").unwrap();
    let position = &document.node(node).unwrap().transform().position;
    assert_eq!(position.axis(motioner_core::Axis::X).keys().len(), 1);
    assert!(position.axis(motioner_core::Axis::Y).keys().is_empty());
    assert!((position.axis(motioner_core::Axis::X).keys()[0].value - 100.0).abs() < 1e-9);

    // Overrides can key the other axis independently.
    let overrides = "override_key(\"card\", \"pos.y\", 1.0, 250)\n";
    let compiled = run_with_overrides(source, Some(overrides), &limits()).unwrap();
    assert!(compiled.warnings.is_empty(), "{:?}", compiled.warnings);
    let node = compiled.document.find("card").unwrap();
    let position = &compiled.document.node(node).unwrap().transform().position;
    assert_eq!(position.axis(motioner_core::Axis::X).keys().len(), 1);
    assert_eq!(position.axis(motioner_core::Axis::Y).keys().len(), 1);
    assert_eq!(position.axis(motioner_core::Axis::Y).keys()[0].frame, 60);
}

#[test]
fn serialising_and_parsing_round_trips() {
    let document = run(HELLO, &limits()).unwrap();
    let title = document.roots()[0];
    let key = document.node(title).unwrap().identity().as_str().to_owned();
    let mut set = OverrideSet::new();
    set.insert(OverrideEntry::new(
        key.clone(),
        "rotation",
        motioner_core::OverrideAction::Set(motioner_core::OverrideValue::Number(12.5)),
    ));
    set.insert(OverrideEntry::new(
        key.clone(),
        "opacity",
        motioner_core::OverrideAction::Key {
            frame: 30,
            value: motioner_core::OverrideValue::Number(0.25),
            ease: motioner_core::Ease::CubicInOut,
        },
    ));
    set.insert(OverrideEntry::new(
        key,
        "visible",
        motioner_core::OverrideAction::Visible(false),
    ));
    let rate = document.composition().frame_rate();
    let text = set.to_mdf(rate);
    let compiled = run_with_overrides(HELLO, Some(&text), &limits()).unwrap();
    assert!(compiled.warnings.is_empty(), "{:?}", compiled.warnings);
    assert_eq!(
        compiled.overrides.to_mdf(rate),
        text,
        "parse(to_mdf(set)) == set"
    );
}

//! The defect fixtures: every rule fires on its bad program and stays quiet on
//! the fixed one (C2 deliverable).

mod common;

use common::{build, Metrics, NoSampler, SolidSampler};
use motioner_analysis::{check_design, CheckOptions, Rules};
use motioner_core::Color;

/// `(rule, bad, good)` as shipped under `examples/design_defects/<rule>/`.
const RULES: &[&str] = &[
    "blank_frame",
    "contrast",
    "dead_time",
    "linear_ease",
    "off_screen",
    "overload",
    "peak_speed",
    "reading_time",
    "safe_area",
    "stagger",
    "text_on_busy",
    "text_overlap",
    "type_hierarchy",
    "velocity_jump",
];

fn source(rule: &str, kind: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/design_defects")
        .join(rule)
        .join(format!("{kind}.motion"));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()))
}

fn fires(rule: &str, source: &str) -> bool {
    let document = build(source);
    let options = CheckOptions::default();
    let report = if rule == "contrast" {
        // Contrast needs pixels: sample the white background of the fixture.
        let sampler = SolidSampler(Color::WHITE);
        check_design(&document, &options, &Metrics, Some(&sampler))
    } else {
        check_design(&document, &options, &Metrics, Some(&NoSampler))
    };
    report.issues.iter().any(|issue| issue.rule == rule)
}

#[test]
fn every_rule_has_a_bad_and_a_good_fixture() {
    for rule in RULES {
        assert!(
            Rules::spec(rule).is_some(),
            "{rule} is not in the rule table"
        );
        assert!(
            fires(rule, &source(rule, "bad")),
            "{rule} does not fire on bad"
        );
        assert!(
            !fires(rule, &source(rule, "good")),
            "{rule} still fires on the fixed program"
        );
    }
}

#[test]
fn the_fixture_count_matches_the_rule_table() {
    for rule in RULES {
        for kind in ["bad", "good"] {
            let text = source(rule, kind);
            assert!(
                text.contains("comp("),
                "{rule}/{kind} does not declare a composition"
            );
        }
    }
    assert_eq!(RULES.len(), motioner_analysis::rule_ids().len());
}

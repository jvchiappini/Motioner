//! Design analysis over tiny fixtures: describe, hit, sample, code, layout and
//! every `check_design` rule that can be triggered without pixels.

mod common;

use common::{build, Metrics, NoSampler, SolidSampler};
use motioner_analysis::{
    check_design, code_for_node, describe_frame, hit_test, layout_report, nodes_for_code,
    sample_property, Box2, CheckOptions, Rules, Severity,
};
use motioner_core::{Color, Vec2};
use motioner_dsl::{execute_detailed, parse, Limits};
use motioner_eval::{Evaluator, RenderPlan};

fn plan(document: &motioner_core::Document, frame: i64) -> RenderPlan {
    let mut evaluator = Evaluator::new();
    let mut plan = RenderPlan::new();
    evaluator.evaluate(document, frame, &mut plan);
    plan
}

const SCENE: &str = r##"
comp(name="Scene", width=1000, height=1000, fps=60, duration=2.0, background="#000000")
r = rect(200, 100, fill="#FF0000")
r.id("r")
r.pos = (500, 500)
t = text("Hello", "Inter", 40, "#FFFFFF")
t.id("t")
t.pos = (500, 800)
"##;

#[test]
fn describe_frame_reports_exact_boxes_and_text() {
    let document = build(SCENE);
    let scene = describe_frame(&document, &plan(&document, 0), 0, None, &Metrics);
    assert_eq!(scene.elements.len(), 2);
    let rect = scene.element(document.find("r").unwrap()).unwrap();
    assert_eq!(rect.kind, motioner_analysis::ElementKind::Shape);
    let bounds = rect.bounds.unwrap();
    assert!((bounds.min_x - 400.0).abs() < 1e-9, "{bounds:?}");
    assert!((bounds.max_y - 550.0).abs() < 1e-9, "{bounds:?}");
    let text = scene.element(document.find("t").unwrap()).unwrap();
    assert_eq!(text.as_text().unwrap().text, "Hello");
    assert_eq!(text.as_text().unwrap().words, 1);
}

#[test]
fn hit_test_returns_the_topmost_element_first() {
    let document = build(
        r##"
comp(name="Hit", width=1000, height=1000, fps=60, duration=1.0)
a = rect(200, 200, fill="#0000FF")
a.id("a")
a.pos = (500, 500)
b = rect(100, 100, fill="#00FF00")
b.id("b")
b.pos = (500, 500)
"##,
    );
    let scene = describe_frame(&document, &plan(&document, 0), 0, None, &Metrics).scene();
    let hits = hit_test(&scene, Vec2::new(500.0, 500.0));
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].id, document.find("b").unwrap());
    assert!(hits[0].z > hits[1].z);
    assert!(hit_test(&scene, Vec2::new(10.0, 10.0)).is_empty());
}

#[test]
fn sample_reads_transform_and_content_channels() {
    let document = build(
        r##"
comp(name="Sample", width=1000, height=1000, fps=60, duration=1.0)
r = rect(200, 100, fill="#FF0000")
r.id("r")
r.key("pos.x", 0.0, 0.0)
r.key("pos.x", 1.0, 600.0, ease="linear")
r.key("opacity", 0.0, 1.0)
r.key("opacity", 0.5, 0.0, ease="linear")
"##,
    );
    let id = document.find("r").unwrap();
    assert_eq!(sample_property(&document, id, "pos.x", 0), Some(0.0));
    assert_eq!(sample_property(&document, id, "pos.x", 60), Some(600.0));
    assert_eq!(sample_property(&document, id, "opacity", 0), Some(1.0));
    assert_eq!(sample_property(&document, id, "width", 0), Some(200.0));
    assert_eq!(sample_property(&document, id, "fill", 0), None);
}

#[test]
fn code_mapping_round_trips_between_node_and_line() {
    let program = parse(SCENE).unwrap();
    let execution = execute_detailed(&program, &Limits::default()).unwrap();
    let rect = execution.document.find("r").unwrap();
    let location = code_for_node(&execution.origins, rect).expect("rect has origins");
    let nodes = nodes_for_code(&execution.origins, location.creation_line);
    assert!(nodes.contains(&rect), "{nodes:?}");
}

#[test]
fn layout_finds_overlaps_and_safe_area_violations() {
    let document = build(
        r##"
comp(name="Layout", width=1000, height=1000, fps=60, duration=1.0)
a = text("Overlap one", "Inter", 40, "#FFFFFF")
a.id("a")
a.pos = (500, 500)
b = text("Overlap two", "Inter", 40, "#FFFFFF")
b.id("b")
b.pos = (520, 505)
c = text("Edge", "Inter", 40, "#FFFFFF")
c.id("c")
c.pos = (10, 800)
"##,
    );
    let scene = describe_frame(&document, &plan(&document, 0), 0, None, &Metrics).scene();
    let report = layout_report(&scene, &Rules::default());
    assert!(!report.text_overlaps.is_empty(), "{report:?}");
    assert!(
        report
            .safe_violations
            .iter()
            .any(|violation| violation.id == document.find("c").unwrap()),
        "{report:?}"
    );
}

fn issues(source: &str, sampler: Option<&dyn motioner_analysis::BackgroundSample>) -> Vec<String> {
    let document = build(source);
    let report = check_design(&document, &CheckOptions::default(), &Metrics, sampler);
    report
        .issues
        .iter()
        .map(|issue| issue.rule.to_owned())
        .collect()
}

#[test]
fn contrast_fires_only_below_aa() {
    let source = r##"
comp(name="C", width=1000, height=1000, fps=60, duration=3.0, background="#FFFFFF")
t = text("Contrast", "Inter", 40, "#FFFFFF")
t.id("t")
t.pos = (100, 500)
"##;
    let white = SolidSampler(Color::WHITE);
    assert!(issues(source, Some(&white)).contains(&"contrast".to_owned()));
    let black = SolidSampler(Color::BLACK);
    assert!(!issues(source, Some(&black)).contains(&"contrast".to_owned()));
    // No sampler, no contrast claim.
    assert!(!issues(source, Some(&NoSampler)).contains(&"contrast".to_owned()));
}

#[test]
fn reading_time_fires_for_a_long_line_that_leaves_too_soon() {
    let source = r##"
comp(name="R", width=1000, height=1000, fps=60, duration=1.0)
t = text("one two three four five six", "Inter", 40, "#FFFFFF")
t.id("t")
t.pos = (100, 500)
"##;
    assert!(issues(source, None).contains(&"reading_time".to_owned()));
}

#[test]
fn velocity_jump_and_peak_speed_are_measured_from_the_curves() {
    let jump = r##"
comp(name="J", width=1000, height=1000, fps=60, duration=2.0)
r = rect(100, 100, fill="#FF0000")
r.id("r")
r.key("pos.x", 0.0, 100.0)
r.key("pos.x", 0.1666, 200.0, ease="linear")
r.key("pos.x", 0.3333, 900.0, ease="linear")
"##;
    assert!(issues(jump, None).contains(&"velocity_jump".to_owned()));

    let fast = r##"
comp(name="F", width=1000, height=1000, fps=60, duration=2.0)
r = rect(100, 100, fill="#FF0000")
r.id("r")
r.key("pos.x", 0.0, 100.0)
r.key("pos.x", 0.0833, 900.0, ease="linear")
"##;
    assert!(issues(fast, None).contains(&"peak_speed".to_owned()));
}

#[test]
fn a_composition_with_nothing_drawn_is_a_blank_frame_error() {
    let source = "comp(name=\"Blank\", width=1000, height=1000, fps=60, duration=1.0)\n";
    let document = build(source);
    let report = check_design(&document, &CheckOptions::default(), &Metrics, None);
    assert!(report.errors > 0);
    assert!(report
        .issues
        .iter()
        .any(|issue| issue.rule == "blank_frame"));
}

#[test]
fn a_well_composed_scene_has_no_errors() {
    let source = r##"
comp(name="Good", width=1000, height=1000, fps=60, duration=3.0, background="#101010")
t = text("Title", "Inter", 64, "#FFFFFF", "center")
t.id("t")
t.pos = (500, 420)
s = text("A short subtitle line", "Inter", 32, "#CCCCCC", "center")
s.id("s")
s.pos = (500, 520)
t.key("opacity", 0.0, 0.0)
t.key("opacity", 0.6, 1.0, ease="quad_out")
s.key("opacity", 0.2, 0.0)
s.key("opacity", 0.8, 1.0, ease="quad_out")
"##;
    let sampler = SolidSampler(Color::BLACK);
    let document = build(source);
    let report = check_design(
        &document,
        &CheckOptions::default(),
        &Metrics,
        Some(&sampler),
    );
    assert_eq!(report.errors, 0, "{:?}", report.issues);
    assert!(report
        .issues
        .iter()
        .all(|issue| issue.severity != Severity::Error));
}

#[test]
fn rule_ids_are_unique_and_documented() {
    let ids = motioner_analysis::rule_ids();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(ids.len(), sorted.len());
    for id in ids {
        assert!(Rules::spec(id).is_some(), "{id}");
    }
}

#[test]
fn box_math_is_shared_by_every_report() {
    let box_ = Box2::new(0.0, 0.0, 10.0, 10.0).unwrap();
    assert!(box_.contains(Vec2::new(10.0, 10.0)));
    assert!(!box_.contains(Vec2::new(10.1, 0.0)));
}

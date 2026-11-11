//! Standard-library prelude: themes, beats, layout, scenes and presets.

use motioner_core::{Document, NodeContent, TextBounds, TextMeasurement, TextMetrics, TextRequest};
use motioner_dsl::{run_with_metrics, Limits};

const COMP: &str = "comp(name=\"Std\", width=1000, height=500, fps=60, duration=10.0)\n";

struct FixedMetrics;

impl TextMetrics for FixedMetrics {
    fn measure(&self, request: &TextRequest<'_>) -> Option<TextMeasurement> {
        Some(TextMeasurement {
            width: request.size * f64::from(u32::try_from(request.text.len()).unwrap_or(0)) * 0.5,
            line_count: 1,
            height: request.size,
            ink: TextBounds {
                left: 0.0,
                top: -request.size,
                right: request.size,
                bottom: 0.0,
            },
            line_advances: vec![request.size],
        })
    }
}

fn build(body: &str) -> Document {
    let source = format!("{COMP}{body}");
    run_with_metrics(&source, &Limits::default(), &FixedMetrics)
        .unwrap_or_else(|error| panic!("{}", error.render(&source, "std")))
}

#[test]
fn themes_resolve_colour_and_type_tokens() {
    let document = build(
        "use_theme(theme(\"dark\"))\nr = rect(100, 100, fill=\"accent\")\nt = text(\"Hi\", style=\"h1\")\n",
    );
    let rect = document.nodes().next().unwrap();
    let NodeContent::Rect { paint, .. } = rect.content() else {
        panic!("expected a rect");
    };
    let fill = paint
        .fill
        .value()
        .expect("fill should resolve to the theme accent");
    assert_eq!(fill.to_hex(), "#6C8CFF");
    let text = document.nodes().nth(1).unwrap();
    let NodeContent::Text(text) = text.content() else {
        panic!("expected text");
    };
    // The type scale is authored for 1080p and scaled by the composition.
    assert!((text.size.value() - 72.0 * 500.0 / 1080.0).abs() < 1e-9);
}

#[test]
fn unknown_tokens_report_the_nearest_name() {
    let source = format!("{COMP}r = rect(10, 10, fill=\"acent\")\n");
    let error = run_with_metrics(&source, &Limits::default(), &FixedMetrics).unwrap_err();
    assert!(error.message.contains("accent"), "{}", error.message);
}

#[test]
fn comp_theme_applies_before_background_tokens_resolve() {
    let source = "comp(name=\"T\", width=1000, height=500, theme=\"light\", background=\"bg\")\nr = rect(10, 10, fill=\"accent\")\n";
    let document =
        run_with_metrics(source, &Limits::default(), &FixedMetrics).expect("theme should apply");
    assert_eq!(document.composition().background().to_hex(), "#F7F8FC");
    let NodeContent::Rect { paint, .. } = document.nodes().next().unwrap().content() else {
        panic!("expected a rect");
    };
    assert_eq!(
        paint
            .fill
            .value()
            .expect("the accent token should resolve")
            .to_hex(),
        "#3D5AFE"
    );
}

#[test]
fn comp_tempo_and_beat_convert_to_seconds() {
    let document = build("n = rect(10, 10)\nn.pos = (beat(2), bar(1))\n");
    let node = document.nodes().next().unwrap();
    assert!((node.transform().position.value().x - 1.0).abs() < 1e-9);
    assert!((node.transform().position.value().y - 2.0).abs() < 1e-9);
}

#[test]
fn scenes_compile_to_hold_keys_and_expose_local_time() {
    let document = build(
        "s = scene(\"intro\", 1.0, 2.0)\nr = rect(10, 10)\ns.add([r])\na = s.t(0.5)\nr.pos = (a, 0)\n",
    );
    let group = document.nodes().next().unwrap();
    assert_eq!(group.name(), Some("intro"));
    let opacity = group.transform().opacity.keys();
    assert_eq!(opacity.len(), 3);
    assert!(opacity[0].value.abs() < 1e-9);
    assert!((opacity[1].value - 1.0).abs() < 1e-9);
    assert!(opacity[2].value.abs() < 1e-9);
    let child = document.nodes().nth(1).unwrap();
    assert!((child.transform().position.value().x - 1.5).abs() < 1e-9);
}

#[test]
fn row_layout_measures_and_centres_children() {
    let document = build(
        "a = rect(100, 50)\nb = rect(200, 50)\nc = rect(100, 50)\nrow([a, b, c], gap=\"m\", align=\"center\")\n",
    );
    let x: Vec<f64> = document
        .nodes()
        .map(|node| node.transform().position.value().x)
        .collect();
    // The `m` gap is 24 px at 1080p, scaled to 11.11 px at a 500 px
    // composition: main = 422.22, so the row starts at 288.89.
    assert!((x[0] - 338.888_888_888_888_9).abs() < 1e-9, "{x:?}");
    assert!((x[1] - 500.0).abs() < 1e-9, "{x:?}");
    assert!((x[2] - 661.111_111_111_111_1).abs() < 1e-9, "{x:?}");
}

#[test]
fn place_centres_a_node_in_the_composition() {
    let document = build("r = rect(100, 100)\nr.place(\"center\")\n");
    let node = document.nodes().next().unwrap();
    assert!((node.transform().position.value().x - 500.0).abs() < 1e-9);
    assert!((node.transform().position.value().y - 250.0).abs() < 1e-9);
    let bounds = document.nodes().next().unwrap();
    let NodeContent::Rect { .. } = bounds.content() else {
        panic!("expected rect");
    };
}

#[test]
fn motion_presets_emit_keys_with_theme_eases() {
    let document = build(
        "use_theme(theme(\"dark\"))\nt = text(\"Title\", style=\"h1\")\nt.enter(\"rise\", at=1.0, duration=2.0)\n",
    );
    let node = document.nodes().next().unwrap();
    let y_keys = node
        .transform()
        .position
        .axis(motioner_core::Axis::Y)
        .keys();
    assert_eq!(y_keys.len(), 2);
    // 2 beats at 120 bpm = 1 s, so the second key lands at 2 s (60 fps).
    assert_eq!(y_keys[1].frame, 120);
    let opacity = node.transform().opacity.keys();
    assert_eq!(opacity.len(), 2);
}

#[test]
fn components_return_a_named_group_with_parts() {
    let document = build("c = card(\"Title\", \"Body\")\nc.pos = (500, 250)\n");
    let group = document.nodes().next().unwrap();
    let NodeContent::Group = group.content() else {
        panic!("expected a group");
    };
    assert_eq!(group.name(), Some("card"));
    assert_eq!(group.children().len(), 3);
    assert!(document
        .nodes()
        .any(|node| node.name() == Some("card.title")));
}

#[test]
fn a_user_definition_shadows_the_library() {
    // The library is never walked into the global scope, so the only way this
    // passes is if lookup prefers the user's own definition (prompt 13b, C0).
    let source =
        "def std_version():\n    return 99\ncomp(name=str(std_version()), width=100, height=100)\n";
    let document = run_with_metrics(source, &Limits::default(), &FixedMetrics).unwrap();
    assert_eq!(document.composition().name(), "99");
}

#[test]
fn library_functions_are_bound_without_executing_the_prelude() {
    let source = "comp(name=\"Cached\", width=100, height=100)\nr = rect(10, 10)\n";
    let parsed = motioner_dsl::parse_with_prelude(source).unwrap();
    assert!(parsed.library_len() > 0);
    assert_eq!(parsed.user_body().len(), 2);
    let execution =
        motioner_dsl::execute_parsed_with_metrics(&parsed, &Limits::default(), &FixedMetrics)
            .expect("the cached prelude path must run");
    assert_eq!(execution.document.len(), 1);
}
#[test]
fn count_up_switches_one_value_per_step_with_hold_keys() {
    let document = build("c = count_up(100, at=0.0, duration=4.0, steps=10)\nc.pos = (500, 250)\n");
    let group = document.nodes().next().unwrap();
    assert_eq!(group.name(), Some("count_up"));
    assert_eq!(group.children().len(), 10);
    let values: Vec<&str> = document
        .nodes()
        .filter(|node| node.name() == Some("count_up.value"))
        .filter_map(|node| match node.content() {
            NodeContent::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(values.len(), 10);
    assert_eq!(values[0], "0");
    assert_eq!(values[9], "100");
    let first = document
        .nodes()
        .find(|node| node.name() == Some("count_up.value"))
        .unwrap();
    assert_eq!(first.transform().opacity.keys().len(), 2);
    assert_eq!(
        first.transform().opacity.keys()[0].ease,
        motioner_core::Ease::Hold
    );
}

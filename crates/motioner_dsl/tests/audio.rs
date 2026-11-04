//! The `audio()` constructor and the audio channels (D-0048).

use motioner_core::NodeContent;
use motioner_dsl::{run, Limits};

const SOURCE: &str = r#"
comp("A", 1280, 720, 60, 2.0)
music = audio("tone.wav", in_point=0.5, out_point=1.5, gain=0.5, start=12,
              pan=-0.5, fade_in=0.2, fade_out=0.3)
music.key("gain", 30, 0.25)
music.id("music")
"#;

#[test]
fn audio_parses_every_reserved_parameter_and_is_not_drawn() {
    let document = run(SOURCE, &Limits::default()).expect("audio program");
    let node = document
        .nodes()
        .find(|node| node.identity().as_str() == "music")
        .expect("audio node");
    let NodeContent::Audio(audio) = node.content() else {
        panic!("expected an audio node");
    };
    assert_eq!(audio.source, "tone.wav");
    assert!((audio.in_point.value() - 0.5).abs() < 1e-9);
    assert!((audio.out_point.value() - 1.5).abs() < 1e-9);
    assert!((audio.gain.value() - 0.5).abs() < 1e-9);
    assert!((audio.pan.value() - -0.5).abs() < 1e-9);
    assert_eq!(audio.start, 12);
    assert!((audio.fade_in - 0.2).abs() < 1e-9);
    assert!((audio.fade_out - 0.3).abs() < 1e-9);
    assert_eq!(audio.gain.keys().len(), 1, "gain is keyable");
    let mut plan = motioner_eval::RenderPlan::new();
    motioner_eval::Evaluator::new().evaluate(&document, 30, &mut plan);
    assert!(plan.items().is_empty(), "audio contributes no draw items");
}

#[test]
fn audio_requires_a_source() {
    let error = run(
        "comp(\"A\", 100, 100, 60, 1)\naudio(\"\")",
        &Limits::default(),
    )
    .expect_err("empty source");
    assert!(error.to_string().contains("source"), "{error}");
}

//! Phase 9 timing: bezier/hold/new eases, motion-blur settings and the
//! `stagger` / `wiggle` helpers.

use motioner_core::{Ease, OverrideAction, OverrideEntry, OverrideSet, OverrideValue};
use motioner_dsl::{run, run_with_overrides, Limits};

fn program(body: &str) -> String {
    format!("comp(name=\"T\", width=320, height=180, fps=60, duration=10)\n{body}")
}

#[test]
fn comp_parses_motion_blur_settings_and_rejects_bad_values() {
    let document = run(&program(""), &Limits::default()).unwrap();
    let composition = document.composition();
    assert!(!composition.motion_blur());
    assert!((composition.shutter() - 180.0).abs() < 1e-9);
    assert_eq!(composition.samples(), 8);

    let document = run(
        "comp(name=\"T\", width=320, height=180, fps=60, duration=10, motion_blur=True, shutter=90, samples=4)\n",
        &Limits::default(),
    )
    .unwrap();
    assert!(document.composition().motion_blur());
    assert!((document.composition().shutter() - 90.0).abs() < 1e-9);
    assert_eq!(document.composition().samples(), 4);

    for bad in [
        "comp(name=\"T\", width=320, height=180, motion_blur=True, shutter=0)\n",
        "comp(name=\"T\", width=320, height=180, motion_blur=True, shutter=361)\n",
        "comp(name=\"T\", width=320, height=180, motion_blur=True, samples=0)\n",
        "comp(name=\"T\", width=320, height=180, motion_blur=True, samples=65)\n",
    ] {
        assert!(run(bad, &Limits::default()).is_err(), "{bad}");
    }
}

#[test]
fn bezier_and_hold_keys_parse_and_keep_their_ease() {
    let source = program(
        "r = rect(20, 20, fill=\"#FFFFFF\")\n\
         r.key(\"pos.x\", 0.0, 0, ease=bezier(0.25, 0.1, 0.25, 1))\n\
         r.key(\"pos.x\", 1.0, 100, ease=\"hold\")\n",
    );
    let document = run(&source, &Limits::default()).unwrap();
    let node = document.roots()[0];
    let keys = document
        .node(node)
        .unwrap()
        .transform()
        .position
        .axis(motioner_core::Axis::X)
        .keys();
    assert_eq!(keys.len(), 2);
    assert_eq!(keys[1].ease, Ease::Hold);
    assert!(!keys[1].ease.is_bezier());
    assert!(keys[0].ease.is_bezier());
    assert_eq!(keys[0].ease.mdf_value(), "bezier(0.25, 0.1, 0.25, 1)");
}

#[test]
fn a_bad_bezier_is_a_span_error() {
    let error = run(
        &program("r = rect(1, 1)\nr.key(\"opacity\", 0, 1, ease=bezier(-0.5, 0, 1, 1))\n"),
        &Limits::default(),
    )
    .unwrap_err();
    assert!(error.message.contains("bezier"), "{}", error.message);
    assert!(error.span.line >= 2, "the error carries a span: {error:?}");
}

#[test]
fn stagger_emits_two_keys_per_node() {
    let source = program(
        r##"bars = [rect(10, 10, fill="#FFFFFF"), rect(10, 10, fill="#FFFFFF"), rect(10, 10, fill="#FFFFFF")]
stagger(bars, "pos.x", [(0, 100)], 0.5, 0.1, 0.4, ease="quad_out")
"##,
    );
    let document = run(&source, &Limits::default()).unwrap();
    let mut all = Vec::new();
    for node in document.nodes() {
        let frames: Vec<_> = node
            .transform()
            .position
            .axis(motioner_core::Axis::X)
            .keys()
            .iter()
            .map(|key| key.frame)
            .collect();
        if !frames.is_empty() {
            all.push(frames);
        }
    }
    assert_eq!(all.len(), 3);
    assert_eq!(all[0], vec![30, 54]);
    assert_eq!(all[1], vec![36, 60]);
    assert_eq!(all[2], vec![42, 66]);
}

#[test]
fn wiggle_is_deterministic_for_one_seed() {
    let source = program(
        "b = circle(10, fill=\"#FFFFFF\")\n\
         b.wiggle(\"pos.x\", 7, 4, 20, 0, 1)\n",
    );
    let first = run(&source, &Limits::default()).unwrap();
    let second = run(&source, &Limits::default()).unwrap();
    let keys = |document: &motioner_core::Document| {
        document
            .node(document.roots()[0])
            .unwrap()
            .transform()
            .position
            .axis(motioner_core::Axis::X)
            .keys()
            .to_vec()
    };
    assert_eq!(keys(&first), keys(&second));
    assert_eq!(keys(&first).len(), 5);
    assert!(keys(&first).iter().all(|key| key.ease == Ease::SineInOut));
    // A different seed moves the values.
    let other = run(
        &program("b = circle(10, fill=\"#FFFFFF\")\nb.wiggle(\"pos.x\", 8, 4, 20, 0, 1)\n"),
        &Limits::default(),
    )
    .unwrap();
    assert_ne!(keys(&first), keys(&other));
}

#[test]
fn every_ease_round_trips_through_the_overrides_file() {
    let source = program("r = rect(20, 20, fill=\"#FFFFFF\").id(\"r\")\n");
    for ease in [
        Ease::Linear,
        Ease::Hold,
        Ease::ElasticOut,
        Ease::BounceOut,
        Ease::ExpoInOut,
        Ease::SineInOut,
        Ease::bezier(0.25, 0.1, 0.25, 1.0).unwrap(),
    ] {
        let mut set = OverrideSet::new();
        set.insert(OverrideEntry::new(
            "r",
            "pos.x",
            OverrideAction::Key {
                frame: 30,
                value: OverrideValue::Number(20.0),
                ease,
            },
        ));
        let text = set.to_mdf(motioner_core::FrameRate::new(60, 1).unwrap());
        assert!(
            text.contains(&format!("ease={}", ease.mdf_literal())),
            "{text}"
        );
        let compiled = run_with_overrides(&source, Some(&text), &Limits::default()).unwrap();
        assert!(compiled.warnings.is_empty(), "{:?}", compiled.warnings);
        let node = compiled.document.roots()[0];
        let keys = compiled
            .document
            .node(node)
            .unwrap()
            .transform()
            .position
            .axis(motioner_core::Axis::X)
            .keys();
        assert_eq!(keys[0].ease, ease, "{text}");
    }
}

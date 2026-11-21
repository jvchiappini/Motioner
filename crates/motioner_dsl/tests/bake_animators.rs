//! Bake round-trip for per-character animators (D-0034).
//!
//! A `text_animators` override is folded into `animate_chars(...)` calls.  The
//! baked program must evaluate to exactly the same render plan as the program
//! plus the override, for every frame of the animation.

use motioner_core::TextAnimator;
use motioner_eval::{Evaluator, RenderPlan};

const SOURCE: &str = "\
comp(name=\"Animators\", width=320, height=180, duration=1.0)
title = text(\"AV\", font=\"Inter\", size=48)
title.id(\"title\")
";

const OVERRIDES: &str = "\
override(\"title\", \"text_animators\", \"opacity:0:1:0:0:60:quad_out:forward;color:#FFFFFF:#5AB0FF:0:0:60:linear:center\")
";

fn plan(
    source: &str,
    overrides: Option<&str>,
    frame: i64,
) -> (motioner_core::Document, RenderPlan) {
    let compiled =
        motioner_dsl::run_with_overrides(source, overrides, &motioner_dsl::Limits::default())
            .expect("program compiles");
    let mut plan = RenderPlan::new();
    Evaluator::new().evaluate(&compiled.document, frame, &mut plan);
    (compiled.document, plan)
}

#[test]
fn bake_writes_animate_chars_and_the_plan_is_identical() {
    let compiled =
        motioner_dsl::run_with_overrides(SOURCE, Some(OVERRIDES), &motioner_dsl::Limits::default())
            .expect("program plus overrides compiles");
    let node = compiled.document.find("title").expect("the title node");
    assert_eq!(
        compiled.document.text_animators(node).map(<[_]>::len),
        Some(2)
    );

    let outcome = motioner_dsl::bake(
        SOURCE,
        &compiled.document,
        &compiled.bindings,
        &compiled.origins,
        &compiled.overrides,
        &[],
    );
    assert!(outcome.refused.is_empty(), "{:?}", outcome.refused);
    assert!(outcome.overrides.is_empty(), "every override was baked");
    assert_eq!(outcome.baked, vec!["title".to_owned()]);
    assert!(
        outcome.program.contains("title.clear_chars()"),
        "the replacement clears the authored list first:\n{}",
        outcome.program
    );
    assert!(
        outcome.program.contains("title.animate_chars(\"opacity\""),
        "the opacity animator is emitted:\n{}",
        outcome.program
    );
    assert!(
        outcome.program.contains("title.animate_chars(\"color\""),
        "the colour animator is emitted:\n{}",
        outcome.program
    );

    // The baked program, with no overrides left, is the same document.
    let baked = motioner_dsl::run(&outcome.program, &motioner_dsl::Limits::default())
        .expect("the baked program compiles");
    let baked_node = baked.find("title").expect("the title node");
    let expected = compiled.document.text_animators(node).expect("animators");
    let actual = baked.text_animators(baked_node).expect("animators");
    assert_eq!(actual, expected);

    // And the same plan at every frame the animators cover.
    for frame in [0_i64, 1, 2, 6, 12, 20, 30] {
        let (_, before) = plan(SOURCE, Some(OVERRIDES), frame);
        let (_, after) = plan(&outcome.program, None, frame);
        assert_eq!(before, after, "frame {frame} must render identically");
    }
}

#[test]
fn an_empty_animator_list_clears_the_authored_ones() {
    let source = "\
comp(name=\"Clear\", width=320, height=180, duration=1.0)
title = text(\"AV\", font=\"Inter\", size=48)
title.id(\"title\")
title.animate_chars(\"opacity\", 0, 1, 0, 0.05, 0.2)
";
    let overrides = "override(\"title\", \"text_animators\", \"\")\n";
    let compiled =
        motioner_dsl::run_with_overrides(source, Some(overrides), &motioner_dsl::Limits::default())
            .expect("compiles");
    let node = compiled.document.find("title").expect("the title node");
    assert!(
        compiled
            .document
            .text_animators(node)
            .is_some_and(<[_]>::is_empty),
        "the empty override clears the authored animator"
    );

    let outcome = motioner_dsl::bake(
        source,
        &compiled.document,
        &compiled.bindings,
        &compiled.origins,
        &compiled.overrides,
        &[],
    );
    assert!(outcome.refused.is_empty(), "{:?}", outcome.refused);
    let baked = motioner_dsl::run(&outcome.program, &motioner_dsl::Limits::default())
        .expect("the baked program compiles");
    let baked_node = baked.find("title").expect("the title node");
    assert!(baked
        .text_animators(baked_node)
        .is_some_and(<[_]>::is_empty));
}

#[test]
fn animator_text_round_trips() {
    let animator = TextAnimator::new(
        motioner_core::CharProperty::Offset,
        motioner_core::CharValue::Vec2(motioner_core::Vec2::new(0.0, 24.5)),
        motioner_core::CharValue::Vec2(motioner_core::Vec2::new(0.0, 0.0)),
        6,
        2.0,
        12.0,
        motioner_core::Ease::QuadOut,
        motioner_core::CharOrder::Center,
    )
    .unwrap();
    let encoded = animator.encode();
    assert_eq!(TextAnimator::decode(&encoded).unwrap(), animator);
    let list = TextAnimator::encode_list(&[animator]);
    assert_eq!(TextAnimator::decode_list(&list).unwrap(), vec![animator]);
}

#[test]
fn a_malformed_animator_names_the_bad_field() {
    let error = TextAnimator::decode_list("opacity:0:1:0:2").unwrap_err();
    assert!(error.contains('8'), "{error}");
    let error = TextAnimator::decode_list("opacity:0:1:0:2:12:nope:forward").unwrap_err();
    assert!(error.contains("nope"), "{error}");
    let error = TextAnimator::decode_list("opacity:0:1:0:2:0:linear:forward").unwrap_err();
    assert!(error.contains("duration"), "{error}");
}

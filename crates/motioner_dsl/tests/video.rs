//! The `video()` constructor and its clip fields.

use motioner_core::NodeContent;
use motioner_dsl::{run, Limits};

const SOURCE: &str = r#"
comp("V", 1280, 720, 60, 3.0)
clip = video("clip.mp4", in_point=0.2, out_point=1.8, start=30, speed=1.5,
             width=1280, height=720, fit="cover", gain=0.8)
clip.id("clip")
"#;

#[test]
fn video_parses_every_reserved_parameter_and_draws_like_an_image() {
    let document = run(SOURCE, &Limits::default()).expect("video program");
    let node = document
        .nodes()
        .find(|node| node.identity().as_str() == "clip")
        .expect("video node");
    let NodeContent::Video(video) = node.content() else {
        panic!("expected a video node");
    };
    assert_eq!(video.source, "clip.mp4");
    assert!((video.in_point.value() - 0.2).abs() < 1e-9);
    assert!((video.out_point.value() - 1.8).abs() < 1e-9);
    assert_eq!(video.start, 30);
    assert!((video.speed - 1.5).abs() < 1e-9);
    assert!((video.width.value() - 1280.0).abs() < 1e-9);
    assert!((video.height.value() - 720.0).abs() < 1e-9);
    assert!((video.gain.value() - 0.8).abs() < 1e-9);
    assert_eq!(video.fit, motioner_core::ImageFit::Cover);

    let mut plan = motioner_eval::RenderPlan::new();
    motioner_eval::Evaluator::new().evaluate(&document, 30, &mut plan);
    assert_eq!(plan.items().len(), 1, "video draws one image item");
}

#[test]
fn video_rejects_a_non_positive_speed() {
    let error = run(
        "comp(\"V\", 100, 100, 60, 1)\nvideo(\"clip.mp4\", speed=0)",
        &Limits::default(),
    )
    .expect_err("speed must be positive");
    assert!(error.to_string().contains("speed"));
}

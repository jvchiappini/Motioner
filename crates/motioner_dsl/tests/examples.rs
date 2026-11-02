//! Integration test: the shipped example program executes through the
//! public API and builds exactly the document it describes.

use motioner_dsl::{run, Limits};

#[test]
fn hello_example_executes_with_the_documented_document() {
    let source = include_str!("../../../examples/hello.motion");
    let document = run(source, &Limits::default())
        .unwrap_or_else(|error| panic!("{}", error.render(source, "examples/hello.motion")));

    let composition = document.composition();
    assert_eq!((composition.width(), composition.height()), (1920, 1080));
    assert_eq!(
        (
            composition.frame_rate().numerator(),
            composition.frame_rate().denominator()
        ),
        (60, 1)
    );
    assert_eq!(composition.duration(), 300);

    // Counted by hand in examples/hello.motion:
    //   1 title text
    // + 5 circles from `for i in range(5)`
    // + 2 calls to spinner(), each creating a group and a rect
    // = 10 nodes.
    assert_eq!(document.len(), 10);
}

#[test]
fn typography_example_executes() {
    let source = include_str!("../../../examples/typography.motion");
    let document = run(source, &Limits::default())
        .unwrap_or_else(|error| panic!("{}", error.render(source, "examples/typography.motion")));
    assert_eq!(document.len(), 3);
}

#[test]
fn showcase_example_executes_with_metrics() {
    struct Fixed;
    impl motioner_core::TextMetrics for Fixed {
        fn measure(
            &self,
            _request: &motioner_core::TextRequest<'_>,
        ) -> Option<motioner_core::TextMeasurement> {
            Some(motioner_core::TextMeasurement {
                width: 100.0,
                line_count: 1,
                height: 100.0,
                ink: motioner_core::TextBounds::default(),
                line_advances: vec![100.0],
            })
        }
    }
    let source = include_str!("../../../examples/showcase.motion");
    motioner_dsl::run_with_metrics(source, &Limits::default(), &Fixed)
        .unwrap_or_else(|error| panic!("{}", error.render(source, "examples/showcase.motion")));
}

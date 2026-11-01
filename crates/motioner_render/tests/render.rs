//! GPU integration tests for the Motioner render adapter.
//!
//! Every test creates a real headless device. When the engine reports
//! [`RenderError::NoAdapter`] the test prints the reason and returns, so
//! CPU-only machines pass without `#[ignore]`. Any other creation error fails.

use motioner_core::{
    Color, Composition, Document, Frame, FrameRate, NodeContent, NodeId, Paint, Property,
    PropertyValue, Stroke, TextAlign, TextContent, Vec2,
};
use motioner_eval::{Evaluator, RenderPlan};
use motioner_render::{FrameRenderer, FrameStats, RenderError};
use sha2::{Digest, Sha256};

const BLACK: Color = Color::BLACK;
const WHITE: Color = Color::WHITE;
const RED: Color = Color::rgb(255, 0, 0);
const GREEN: Color = Color::rgb(0, 255, 0);
const BLUE: Color = Color::rgb(0, 0, 255);

fn composition(width: u32, height: u32, background: Color) -> Document {
    let rate = FrameRate::new(60, 1).unwrap();
    Document::new(Composition::new("Test", width, height, rate, 60, background).unwrap())
}

fn add_rect(document: &mut Document, width: f64, height: f64, radius: f64, paint: Paint) -> NodeId {
    document
        .add_node(NodeContent::rect(width, height, radius, paint))
        .unwrap()
}

fn add_text(
    document: &mut Document,
    text: &str,
    size: f64,
    color: Color,
    align: TextAlign,
) -> NodeId {
    document
        .add_node(NodeContent::Text(TextContent::new(
            text, "Roboto", size, color, align,
        )))
        .unwrap()
}

fn filled(color: Color) -> Paint {
    Paint {
        fill: Some(color),
        stroke: None,
    }
}

fn stroked(color: Color, width: f64) -> Paint {
    Paint {
        fill: None,
        stroke: Some(Stroke { color, width }),
    }
}

fn set_position(document: &mut Document, node: NodeId, x: f64, y: f64) {
    document
        .set_property(
            node,
            Property::Position,
            PropertyValue::Vec2(Vec2::new(x, y)),
        )
        .unwrap();
}

fn set_scale(document: &mut Document, node: NodeId, scale: f64) {
    document
        .set_property(node, Property::Scale, PropertyValue::Scalar(scale))
        .unwrap();
}

fn set_rotation(document: &mut Document, node: NodeId, degrees: f64) {
    document
        .set_property(node, Property::Rotation, PropertyValue::Scalar(degrees))
        .unwrap();
}

fn set_opacity(document: &mut Document, node: NodeId, opacity: f64) {
    document
        .set_property(node, Property::Opacity, PropertyValue::Scalar(opacity))
        .unwrap();
}

fn evaluate(document: &Document, frame: Frame) -> RenderPlan {
    let mut evaluator = Evaluator::new();
    let mut plan = RenderPlan::new();
    evaluator.evaluate(document, frame, &mut plan);
    plan
}

fn try_renderer(width: u32, height: u32) -> Option<FrameRenderer> {
    match FrameRenderer::new(width, height) {
        Ok(renderer) => Some(renderer),
        Err(RenderError::NoAdapter { message }) => {
            eprintln!("skipping GPU test: no usable adapter ({message})");
            None
        }
        Err(other) => panic!("renderer creation failed: {other}"),
    }
}

fn render(
    renderer: &mut FrameRenderer,
    document: &Document,
    plan: &RenderPlan,
) -> (Vec<u8>, FrameStats) {
    let mut out = Vec::new();
    let stats = renderer.render(document, plan, &mut out).unwrap();
    (out, stats)
}

fn pixel(frame: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let offset = ((y * width + x) * 4) as usize;
    [
        frame[offset],
        frame[offset + 1],
        frame[offset + 2],
        frame[offset + 3],
    ]
}

fn is_lit(value: [u8; 4]) -> bool {
    value[0] > 128 && value[1] > 128 && value[2] > 128
}

fn count_lit_row(frame: &[u8], width: u32, y: u32) -> u32 {
    let mut count = 0;
    for x in 0..width {
        if is_lit(pixel(frame, width, x, y)) {
            count += 1;
        }
    }
    count
}

fn ink_bbox(frame: &[u8], width: u32, height: u32) -> Option<(u32, u32, u32, u32)> {
    let mut found = false;
    let mut min_x = width;
    let mut min_y = height;
    let mut max_x = 0;
    let mut max_y = 0;
    for y in 0..height {
        for x in 0..width {
            if is_lit(pixel(frame, width, x, y)) {
                found = true;
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }
    found.then_some((min_x, min_y, max_x, max_y))
}

fn has_green_ink(frame: &[u8], width: u32, height: u32) -> bool {
    for y in 0..height {
        for x in 0..width {
            let value = pixel(frame, width, x, y);
            let green = i32::from(value[1]);
            if green > i32::from(value[0]) + 20 && green > i32::from(value[2]) + 20 {
                return true;
            }
        }
    }
    false
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // clamped to 0..=255
fn srgb_encode(linear: f64) -> u8 {
    let encoded = if linear <= 0.003_130_8 {
        12.92 * linear
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    };
    (encoded.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn render_single_text(
    renderer: &mut FrameRenderer,
    width: u32,
    height: u32,
    text: &str,
    size: f64,
    align: TextAlign,
    origin: (f64, f64),
) -> Vec<u8> {
    let mut document = composition(width, height, BLACK);
    let node = add_text(&mut document, text, size, WHITE, align);
    set_position(&mut document, node, origin.0, origin.1);
    let plan = evaluate(&document, 0);
    render(renderer, &document, &plan).0
}

#[test]
fn background_fills_the_frame() {
    const W: u32 = 64;
    const H: u32 = 48;
    let Some(mut renderer) = try_renderer(W, H) else {
        return;
    };
    let document = composition(W, H, Color::rgb(0x33, 0x66, 0x99));
    let plan = evaluate(&document, 0);
    let (frame, stats) = render(&mut renderer, &document, &plan);
    assert_eq!(frame.len(), (W * H * 4) as usize);
    assert_eq!(stats.shape_draws, 0);
    for (x, y) in [(0, 0), (W - 1, 0), (0, H - 1), (W - 1, H - 1)] {
        let value = pixel(&frame, W, x, y);
        assert!((i32::from(value[0]) - 0x33).abs() <= 1, "red at {x},{y}");
        assert!((i32::from(value[1]) - 0x66).abs() <= 1, "green at {x},{y}");
        assert!((i32::from(value[2]) - 0x99).abs() <= 1, "blue at {x},{y}");
        assert_eq!(value[3], 255);
    }
}

fn overlapping_rects(renderer: &mut FrameRenderer) -> Vec<u8> {
    let mut document = composition(300, 300, BLACK);
    let red = add_rect(&mut document, 160.0, 160.0, 0.0, filled(RED));
    set_position(&mut document, red, 150.0, 150.0);
    let blue = add_rect(&mut document, 160.0, 160.0, 0.0, filled(BLUE));
    set_position(&mut document, blue, 150.0, 150.0);
    let plan = evaluate(&document, 0);
    render(renderer, &document, &plan).0
}

fn shapes_then_text(renderer: &mut FrameRenderer) -> Vec<u8> {
    let mut document = composition(300, 300, BLACK);
    let red = add_rect(&mut document, 300.0, 300.0, 0.0, filled(RED));
    set_position(&mut document, red, 150.0, 150.0);
    let text = add_text(&mut document, "HHH", 100.0, GREEN, TextAlign::Center);
    set_position(&mut document, text, 150.0, 190.0);
    let plan = evaluate(&document, 0);
    render(renderer, &document, &plan).0
}

fn text_then_shapes(renderer: &mut FrameRenderer) -> Vec<u8> {
    let mut document = composition(300, 300, BLACK);
    let text = add_text(&mut document, "HHH", 100.0, GREEN, TextAlign::Center);
    set_position(&mut document, text, 150.0, 190.0);
    let red = add_rect(&mut document, 300.0, 300.0, 0.0, filled(RED));
    set_position(&mut document, red, 150.0, 150.0);
    let plan = evaluate(&document, 0);
    render(renderer, &document, &plan).0
}

#[test]
fn painter_order_is_plan_order() {
    let Some(mut renderer) = try_renderer(300, 300) else {
        return;
    };
    let overlap = overlapping_rects(&mut renderer);
    let value = pixel(&overlap, 300, 150, 150);
    assert!(value[2] > 200 && value[0] < 40, "overlap must be blue");

    let green_on_top = shapes_then_text(&mut renderer);
    assert!(
        has_green_ink(&green_on_top, 300, 300),
        "text must draw after shapes"
    );

    let red_on_top = text_then_shapes(&mut renderer);
    assert!(
        !has_green_ink(&red_on_top, 300, 300),
        "shapes must draw after text"
    );
}

#[test]
fn y_axis_points_down() {
    const W: u32 = 300;
    const H: u32 = 300;
    let Some(mut renderer) = try_renderer(W, H) else {
        return;
    };
    let mut document = composition(W, H, BLACK);
    let rect = add_rect(&mut document, 20.0, 20.0, 0.0, filled(WHITE));
    set_position(&mut document, rect, 100.0, 100.0);
    let plan = evaluate(&document, 0);
    let (frame, _) = render(&mut renderer, &document, &plan);
    assert!(is_lit(pixel(&frame, W, 100, 100)), "pixel at composition y");
    assert!(
        !is_lit(pixel(&frame, W, 100, H - 100)),
        "the mirrored row must stay dark"
    );
}

#[test]
fn rotation_and_parent_scale() {
    const W: u32 = 400;
    const H: u32 = 400;
    let Some(mut renderer) = try_renderer(W, H) else {
        return;
    };
    let mut document = composition(W, H, BLACK);
    let group = document.add_node(NodeContent::Group).unwrap();
    set_position(&mut document, group, 200.0, 200.0);
    set_scale(&mut document, group, 2.0);
    let rect = add_rect(&mut document, 200.0, 20.0, 0.0, filled(WHITE));
    set_rotation(&mut document, rect, 90.0);
    document.reparent(rect, group).unwrap();
    let plan = evaluate(&document, 0);
    let (frame, _) = render(&mut renderer, &document, &plan);
    assert!(
        is_lit(pixel(&frame, W, 200, 100)),
        "inside the vertical strip"
    );
    assert!(
        !is_lit(pixel(&frame, W, 100, 200)),
        "outside the perpendicular strip"
    );
}

#[test]
fn stroke_is_centered_on_the_edge() {
    const W: u32 = 400;
    const H: u32 = 400;
    let Some(mut renderer) = try_renderer(W, H) else {
        return;
    };
    let mut document = composition(W, H, BLACK);
    let rect = add_rect(&mut document, 100.0, 100.0, 0.0, stroked(WHITE, 10.0));
    set_position(&mut document, rect, 200.0, 200.0);
    let plan = evaluate(&document, 0);
    let (frame, _) = render(&mut renderer, &document, &plan);
    assert!(is_lit(pixel(&frame, W, 246, 200)), "4 px inside the edge");
    assert!(is_lit(pixel(&frame, W, 254, 200)), "4 px outside the edge");
    assert!(!is_lit(pixel(&frame, W, 240, 200)), "6 px inside is hollow");
    assert!(
        !is_lit(pixel(&frame, W, 260, 200)),
        "10 px outside is empty"
    );
    assert!(!is_lit(pixel(&frame, W, 200, 200)), "the centre is hollow");
}

#[test]
fn opacity_and_linear_color() {
    const W: u32 = 400;
    const H: u32 = 400;
    let Some(mut renderer) = try_renderer(W, H) else {
        return;
    };
    let mut document = composition(W, H, BLACK);
    let rect = add_rect(&mut document, 200.0, 200.0, 0.0, filled(WHITE));
    set_position(&mut document, rect, 200.0, 200.0);
    set_opacity(&mut document, rect, 0.5);
    let plan = evaluate(&document, 0);
    let (frame, _) = render(&mut renderer, &document, &plan);
    let value = pixel(&frame, W, 200, 200);
    let expected = srgb_encode(0.5);
    assert!(
        (i32::from(value[0]) - i32::from(expected)).abs() <= 2,
        "expected about {expected}, got {}",
        value[0]
    );
    assert_eq!(value[0], value[1]);
    assert_eq!(value[1], value[2]);
}

#[test]
fn text_is_upright() {
    const W: u32 = 500;
    const H: u32 = 500;
    let Some(mut renderer) = try_renderer(W, H) else {
        return;
    };
    let frame = render_single_text(
        &mut renderer,
        W,
        H,
        "T",
        200.0,
        TextAlign::Left,
        (200.0, 400.0),
    );
    let crossbar = count_lit_row(&frame, W, 400 - 135);
    let stem = count_lit_row(&frame, W, 400 - 30);
    assert!(
        crossbar > stem * 2,
        "crossbar {crossbar} must be much wider than stem {stem}"
    );
}

#[test]
fn text_baseline_and_alignment() {
    const W: u32 = 500;
    const H: u32 = 500;
    const ORIGIN: f64 = 250.0;
    let Some(mut renderer) = try_renderer(W, H) else {
        return;
    };

    let frame = render_single_text(
        &mut renderer,
        W,
        H,
        "H",
        200.0,
        TextAlign::Left,
        (ORIGIN, ORIGIN),
    );
    let bbox = ink_bbox(&frame, W, H).expect("H must draw ink");
    let gap = 250 - i64::from(bbox.3);
    assert!(
        (0..=2).contains(&gap),
        "H bottom is {gap} px above baseline"
    );

    let left = render_single_text(
        &mut renderer,
        W,
        H,
        "_",
        200.0,
        TextAlign::Left,
        (ORIGIN, ORIGIN),
    );
    let left_box = ink_bbox(&left, W, H).expect("underscore must draw ink");
    assert!(
        (i64::from(left_box.0) - 250).abs() <= 3,
        "left edge at {}",
        left_box.0
    );

    let center = render_single_text(
        &mut renderer,
        W,
        H,
        "_",
        200.0,
        TextAlign::Center,
        (ORIGIN, ORIGIN),
    );
    let center_box = ink_bbox(&center, W, H).expect("underscore must draw ink");
    let mid = i64::midpoint(i64::from(center_box.0), i64::from(center_box.2));
    assert!((mid - 250).abs() <= 3, "centre at {mid}");

    let right = render_single_text(
        &mut renderer,
        W,
        H,
        "_",
        200.0,
        TextAlign::Right,
        (ORIGIN, ORIGIN),
    );
    let right_box = ink_bbox(&right, W, H).expect("underscore must draw ink");
    assert!(
        (i64::from(right_box.2) - 250).abs() <= 3,
        "right edge at {}",
        right_box.2
    );
}

#[test]
fn same_frame_twice_is_identical() {
    let source = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/hello.motion"
    ));
    let document = motioner_dsl::run(source, &motioner_dsl::Limits::default()).unwrap();
    let mut evaluator = Evaluator::new();
    let mut first_plan = RenderPlan::new();
    let mut second_plan = RenderPlan::new();
    evaluator.evaluate(&document, 120, &mut first_plan);
    evaluator.evaluate(&document, 120, &mut second_plan);
    assert_eq!(first_plan, second_plan);

    let Some(mut renderer) = try_renderer(first_plan.width(), first_plan.height()) else {
        return;
    };
    let mut first = Vec::new();
    let mut second = Vec::new();
    renderer.render(&document, &first_plan, &mut first).unwrap();
    renderer
        .render(&document, &second_plan, &mut second)
        .unwrap();
    assert_eq!(first.len(), second.len());
    assert_eq!(Sha256::digest(&first), Sha256::digest(&second));
}

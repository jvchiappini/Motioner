//! The live painter never draws a substituted font (prompt 17 S2).
//!
//! A lazily built painter registers only its fallback face; the faces a
//! document's text uses are registered by `paint` before it draws anything.
//! These tests pin that guarantee and show that a substituted face would be
//! visible, so "registered before the frame" is a real correctness property and
//! not a formality.
//!
//! Every test skips cleanly when no GPU adapter is available.

use std::sync::Mutex;

use ferrous_renderer::headless::{Pure2dHarness, DEFAULT_FORMAT};
use motioner_core::{Color, Composition, Document, FrameRate, NodeContent, TextAlign, TextContent};
use motioner_eval::{Evaluator, RenderPlan};
use motioner_render::{FontSet, LivePainter, ViewFit};

static GPU_LOCK: Mutex<()> = Mutex::new(());

const INTER_BOLD: &[u8] = include_bytes!("../../../assets/fonts/Inter-Bold.ttf");

fn harness() -> Option<Pure2dHarness> {
    match Pure2dHarness::new(320, 180, DEFAULT_FORMAT) {
        Ok(harness) => Some(harness),
        Err(error) => {
            println!("[skip] headless GPU unavailable: {error}");
            None
        }
    }
}

/// One text node in the project face (weight 700), not the fallback.
fn document() -> Document {
    let rate = FrameRate::new(60, 1).unwrap();
    let mut document =
        Document::new(Composition::new("Fonts", 320, 180, rate, 10, Color::BLACK).unwrap());
    let mut content = TextContent::new(
        "Hamburgefonstiv",
        "ProjectFace",
        42.0,
        Color::WHITE,
        TextAlign::Left,
    );
    content.weight = 700;
    document.add_node(NodeContent::Text(content)).unwrap();
    document
}

/// The document's set: the bundled faces plus the project face under test.
fn fonts_with_face() -> FontSet {
    let mut fonts = FontSet::bundled();
    // A face the bundled set does not have: the document below uses it.
    fonts.push_face("ProjectFace", 700, INTER_BOLD.to_vec());
    fonts
}

fn plan(document: &Document) -> RenderPlan {
    let mut evaluator = Evaluator::new();
    let mut plan = RenderPlan::new();
    evaluator.evaluate(document, 0, &mut plan);
    plan
}

fn fit() -> ViewFit {
    ViewFit::contain(320.0, 180.0, [0.0, 0.0, 320.0, 180.0], 320.0, 180.0)
}

/// Paints one frame with a lazy painter for `fonts` and returns the pixels.
fn render(harness: &mut Pure2dHarness, fonts: FontSet, document: &Document) -> Vec<u8> {
    let mut painter = LivePainter::lazy(harness.renderer_mut(), fonts).expect("painter");
    harness.begin_frame();
    painter
        .paint(harness.renderer_mut(), document, &plan(document), fit())
        .expect("the plan paints");
    harness.render_frame();
    harness.read_pixels()
}

#[test]
fn a_lazy_painter_registers_the_face_before_drawing() {
    let _guard = GPU_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(mut harness) = harness() else {
        return;
    };
    let document = document();
    let mut painter =
        LivePainter::lazy(harness.renderer_mut(), fonts_with_face()).expect("painter");
    assert!(
        !painter.face_registered("ProjectFace", 700),
        "a lazy painter registers only its fallback"
    );

    harness.begin_frame();
    let stats = painter
        .paint(harness.renderer_mut(), &document, &plan(&document), fit())
        .expect("the plan paints");

    assert!(
        painter.face_registered("ProjectFace", 700),
        "paint must register the face before drawing it"
    );
    assert!(stats.text_draws > 0, "the text must have been drawn");

    // A second frame reuses the same atlas.
    let id = painter.font_id("ProjectFace", 700).expect("registered");
    harness.begin_frame();
    painter
        .paint(harness.renderer_mut(), &document, &plan(&document), fit())
        .expect("the plan paints again");
    assert_eq!(painter.font_id("ProjectFace", 700), Some(id));
}

#[test]
fn the_face_changes_the_pixels_so_a_substitution_would_be_visible() {
    let _guard = GPU_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(mut harness) = harness() else {
        return;
    };
    let document = document();

    // No Inter in the set: layout falls back to Roboto (documented).
    let fallback = render(&mut harness, FontSet::bundled(), &document);
    // ProjectFace present: the lazy painter registers it before the frame.
    let proper = render(&mut harness, fonts_with_face(), &document);

    let ink = |pixels: &[u8]| {
        pixels
            .chunks(4)
            .filter(|px| px[0] > 8 || px[1] > 8 || px[2] > 8)
            .count()
    };
    assert!(
        ink(&fallback) > 100,
        "the fallback render must show text, got {} lit pixels",
        ink(&fallback)
    );
    assert!(
        ink(&proper) > 100,
        "the Inter render must show text, got {} lit pixels",
        ink(&proper)
    );
    assert_ne!(
        fallback, proper,
        "ProjectFace and the Roboto fallback must render different pixels"
    );
}

#[test]
fn an_unknown_family_keeps_the_roboto_fallback() {
    let _guard = GPU_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(mut harness) = harness() else {
        return;
    };
    let rate = FrameRate::new(60, 1).unwrap();
    let mut document =
        Document::new(Composition::new("Fonts", 320, 180, rate, 10, Color::BLACK).unwrap());
    document
        .add_node(NodeContent::Text(TextContent::new(
            "Hamburgefonstiv",
            "Not A Real Family",
            42.0,
            Color::WHITE,
            TextAlign::Left,
        )))
        .unwrap();

    let mut painter =
        LivePainter::lazy(harness.renderer_mut(), fonts_with_face()).expect("painter");
    harness.begin_frame();
    painter
        .paint(harness.renderer_mut(), &document, &plan(&document), fit())
        .expect("the documented fallback still paints");
    assert!(!painter.face_registered("Not A Real Family", 400));
}

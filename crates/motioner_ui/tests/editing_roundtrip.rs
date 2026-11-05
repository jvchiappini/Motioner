//! Headless integration test: the editor's edits survive a round trip through
//! the overrides file, and a hidden layer disappears from the painted pixels.
//!
//! The editing part needs no GPU: it applies [`EditCommand`]s to an
//! [`OverrideSet`], recompiles in memory, writes the overrides text, reloads a
//! fresh session from disk and compares the documents. The pixel part follows
//! `tests/live_painter.rs` and skips cleanly without an adapter.

// Test bodies take the GPU lock before their local imports and constants.
#![allow(clippy::items_after_statements)]

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;

use motioner_core::{Ease, OverrideValue, Property, Vec2};
use motioner_ui::{
    apply_edit, overrides_path, EditCommand, PreviewSession, UndoStack, UndoState, WRITE_DEBOUNCE,
};

const SOURCE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../examples/hello.motion"
));

static GPU_LOCK: Mutex<()> = Mutex::new(());

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("motioner_ui_edit_{}_{name}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn identity_of(session: &PreviewSession, outline_index: usize) -> String {
    let document = session.document().unwrap();
    let rows = motioner_ui::outline_rows(document);
    document
        .node(rows[outline_index].node)
        .unwrap()
        .identity()
        .as_str()
        .to_owned()
}

#[test]
fn edits_persist_through_the_overrides_file() {
    let dir = temp_dir("roundtrip");
    let path = dir.join("hello.motion");
    fs::write(&path, SOURCE).unwrap();

    let mut session = PreviewSession::new(&path);
    session.reload().unwrap();
    let title = identity_of(&session, 0);
    let circle = identity_of(&session, 1);
    let spinner = identity_of(&session, 6);

    let mut set = session.overrides().clone();
    apply_edit(
        &EditCommand::SetValue {
            id: title.clone(),
            property: "pos".to_owned(),
            value: OverrideValue::Vec2(Vec2::new(960.0, 500.0)),
        },
        &mut set,
    );
    apply_edit(
        &EditCommand::SetValue {
            id: circle.clone(),
            property: "fill".to_owned(),
            value: OverrideValue::Text("#00ff88".to_owned()),
        },
        &mut set,
    );
    apply_edit(
        &EditCommand::SetKey {
            id: title.clone(),
            property: "rotation".to_owned(),
            axis: None,
            frame: 90,
            value: OverrideValue::Number(15.0),
            ease: Ease::Linear,
        },
        &mut set,
    );
    apply_edit(
        &EditCommand::SetVisible {
            id: circle.clone(),
            visible: false,
        },
        &mut set,
    );
    apply_edit(
        &EditCommand::SetLocked {
            id: spinner.clone(),
            locked: true,
        },
        &mut set,
    );
    apply_edit(
        &EditCommand::SetName {
            id: title.clone(),
            name: "Headline".to_owned(),
        },
        &mut set,
    );

    let now = Instant::now();
    assert!(session.edit_overrides(&set, now), "edits compile");
    let document = session.document().unwrap();
    let title_node = document.find(&title).unwrap();
    assert_eq!(
        document
            .node(title_node)
            .unwrap()
            .transform()
            .position
            .value(),
        Vec2::new(960.0, 500.0)
    );
    let circle_node = document.find(&circle).unwrap();
    assert!(!document.node(circle_node).unwrap().is_visible());
    assert!(document.node(document.find(&spinner).unwrap()).is_some());

    assert!(session.flush_overrides(now + WRITE_DEBOUNCE));
    let written = fs::read_to_string(overrides_path(&path)).unwrap();
    assert!(written.contains("override_visible"), "{written}");
    assert!(written.contains("\"name\", \"Headline\""), "{written}");

    // A fresh session reads the file back and compiles the same document.
    let mut reopened = PreviewSession::new(&path);
    reopened.reload().unwrap();
    assert_eq!(reopened.document(), session.document());
    assert!(reopened.warnings().is_empty(), "{:?}", reopened.warnings());

    // The program itself is untouched: only bake writes it.
    assert_eq!(fs::read_to_string(&path).unwrap(), SOURCE);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn undo_redo_survives_a_recompile() {
    let dir = temp_dir("undo");
    let path = dir.join("hello.motion");
    fs::write(&path, SOURCE).unwrap();
    let mut session = PreviewSession::new(&path);
    session.reload().unwrap();
    let title = identity_of(&session, 0);

    let mut undo = UndoStack::default_limit();
    let mut set = session.overrides().clone();
    let rotation = session
        .document()
        .unwrap()
        .node(session.document().unwrap().find(&title).unwrap())
        .unwrap()
        .transform()
        .rotation
        .value();

    undo.record(UndoState {
        source: session.source().to_owned(),
        overrides: set.clone(),
    });
    apply_edit(
        &EditCommand::SetKey {
            id: title.clone(),
            property: Property::Rotation.name().to_owned(),
            axis: None,
            frame: 30,
            value: OverrideValue::Number(rotation + 20.0),
            ease: Ease::Linear,
        },
        &mut set,
    );
    assert!(session.edit_overrides(&set, Instant::now()));
    let changed = session
        .document()
        .unwrap()
        .node(session.document().unwrap().find(&title).unwrap())
        .unwrap()
        .transform()
        .rotation
        .keys()
        .len();
    assert_eq!(changed, 1);

    let restored = undo
        .undo(&UndoState {
            source: session.source().to_owned(),
            overrides: set.clone(),
        })
        .unwrap();
    assert!(session.edit_overrides(&restored.overrides, Instant::now()));
    assert_eq!(
        session
            .document()
            .unwrap()
            .node(session.document().unwrap().find(&title).unwrap())
            .unwrap()
            .transform()
            .rotation
            .keys()
            .len(),
        0
    );
    let redone = undo.redo(&restored).unwrap();
    assert!(session.edit_overrides(&redone.overrides, Instant::now()));
    assert_eq!(
        session
            .document()
            .unwrap()
            .node(session.document().unwrap().find(&title).unwrap())
            .unwrap()
            .transform()
            .rotation
            .keys()
            .len(),
        1
    );
    fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_hidden_layer_is_not_painted() {
    let _gpu = motioner_testkit::gpu::lock();
    use ferrous_engine::wgpu::{Color as ClearColor, TextureFormat};
    use ferrous_renderer::headless::Pure2dHarness;
    use motioner_eval::{Evaluator, RenderPlan};
    use motioner_render::{LivePainter, ViewFit};

    let _guard = GPU_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let mut document = motioner_dsl::run(SOURCE, &motioner_dsl::Limits::default()).unwrap();
    let rows = motioner_ui::outline_rows(&document);
    let first_dot = rows[1].node;
    let mut harness = match Pure2dHarness::new(1920, 1080, TextureFormat::Rgba8UnormSrgb) {
        Ok(harness) => harness,
        Err(error) => {
            println!("[skip] headless GPU unavailable: {error}");
            return;
        }
    };

    let render = |document: &motioner_core::Document, harness: &mut Pure2dHarness| {
        let mut evaluator = Evaluator::new();
        let mut plan = RenderPlan::new();
        evaluator.evaluate(document, 120, &mut plan);
        let background = plan.background();
        harness.set_clear_color(ClearColor {
            r: linear(background.r),
            g: linear(background.g),
            b: linear(background.b),
            a: f64::from(background.a) / 255.0,
        });
        let mut painter =
            LivePainter::new(harness.renderer_mut()).expect("font registration must work");
        harness.begin_frame();
        painter.paint(harness.renderer_mut(), document, &plan, ViewFit::IDENTITY);
        harness.render_frame();
        harness.read_pixels()
    };

    // Dot 1 sits at (480, 820) on frame 120, fully scaled in.
    let (x, y) = (480_u32, 820_u32);
    let painted = render(&document, &mut harness);
    let before = pixel(&painted, x, y);
    assert!(
        before[0] > 200 && before[1] < 120,
        "the dot is red: {before:?}"
    );

    document.set_visible(first_dot, false).unwrap();
    let hidden = render(&document, &mut harness);
    let after = pixel(&hidden, x, y);
    assert!(
        after[0] < 40 && after[1] < 40 && after[2] < 40,
        "the hidden dot leaves the background: {after:?}"
    );
}

/// sRGB byte to linear light, matching the exporter.
fn linear(value: u8) -> f64 {
    let channel = f64::from(value) / 255.0;
    if channel <= 0.040_45 {
        channel / 12.92
    } else {
        ((channel + 0.055) / 1.055).powf(2.4)
    }
}

fn pixel(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    let index = ((y * 1920 + x) * 4) as usize;
    [
        pixels[index],
        pixels[index + 1],
        pixels[index + 2],
        pixels[index + 3],
    ]
}

//! Zero-allocation headless editor frame (prompt 18 P2).
//!
//! Run with:
//!
//! ```text
//! cargo test -p motioner_ui --features alloc-stats --test zero_alloc
//! ```

#![cfg(feature = "alloc-stats")]

use std::fs;
use std::path::PathBuf;

use motioner_ui::{Editor, PreviewSession, Transport};

const SOURCE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../examples/hello.motion"
));

fn temp_path(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("motioner_ui_zero_{}_{name}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir.join("hello.motion")
}

#[test]
fn a_headless_editor_frame_allocates_nothing_after_warmup() {
    let path = temp_path("editor");
    fs::write(&path, SOURCE).unwrap();

    let mut session = PreviewSession::new(&path);
    session.reload().unwrap();
    let (rate, duration) = session.composition().map_or(
        (motioner_core::FrameRate::new(60, 1).unwrap(), 60),
        |comp| (comp.frame_rate(), comp.duration()),
    );
    let mut editor = Editor::new(session, Transport::new(rate, duration), None);

    for _ in 0..10 {
        editor.sync_models();
        let _ = editor.code_panel();
        let _ = editor.orphan_suggestions_shared();
    }

    ferrous_core::alloc_stats::reset();
    let scope = ferrous_core::alloc_stats::ThreadScope::new();
    editor.sync_models();
    let _ = editor.code_panel();
    let _ = editor.orphan_suggestions_shared();
    let delta = scope.finish();
    assert_eq!(delta.allocating_calls(), 0, "{delta:?}");

    fs::remove_file(path).ok();
}

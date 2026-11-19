//! P2 cache-key audit (prompt 18 P3).
//!
//! Each test builds one cache, changes exactly one input and asserts that the
//! cache was rebuilt.  The rule under test: a cache key must cover every input
//! the cached value reads.

use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use motioner_ui::{parse_commands, EditMode, Editor, PreviewSession};

const SOURCE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../examples/hello.motion"
));

/// A private copy of `examples/hello.motion` and an editor over it.
fn temp_editor(name: &str) -> (Editor, PathBuf) {
    let dir =
        std::env::temp_dir().join(format!("motioner_cache_keys_{}_{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("hello.motion");
    fs::write(&path, SOURCE).unwrap();
    let mut editor = Editor::open(&path).unwrap();
    editor.set_edit_mode(EditMode::Code);
    (editor, path)
}

fn run(editor: &mut Editor, text: &str) {
    for command in parse_commands(text).unwrap() {
        editor
            .execute(&command, Duration::ZERO)
            .unwrap_or_else(|error| panic!("{command}: {error}"));
    }
}

#[test]
fn orphan_suggestions_refresh_when_the_generation_changes() {
    let (mut editor, path) = temp_editor("orphans_generation");
    let first = editor.orphan_suggestions_shared();
    let again = editor.orphan_suggestions_shared();
    assert!(
        Rc::ptr_eq(&first, &again),
        "an unchanged document uses the cached list"
    );

    // A reload compiles a new document: the generation input changed.
    let source = fs::read_to_string(&path).unwrap();
    fs::write(&path, format!("# shifted\n{source}")).unwrap();
    assert!(editor.reload(Duration::ZERO));
    let after = editor.orphan_suggestions_shared();
    assert!(
        !Rc::ptr_eq(&first, &after),
        "a new generation must rebuild the orphan list"
    );
}

#[test]
fn orphan_suggestions_refresh_when_the_session_is_replaced() {
    let (mut editor, path) = temp_editor("orphans_session");
    let first = editor.orphan_suggestions_shared();

    // A fresh session restarts its generation at the same value; the cache
    // key must include the session identity, not just the generation.
    let mut session = PreviewSession::new(path.clone());
    session.reload().unwrap();
    editor.replace_session(session).unwrap();
    let after = editor.orphan_suggestions_shared();
    assert!(
        !Rc::ptr_eq(&first, &after),
        "a replaced session must rebuild the orphan list"
    );
}

#[test]
fn code_panel_refreshes_when_the_last_edit_changes_without_a_generation() {
    let (mut editor, _path) = temp_editor("code_edit");
    let before = editor.code_panel();
    assert_eq!(before.highlight, None, "{before:?}");

    // A code-mode edit sets the last edit and recompiles.
    run(&mut editor, "edit(\"@281\", \"color\", \"#00FF88\")");
    let edited = editor.code_panel();
    assert!(
        edited.highlight.is_some(),
        "the committed edit is highlighted: {edited:?}"
    );
    let generation = editor.session().generation();

    // A view command only clears `last_edit`; the generation does not move.
    run(&mut editor, "deselect()");
    assert_eq!(
        editor.session().generation(),
        generation,
        "a view command must not recompile"
    );
    let cleared = editor.code_panel();
    assert_eq!(
        cleared.highlight, None,
        "the stale highlight must be dropped: {cleared:?}"
    );
}

#[test]
fn code_panel_refreshes_when_the_program_changes() {
    let (mut editor, _path) = temp_editor("code_generation");
    let first = editor.code_panel();
    run(&mut editor, "edit(\"@281\", \"color\", \"#00FF88\")");
    let after = editor.code_panel();
    assert!(
        !Rc::ptr_eq(&first, &after),
        "a new program text must rebuild the panel"
    );
    assert_ne!(first.lines.len(), 0);
}

#[test]
fn selection_and_view_geometry_are_not_inputs_of_the_code_panel() {
    let (mut editor, _path) = temp_editor("code_unrelated");
    let first = editor.code_panel();
    run(&mut editor, "select(\"@281\")");
    editor.view_mut().zoom = Some(2.0);
    let after = editor.code_panel();
    assert!(
        Rc::ptr_eq(&first, &after),
        "an unrelated input must not rebuild the panel"
    );
}

#[test]
fn layer_flags_revision_covers_solo_shy_and_show_shy() {
    let (mut editor, _path) = temp_editor("layer_flags");
    let base = editor.layer_flags_revision();

    run(&mut editor, "solo(\"@281\", true)");
    let after_solo = editor.layer_flags_revision();
    assert_ne!(after_solo, base, "solo changes preview_hidden");

    run(&mut editor, "shy(\"@281\", true)");
    let after_shy = editor.layer_flags_revision();
    assert_ne!(after_shy, after_solo, "shy changes preview_hidden");

    run(&mut editor, "show_shy(true)");
    let after_show = editor.layer_flags_revision();
    assert_ne!(after_show, after_shy, "show_shy changes preview_hidden too");
}

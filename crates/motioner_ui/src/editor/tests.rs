use std::path::PathBuf;
use std::time::Duration;

use motioner_core::{Ease, OverrideAction, OverrideValue, Vec2};

use super::{Editor, Effect};
use crate::command::{parse_commands, Command, ErrorKind};
use crate::timeline_model::RowKind;

const HELLO: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../examples/hello.motion"
));

/// A temporary folder that is removed when dropped.
pub(crate) struct TempDir(pub(crate) PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// An editor on a private copy of `examples/hello.motion`.
pub(crate) fn hello_editor(name: &str) -> (Editor, TempDir) {
    let dir = std::env::temp_dir().join(format!("motioner_editor_{}_{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("hello.motion");
    std::fs::write(&path, HELLO).unwrap();
    (Editor::open(&path).unwrap(), TempDir(dir))
}

fn run(editor: &mut Editor, text: &str) {
    for command in parse_commands(text).unwrap() {
        editor
            .execute(&command, Duration::ZERO)
            .unwrap_or_else(|e| panic!("{command}: {e}"));
    }
}

fn error(editor: &mut Editor, text: &str) -> ErrorKind {
    let command = parse_commands(text).unwrap().remove(0);
    editor.execute(&command, Duration::ZERO).unwrap_err().kind
}

const TITLE: &str = "@288";

#[test]
fn blend_edits_write_a_static_override() {
    let (mut editor, _dir) = hello_editor("blend");
    run(&mut editor, "edit(\"@288\", \"blend\", \"multiply\")");
    assert_eq!(
        editor
            .session()
            .overrides()
            .get(TITLE, "blend", None)
            .unwrap()
            .action,
        OverrideAction::Set(OverrideValue::Text("multiply".to_owned()))
    );
    // Applying the override changes the document node.
    let document = editor.session().document().expect("document");
    let id = document.find(TITLE).unwrap();
    assert_eq!(
        document.node(id).unwrap().blend(),
        motioner_core::BlendMode::Multiply
    );
    // Query reports the mode.
    let json = editor.query_json(Some(TITLE)).unwrap();
    assert!(json.contains("\"blend\":\"multiply\""), "{json}");
    // A blend cannot be keyed.
    assert_eq!(
        error(&mut editor, "key(\"@288\", \"blend\", 1.0, \"multiply\")"),
        ErrorKind::UnknownProperty
    );
}

#[test]
fn edit_writes_a_static_value_or_a_key_at_the_playhead() {
    let (mut editor, _dir) = hello_editor("edit");
    // Rotation has no keys: a static override.
    run(&mut editor, "edit(\"@288\", \"rotation\", 15)");
    assert_eq!(
        editor
            .session()
            .overrides()
            .get(TITLE, "rotation", None)
            .unwrap()
            .action,
        OverrideAction::Set(OverrideValue::Number(15.0))
    );
    // pos.x is animated: a key at the playhead that keeps the key's ease.
    run(&mut editor, "seek(48)\nedit(\"@288\", \"pos.x\", 900)");
    assert_eq!(
        editor
            .session()
            .overrides()
            .get(TITLE, "pos.x", Some(48))
            .unwrap()
            .action,
        OverrideAction::Key {
            frame: 48,
            value: OverrideValue::Number(900.0),
            ease: Ease::CubicOut
        }
    );
    // A static axis edit writes the pair, keeping the other axis.
    let (mut editor, _dir) = hello_editor("edit_axis");
    run(&mut editor, "edit(\"@586/i0\", \"anchor.y\", 4)");
    assert_eq!(
        editor
            .session()
            .overrides()
            .get("@586/i0", "anchor", None)
            .unwrap()
            .action,
        OverrideAction::Set(OverrideValue::Vec2(Vec2::new(0.0, 4.0)))
    );
}

#[test]
fn bad_values_and_unknown_targets_are_typed_errors() {
    let (mut editor, _dir) = hello_editor("errors");
    assert_eq!(
        error(&mut editor, "edit(\"nope\", \"pos\", (1, 2))"),
        ErrorKind::UnknownNode
    );
    assert_eq!(
        error(&mut editor, "edit(\"@288\", \"wobble\", 1)"),
        ErrorKind::UnknownProperty
    );
    assert_eq!(
        error(&mut editor, "set(\"@288\", \"color\", \"red\")"),
        ErrorKind::BadValue
    );
    assert_eq!(
        error(&mut editor, "key(\"@288\", \"pos\", 1, 3)"),
        ErrorKind::BadValue
    );
    assert_eq!(
        error(&mut editor, "unkey(\"@288\", \"pos.x\", 2)"),
        ErrorKind::NoKey
    );
    assert_eq!(
        error(&mut editor, "set_ease(\"linear\")"),
        ErrorKind::NoSelection
    );
    assert_eq!(error(&mut editor, "add_keys()"), ErrorKind::NoSelection);
    assert_eq!(
        error(&mut editor, "delete_orphan(\"@288\", \"pos\")"),
        ErrorKind::Refused
    );
    // Nothing was recorded by the failed commands.
    assert_eq!(editor.history(), (0, 0));
}

#[test]
fn keys_move_ease_and_follow_the_selection() {
    let (mut editor, _dir) = hello_editor("keys");
    run(
        &mut editor,
        "select_keys([(\"@288\", \"pos.x\", 0.8), (\"@288\", \"pos.y\", frame(48))])",
    );
    assert_eq!(editor.selected_keys().len(), 2);
    run(&mut editor, "set_ease(\"back_out\")\nretime_keys(12)");
    let set = editor.session().overrides();
    assert!(matches!(
        set.get(TITLE, "pos.y", Some(60)).unwrap().action,
        OverrideAction::Key {
            ease: Ease::BackOut,
            ..
        }
    ));
    assert!(editor.selected_keys().iter().all(|key| key.frame == 60));
    run(
        &mut editor,
        "move_key(\"@288\", \"pos.x\", frame(60), 0.5)\ndelete_keys()",
    );
    assert!(editor.selected_keys().is_empty());
    assert_eq!(editor.history().0, 4);
}

#[test]
fn undo_redo_and_a_gesture_are_one_step_each() {
    let (mut editor, _dir) = hello_editor("undo");
    editor.begin_gesture();
    for x in [100.0, 200.0, 300.0] {
        let command = Command::Edit {
            id: TITLE.to_owned(),
            property: "rotation".to_owned(),
            value: OverrideValue::Number(x),
        };
        editor.execute(&command, Duration::ZERO).unwrap();
    }
    editor.end_gesture();
    assert_eq!(editor.history(), (1, 0));
    run(&mut editor, "undo()");
    assert!(editor.session().overrides().is_empty());
    run(&mut editor, "redo()");
    assert_eq!(
        editor
            .session()
            .overrides()
            .get(TITLE, "rotation", None)
            .unwrap()
            .action,
        OverrideAction::Set(OverrideValue::Number(300.0))
    );
}

#[test]
fn view_selection_and_transport_commands_update_the_state() {
    let (mut editor, _dir) = hello_editor("view");
    run(
        &mut editor,
        "select(\"@288\")\nexpand_all(true)\nonly_animated(true)\nsafe_area(true)\n\
         snap(false)\nloop(false)\npanel_size(\"timeline\", 401)\nscroll(\"inspector\", 40)\n\
         seek(500)\nprev_key(\"@288\", \"opacity\")",
    );
    assert_eq!(editor.selected_row(), Some(0));
    assert_eq!(editor.expanded().len(), editor.outline().len());
    assert!(editor.only_animated() && editor.view().safe_area && !editor.view().snap);
    assert!(!editor.transport().looping());
    assert!((editor.view().sizes.timeline - 400.0).abs() < f32::EPSILON);
    assert_eq!(editor.frame(), 48);
    let copied = editor.execute(&Command::CopyId, Duration::ZERO).unwrap();
    assert_eq!(copied.effect, Effect::Copy(TITLE.to_owned()));
    run(&mut editor, "deselect()\nselect_relative(1)");
    assert_eq!(editor.selected_row(), Some(0));
}

#[test]
fn layer_actions_orphans_and_bake() {
    let (mut editor, dir) = hello_editor("bake");
    run(
        &mut editor,
        "visible(\"@586/i1\", false)\nlock(\"@845/c1172\", true)\nrename(\"@288\", \"Title\")\n\
         edit(\"@288\", \"color\", \"#00FF88\")",
    );
    assert!(editor.node_locked(editor.document().unwrap().find("@845/c1172").unwrap()));
    // An override for a node that does not exist is an orphan.
    let overrides = dir.0.join("hello.overrides.motion");
    std::fs::write(&overrides, "override(\"gone\", \"opacity\", 0.5)\n").unwrap();
    assert!(editor.reload(Duration::ZERO));
    assert_eq!(editor.orphans().len(), 1);
    run(&mut editor, "delete_orphan(\"gone\", \"opacity\")");
    assert!(editor.orphans().is_empty());
    // Bake folds the title's colour into the program.
    run(
        &mut editor,
        "edit(\"@288\", \"color\", \"#00FF88\")\nbake()",
    );
    let program = std::fs::read_to_string(dir.0.join("hello.motion")).unwrap();
    assert!(program.contains("#00FF88"));
    assert!(
        !program.contains("baked by motioner"),
        "a private literal is edited in place, not appended"
    );
}

#[test]
fn zoom_and_pan_are_view_state_only() {
    let (mut editor, _dir) = hello_editor("zoom");
    assert!(editor.state_json().contains("\"zoom\":null"));
    assert!(editor.state_json().contains("\"pan\":[0,0]"));
    run(&mut editor, "zoom(2)\npan(10, -5)");
    assert_eq!(editor.view().zoom, Some(2.0));
    assert_eq!(editor.view().pan, (10.0, -5.0));
    assert!(
        editor.session().overrides().is_empty(),
        "no override is written"
    );
    run(&mut editor, "zoom_fit()");
    assert_eq!(editor.view().zoom, None);
    assert_eq!(editor.view().pan, (0.0, 0.0));
    run(&mut editor, "zoom(1000)");
    assert_eq!(editor.view().zoom, Some(crate::MAX_ZOOM));
    run(&mut editor, "zoom(0.001)");
    assert_eq!(editor.view().zoom, Some(crate::MIN_ZOOM));
    assert_eq!(
        crate::parse_commands("zoom(1, 2)").unwrap_err().kind,
        ErrorKind::BadArgument
    );
}

#[test]
fn zoom_by_multiplies_and_keeps_the_anchor_fixed() {
    let (mut editor, _dir) = hello_editor("zoom_by");
    editor.view_mut().zoom = Some(1.0);
    run(&mut editor, "zoom_by(1, 960, 540)");
    assert_eq!(editor.view().zoom, Some(1.25));
    assert_eq!(editor.view().pan, (0.0, 0.0), "the centre is already fixed");
    // Zooming in around the top-left corner moves the composition right and
    // down by (size / 2) * (1 - from / to): 1920/2 * 0.2 and 1080/2 * 0.2.
    run(&mut editor, "zoom_fit()\nzoom_by(1, 0, 0)");
    let view = (editor.view().pan.0, editor.view().pan.1);
    assert!((view.0 - 192.0).abs() < 1e-9, "{view:?}");
    assert!((view.1 - 108.0).abs() < 1e-9, "{view:?}");
    assert_eq!(error(&mut editor, "zoom_by(1, 0)"), ErrorKind::BadArgument);
    assert_eq!(
        crate::parse_commands("zoom(1, 2)").unwrap_err().kind,
        ErrorKind::BadArgument
    );
}

#[test]
fn state_reports_zoom_and_pan() {
    let (mut editor, _dir) = hello_editor("state_zoom");
    run(&mut editor, "zoom(0.5)\npan(-3, 4)");
    let state = editor.state_json();
    assert!(state.contains("\"zoom\":0.5"), "{state}");
    assert!(state.contains("\"pan\":[-3,4]"), "{state}");
}

#[test]
fn content_fields_get_stopwatches_and_keys() {
    let (mut editor, _dir) = hello_editor("content_keys");
    run(&mut editor, "seek(0)\nstopwatch(\"@288\", \"size\")");
    let entry = editor
        .session()
        .overrides()
        .get(TITLE, "size", Some(0))
        .expect("the stopwatch keys size at the playhead");
    assert!(matches!(entry.action, OverrideAction::Key { .. }));
    // The channel is now animated, so a field edit becomes a key at the
    // playhead.
    run(&mut editor, "seek(30)\nedit(\"@288\", \"size\", 200)");
    assert_eq!(
        editor
            .session()
            .overrides()
            .get(TITLE, "size", Some(30))
            .unwrap()
            .action,
        OverrideAction::Key {
            frame: 30,
            value: OverrideValue::Number(200.0),
            ease: Ease::Linear,
        }
    );
    run(
        &mut editor,
        "key(\"@288\", \"color\", 1.0, \"#00FF88\", ease=\"quad_out\")",
    );
    let colour = editor
        .session()
        .overrides()
        .get(TITLE, "color", Some(60))
        .unwrap();
    assert_eq!(
        colour.action,
        OverrideAction::Key {
            frame: 60,
            value: OverrideValue::Text("#00FF88".to_owned()),
            ease: Ease::QuadOut,
        }
    );
    // The stopwatch again clears the keys and keeps the playhead value.
    run(&mut editor, "stopwatch(\"@288\", \"size\")");
    assert!(editor
        .session()
        .overrides()
        .get(TITLE, "size", Some(30))
        .is_some_and(|entry| matches!(entry.action, OverrideAction::Unkey { .. })));
    // A discrete field is not an animatable channel.
    assert_eq!(
        error(&mut editor, "key(\"@288\", \"text\", 1.0, \"x\")"),
        ErrorKind::UnknownProperty
    );
}

#[test]
fn query_reports_content_channels() {
    let (mut editor, _dir) = hello_editor("content_query");
    run(
        &mut editor,
        "key(\"@288\", \"size\", 1.0, 90)\nkey(\"@288\", \"color\", 0.5, \"#FF5A6E\")",
    );
    let json = editor.query_json(Some(TITLE)).unwrap();
    assert!(
        json.contains("\"size\":{\"value\":90,\"keys\":[{\"frame\":60"),
        "{json}"
    );
    assert!(json.contains("\"color\":{\"value\":\"#FF5A6E\""), "{json}");
    assert!(json.contains("\"frame\":30"), "{json}");
    assert!(
        json.contains("\"content\":{\"text\":\"Motioner\""),
        "{json}"
    );
}

const PATHS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../examples/paths.motion"
));

/// An editor with the vector-path example loaded.
pub(crate) fn paths_editor(name: &str) -> (Editor, TempDir) {
    let dir = std::env::temp_dir().join(format!("motioner_paths_{}_{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("paths.motion");
    std::fs::write(&path, PATHS).unwrap();
    (Editor::open(&path).unwrap(), TempDir(dir))
}

#[test]
fn path_layers_have_channels_rows_and_stopwatches() {
    let (mut editor, _dir) = paths_editor("path_channels");
    run(&mut editor, "select(\"ring\")");
    let inspector = editor.inspector();
    for label in ["Trim Start", "Trim End", "Dash", "Dash Offset", "Cap"] {
        assert!(
            inspector.rows.iter().any(|row| row.label == label),
            "the inspector shows {label}"
        );
    }
    // The ring animates dash_offset, so the timeline has a content row with
    // keys for it.
    let dash_row = editor
        .timeline()
        .rows()
        .iter()
        .find(|row| {
            matches!(
                row.kind,
                RowKind::Content {
                    field: motioner_core::ContentField::DashOffset,
                    ..
                }
            )
        })
        .expect("a dash offset row");
    assert!(dash_row.animated);
    assert!(!dash_row.keys.is_empty());

    run(&mut editor, "seek(30)\nstopwatch(\"ring\", \"trim_end\")");
    let entry = editor
        .session()
        .overrides()
        .get("ring", "trim_end", Some(30))
        .expect("the stopwatch keys trim_end");
    assert!(matches!(entry.action, OverrideAction::Key { .. }));

    run(&mut editor, "key(\"ribbon\", \"trim_start\", 1.0, 0.9)");
    let json = editor.query_json(Some("ribbon")).unwrap();
    assert!(json.contains("\"trim_start\":{\"value\":0.45"), "{json}");
    assert!(json.contains("\"kind\":\"path\""), "{json}");
    // Cap and join are discrete and cannot be keyed.
    assert_eq!(
        error(&mut editor, "key(\"ring\", \"cap\", 1.0, \"butt\")"),
        ErrorKind::UnknownProperty
    );
}

#[test]
fn effect_commands_add_reorder_remove_and_edit_parameters() {
    let (mut editor, _dir) = hello_editor("effects");
    let kinds = |editor: &Editor| {
        let document = editor.session().document().expect("document");
        document
            .node(document.find(TITLE).unwrap())
            .unwrap()
            .effects()
            .iter()
            .map(|effect| effect.kind().name())
            .collect::<Vec<_>>()
    };
    run(
        &mut editor,
        "effect_add(\"@288\", \"drop_shadow\")\neffect_add(\"@288\", \"blur\")\neffect_add(\"@288\", \"glow\")",
    );
    assert_eq!(kinds(&editor), ["drop_shadow", "blur", "glow"]);

    // The stack is an override, so it survives a reload.
    assert_eq!(
        editor
            .session()
            .overrides()
            .get(TITLE, "effects", None)
            .unwrap()
            .action,
        OverrideAction::Set(OverrideValue::Text("drop_shadow,blur,glow".to_owned()))
    );

    run(&mut editor, "effect_move(\"@288\", \"glow\", 0)");
    assert_eq!(kinds(&editor), ["glow", "drop_shadow", "blur"]);
    run(&mut editor, "effect_remove(\"@288\", \"drop_shadow\")");
    assert_eq!(kinds(&editor), ["glow", "blur"]);

    // Parameters are normal animatable channels.
    run(&mut editor, "edit(\"@288\", \"blur.radius\", 12)");
    let radius = {
        let document = editor.session().document().expect("document");
        let node = document.node(document.find(TITLE).unwrap()).unwrap();
        node.effect(motioner_core::EffectKind::Blur)
            .and_then(
                |effect| match effect.channel_ref(motioner_core::EffectParam::Radius, None) {
                    Some(motioner_core::EffectChannelRef::Number(value)) => Some(value.value()),
                    _ => None,
                },
            )
            .unwrap()
    };
    assert!((radius - 12.0).abs() < 1e-9, "radius {radius}");
    assert_eq!(
        editor
            .session()
            .overrides()
            .get(TITLE, "blur.radius", None)
            .unwrap()
            .action,
        OverrideAction::Set(OverrideValue::Number(12.0))
    );

    run(&mut editor, "key(\"@288\", \"blur.radius\", 1.0, 4)");
    let json = editor.query_json(Some(TITLE)).unwrap();
    assert!(json.contains("\"blur.radius\":{\"value\":4"), "{json}");
    assert!(json.contains("\"kind\":\"glow\""), "{json}");

    assert_eq!(
        error(&mut editor, "effect_add(\"@288\", \"blur\")"),
        ErrorKind::BadValue
    );
    assert_eq!(
        error(&mut editor, "effect_remove(\"@288\", \"drop_shadow\")"),
        ErrorKind::BadValue
    );
    assert_eq!(
        error(&mut editor, "effect_move(\"@288\", \"blur\", 9)"),
        ErrorKind::BadArgument
    );
}

#[test]
fn effect_parameters_key_through_the_generic_commands() {
    let (mut editor, _dir) = hello_editor("effect_keys");
    run(
        &mut editor,
        "effect_add(\"@288\", \"drop_shadow\")\nkey(\"@288\", \"drop_shadow.offset.x\", 1.0, 20)",
    );
    let document = editor.session().document().expect("document");
    let node = document.node(document.find(TITLE).unwrap()).unwrap();
    let keys = node
        .effect(motioner_core::EffectKind::DropShadow)
        .and_then(|effect| {
            effect.channel_ref(
                motioner_core::EffectParam::Offset,
                Some(motioner_core::Axis::X),
            )
        })
        .map(|channel| match channel {
            motioner_core::EffectChannelRef::Number(value) => value.keys().len(),
            motioner_core::EffectChannelRef::Color(_) => 0,
        })
        .unwrap();
    assert_eq!(keys, 1);
    assert_eq!(
        error(&mut editor, "key(\"@288\", \"drop_shadow.offset\", 1.0, 4)"),
        ErrorKind::UnknownProperty
    );
    assert_eq!(
        error(&mut editor, "edit(\"@288\", \"wobble.radius\", 4)"),
        ErrorKind::UnknownProperty
    );
}

//! Rig write-through through the real editor session (prompt 16b E2).

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use motioner_core::{Ease, NoTextMetrics, PathPoint, RigValue, Vec2};
use motioner_dsl::{RigCompare, RigEdit, RigSide};
use motioner_io::ProjectAssets;
use motioner_ui::Editor;

fn close(left: f64, right: f64) -> bool {
    (left - right).abs() < 1e-6
}

fn points_close(left: &[motioner_core::PathPoint], right: &[motioner_core::PathPoint]) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            close(left.position.x, right.position.x)
                && close(left.position.y, right.position.y)
                && close(left.handle_in.x, right.handle_in.x)
                && close(left.handle_in.y, right.handle_in.y)
                && close(left.handle_out.x, right.handle_out.x)
                && close(left.handle_out.y, right.handle_out.y)
        })
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn copy_project(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("motioner_rig_edit_{}_{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("examples/rig/assets")).unwrap();
    for file in ["mascot_demo.motion", "mascot.motion"] {
        std::fs::copy(
            root().join("examples/rig").join(file),
            dir.join("examples/rig").join(file),
        )
        .unwrap();
    }
    std::fs::copy(
        root().join("examples/rig/assets/mascot.svg"),
        dir.join("examples/rig/assets/mascot.svg"),
    )
    .unwrap();
    dir
}

fn editor(dir: &Path) -> Editor {
    Editor::open_with_assets(
        &dir.join("examples/rig/mascot_demo.motion"),
        Arc::new(NoTextMetrics),
        Rc::new(ProjectAssets::new(dir)),
    )
    .unwrap()
}

fn rig<'a>(editor: &'a Editor, name: &str) -> &'a motioner_core::RigInstanceManifest {
    editor
        .session()
        .document()
        .unwrap()
        .rig_manifest()
        .rigs
        .iter()
        .find(|rig| rig.name == name)
        .unwrap()
}

#[test]
fn property_keys_set_mirror_and_inputs_write_through() {
    let dir = copy_project("properties");
    let mut editor = editor(&dir);

    editor
        .apply_rig_edit(&RigEdit::Key {
            rig: "mascot".to_owned(),
            property: "look".to_owned(),
            side: RigSide::Both,
            time: 2.6,
            value: RigValue::Vec2(Vec2::new(0.33, 0.44)),
            ease: Ease::CubicOut,
        })
        .unwrap();
    assert!(editor
        .session()
        .source()
        .contains("m.look.key(2.6, (0.33, 0.44), ease=\"cubic_out\")"));
    let look = rig(&editor, "mascot")
        .properties
        .iter()
        .find(|property| property.name == "look")
        .unwrap();
    assert!(look.keys.iter().any(|key| {
        close(key.frame, 2.6 * 60.0) && key.value == RigValue::Vec2(Vec2::new(0.33, 0.44))
    }));

    editor
        .apply_rig_edit(&RigEdit::Set {
            rig: "mascot".to_owned(),
            property: "lid".to_owned(),
            side: RigSide::Both,
            value: RigValue::Number(0.6),
        })
        .unwrap();
    assert!(editor.session().source().contains("m.lid.set(0.6)"));
    for name in ["lt_L", "lt_R"] {
        let property = rig(&editor, "mascot")
            .properties
            .iter()
            .find(|property| property.name == name)
            .unwrap();
        assert!(property.keys.iter().any(|key| key.frame == 0.0));
    }

    editor
        .apply_rig_edit(&RigEdit::Key {
            rig: "mascot".to_owned(),
            property: "mouth_open".to_owned(),
            side: RigSide::Both,
            time: 3.25,
            value: RigValue::Number(14.0),
            ease: Ease::Linear,
        })
        .unwrap();
    let mouth = rig(&editor, "mascot")
        .properties
        .iter()
        .find(|property| property.name == "mouth_open")
        .unwrap();
    assert!(mouth
        .keys
        .iter()
        .any(|key| { close(key.frame, 3.25 * 60.0) && key.value == RigValue::Number(14.0) }));

    editor
        .apply_rig_edit(&RigEdit::SetInput {
            rig: "mascot".to_owned(),
            property: "mood".to_owned(),
            value: RigValue::Text("sad".to_owned()),
        })
        .expect_err("mood is declared in the module and must refuse");

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
#[allow(clippy::too_many_lines)]
fn pose_play_and_machine_calls_write_through() {
    let dir = copy_project("calls");
    let mut editor = editor(&dir);

    editor
        .apply_rig_edit(&RigEdit::ApplyPose {
            rig: "mascot".to_owned(),
            pose: "Feliz".to_owned(),
            at: 11.5,
            duration: 0.4,
            ease: Ease::CubicOut,
        })
        .unwrap();
    assert!(editor
        .session()
        .source()
        .contains("m.pose(\"Feliz\", at=11.5, dur=0.4, ease=\"cubic_out\")"));
    let tracks = &rig(&editor, "mascot").pose_tracks;
    assert!(tracks
        .iter()
        .any(|track| track.pose == "Feliz" && close(track.start, 11.5 * 60.0)));

    editor
        .apply_rig_edit(&RigEdit::SavePose {
            rig: "mascot".to_owned(),
            pose: "TestPose".to_owned(),
            reset: true,
            values: vec![("sx".to_owned(), RigValue::Number(1.2))],
        })
        .unwrap();
    assert!(editor
        .session()
        .source()
        .contains("m.pose(\"TestPose\", reset=True, sx=1.2)"));
    assert!(rig(&editor, "mascot")
        .poses
        .iter()
        .any(|pose| pose.name == "TestPose"));

    editor
        .apply_rig_edit(&RigEdit::Play {
            rig: "mascot".to_owned(),
            clip: "salto".to_owned(),
            at: 11.0,
            loops: 2,
            speed: 0.5,
        })
        .unwrap();
    assert!(editor
        .session()
        .source()
        .contains("m.play(\"salto\", at=11, loops=2, speed=0.5)"));
    assert!(rig(&editor, "mascot")
        .plays
        .iter()
        .any(|play| { play.clip == "salto" && close(play.start, 11.0 * 60.0) && play.loops == 2 }));

    editor
        .apply_rig_edit(&RigEdit::AddState {
            rig: "mascot".to_owned(),
            name: "test".to_owned(),
            pose: Some("TestPose".to_owned()),
            clip: None,
            speed: 1.0,
            looping: true,
        })
        .unwrap();
    assert!(editor
        .session()
        .source()
        .contains("m.state(\"test\", pose=\"TestPose\")"));
    assert!(rig(&editor, "mascot")
        .machine
        .as_ref()
        .unwrap()
        .states
        .iter()
        .any(|state| state.name == "test"));

    editor
        .apply_rig_edit(&RigEdit::AddTransition {
            rig: "mascot".to_owned(),
            from: Some("calm".to_owned()),
            to: "test".to_owned(),
            input: "mood".to_owned(),
            compare: RigCompare::Equals,
            value: RigValue::Text("sad".to_owned()),
            duration: 0.2,
        })
        .unwrap();
    assert!(editor
        .session()
        .source()
        .contains("m.transition(\"calm\", \"test\", input=\"mood\", equals=\"sad\", dur=0.2)"));
    assert!(rig(&editor, "mascot")
        .machine
        .as_ref()
        .unwrap()
        .transitions
        .iter()
        .any(|transition| transition.to == "test"));

    let refused = editor
        .apply_rig_edit(&RigEdit::ClipKey {
            rig: "mascot".to_owned(),
            clip: "salto".to_owned(),
            property: "sy".to_owned(),
            time: 0.5,
            value: RigValue::Number(1.1),
            ease: Ease::Linear,
        })
        .expect_err("the clip is declared in the module");
    assert_eq!(refused.kind, motioner_ui::ErrorKind::Refused);

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn undo_restores_the_exact_bytes() {
    let dir = copy_project("undo");
    let mut editor = editor(&dir);
    let before = editor.session().source().to_owned();
    editor
        .apply_rig_edit(&RigEdit::Set {
            rig: "mascot".to_owned(),
            property: "glow".to_owned(),
            side: RigSide::Both,
            value: RigValue::Number(0.7),
        })
        .unwrap();
    assert_ne!(editor.session().source(), before);
    editor
        .execute(&motioner_ui::Command::Undo, std::time::Duration::ZERO)
        .unwrap();
    assert_eq!(editor.session().source(), before);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn geometry_keys_write_through_and_undo() {
    let dir = copy_project("geometry");
    let mut editor = editor(&dir);
    let before = editor.session().source().to_owned();
    let base = {
        let document = editor.session().document().unwrap();
        let node = document
            .nodes()
            .find(|node| node.name() == Some("lid-top-left"))
            .unwrap();
        let motioner_core::NodeContent::Path { geometry, .. } = node.content() else {
            panic!("lid-top-left is a path");
        };
        geometry.points().to_vec()
    };
    let mut points = base.clone();
    points[0].position.y += 2.0;
    editor
        .apply_rig_edit(&RigEdit::GeometryKey {
            rig: "mascot".to_owned(),
            part: "lid-top-left".to_owned(),
            time: 1.0,
            points: points.clone(),
            ease: Ease::Linear,
        })
        .unwrap();
    assert!(editor
        .session()
        .source()
        .contains("m.geometry(\"lid-top-left\", 1, ["));
    let document = editor.session().document().unwrap();
    let node = document
        .nodes()
        .find(|node| node.name() == Some("lid-top-left"))
        .unwrap();
    let motioner_core::NodeContent::Path { geometry, .. } = node.content() else {
        panic!("lid-top-left is a path");
    };
    let key = geometry
        .keys()
        .iter()
        .find(|key| key.frame == 60)
        .expect("geometry key");
    assert!(points_close(&key.points, &points));
    editor
        .apply_rig_edit(&RigEdit::GeometryUnkey {
            rig: "mascot".to_owned(),
            part: "lid-top-left".to_owned(),
            time: 1.0,
        })
        .unwrap();
    assert!(!editor
        .session()
        .source()
        .contains("m.geometry(\"lid-top-left\""));
    editor
        .execute(&motioner_ui::Command::Undo, std::time::Duration::ZERO)
        .unwrap();
    editor
        .execute(&motioner_ui::Command::Undo, std::time::Duration::ZERO)
        .unwrap();
    assert_eq!(editor.session().source(), before);
    let _ = std::fs::remove_dir_all(dir);
}

fn root_editor(name: &str) -> (PathBuf, Editor) {
    root_editor_source(
        name,
        r#"comp(name="Rig", width=640, height=480, fps=60, duration=5.0)
m = rig(use("assets/mascot.svg"), name="mascot")
m.prop("glow", 0.0, range=(0.0, 1.0))
m.prop("lt_L", 0.0)
m.prop("lt_R", 0.0)
m.mirror("lid", ("lt_L", "lt_R"), False)
m.prop("mood", "calm", kind="enum", values=["calm", "sad", "happy"])
m.pose("Neutral", glow=0.0)
j = m.clip("jump", length=1.0)
j.key("glow", 0.5, 0.5)
m.input("mood", "calm", kind="enum", values=["calm", "sad", "happy"])
m.state("calm")
m.state("sad", pose="Neutral")
m.transition("calm", "sad", input="mood", equals="sad", dur=0.2)
"#,
    )
}

fn root_editor_source(name: &str, source: &str) -> (PathBuf, Editor) {
    let dir = std::env::temp_dir().join(format!("motioner_rig_root_{}_{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("assets")).unwrap();
    std::fs::copy(
        root().join("examples/rig/assets/mascot.svg"),
        dir.join("assets/mascot.svg"),
    )
    .unwrap();
    std::fs::write(dir.join("main.motion"), source).unwrap();
    let editor = Editor::open_with_assets(
        &dir.join("main.motion"),
        Arc::new(NoTextMetrics),
        Rc::new(ProjectAssets::new(&dir)),
    )
    .unwrap();
    (dir, editor)
}

#[test]
#[allow(clippy::too_many_lines)]
fn clip_machine_pose_and_mirror_edits_write_through() {
    let (dir, mut editor) = root_editor("everything");

    editor
        .apply_rig_edit(&RigEdit::ClipKey {
            rig: "mascot".to_owned(),
            clip: "jump".to_owned(),
            property: "glow".to_owned(),
            time: 1.0,
            value: RigValue::Number(0.9),
            ease: Ease::CubicOut,
        })
        .unwrap();
    assert!(editor
        .session()
        .source()
        .contains("j.key(\"glow\", 1, 0.9, ease=\"cubic_out\")"));

    editor
        .apply_rig_edit(&RigEdit::ClipLength {
            rig: "mascot".to_owned(),
            clip: "jump".to_owned(),
            length: 2.0,
        })
        .unwrap();
    assert!(editor
        .session()
        .source()
        .contains("j = m.clip(\"jump\", length=2)"));

    editor
        .apply_rig_edit(&RigEdit::Play {
            rig: "mascot".to_owned(),
            clip: "jump".to_owned(),
            at: 1.0,
            loops: 2,
            speed: 1.0,
        })
        .unwrap();

    editor
        .apply_rig_edit(&RigEdit::SetInput {
            rig: "mascot".to_owned(),
            property: "mood".to_owned(),
            value: RigValue::Text("sad".to_owned()),
        })
        .unwrap();
    assert!(editor
        .session()
        .source()
        .contains("m.prop(\"mood\", \"sad\", kind=\"enum\""));

    editor
        .apply_rig_edit(&RigEdit::AddState {
            rig: "mascot".to_owned(),
            name: "happy".to_owned(),
            pose: Some("Neutral".to_owned()),
            clip: None,
            speed: 1.0,
            looping: true,
        })
        .unwrap();
    editor
        .apply_rig_edit(&RigEdit::RetargetTransition {
            rig: "mascot".to_owned(),
            from: Some("calm".to_owned()),
            to: "sad".to_owned(),
            input: "mood".to_owned(),
            new_from: Some("calm".to_owned()),
            new_to: "happy".to_owned(),
        })
        .unwrap();
    assert!(editor
        .session()
        .source()
        .contains("m.transition(\"calm\", \"happy\", input=\"mood\""));

    editor
        .apply_rig_edit(&RigEdit::RemoveTransition {
            rig: "mascot".to_owned(),
            from: Some("calm".to_owned()),
            to: "happy".to_owned(),
            input: "mood".to_owned(),
        })
        .unwrap();
    editor
        .apply_rig_edit(&RigEdit::RemoveState {
            rig: "mascot".to_owned(),
            name: "happy".to_owned(),
        })
        .unwrap();

    editor
        .apply_rig_edit(&RigEdit::SavePose {
            rig: "mascot".to_owned(),
            pose: "Blink".to_owned(),
            reset: true,
            values: vec![("glow".to_owned(), RigValue::Number(0.8))],
        })
        .unwrap();
    editor
        .apply_rig_edit(&RigEdit::DeletePose {
            rig: "mascot".to_owned(),
            pose: "Blink".to_owned(),
        })
        .unwrap();

    editor
        .apply_rig_edit(&RigEdit::Set {
            rig: "mascot".to_owned(),
            property: "lid".to_owned(),
            side: RigSide::Both,
            value: RigValue::Number(0.5),
        })
        .unwrap();
    editor
        .apply_rig_edit(&RigEdit::Key {
            rig: "mascot".to_owned(),
            property: "lid".to_owned(),
            side: RigSide::Left,
            time: 1.0,
            value: RigValue::Number(0.3),
            ease: Ease::Linear,
        })
        .unwrap();
    assert!(editor.session().source().contains("m.lt_L.key(1, 0.3"));
    let properties = &rig(&editor, "mascot").properties;
    let left = properties.iter().find(|item| item.name == "lt_L").unwrap();
    let right = properties.iter().find(|item| item.name == "lt_R").unwrap();
    assert!(left.keys.iter().any(|key| close(key.frame, 60.0)));
    assert!(!right.keys.iter().any(|key| close(key.frame, 60.0)));

    let _ = std::fs::remove_dir_all(dir);
}

const CORPUS_SOURCE: &str = r#"comp(name="Rig", width=640, height=480, fps=60, duration=5.0)
m = rig(use("assets/mascot.svg"), name="mascot")
m.prop("glow", 0.0, range=(0.0, 1.0))
m.prop("lt_L", 0.0)
m.prop("lt_R", 0.0)
m.mirror("lid", ("lt_L", "lt_R"), False)
m.prop("mood", "calm", kind="enum", values=["calm", "sad", "happy"])
m.glow.key(0.25, 0.5)
m.pose("Neutral", glow=0.0)
m.pose("Unused", glow=1.0)
j = m.clip("jump", length=1.0)
j.key("glow", 0.5, 0.5)
m.input("mood", "calm", kind="enum", values=["calm", "sad", "happy"])
m.state("calm")
m.state("sad", pose="Neutral")
m.state("happy", pose="Neutral")
m.transition("calm", "sad", input="mood", equals="sad", dur=0.2)
"#;

struct RigCase {
    name: &'static str,
    edit: RigEdit,
}

#[allow(clippy::too_many_lines)]
fn rig_cases() -> Vec<RigCase> {
    vec![
        RigCase {
            name: "rig_property_set",
            edit: RigEdit::Set {
                rig: "mascot".into(),
                property: "glow".into(),
                side: RigSide::Both,
                value: RigValue::Number(0.4),
            },
        },
        RigCase {
            name: "rig_property_key",
            edit: RigEdit::Key {
                rig: "mascot".into(),
                property: "glow".into(),
                side: RigSide::Both,
                time: 2.0,
                value: RigValue::Number(0.7),
                ease: Ease::CubicOut,
            },
        },
        RigCase {
            name: "rig_property_unkey",
            edit: RigEdit::Unkey {
                rig: "mascot".into(),
                property: "glow".into(),
                side: RigSide::Both,
                time: 0.25,
            },
        },
        RigCase {
            name: "rig_mirror_both",
            edit: RigEdit::Set {
                rig: "mascot".into(),
                property: "lid".into(),
                side: RigSide::Both,
                value: RigValue::Number(0.5),
            },
        },
        RigCase {
            name: "rig_pose_apply",
            edit: RigEdit::ApplyPose {
                rig: "mascot".into(),
                pose: "Neutral".into(),
                at: 2.0,
                duration: 0.2,
                ease: Ease::Linear,
            },
        },
        RigCase {
            name: "rig_pose_save",
            edit: RigEdit::SavePose {
                rig: "mascot".into(),
                pose: "Fresh".into(),
                reset: true,
                values: vec![("glow".into(), RigValue::Number(0.8))],
            },
        },
        RigCase {
            name: "rig_pose_delete",
            edit: RigEdit::DeletePose {
                rig: "mascot".into(),
                pose: "Unused".into(),
            },
        },
        RigCase {
            name: "rig_clip_key",
            edit: RigEdit::ClipKey {
                rig: "mascot".into(),
                clip: "jump".into(),
                property: "glow".into(),
                time: 1.0,
                value: RigValue::Number(0.9),
                ease: Ease::CubicOut,
            },
        },
        RigCase {
            name: "rig_clip_length",
            edit: RigEdit::ClipLength {
                rig: "mascot".into(),
                clip: "jump".into(),
                length: 2.0,
            },
        },
        RigCase {
            name: "rig_play",
            edit: RigEdit::Play {
                rig: "mascot".into(),
                clip: "jump".into(),
                at: 1.0,
                loops: 2,
                speed: 1.0,
            },
        },
        RigCase {
            name: "rig_state_add",
            edit: RigEdit::AddState {
                rig: "mascot".into(),
                name: "extra".into(),
                pose: Some("Neutral".into()),
                clip: None,
                speed: 1.0,
                looping: true,
            },
        },
        RigCase {
            name: "rig_state_remove",
            edit: RigEdit::RemoveState {
                rig: "mascot".into(),
                name: "happy".into(),
            },
        },
        RigCase {
            name: "rig_transition_retarget",
            edit: RigEdit::RetargetTransition {
                rig: "mascot".into(),
                from: Some("calm".into()),
                to: "sad".into(),
                input: "mood".into(),
                new_from: Some("calm".into()),
                new_to: "happy".into(),
            },
        },
        RigCase {
            name: "rig_transition_remove",
            edit: RigEdit::RemoveTransition {
                rig: "mascot".into(),
                from: Some("calm".into()),
                to: "sad".into(),
                input: "mood".into(),
            },
        },
        RigCase {
            name: "rig_geometry_key",
            edit: RigEdit::GeometryKey {
                rig: "mascot".into(),
                part: "lid-top-left".into(),
                time: 1.0,
                points: vec![
                    PathPoint::curved(
                        Vec2::new(61.0, 90.0),
                        Vec2::ZERO,
                        Vec2::new(0.0, -8.284_271_247_461_902),
                    ),
                    PathPoint::curved(
                        Vec2::new(76.0, 73.0),
                        Vec2::new(-8.284_271_247_461_902, 0.0),
                        Vec2::new(8.284_271_247_461_902, 0.0),
                    ),
                    PathPoint::curved(
                        Vec2::new(91.0, 88.0),
                        Vec2::new(0.0, -8.284_271_247_461_902),
                        Vec2::ZERO,
                    ),
                    PathPoint::corner(Vec2::new(91.0, 70.0)),
                    PathPoint::corner(Vec2::new(61.0, 70.0)),
                ],
                ease: Ease::Linear,
            },
        },
        RigCase {
            name: "rig_input_set",
            edit: RigEdit::SetInput {
                rig: "mascot".into(),
                property: "mood".into(),
                value: RigValue::Text("sad".into()),
            },
        },
    ]
}

fn run_rig_case(case: &RigCase) -> (String, String) {
    let (dir, mut editor) = root_editor_source(case.name, CORPUS_SOURCE);
    editor
        .apply_rig_edit(&case.edit)
        .unwrap_or_else(|error| panic!("{}: {error}", case.name));
    let after = editor.session().source().to_owned();
    let strategy = editor
        .last_commit()
        .and_then(|report| report.strategies.first().copied())
        .map_or_else(|| "code".to_owned(), |strategy| strategy.name().to_owned());
    let _ = std::fs::remove_dir_all(dir);
    (after, strategy)
}

#[test]
#[allow(clippy::too_many_lines)]
fn the_rig_write_through_corpus_matches_exactly() {
    let update = std::env::var_os("UPDATE_RIG_CORPUS").is_some();
    let root = root().join("crates/motioner_dsl/tests/write_through");
    for case in rig_cases() {
        let (after, strategy) = run_rig_case(&case);
        let dir = root.join(case.name);
        if update {
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("before.motion"), CORPUS_SOURCE).unwrap();
            std::fs::write(dir.join("command.txt"), format!("{:#?}\n", case.edit)).unwrap();
            std::fs::write(dir.join("after.motion"), &after).unwrap();
            std::fs::write(dir.join("strategy.txt"), &strategy).unwrap();
            continue;
        }
        let before = std::fs::read_to_string(dir.join("before.motion"))
            .unwrap_or_else(|_| panic!("missing {} (run UPDATE_RIG_CORPUS=1)", case.name));
        let expected_after = std::fs::read_to_string(dir.join("after.motion")).unwrap();
        let expected_strategy = std::fs::read_to_string(dir.join("strategy.txt")).unwrap();
        assert_eq!(before, CORPUS_SOURCE, "{}: before changed", case.name);
        assert_eq!(after, expected_after, "{}: program differs", case.name);
        assert_eq!(
            strategy, expected_strategy,
            "{}: strategy differs",
            case.name
        );
    }
}

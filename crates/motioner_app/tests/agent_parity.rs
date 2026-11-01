//! Agent parity (D-0024): the prompt 08 manual run, done only through
//! `motioner edit` and `motioner bake`, must leave exactly the overrides and
//! document that the same edits produce through `edit::apply`, and every
//! command must be reachable from the command line.
//!
//! No GPU is needed: `edit`, `query` and `bake` are headless.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command as Process, Output};

use motioner_core::{Ease, OverrideSet, OverrideValue};
use motioner_dsl::Limits;
use motioner_ui::{apply_edit, CommandKind, EditCommand, KeyRef, MoveKey};

const HELLO: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../examples/hello.motion"
));
const TITLE: &str = "@288";
const CIRCLE_1: &str = "@586/i0";
const GROUP_1: &str = "@800/c1172";

/// A private copy of `hello.motion`, removed when dropped.
struct Project(PathBuf);

impl Project {
    fn new(name: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("motioner-agent-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("hello.motion"), HELLO).unwrap();
        Self(dir)
    }

    fn program(&self) -> PathBuf {
        self.0.join("hello.motion")
    }

    fn overrides(&self) -> PathBuf {
        self.0.join("hello.overrides.motion")
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn motioner(args: &[&str]) -> Output {
    Process::new(env!("CARGO_BIN_EXE_motioner"))
        .args(args)
        .output()
        .expect("run motioner")
}

fn edit(program: &Path, commands: &[&str]) -> String {
    let path = program.to_str().unwrap();
    let mut args = vec!["edit", path];
    args.extend_from_slice(commands);
    let out = motioner(&args);
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(out.status.success(), "{commands:?}: {stdout}");
    assert!(stdout.starts_with("{\"ok\":true"), "{stdout}");
    stdout
}

fn rate() -> motioner_core::FrameRate {
    motioner_core::FrameRate::new(60, 1).unwrap()
}

fn key(
    id: &str,
    property: &str,
    axis: Option<motioner_core::Axis>,
    frame: i64,
    value: f64,
) -> EditCommand {
    EditCommand::SetKey {
        id: id.to_owned(),
        property: property.to_owned(),
        axis,
        frame,
        value: OverrideValue::Number(value),
        ease: Ease::Linear,
    }
}

/// The prompt 08 edits written directly against the edit layer.
fn expected_overrides() -> OverrideSet {
    use motioner_core::Axis::{X, Y};
    let mut set = OverrideSet::new();
    let edits = [
        // K at frame 120 keys the title's animated channels.
        key(TITLE, "pos", Some(X), 120, 960.0),
        key(TITLE, "pos", Some(Y), 120, 480.0),
        key(TITLE, "opacity", None, 120, 1.0),
        // The gizmo moves the title 40 px down (undone and redone).
        key(TITLE, "pos", Some(X), 120, 960.0),
        key(TITLE, "pos", Some(Y), 120, 520.0),
        // The inspector colour field.
        EditCommand::SetValue {
            id: TITLE.to_owned(),
            property: "color".to_owned(),
            value: OverrideValue::Text("#FF3366".to_owned()),
        },
        // Retime the pos.y key by 30 frames, then ease it.
        EditCommand::MoveKeys(vec![MoveKey {
            key: KeyRef {
                id: TITLE.to_owned(),
                property: "pos.y".to_owned(),
                frame: 120,
            },
            to: 150,
            value: OverrideValue::Number(520.0),
            ease: Ease::Linear,
        }]),
        EditCommand::MoveKeys(vec![MoveKey {
            key: KeyRef {
                id: TITLE.to_owned(),
                property: "pos.y".to_owned(),
                frame: 150,
            },
            to: 150,
            value: OverrideValue::Number(520.0),
            ease: Ease::QuadOut,
        }]),
        // Hide a circle and lock a spinner.
        EditCommand::SetVisible {
            id: CIRCLE_1.to_owned(),
            visible: false,
        },
        EditCommand::SetLocked {
            id: GROUP_1.to_owned(),
            locked: true,
        },
    ];
    for edit in &edits {
        apply_edit(edit, &mut set);
    }
    set
}

fn document(program: &str, overrides: Option<&str>) -> motioner_core::Document {
    motioner_dsl::run_with_overrides(program, overrides, &Limits::default())
        .unwrap()
        .document
}

/// What a re-bound document must keep, ignoring provenance keys (D-0025).
fn semantics(
    document: &motioner_core::Document,
) -> Vec<(motioner_core::NodeContent, motioner_core::Transform, bool)> {
    document
        .nodes()
        .map(|node| {
            (
                node.content().clone(),
                node.transform().clone(),
                node.is_visible(),
            )
        })
        .collect()
}

#[test]
fn a_program_edit_orphans_overrides_and_they_rebind_with_suggestions() {
    let project = Project::new("rebind");
    let program = project.program();
    edit(
        &program,
        &[
            "edit(\"@288\", \"rotation\", 12)",
            "visible(\"@586/i1\", false)",
        ],
    );
    let before = document(
        HELLO,
        Some(&fs::read_to_string(project.overrides()).unwrap()),
    );

    // A comment at the top shifts every byte-offset key by its length.
    fs::write(&program, format!("# shifted\n{HELLO}")).unwrap();
    let out = motioner(&["query", program.to_str().unwrap()]);
    let query = String::from_utf8_lossy(&out.stdout);
    assert!(query.contains("\"id\":\"@288\""), "{query}");
    assert!(
        query.contains("\"suggestions\":[\"@298\""),
        "the shifted title is suggested first: {query}"
    );
    assert!(query.contains("\"id\":\"@586/i1\""), "{query}");
    assert!(
        query.contains("\"suggestions\":[\"@596/i1\""),
        "the same loop iteration is suggested first: {query}"
    );

    edit(
        &program,
        &[
            "assign_id(\"@288\", \"@298\")",
            "assign_id(\"@586/i1\", \"@596/i1\")",
        ],
    );
    let after = document(
        &fs::read_to_string(&program).unwrap(),
        Some(&fs::read_to_string(project.overrides()).unwrap()),
    );
    assert_eq!(semantics(&after), semantics(&before));
}

#[test]
fn the_prompt_08_run_through_the_command_line_matches_the_edit_layer() {
    let project = Project::new("run");
    let program = project.program();
    edit(
        &program,
        &[
            "seek(120)",
            "select(\"@288\")",
            "add_keys()",
            "edit(\"@288\", \"pos\", (960, 520))",
            "undo()",
            "redo()",
            "edit(\"@288\", \"color\", \"#FF3366\")",
            "select_keys([(\"@288\", \"pos.y\", frame(120))])",
            "retime_keys(30)",
            "set_ease(\"quad_out\")",
            "visible(\"@586/i0\", false)",
            "lock(\"@800/c1172\", true)",
        ],
    );
    let written = fs::read_to_string(project.overrides()).unwrap();
    assert_eq!(written, expected_overrides().to_mdf(rate()));
    assert_eq!(
        document(HELLO, Some(&written)),
        document(HELLO, Some(&expected_overrides().to_mdf(rate())))
    );
    // The title really moved: pos.y is 520 at 120 and eased to 150.
    let query = motioner(&[
        "query",
        program.to_str().unwrap(),
        "--frame",
        "150",
        "--node",
        TITLE,
    ]);
    let query = String::from_utf8_lossy(&query.stdout);
    assert!(query.contains("\"pos.y\":{\"value\":520"), "{query}");
    assert!(query.contains("\"ease\":\"quad_out\""), "{query}");

    // An external edit adds an orphan; deleting it through the command
    // layer leaves the expected overrides again.
    fs::write(
        project.overrides(),
        format!("{written}override(\"gone\", \"opacity\", 0.5)\n"),
    )
    .unwrap();
    let out = edit(&program, &["delete_orphan(\"gone\", \"opacity\")"]);
    assert!(
        out.contains("\"removed\":[\"override(\\\"gone\\\", \\\"opacity\\\", 0.5)\"]"),
        "{out}"
    );
    assert_eq!(fs::read_to_string(project.overrides()).unwrap(), written);

    // Bake is exact: the baked program plus what is left equals the program
    // plus every override.
    let before = document(HELLO, Some(&written));
    let out = motioner(&["bake", program.to_str().unwrap()]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let baked = fs::read_to_string(&program).unwrap();
    let left = fs::read_to_string(project.overrides()).unwrap();
    assert!(baked.contains("baked by motioner"));
    assert_eq!(document(&baked, Some(&left)), before);
    // The circle (created in a loop) and the lock (editor only) stay.
    assert!(left.contains("override_visible(\"@586/i0\", false)"));
    assert!(left.contains("override_locked(\"@800/c1172\", true)"));
}

#[test]
fn a_failed_command_writes_nothing_and_reports_a_typed_error() {
    let project = Project::new("failed");
    let path = project.program();
    let out = motioner(&[
        "edit",
        path.to_str().unwrap(),
        "visible(\"@586/i0\", false)",
        "unkey(\"@288\", \"rotation\", 1)",
    ]);
    assert_eq!(out.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("\"kind\":\"no_key\""), "{stdout}");
    assert!(stdout.contains("\"command\":1"), "{stdout}");
    assert!(
        !project.overrides().exists(),
        "nothing is written on failure"
    );
}

#[test]
fn dry_run_prints_the_overrides_without_writing() {
    let project = Project::new("dry");
    let path = project.program();
    let out = motioner(&[
        "edit",
        path.to_str().unwrap(),
        "--dry-run",
        "set(\"@288\", \"rotation\", 12)",
    ]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{stdout}");
    assert!(stdout.contains("\"written\":null"), "{stdout}");
    assert!(
        stdout.contains("override(\\\"@288\\\", \\\"rotation\\\", 12)"),
        "{stdout}"
    );
    assert!(!project.overrides().exists());
}

#[test]
fn every_command_reaches_the_executor_from_the_command_line() {
    let project = Project::new("reach");
    let path = project.program();
    for kind in CommandKind::ALL {
        let text = kind.example().to_text();
        let out = motioner(&["edit", path.to_str().unwrap(), "--dry-run", &text]);
        let stdout = String::from_utf8_lossy(&out.stdout);
        // The examples use the id "title", which hello.motion does not
        // define, so most fail in the executor. None may fail before it.
        for early in ["\"parse\"", "\"unknown_command\"", "\"bad_argument\""] {
            assert!(!stdout.contains(early), "{}: {stdout}", kind.name());
        }
        assert!(stdout.starts_with("{\"ok\":"), "{}: {stdout}", kind.name());
    }
    // And the same text means the same command to `ctl`.
    for kind in CommandKind::ALL {
        let example = kind.example();
        assert_eq!(
            motioner_ui::Request::parse(&example.to_text()).unwrap(),
            motioner_ui::Request::Command(example)
        );
    }
}

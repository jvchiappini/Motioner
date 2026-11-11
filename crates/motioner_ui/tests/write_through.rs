//! The write-through corpus and the deterministic property test (prompt 12
//! test strategy, finished by prompt 12b phase B3).
//!
//! Each corpus case lives under
//! `crates/motioner_dsl/tests/write_through/<case>/` with its before program,
//! the D-0024 command text, the exact expected program and the strategy (or
//! the expected fallback reason). The test drives the real [`Editor`] sink, so
//! a case covers parsing, planning, writing and verification together.
//!
//! Run `UPDATE_CORPUS=1` once after an intentional planner change to rewrite
//! the expected files; without it the test is read-only.

use std::path::{Path, PathBuf};
use std::time::Duration;

use motioner_core::{Document, OverrideValue, Vec2};
use motioner_ui::{parse_commands, Command, EditMode, Editor};

struct Case {
    name: &'static str,
    before: &'static str,
    command: &'static str,
}

/// Every case, covering the strategy ladder, structural gestures, refusals
/// and the tricky layouts the planner must survive.
const CASES: &[Case] = &[
    Case {
        name: "private_literal",
        before: "comp(name=\"P\", duration=1.0)\ntitle = text(\"Hi\", color=\"#ffffff\")\n",
        command: "edit(\"@0\", \"color\", \"#ff0000\")",
    },
    Case {
        name: "local_assignment",
        before: "comp(name=\"P\", duration=1.0)\nc = circle(10)\nc.pos = (100, 200)\n",
        command: "edit(\"@0\", \"rotation\", 45)",
    },
    Case {
        name: "loop_guard",
        before: "comp(name=\"P\", duration=1.0)\nfor i in range(3):\n    c = circle(10)\n    c.pos = (100 + i * 20, 200)\n",
        command: "edit(\"@0\", \"fill\", \"#ff0000\")",
    },
    Case {
        name: "loop_guard_key",
        before: "comp(name=\"P\", duration=1.0)\nfor i in range(3):\n    c = circle(10)\n    c.pos = (100 + i * 20, 200)\n",
        command: "key(\"@0\", \"opacity\", 0.5, 0.25)",
    },
    Case {
        name: "nested_loops",
        before: "comp(name=\"P\", duration=1.0)\nfor row in range(2):\n    for col in range(3):\n        c = circle(8)\n        c.pos = (100 + col * 20, 100 + row * 20)\n",
        command: "edit(\"@4\", \"fill\", \"#00ff00\")",
    },
    Case {
        name: "call_argument",
        before: "comp(name=\"P\", duration=1.0)\ndef badge(x, fill=\"#ffffff\"):\n    c = circle(10, fill=fill)\n    c.pos = (x, 100)\n\nbadge(100)\nbadge(200)\n",
        command: "edit(\"@0\", \"fill\", \"#ff0000\")",
    },
    Case {
        name: "call_promotion",
        before: "comp(name=\"P\", duration=1.0)\ndef badge(x):\n    c = circle(10, fill=\"#ffffff\")\n    c.pos = (x, 100)\n\nbadge(100)\nbadge(200)\n",
        command: "edit(\"@0\", \"fill\", \"#ff0000\")",
    },
    Case {
        name: "call_result",
        before: "comp(name=\"P\", duration=1.0)\ndef badge(x):\n    c = circle(10, fill=\"#ffffff\")\n    c.pos = (x, 100)\n    return c\n\nbadge(100)\nbadge(200)\n",
        command: "edit(\"@0\", \"rotation\", 15)",
    },
    Case {
        name: "structural_add",
        before: "comp(name=\"P\", duration=1.0)\na = circle(10)\nb = circle(20)\n",
        command: "add_layer(\"rect\", width=100, height=50, after=\"@0\", name=\"card\")",
    },
    Case {
        name: "structural_delete",
        before: "comp(name=\"P\", duration=1.0)\na = circle(10)\nb = circle(20)\n",
        command: "delete_layer(\"@0\")",
    },
    Case {
        name: "structural_duplicate",
        before: "comp(name=\"P\", duration=1.0)\na = circle(10)\na.pos = (10, 20)\nb = circle(20)\n",
        command: "duplicate_layer(\"@0\")",
    },
    Case {
        name: "structural_rename",
        before: "comp(name=\"P\", duration=1.0)\na = circle(10)\n",
        command: "rename(\"@0\", \"Hero\")",
    },
    Case {
        name: "structural_group",
        before: "comp(name=\"P\", duration=1.0)\na = circle(10)\nb = circle(20)\n",
        command: "group_layers([\"@0\", \"@1\"], name=\"pair\")",
    },
    Case {
        name: "structural_loop_continue",
        before: "comp(name=\"P\", duration=1.0)\nfor i in range(3):\n    c = circle(10)\n    c.pos = (100 + i * 20, 200)\n",
        command: "delete_layer(\"@1\")",
    },
    Case {
        name: "refusal_function_delete",
        before: "comp(name=\"P\", duration=1.0)\ndef badge(x):\n    c = circle(10)\n    c.pos = (x, 100)\n    return c\n\nbadge(100)\n",
        command: "delete_layer(\"@0\")",
    },
    Case {
        name: "refusal_locked",
        before: "comp(name=\"P\", duration=1.0)\na = circle(10)\n",
        command: "lock(\"@0\", true)",
    },
    Case {
        name: "crlf_private_literal",
        before: "comp(name=\"P\", duration=1.0)\r\na = circle(10)\r\n",
        command: "edit(\"@0\", \"rotation\", 15)",
    },
    // Prompt 13b, C0: presets and components are library calls. An edit must
    // resolve to the user's call site (or a local assignment) and never touch
    // the prelude; a theme token argument must survive the edit.
    Case {
        name: "preset_enter_rise",
        before: "comp(name=\"P\", width=1000, height=600, duration=4.0, theme=\"dark\")\nuse_theme(theme(\"dark\"))\nr = rect(220, 120, fill=\"accent\")\nr.id(\"r\")\nr.pos = (500, 300)\nr.enter(\"rise\", at=beat(2), duration=2.0)\n",
        command: "edit(\"r\", \"rotation\", 15)",
    },
    Case {
        name: "preset_exit_slide",
        before: "comp(name=\"P\", width=1000, height=600, duration=4.0, theme=\"dark\")\nuse_theme(theme(\"dark\"))\nr = rect(220, 120, fill=\"accent\")\nr.id(\"r\")\nr.pos = (500, 300)\nr.exit(\"slide\", at=1.0, duration=2.0)\n",
        command: "edit(\"r\", \"rotation\", 15)",
    },
    Case {
        name: "preset_emphasis_bounce",
        before: "comp(name=\"P\", width=1000, height=600, duration=3.0, theme=\"dark\")\nuse_theme(theme(\"dark\"))\nr = rect(220, 120, fill=\"accent\")\nr.id(\"r\")\nr.pos = (500, 300)\nr.emphasis(\"bounce\", at=0.5, duration=2.0)\n",
        command: "edit(\"r\", \"scale.x\", 1.5)",
    },
    Case {
        name: "preset_text_reveal",
        before: "comp(name=\"P\", width=1000, height=600, duration=4.0, theme=\"dark\")\nuse_theme(theme(\"dark\"))\nt = text(\"Kinetic\", token(\"font:body\"), token(\"h1\"), token(\"text\"), \"center\", 700)\nt.id(\"t\")\nt.pos = (500, 300)\nt.reveal(\"rise\", at=0.0, duration=3.0)\n",
        command: "edit(\"t\", \"rotation\", 10)",
    },
    Case {
        name: "component_card_move",
        before: "comp(name=\"P\", width=1000, height=600, duration=4.0, theme=\"dark\")\nuse_theme(theme(\"dark\"))\ng = card(\"Quarterly\", \"Revenue grew 24 percent\")\ng.id(\"card\")\ng.pos = (500, 300)\n",
        command: "edit(\"card\", \"pos.x\", 640)",
    },
    Case {
        name: "component_lower_third_part",
        before: "comp(name=\"P\", width=1000, height=600, duration=4.0, theme=\"dark\")\nuse_theme(theme(\"dark\"))\ng = lower_third(\"Ana Silva\", \"Design lead\")\ng.id(\"third\")\ng.pos = (500, 420)\n",
        command: "edit(\"third\", \"rotation\", 5)",
    },
    Case {
        name: "comments_and_blank_lines",
        before: "comp(name=\"P\", duration=1.0)\n\n# a commented loop\nfor i in range(2):\n    c = circle(10)\n    c.pos = (100 + i * 20, 200)  # a dot\n",
        command: "edit(\"@0\", \"rotation\", 15)",
    },
    Case {
        name: "comment_inside_call",
        before: "comp(name=\"P\", duration=1.0)\na = circle(10)  # a dot\n",
        command: "edit(\"@0\", \"rotation\", 15)",
    },
];

fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../motioner_dsl/tests/write_through")
        .canonicalize()
        .unwrap_or_else(|_| {
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../motioner_dsl/tests/write_through")
        })
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("motioner_corpus_{}_{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Replaces `@N` with the identity of node `N` in `editor`.
fn substitute(command: &str, editor: &Editor) -> String {
    let mut text = command.to_owned();
    let document = editor.session().document().expect("a document");
    for (index, node) in document.nodes().enumerate() {
        let placeholder = format!("@{index}");
        if text.contains(&placeholder) {
            text = text.replace(&placeholder, node.identity().as_str());
        }
    }
    text
}

/// Runs one case through the real editor sink.
fn run_case(case: &Case) -> (String, String) {
    let dir = temp_dir(case.name);
    let path = dir.join("program.motion");
    std::fs::write(&path, case.before).unwrap();
    let mut editor = Editor::open(&path).unwrap();
    editor.set_edit_mode(EditMode::Code);
    let command_text = substitute(case.command, &editor);
    let commands = parse_commands(&command_text).expect("the command parses");
    assert_eq!(commands.len(), 1, "{}: one command", case.name);
    let mut refusal = None;
    for command in &commands {
        match editor.execute(command, Duration::ZERO) {
            Ok(_) => {}
            Err(error) if error.kind == motioner_ui::ErrorKind::Refused => {
                refusal = Some(error.message);
            }
            Err(error) => panic!("{}: {command}: {error}", case.name),
        }
    }
    let after = editor.session().source().to_owned();
    if let Some(reason) = refusal {
        assert_eq!(after, case.before, "{}: a refusal never writes", case.name);
        let _ = std::fs::remove_dir_all(&dir);
        return (after, format!("fallback: {reason}"));
    }
    let strategy = match editor.last_commit() {
        Some(report) if !report.code => {
            assert_eq!(after, case.before, "{}: a fallback never writes", case.name);
            let reason = report
                .reasons
                .first()
                .cloned()
                .unwrap_or_else(|| "no reason".to_owned());
            format!("fallback: {reason}")
        }
        Some(report) => report
            .strategies
            .first()
            .map_or_else(|| "code".to_owned(), |strategy| strategy.name().to_owned()),
        None => panic!("{}: the command reported no commit", case.name),
    };
    let _ = std::fs::remove_dir_all(&dir);
    (after, strategy)
}

#[test]
fn the_write_through_corpus_matches_exactly() {
    let update = std::env::var_os("UPDATE_CORPUS").is_some();
    let root = corpus_root();
    for case in CASES {
        let (after, strategy) = run_case(case);
        let dir = root.join(case.name);
        if update {
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("before.motion"), case.before).unwrap();
            std::fs::write(dir.join("command.txt"), case.command).unwrap();
            std::fs::write(dir.join("after.motion"), &after).unwrap();
            std::fs::write(dir.join("strategy.txt"), &strategy).unwrap();
            continue;
        }
        let before = std::fs::read_to_string(dir.join("before.motion"))
            .unwrap_or_else(|_| panic!("missing corpus case {} (run UPDATE_CORPUS=1)", case.name));
        let command = std::fs::read_to_string(dir.join("command.txt")).unwrap();
        let expected_after = std::fs::read_to_string(dir.join("after.motion")).unwrap();
        let expected_strategy = std::fs::read_to_string(dir.join("strategy.txt")).unwrap();
        assert_eq!(before, case.before, "{}: before changed", case.name);
        assert_eq!(command, case.command, "{}: command changed", case.name);
        assert_eq!(
            after, expected_after,
            "{}: the planner produced a different program",
            case.name
        );
        assert_eq!(
            strategy, expected_strategy,
            "{}: the strategy changed",
            case.name
        );
    }
}

// ---- property test -------------------------------------------------------

/// A tiny deterministic generator (no new dependency).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, limit: usize) -> usize {
        if limit == 0 {
            0
        } else {
            usize::try_from(self.next() % u64::try_from(limit).unwrap_or(1)).unwrap_or(0)
        }
    }
}

/// The observable document by position, ignoring provenance identities.
fn semantics(
    document: &Document,
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

fn property_editor(name: &str, source: &str, mode: EditMode) -> (Editor, PathBuf) {
    let dir = temp_dir(name);
    let path = dir.join("program.motion");
    std::fs::write(&path, source).unwrap();
    let mut editor = Editor::open(&path).unwrap();
    editor.set_edit_mode(mode);
    (editor, dir)
}

const PROPERTY_EXAMPLES: &[&str] = &[
    include_str!("../../../examples/hello.motion"),
    include_str!("../../../examples/blend.motion"),
    include_str!("../../../examples/effects.motion"),
    include_str!("../../../examples/gradients.motion"),
    include_str!("../../../examples/masks.motion"),
    include_str!("../../../examples/paths.motion"),
    include_str!("../../../examples/timing.motion"),
    include_str!("../../../examples/animatable.motion"),
    include_str!("../../../examples/typography.motion"),
];

#[test]
#[allow(clippy::too_many_lines, clippy::cast_precision_loss)] // one readable property loop; tiny integers
fn random_command_sequences_keep_every_property() {
    let mut rng = Rng(0xC0FF_EE12_3456_789A);
    for (example, source) in PROPERTY_EXAMPLES.iter().enumerate() {
        let (mut code, code_dir) =
            property_editor(&format!("prop_code_{example}"), source, EditMode::Code);
        let (mut overrides, overrides_dir) = property_editor(
            &format!("prop_overrides_{example}"),
            source,
            EditMode::Overrides,
        );
        let lint_before = motioner_dsl::lint::lint(source).unwrap().len();
        let mut undo_steps = 0_usize;
        for step in 0..6 {
            let nodes = code
                .session()
                .document()
                .expect("a document")
                .nodes()
                .count();
            if nodes == 0 {
                break;
            }
            let index = rng.below(nodes);
            let property = ["rotation", "opacity", "pos"][rng.below(3)];
            // Each editor resolves the identity of the same node position.
            let code_id = {
                let document = code.session().document().unwrap();
                document
                    .nodes()
                    .nth(index)
                    .unwrap()
                    .identity()
                    .as_str()
                    .to_owned()
            };
            let overrides_id = {
                let document = overrides.session().document().unwrap();
                document
                    .nodes()
                    .nth(index)
                    .unwrap()
                    .identity()
                    .as_str()
                    .to_owned()
            };
            let value = match property {
                "pos" => {
                    let x = 100.0 + (rng.below(400) as f64);
                    let y = 100.0 + (rng.below(300) as f64);
                    OverrideValue::Vec2(Vec2::new(x, y))
                }
                "opacity" => OverrideValue::Number((rng.below(100) as f64) / 100.0),
                _ => OverrideValue::Number((rng.below(90) as f64) - 45.0),
            };
            let history_before = code.history().0;
            let command = Command::Edit {
                id: code_id,
                property: property.to_owned(),
                value: value.clone(),
            };
            code.execute(&command, Duration::ZERO)
                .unwrap_or_else(|error| {
                    panic!("example {example} step {step}: {property} on {index}: {error}")
                });
            if code.history().0 != history_before {
                undo_steps += 1;
            }
            let command = Command::Edit {
                id: overrides_id,
                property: property.to_owned(),
                value,
            };
            overrides
                .execute(&command, Duration::ZERO)
                .expect("the override edit runs");

            // (1) The program re-runs without warnings or diagnostics.
            assert!(
                code.session().warnings().is_empty(),
                "example {example} step {step} produced warnings"
            );
            assert!(
                code.session().diagnostics().is_empty(),
                "example {example} step {step} produced diagnostics"
            );
            // (2) Both sinks produce the same document.
            let code_document = code.session().document().unwrap();
            let overrides_document = overrides.session().document().unwrap();
            assert_eq!(
                semantics(code_document),
                semantics(overrides_document),
                "example {example} step {step}: sinks diverged on {property}"
            );
        }
        let final_source = code.session().source().to_owned();
        // (4) The result is still in the canonical style.
        if motioner_dsl::printer::format_check(&final_source).is_err() {
            let formatted = motioner_dsl::printer::format(&final_source).unwrap();
            let first = final_source
                .lines()
                .zip(formatted.lines())
                .find(|(a, b)| a != b);
            panic!("example {example}: the result is not canonical: {first:?}");
        }
        // (5) The planner introduced no new lint finding.
        let lint_after = motioner_dsl::lint::lint(&final_source).unwrap();
        assert!(
            lint_after.len() <= lint_before,
            "example {example}: lint grew from {lint_before} to {}",
            lint_after.len()
        );
        // (3) Undoing every recorded step returns the original bytes.
        for _ in 0..undo_steps {
            code.execute(&Command::Undo, Duration::ZERO).unwrap();
        }
        assert_eq!(
            code.session().source(),
            *source,
            "example {example}: undo did not restore the original bytes"
        );
        let _ = std::fs::remove_dir_all(&code_dir);
        let _ = std::fs::remove_dir_all(&overrides_dir);
    }
}

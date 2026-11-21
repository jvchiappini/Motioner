//! Advisory linter regressions (prompt 12 phase 7).

use std::path::{Path, PathBuf};

use motioner_dsl::lint::lint;
use motioner_dsl::printer::format;

fn messages(source: &str) -> Vec<String> {
    lint(source)
        .unwrap_or_else(|error| panic!("{error}\n{source}"))
        .into_iter()
        .map(|diagnostic| diagnostic.message)
        .collect()
}

fn assert_warns(source: &str, needle: &str) {
    let messages = messages(source);
    assert!(
        messages.iter().any(|message| message.contains(needle)),
        "expected {needle:?} in {messages:?}"
    );
}

fn examples_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}

fn example_sources() -> Vec<(String, String)> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(examples_dir())
        .expect("examples directory")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "motion")
        })
        .collect();
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let name = path
                .file_name()
                .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
            let source = std::fs::read_to_string(&path).expect("read example");
            (name, source)
        })
        .collect()
}

#[test]
fn an_inline_constructor_without_a_binding_is_reported() {
    assert_warns("g = group()\ng.add(circle(10))\n", "without a binding");
    assert!(lint("g = group()\nc = circle(10)\ng.add(c)\n")
        .unwrap()
        .is_empty());
}

#[test]
fn an_empty_guard_is_reported() {
    assert_warns("x = 1\nif x == 1:\n    pass\n", "empty guard");
}

#[test]
fn a_dead_guard_is_reported() {
    assert_warns(
        "c = circle(10, fill=\"#ffffff\")\nif 1 == 1:\n    c.fill = \"#ffffff\"\n",
        "dead guard",
    );
    assert_warns(
        "c = circle(10)\nc.radius = 10\nif 1 == 1:\n    c.radius = 10\n",
        "dead guard",
    );
}

#[test]
fn a_duplicate_assignment_in_one_block_is_reported() {
    assert_warns(
        "c = circle(10)\nc.radius = 10\nc.radius = 20\n",
        "duplicate assignment",
    );
    // Reassigning a plain variable is normal, not a duplicated property write.
    assert!(lint("x = 10\nx = 20\n").unwrap().is_empty());
}

#[test]
fn a_guard_that_reads_underscore_is_reported() {
    assert_warns(
        "for _ in range(3):\n    if _ == 1:\n        x = 1\n",
        "guard reads `_`",
    );
}

#[test]
fn an_unused_parameter_is_reported() {
    assert_warns("def f(a, b):\n    return a\nf(1, 2)\n", "parameter `b`");
}

#[test]
fn a_repeated_magic_number_is_reported() {
    assert_warns(
        "def f():\n    a = 42\n    b = 42\n    c = 42\n    d = 42\n",
        "appears 4 times",
    );
}

#[test]
fn malformed_programs_return_a_diagnostic_instead_of_panicking() {
    assert!(lint("def (\n").is_err());
}

#[test]
fn every_formatted_example_is_lint_clean() {
    for (name, source) in example_sources() {
        let formatted = format(&source).unwrap_or_else(|error| panic!("{name}: {error}"));
        let diagnostics = lint(&formatted).unwrap_or_else(|error| panic!("{name}: {error}"));
        assert!(
            diagnostics.is_empty(),
            "{name} reported {:?}",
            diagnostics
                .iter()
                .map(|diagnostic| diagnostic.message.as_str())
                .collect::<Vec<_>>()
        );
    }
}

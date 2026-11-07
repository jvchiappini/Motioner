//! `motioner trace`: local edit traces (D-0036 section 6).
//!
//! Recording happens next to every committed command in the window, in
//! `motioner edit` and in the live control endpoint. These commands export a
//! clean, deduplicated corpus and replay it to prove every record is exact.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

use motioner_io::{Project, TraceRecord};
use motioner_ui::{parse_commands, Editor};

/// Arguments of `trace export`.
#[derive(Debug, PartialEq, Eq)]
pub struct ExportArgs {
    /// Project folder, manifest or a directory with `traces/`.
    pub input: PathBuf,
    /// Destination JSONL file.
    pub out: PathBuf,
    /// Only records with this strategy.
    pub strategy: Option<String>,
    /// Only records at or after this timestamp prefix (`2026-11-06` or a full
    /// RFC 3339 time).
    pub since: Option<String>,
}

/// Arguments of `trace replay`.
#[derive(Debug, PartialEq, Eq)]
pub struct ReplayArgs {
    /// JSONL file with trace records.
    pub input: PathBuf,
}

/// One `motioner trace` subcommand.
#[derive(Debug, PartialEq, Eq)]
pub enum TraceCommand {
    /// Export a deduplicated corpus.
    Export(ExportArgs),
    /// Replay a corpus file.
    Replay(ReplayArgs),
}

impl TraceCommand {
    /// Runs the subcommand.
    pub fn run(&self) {
        match self {
            Self::Export(args) => run_export(args),
            Self::Replay(args) => run_replay(args),
        }
    }
}

/// Whether this project records traces (the same rule as the window).
#[must_use]
pub fn enabled(project: &Project) -> bool {
    if std::env::var("MOTIONER_TRACES").is_ok_and(|value| value == "1") {
        return true;
    }
    let setting = motioner_io::EditorState::load(project.root())
        .ok()
        .and_then(|state| state.traces);
    if cfg!(debug_assertions) {
        setting.unwrap_or(true)
    } else {
        setting.unwrap_or(false)
    }
}

/// The trace record of the editor's last committed edit, if it wrote one.
#[must_use]
pub fn build(editor: &Editor, command: &str) -> Option<TraceRecord> {
    let (before, after) = editor.last_program_change()?;
    let report = editor.last_commit()?;
    let strategy = if report.code {
        report
            .strategies
            .iter()
            .map(|strategy| strategy.name())
            .collect::<Vec<_>>()
            .join(", ")
    } else {
        "fallback".to_owned()
    };
    let file = editor
        .session()
        .path()
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("program.motion");
    let mut record = TraceRecord::new(before, after, command, &strategy, &editor.last_diff());
    file.clone_into(&mut record.file);
    record.frame = editor.frame();
    record.selection = editor.selected_identity();
    Some(record)
}

/// Re-applies one record and checks that it produces the recorded program.
///
/// # Errors
///
/// A message when the command does not parse, the editor cannot open the
/// before program or the result differs from the recorded after program.
pub fn replay(record: &TraceRecord) -> Result<(), String> {
    let commands = parse_commands(&record.command).map_err(|error| error.to_string())?;
    let dir = std::env::temp_dir().join(format!(
        "motioner_trace_replay_{}_{}",
        std::process::id(),
        &record.before_hash.get(..12).unwrap_or("0")
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let path = dir.join("program.motion");
    std::fs::write(&path, &record.before).map_err(|error| error.to_string())?;
    let mut editor = Editor::open(&path).map_err(|error| error.to_string())?;
    for command in &commands {
        editor
            .execute(command, Duration::ZERO)
            .map_err(|error| error.to_string())?;
    }
    let after = editor.session().source().to_owned();
    let _ = std::fs::remove_dir_all(&dir);
    let hash = motioner_io::sha256_hex(after.as_bytes());
    if hash != record.after_hash {
        return Err(format!(
            "{}: replay produced a different program",
            record.command
        ));
    }
    Ok(())
}

/// The root that holds `traces/` for an input.
fn trace_root(input: &Path) -> Result<PathBuf, String> {
    match Project::open(input) {
        Ok(project) => Ok(project.root().to_path_buf()),
        Err(_) if input.is_dir() => Ok(input.to_path_buf()),
        Err(error) => Err(error.to_string()),
    }
}

/// `motioner trace export`: writes a deduplicated, replayable JSONL corpus.
pub fn run_export(args: &ExportArgs) {
    let root = match trace_root(&args.input) {
        Ok(root) => root,
        Err(message) => {
            eprintln!("error: {message}");
            return;
        }
    };
    let records = match motioner_io::read_traces(&root) {
        Ok(records) => records,
        Err(error) => {
            eprintln!("error: cannot read traces: {error}");
            return;
        }
    };
    let mut seen = std::collections::BTreeSet::new();
    let mut kept = Vec::new();
    let mut dropped = 0_usize;
    for record in records {
        if args
            .strategy
            .as_deref()
            .is_some_and(|strategy| record.strategy != strategy)
        {
            continue;
        }
        if args
            .since
            .as_deref()
            .is_some_and(|since| record.time.as_str() < since)
        {
            continue;
        }
        if !record.is_replayable() {
            dropped += 1;
            continue;
        }
        let key = (record.before_hash.clone(), record.command.clone());
        if !seen.insert(key) {
            continue;
        }
        if replay(&record).is_ok() {
            kept.push(record);
        } else {
            dropped += 1;
        }
    }
    let write = std::fs::File::create(&args.out).and_then(|mut file| {
        for record in &kept {
            file.write_all(record.to_json().as_bytes())?;
            file.write_all(b"\n")?;
        }
        Ok(())
    });
    match write {
        Ok(()) => println!(
            "{} trace(s) exported to {} ({} dropped)",
            kept.len(),
            args.out.display(),
            dropped
        ),
        Err(error) => eprintln!("error: cannot write {}: {error}", args.out.display()),
    }
}

/// `motioner trace replay`: proves every record reproduces its program.
pub fn run_replay(args: &ReplayArgs) {
    let text = match std::fs::read_to_string(&args.input) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("error: cannot read {}: {error}", args.input.display());
            return;
        }
    };
    let mut ok = 0_usize;
    let mut failed = 0_usize;
    for (line, text) in text.lines().enumerate() {
        if text.trim().is_empty() {
            continue;
        }
        let Some(record) = TraceRecord::parse(text) else {
            failed += 1;
            println!("line {}: malformed trace record", line + 1);
            continue;
        };
        match replay(&record) {
            Ok(()) => ok += 1,
            Err(message) => {
                failed += 1;
                println!("line {}: {message}", line + 1);
            }
        }
    }
    println!("{ok} replayed, {failed} failed");
    if failed > 0 {
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use motioner_ui::{parse_commands, Command, EditMode, Editor};
    use std::time::Duration;

    const SOURCE: &str = "comp(name=\"P\", duration=1.0)\na = circle(10)\n";

    #[test]
    fn a_recorded_edit_replays_to_the_recorded_program() {
        let dir = std::env::temp_dir().join(format!("motioner_trace_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("program.motion");
        std::fs::write(&path, SOURCE).unwrap();
        let mut editor = Editor::open(&path).unwrap();
        editor.set_edit_mode(EditMode::Code);
        let identity = editor
            .session()
            .document()
            .unwrap()
            .nodes()
            .next()
            .unwrap()
            .identity()
            .as_str()
            .to_owned();
        let text = format!("edit({identity:?}, \"rotation\", 15)");
        editor
            .execute(
                &Command::Edit {
                    id: identity,
                    property: "rotation".to_owned(),
                    value: motioner_core::OverrideValue::Number(15.0),
                },
                Duration::ZERO,
            )
            .unwrap();
        let Some((before, after)) = editor.last_program_change() else {
            panic!("the edit wrote code");
        };
        let command = editor.last_commit().unwrap();
        assert!(command.code);
        let mut record = motioner_io::TraceRecord::new(before, after, &text, "structural-edit", "");
        record.frame = 0;
        super::replay(&record).unwrap();
        record.after_hash = "wrong".to_owned();
        assert!(super::replay(&record).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    const BEFORE: &str =
        "comp(name=\"P\", duration=1.0)\ntitle = text(\"Hi\", color=\"#ffffff\")\n";

    /// Writes the committed CI fixture (`MOTIONER_WRITE_TRACE_FIXTURE=1`).
    #[test]
    fn write_the_committed_trace_fixture() {
        if std::env::var_os("MOTIONER_WRITE_TRACE_FIXTURE").is_none() {
            return;
        }
        let dir =
            std::env::temp_dir().join(format!("motioner_trace_fixture_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("program.motion");
        std::fs::write(&path, BEFORE).unwrap();
        let mut editor = Editor::open(&path).unwrap();
        editor.set_edit_mode(EditMode::Code);
        let identity = editor
            .session()
            .document()
            .unwrap()
            .nodes()
            .next()
            .unwrap()
            .identity()
            .as_str()
            .to_owned();
        let command = format!("edit({identity:?}, \"color\", \"#ff0000\")");
        let parsed = parse_commands(&command).unwrap();
        for command in &parsed {
            editor.execute(command, Duration::ZERO).unwrap();
        }
        let record = super::build(&editor, &command).expect("the edit wrote code");
        let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        std::fs::create_dir_all(&fixtures).unwrap();
        std::fs::write(
            fixtures.join("traces.jsonl"),
            format!("{}\n", record.to_json()),
        )
        .unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_committed_fixture_replays() {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/traces.jsonl");
        let text = std::fs::read_to_string(&path).expect("the committed trace fixture");
        let records: Vec<motioner_io::TraceRecord> = text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .filter_map(motioner_io::TraceRecord::parse)
            .collect();
        assert!(!records.is_empty(), "the fixture has records");
        for record in &records {
            super::replay(record).unwrap();
        }
    }
}

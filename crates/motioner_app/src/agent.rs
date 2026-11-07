//! The agent-facing commands: `query`, `edit` and `ctl` (D-0024).
//!
//! `query` and `edit` run headless through [`motioner_ui::Editor`], the
//! same executor the window uses; `ctl` sends requests to a running editor
//! opened with `preview --control`. All three print JSON on stdout.

use std::path::{Path, PathBuf};
use std::time::Duration;

use motioner_ui::{
    parse_commands, Command, CommandError, EditMode, Editor, Endpoint, ErrorKind, Json,
};

/// Arguments of `query`.
#[derive(Debug, PartialEq, Eq)]
pub struct QueryArgs {
    /// MDF program.
    pub input: PathBuf,
    /// Frame to sample (0 by default).
    pub frame: Option<i64>,
    /// Only this node identity.
    pub node: Option<String>,
}

/// Arguments of `edit`.
#[derive(Debug, PartialEq, Eq)]
pub struct EditArgs {
    /// MDF program whose overrides file is edited.
    pub input: PathBuf,
    /// Playhead for commands that use it (`edit`, `toggle_key`, ...).
    pub frame: Option<i64>,
    /// Print the resulting overrides instead of writing them.
    pub dry_run: bool,
    /// Keep the D-0022 behaviour: every edit becomes an override entry.
    pub overrides: bool,
    /// Force write-through for this run even when the project prefers
    /// overrides (`--code`).
    pub code: bool,
    /// Commands in their text form, run in order.
    pub commands: Vec<String>,
}

/// Arguments of `fmt`.
#[derive(Debug, PartialEq, Eq)]
pub struct FmtArgs {
    /// Program, project folder or manifest.
    pub input: PathBuf,
    /// Report non-canonical files instead of rewriting them.
    pub check: bool,
}

/// Arguments of `lint`.
#[derive(Debug, PartialEq, Eq)]
pub struct LintArgs {
    /// Program, project folder or manifest.
    pub input: PathBuf,
}

/// Arguments of `ctl`.
#[derive(Debug, PartialEq, Eq)]
pub struct CtlArgs {
    /// Endpoint file; the default one when `None`.
    pub endpoint: Option<PathBuf>,
    /// Requests, sent one per connection in order.
    pub requests: Vec<String>,
}

/// Whether the run succeeded, for the exit code.
pub enum Status {
    /// Everything ran.
    Ok,
    /// A command failed; its error was printed as JSON.
    Failed,
}

fn open(input: &Path, frame: Option<i64>) -> Result<Editor, CommandError> {
    let target = crate::resolve_input(input)
        .map_err(|failure| CommandError::new(ErrorKind::Io, failure.to_string()))?;
    let fonts = crate::project_fonts(&target);
    let mut editor = Editor::open_with_metrics(
        &target.composition,
        std::sync::Arc::new(motioner_render::CpuTextMetrics::new(fonts)),
    )?;
    let images = match &target.project {
        Some(project) => motioner_io::ProjectImages::new(project),
        None => motioner_io::ProjectImages::loose(&target.root),
    };
    editor
        .session_mut()
        .set_image_resolver(std::rc::Rc::new(images));
    let _ = editor.session_mut().reload();
    if let Some(project) = &target.project {
        let overrides =
            motioner_io::EditorState::load(project.root()).is_ok_and(|state| state.overrides_mode);
        editor.set_edit_mode(if overrides {
            EditMode::Overrides
        } else {
            EditMode::Code
        });
    }
    if let Some(frame) = frame {
        editor.execute(&Command::Seek(frame), Duration::ZERO)?;
    }
    Ok(editor)
}

fn error_json(error: &CommandError, index: Option<usize>) -> String {
    let mut j = Json::new();
    j.begin_object();
    j.key("ok").boolean(false);
    j.key("error").begin_object();
    j.key("kind").string(error.kind.name());
    j.key("message").string(&error.message);
    if let Some(index) = index {
        j.key("command")
            .int(i64::try_from(index).unwrap_or(i64::MAX));
    }
    j.end_object();
    j.end_object();
    j.finish()
}

/// `motioner query`: prints the scene at a frame.
pub fn run_query(args: &QueryArgs) -> Status {
    let result = open(&args.input, args.frame).and_then(|e| e.query_json(args.node.as_deref()));
    match result {
        Ok(json) => {
            println!("{json}");
            Status::Ok
        }
        Err(error) => {
            println!("{}", error_json(&error, None));
            Status::Failed
        }
    }
}

/// `motioner edit`: runs commands through the editor and writes code
/// (write-through, D-0036) or overrides (`--overrides`, D-0022), all or
/// nothing.
#[allow(clippy::too_many_lines)] // the JSON report keeps the transaction in one place
pub fn run_edit(args: &EditArgs) -> Status {
    let mut editor = match open(&args.input, args.frame) {
        Ok(editor) => editor,
        Err(error) => {
            println!("{}", error_json(&error, None));
            return Status::Failed;
        }
    };
    let trace_project = crate::resolve_input(&args.input)
        .ok()
        .and_then(|target| target.project);
    let mut traces = Vec::new();
    // A flag overrides the project setting for this run only; without one the
    // mode read from `editor.motion` (code by default) applies.
    if args.overrides {
        editor.set_edit_mode(EditMode::Overrides);
    } else if args.code {
        editor.set_edit_mode(EditMode::Code);
    }
    // Buffer every write so the whole command list is all-or-nothing: a later
    // failure must not leave earlier commands written (D-0024).
    editor.session_mut().set_write_enabled(false);
    let rate = editor
        .session()
        .composition()
        .map(motioner_core::Composition::frame_rate);
    let before_source = editor.session().source().to_owned();
    let before = rate.map(|rate| editor.session().overrides().to_mdf(rate));
    let mut results = Vec::new();
    let mut strategies: Vec<String> = Vec::new();
    let mut overrides_kept = 0_usize;
    let mut fallback_reasons: Vec<String> = Vec::new();
    let mut index = 0;
    for text in &args.commands {
        let commands = match parse_commands(text) {
            Ok(commands) => commands,
            Err(error) => {
                println!("{}", error_json(&error, Some(index)));
                return Status::Failed;
            }
        };
        for command in commands {
            match editor.execute(&command, Duration::ZERO) {
                Ok(outcome) => {
                    if let Some(report) = editor.last_commit() {
                        for strategy in &report.strategies {
                            let name = strategy.name().to_owned();
                            if !strategies.contains(&name) {
                                strategies.push(name);
                            }
                        }
                        overrides_kept += report.overrides;
                        fallback_reasons.extend(report.reasons.iter().cloned());
                    }
                    if !args.dry_run {
                        if let Some(project) = &trace_project {
                            if crate::trace::enabled(project) {
                                if let Some(record) =
                                    crate::trace::build(&editor, &command.to_text())
                                {
                                    traces.push(record);
                                }
                            }
                        }
                    }
                    results.push((command.to_text(), outcome.summary));
                }
                Err(error) => {
                    // Nothing has been written yet: both files are untouched.
                    println!("{}", error_json(&error, Some(index)));
                    return Status::Failed;
                }
            }
            index += 1;
        }
    }
    let after_source = editor.session().source().to_owned();
    let after = rate.map(|rate| editor.session().overrides().to_mdf(rate));
    let program_changed = after_source != before_source;
    let overrides_changed = before != after;
    let written = if args.dry_run || !(program_changed || overrides_changed) {
        None
    } else if let Err(error) = editor.flush_now() {
        println!("{}", error_json(&error, None));
        return Status::Failed;
    } else if program_changed {
        Some(editor.session().path().display().to_string())
    } else {
        Some(editor.session().overrides_path().display().to_string())
    };
    // Traces go in only after the whole transaction was written, so a failed
    // command never leaves a record of an edit that did not happen.
    if let Some(project) = &trace_project {
        for record in &traces {
            let _ = motioner_io::append_trace(project.root(), record);
        }
    }
    let lines = |text: &Option<String>| -> Vec<String> {
        text.as_deref()
            .unwrap_or("")
            .lines()
            .filter(|line| !line.starts_with('#') && !line.is_empty())
            .map(str::to_owned)
            .collect()
    };
    let (old, new) = (lines(&before), lines(&after));
    let path = editor
        .session()
        .path()
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("program.motion");
    let diff = motioner_ui::unified_diff(&before_source, &after_source, path);
    let mut j = Json::new();
    j.begin_object();
    j.key("ok").boolean(true);
    j.key("results").begin_array();
    for (command, summary) in &results {
        j.begin_object();
        j.key("command").string(command);
        j.key("result").string(summary);
        j.end_object();
    }
    j.end_array();
    j.key("removed").begin_array();
    for line in old.iter().filter(|line| !new.contains(line)) {
        j.string(line);
    }
    j.end_array();
    j.key("added").begin_array();
    for line in new.iter().filter(|line| !old.contains(line)) {
        j.string(line);
    }
    j.end_array();
    j.key("strategies").begin_array();
    for strategy in &strategies {
        j.string(strategy);
    }
    j.end_array();
    j.key("overrides_kept")
        .int(i64::try_from(overrides_kept).unwrap_or(i64::MAX));
    j.key("fallback_reasons").begin_array();
    for reason in &fallback_reasons {
        j.string(reason);
    }
    j.end_array();
    j.key("diff").string(&diff);
    j.key("written").opt_string(written.as_deref());
    if args.dry_run {
        j.key("overrides").opt_string(after.as_deref());
        j.key("program").string(&after_source);
    }
    j.end_object();
    println!("{}", j.finish());
    Status::Ok
}

/// Resolves an input (program, project folder or manifest) to its composition.
fn target(input: &Path) -> Result<PathBuf, String> {
    crate::resolve_input(input)
        .map(|target| target.composition)
        .map_err(|failure| failure.to_string())
}

/// `motioner fmt`: rewrites a program in the canonical style, or reports it
/// with `--check`.
pub fn run_fmt(args: &FmtArgs) -> Status {
    let path = match target(&args.input) {
        Ok(path) => path,
        Err(message) => {
            eprintln!("error: {message}");
            return Status::Failed;
        }
    };
    let source = match std::fs::read_to_string(&path) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("error: cannot read {}: {error}", path.display());
            return Status::Failed;
        }
    };
    let canonical = match motioner_dsl::printer::format(&source) {
        Ok(canonical) => canonical,
        Err(diagnostic) => {
            eprintln!(
                "{}:{}:{}: {}",
                path.display(),
                diagnostic.span.line,
                diagnostic.span.column,
                diagnostic.message
            );
            return Status::Failed;
        }
    };
    if canonical == source {
        println!("{}: already formatted", path.display());
        return Status::Ok;
    }
    if args.check {
        println!("{}: not formatted", path.display());
        return Status::Failed;
    }
    if let Err(error) = motioner_ui::write_atomic(&path, &canonical) {
        eprintln!("error: cannot write {}: {error}", path.display());
        return Status::Failed;
    }
    println!("Formatted {}", path.display());
    Status::Ok
}

/// `motioner lint`: prints advisory diagnostics; it only fails on tool errors.
pub fn run_lint(args: &LintArgs) -> Status {
    let path = match target(&args.input) {
        Ok(path) => path,
        Err(message) => {
            eprintln!("error: {message}");
            return Status::Failed;
        }
    };
    let source = match std::fs::read_to_string(&path) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("error: cannot read {}: {error}", path.display());
            return Status::Failed;
        }
    };
    match motioner_dsl::lint::lint(&source) {
        Ok(diagnostics) => {
            for diagnostic in &diagnostics {
                println!(
                    "{}:{}:{}: {}",
                    path.display(),
                    diagnostic.span.line,
                    diagnostic.span.column,
                    diagnostic.message
                );
            }
            if diagnostics.is_empty() {
                println!("{}: no findings", path.display());
            }
            Status::Ok
        }
        Err(diagnostic) => {
            eprintln!(
                "{}:{}:{}: {}",
                path.display(),
                diagnostic.span.line,
                diagnostic.span.column,
                diagnostic.message
            );
            Status::Failed
        }
    }
}

/// `motioner ctl`: sends requests to a running editor and prints each reply.
pub fn run_ctl(args: &CtlArgs) -> Status {
    let file = args
        .endpoint
        .clone()
        .unwrap_or_else(motioner_ui::default_endpoint_file);
    let endpoint = match Endpoint::read(&file) {
        Ok(endpoint) => endpoint,
        Err(error) => {
            let error = CommandError::new(
                ErrorKind::Io,
                format!(
                    "no running editor ({}: {error}); start one with `motioner preview <file> \
                     --control`",
                    file.display()
                ),
            );
            println!("{}", error_json(&error, None));
            return Status::Failed;
        }
    };
    let mut status = Status::Ok;
    for request in &args.requests {
        match motioner_ui::send_control(&endpoint, request) {
            Ok(reply) => {
                if reply.starts_with("{\"ok\":false") {
                    status = Status::Failed;
                }
                println!("{reply}");
            }
            Err(error) => {
                let error = CommandError::new(
                    ErrorKind::Io,
                    format!("cannot reach the editor on port {}: {error}", endpoint.port),
                );
                println!("{}", error_json(&error, None));
                return Status::Failed;
            }
        }
    }
    status
}

/// The help section listing every command and query, generated from the
/// command layer so it can never drift from it.
pub fn command_help() -> String {
    let mut out = String::from("Commands (edit, ctl; times are seconds or frame(N)):\n");
    for kind in motioner_ui::CommandKind::ALL {
        out.push_str("  ");
        out.push_str(kind.signature());
        out.push('\n');
    }
    out.push_str("Queries (ctl only):\n");
    for query in motioner_ui::Request::QUERIES {
        out.push_str("  ");
        out.push_str(query);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const HELLO: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/hello.motion"
    ));

    fn temp_program(name: &str) -> (PathBuf, PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("motioner_app_edit_{}_{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("hello.motion");
        std::fs::write(&path, HELLO).unwrap();
        (dir, path)
    }

    #[test]
    fn fmt_and_lint_accept_a_loose_program() {
        let path =
            std::env::temp_dir().join(format!("motioner_app_fmt_{}.motion", std::process::id()));
        std::fs::write(&path, "comp(name=\"A\", duration=1.0)\n").unwrap();
        assert!(matches!(
            run_fmt(&FmtArgs {
                input: path.clone(),
                check: false,
            }),
            Status::Ok
        ));
        assert!(matches!(
            run_fmt(&FmtArgs {
                input: path.clone(),
                check: true,
            }),
            Status::Ok
        ));
        assert!(matches!(
            run_lint(&LintArgs {
                input: path.clone(),
            }),
            Status::Ok
        ));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn edit_writes_code_by_default_and_overrides_on_request() {
        let (dir, path) = temp_program("write_through");
        let args = EditArgs {
            input: path.clone(),
            frame: None,
            dry_run: false,
            overrides: false,
            code: false,
            commands: vec!["edit(\"@281\", \"color\", \"#00FF88\")".to_owned()],
        };
        assert!(matches!(run_edit(&args), Status::Ok));
        assert!(std::fs::read_to_string(&path).unwrap().contains("#00FF88"));
        assert!(
            !dir.join("hello.overrides.motion").exists(),
            "write-through writes the program, not the overrides"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn structural_commands_write_code_and_refuse_overrides() {
        let (dir, path) = temp_program("structural_cli");
        let args = EditArgs {
            input: path.clone(),
            frame: None,
            dry_run: false,
            overrides: false,
            code: false,
            commands: vec!["add_layer(\"circle\", radius=10)".to_owned()],
        };
        assert!(matches!(run_edit(&args), Status::Ok));
        let written = std::fs::read_to_string(&path).unwrap();
        assert!(written.contains("= circle(10)"), "{written}");
        let _ = std::fs::remove_dir_all(&dir);

        let (dir, path) = temp_program("structural_cli_overrides");
        let before = std::fs::read_to_string(&path).unwrap();
        let args = EditArgs {
            input: path.clone(),
            frame: None,
            dry_run: false,
            overrides: true,
            code: false,
            commands: vec!["add_layer(\"circle\", radius=10)".to_owned()],
        };
        assert!(matches!(run_edit(&args), Status::Failed));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn edit_overrides_flag_keeps_the_program_untouched() {
        let (dir, path) = temp_program("overrides");
        let before = std::fs::read_to_string(&path).unwrap();
        let args = EditArgs {
            input: path.clone(),
            frame: None,
            dry_run: false,
            overrides: true,
            code: false,
            commands: vec!["edit(\"@281\", \"color\", \"#00FF88\")".to_owned()],
        };
        assert!(matches!(run_edit(&args), Status::Ok));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
        assert!(std::fs::read_to_string(dir.join("hello.overrides.motion"))
            .unwrap()
            .contains("#00FF88"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

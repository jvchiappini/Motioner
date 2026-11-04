//! The agent-facing commands: `query`, `edit` and `ctl` (D-0024).
//!
//! `query` and `edit` run headless through [`motioner_ui::Editor`], the
//! same executor the window uses; `ctl` sends requests to a running editor
//! opened with `preview --control`. All three print JSON on stdout.

use std::path::{Path, PathBuf};
use std::time::Duration;

use motioner_ui::{parse_commands, Command, CommandError, Editor, Endpoint, ErrorKind, Json};

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
    /// Commands in their text form, run in order.
    pub commands: Vec<String>,
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

/// `motioner edit`: runs commands on the overrides file, all or nothing.
pub fn run_edit(args: &EditArgs) -> Status {
    let mut editor = match open(&args.input, args.frame) {
        Ok(editor) => editor,
        Err(error) => {
            println!("{}", error_json(&error, None));
            return Status::Failed;
        }
    };
    let rate = editor
        .session()
        .composition()
        .map(motioner_core::Composition::frame_rate);
    let before = rate.map(|rate| editor.session().overrides().to_mdf(rate));
    let mut results = Vec::new();
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
                Ok(outcome) => results.push((command.to_text(), outcome.summary)),
                Err(error) => {
                    // Nothing has been written yet: the file is untouched.
                    println!("{}", error_json(&error, Some(index)));
                    return Status::Failed;
                }
            }
            index += 1;
        }
    }
    let after = rate.map(|rate| editor.session().overrides().to_mdf(rate));
    let written = if args.dry_run || before == after {
        None
    } else if let Err(error) = editor.save_now() {
        println!("{}", error_json(&error, None));
        return Status::Failed;
    } else {
        Some(editor.session().overrides_path().display().to_string())
    };
    let lines = |text: &Option<String>| -> Vec<String> {
        text.as_deref()
            .unwrap_or("")
            .lines()
            .filter(|line| !line.starts_with('#') && !line.is_empty())
            .map(str::to_owned)
            .collect()
    };
    let (old, new) = (lines(&before), lines(&after));
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
    j.key("written").opt_string(written.as_deref());
    if args.dry_run {
        j.key("overrides").opt_string(after.as_deref());
    }
    j.end_object();
    println!("{}", j.finish());
    Status::Ok
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

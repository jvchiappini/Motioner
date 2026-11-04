//! Motioner application entry point.
//!
//! Wires the document, evaluator, renderer and export crates into a small
//! command-line front end:
//!
//! ```text
//! motioner render  <file.motion> --out <file.mp4> [--from N] [--to N] [--ffmpeg PATH]
//! motioner still   <file.motion> --frame N --out <file.png>
//! motioner info    <file.motion> [--json]
//! motioner bake    <file.motion> [--id ID]...
//! motioner query   <file.motion> [--frame N] [--node ID]
//! motioner edit    <file.motion> [--frame N] [--dry-run] <command>...
//! motioner ctl     [--endpoint FILE] <request>...
//! motioner preview <file.motion> [--control] ...
//! ```
//!
//! MDF diagnostics are printed with their source line and exit with code 2.
//! Every other failure (rendering, encoding, I/O, bad arguments, a failed
//! command) exits with code 1. `query`, `edit` and `ctl` print JSON
//! (`docs/agents.md`). `anyhow` is allowed here because this is the
//! application edge.

mod agent;
mod host;

use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use motioner_core::Document;
use motioner_dsl::Limits;

use motioner_io::{
    write_png, ExportError, FfmpegLocation, Mp4Encoder, Project, ProjectImages, ProjectOptions,
    ProjectSettings, VideoSettings,
};
use motioner_render::{CpuTextMetrics, FontSet, ImageData, ImageSet};
use motioner_render::{FrameRenderer, RenderError};
use std::sync::Arc;

/// Printed by `--help` and on argument errors.
const HELP: &str = "\
Motioner - AI-first motion graphics

Usage:
  motioner new     <dir> [--name NAME] [--size WxH] [--fps N] [--duration FRAMES]
  motioner migrate <file.motion> <dir> [--name NAME]
  motioner render  <project|file.motion> --out <file.mp4> [--from N] [--to N] [--ffmpeg PATH]
  motioner still   <project|file.motion> --frame N --out <file.png>
  motioner import  <project> <file>...
  motioner backup  <project>
  motioner info    <project|file.motion> [--json]
  motioner bake    <project|file.motion> [--id ID]...
  motioner query   <project|file.motion> [--frame N] [--node ID]
  motioner edit    <project|file.motion> [--frame N] [--dry-run] <command>...
  motioner ctl     [--endpoint FILE] <request>...
  motioner preview <project|file.motion> [--frame N] [--paused] [--exit-after-frames N]
                   [--window-size WxH] [--ui-scale F] [--select ROW]
                   [--screenshot <file.png>] [--control] [--control-file FILE]
  motioner --help

A project is a `<name>.motioner/` folder (D-0045); every command also accepts
its folder, its `project.motion` or one of its composition files. A loose
`.motion` file keeps single-file mode with the process working directory as
its asset root.

Frames are integer indices. `render` draws the half-open range [from, to) and
defaults to the whole composition. `still` draws one frame. `preview` opens the
live editor window; `--exit-after-frames` closes it after N presented frames
(for smoke runs and CI), `--window-size` sets the window in physical pixels,
`--ui-scale` overrides the chrome scale (to check other DPI settings),
`--select` selects an outline row at startup and `--screenshot` writes the
window to a PNG after `--exit-after-frames` frames (1 by default) and closes
it.

`new` creates the folder, manifest, a default composition and `editor.motion`.
`migrate` copies a loose program and the assets it references into a project.

`bake` folds <name>.overrides.motion into <name>.motion for nodes bound to a
top-level variable, appends one generated block and rewrites both files
atomically. Nodes created inside loops or functions cannot be baked and are
reported; their overrides stay in the overrides file.

Agents (docs/agents.md): `query` prints the scene at a frame as JSON. `edit`
runs commands on the overrides file through the editor's command layer, all
or nothing, and prints what changed (`--dry-run` writes nothing). `preview
--control` opens a local control endpoint; `ctl` sends it commands and the
queries state(), query(node=None) and screenshot(path).";

/// Parsed command line.
#[derive(Debug, PartialEq, Eq)]
enum Command {
    /// Create a project folder.
    New(NewArgs),
    /// Copy a loose program into a project.
    Migrate(MigrateArgs),
    /// Render a frame range to an H.264 MP4.
    Render(RenderArgs),
    /// Render one frame to a PNG still.
    Still(StillArgs),
    /// Print composition information.
    Info(InfoArgs),
    /// Fold the overrides file into the program.
    Bake(BakeArgs),
    /// Open the live preview window.
    Preview(PreviewArgs),
    /// Print the scene as JSON.
    Query(agent::QueryArgs),
    /// Run commands on the overrides file.
    Edit(agent::EditArgs),
    /// Drive a running editor.
    Ctl(agent::CtlArgs),
    /// Print usage.
    Help,
    /// Open the start screen with no project.
    Desktop(DesktopArgs),
    /// Import media files into a project.
    Import(ImportArgs),
    /// Write a project backup now.
    Backup(BackupArgs),
}

/// Arguments of the `backup` command.
#[derive(Debug, PartialEq, Eq)]
struct BackupArgs {
    /// Project folder, manifest or composition.
    project: PathBuf,
}

/// Arguments of the `import` command.
#[derive(Debug, PartialEq, Eq)]
struct ImportArgs {
    /// Project folder, manifest or composition.
    project: PathBuf,
    /// Files to import.
    files: Vec<PathBuf>,
}

/// Arguments of the start screen (the no-argument launch).
#[derive(Debug, PartialEq, Eq, Default)]
struct DesktopArgs {
    /// Close the window after this many presented frames.
    exit_after_frames: Option<u64>,
    /// Window size in physical pixels.
    window_size: Option<(u32, u32)>,
    /// Chrome scale override, stored in thousandths.
    ui_scale_milli: Option<u32>,
    /// Capture the window to this PNG and close it.
    screenshot: Option<PathBuf>,
}

/// Arguments of the `new` command.
#[derive(Debug, PartialEq, Eq)]
struct NewArgs {
    /// Destination folder; `.motioner` is added when absent.
    dir: PathBuf,
    /// Project name; the folder stem when absent.
    name: Option<String>,
    /// Composition size.
    size: Option<(u32, u32)>,
    /// Frame rate.
    fps: Option<u32>,
    /// Composition length in frames.
    duration: Option<i64>,
}

/// Arguments of the `migrate` command.
#[derive(Debug, PartialEq, Eq)]
struct MigrateArgs {
    /// Loose program to copy.
    input: PathBuf,
    /// Destination folder.
    dir: PathBuf,
    /// Project name; the folder stem when absent.
    name: Option<String>,
}

/// Arguments of the `render` command.
#[derive(Debug, PartialEq, Eq)]
struct RenderArgs {
    /// MDF program.
    input: PathBuf,
    /// Destination MP4.
    output: PathBuf,
    /// First rendered frame (inclusive); `None` means the composition start.
    from: Option<i64>,
    /// Last rendered frame (exclusive); `None` means the composition end.
    to: Option<i64>,
    /// Explicit `ffmpeg` executable.
    ffmpeg: Option<PathBuf>,
}

/// Arguments of the `still` command.
#[derive(Debug, PartialEq, Eq)]
struct StillArgs {
    /// MDF program.
    input: PathBuf,
    /// Frame to render.
    frame: i64,
    /// Destination PNG.
    output: PathBuf,
}

/// Arguments of the `info` command.
#[derive(Debug, PartialEq, Eq)]
struct InfoArgs {
    /// MDF program.
    input: PathBuf,
    /// Print JSON instead of text.
    json: bool,
}

/// Arguments of the `bake` command.
#[derive(Debug, PartialEq, Eq)]
struct BakeArgs {
    /// MDF program whose overrides file is folded in.
    input: PathBuf,
    /// Only bake these node identities (all of them when empty).
    ids: Vec<String>,
}

/// Arguments of the `preview` command.
#[derive(Debug, PartialEq, Eq)]
struct PreviewArgs {
    /// MDF program.
    input: PathBuf,
    /// Initial frame.
    frame: Option<i64>,
    /// Start paused instead of playing.
    paused: bool,
    /// Close the window after this many presented frames.
    exit_after_frames: Option<u64>,
    /// Window size in physical pixels.
    window_size: Option<(u32, u32)>,
    /// Chrome scale override, stored in thousandths so the struct stays `Eq`.
    ui_scale_milli: Option<u32>,
    /// Outline row selected at startup.
    select: Option<usize>,
    /// Capture the window to this PNG and close it.
    screenshot: Option<PathBuf>,
    /// Endpoint file of the live control endpoint, when it is opened.
    control: Option<PathBuf>,
    /// Show the assets tab instead of the layers tab at startup.
    assets: bool,
}

/// Why the run stopped and with which exit code.
enum Failure {
    /// An MDF lexical, parse or runtime diagnostic (exit code 2).
    Diagnostic(String),
    /// Any other application error (exit code 1).
    Error(anyhow::Error),
}

impl From<anyhow::Error> for Failure {
    fn from(error: anyhow::Error) -> Self {
        Self::Error(error)
    }
}

impl From<RenderError> for Failure {
    fn from(error: RenderError) -> Self {
        Self::Error(error.into())
    }
}

impl From<ExportError> for Failure {
    fn from(error: ExportError) -> Self {
        Self::Error(error.into())
    }
}

impl From<motioner_ui::UiError> for Failure {
    fn from(error: motioner_ui::UiError) -> Self {
        Self::Error(error.into())
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Diagnostic(report) => f.write_str(report),
            Self::Error(error) => write!(f, "{error:#}"),
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    match run(&args) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(Failure::Diagnostic(report)) => {
            eprintln!("{report}");
            ExitCode::from(2)
        }
        Err(Failure::Error(error)) => {
            eprintln!("error: {error:#}");
            ExitCode::from(1)
        }
    }
}

/// Parses the arguments and dispatches the command. `Ok(false)` means a
/// command failed after printing its own (JSON) error.
fn run(args: &[OsString]) -> Result<bool, Failure> {
    let status = |status: agent::Status| matches!(status, agent::Status::Ok);
    match parse(args)? {
        Command::Help => {
            println!("{HELP}\n\n{}", agent::command_help());
            Ok(true)
        }
        Command::Info(args) => run_info(&args).map(|()| true),
        Command::New(args) => run_new(&args).map(|()| true),
        Command::Migrate(args) => run_migrate(&args).map(|()| true),
        Command::Bake(args) => run_bake(&args).map(|()| true),
        Command::Still(args) => run_still(&args).map(|()| true),
        Command::Render(args) => run_render(&args).map(|()| true),
        Command::Preview(args) => run_preview(&args).map(|()| true),
        Command::Query(args) => Ok(status(agent::run_query(&args))),
        Command::Edit(args) => Ok(status(agent::run_edit(&args))),
        Command::Ctl(args) => Ok(status(agent::run_ctl(&args))),
        Command::Desktop(args) => run_desktop(&args).map(|()| true),
        Command::Import(args) => run_import(&args).map(|()| true),
        Command::Backup(args) => run_backup(&args).map(|()| true),
    }
}

/// Parses the command line into a [`Command`].
fn parse(args: &[OsString]) -> Result<Command> {
    let Some(first) = args.first().and_then(|arg| arg.to_str()) else {
        return Ok(Command::Desktop(DesktopArgs::default()));
    };
    if first == "--help" || first == "-h" {
        return Ok(Command::Help);
    }
    let rest = &args[1..];
    match first {
        "--start" => parse_desktop(rest),
        "import" => parse_import(rest),
        "backup" => parse_backup(rest),
        "new" => parse_new(rest),
        "migrate" => parse_migrate(rest),
        "render" => parse_render(rest),
        "still" => parse_still(rest),
        "info" => parse_info(rest),
        "bake" => parse_bake(rest),
        "preview" => parse_preview(rest),
        "query" => parse_query(rest),
        "edit" => parse_edit(rest),
        "ctl" => parse_ctl(rest),
        other
            if !other.starts_with('-')
                && (other.ends_with(".motion")
                    || other.ends_with(".motioner")
                    || Path::new(other).exists()) =>
        {
            // `motioner <path>` opens it directly.
            parse_preview(args)
        }
        other => bail!("unknown command '{other}' (try --help)"),
    }
}

/// Parses `backup <project>`.
fn parse_backup(args: &[OsString]) -> Result<Command> {
    let parsed = parse_flags("backup", args, true, &[], &[])?;
    if !parsed.rest.is_empty() {
        bail!("backup takes exactly one project");
    }
    Ok(Command::Backup(BackupArgs {
        project: parsed
            .input
            .ok_or_else(|| anyhow!("backup needs a project"))?,
    }))
}

/// Writes one backup snapshot and prints its folder.
fn run_backup(args: &BackupArgs) -> std::result::Result<(), Failure> {
    let project = Project::open(&args.project).map_err(|error| Failure::Error(anyhow!(error)))?;
    let info = project
        .snapshot(std::time::SystemTime::now())
        .map_err(|error| Failure::Error(anyhow!(error)))?;
    println!(
        "Backup {} ({} files, {} bytes)",
        info.path.display(),
        info.files,
        info.bytes
    );
    Ok(())
}

/// Parses `import <project> <file>...`.
fn parse_import(args: &[OsString]) -> Result<Command> {
    let mut values: Vec<PathBuf> = Vec::new();
    for argument in args {
        let Some(text) = argument.to_str() else {
            bail!("arguments must be valid UTF-8");
        };
        if text.starts_with("--") {
            bail!("unknown flag '{text}' for import");
        }
        values.push(PathBuf::from(text));
    }
    if values.len() < 2 {
        bail!("import takes a project and at least one file");
    }
    let project = values.remove(0);
    Ok(Command::Import(ImportArgs {
        project,
        files: values,
    }))
}

/// Imports files into a project and prints their keys.
fn run_import(args: &ImportArgs) -> std::result::Result<(), Failure> {
    let root = std::env::current_dir().unwrap_or_default();
    let host = host::Host::new(root);
    let services: &dyn motioner_ui::ProjectHost = &host;
    let composition = services
        .open_project(&args.project)
        .map_err(|message| Failure::Error(anyhow!(message)))?;
    for file in &args.files {
        let summary = services
            .import_asset(&composition, file)
            .map_err(|message| Failure::Error(anyhow!(message)))?;
        println!(
            "{} <- {} ({} bytes, used by {})",
            summary.key,
            file.display(),
            summary.size_bytes,
            summary.used_by
        );
    }
    Ok(())
}

/// Parses the start screen's smoke-test flags.
fn parse_desktop(args: &[OsString]) -> Result<Command> {
    let parsed = parse_flags(
        "start",
        args,
        false,
        &[
            "--window-size",
            "--ui-scale",
            "--screenshot",
            "--exit-after-frames",
        ],
        &[],
    )?;
    if !parsed.rest.is_empty() {
        bail!("the start screen takes no positional arguments");
    }
    Ok(Command::Desktop(DesktopArgs {
        exit_after_frames: parsed
            .value("--exit-after-frames")
            .map(parse_u64)
            .transpose()?,
        window_size: parsed.value("--window-size").map(parse_size).transpose()?,
        ui_scale_milli: parsed
            .value("--ui-scale")
            .map(|text| -> Result<u32> {
                let scale: f64 = text
                    .parse()
                    .with_context(|| format!("invalid scale '{text}' for --ui-scale"))?;
                if !(0.5..=4.0).contains(&scale) {
                    bail!("--ui-scale must be between 0.5 and 4.0, got '{text}'");
                }
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                Ok((scale * 1000.0).round() as u32)
            })
            .transpose()?,
        screenshot: parsed.value("--screenshot").map(PathBuf::from),
    }))
}

/// Parses a whole number.
fn parse_u64(text: &str) -> Result<u64> {
    text.parse()
        .with_context(|| format!("invalid whole number '{text}'"))
}

/// Parses `new <dir> [--name NAME] [--size WxH] [--fps N] [--duration FRAMES]`.
fn parse_new(args: &[OsString]) -> Result<Command> {
    let parsed = parse_flags(
        "new",
        args,
        false,
        &["--name", "--size", "--fps", "--duration"],
        &[],
    )?;
    if parsed.rest.len() > 1 {
        bail!("new takes one destination folder");
    }
    let dir = parsed
        .rest
        .first()
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("new needs a destination folder"))?;
    let size = parsed.value("--size").map(parse_size).transpose()?;
    let fps = parsed.value("--fps").map(parse_u32).transpose()?;
    let duration = parsed.value("--duration").map(parse_i64).transpose()?;
    Ok(Command::New(NewArgs {
        dir,
        name: parsed.value("--name").map(str::to_owned),
        size,
        fps,
        duration,
    }))
}

/// Parses `migrate <file.motion> <dir> [--name NAME]`.
fn parse_migrate(args: &[OsString]) -> Result<Command> {
    let parsed = parse_flags("migrate", args, false, &["--name"], &[])?;
    if parsed.rest.len() != 2 {
        bail!("migrate takes a .motion file and a destination folder");
    }
    Ok(Command::Migrate(MigrateArgs {
        input: PathBuf::from(&parsed.rest[0]),
        dir: PathBuf::from(&parsed.rest[1]),
        name: parsed.value("--name").map(str::to_owned),
    }))
}

/// Parses a `WxH` size value.
fn parse_size(text: &str) -> Result<(u32, u32)> {
    let (width, height) = text
        .split_once(['x', 'X'])
        .ok_or_else(|| anyhow!("--size expects WxH, got '{text}'"))?;
    Ok((parse_u32(width)?, parse_u32(height)?))
}

/// Parses a positive whole number.
fn parse_u32(text: &str) -> Result<u32> {
    let value: u32 = text
        .parse()
        .with_context(|| format!("invalid whole number '{text}'"))?;
    if value == 0 {
        bail!("'{text}' must be positive");
    }
    Ok(value)
}

/// Parses a positive frame count.
fn parse_i64(text: &str) -> Result<i64> {
    let value: i64 = text
        .parse()
        .with_context(|| format!("invalid whole number '{text}'"))?;
    if value <= 0 {
        bail!("'{text}' must be positive");
    }
    Ok(value)
}

/// Parses `render <file> --out <file> [--from N] [--to N] [--ffmpeg PATH]`.
fn parse_render(args: &[OsString]) -> Result<Command> {
    let mut input = None;
    let mut output = None;
    let mut from = None;
    let mut to = None;
    let mut ffmpeg = None;
    let mut index = 0;
    while index < args.len() {
        let Some(arg) = args[index].to_str() else {
            bail!("arguments must be valid UTF-8");
        };
        match arg {
            "--out" => {
                output = Some(PathBuf::from(flag_value(args, index, "--out")?));
                index += 2;
            }
            "--from" => {
                from = Some(frame_value(args, index, "--from")?);
                index += 2;
            }
            "--to" => {
                to = Some(frame_value(args, index, "--to")?);
                index += 2;
            }
            "--ffmpeg" => {
                ffmpeg = Some(PathBuf::from(flag_value(args, index, "--ffmpeg")?));
                index += 2;
            }
            other if other.starts_with("--") => bail!("unknown flag '{other}' for render"),
            _ => {
                if input.is_some() {
                    bail!("unexpected argument '{arg}' (already got the input file)");
                }
                input = Some(PathBuf::from(&args[index]));
                index += 1;
            }
        }
    }
    let input = input.ok_or_else(|| anyhow!("render needs an input .motion file"))?;
    let output = output.ok_or_else(|| anyhow!("render needs --out <file.mp4>"))?;
    Ok(Command::Render(RenderArgs {
        input,
        output,
        from,
        to,
        ffmpeg,
    }))
}

/// Parses `still <file> --frame N --out <file>`.
fn parse_still(args: &[OsString]) -> Result<Command> {
    let mut input = None;
    let mut frame = None;
    let mut output = None;
    let mut index = 0;
    while index < args.len() {
        let Some(arg) = args[index].to_str() else {
            bail!("arguments must be valid UTF-8");
        };
        match arg {
            "--frame" => {
                frame = Some(frame_value(args, index, "--frame")?);
                index += 2;
            }
            "--out" => {
                output = Some(PathBuf::from(flag_value(args, index, "--out")?));
                index += 2;
            }
            other if other.starts_with("--") => bail!("unknown flag '{other}' for still"),
            _ => {
                if input.is_some() {
                    bail!("unexpected argument '{arg}' (already got the input file)");
                }
                input = Some(PathBuf::from(&args[index]));
                index += 1;
            }
        }
    }
    let input = input.ok_or_else(|| anyhow!("still needs an input .motion file"))?;
    let frame = frame.ok_or_else(|| anyhow!("still needs --frame N"))?;
    let output = output.ok_or_else(|| anyhow!("still needs --out <file.png>"))?;
    Ok(Command::Still(StillArgs {
        input,
        frame,
        output,
    }))
}

/// Parses `info <file> [--json]`.
fn parse_info(args: &[OsString]) -> Result<Command> {
    let parsed = parse_flags("info", args, true, &[], &["--json"])?;
    if !parsed.rest.is_empty() {
        bail!("info takes exactly one input file");
    }
    Ok(Command::Info(InfoArgs {
        input: parsed
            .input
            .clone()
            .ok_or_else(|| anyhow!("info needs an input .motion file"))?,
        json: parsed.switches.contains(&"--json"),
    }))
}

/// Flags and positional values of a command line.
struct Parsed {
    /// The first positional value, when the command takes an input file.
    input: Option<PathBuf>,
    /// Values of flags that take one.
    values: Vec<(&'static str, String)>,
    /// Switches that were given.
    switches: Vec<&'static str>,
    /// The other positional values, in order.
    rest: Vec<String>,
}

/// Splits `args` into the input file, flag values, switches and the
/// remaining positional values. Unknown `--` flags are errors.
fn parse_flags(
    command: &str,
    args: &[OsString],
    with_input: bool,
    value_flags: &[&'static str],
    switch_flags: &[&'static str],
) -> Result<Parsed> {
    let mut parsed = Parsed {
        input: None,
        values: Vec::new(),
        switches: Vec::new(),
        rest: Vec::new(),
    };
    let mut index = 0;
    while index < args.len() {
        let Some(arg) = args[index].to_str() else {
            bail!("arguments must be valid UTF-8");
        };
        if let Some(flag) = value_flags.iter().find(|flag| **flag == arg) {
            let value = flag_value(args, index, flag)?;
            let value = value
                .to_str()
                .ok_or_else(|| anyhow!("the value of {flag} must be valid UTF-8"))?;
            parsed.values.push((flag, value.to_owned()));
            index += 2;
            continue;
        }
        if let Some(flag) = switch_flags.iter().find(|flag| **flag == arg) {
            parsed.switches.push(flag);
            index += 1;
            continue;
        }
        if arg.starts_with("--") {
            bail!("unknown flag '{arg}' for {command}");
        }
        if with_input && parsed.input.is_none() {
            parsed.input = Some(PathBuf::from(&args[index]));
        } else {
            parsed.rest.push(arg.to_owned());
        }
        index += 1;
    }
    Ok(parsed)
}

impl Parsed {
    /// The last value given for `flag`.
    fn value(&self, flag: &str) -> Option<&str> {
        self.values
            .iter()
            .rev()
            .find(|(name, _)| *name == flag)
            .map(|(_, value)| value.as_str())
    }

    /// The `--frame` value.
    fn frame(&self) -> Result<Option<i64>> {
        self.value("--frame")
            .map(|text| {
                text.parse::<i64>()
                    .with_context(|| format!("invalid frame number '{text}' for --frame"))
            })
            .transpose()
    }
}

/// Parses `query <file> [--frame N] [--node ID]`.
fn parse_query(args: &[OsString]) -> Result<Command> {
    let parsed = parse_flags("query", args, true, &["--frame", "--node"], &[])?;
    if !parsed.rest.is_empty() {
        bail!("query takes one input file");
    }
    Ok(Command::Query(agent::QueryArgs {
        input: parsed
            .input
            .clone()
            .ok_or_else(|| anyhow!("query needs an input .motion file"))?,
        frame: parsed.frame()?,
        node: parsed.value("--node").map(str::to_owned),
    }))
}

/// Parses `edit <file> [--frame N] [--dry-run] <command>...`.
fn parse_edit(args: &[OsString]) -> Result<Command> {
    let parsed = parse_flags("edit", args, true, &["--frame"], &["--dry-run"])?;
    let input = parsed
        .input
        .clone()
        .ok_or_else(|| anyhow!("edit needs an input .motion file"))?;
    if parsed.rest.is_empty() {
        bail!("edit needs at least one command, for example 'seek(30)'");
    }
    Ok(Command::Edit(agent::EditArgs {
        input,
        frame: parsed.frame()?,
        dry_run: parsed.switches.contains(&"--dry-run"),
        commands: parsed.rest,
    }))
}

/// Parses `ctl [--endpoint FILE] <request>...`.
fn parse_ctl(args: &[OsString]) -> Result<Command> {
    let parsed = parse_flags("ctl", args, false, &["--endpoint"], &[])?;
    if parsed.rest.is_empty() {
        bail!("ctl needs at least one request, for example 'state()'");
    }
    Ok(Command::Ctl(agent::CtlArgs {
        endpoint: parsed.value("--endpoint").map(PathBuf::from),
        requests: parsed.rest,
    }))
}

/// Parses `bake <file> [--id ID]...`.
fn parse_bake(args: &[OsString]) -> Result<Command> {
    let mut input = None;
    let mut ids = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let Some(arg) = args[index].to_str() else {
            bail!("arguments must be valid UTF-8");
        };
        match arg {
            "--id" => {
                let value = flag_value(args, index, "--id")?;
                let text = value
                    .to_str()
                    .ok_or_else(|| anyhow!("the value of --id must be valid UTF-8"))?;
                ids.push(text.to_owned());
                index += 2;
            }
            other if other.starts_with("--") => bail!("unknown flag '{other}' for bake"),
            _ => {
                if input.is_some() {
                    bail!("unexpected argument '{arg}' (already got the input file)");
                }
                input = Some(PathBuf::from(&args[index]));
                index += 1;
            }
        }
    }
    let input = input.ok_or_else(|| anyhow!("bake needs an input .motion file"))?;
    Ok(Command::Bake(BakeArgs { input, ids }))
}

/// Parses `preview <file> [--frame N] [--paused] [--exit-after-frames N]`.
fn parse_preview(args: &[OsString]) -> Result<Command> {
    let mut input = None;
    let mut frame = None;
    let mut paused = false;
    let mut exit_after_frames = None;
    let mut window_size = None;
    let mut ui_scale_milli = None;
    let mut select = None;
    let mut screenshot = None;
    let mut control = None;
    let mut assets = false;
    let mut index = 0;
    while index < args.len() {
        let Some(arg) = args[index].to_str() else {
            bail!("arguments must be valid UTF-8");
        };
        match arg {
            "--frame" => {
                frame = Some(frame_value(args, index, "--frame")?);
                index += 2;
            }
            "--paused" => {
                paused = true;
                index += 1;
            }
            "--exit-after-frames" => {
                exit_after_frames = Some(count_value(args, index, "--exit-after-frames")?);
                index += 2;
            }
            "--window-size" => {
                window_size = Some(size_value(args, index)?);
                index += 2;
            }
            "--ui-scale" => {
                ui_scale_milli = Some(scale_value(args, index)?);
                index += 2;
            }
            "--select" => {
                let row = count_value(args, index, "--select")?;
                select = Some(usize::try_from(row).context("--select row is too large")?);
                index += 2;
            }
            "--screenshot" => {
                screenshot = Some(PathBuf::from(flag_value(args, index, "--screenshot")?));
                index += 2;
            }
            "--assets" => {
                assets = true;
                index += 1;
            }
            "--control" => {
                control.get_or_insert_with(motioner_ui::default_endpoint_file);
                index += 1;
            }
            "--control-file" => {
                control = Some(PathBuf::from(flag_value(args, index, "--control-file")?));
                index += 2;
            }
            other if other.starts_with("--") => bail!("unknown flag '{other}' for preview"),
            _ => {
                if input.is_some() {
                    bail!("unexpected argument '{arg}' (already got the input file)");
                }
                input = Some(PathBuf::from(&args[index]));
                index += 1;
            }
        }
    }
    let input = input.ok_or_else(|| anyhow!("preview needs an input .motion file"))?;
    Ok(Command::Preview(PreviewArgs {
        input,
        frame,
        paused,
        exit_after_frames,
        window_size,
        ui_scale_milli,
        select,
        screenshot,
        control,
        assets,
    }))
}

/// Parses the `WxH` value of `--window-size`.
fn size_value(args: &[OsString], index: usize) -> Result<(u32, u32)> {
    let value = flag_value(args, index, "--window-size")?;
    let text = value
        .to_str()
        .ok_or_else(|| anyhow!("the value of --window-size must be valid UTF-8"))?;
    let (w, h) = text
        .split_once(['x', 'X'])
        .ok_or_else(|| anyhow!("--window-size expects WxH, got '{text}'"))?;
    let w: u32 = w
        .parse()
        .with_context(|| format!("invalid width in '{text}'"))?;
    let h: u32 = h
        .parse()
        .with_context(|| format!("invalid height in '{text}'"))?;
    if !(320..=8192).contains(&w) || !(240..=8192).contains(&h) {
        bail!("--window-size must be between 320x240 and 8192x8192, got '{text}'");
    }
    Ok((w, h))
}

/// Parses the `--ui-scale` factor (0.5 to 4.0) into thousandths.
fn scale_value(args: &[OsString], index: usize) -> Result<u32> {
    let value = flag_value(args, index, "--ui-scale")?;
    let text = value
        .to_str()
        .ok_or_else(|| anyhow!("the value of --ui-scale must be valid UTF-8"))?;
    let scale: f64 = text
        .parse()
        .with_context(|| format!("invalid scale '{text}' for --ui-scale"))?;
    if !(0.5..=4.0).contains(&scale) {
        bail!("--ui-scale must be between 0.5 and 4.0, got '{text}'");
    }
    // In range, so the rounded value fits in u32.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ok((scale * 1000.0).round() as u32)
}

/// Reads the value that follows a flag, rejecting a missing value or another
/// flag.
fn flag_value(args: &[OsString], index: usize, flag: &str) -> Result<OsString> {
    let Some(value) = args.get(index + 1) else {
        bail!("missing value for {flag}");
    };
    if value.to_str().is_some_and(|text| text.starts_with("--")) {
        bail!("missing value for {flag}");
    }
    Ok(value.clone())
}

/// Reads and parses the integer value that follows a frame flag.
fn frame_value(args: &[OsString], index: usize, flag: &str) -> Result<i64> {
    let value = flag_value(args, index, flag)?;
    let text = value
        .to_str()
        .ok_or_else(|| anyhow!("the value of {flag} must be valid UTF-8"))?;
    text.parse::<i64>()
        .with_context(|| format!("invalid frame number '{text}' for {flag}"))
}

/// Reads and parses the non-negative count that follows a flag.
fn count_value(args: &[OsString], index: usize, flag: &str) -> Result<u64> {
    let value = flag_value(args, index, flag)?;
    let text = value
        .to_str()
        .ok_or_else(|| anyhow!("the value of {flag} must be valid UTF-8"))?;
    text.parse::<u64>()
        .with_context(|| format!("invalid frame count '{text}' for {flag}"))
}
/// A resolved input: the composition file to load plus the asset root.
pub(crate) struct InputTarget {
    /// Program file to load and edit.
    composition: PathBuf,
    /// Project root, or the process working directory in single-file mode.
    root: PathBuf,
    /// The project, when the input belongs to one.
    project: Option<Project>,
}

/// Resolves a folder, a manifest or a program inside a project to the main
/// composition and its asset root (D-0045). A loose file keeps single-file
/// mode with the working directory as its root.
pub(crate) fn resolve_input(path: &Path) -> std::result::Result<InputTarget, Failure> {
    match Project::open(path) {
        Ok(project) => {
            let composition = project.composition_path(&project.settings().main);
            let root = project.root().to_path_buf();
            Ok(InputTarget {
                composition,
                root,
                project: Some(project),
            })
        }
        Err(motioner_io::ProjectError::NotFound { .. }) => Ok(InputTarget {
            composition: path.to_path_buf(),
            root: std::env::current_dir().unwrap_or_default(),
            project: None,
        }),
        Err(error) => Err(Failure::Error(anyhow!(error))),
    }
}

/// Opens the start screen with no project (double-clicking the binary).
fn run_desktop(args: &DesktopArgs) -> std::result::Result<(), Failure> {
    eprintln!("desktop: enter");
    let root = std::env::current_dir().unwrap_or_default();
    let services = host::Host::new(root);
    let placement = services.state().window;
    let services: std::rc::Rc<dyn motioner_ui::ProjectHost> = std::rc::Rc::new(services);
    let fonts = FontSet::bundled();
    let options = motioner_ui::PreviewOptions {
        host: Some(std::rc::Rc::clone(&services)),
        start: None,
        exit_after_frames: args.exit_after_frames,
        window_size: args.window_size.or_else(|| {
            (placement.width > 0 && placement.height > 0)
                .then_some((placement.width, placement.height))
        }),
        #[allow(clippy::cast_precision_loss)]
        ui_scale: args.ui_scale_milli.map(|milli| milli as f32 / 1000.0),
        screenshot: args.screenshot.clone(),
        png_writer: Some(write_screenshot),
        text_metrics: Some(Arc::new(CpuTextMetrics::new(fonts.clone()))),
        fonts: Some(fonts),
        ..motioner_ui::PreviewOptions::default()
    };
    if let Some(path) = &args.screenshot {
        ensure_parent(path)?;
    }
    motioner_ui::run_app(options)?;
    if let Some(path) = &args.screenshot {
        println!("Wrote {}", path.display());
    }
    Ok(())
}

/// Opens the live preview window.
fn run_preview(args: &PreviewArgs) -> std::result::Result<(), Failure> {
    let target = resolve_input(&args.input)?;
    let fonts = project_fonts(&target);
    let resolver = match &target.project {
        Some(project) => ProjectImages::new(project),
        None => ProjectImages::loose(&target.root),
    };
    let options = motioner_ui::PreviewOptions {
        frame: args.frame,
        paused: args.paused,
        exit_after_frames: args.exit_after_frames,
        window_size: args.window_size,
        #[allow(clippy::cast_precision_loss)] // at most 4000
        ui_scale: args.ui_scale_milli.map(|milli| milli as f32 / 1000.0),
        select: args.select,
        screenshot: args.screenshot.clone(),
        png_writer: Some(write_screenshot),
        control: args.control.clone(),
        text_metrics: Some(Arc::new(CpuTextMetrics::new(fonts.clone()))),
        fonts: Some(fonts),
        image_resolver: Some(std::rc::Rc::new(resolver)),
        host: Some(std::rc::Rc::new(host::Host::new(
            std::env::current_dir().unwrap_or_default(),
        ))),
        start: None,
        panel_tab: args.assets.then_some(motioner_ui::LeftTab::Assets),
    };
    if let Some(path) = &args.screenshot {
        ensure_parent(path)?;
    }
    motioner_ui::run_preview(&target.composition, options)?;
    if let Some(path) = &args.screenshot {
        println!("Wrote {}", path.display());
    }
    Ok(())
}

/// The [`motioner_ui::PngWriter`] used for in-app screenshots.
fn write_screenshot(path: &Path, width: u32, height: u32, rgba: &[u8]) -> Result<(), String> {
    write_png(path, width, height, rgba).map_err(|error| error.to_string())
}

/// The bundled faces plus every project face (D-0034, D-0045).
///
/// Project faces win on a family/weight clash. A project contributes its
/// registry fonts; single-file mode reads `assets/fonts/` under the root.
pub(crate) fn project_fonts(target: &InputTarget) -> FontSet {
    let mut fonts = FontSet::bundled();
    let label = match &target.project {
        Some(project) => project.root().join("assets"),
        None => target.root.join("assets/fonts"),
    };
    fonts.set_project_label(label.display().to_string());
    if let Some(project) = &target.project {
        if let Ok(faces) = motioner_io::project_fonts(project) {
            for face in faces {
                fonts.push_face(face.family, face.weight, face.bytes);
            }
        }
    }
    let dir = target.root.join("assets/fonts");
    if let Ok(project) = motioner_io::load_project_fonts(&dir) {
        for face in project {
            fonts.push_face(face.family, face.weight, face.bytes);
        }
    }
    fonts
}

/// Unique family names in `fonts`, sorted for the inspector picker.
fn font_families(fonts: &FontSet) -> Vec<String> {
    let mut families: Vec<String> = fonts.faces().map(|(family, _)| family.to_owned()).collect();
    families.sort();
    families.dedup();
    families
}

/// Loads and executes an MDF program inside `target`.
///
/// Returns the source text, the document and the image resolver. An MDF
/// diagnostic becomes [`Failure::Diagnostic`] so `main` can print it and exit
/// with code 2.
fn load_document(
    target: &InputTarget,
) -> std::result::Result<(String, Document, ProjectImages), Failure> {
    let path = &target.composition;
    let source = fs::read_to_string(path)
        .with_context(|| format!("cannot read {}", path.display()))
        .map_err(Failure::Error)?;
    let name = path.display().to_string();
    let metrics = CpuTextMetrics::new(project_fonts(target));
    match motioner_dsl::run_with_metrics(&source, &Limits::default(), &metrics) {
        Ok(mut document) => {
            let images = match &target.project {
                Some(project) => ProjectImages::new(project),
                None => ProjectImages::loose(&target.root),
            };
            for issue in document.resolve_images(&images) {
                eprintln!(
                    "warning: image {} ({}): {}",
                    issue.source, issue.node, issue.message
                );
            }
            Ok((source, document, images))
        }
        Err(diagnostic) => Err(Failure::Diagnostic(diagnostic.render(&source, &name))),
    }
}

/// Every decoded image as the renderer's upload set.
fn image_set(images: &ProjectImages) -> ImageSet {
    let mut set = ImageSet::new();
    for image in images.decoded() {
        set.insert(ImageData {
            key: image.key,
            width: image.width,
            height: image.height,
            rgba8: image.rgba8,
        });
    }
    set
}

/// Creates the output's parent directory when it does not exist.
fn ensure_parent(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("cannot create {}", parent.display()))?;
        }
    }
    Ok(())
}

/// Creates the project folder for `motioner new`.
fn run_new(args: &NewArgs) -> std::result::Result<(), Failure> {
    let dir = &args.dir;
    let default_name = dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("untitled");
    let name = args.name.as_deref().unwrap_or(default_name);
    let mut settings = ProjectSettings::new(name);
    if let Some((width, height)) = args.size {
        settings.width = width;
        settings.height = height;
    }
    if let Some(fps) = args.fps {
        settings.fps_numerator = fps;
        settings.fps_denominator = 1;
    }
    if let Some(duration) = args.duration {
        settings.duration = duration;
    }
    let mut options = ProjectOptions::new(name);
    options.program = motioner_io::default_program(&settings, "Main");
    options.settings = settings;
    let project = Project::create(dir, &options).map_err(|error| Failure::Error(anyhow!(error)))?;
    println!("Created {}", project.root().display());
    println!(
        "Composition: {}",
        project.composition_path("main").display()
    );
    Ok(())
}

/// Copies a loose program and the assets it references into a project.
fn run_migrate(args: &MigrateArgs) -> std::result::Result<(), Failure> {
    let source = fs::read_to_string(&args.input)
        .with_context(|| format!("cannot read {}", args.input.display()))
        .map_err(Failure::Error)?;
    let default_name = args
        .dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("untitled");
    let name = args.name.as_deref().unwrap_or(default_name);
    let mut options = ProjectOptions::new(name);
    options.program = source;
    let mut project =
        Project::create(&args.dir, &options).map_err(|error| Failure::Error(anyhow!(error)))?;
    let root = std::env::current_dir().unwrap_or_default();
    let loose = InputTarget {
        composition: args.input.clone(),
        root: root.clone(),
        project: None,
    };
    let (_, document, _) = load_document(&loose)?;
    let mut imported = 0_usize;
    for node in document.nodes() {
        if let motioner_core::NodeContent::Image(image) = &node.content() {
            let candidate = root.join(&image.source);
            if candidate.is_file() {
                if let Ok(outcome) = project.import_file(&candidate) {
                    if !outcome.deduped {
                        imported += 1;
                    }
                }
            }
        }
    }
    let fonts_dir = root.join("assets/fonts");
    if let Ok(entries) = fs::read_dir(&fonts_dir) {
        for entry in entries.flatten() {
            if entry.path().is_file() {
                imported += usize::from(
                    project
                        .import_file(&entry.path())
                        .is_ok_and(|outcome| !outcome.deduped),
                );
            }
        }
    }
    println!(
        "Created {} from {} ({} asset(s) imported)",
        project.root().display(),
        args.input.display(),
        imported
    );
    Ok(())
}

/// Folds the overrides file into the program (`motioner bake`) through the
/// editor's `bake` command, so the CLI, the window and `ctl` share one
/// implementation.
fn run_bake(args: &BakeArgs) -> std::result::Result<(), Failure> {
    let target = resolve_input(&args.input)?;
    let fonts = project_fonts(&target);
    let families = font_families(&fonts);
    let mut editor = motioner_ui::Editor::open_with_metrics(
        &target.composition,
        Arc::new(CpuTextMetrics::new(fonts)),
    )
    .map_err(|error| match error.kind {
        motioner_ui::ErrorKind::NoDocument => Failure::Diagnostic(error.message),
        _ => Failure::Error(anyhow!("{}", error.message)),
    })?;
    editor.set_font_families(families);
    for warning in editor.session().warnings() {
        println!("warning: line {}: {}", warning.line, warning.message);
    }
    let outcome = editor
        .execute(
            &motioner_ui::Command::Bake(args.ids.clone()),
            Duration::ZERO,
        )
        .map_err(|error| Failure::Error(anyhow!("{error}")))?;
    for line in outcome.summary.split("; ") {
        println!("{line}");
    }
    println!(
        "Wrote {} and {}",
        target.composition.display(),
        editor.session().overrides_path().display()
    );
    Ok(())
}

/// Prints composition facts for the `info` command.
fn run_info(args: &InfoArgs) -> std::result::Result<(), Failure> {
    let target = resolve_input(&args.input)?;
    let (_, document, _images) = load_document(&target)?;
    let composition = document.composition();
    let rate = composition.frame_rate();
    let frames = composition.duration();
    #[allow(clippy::cast_precision_loss)] // display only
    let seconds = frames as f64 * f64::from(rate.denominator()) / f64::from(rate.numerator());
    if args.json {
        let mut j = motioner_ui::Json::new();
        j.begin_object();
        j.key("file")
            .string(&target.composition.display().to_string());
        j.key("project").opt_string(
            target
                .project
                .as_ref()
                .map(|project| project.root().display().to_string())
                .as_deref(),
        );
        j.key("name").string(composition.name());
        j.key("width").int(i64::from(composition.width()));
        j.key("height").int(i64::from(composition.height()));
        j.key("fps")
            .string(&format!("{}/{}", rate.numerator(), rate.denominator()));
        j.key("duration").int(frames);
        j.key("seconds").number(seconds);
        j.key("nodes")
            .int(i64::try_from(document.len()).unwrap_or(i64::MAX));
        if let Some(project) = &target.project {
            j.key("assets")
                .int(i64::try_from(project.assets().len()).unwrap_or(i64::MAX));
            j.key("compositions")
                .int(i64::try_from(project.compositions().len()).unwrap_or(i64::MAX));
        }
        j.end_object();
        println!("{}", j.finish());
        return Ok(());
    }
    if let Some(project) = &target.project {
        println!("Project: {}", project.root().display());
        println!("Project name: {}", project.settings().name);
        println!("Main composition: {}", project.settings().main);
        println!("Compositions: {}", project.compositions().len());
        println!("Assets: {}", project.assets().len());
    }
    println!("File: {}", target.composition.display());
    println!("Composition: {}", composition.name());
    println!("Canvas: {}x{}", composition.width(), composition.height());
    println!(
        "Frame rate: {}/{} ({:.3} fps)",
        rate.numerator(),
        rate.denominator(),
        rate.as_f64()
    );
    println!("Duration: {frames} frames ({seconds:.3} s)");
    println!("Nodes: {}", document.len());
    Ok(())
}

/// Renders one frame to a PNG file.
fn run_still(args: &StillArgs) -> std::result::Result<(), Failure> {
    let target = resolve_input(&args.input)?;
    let (_, document, images) = load_document(&target)?;
    if let Some(issue) = document.first_unresolved_image() {
        return Err(Failure::Error(anyhow!(RenderError::UnresolvedImage {
            source: issue.source,
        })));
    }
    let composition = document.composition();
    if args.frame < 0 || args.frame >= composition.duration() {
        return Err(Failure::Error(anyhow!(
            "frame {} is outside the composition range 0..{}",
            args.frame,
            composition.duration()
        )));
    }
    ensure_parent(&args.output)?;

    let mut renderer = FrameRenderer::with_fonts(
        composition.width(),
        composition.height(),
        project_fonts(&target),
    )?;
    renderer.set_images(image_set(&images));
    let mut rgba = Vec::new();
    renderer.render_frame(&document, args.frame, &mut rgba)?;
    write_png(
        &args.output,
        composition.width(),
        composition.height(),
        &rgba,
    )?;
    println!(
        "Wrote {} ({}x{}, frame {}, adapter {})",
        args.output.display(),
        composition.width(),
        composition.height(),
        args.frame,
        renderer.adapter_name()
    );
    Ok(())
}

/// Renders a frame range and streams it to ffmpeg.
fn run_render(args: &RenderArgs) -> std::result::Result<(), Failure> {
    let target = resolve_input(&args.input)?;
    let (_, document, images) = load_document(&target)?;
    if let Some(issue) = document.first_unresolved_image() {
        return Err(Failure::Error(anyhow!(RenderError::UnresolvedImage {
            source: issue.source,
        })));
    }
    let composition = document.composition();
    let duration = composition.duration();
    let from = args.from.unwrap_or(0);
    let to = args.to.unwrap_or(duration);
    if from < 0 || from >= to || to > duration {
        return Err(Failure::Error(anyhow!(
            "invalid frame range [{from}, {to}) for a {duration}-frame composition"
        )));
    }
    ensure_parent(&args.output)?;

    let rate = composition.frame_rate();
    let settings = VideoSettings {
        width: composition.width(),
        height: composition.height(),
        fps_numerator: rate.numerator(),
        fps_denominator: rate.denominator(),
        crf: VideoSettings::youtube_1080p().crf,
    };
    let location = args
        .ffmpeg
        .as_ref()
        .map_or(FfmpegLocation::FromEnvOrPath, |path| {
            FfmpegLocation::Path(path.clone())
        });

    let mut renderer =
        FrameRenderer::with_fonts(settings.width, settings.height, project_fonts(&target))?;
    renderer.set_images(image_set(&images));
    let mut encoder = Mp4Encoder::start(&args.output, &settings, &location)?;
    let mut rgba = Vec::new();

    let total = u64::try_from(to - from).unwrap_or(0);
    let started = Instant::now();
    let mut render_time = Duration::ZERO;
    let mut encode_time = Duration::ZERO;
    let mut missing_glyphs = 0_u64;
    let mut completed = 0_u64;

    for frame in from..to {
        let render_start = Instant::now();
        let stats = renderer.render_frame(&document, frame, &mut rgba)?;
        render_time += render_start.elapsed();
        missing_glyphs = missing_glyphs.saturating_add(u64::from(stats.missing_glyphs));

        let encode_start = Instant::now();
        encoder.push_frame(&rgba)?;
        encode_time += encode_start.elapsed();

        completed += 1;
        if completed.is_multiple_of(30) {
            let percent = if total == 0 {
                100.0
            } else {
                #[allow(clippy::cast_precision_loss)] // progress display only
                let value = completed as f64 / total as f64 * 100.0;
                value
            };
            println!(
                "frame {completed}/{total} ({percent:.1}%) elapsed {:.2} s",
                started.elapsed().as_secs_f64()
            );
        }
    }

    let encode = encoder.finish()?;
    let elapsed = started.elapsed();
    let seconds = elapsed.as_secs_f64();
    println!(
        "Rendered {completed} frames in {seconds:.3} s ({} encoded)",
        encode.frames
    );
    print_average("render+readback", render_time, completed);
    print_average("encode", encode_time, completed);
    println!("Adapter: {}", renderer.adapter_name());
    println!("Missing glyphs: {missing_glyphs}");
    println!("Output: {}", args.output.display());
    Ok(())
}

/// Prints one per-frame average timing line.
fn print_average(stage: &str, total: Duration, frames: u64) {
    #[allow(clippy::cast_precision_loss)] // timing report only
    let frames = frames as f64;
    let average = if frames == 0.0 {
        0.0
    } else {
        total.as_secs_f64() * 1000.0 / frames
    };
    println!("  {stage}: {average:.3} ms/frame");
}

#[cfg(test)]
mod tests {
    use super::{agent, parse, BakeArgs, Command, PreviewArgs, RenderArgs, StillArgs};
    use std::ffi::OsString;
    use std::path::PathBuf;

    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn parses_new_and_migrate() {
        assert_eq!(
            parse(&args(&[
                "new",
                "demo",
                "--name",
                "Demo",
                "--size",
                "1280x720",
                "--fps",
                "30",
                "--duration",
                "120"
            ]))
            .unwrap(),
            Command::New(super::NewArgs {
                dir: PathBuf::from("demo"),
                name: Some("Demo".to_owned()),
                size: Some((1280, 720)),
                fps: Some(30),
                duration: Some(120),
            })
        );
        assert_eq!(
            parse(&args(&["migrate", "show.motion", "demo", "--name", "Demo"])).unwrap(),
            Command::Migrate(super::MigrateArgs {
                input: PathBuf::from("show.motion"),
                dir: PathBuf::from("demo"),
                name: Some("Demo".to_owned()),
            })
        );
        assert!(parse(&args(&["new"])).is_err());
        assert!(parse(&args(&["new", "a", "b"])).is_err());
        assert!(parse(&args(&["new", "a", "--size", "wide"])).is_err());
        assert!(parse(&args(&["new", "a", "--fps", "0"])).is_err());
        assert!(parse(&args(&["migrate", "a.motion"])).is_err());
        assert!(parse(&args(&["migrate", "a.motion", "b", "c"])).is_err());
    }

    #[test]
    fn parses_render_with_all_options() {
        let command = parse(&args(&[
            "render",
            "in.motion",
            "--out",
            "out.mp4",
            "--from",
            "10",
            "--to",
            "20",
            "--ffmpeg",
            "ffmpeg.exe",
        ]))
        .unwrap();
        assert_eq!(
            command,
            Command::Render(RenderArgs {
                input: PathBuf::from("in.motion"),
                output: PathBuf::from("out.mp4"),
                from: Some(10),
                to: Some(20),
                ffmpeg: Some(PathBuf::from("ffmpeg.exe")),
            })
        );
    }

    #[test]
    fn parses_render_defaults_to_the_whole_composition() {
        let command = parse(&args(&["render", "in.motion", "--out", "out.mp4"])).unwrap();
        assert_eq!(
            command,
            Command::Render(RenderArgs {
                input: PathBuf::from("in.motion"),
                output: PathBuf::from("out.mp4"),
                from: None,
                to: None,
                ffmpeg: None,
            })
        );
    }

    #[test]
    fn parses_still_and_info() {
        assert_eq!(
            parse(&args(&[
                "still",
                "in.motion",
                "--frame",
                "7",
                "--out",
                "f.png"
            ]))
            .unwrap(),
            Command::Still(StillArgs {
                input: PathBuf::from("in.motion"),
                frame: 7,
                output: PathBuf::from("f.png"),
            })
        );
        assert!(matches!(
            parse(&args(&["info", "in.motion"])).unwrap(),
            Command::Info(_)
        ));
    }

    #[test]
    fn parses_preview_with_all_options() {
        assert_eq!(
            parse(&args(&[
                "preview",
                "in.motion",
                "--frame",
                "42",
                "--paused",
                "--exit-after-frames",
                "600",
                "--window-size",
                "1280x720",
                "--ui-scale",
                "1.25",
                "--select",
                "3",
                "--screenshot",
                "shot.png",
                "--control-file",
                "ctl.txt",
            ]))
            .unwrap(),
            Command::Preview(PreviewArgs {
                input: PathBuf::from("in.motion"),
                frame: Some(42),
                paused: true,
                exit_after_frames: Some(600),
                window_size: Some((1280, 720)),
                ui_scale_milli: Some(1250),
                select: Some(3),
                screenshot: Some(PathBuf::from("shot.png")),
                control: Some(PathBuf::from("ctl.txt")),
                assets: false,
            })
        );
    }

    #[test]
    fn parses_preview_defaults() {
        assert_eq!(
            parse(&args(&["preview", "in.motion"])).unwrap(),
            Command::Preview(PreviewArgs {
                input: PathBuf::from("in.motion"),
                frame: None,
                paused: false,
                exit_after_frames: None,
                window_size: None,
                ui_scale_milli: None,
                select: None,
                screenshot: None,
                control: None,
                assets: false,
            })
        );
    }

    #[test]
    fn rejects_bad_preview_arguments() {
        assert!(parse(&args(&["preview"])).is_err());
        assert!(parse(&args(&["preview", "in.motion", "--frame"])).is_err());
        assert!(parse(&args(&[
            "preview",
            "in.motion",
            "--exit-after-frames",
            "-1"
        ]))
        .is_err());
        assert!(parse(&args(&["preview", "in.motion", "--bogus"])).is_err());
        assert!(parse(&args(&["preview", "a.motion", "b.motion"])).is_err());
        assert!(parse(&args(&["preview", "a.motion", "--window-size", "1280"])).is_err());
        assert!(parse(&args(&["preview", "a.motion", "--window-size", "10x10"])).is_err());
        assert!(parse(&args(&["preview", "a.motion", "--ui-scale", "9"])).is_err());
        assert!(parse(&args(&["preview", "a.motion", "--ui-scale", "big"])).is_err());
    }

    #[test]
    fn parses_bake_with_ids() {
        assert_eq!(
            parse(&args(&[
                "bake",
                "in.motion",
                "--id",
                "title",
                "--id",
                "card"
            ]))
            .unwrap(),
            Command::Bake(BakeArgs {
                input: PathBuf::from("in.motion"),
                ids: vec!["title".to_owned(), "card".to_owned()],
            })
        );
        assert_eq!(
            parse(&args(&["bake", "in.motion"])).unwrap(),
            Command::Bake(BakeArgs {
                input: PathBuf::from("in.motion"),
                ids: Vec::new(),
            })
        );
        assert!(parse(&args(&["bake"])).is_err());
        assert!(parse(&args(&["bake", "a.motion", "--id"])).is_err());
        assert!(parse(&args(&["bake", "a.motion", "b.motion"])).is_err());
        assert!(parse(&args(&["bake", "a.motion", "--bogus"])).is_err());
    }

    #[test]
    fn parses_the_agent_commands() {
        assert_eq!(
            parse(&args(&[
                "query", "a.motion", "--frame", "30", "--node", "title"
            ]))
            .unwrap(),
            Command::Query(agent::QueryArgs {
                input: PathBuf::from("a.motion"),
                frame: Some(30),
                node: Some("title".to_owned()),
            })
        );
        assert_eq!(
            parse(&args(&[
                "edit",
                "a.motion",
                "--dry-run",
                "seek(3)",
                "--frame",
                "9",
                "visible(\"x\", false)"
            ]))
            .unwrap(),
            Command::Edit(agent::EditArgs {
                input: PathBuf::from("a.motion"),
                frame: Some(9),
                dry_run: true,
                commands: vec!["seek(3)".to_owned(), "visible(\"x\", false)".to_owned()],
            })
        );
        assert_eq!(
            parse(&args(&["ctl", "--endpoint", "e.txt", "state()", "seek(1)"])).unwrap(),
            Command::Ctl(agent::CtlArgs {
                endpoint: Some(PathBuf::from("e.txt")),
                requests: vec!["state()".to_owned(), "seek(1)".to_owned()],
            })
        );
        assert!(parse(&args(&["edit", "a.motion"])).is_err());
        assert!(parse(&args(&["ctl"])).is_err());
        assert!(parse(&args(&["query"])).is_err());
        assert!(parse(&args(&["query", "a.motion", "--bogus"])).is_err());
        assert!(matches!(
            parse(&args(&["info", "a.motion", "--json"])).unwrap(),
            Command::Info(super::InfoArgs { json: true, .. })
        ));
    }

    #[test]
    fn the_help_lists_every_command() {
        let help = agent::command_help();
        for kind in motioner_ui::CommandKind::ALL {
            assert!(help.contains(kind.signature()), "{}", kind.name());
        }
    }

    #[test]
    fn no_arguments_opens_the_start_screen_and_help_is_explicit() {
        assert_eq!(
            parse(&args(&[])).unwrap(),
            Command::Desktop(super::DesktopArgs::default())
        );
        assert_eq!(parse(&args(&["--help"])).unwrap(), Command::Help);
    }

    #[test]
    fn a_path_opens_directly() {
        assert!(matches!(
            parse(&args(&["in.motion"])).unwrap(),
            Command::Preview(_)
        ));
        assert!(matches!(
            parse(&args(&["demo.motioner", "--paused"])).unwrap(),
            Command::Preview(_)
        ));
        assert!(parse(&args(&["nope"])).is_err());
    }

    #[test]
    fn rejects_missing_values() {
        assert!(parse(&args(&["render", "in.motion", "--out"])).is_err());
        assert!(parse(&args(&["render", "in.motion", "--out", "--from"])).is_err());
        assert!(parse(&args(&["still", "in.motion", "--out", "f.png"])).is_err());
        assert!(parse(&args(&["still", "in.motion", "--frame"])).is_err());
        assert!(parse(&args(&["render", "in.motion"])).is_err());
    }

    #[test]
    fn rejects_unknown_flags_and_commands() {
        assert!(parse(&args(&["render", "in.motion", "--out", "o.mp4", "--bogus"])).is_err());
        assert!(parse(&args(&[
            "still",
            "in.motion",
            "--frame",
            "0",
            "--out",
            "f.png",
            "-x"
        ]))
        .is_err());
        assert!(parse(&args(&["nope"])).is_err());
        assert!(parse(&args(&["info"])).is_err());
        assert!(parse(&args(&["info", "a.motion", "b.motion"])).is_err());
    }

    #[test]
    fn rejects_bad_frame_numbers() {
        assert!(parse(&args(&[
            "still",
            "in.motion",
            "--frame",
            "banana",
            "--out",
            "f.png"
        ]))
        .is_err());
        assert!(parse(&args(&[
            "render",
            "in.motion",
            "--out",
            "o.mp4",
            "--from",
            "x"
        ]))
        .is_err());
    }
}

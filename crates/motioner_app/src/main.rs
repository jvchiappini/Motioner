//! Motioner application entry point.
//!
//! Wires the document, evaluator, renderer and export crates into a small
//! command-line front end:
//!
//! ```text
//! motioner render <file.motion> --out <file.mp4> [--from N] [--to N] [--ffmpeg PATH]
//! motioner still  <file.motion> --frame N --out <file.png>
//! motioner info   <file.motion>
//! ```
//!
//! MDF diagnostics are printed with their source line and exit with code 2.
//! Every other failure (rendering, encoding, I/O, bad arguments) exits with
//! code 1. `anyhow` is allowed here because this is the application edge.

use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use motioner_core::Document;
use motioner_dsl::Limits;
use motioner_eval::{Evaluator, RenderPlan};
use motioner_io::{write_png, ExportError, FfmpegLocation, Mp4Encoder, VideoSettings};
use motioner_render::{FrameRenderer, RenderError};

/// Printed by `--help` and on argument errors.
const HELP: &str = "\
Motioner - AI-first motion graphics

Usage:
  motioner render  <file.motion> --out <file.mp4> [--from N] [--to N] [--ffmpeg PATH]
  motioner still   <file.motion> --frame N --out <file.png>
  motioner info    <file.motion>
  motioner bake    <file.motion> [--id ID]...
  motioner preview <file.motion> [--frame N] [--paused] [--exit-after-frames N]
                   [--window-size WxH] [--ui-scale F] [--select ROW]
  motioner --help

Frames are integer indices. `render` draws the half-open range [from, to) and
defaults to the whole composition. `still` draws one frame. `preview` opens the
live editor window; `--exit-after-frames` closes it after N presented frames
(for smoke runs and CI), `--window-size` sets the window in physical pixels,
`--ui-scale` overrides the chrome scale (to check other DPI settings) and
`--select` selects an outline row at startup.

`bake` folds <name>.overrides.motion into <name>.motion for nodes bound to a
top-level variable, appends one generated block and rewrites both files
atomically. Nodes created inside loops or functions cannot be baked and are
reported; their overrides stay in the overrides file.";

/// Parsed command line.
#[derive(Debug, PartialEq, Eq)]
enum Command {
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
    /// Print usage.
    Help,
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
        Ok(()) => ExitCode::SUCCESS,
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

/// Parses the arguments and dispatches the command.
fn run(args: &[OsString]) -> Result<(), Failure> {
    match parse(args)? {
        Command::Help => {
            println!("{HELP}");
            Ok(())
        }
        Command::Info(args) => run_info(&args),
        Command::Bake(args) => run_bake(&args),
        Command::Still(args) => run_still(&args),
        Command::Render(args) => run_render(&args),
        Command::Preview(args) => run_preview(&args),
    }
}

/// Parses the command line into a [`Command`].
fn parse(args: &[OsString]) -> Result<Command> {
    let Some(first) = args.first().and_then(|arg| arg.to_str()) else {
        return Ok(Command::Help);
    };
    if first == "--help" || first == "-h" {
        return Ok(Command::Help);
    }
    let rest = &args[1..];
    match first {
        "render" => parse_render(rest),
        "still" => parse_still(rest),
        "info" => parse_info(rest),
        "bake" => parse_bake(rest),
        "preview" => parse_preview(rest),
        other => bail!("unknown command '{other}' (try --help)"),
    }
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

/// Parses `info <file>`.
fn parse_info(args: &[OsString]) -> Result<Command> {
    let Some(input) = args.first() else {
        bail!("info needs an input .motion file");
    };
    if args.len() > 1 {
        bail!("info takes exactly one input file");
    }
    Ok(Command::Info(InfoArgs {
        input: PathBuf::from(input),
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

/// Opens the live preview window.
fn run_preview(args: &PreviewArgs) -> std::result::Result<(), Failure> {
    let options = motioner_ui::PreviewOptions {
        frame: args.frame,
        paused: args.paused,
        exit_after_frames: args.exit_after_frames,
        window_size: args.window_size,
        #[allow(clippy::cast_precision_loss)] // at most 4000
        ui_scale: args.ui_scale_milli.map(|milli| milli as f32 / 1000.0),
        select: args.select,
    };
    motioner_ui::run_preview(&args.input, options)?;
    Ok(())
}

/// Loads and executes an MDF program.
///
/// Returns the source text and the document. An MDF diagnostic becomes
/// [`Failure::Diagnostic`] so `main` can print it and exit with code 2.
fn load_document(path: &Path) -> std::result::Result<(String, Document), Failure> {
    let source = fs::read_to_string(path)
        .with_context(|| format!("cannot read {}", path.display()))
        .map_err(Failure::Error)?;
    let name = path.display().to_string();
    match motioner_dsl::run(&source, &Limits::default()) {
        Ok(document) => Ok((source, document)),
        Err(diagnostic) => Err(Failure::Diagnostic(diagnostic.render(&source, &name))),
    }
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

/// The overrides file that belongs to `program`.
///
/// `<dir>/<stem>.overrides.motion`, next to the program (D-0022).
fn overrides_path(program: &Path) -> PathBuf {
    let stem = program
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("program");
    let name = format!("{stem}.overrides.motion");
    program.with_file_name(name)
}

/// Writes `text` to `path` through a temporary file in the same directory and
/// a rename, so a crash never leaves a half-written program or overrides file.
fn write_atomic(path: &Path, text: &str) -> Result<()> {
    ensure_parent(path)?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file");
    let temp = path.with_file_name(format!(".{file_name}.tmp"));
    fs::write(&temp, text).with_context(|| format!("cannot write {}", temp.display()))?;
    fs::rename(&temp, path).with_context(|| {
        let _ = fs::remove_file(&temp);
        format!("cannot replace {}", path.display())
    })?;
    Ok(())
}

/// Folds the overrides file into the program (`motioner bake`).
fn run_bake(args: &BakeArgs) -> std::result::Result<(), Failure> {
    let source = fs::read_to_string(&args.input)
        .with_context(|| format!("cannot read {}", args.input.display()))
        .map_err(Failure::Error)?;
    let overrides_file = overrides_path(&args.input);
    let overrides_source = match fs::read_to_string(&overrides_file) {
        Ok(text) => Some(text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(Failure::Error(anyhow!(
                "cannot read {}: {error}",
                overrides_file.display()
            )))
        }
    };
    let name = args.input.display().to_string();
    let compiled =
        motioner_dsl::run_with_overrides(&source, overrides_source.as_deref(), &Limits::default())
            .map_err(|diagnostic| Failure::Diagnostic(diagnostic.render(&source, &name)))?;
    for warning in &compiled.warnings {
        println!("warning: {}", warning.render(&source, &name));
    }

    let outcome = motioner_dsl::bake(
        &source,
        &compiled.document,
        &compiled.bindings,
        &compiled.overrides,
        &args.ids,
    );
    if outcome.program != source {
        write_atomic(&args.input, &outcome.program)?;
    }
    let rate = compiled.document.composition().frame_rate();
    let remaining = outcome.overrides.to_mdf(rate);
    write_atomic(&overrides_file, &remaining)?;

    for id in &outcome.baked {
        println!("baked {id}");
    }
    for refusal in &outcome.refused {
        println!(
            "refused {} '{}': {}",
            refusal.id, refusal.property, refusal.reason
        );
    }
    if outcome.baked.is_empty() && outcome.refused.is_empty() {
        println!("Nothing to bake.");
    }
    println!(
        "Wrote {} and {} ({} override(s) left)",
        args.input.display(),
        overrides_file.display(),
        outcome.overrides.len()
    );
    Ok(())
}

/// Prints composition facts for the `info` command.
fn run_info(args: &InfoArgs) -> std::result::Result<(), Failure> {
    let (_, document) = load_document(&args.input)?;
    let composition = document.composition();
    let rate = composition.frame_rate();
    let frames = composition.duration();
    #[allow(clippy::cast_precision_loss)] // display only
    let seconds = frames as f64 * f64::from(rate.denominator()) / f64::from(rate.numerator());
    println!("File: {}", args.input.display());
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
    let (_, document) = load_document(&args.input)?;
    let composition = document.composition();
    if args.frame < 0 || args.frame >= composition.duration() {
        return Err(Failure::Error(anyhow!(
            "frame {} is outside the composition range 0..{}",
            args.frame,
            composition.duration()
        )));
    }
    ensure_parent(&args.output)?;

    let mut renderer = FrameRenderer::new(composition.width(), composition.height())?;
    let mut evaluator = Evaluator::new();
    let mut plan = RenderPlan::new();
    let mut rgba = Vec::new();
    evaluator.evaluate(&document, args.frame, &mut plan);
    renderer.render(&document, &plan, &mut rgba)?;
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
    let (_, document) = load_document(&args.input)?;
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

    let mut renderer = FrameRenderer::new(settings.width, settings.height)?;
    let mut encoder = Mp4Encoder::start(&args.output, &settings, &location)?;
    let mut evaluator = Evaluator::new();
    let mut plan = RenderPlan::new();
    let mut rgba = Vec::new();

    let total = u64::try_from(to - from).unwrap_or(0);
    let started = Instant::now();
    let mut evaluate_time = Duration::ZERO;
    let mut render_time = Duration::ZERO;
    let mut encode_time = Duration::ZERO;
    let mut missing_glyphs = 0_u64;
    let mut completed = 0_u64;

    for frame in from..to {
        let evaluate_start = Instant::now();
        evaluator.evaluate(&document, frame, &mut plan);
        evaluate_time += evaluate_start.elapsed();

        let render_start = Instant::now();
        let stats = renderer.render(&document, &plan, &mut rgba)?;
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
    print_average("evaluate", evaluate_time, completed);
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
    use super::{overrides_path, parse, BakeArgs, Command, PreviewArgs, RenderArgs, StillArgs};
    use std::ffi::OsString;
    use std::path::PathBuf;

    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
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
    fn the_overrides_file_sits_next_to_the_program() {
        assert_eq!(
            overrides_path(std::path::Path::new("out/edit/hello.motion")),
            PathBuf::from("out/edit/hello.overrides.motion")
        );
        assert_eq!(
            overrides_path(std::path::Path::new("hello.motion")),
            PathBuf::from("hello.overrides.motion")
        );
    }

    #[test]
    fn help_needs_no_arguments() {
        assert_eq!(parse(&args(&[])).unwrap(), Command::Help);
        assert_eq!(parse(&args(&["--help"])).unwrap(), Command::Help);
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

//! `motioner corpus check`: validates a directory of generated MDF programs.
//!
//! The check is the filter between a frontier model's output and the training
//! corpus (D-0036): parse, run without warnings, format, lint, render a few
//! stills headlessly, reject blank or near-blank frames and prove the render is
//! deterministic. It writes `manifest.json` next to the programs.
//!
//! No model is called here: the tool never needs a key or the network.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Instant;

use motioner_dsl::ast::{Expr, ExprKind, Program, Stmt, StmtKind};
use motioner_dsl::Limits;
use motioner_render::{FontSet, FrameRenderer, ImageSet};

/// Arguments of `corpus check`.
#[derive(Debug, PartialEq, Eq)]
pub struct CheckArgs {
    /// Directory of `.motion` programs.
    pub input: PathBuf,
    /// How many evenly spaced stills to render (at least 2 for determinism).
    pub frames: usize,
}

/// What the corpus check found in one program.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileReport {
    /// File name.
    pub file: String,
    /// `ok` or the first rejection reason.
    pub status: String,
    /// Whether the program runs, formats and renders deterministically.
    pub accepted: bool,
    /// Time spent on this file in milliseconds.
    pub duration_ms: u64,
    /// Layer count.
    pub layers: usize,
    /// Composition length in frames.
    pub duration: i64,
    /// Language features the program uses.
    pub features: Vec<String>,
    /// Render time per still in milliseconds.
    pub render_ms: Vec<u64>,
    /// Why the file was rejected, when it was.
    pub reasons: Vec<String>,
}

/// Whether `rgba` (a rendered frame) is blank or near-blank.
///
/// A frame is blank when fewer than 0.5 % of its pixels differ from the first
/// pixel: a solid background, transparent or not.
#[must_use]
pub fn near_blank(rgba: &[u8]) -> bool {
    if rgba.len() < 4 {
        return true;
    }
    let first = &rgba[..4];
    let mut different = 0_usize;
    let total = rgba.len() / 4;
    for pixel in rgba.chunks_exact(4) {
        if pixel != first {
            different += 1;
        }
    }
    different * 200 < total
}

/// The language features a program uses (AST-based, not a text search).
#[must_use]
pub fn features(program: &Program) -> Vec<String> {
    let mut found = BTreeSet::new();
    for statement in &program.body {
        visit_statement(statement, &mut found);
    }
    found.into_iter().collect()
}

fn visit_statement(statement: &Stmt, found: &mut BTreeSet<String>) {
    match &statement.kind {
        StmtKind::For { iterable, body, .. } => {
            found.insert("loops".to_owned());
            visit_expr(iterable, found);
            for statement in body {
                visit_statement(statement, found);
            }
        }
        StmtKind::If {
            branches,
            otherwise,
        } => {
            found.insert("conditionals".to_owned());
            for (condition, body) in branches {
                visit_expr(condition, found);
                for statement in body {
                    visit_statement(statement, found);
                }
            }
            for statement in otherwise {
                visit_statement(statement, found);
            }
        }
        StmtKind::Def(def) => {
            found.insert("functions".to_owned());
            for statement in &def.body {
                visit_statement(statement, found);
            }
        }
        StmtKind::Assign { value, .. } => {
            visit_expr(value, found);
        }
        StmtKind::Return(Some(expr)) | StmtKind::Expr(expr) => visit_expr(expr, found),
        StmtKind::Return(None) | StmtKind::Break | StmtKind::Continue | StmtKind::Pass => {}
    }
}

fn visit_expr(expr: &Expr, found: &mut BTreeSet<String>) {
    match &expr.kind {
        ExprKind::Call { callee, args } => {
            if let ExprKind::Name(name) = &callee.kind {
                match name.as_ref() {
                    "text" => {
                        found.insert("text".to_owned());
                    }
                    "image" => {
                        found.insert("images".to_owned());
                    }
                    "video" => {
                        found.insert("video".to_owned());
                    }
                    "audio" => {
                        found.insert("audio".to_owned());
                    }
                    "path" => {
                        found.insert("paths".to_owned());
                    }
                    "gradient" => {
                        found.insert("gradients".to_owned());
                    }
                    "rect" | "circle" => {
                        found.insert("shapes".to_owned());
                    }
                    _ => {}
                }
            }
            for argument in args {
                if matches!(argument.name.as_deref(), Some("ease")) {
                    found.insert("easing".to_owned());
                }
                if matches!(argument.name.as_deref(), Some("motion_blur")) {
                    found.insert("motion_blur".to_owned());
                }
                visit_expr(&argument.value, found);
            }
            visit_expr(callee, found);
        }
        ExprKind::Attribute { object, name } => {
            match name.as_ref() {
                "key" => {
                    found.insert("keys".to_owned());
                }
                "effect" => {
                    found.insert("effects".to_owned());
                }
                "mask" | "masked_by" => {
                    found.insert("masks".to_owned());
                }
                "blend" => {
                    found.insert("blends".to_owned());
                }
                "animate_chars" => {
                    found.insert("animators".to_owned());
                }
                _ => {}
            }
            visit_expr(object, found);
        }
        ExprKind::List(items) | ExprKind::Tuple(items) => {
            for item in items {
                visit_expr(item, found);
            }
        }
        ExprKind::Unary { operand, .. } => visit_expr(operand, found),
        ExprKind::Binary { left, right, .. }
        | ExprKind::Compare { left, right, .. }
        | ExprKind::Logical { left, right, .. } => {
            visit_expr(left, found);
            visit_expr(right, found);
        }
        ExprKind::Not(inner) => visit_expr(inner, found),
        ExprKind::Index { object, index } => {
            visit_expr(object, found);
            visit_expr(index, found);
        }
        ExprKind::Number(_)
        | ExprKind::Str(_)
        | ExprKind::Bool(_)
        | ExprKind::None
        | ExprKind::Name(_) => {}
    }
}

/// Checks one program.
#[allow(clippy::too_many_lines)] // one file: parse, run, format, lint, render, determinism
fn check_file(path: &Path, frames: usize) -> FileReport {
    let start = Instant::now();
    let mut report = FileReport {
        file: path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("program.motion")
            .to_owned(),
        status: "ok".to_owned(),
        ..FileReport::default()
    };
    let source = match std::fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) => {
            "unreadable".clone_into(&mut report.status);
            report.reasons.push(error.to_string());
            report.duration_ms = millis(start);
            return report;
        }
    };
    let program = match motioner_dsl::parse(&source) {
        Ok(program) => program,
        Err(diagnostic) => {
            "parse".clone_into(&mut report.status);
            report.reasons.push(format!(
                "{}:{}: {}",
                diagnostic.span.line, diagnostic.span.column, diagnostic.message
            ));
            report.duration_ms = millis(start);
            return report;
        }
    };
    report.features = features(&program);
    let compiled = match motioner_dsl::run_with_overrides(&source, None, &Limits::default()) {
        Ok(compiled) => compiled,
        Err(diagnostic) => {
            "run".clone_into(&mut report.status);
            report.reasons.push(format!(
                "{}:{}: {}",
                diagnostic.span.line, diagnostic.span.column, diagnostic.message
            ));
            report.duration_ms = millis(start);
            return report;
        }
    };
    if !compiled.warnings.is_empty() {
        "warnings".clone_into(&mut report.status);
        report.reasons.extend(
            compiled
                .warnings
                .iter()
                .map(|warning| warning.message.clone()),
        );
    }
    report.layers = compiled.document.len();
    report.duration = compiled.document.composition().duration();
    if motioner_dsl::printer::format_check(&source).is_err() {
        report
            .reasons
            .push("not formatted (`motioner fmt`)".to_owned());
    }
    if let Ok(findings) = motioner_dsl::lint::lint(&source) {
        if !findings.is_empty() {
            report
                .reasons
                .push(format!("{} lint finding(s)", findings.len()));
        }
    }
    if !report.reasons.is_empty() {
        "rejected".clone_into(&mut report.status);
        report.duration_ms = millis(start);
        return report;
    }
    // Render evenly spaced stills, including the first and the last frame.
    let composition = compiled.document.composition();
    let count = frames.max(2);
    let last = (composition.duration() - 1).max(0);
    let mut renderer = match FrameRenderer::with_fonts(
        composition.width(),
        composition.height(),
        FontSet::bundled(),
    ) {
        Ok(renderer) => renderer,
        Err(error) => {
            "renderer".clone_into(&mut report.status);
            report.reasons.push(error.to_string());
            report.duration_ms = millis(start);
            return report;
        }
    };
    renderer.set_images(ImageSet::new());
    let mut first_frame: Option<Vec<u8>> = None;
    let mut any_drawn = false;
    for index in 0..count {
        let frame = if count <= 1 {
            0
        } else {
            last * i64::try_from(index).unwrap_or(0) / i64::try_from(count - 1).unwrap_or(1)
        };
        let began = Instant::now();
        let mut rgba = Vec::new();
        match renderer.render_frame(&compiled.document, frame, &mut rgba) {
            Ok(_) => {}
            Err(error) => {
                "render".clone_into(&mut report.status);
                report.reasons.push(error.to_string());
                report.duration_ms = millis(start);
                return report;
            }
        }
        report.render_ms.push(millis(began));
        if !near_blank(&rgba) {
            any_drawn = true;
        }
        if index == 0 {
            first_frame = Some(rgba);
        }
    }
    if !any_drawn {
        "blank".clone_into(&mut report.status);
        report
            .reasons
            .push("every rendered still is blank".to_owned());
        report.duration_ms = millis(start);
        return report;
    }
    // Determinism: the first frame rendered again must be byte-identical.
    if let Some(first) = first_frame {
        let mut again = Vec::new();
        if renderer
            .render_frame(&compiled.document, 0, &mut again)
            .is_ok()
            && again != first
        {
            "nondeterministic".clone_into(&mut report.status);
            report
                .reasons
                .push("two renders of frame 0 differ".to_owned());
        }
    }
    report.accepted = report.reasons.is_empty();
    if !report.accepted {
        "rejected".clone_into(&mut report.status);
    }
    report.duration_ms = millis(start);
    report
}

fn millis(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// Runs the check over `input` and writes `manifest.json`.
///
/// Returns `Err` only for a tool failure (the directory cannot be read or the
/// manifest cannot be written); a rejected program is a normal result.
pub fn run(args: &CheckArgs) -> Result<(), String> {
    if !args.input.is_dir() {
        return Err(format!("{} is not a directory", args.input.display()));
    }
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&args.input)
        .map_err(|error| error.to_string())?
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "motion")
        })
        .collect();
    paths.sort();
    if paths.is_empty() {
        return Err(format!("{} has no .motion programs", args.input.display()));
    }
    let reports: Vec<FileReport> = paths
        .iter()
        .map(|path| check_file(path, args.frames))
        .collect();
    let accepted = reports.iter().filter(|report| report.accepted).count();
    let mut manifest = String::from("{\n  \"files\": [\n");
    for (index, report) in reports.iter().enumerate() {
        let features: Vec<String> = report
            .features
            .iter()
            .map(|feature| format!("\"{feature}\""))
            .collect();
        let reasons: Vec<String> = report
            .reasons
            .iter()
            .map(|reason| format!("\"{}\"", escape(reason)))
            .collect();
        let renders: Vec<String> = report.render_ms.iter().map(u64::to_string).collect();
        let _ = writeln!(
            manifest,
            "    {{\"file\": \"{}\", \"status\": \"{}\", \"accepted\": {}, \
             \"duration_ms\": {}, \"layers\": {}, \"duration\": {}, \
             \"features\": [{}], \"render_ms\": [{}], \"reasons\": [{}]}}{}",
            escape(&report.file),
            escape(&report.status),
            report.accepted,
            report.duration_ms,
            report.layers,
            report.duration,
            features.join(", "),
            renders.join(", "),
            reasons.join(", "),
            if index + 1 == reports.len() { "" } else { "," }
        );
    }
    let _ = writeln!(
        manifest,
        "  ],\n  \"accepted\": {accepted},\n  \"rejected\": {}\n}}",
        reports.len() - accepted
    );
    let path = args.input.join("manifest.json");
    std::fs::write(&path, &manifest).map_err(|error| error.to_string())?;
    for report in &reports {
        println!(
            "{}: {} ({}, {} layers, {} ms)",
            report.file, report.status, report.duration_ms, report.layers, report.duration_ms
        );
        for reason in &report.reasons {
            println!("  - {reason}");
        }
    }
    println!(
        "{accepted} accepted, {} rejected; manifest at {}",
        reports.len() - accepted,
        path.display()
    );
    Ok(())
}

fn escape(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::{features, near_blank};
    use motioner_dsl::parse;

    #[test]
    fn a_blank_frame_is_rejected_and_a_drawn_one_is_not() {
        assert!(near_blank(&[0, 0, 0, 255].repeat(400)));
        let mut frame = vec![0_u8; 400 * 4];
        frame[0..4].copy_from_slice(&[255, 0, 0, 255]);
        assert!(!near_blank(&frame));
    }

    #[test]
    fn the_feature_detector_reads_the_ast() {
        let source = "comp(name=\"P\", duration=1.0)\nfor i in range(2):\n    c = circle(10)\n    c.key(\"opacity\", i, 1, ease=\"quad_out\")\nt = text(\"Hi\", motion_blur=True)\n";
        let found = features(&parse(source).unwrap());
        for feature in ["loops", "keys", "easing", "motion_blur", "text", "shapes"] {
            assert!(found.contains(&feature.to_owned()), "{feature}: {found:?}");
        }
    }
}

//! The MDF (Motioner Document Format) language.
//!
//! MDF is program-first (D-0011): the program is the canonical source of a
//! composition and executing it builds a [`motioner_core::Document`]. This
//! crate implements the frozen v0 subset of `spec/mdf-v0.md`:
//!
//! - a hand-written lexer with Python-style indentation ([`tokenize`]);
//! - a recursive-descent parser producing a spanned AST ([`parse`]);
//! - a sandboxed, deterministic tree-walking interpreter with resource
//!   [`Limits`] and the 2D scene API ([`execute`], [`run`]).
//!
//! Every failure is a [`Diagnostic`] with a category, message, source span
//! and call stack. The crate has no GPU, UI, file system or clock access.
//!
//! ```
//! let source = "comp(name=\"Demo\", duration=1.0)\nbox = rect(100, 50)\nbox.pos = (960, 540)\n";
//! let document = motioner_dsl::run(source, &motioner_dsl::Limits::default()).unwrap();
//! assert_eq!(document.len(), 1);
//! ```

pub mod ast;
mod bake;
mod builtins;
mod diagnostic;
mod host;
mod interpreter;
mod lexer;
mod limits;
mod naming;
mod origins;
mod overrides_file;
mod parser;
mod scene;
mod token;
mod value;

pub use bake::{bake, BakeOutcome};
pub use diagnostic::{Diagnostic, ErrorCategory, Severity, Span, StackFrame};
pub use interpreter::{
    execute, execute_detailed, execute_detailed_with_metrics, execute_full,
    execute_full_with_metrics, execute_with_metrics, Execution,
};
pub use lexer::tokenize;
pub use limits::Limits;
pub use naming::{assign_default_names, default_name, NameNote};
pub use origins::{
    BindingOrigin, BlockOrigin, CallOrigin, EaseOrigin, ExpressionOrigin, IterationValue, Literal,
    LoopOrigin, NameId, NodeOrigin, OriginKind, OriginSite, ParameterOrigin, ValueOrigin,
    ValueOrigins, ValueSource,
};
pub use parser::{parse, MAX_EXPR_DEPTH};
pub use token::{Keyword, Op, Token, TokenKind};

use motioner_core::{Document, NodeId, OverrideSet};

/// The result of compiling a program together with its overrides file.
#[derive(Debug, Clone)]
pub struct Compiled {
    /// The document with every applicable override already applied.
    pub document: Document,
    /// The overrides that were read (and applied where possible).
    pub overrides: OverrideSet,
    /// Orphaned or inapplicable overrides, reported as warnings.
    pub warnings: Vec<Diagnostic>,
    /// Top-level variable bindings that point at nodes, sorted by name; bake
    /// uses them to name nodes in generated statements.
    pub bindings: Vec<(String, NodeId)>,
    /// Where each node property value came from; the hybrid bake edits a
    /// private literal in place when that is exact.
    pub origins: ValueOrigins,
}

/// Stack size of the interpreter thread. Deep MDF call chains (up to
/// [`Limits::max_call_depth`]) recurse through the Rust stack, which is
/// only 1 MiB on the Windows main thread.
const INTERPRETER_STACK_BYTES: usize = 64 * 1024 * 1024;

/// Parses and executes MDF source on a dedicated large-stack thread.
///
/// # Errors
///
/// Returns the first lexical, parse or runtime [`Diagnostic`].
///
/// # Panics
///
/// Re-raises a panic from the interpreter thread, which would indicate an
/// interpreter bug rather than a program error.
pub fn run(source: &str, limits: &Limits) -> Result<Document, Diagnostic> {
    run_with_metrics(source, limits, &motioner_core::NoTextMetrics)
}

/// [`run`] with text metrics installed (D-0034).
///
/// # Errors
///
/// Same as [`run`].
///
/// # Panics
///
/// Re-raises a panic from the interpreter thread.
pub fn run_with_metrics(
    source: &str,
    limits: &Limits,
    metrics: &dyn motioner_core::TextMetrics,
) -> Result<Document, Diagnostic> {
    let source = source.to_owned();
    let limits = limits.clone();
    std::thread::scope(|scope| {
        let worker = std::thread::Builder::new()
            .name("mdf-interpreter".to_owned())
            .stack_size(INTERPRETER_STACK_BYTES)
            .spawn_scoped(scope, move || {
                interpreter::execute_with_metrics(&parse(&source)?, &limits, metrics)
            });
        let handle = match worker {
            Ok(handle) => handle,
            Err(error) => {
                return Err(Diagnostic::new(
                    ErrorCategory::Limit,
                    Span::default(),
                    format!("could not start the interpreter thread: {error}"),
                ))
            }
        };
        handle
            .join()
            .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
    })
}

/// Parses and executes MDF source plus an optional overrides file.
///
/// The program runs first; its composition rate converts the override times,
/// then every override that matches a node and its kind is applied to the
/// document. Orphans and kind mismatches become [`Severity::Warning`]
/// diagnostics pointing at their statement and are also kept in
/// [`Compiled::overrides`], so a later save never drops them.
///
/// # Errors
///
/// Returns the first lexical, parse or runtime diagnostic of the program, or
/// the first malformed statement of the overrides file (prefixed
/// `overrides:`).
///
/// # Panics
///
/// Re-raises a panic from the interpreter thread, which would indicate an
/// interpreter bug rather than a program error.
pub fn run_with_overrides(
    source: &str,
    overrides_source: Option<&str>,
    limits: &Limits,
) -> Result<Compiled, Diagnostic> {
    run_with_overrides_and_metrics(
        source,
        overrides_source,
        limits,
        &motioner_core::NoTextMetrics,
    )
}

/// [`run_with_overrides`] with text metrics installed (D-0034).
///
/// # Errors
///
/// Same as [`run_with_overrides`].
///
/// # Panics
///
/// Re-raises a panic from the interpreter thread.
pub fn run_with_overrides_and_metrics(
    source: &str,
    overrides_source: Option<&str>,
    limits: &Limits,
    metrics: &dyn motioner_core::TextMetrics,
) -> Result<Compiled, Diagnostic> {
    let source = source.to_owned();
    let overrides_source = overrides_source.map(str::to_owned);
    let limits = limits.clone();
    std::thread::scope(|scope| {
        let worker = std::thread::Builder::new()
            .name("mdf-interpreter".to_owned())
            .stack_size(INTERPRETER_STACK_BYTES)
            .spawn_scoped(scope, move || {
                compile(&source, overrides_source.as_deref(), &limits, metrics)
            });
        let handle = match worker {
            Ok(handle) => handle,
            Err(error) => {
                return Err(Diagnostic::new(
                    ErrorCategory::Limit,
                    Span::default(),
                    format!("could not start the interpreter thread: {error}"),
                ))
            }
        };
        handle
            .join()
            .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
    })
}

fn compile(
    source: &str,
    overrides_source: Option<&str>,
    limits: &Limits,
    metrics: &dyn motioner_core::TextMetrics,
) -> Result<Compiled, Diagnostic> {
    let program = parse(source)?;
    let execution = interpreter::execute_detailed_with_metrics(&program, limits, metrics)?;
    let mut document = execution.document;
    let bindings = execution.bindings;
    let origins = execution.origins;
    let Some(overrides_source) = overrides_source else {
        return Ok(Compiled {
            document,
            overrides: OverrideSet::new(),
            warnings: Vec::new(),
            bindings,
            origins,
        });
    };
    let rate = document.composition().frame_rate();
    let parsed = overrides_file::parse_overrides(overrides_source, rate)?;
    let mut warnings = Vec::new();
    for index in 0..parsed.set.len() {
        if let Some(issue) = parsed.set.apply_entry(&mut document, index) {
            let span = parsed.spans.get(index).copied().unwrap_or_default();
            warnings.push(Diagnostic::warning(
                ErrorCategory::Value,
                span,
                format!(
                    "overrides: override for {}: '{}': {}",
                    issue.id, issue.property, issue.message
                ),
            ));
        }
    }
    Ok(Compiled {
        document,
        overrides: parsed.set,
        warnings,
        bindings,
        origins,
    })
}

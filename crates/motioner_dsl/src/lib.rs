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

pub mod assets;
pub mod ast;
mod bake;
mod builtins;
mod call_site;
mod code_edit;
mod diagnostic;
mod host;
mod interpreter;
mod lexer;
mod limits;
mod loop_guard;
mod naming;
mod origins;
mod overrides_file;
mod parser;
mod plan;
pub mod prelude;
mod scene;
pub mod theme;
mod token;
mod value;
mod write_text;

// Prompt 12 phase 5-7 modules. They are declared here once so the phase
// subagents never edit this file concurrently.
pub mod lint;
pub mod printer;
pub mod structural;

pub use assets::{AssetResolver, LoadedAsset, NoAssets};
pub use ast::Program;
pub use bake::{bake, BakeOutcome};
pub use code_edit::{CodeEdit, CodeEditError, Replacement, Strategy};
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
pub use plan::{
    plan, plan_with_scope, plan_with_scope_and_metrics, verify, verify_with_metrics, Plan, Scope,
};
pub use token::{Keyword, Op, Token, TokenKind};

/// The MDF standard-library version shipped with this build (D-0039).
#[must_use]
pub const fn std_version() -> u32 {
    prelude::VERSION
}

/// Rewrites a diagnostic raised inside the prelude so it reports the library
/// source rather than the user's file.
fn normalize_diagnostic(mut diagnostic: Diagnostic) -> Diagnostic {
    if prelude::is_library_span(diagnostic.span) {
        diagnostic.span = prelude::normalize_span(diagnostic.span);
        if !diagnostic.message.starts_with("std: ") {
            diagnostic.message = format!("std: {}", diagnostic.message);
        }
    }
    diagnostic
}

/// A user program with the standard-library prelude prepended (D-0039).
///
/// Tools that execute a program themselves (benchmarks, the agent runtime)
/// use this instead of [`parse`] so the prelude, its library spans and the
/// shadowing rules are identical to [`run`]. The leading [`Self::library_len`]
/// statements are library definitions; [`execute_parsed_with_metrics`] binds
/// them once instead of walking them on every run.
#[derive(Debug, Clone)]
pub struct ParsedProgram {
    program: crate::ast::Program,
    library_len: usize,
}

impl ParsedProgram {
    /// The merged statement list (prelude first, then user code).
    #[must_use]
    pub fn program(&self) -> &crate::ast::Program {
        &self.program
    }

    /// How many leading statements come from the standard library.
    #[must_use]
    pub fn library_len(&self) -> usize {
        self.library_len
    }

    /// The user's statements only (the prelude stripped).
    #[must_use]
    pub fn user_body(&self) -> &[crate::ast::Stmt] {
        &self.program.body[self.library_len..]
    }
}

/// Parses a user program with the standard-library prelude prepended.
///
/// The prelude is parsed once per process and shared, so repeated calls are
/// cheap.
///
/// # Errors
///
/// Returns the first lexical or parse diagnostic of either source.
pub fn parse_with_prelude(source: &str) -> Result<ParsedProgram, Diagnostic> {
    let (program, library_len) = with_prelude(source)?;
    Ok(ParsedProgram {
        program,
        library_len,
    })
}

/// Executes a [`ParsedProgram`] with text metrics installed, binding the
/// library definitions directly.
///
/// Unlike [`run_with_metrics`] this runs on the caller's thread and assumes
/// the caller has enough stack for the program's call depth; benchmarks and
/// tests that already control their stack use it. The editor and CLI keep
/// using [`run_with_metrics`], which owns the large interpreter stack.
///
/// # Errors
///
/// Same as [`run_with_metrics`].
pub fn execute_parsed_with_metrics(
    parsed: &ParsedProgram,
    limits: &Limits,
    metrics: &dyn motioner_core::TextMetrics,
) -> Result<interpreter::Execution, Diagnostic> {
    interpreter::execute_library_detailed_with_metrics(
        &parsed.program,
        parsed.library_len,
        limits,
        metrics,
    )
    .map_err(normalize_diagnostic)
}

/// Parses a user program and prepends the standard-library prelude.
///
/// Reports how many leading statements come from the library so execution can
/// bind their definitions once instead of walking them on every run.
fn with_prelude(source: &str) -> Result<(crate::ast::Program, usize), Diagnostic> {
    let user = parse(source)?;
    let library = prelude::parse_library()?;
    Ok(prelude::with_user(library, user))
}

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
    run_with_assets(source, limits, metrics, &NoAssets)
}

/// [`run`] with an asset resolver for `use()` (D-0051).
///
/// The resolver is the host's seam into the file system; the DSL itself stays
/// file-system free. Without a resolver, `use()` of a `.motion` module or an
/// SVG fails with a clear message while `use()` of a raster or media file
/// still works (their readers resolve the path later).
///
/// # Errors
///
/// Same as [`run`].
///
/// # Panics
///
/// Re-raises a panic from the interpreter thread.
pub fn run_with_assets(
    source: &str,
    limits: &Limits,
    metrics: &dyn motioner_core::TextMetrics,
    assets: &dyn AssetResolver,
) -> Result<Document, Diagnostic> {
    let source = source.to_owned();
    let limits = limits.clone();
    std::thread::scope(|scope| {
        let worker = std::thread::Builder::new()
            .name("mdf-interpreter".to_owned())
            .stack_size(INTERPRETER_STACK_BYTES)
            .spawn_scoped(scope, move || {
                let (program, library_len) = with_prelude(&source)?;
                interpreter::execute_library_detailed_with_assets(
                    &program,
                    library_len,
                    &limits,
                    metrics,
                    assets,
                )
                .map(|execution| execution.document)
                .map_err(normalize_diagnostic)
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
    let (program, library_len) = with_prelude(source)?;
    let execution =
        interpreter::execute_library_detailed_with_metrics(&program, library_len, limits, metrics)
            .map_err(normalize_diagnostic)?;
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

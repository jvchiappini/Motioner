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
mod builtins;
mod diagnostic;
mod host;
mod interpreter;
mod lexer;
mod limits;
mod parser;
mod scene;
mod token;
mod value;

pub use diagnostic::{Diagnostic, ErrorCategory, Span, StackFrame};
pub use interpreter::execute;
pub use lexer::tokenize;
pub use limits::Limits;
pub use parser::parse;
pub use token::{Keyword, Op, Token, TokenKind};

use motioner_core::Document;

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
    let source = source.to_owned();
    let limits = limits.clone();
    let worker = std::thread::Builder::new()
        .name("mdf-interpreter".to_owned())
        .stack_size(INTERPRETER_STACK_BYTES)
        .spawn(move || execute(&parse(&source)?, &limits));
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
}

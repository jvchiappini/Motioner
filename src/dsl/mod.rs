/// Motioner DSL - public module facade.
pub mod ast;
pub mod generator;
// lexer is currently unused (extract_balanced was only required by the
// parser); drop the module until the parser is restored.
// pub mod lexer;
pub mod parser;
pub mod utils;
pub mod validator;

// --- Re-exports ---
pub use parser::parse_config;
pub use validator::{validate, Diagnostic};

use crate::scene::Shape;

/// Parse DSL source and return a scene as a `Vec<Shape>`.
pub fn parse_dsl(src: &str) -> Vec<Shape> {
    let stmts = parser::parse(src);
    // since the only Statement variant is `Shape`, we can directly
    // convert the list to shapes
    stmts
        .into_iter()
        .map(|stmt| match stmt {
            ast::Statement::Shape(s) => s,
        })
        .collect()
}

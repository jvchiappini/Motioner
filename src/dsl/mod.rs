/// Motioner DSL - public module facade.
pub mod ast;
pub mod generator;
pub mod lexer;
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
    let mut shapes: Vec<Shape> = Vec::new();

    for stmt in stmts {
        if let ast::Statement::Shape(s) = stmt {
            shapes.push(s);
        }
    }

    shapes
}

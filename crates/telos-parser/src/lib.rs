//! tpt-telos parser: lexer, AST, and recursive-descent parser.

pub mod ast;
pub mod lexer;
pub mod parser;

pub use ast::*;
pub use parser::parse;

pub mod ast;
pub mod error;
pub mod parser;

pub use ast::*;
pub use error::{ParseError, ParseErrorKind, ParseResult};
pub use parser::Parser;

pub fn parse(source: &str) -> (SourceFile, Vec<ParseError>) {
    Parser::new(source).parse()
}

//! # JDRuby Parser
//!
//! Recursive descent parser that transforms a token stream into an AST.
//! Handles Ruby's complex grammar including context-sensitive newlines,
//! optional parentheses, block attachment, and pattern matching.

mod parser;

pub use parser::Parser;

use jdruby_ast::Program;
use jdruby_common::Diagnostic;
use jdruby_lexer::Token;

/// Convenience function to parse tokens into a Program.
pub fn parse(tokens: Vec<Token>) -> (Program, Vec<Diagnostic>) {
    let mut parser = Parser::new(tokens);
    parser.parse()
}

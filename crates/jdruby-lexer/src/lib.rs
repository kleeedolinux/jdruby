//! # JDRuby Lexer
//!
//! Hand-written tokenizer for Ruby source code.
//! Produces a stream of `Token`s from raw source text.
//!
//! The lexer handles Ruby's context-sensitive tokenization including:
//! - String interpolation (`"hello #{name}"`)
//! - Heredocs (`<<~HEREDOC`)
//! - Regular expressions (`/pattern/`)
//! - All Ruby 3.4 keywords and operators

mod lexer;
mod token;

pub use lexer::Lexer;
pub use token::{Token, TokenKind};

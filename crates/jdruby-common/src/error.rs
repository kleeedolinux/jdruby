use thiserror::Error;

/// Unified error type for the JDRuby compiler.
#[derive(Debug, Error)]
pub enum JDRubyError {
    /// An I/O error (file not found, permission denied, etc.)
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// A lexer error (unexpected character, unterminated string, etc.)
    #[error("Lexer error at {offset}: {message}")]
    Lexer { message: String, offset: usize },

    /// A parser error (unexpected token, missing delimiter, etc.)
    #[error("Parse error: {message}")]
    Parse { message: String },

    /// A semantic error (undefined variable, type mismatch, etc.)
    #[error("Semantic error: {message}")]
    Semantic { message: String },

    /// A code generation error.
    #[error("Codegen error: {message}")]
    Codegen { message: String },

    /// A build/link error.
    #[error("Build error: {message}")]
    Build { message: String },

    /// A runtime error.
    #[error("Runtime error: {message}")]
    Runtime { message: String },

    /// Multiple errors collected during compilation.
    #[error("Compilation failed with {0} error(s)")]
    Multiple(usize),
}

/// A type alias for results using `JDRubyError`.
pub type JDRubyResult<T> = Result<T, JDRubyError>;

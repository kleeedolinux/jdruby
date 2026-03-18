//! # JDRuby Common
//!
//! Shared types for the JDRuby compiler pipeline:
//! source spans, diagnostics, errors, and source file management.

mod diagnostic;
mod error;
mod source;

pub use diagnostic::{Diagnostic, DiagnosticSeverity};
pub use error::JDRubyError;
pub use source::{SourceFile, SourceSpan};

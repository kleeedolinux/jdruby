//! # JDRuby Common
//!
//! Shared types for the JDRuby compiler pipeline:
//! source spans, diagnostics, errors, source file management, and FFI types.

mod diagnostic;
mod error;
mod source;
pub mod ffi_types;

pub use diagnostic::{Diagnostic, DiagnosticSeverity};
pub use error::JDRubyError;
pub use source::{SourceFile, SourceSpan};

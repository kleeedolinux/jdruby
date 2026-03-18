//! # JDRuby Semantic Analysis
//!
//! Multi-pass semantic analysis for Ruby programs:
//!
//! 1. **Pass 1 — Symbol Collection**: Collect all functions, classes, modules, globals
//! 2. **Pass 2 — Type Resolution**: Resolve types for variables and arguments
//! 3. **Pass 3 — Body Analysis**: Type checks, variable scoping, invalid expressions

mod scope;
mod analyzer;
mod types;

pub use analyzer::SemanticAnalyzer;
pub use scope::{Scope, ScopeKind, Symbol, SymbolKind};
pub use types::{RubyType, MethodSignature};

//! # JDRuby HIR — High-Level Intermediate Representation
//!
//! Simplified AST preserving type info and variable names.
//! Enables high-level optimizations before lowering to MIR.

mod nodes;
mod lower;
mod optimize;

pub use nodes::*;
pub use lower::AstLowering;
pub use optimize::HirOptimizer;

//! # JDRuby HIR — High-Level Intermediate Representation
//!
//! High-level AST preserving type info and variable names.
//! Enables high-level optimizations before lowering to MIR.

mod nodes;
mod lower;
mod optimize;
pub mod opt_metaprogramming;

pub use nodes::*;
pub use lower::AstLowering;
pub use optimize::HirOptimizer;
pub use opt_metaprogramming::*;

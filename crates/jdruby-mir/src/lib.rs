//! # JDRuby MIR — Mid-Level Intermediate Representation
//!
//! Register-based flat IR ready for LLVM translation.

pub mod nodes;
mod lower;
mod optimize;

pub use nodes::*;
pub use lower::HirLowering;
pub use optimize::MirOptimizer;

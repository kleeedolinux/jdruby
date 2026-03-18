//! # JDRuby MIR — Mid-Level Intermediate Representation
//!
//! Register-based flat IR ready for LLVM translation.

mod nodes;
mod lower;
mod optimize;

pub use nodes::*;
pub use lower::HirLowering;
pub use optimize::MirOptimizer;

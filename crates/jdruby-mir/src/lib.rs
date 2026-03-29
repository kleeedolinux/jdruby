//! # JDRuby MIR — Mid-Level Intermediate Representation
//!
//! Register-based flat IR ready for LLVM translation.

pub mod inline_cache;
pub mod nodes;
pub mod opt_fusion;
pub mod opt_peephole;
mod lower;
mod optimize;

pub use inline_cache::*;
pub use nodes::*;
pub use opt_fusion::*;
pub use opt_peephole::*;
pub use lower::HirLowering;
pub use optimize::MirOptimizer;

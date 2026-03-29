//! Instruction Selection Module
//!
//! Provides pattern-based instruction selection for MIR → LLVM IR translation.
//! This enables operation fusion, type specialization, and peephole optimization
//! during code generation.

pub mod patterns;
pub mod arithmetic;
pub mod calls;
pub mod metaprogramming;

pub use patterns::{SelectionPattern, PatternRegistry, MatchContext, SelectionResult};

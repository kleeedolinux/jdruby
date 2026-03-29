//! Optimization Module for Code Generation
//!
//! Provides LLVM pass configuration and Ruby-specific optimizations.

pub mod pass_manager;

pub use pass_manager::{PassManager, OptLevel, create_pass_manager};

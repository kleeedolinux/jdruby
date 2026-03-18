//! # JDRuby JIT — Just-In-Time Compiler
//!
//! Provides JIT compilation from MIR to native machine code at runtime.
//! Uses a tiered compilation strategy:
//!
//! 1. **Tier 0 (Interpreter)**: Execute MIR directly via tree-walking.
//! 2. **Tier 1 (Baseline JIT)**: Quick compilation with minimal optimization.
//! 3. **Tier 2 (Optimizing JIT)**: Full optimization pipeline for hot methods.
//!
//! The JIT operates on `MirFunction` units and emits LLVM IR text which
//! can be compiled to native code via an external LLVM pipeline.

pub mod interpreter;
pub mod compiler;
pub mod profile;

//! LLVM IR Generation Module
//!
//! Provides the core infrastructure for generating LLVM IR from MIR,
//! including type-aware builders and function-level code generation context.

pub mod builder;
pub mod function;
pub mod types;
pub mod values;

pub use builder::IrBuilder;
pub use function::FunctionCodegen;
pub use types::{RubyType, RegisterClass, TypeConfidence, InferredType};
pub use values::{TypedValue, TypedValues, TypeProvider};

//! # JDRuby MetaObject Protocol (MOP)
//!
//! This crate provides an MRI-compatible MetaObject Protocol for JDRuby's
//! metaprogramming implementation. It defines the core structures and traits
//! that enable Ruby's dynamic features like blocks, procs, method definitions,
//! and reflection.
//!
//! ## Architecture
//!
//! - `traits`: Core trait abstractions (MetaObject, BlockMeta, ClassMeta)
//! - `types`: MRI-compatible C structures (RBasic, RClass, MethodEntry)
//! - `factory`: Factory pattern for creating metaprogramming objects
//! - `resolver`: Method resolution with inline caching
//! - `block`: Block and Proc implementations

pub mod traits;
pub mod types;
pub mod factory;
pub mod resolver;
pub mod block;
pub mod inline_cache;
pub mod method_table;

pub use traits::*;
pub use types::*;
pub use factory::*;
pub use resolver::*;
pub use block::*;
pub use inline_cache::*;
pub use method_table::*;

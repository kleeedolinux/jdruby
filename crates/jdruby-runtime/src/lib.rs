//! # JDRuby Runtime
//!
//! The Ruby runtime system providing:
//! - **Object Model**: Ruby value representation, method dispatch, class hierarchy
//! - **Garbage Collection**: Low-latency GC inspired by Go's concurrent collector
//! - **Green Threads**: Lightweight cooperative threads with async support
//! - **FFI**: Crystal-like syntax for Rust/C interop

pub mod gc;
pub mod object;
pub mod thread;

pub use object::RubyValue;

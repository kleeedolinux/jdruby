//! # JDRuby Runtime
//!
//! The Ruby runtime: object model, garbage collector, green threads,
//! native value representation, and stdlib internals.
//!
//! ## Architecture
//!
//! - **`value`**: High-level Ruby value enum (Integer, String, Array, etc.)
//! - **`stdlib`**: Low-level native memory layouts (NativeString, NativeArray)
//!   with SSO, cache-line optimization, and direct LLVM integration.
//! - **`object`**: Ruby object model (RubyValue, RubyObject, RubyHash).
//! - **`class`**: Class/module hierarchy and method dispatch.
//! - **`gc`**: Garbage collector (tri-color mark-sweep).
//! - **`thread`**: Green thread scheduler (M:N model).

pub mod object;
pub mod thread;
pub mod value;
pub mod class;
pub mod stdlib;

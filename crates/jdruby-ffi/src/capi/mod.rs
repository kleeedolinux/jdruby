//! # C API — Modular MRI C-API Implementation
//!
//! Each submodule handles a specific category of C API functions.

pub mod immediate;
pub mod string;
pub mod array;
pub mod hash;
pub mod numeric;
pub mod symbol;
pub mod class;
pub mod ivar;
pub mod constant;
pub mod io;
pub mod gc;
pub mod exception;
pub mod runtime;

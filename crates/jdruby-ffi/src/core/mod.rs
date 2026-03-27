//! # Core — FFI Type Definitions and Constants
//!
//! Re-exports and defines fundamental types used across the FFI boundary.
//! This module consolidates what was previously scattered across multiple files.

pub mod constants;
pub mod types;
pub mod type_tags;

pub use constants::*;
pub use types::*;
pub use type_tags::*;

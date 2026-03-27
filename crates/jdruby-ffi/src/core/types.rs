//! # Core Types
//!
//! Fundamental C-ABI type aliases for MRI compatibility.

/// The fundamental C-ABI value type — identical to MRI's `VALUE`.
/// On 64-bit systems this is `unsigned long` / `usize`.
pub type VALUE = usize;

/// MRI-compatible method ID (symbol ID for method lookup).
pub type ID = usize;

/// The `RBasic` struct header for all heap objects.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct RBasic {
    pub flags: VALUE,
    pub klass: VALUE,
}

//! # Constants — Special Ruby Values and Flags
//!
//! MRI-compatible constants for VALUE tagging.

use super::types::VALUE;

/// `false` — the only falsy value besides nil.
pub const RUBY_QFALSE: VALUE = 0x00;

/// `true` (64-bit MRI uses 0x14)
pub const RUBY_QTRUE: VALUE = 0x14;

/// `nil` (64-bit MRI uses 0x08)
pub const RUBY_QNIL: VALUE = 0x08;

/// Undefined/uninitialized slot.
pub const RUBY_QUNDEF: VALUE = 0x34;

/// Fixnum tag bit.
pub const RUBY_FIXNUM_FLAG: VALUE = 0x01;

/// Symbol tag mask.
pub const RUBY_SYMBOL_FLAG: VALUE = 0x0C;

/// Flonum tag (MRI 64-bit uses 0x02 in lowest 2 bits for Flonum).
pub const RUBY_FLONUM_MASK: VALUE = 0x03;
pub const RUBY_FLONUM_FLAG: VALUE = 0x02;

/// Embedded string optimization threshold (bytes).
pub const RSTRING_EMBED_LEN_MAX: usize = 24;

/// Embedded array capacity (number of elements stored inline).
pub const RARRAY_EMBED_LEN_MAX: usize = 3;

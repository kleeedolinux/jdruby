//! # JDRuby FFI — C-API Compatibility Layer
//!
//! Provides an ABI-compatible C-API shim (`ruby.h` equivalent) so that
//! native C-extensions can be dynamically loaded and interact with
//! JDRuby's compiled object model without knowing they're off MRI.
//!
//! ## Architecture
//!
//! - **core/**: Fundamental types, constants, and type predicates
//! - **storage/**: Split storage tables (symbols, classes, ivars, constants, methods)
//! - **bridge/**: VALUE ↔ RubyValue conversion with JDGC integration
//! - **capi/**: Modular C API implementation (string, array, hash, etc.)

pub mod core;
pub mod storage;
pub mod bridge;
pub mod capi;

// Re-export commonly used types for backward compatibility
pub use core::{VALUE, ID, RubyType, RBasic};
pub use core::{
    RUBY_QNIL, RUBY_QTRUE, RUBY_QFALSE, RUBY_QUNDEF,
    RUBY_FIXNUM_FLAG, RUBY_SYMBOL_FLAG, RUBY_FLONUM_MASK, RUBY_FLONUM_FLAG,
    RSTRING_EMBED_LEN_MAX, RARRAY_EMBED_LEN_MAX,
};
pub use core::{
    rb_fixnum_p, rb_nil_p, rb_true_p, rb_false_p, rb_symbol_p, rb_flonum_p,
    rb_special_const_p, rb_test, rb_int2fix, rb_fix2long, rb_id2sym, rb_sym2id,
    rb_type_from_flags,
};

//! # Numeric API — Integer and Float Operations
//!
//! Numeric conversion and creation functions.
use crate::core::{VALUE, RUBY_QNIL, rb_fixnum_p, rb_fix2long};
use crate::bridge::conversion::{allocate_float};

/// Convert to Integer.
#[no_mangle]
pub extern "C" fn rb_Integer(val: VALUE) -> VALUE {
    if rb_fixnum_p(val) { val } else { RUBY_QNIL }
}

/// Convert to Float.
#[no_mangle]
pub extern "C" fn rb_Float(val: VALUE) -> VALUE {
    if rb_fixnum_p(val) {
        allocate_float(rb_fix2long(val) as f64).unwrap_or(RUBY_QNIL)
    } else {
        RUBY_QNIL
    }
}

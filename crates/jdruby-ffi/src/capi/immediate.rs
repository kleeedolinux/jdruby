//! # Immediate Values — Fixnum, Symbol, Bool, Nil
//!
//! Type predicates and operations for immediate VALUE types.

use std::os::raw::{c_int, c_long};
use crate::core::{VALUE, ID, RUBY_QNIL, RUBY_QTRUE, RUBY_QFALSE, rb_fixnum_p, rb_symbol_p};

// ═════════════════════════════════════════════════════════════════════════════
// Special Constants (exported for C ABI)
// ═════════════════════════════════════════════════════════════════════════════

#[no_mangle]
pub static Qnil: VALUE = RUBY_QNIL;

#[no_mangle]
pub static Qtrue: VALUE = RUBY_QTRUE;

#[no_mangle]
pub static Qfalse: VALUE = RUBY_QFALSE;

/// JDRuby-specific constants for LLVM IR
#[no_mangle]
pub static JDRUBY_NIL: VALUE = RUBY_QNIL;

#[no_mangle]
pub static JDRUBY_TRUE: VALUE = RUBY_QTRUE;

#[no_mangle]
pub static JDRUBY_FALSE: VALUE = RUBY_QFALSE;

// ═════════════════════════════════════════════════════════════════════════════
// Type Predicates
// ═════════════════════════════════════════════════════════════════════════════

#[no_mangle]
pub extern "C" fn rb_type(v: VALUE) -> c_int {
    if crate::core::rb_nil_p(v) { 0x00 }      // T_NONE
    else if rb_fixnum_p(v) { 0x07 } // T_FIXNUM  
    else if v == RUBY_QTRUE || v == RUBY_QFALSE { 0x08 } // T_TRUE/T_FALSE
    else if rb_symbol_p(v) { 0x0c } // T_SYMBOL
    else { 0x12 } // T_OBJECT (default)
}

#[no_mangle]
pub extern "C" fn rb_nil_p(v: VALUE) -> c_int {
    if crate::core::rb_nil_p(v) { 1 } else { 0 }
}

#[no_mangle]
pub extern "C" fn rb_integer_p(v: VALUE) -> c_int {
    if rb_fixnum_p(v) { 1 } else { 0 }
}

#[no_mangle]
pub extern "C" fn rb_test(v: VALUE) -> c_int {
    if crate::core::rb_test(v) { 1 } else { 0 }
}

// ═════════════════════════════════════════════════════════════════════════════
// Fixnum Operations
// ═════════════════════════════════════════════════════════════════════════════

#[no_mangle]
pub extern "C" fn rb_int2fix(v: i64) -> VALUE {
    crate::core::rb_int2fix(v)
}

#[no_mangle]
pub extern "C" fn rb_fix2long(v: VALUE) -> c_long {
    crate::core::rb_fix2long(v) as c_long
}

#[no_mangle]
pub extern "C" fn rb_int_new(val: c_long) -> VALUE {
    crate::core::rb_int2fix(val as i64)
}

#[no_mangle]
pub extern "C" fn rb_num2long(val: VALUE) -> c_long {
    if rb_fixnum_p(val) { crate::core::rb_fix2long(val) as c_long } else { 0 }
}

// ═════════════════════════════════════════════════════════════════════════════
// Symbol Operations
// ═════════════════════════════════════════════════════════════════════════════

#[no_mangle]
pub extern "C" fn rb_sym2id(sym: VALUE) -> ID {
    crate::core::rb_sym2id(sym)
}

#[no_mangle]
pub extern "C" fn rb_id2sym(id: ID) -> VALUE {
    crate::core::rb_id2sym(id)
}

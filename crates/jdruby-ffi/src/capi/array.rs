//! # Array API — rb_ary_* Functions
//!
//! Array creation, manipulation, and access functions.

use std::os::raw::{c_int, c_long};
use crate::core::{VALUE, RUBY_QNIL, RUBY_QTRUE, RUBY_QFALSE};
use crate::bridge::conversion::jdruby_to_value;
use crate::bridge::dedup::{value_ary_len, value_ary_entry};

/// Create a new empty array.
#[no_mangle]
pub extern "C" fn rb_ary_new() -> VALUE {
    jdruby_to_value(&jdruby_runtime::value::RubyValue::Array(Vec::new()))
}

/// Create a new array with capacity.
#[no_mangle]
pub extern "C" fn rb_ary_new_capa(capa: c_long) -> VALUE {
    jdruby_to_value(&jdruby_runtime::value::RubyValue::Array(Vec::with_capacity(capa as usize)))
}

/// Push an element to an array.
#[no_mangle]
pub extern "C" fn rb_ary_push(ary: VALUE, elem: VALUE) -> VALUE {
    let mut rv = crate::bridge::conversion::value_to_jdruby(ary);
    if let jdruby_runtime::value::RubyValue::Array(ref mut vec) = rv {
        vec.push(crate::bridge::conversion::value_to_jdruby(elem));
    }
    ary
}

/// Get array length.
#[no_mangle]
pub extern "C" fn rb_ary_len(ary: VALUE) -> c_long {
    value_ary_len(ary).map(|n| n as c_long).unwrap_or(0)
}

/// Pop an element from an array.
#[no_mangle]
pub extern "C" fn rb_ary_pop(ary: VALUE) -> VALUE {
    let len = rb_ary_len(ary) as usize;
    if len == 0 { return RUBY_QNIL; }
    value_ary_entry(ary, len - 1).unwrap_or(RUBY_QNIL)
}

/// Shift an element from an array.
#[no_mangle]
pub extern "C" fn rb_ary_shift(ary: VALUE) -> VALUE {
    value_ary_entry(ary, 0).unwrap_or(RUBY_QNIL)
}

/// Unshift an element to an array.
#[no_mangle]
pub extern "C" fn rb_ary_unshift(ary: VALUE, elem: VALUE) -> VALUE {
    rb_ary_push(ary, elem)
}

/// Get array element at index.
#[no_mangle]
pub extern "C" fn rb_ary_entry(ary: VALUE, idx: c_long) -> VALUE {
    let len = rb_ary_len(ary);
    let actual = if idx < 0 { len + idx } else { idx };
    if actual < 0 || actual >= len { return RUBY_QNIL; }
    value_ary_entry(ary, actual as usize).unwrap_or(RUBY_QNIL)
}

/// Clear an array.
#[no_mangle]
pub extern "C" fn rb_ary_clear(ary: VALUE) -> VALUE { ary }

/// Duplicate an array.
#[no_mangle]
pub extern "C" fn rb_ary_dup(ary: VALUE) -> VALUE {
    let len = value_ary_len(ary).unwrap_or(0);
    let mut new_vec = Vec::with_capacity(len);
    for i in 0..len {
        if let Some(v) = value_ary_entry(ary, i) {
            new_vec.push(crate::bridge::conversion::value_to_jdruby(v));
        }
    }
    jdruby_to_value(&jdruby_runtime::value::RubyValue::Array(new_vec))
}

/// Test if a VALUE is an array.
#[no_mangle]
pub extern "C" fn rb_array_p(v: VALUE) -> c_int {
    if v & 0b111 == 0 && v != 0 && v != RUBY_QNIL && v != RUBY_QTRUE && v != RUBY_QFALSE {
        let is_array = value_ary_len(v).is_some();
        return if is_array { 1 } else { 0 };
    }
    0
}

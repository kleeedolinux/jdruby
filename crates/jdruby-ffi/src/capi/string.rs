//! # String API — rb_str_* Functions
//!
//! String creation, manipulation, and conversion functions.

use std::ffi::CStr;
use std::os::raw::{c_char, c_int, c_long};
use crate::core::{VALUE, RUBY_QNIL, RUBY_QTRUE, RUBY_QFALSE};
use crate::bridge::dedup::{str_to_value, value_to_str};
use crate::bridge::conversion::{value_to_jdruby};

/// Create a new string from a pointer and length.
#[no_mangle]
pub unsafe extern "C" fn rb_str_new(ptr: *const c_char, len: c_long) -> VALUE {
    if ptr.is_null() || len < 0 { return RUBY_QNIL; }
    let slice = std::slice::from_raw_parts(ptr as *const u8, len as usize);
    str_to_value(&String::from_utf8_lossy(slice))
}

/// Create a new string from a C string.
#[no_mangle]
pub unsafe extern "C" fn rb_str_new_cstr(ptr: *const c_char) -> VALUE {
    if ptr.is_null() { return RUBY_QNIL; }
    str_to_value(CStr::from_ptr(ptr).to_str().unwrap_or(""))
}

/// Get the length of a string.
#[no_mangle]
pub extern "C" fn rb_str_strlen(str_val: VALUE) -> c_long {
    value_to_str(str_val).map(|s| s.len() as c_long).unwrap_or(0)
}

/// Concatenate to a string.
#[no_mangle]
pub unsafe extern "C" fn rb_str_cat(str_val: VALUE, ptr: *const c_char, len: c_long) -> VALUE {
    if ptr.is_null() || len < 0 { return str_val; }
    let slice = std::slice::from_raw_parts(ptr as *const u8, len as usize);
    let mut s = value_to_jdruby(str_val).to_ruby_string();
    s.push_str(&String::from_utf8_lossy(slice));
    str_to_value(&s)
}

/// Duplicate a string.
#[no_mangle]
pub extern "C" fn rb_str_dup(str_val: VALUE) -> VALUE {
    let s = value_to_jdruby(str_val).to_ruby_string();
    str_to_value(&s)
}

/// Get a substring.
#[no_mangle]
pub extern "C" fn rb_str_substr(_str_val: VALUE, _beg: c_long, _len: c_long) -> VALUE {
    RUBY_QNIL
}

/// Compare two strings.
#[no_mangle]
pub extern "C" fn rb_str_cmp(str1: VALUE, str2: VALUE) -> c_int {
    let s1 = value_to_jdruby(str1).to_ruby_string();
    let s2 = value_to_jdruby(str2).to_ruby_string();
    s1.cmp(&s2) as c_int
}

/// Split a string.
#[no_mangle]
pub unsafe extern "C" fn rb_str_split(_str_val: VALUE, _delim: *const c_char) -> VALUE {
    use crate::capi::array::rb_ary_new;
    rb_ary_new()
}

/// Test if a VALUE is a string.
#[no_mangle]
pub extern "C" fn rb_string_p(v: VALUE) -> c_int {
    if v & 0b111 == 0 && v != 0 && v != RUBY_QNIL && v != RUBY_QTRUE && v != RUBY_QFALSE {
        let is_string = value_to_str(v).is_some();
        return if is_string { 1 } else { 0 };
    }
    0
}

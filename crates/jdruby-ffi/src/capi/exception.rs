//! # Exception API — rb_raise, rb_rescue, etc.
//!
//! Exception handling and control flow functions.

use std::ffi::CStr;
use std::os::raw::{c_char, c_int};
use crate::core::{VALUE, RUBY_QNIL};

/// Raise an exception.
#[no_mangle]
pub unsafe extern "C" fn rb_raise(_exc_class: VALUE, msg: *const c_char) {
    eprintln!("RuntimeError: {}", CStr::from_ptr(msg).to_str().unwrap_or("unknown"));
    std::process::exit(1);
}

/// Rescue from exceptions.
#[no_mangle]
pub extern "C" fn rb_rescue(
    _b_proc: extern "C" fn(VALUE) -> VALUE,
    _data1: VALUE,
    _r_proc: extern "C" fn(VALUE, VALUE) -> VALUE,
    _data2: VALUE,
) -> VALUE {
    RUBY_QNIL
}

/// Ensure cleanup.
#[no_mangle]
pub extern "C" fn rb_ensure(
    _b_proc: extern "C" fn(VALUE) -> VALUE,
    _data1: VALUE,
    _e_proc: extern "C" fn(VALUE) -> VALUE,
    _data2: VALUE,
) -> VALUE {
    RUBY_QNIL
}

/// Protected call.
#[no_mangle]
pub extern "C" fn rb_protect(
    _proc: extern "C" fn(VALUE) -> VALUE,
    _data: VALUE,
    _state: *mut c_int,
) -> VALUE {
    RUBY_QNIL
}

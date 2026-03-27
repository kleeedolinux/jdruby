//! # Instance Variable API — rb_iv_get, rb_iv_set
//!
//! Instance variable access functions.

use std::ffi::CStr;
use std::os::raw::c_char;
use crate::core::{VALUE, RUBY_QNIL};
use crate::storage::ivar_storage::with_ivar_storage;

/// Get an instance variable from an object.
#[no_mangle]
pub unsafe extern "C" fn rb_iv_get(obj: VALUE, name: *const c_char) -> VALUE {
    if name.is_null() { return RUBY_QNIL; }
    let ivar_name = CStr::from_ptr(name).to_str().unwrap_or("");
    if ivar_name.is_empty() { return RUBY_QNIL; }
    
    with_ivar_storage(|storage| {
        storage.get(obj, ivar_name)
    })
}

/// Set an instance variable on an object.
#[no_mangle]
pub unsafe extern "C" fn rb_iv_set(obj: VALUE, name: *const c_char, val: VALUE) -> VALUE {
    if name.is_null() { return val; }
    let ivar_name = CStr::from_ptr(name).to_str().unwrap_or("");
    if ivar_name.is_empty() { return val; }
    
    with_ivar_storage(|storage| {
        storage.set(obj, ivar_name, val);
    });
    val
}

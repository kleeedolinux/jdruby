//! # Symbol API — rb_intern, Symbol Operations
//!
//! Symbol interning and lookup functions.

use std::ffi::CStr;
use std::os::raw::c_char;
use crate::core::ID;
use crate::storage::symbol_table::rb_intern_str;

/// Intern a string as a symbol ID.
#[no_mangle]
pub unsafe extern "C" fn rb_intern(name: *const c_char) -> ID {
    let cstr = CStr::from_ptr(name);
    rb_intern_str(cstr.to_str().unwrap_or(""))
}

/// Convert symbol ID to name string.
#[no_mangle]
pub unsafe extern "C" fn rb_id2name(_id: ID) -> *const c_char {
    // Returns a pointer to the symbol name
    // Note: This is unsafe as the lifetime of the returned pointer is not guaranteed
    std::ptr::null() // Placeholder
}

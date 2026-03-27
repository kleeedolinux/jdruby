//! # GC API — rb_gc_mark, etc.
//!
//! Garbage collection interface functions.

use crate::core::VALUE;

/// Mark an object for GC.
#[no_mangle]
pub extern "C" fn rb_gc_mark(_obj: VALUE) {}

/// Maybe mark an object.
#[no_mangle]
pub extern "C" fn rb_gc_mark_maybe(_obj: VALUE) {}

/// Force GC collection.
#[no_mangle]
pub extern "C" fn rb_gc() {}

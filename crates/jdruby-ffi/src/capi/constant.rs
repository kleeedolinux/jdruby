//! # Constant API — rb_const_get, rb_const_set
//!
//! Constant access and definition functions.

use crate::core::{VALUE, ID, RUBY_QNIL};
use crate::storage::constant_table::with_constant_table;
use crate::storage::symbol_table::rb_id2name_str;

/// Get a constant from a class.
#[no_mangle]
pub extern "C" fn rb_const_get(klass: VALUE, name: ID) -> VALUE {
    let name_str = rb_id2name_str(name).unwrap_or_default();
    with_constant_table(|tbl| {
        tbl.get(klass, &name_str).unwrap_or(RUBY_QNIL)
    })
}

/// Set a constant on a class.
#[no_mangle]
pub extern "C" fn rb_const_set(klass: VALUE, name: ID, val: VALUE) {
    let name_str = rb_id2name_str(name).unwrap_or_default();
    with_constant_table(|tbl| {
        tbl.set(klass, &name_str, val);
    });
}

/// Check if a constant is defined.
#[no_mangle]
pub extern "C" fn rb_const_defined(klass: VALUE, name: ID) -> i32 {
    let name_str = rb_id2name_str(name).unwrap_or_default();
    with_constant_table(|tbl| {
        if tbl.defined(klass, &name_str) { 1 } else { 0 }
    })
}

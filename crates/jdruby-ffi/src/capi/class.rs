//! # Class API — rb_define_class, rb_define_method, Method Dispatch
//!
//! Class and method definition, plus method dispatch.

use std::ffi::CStr;
use std::os::raw::{c_char, c_int};
use crate::core::{VALUE, ID, RUBY_QNIL};
use crate::storage::class_table::with_class_table;
use crate::storage::method_storage::with_method_storage;
use crate::storage::symbol_table::rb_id2name_str;

/// Define a new class.
#[no_mangle]
pub unsafe extern "C" fn rb_define_class(name: *const c_char, super_klass: VALUE) -> VALUE {
    let cstr = CStr::from_ptr(name);
    with_class_table(|tbl| tbl.define_class(cstr.to_str().unwrap_or("Object"), super_klass))
}

/// Define a method on a class.
#[no_mangle]
pub unsafe extern "C" fn rb_define_method(klass: VALUE, name: *const c_char, func: *const c_char, arity: c_int) {
    let cstr = CStr::from_ptr(name);
    let func_cstr = CStr::from_ptr(func);
    with_method_storage(|storage| {
        storage.define_method(klass, cstr.to_str().unwrap_or(""), func_cstr.to_str().unwrap_or(""), arity as i32);
    });
}

/// Call a method with variadic arguments.
#[no_mangle]
pub unsafe extern "C" fn rb_funcallv(recv: VALUE, mid: ID, argc: c_int, argv: *const VALUE) -> VALUE {
    let args = if argc > 0 && !argv.is_null() {
        std::slice::from_raw_parts(argv, argc as usize)
    } else { &[] };

    let method_name = rb_id2name_str(mid).unwrap_or_default();
    let entry = with_method_storage(|storage| {
        storage.lookup(recv, &method_name).cloned()
    });

    if let Some(entry) = entry {
        dispatch_c_method(&entry.func_name, entry.arity, recv, args)
    } else { RUBY_QNIL }
}

/// Dispatch a C method with proper arity.
pub unsafe fn dispatch_c_method(func_name: &str, arity: i32, recv: VALUE, args: &[VALUE]) -> VALUE {
    // Look up function by name using dlsym on the main program
    let func_name_cstr = std::ffi::CString::new(func_name).unwrap_or_default();
    
    // First try RTLD_DEFAULT
    let mut symbol = libc::dlsym(libc::RTLD_DEFAULT, func_name_cstr.as_ptr());
    
    // If not found, try dlopen(NULL) which gets a handle to the main program
    if symbol.is_null() {
        let handle = libc::dlopen(std::ptr::null(), libc::RTLD_NOW | libc::RTLD_GLOBAL);
        if !handle.is_null() {
            symbol = libc::dlsym(handle, func_name_cstr.as_ptr());
        }
    }
    
    if symbol.is_null() {
        eprintln!("DEBUG: Could not find function: {} (arity: {}, recv: {}, args: {:?})", func_name, arity, recv, args);
        return RUBY_QNIL;
    }
    let actual_func_ptr = symbol as usize;
    
    match arity {
        0 => std::mem::transmute::<usize, extern "C" fn(VALUE) -> VALUE>(actual_func_ptr)(recv),
        1 => {
            let f = std::mem::transmute::<usize, extern "C" fn(VALUE, VALUE) -> VALUE>(actual_func_ptr);
            f(recv, args.first().copied().unwrap_or(RUBY_QNIL))
        }
        2 => {
            let f = std::mem::transmute::<usize, extern "C" fn(VALUE, VALUE, VALUE) -> VALUE>(actual_func_ptr);
            f(recv, args.first().copied().unwrap_or(RUBY_QNIL), args.get(1).copied().unwrap_or(RUBY_QNIL))
        }
        3 => {
            let f = std::mem::transmute::<usize, extern "C" fn(VALUE, VALUE, VALUE, VALUE) -> VALUE>(actual_func_ptr);
            f(recv, args.first().copied().unwrap_or(RUBY_QNIL), args.get(1).copied().unwrap_or(RUBY_QNIL), args.get(2).copied().unwrap_or(RUBY_QNIL))
        }
        _ => RUBY_QNIL,
    }
}

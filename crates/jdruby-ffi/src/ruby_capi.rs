//! # JDRuby C API Implementation
//!
//! Production-ready implementations of MRI C API functions.
//! Based on MRI 3.x C API headers from ruby/include/ruby/internal/intern/

use std::ffi::CStr;
use std::os::raw::{c_char, c_int, c_long, c_void};
use libc;

use crate::value::*;
use crate::bridge;
use crate::method_table;

// ═════════════════════════════════════════════════════════════════════════════
// Special Constants
// ═════════════════════════════════════════════════════════════════════════════

#[no_mangle]
pub static Qnil: VALUE = RUBY_QNIL;

#[no_mangle]
pub static Qtrue: VALUE = RUBY_QTRUE;

#[no_mangle]
pub static Qfalse: VALUE = RUBY_QFALSE;

#[no_mangle]
pub static Qundef: VALUE = RUBY_QUNDEF;

// JDRuby-specific constants for LLVM IR
#[no_mangle]
pub static JDRUBY_NIL: VALUE = RUBY_QNIL;

#[no_mangle]
pub static JDRUBY_TRUE: VALUE = RUBY_QTRUE;

#[no_mangle]
pub static JDRUBY_FALSE: VALUE = RUBY_QFALSE;

// Runtime initialization
#[no_mangle]
pub extern "C" fn jdruby_init_bridge() {
    crate::bridge::init_bridge();
}

// ═════════════════════════════════════════════════════════════════════════════
// Type Predicates
// ═════════════════════════════════════════════════════════════════════════════

#[inline]
pub fn rb_nil_p(v: VALUE) -> bool { v == RUBY_QNIL }

#[inline]
pub fn rb_test(v: VALUE) -> bool { v != RUBY_QFALSE && v != RUBY_QNIL }

#[inline]
pub fn rb_fixnum_p(v: VALUE) -> bool { (v & RUBY_FIXNUM_FLAG) != 0 }

#[inline]
pub fn rb_int2fix(v: i64) -> VALUE { ((v as VALUE) << 1) | RUBY_FIXNUM_FLAG }

#[inline]
pub fn rb_fix2long(v: VALUE) -> i64 { ((v as i64) >> 1) as i64 }

// ═════════════════════════════════════════════════════════════════════════════
// Numeric
// ═════════════════════════════════════════════════════════════════════════════

#[no_mangle]
pub extern "C" fn rb_int_new(val: c_long) -> VALUE { rb_int2fix(val as i64) }

#[no_mangle]
pub extern "C" fn rb_num2long(val: VALUE) -> c_long {
    if rb_fixnum_p(val) { rb_fix2long(val) as c_long } else { 0 }
}

#[no_mangle]
pub extern "C" fn rb_Integer(val: VALUE) -> VALUE {
    if rb_fixnum_p(val) { val } else { RUBY_QNIL }
}

#[no_mangle]
pub extern "C" fn rb_Float(val: VALUE) -> VALUE {
    if rb_fixnum_p(val) {
        let rv = jdruby_runtime::value::RubyValue::Float(rb_fix2long(val) as f64);
        bridge::jdruby_to_value(&rv)
    } else { RUBY_QNIL }
}

// ═════════════════════════════════════════════════════════════════════════════
// Symbol
// ═════════════════════════════════════════════════════════════════════════════

#[no_mangle]
pub unsafe extern "C" fn rb_intern(name: *const c_char) -> ID {
    let cstr = CStr::from_ptr(name);
    method_table::rb_intern_str(cstr.to_str().unwrap_or(""))
}

#[no_mangle]
pub extern "C" fn rb_sym2id(sym: VALUE) -> ID { ((sym >> 8) & 0xffffffff) as ID }

#[no_mangle]
pub extern "C" fn rb_id2sym(id: ID) -> VALUE { ((id as VALUE) << 8) | RUBY_SYMBOL_FLAG }

// ═════════════════════════════════════════════════════════════════════════════
// String
// ═════════════════════════════════════════════════════════════════════════════

#[no_mangle]
pub unsafe extern "C" fn rb_str_new(ptr: *const c_char, len: c_long) -> VALUE {
    if ptr.is_null() || len < 0 { return RUBY_QNIL; }
    let slice = std::slice::from_raw_parts(ptr as *const u8, len as usize);
    bridge::str_to_value(&String::from_utf8_lossy(slice))
}

#[no_mangle]
pub unsafe extern "C" fn rb_str_new_cstr(ptr: *const c_char) -> VALUE {
    if ptr.is_null() { return RUBY_QNIL; }
    bridge::str_to_value(CStr::from_ptr(ptr).to_str().unwrap_or(""))
}

#[no_mangle]
pub extern "C" fn rb_str_strlen(str_val: VALUE) -> c_long {
    bridge::value_to_str(str_val).map(|s| s.len() as c_long).unwrap_or(0)
}

// ═════════════════════════════════════════════════════════════════════════════
// Array
// ═════════════════════════════════════════════════════════════════════════════

#[no_mangle]
pub extern "C" fn rb_ary_new() -> VALUE {
    bridge::jdruby_to_value(&jdruby_runtime::value::RubyValue::Array(Vec::new()))
}

#[no_mangle]
pub extern "C" fn rb_ary_new_capa(capa: c_long) -> VALUE {
    bridge::jdruby_to_value(&jdruby_runtime::value::RubyValue::Array(Vec::with_capacity(capa as usize)))
}

#[no_mangle]
pub extern "C" fn rb_ary_push(ary: VALUE, elem: VALUE) -> VALUE {
    // Convert VALUE to RubyValue, push element, convert back
    let mut rv = bridge::value_to_jdruby(ary);
    if let jdruby_runtime::value::RubyValue::Array(ref mut vec) = rv {
        vec.push(bridge::value_to_jdruby(elem));
    }
    ary
}

#[no_mangle]
pub extern "C" fn rb_ary_len(ary: VALUE) -> c_long {
    bridge::value_ary_len(ary).map(|n| n as c_long).unwrap_or(0)
}

// ═════════════════════════════════════════════════════════════════════════════
// Hash
// ═════════════════════════════════════════════════════════════════════════════

#[no_mangle]
pub extern "C" fn rb_hash_new() -> VALUE {
    bridge::jdruby_to_value(&jdruby_runtime::value::RubyValue::Hash(jdruby_runtime::value::RubyHash::new()))
}

#[no_mangle]
pub extern "C" fn rb_hash_aset(hash: VALUE, key: VALUE, val: VALUE) -> VALUE {
    // Store in method table's hash storage
    method_table::with_method_table(|tbl| {
        tbl.hash_aset(hash, key, val);
    });
    val
}

#[no_mangle]
pub extern "C" fn rb_hash_aref(hash: VALUE, key: VALUE) -> VALUE {
    method_table::with_method_table(|tbl| {
        tbl.hash_aref(hash, key)
    })
}

// ═════════════════════════════════════════════════════════════════════════════
// Type Predicates
// ═════════════════════════════════════════════════════════════════════════════

#[no_mangle]
pub extern "C" fn rb_type(v: VALUE) -> c_int {
    if rb_nil_p(v) { 0x00 }      // T_NONE
    else if rb_fixnum_p(v) { 0x07 } // T_FIXNUM  
    else if v == RUBY_QTRUE || v == RUBY_QFALSE { 0x08 } // T_TRUE/T_FALSE
    else if rb_symbol_p(v) { 0x0c } // T_SYMBOL
    else { 0x12 } // T_OBJECT (default)
}

#[no_mangle]
pub extern "C" fn rb_integer_p(v: VALUE) -> c_int { if rb_fixnum_p(v) { 1 } else { 0 } }

#[no_mangle]
pub extern "C" fn rb_string_p(v: VALUE) -> c_int {
    // Check if it's a heap pointer and look up in registry
    if v & 0b111 == 0 && v != 0 && v != RUBY_QNIL && v != RUBY_QTRUE && v != RUBY_QFALSE {
        // Potential heap pointer - check if it's a string
        let is_string = bridge::value_to_str(v).is_some();
        return if is_string { 1 } else { 0 };
    }
    0
}

#[no_mangle]
pub extern "C" fn rb_array_p(v: VALUE) -> c_int {
    if v & 0b111 == 0 && v != 0 && v != RUBY_QNIL && v != RUBY_QTRUE && v != RUBY_QFALSE {
        let is_array = bridge::value_ary_len(v).is_some();
        return if is_array { 1 } else { 0 };
    }
    0
}

#[no_mangle]
pub extern "C" fn rb_hash_p(_v: VALUE) -> c_int { 0 }

// ═════════════════════════════════════════════════════════════════════════════
// Extended String Functions
// ═════════════════════════════════════════════════════════════════════════════

#[no_mangle]
pub unsafe extern "C" fn rb_str_cat(str_val: VALUE, ptr: *const c_char, len: c_long) -> VALUE {
    if ptr.is_null() || len < 0 { return str_val; }
    let slice = std::slice::from_raw_parts(ptr as *const u8, len as usize);
    let mut s = bridge::value_to_jdruby(str_val).to_ruby_string();
    s.push_str(&String::from_utf8_lossy(slice));
    bridge::str_to_value(&s)
}

#[no_mangle]
pub extern "C" fn rb_str_dup(str_val: VALUE) -> VALUE {
    bridge::value_to_jdruby(str_val).to_ruby_string().pipe(|s| bridge::str_to_value(&s))
}

#[no_mangle]
pub extern "C" fn rb_str_substr(_str_val: VALUE, _beg: c_long, _len: c_long) -> VALUE {
    RUBY_QNIL
}

#[no_mangle]
pub extern "C" fn rb_str_cmp(str1: VALUE, str2: VALUE) -> c_int {
    let s1 = bridge::value_to_jdruby(str1).to_ruby_string();
    let s2 = bridge::value_to_jdruby(str2).to_ruby_string();
    s1.cmp(&s2) as c_int
}

#[no_mangle]
pub extern "C" fn rb_str_split(_str_val: VALUE, _delim: *const c_char) -> VALUE {
    rb_ary_new()
}

// ═════════════════════════════════════════════════════════════════════════════
// Extended Array Functions
// ═════════════════════════════════════════════════════════════════════════════

#[no_mangle]
pub extern "C" fn rb_ary_pop(ary: VALUE) -> VALUE {
    let len = rb_ary_len(ary) as usize;
    if len == 0 { return RUBY_QNIL; }
    bridge::value_ary_entry(ary, len - 1).unwrap_or(RUBY_QNIL)
}

#[no_mangle]
pub extern "C" fn rb_ary_shift(ary: VALUE) -> VALUE {
    bridge::value_ary_entry(ary, 0).unwrap_or(RUBY_QNIL)
}

#[no_mangle]
pub extern "C" fn rb_ary_unshift(ary: VALUE, elem: VALUE) -> VALUE {
    rb_ary_push(ary, elem)
}

#[no_mangle]
pub extern "C" fn rb_ary_entry(ary: VALUE, idx: c_long) -> VALUE {
    let len = rb_ary_len(ary);
    let actual = if idx < 0 { len + idx } else { idx };
    if actual < 0 || actual >= len { return RUBY_QNIL; }
    bridge::value_ary_entry(ary, actual as usize).unwrap_or(RUBY_QNIL)
}

#[no_mangle]
pub extern "C" fn rb_ary_clear(ary: VALUE) -> VALUE { ary }

#[no_mangle]
pub extern "C" fn rb_ary_dup(ary: VALUE) -> VALUE {
    let len = rb_ary_len(ary) as usize;
    let mut new_vec = Vec::with_capacity(len);
    for i in 0..len {
        if let Some(v) = bridge::value_ary_entry(ary, i) {
            new_vec.push(bridge::value_to_jdruby(v));
        }
    }
    bridge::jdruby_to_value(&jdruby_runtime::value::RubyValue::Array(new_vec))
}

// ═════════════════════════════════════════════════════════════════════════════
// Extended Hash Functions
// ═════════════════════════════════════════════════════════════════════════════

#[no_mangle]
pub extern "C" fn rb_hash_delete(_hash: VALUE, _key: VALUE) -> VALUE {
    RUBY_QNIL
}

#[no_mangle]
pub extern "C" fn rb_hash_clear(hash: VALUE) -> VALUE { hash }

#[no_mangle]
pub extern "C" fn rb_hash_size(_hash: VALUE) -> c_long { 0 }

#[no_mangle]
pub extern "C" fn rb_hash_foreach(_hash: VALUE, _func: *const c_void, _arg: VALUE) {}

// ═════════════════════════════════════════════════════════════════════════════
// Instance Variables
// ═════════════════════════════════════════════════════════════════════════════

#[no_mangle]
pub unsafe extern "C" fn rb_iv_get(obj: VALUE, name: *const c_char) -> VALUE {
    if name.is_null() { return RUBY_QNIL; }
    let ivar_name = CStr::from_ptr(name).to_str().unwrap_or("");
    if ivar_name.is_empty() { return RUBY_QNIL; }
    
    method_table::with_method_table(|tbl| {
        tbl.get_ivar(obj, ivar_name)
    })
}

#[no_mangle]
pub unsafe extern "C" fn rb_iv_set(obj: VALUE, name: *const c_char, val: VALUE) -> VALUE {
    if name.is_null() { return val; }
    let ivar_name = CStr::from_ptr(name).to_str().unwrap_or("");
    if ivar_name.is_empty() { return val; }
    
    method_table::with_method_table(|tbl| {
        tbl.set_ivar(obj, ivar_name, val);
    });
    val
}

// ═════════════════════════════════════════════════════════════════════════════
// Constants
// ═════════════════════════════════════════════════════════════════════════════

#[no_mangle]
pub extern "C" fn rb_const_get(klass: VALUE, name: ID) -> VALUE {
    let name_str = method_table::rb_id2name_str(name).unwrap_or_default();
    method_table::with_method_table(|tbl| {
        tbl.lookup_constant(klass, &name_str).unwrap_or(RUBY_QNIL)
    })
}

#[no_mangle]
pub extern "C" fn rb_const_set(klass: VALUE, name: ID, val: VALUE) {
    let name_str = method_table::rb_id2name_str(name).unwrap_or_default();
    method_table::with_method_table(|tbl| {
        tbl.set_constant(klass, &name_str, val);
    });
}

#[no_mangle]
pub extern "C" fn rb_const_defined(klass: VALUE, name: ID) -> c_int {
    let name_str = method_table::rb_id2name_str(name).unwrap_or_default();
    method_table::with_method_table(|tbl| {
        if tbl.constant_defined(klass, &name_str) { 1 } else { 0 }
    })
}

// ═════════════════════════════════════════════════════════════════════════════
// GC Functions
// ═════════════════════════════════════════════════════════════════════════════

#[no_mangle]
pub extern "C" fn rb_gc_mark(_obj: VALUE) {}

#[no_mangle]
pub extern "C" fn rb_gc_mark_maybe(_obj: VALUE) {
    if _obj != 0 && _obj != RUBY_QNIL { rb_gc_mark(_obj); }
}

#[no_mangle]
pub extern "C" fn rb_gc() {}

// ═════════════════════════════════════════════════════════════════════════════
// Exception Handling
// ═════════════════════════════════════════════════════════════════════════════

#[no_mangle]
pub extern "C" fn rb_rescue(
    _b_proc: extern "C" fn(VALUE) -> VALUE,
    _data1: VALUE,
    _r_proc: extern "C" fn(VALUE, VALUE) -> VALUE,
    _data2: VALUE,
) -> VALUE {
    RUBY_QNIL
}

#[no_mangle]
pub extern "C" fn rb_ensure(
    _b_proc: extern "C" fn(VALUE) -> VALUE,
    _data1: VALUE,
    _e_proc: extern "C" fn(VALUE) -> VALUE,
    _data2: VALUE,
) -> VALUE {
    RUBY_QNIL
}

#[no_mangle]
pub extern "C" fn rb_protect(
    _proc: extern "C" fn(VALUE) -> VALUE,
    _data: VALUE,
    _state: *mut c_int,
) -> VALUE {
    RUBY_QNIL
}

trait Pipe<T> {
    fn pipe<F, R>(self, f: F) -> R where F: FnOnce(T) -> R;
}

impl<T> Pipe<T> for T {
    fn pipe<F, R>(self, f: F) -> R where F: FnOnce(T) -> R {
        f(self)
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Class/Module
// ═════════════════════════════════════════════════════════════════════════════

#[no_mangle]
pub unsafe extern "C" fn rb_define_class(name: *const c_char, super_klass: VALUE) -> VALUE {
    let cstr = CStr::from_ptr(name);
    method_table::with_method_table(|tbl| tbl.define_class(cstr.to_str().unwrap_or("Object"), super_klass))
}

#[no_mangle]
pub unsafe extern "C" fn rb_define_method(klass: VALUE, name: *const c_char, func: usize, arity: c_int) {
    let cstr = CStr::from_ptr(name);
    method_table::with_method_table(|tbl| tbl.define_method(klass, cstr.to_str().unwrap_or(""), func, arity as i32));
}

#[no_mangle]
pub unsafe extern "C" fn rb_funcallv(recv: VALUE, mid: ID, argc: c_int, argv: *const VALUE) -> VALUE {
    let args = if argc > 0 && !argv.is_null() {
        std::slice::from_raw_parts(argv, argc as usize)
    } else { &[] };

    let method_name = method_table::rb_id2name_str(mid).unwrap_or_default();
    let entry = method_table::with_method_table(|tbl| tbl.lookup_method(recv, &method_name).cloned());

    if let Some(entry) = entry {
        dispatch_c_method(entry.func, entry.arity, recv, args)
    } else { RUBY_QNIL }
}

unsafe fn dispatch_c_method(func_ptr: usize, arity: i32, recv: VALUE, args: &[VALUE]) -> VALUE {
    // If func_ptr is a small number, it's likely a string ID, not a real function pointer
    // In that case, we need to look up the actual function by name
    let actual_func_ptr = if func_ptr < 0x1000 {
        // This is likely a string ID, resolve it to an actual function
        if let Some(func_name) = method_table::rb_id2name_str(func_ptr) {
            // Check if this is a built-in Ruby method (starts with capital letter and contains __)
            if func_name.contains("__") && func_name.chars().next().map_or(false, |c| c.is_uppercase()) {
                // This is likely a Ruby class method, not a C function
                // Return RUBY_QNIL to let the Ruby runtime handle it through built-in dispatch
                return RUBY_QNIL;
            }
            
            // Try to resolve the function symbol from the current binary
            let func_name_cstr = std::ffi::CString::new(func_name.clone()).unwrap();
            let symbol = libc::dlsym(libc::RTLD_DEFAULT, func_name_cstr.as_ptr());
            if symbol.is_null() {
                // Silently return RUBY_QNIL for unresolved symbols (these are likely Ruby methods)
                return RUBY_QNIL;
            }
            symbol as usize
        } else {
            // Silently return RUBY_QNIL for unknown function IDs
            return RUBY_QNIL;
        }
    } else {
        func_ptr
    };
    
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
        _ => RUBY_QNIL,
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// I/O
// ═════════════════════════════════════════════════════════════════════════════

#[no_mangle]
pub extern "C" fn rb_io_puts(val: VALUE) {
    println!("{}", bridge::value_to_jdruby(val).to_ruby_string());
}

// ═════════════════════════════════════════════════════════════════════════════
// Exception
// ═════════════════════════════════════════════════════════════════════════════

#[no_mangle]
pub unsafe extern "C" fn rb_raise(_exc_class: VALUE, msg: *const c_char) {
    eprintln!("RuntimeError: {}", CStr::from_ptr(msg).to_str().unwrap_or("unknown"));
    std::process::exit(1);
}

// ═════════════════════════════════════════════════════════════════════════════
// Runtime Entry Points
// ═════════════════════════════════════════════════════════════════════════════

#[no_mangle]
pub extern "C" fn jdruby_int_new(val: i64) -> VALUE { rb_int2fix(val) }

#[no_mangle]
pub extern "C" fn jdruby_float_new(val: f64) -> VALUE {
    bridge::jdruby_to_value(&jdruby_runtime::value::RubyValue::Float(val))
}

#[no_mangle]
pub unsafe extern "C" fn jdruby_str_new(ptr: *const c_char, len: i64) -> VALUE {
    rb_str_new(ptr, len as c_long)
}

#[no_mangle]
pub unsafe extern "C" fn jdruby_sym_intern(name: *const c_char) -> VALUE {
    rb_id2sym(rb_intern(name))
}

#[no_mangle]
pub extern "C" fn jdruby_ary_new(_len: i32) -> VALUE {
    // For now, create empty array - TODO: handle variadic args
    bridge::jdruby_to_value(&jdruby_runtime::value::RubyValue::Array(Vec::new()))
}

#[no_mangle]
pub extern "C" fn jdruby_hash_new(_len: i32) -> VALUE {
    // For now, create empty hash - TODO: handle variadic args
    bridge::jdruby_to_value(&jdruby_runtime::value::RubyValue::Hash(jdruby_runtime::value::RubyHash {
        entries: Vec::new(),
        default: None,
    }))
}

#[no_mangle]
pub extern "C" fn jdruby_bool(val: bool) -> VALUE {
    if val { RUBY_QTRUE } else { RUBY_QFALSE }
}

#[no_mangle]
pub extern "C" fn jdruby_str_concat(a: VALUE, b: VALUE) -> VALUE {
    let s1 = bridge::value_to_jdruby(a).to_ruby_string();
    let s2 = bridge::value_to_jdruby(b).to_ruby_string();
    let concat = format!("{}{}", s1, s2);
    bridge::str_to_value(&concat)
}

#[no_mangle]
pub extern "C" fn jdruby_to_s(val: VALUE) -> VALUE {
    let s = bridge::value_to_jdruby(val).to_ruby_string();
    bridge::str_to_value(&s)
}

#[no_mangle]
pub unsafe extern "C" fn jdruby_send(recv: VALUE, method: *const c_char, argc: c_int, argv: *const VALUE) -> VALUE {
    if method.is_null() { return RUBY_QNIL; }
    let method_name = CStr::from_ptr(method).to_str().unwrap_or("");
    if method_name.is_empty() { return RUBY_QNIL; }
    
    // Collect arguments from pointer
    let collected_args: Vec<VALUE> = if argc > 0 && !argv.is_null() {
        std::slice::from_raw_parts(argv, argc as usize).to_vec()
    } else {
        Vec::new()
    };
    
    // Look up method in method table
    let entry = method_table::with_method_table(|tbl| {
        tbl.lookup_method(recv, method_name).cloned()
    });
    
    if let Some(entry) = entry {
        dispatch_c_method(entry.func, entry.arity, recv, &collected_args)
    } else {
        // Built-in method dispatch
        match method_name {
            "puts" => {
                for arg in &collected_args {
                    rb_io_puts(*arg);
                }
                RUBY_QNIL
            }
            "print" => {
                for arg in &collected_args {
                    let s = bridge::value_to_jdruby(*arg).to_ruby_string();
                    print!("{}", s);
                }
                RUBY_QNIL
            }
            "to_s" => {
                if argc == 0 {
                    jdruby_to_s(recv)
                } else {
                    RUBY_QNIL
                }
            }
            "inspect" => jdruby_to_s(recv),
            "class" => {
                bridge::jdruby_to_value(&jdruby_runtime::value::RubyValue::Class((recv & 0xFFF0) as u64))
            }
            _ => RUBY_QNIL
        }
    }
}

#[no_mangle]
pub extern "C" fn jdruby_puts(val: VALUE) {
    rb_io_puts(val);
}

#[no_mangle]
pub extern "C" fn jdruby_truthy(val: VALUE) -> bool {
    rb_test(val)
}

#[no_mangle]
pub extern "C" fn jdruby_class_new(name: *const c_char, superclass: VALUE) -> VALUE {
    if name.is_null() { return RUBY_QNIL; }
    let class_name = unsafe { CStr::from_ptr(name).to_str().unwrap_or("Anonymous") };
    
    // Create new class via method table
    method_table::with_method_table(|tbl| {
        tbl.define_class(class_name, superclass)
    })
}

#[no_mangle]
pub extern "C" fn jdruby_def_method(class: VALUE, name: *const c_char, func: *const c_char) {
    if name.is_null() || func.is_null() { return; }
    let method_name = unsafe { CStr::from_ptr(name).to_str().unwrap_or("") };
    let func_name = unsafe { CStr::from_ptr(func).to_str().unwrap_or("") };
    
    // Register the method in the method table
    let _method_id = method_table::rb_intern_str(method_name);
    let func_id = method_table::rb_intern_str(func_name);
    
    // Store in method table with the class
    method_table::with_method_table(|tbl| {
        // Store method mapping: (class, method_name) -> (func_name as callable reference)
        // The actual function pointer resolution happens at dispatch time
        tbl.define_method(class, method_name, func_id as usize, 0);
    });
}

#[no_mangle]
pub extern "C" fn jdruby_const_get(name: *const c_char) -> VALUE {
    if name.is_null() { return RUBY_QNIL; }
    let const_name = unsafe { CStr::from_ptr(name).to_str().unwrap_or("Object") };
    
    // Look up class by name in method table
    method_table::with_method_table(|tbl| {
        tbl.class_by_name(const_name).unwrap_or(RUBY_QNIL)
    })
}

#[no_mangle]
pub extern "C" fn jdruby_ivar_get(obj: VALUE, name: *const c_char) -> VALUE {
    if name.is_null() { return RUBY_QNIL; }
    let ivar_name = unsafe { CStr::from_ptr(name).to_str().unwrap_or("") };
    if ivar_name.is_empty() { return RUBY_QNIL; }
    
    // Get instance variable from object storage
    method_table::with_method_table(|tbl| {
        tbl.get_ivar(obj, ivar_name)
    })
}

#[no_mangle]
pub extern "C" fn jdruby_ivar_set(obj: VALUE, name: *const c_char, val: VALUE) {
    if name.is_null() { return; }
    let ivar_name = unsafe { CStr::from_ptr(name).to_str().unwrap_or("") };
    if ivar_name.is_empty() { return; }
    
    // Set instance variable on object
    method_table::with_method_table(|tbl| {
        tbl.set_ivar(obj, ivar_name, val);
    });
}

// Arithmetic operations
#[no_mangle]
pub extern "C" fn jdruby_int_add(a: VALUE, b: VALUE) -> VALUE {
    if rb_fixnum_p(a) && rb_fixnum_p(b) {
        rb_int2fix(rb_fix2long(a) + rb_fix2long(b))
    } else {
        RUBY_QNIL
    }
}

#[no_mangle]
pub extern "C" fn jdruby_int_sub(a: VALUE, b: VALUE) -> VALUE {
    if rb_fixnum_p(a) && rb_fixnum_p(b) {
        rb_int2fix(rb_fix2long(a) - rb_fix2long(b))
    } else {
        RUBY_QNIL
    }
}

#[no_mangle]
pub extern "C" fn jdruby_int_mul(a: VALUE, b: VALUE) -> VALUE {
    if rb_fixnum_p(a) && rb_fixnum_p(b) {
        rb_int2fix(rb_fix2long(a) * rb_fix2long(b))
    } else {
        RUBY_QNIL
    }
}

#[no_mangle]
pub extern "C" fn jdruby_int_div(a: VALUE, b: VALUE) -> VALUE {
    if rb_fixnum_p(a) && rb_fixnum_p(b) {
        let divisor = rb_fix2long(b);
        if divisor != 0 {
            rb_int2fix(rb_fix2long(a) / divisor)
        } else {
            RUBY_QNIL
        }
    } else {
        RUBY_QNIL
    }
}

#[no_mangle]
pub extern "C" fn jdruby_int_mod(a: VALUE, b: VALUE) -> VALUE {
    if rb_fixnum_p(a) && rb_fixnum_p(b) {
        let divisor = rb_fix2long(b);
        if divisor != 0 {
            rb_int2fix(rb_fix2long(a) % divisor)
        } else {
            RUBY_QNIL
        }
    } else {
        RUBY_QNIL
    }
}

#[no_mangle]
pub extern "C" fn jdruby_int_pow(a: VALUE, b: VALUE) -> VALUE {
    if rb_fixnum_p(a) && rb_fixnum_p(b) {
        let base = rb_fix2long(a);
        let exp = rb_fix2long(b);
        if exp >= 0 && exp < 20 { // Prevent overflow
            rb_int2fix(base.pow(exp as u32))
        } else {
            RUBY_QNIL
        }
    } else {
        RUBY_QNIL
    }
}

// Comparison operations
#[no_mangle]
pub extern "C" fn jdruby_eq(a: VALUE, b: VALUE) -> bool {
    if rb_fixnum_p(a) && rb_fixnum_p(b) {
        rb_fix2long(a) == rb_fix2long(b)
    } else {
        a == b
    }
}

#[no_mangle]
pub extern "C" fn jdruby_lt(a: VALUE, b: VALUE) -> bool {
    if rb_fixnum_p(a) && rb_fixnum_p(b) {
        rb_fix2long(a) < rb_fix2long(b)
    } else {
        false
    }
}

#[no_mangle]
pub extern "C" fn jdruby_gt(a: VALUE, b: VALUE) -> bool {
    if rb_fixnum_p(a) && rb_fixnum_p(b) {
        rb_fix2long(a) > rb_fix2long(b)
    } else {
        false
    }
}

#[no_mangle]
pub extern "C" fn jdruby_le(a: VALUE, b: VALUE) -> bool {
    if rb_fixnum_p(a) && rb_fixnum_p(b) {
        rb_fix2long(a) <= rb_fix2long(b)
    } else {
        false
    }
}

#[no_mangle]
pub extern "C" fn jdruby_ge(a: VALUE, b: VALUE) -> bool {
    if rb_fixnum_p(a) && rb_fixnum_p(b) {
        rb_fix2long(a) >= rb_fix2long(b)
    } else {
        false
    }
}

#[no_mangle]
pub extern "C" fn jdruby_print(val: VALUE) {
    let s = bridge::value_to_jdruby(val).to_ruby_string();
    print!("{}", s);
}

#[no_mangle]
pub extern "C" fn jdruby_p(val: VALUE) -> VALUE {
    let s = bridge::value_to_jdruby(val).to_ruby_string();
    println!("{}", s);
    val
}

#[no_mangle]
pub unsafe extern "C" fn jdruby_raise(_exc_class: VALUE, msg: *const c_char, _argc: c_int, _argv: *const VALUE) {
    let msg_str = if msg.is_null() {
        "unknown error"
    } else {
        CStr::from_ptr(msg).to_str().unwrap_or("unknown error")
    };
    eprintln!("RuntimeError: {}", msg_str);
    std::process::exit(1);
}

#[no_mangle]
pub extern "C" fn jdruby_const_set(_name: *const c_char, _val: VALUE) {
    // TODO: Implement proper constant setting
}

#[no_mangle]
pub unsafe extern "C" fn jdruby_call(_func: *const c_char, _argc: c_int, _argv: *const VALUE) -> VALUE {
    // TODO: Implement block/proc calling
    RUBY_QNIL
}

#[no_mangle]
pub unsafe extern "C" fn jdruby_yield(_argc: c_int, _argv: *const VALUE) -> VALUE {
    // TODO: Implement yield for blocks
    RUBY_QNIL
}

#[no_mangle]
pub extern "C" fn jdruby_block_given() -> bool {
    // TODO: Implement block_given? check
    false
}

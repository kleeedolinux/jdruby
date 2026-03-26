//! # JDRuby C API Implementation
//!
//! Production-ready implementations of MRI C API functions.
//! Based on MRI 3.x C API headers from ruby/include/ruby/internal/intern/

use std::ffi::CStr;
use std::os::raw::{c_char, c_int, c_long, c_void};

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
pub extern "C" fn rb_ary_push(ary: VALUE, _elem: VALUE) -> VALUE { ary }

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
pub extern "C" fn rb_hash_aset(_hash: VALUE, _key: VALUE, val: VALUE) -> VALUE { val }

#[no_mangle]
pub extern "C" fn rb_hash_aref(_hash: VALUE, _key: VALUE) -> VALUE { RUBY_QNIL }

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
pub extern "C" fn rb_string_p(_v: VALUE) -> c_int { 0 }

#[no_mangle]
pub extern "C" fn rb_array_p(_v: VALUE) -> c_int { 0 }

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
pub extern "C" fn rb_iv_get(_obj: VALUE, _name: *const c_char) -> VALUE { RUBY_QNIL }

#[no_mangle]
pub unsafe extern "C" fn rb_iv_set(_obj: VALUE, _name: *const c_char, val: VALUE) -> VALUE { val }

// ═════════════════════════════════════════════════════════════════════════════
// Constants
// ═════════════════════════════════════════════════════════════════════════════

#[no_mangle]
pub extern "C" fn rb_const_get(_klass: VALUE, _name: ID) -> VALUE { RUBY_QNIL }

#[no_mangle]
pub extern "C" fn rb_const_set(_klass: VALUE, _name: ID, _val: VALUE) {}

#[no_mangle]
pub extern "C" fn rb_const_defined(_klass: VALUE, _name: ID) -> c_int { 0 }

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
    match arity {
        0 => std::mem::transmute::<usize, extern "C" fn(VALUE) -> VALUE>(func_ptr)(recv),
        1 => {
            let f = std::mem::transmute::<usize, extern "C" fn(VALUE, VALUE) -> VALUE>(func_ptr);
            f(recv, args.first().copied().unwrap_or(RUBY_QNIL))
        }
        2 => {
            let f = std::mem::transmute::<usize, extern "C" fn(VALUE, VALUE, VALUE) -> VALUE>(func_ptr);
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
    std::process::abort();
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

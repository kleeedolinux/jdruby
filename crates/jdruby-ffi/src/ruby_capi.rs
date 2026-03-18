//! # Ruby C-API Shim
//!
//! `#[no_mangle] extern "C"` implementations of the MRI C-API functions.
//! These are the functions that C-extensions call. When a native Gem's
//! `.so` file calls `rb_define_method`, it goes through here.
//!
//! ## Safety
//!
//! All functions in this module are `unsafe` because they operate on
//! raw C pointers and call through function pointers from C-extensions.

use std::ffi::CStr;
use std::os::raw::{c_char, c_int, c_long};
use crate::value::*;
use crate::method_table;
use crate::bridge;

// ══════════════════════════════════════════════════════════
// ── Special Constants ────────────────────────────────────
// ══════════════════════════════════════════════════════════

/// MRI's `Qnil`.
#[no_mangle]
pub static Qnil: VALUE = RUBY_QNIL;

/// MRI's `Qtrue`.
#[no_mangle]
pub static Qtrue: VALUE = RUBY_QTRUE;

/// MRI's `Qfalse`.
#[no_mangle]
pub static Qfalse: VALUE = RUBY_QFALSE;

/// MRI's `Qundef`.
#[no_mangle]
pub static Qundef: VALUE = RUBY_QUNDEF;

// ══════════════════════════════════════════════════════════
// ── Symbol Interning ─────────────────────────────────────
// ══════════════════════════════════════════════════════════

/// Intern a C string as a Ruby symbol ID.
///
/// C-extension contract: `ID rb_intern(const char *name)`
#[no_mangle]
pub unsafe extern "C" fn rb_intern(name: *const c_char) -> ID {
    let cstr = CStr::from_ptr(name);
    let name_str = cstr.to_str().unwrap_or("");
    method_table::rb_intern_str(name_str)
}

/// Get the name of a symbol ID as a C string.
///
/// C-extension contract: `const char *rb_id2name(ID id)`
#[no_mangle]
pub unsafe extern "C" fn rb_id2name(id: ID) -> *const c_char {
    // In production, this would return a pointer to the interned string.
    // For now, return a static empty string for safety.
    static EMPTY: &[u8] = b"\0";
    EMPTY.as_ptr() as *const c_char
}

/// Convert a symbol VALUE to its ID.
#[no_mangle]
pub extern "C" fn rb_sym2id_api(sym: VALUE) -> ID {
    rb_sym2id(sym)
}

/// Convert an ID to a symbol VALUE.
#[no_mangle]
pub extern "C" fn rb_id2sym_api(id: ID) -> VALUE {
    rb_id2sym(id)
}

// ══════════════════════════════════════════════════════════
// ── Value Construction ───────────────────────────────────
// ══════════════════════════════════════════════════════════

/// Create a tagged Fixnum from a C long.
#[no_mangle]
pub extern "C" fn rb_int_new(val: c_long) -> VALUE {
    rb_int2fix(val as i64)
}

/// Extract a C long from a Fixnum VALUE.
#[no_mangle]
pub extern "C" fn rb_num2long(val: VALUE) -> c_long {
    if rb_fixnum_p(val) {
        rb_fix2long(val) as c_long
    } else {
        0
    }
}

/// Create a new Ruby string from a C string + length.
#[no_mangle]
pub unsafe extern "C" fn rb_str_new(ptr: *const c_char, len: c_long) -> VALUE {
    let slice = std::slice::from_raw_parts(ptr as *const u8, len as usize);
    let s = String::from_utf8_lossy(slice);
    bridge::str_to_value(&s)
}

/// Create a new Ruby string from a null-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn rb_str_new_cstr(ptr: *const c_char) -> VALUE {
    let cstr = CStr::from_ptr(ptr);
    let s = cstr.to_str().unwrap_or("");
    bridge::str_to_value(s)
}

/// Get the length of a Ruby string.
#[no_mangle]
pub extern "C" fn rb_str_strlen(str_val: VALUE) -> c_long {
    bridge::value_to_str(str_val)
        .map(|s| s.len() as c_long)
        .unwrap_or(0)
}

/// Create a new empty Ruby array.
#[no_mangle]
pub extern "C" fn rb_ary_new() -> VALUE {
    let rv = jdruby_runtime::value::RubyValue::Array(Vec::new());
    bridge::jdruby_to_value(&rv)
}

/// Create a new Ruby array with initial capacity.
#[no_mangle]
pub extern "C" fn rb_ary_new_capa(capa: c_long) -> VALUE {
    let rv = jdruby_runtime::value::RubyValue::Array(Vec::with_capacity(capa as usize));
    bridge::jdruby_to_value(&rv)
}

/// Push an element onto a Ruby array.
#[no_mangle]
pub extern "C" fn rb_ary_push(ary: VALUE, val: VALUE) -> VALUE {
    // In production, we'd mutate the array in the arena.
    // For now, return the original array.
    ary
}

/// Get array length.
#[no_mangle]
pub extern "C" fn rb_ary_len(ary: VALUE) -> c_long {
    bridge::value_ary_len(ary)
        .map(|n| n as c_long)
        .unwrap_or(0)
}

// ══════════════════════════════════════════════════════════
// ── Method Definition ────────────────────────────────────
// ══════════════════════════════════════════════════════════

/// Define a method on a class.
///
/// This is the core C-API function that C-extensions use to register methods.
///
/// C-extension contract:
/// ```c
/// void rb_define_method(VALUE klass, const char *name,
///                       VALUE (*func)(ANYARGS), int argc);
/// ```
///
/// # Safety
///
/// `name` must be a valid null-terminated C string.
/// `func` must be a valid function pointer with the correct arity.
#[no_mangle]
pub unsafe extern "C" fn rb_define_method(
    klass: VALUE,
    name: *const c_char,
    func: usize, // function pointer as usize
    arity: c_int,
) {
    let cstr = CStr::from_ptr(name);
    let name_str = cstr.to_str().unwrap_or("");
    method_table::with_method_table(|tbl| {
        tbl.define_method(klass, name_str, func, arity as i32);
    });
}

/// Define a singleton (class-level) method.
#[no_mangle]
pub unsafe extern "C" fn rb_define_singleton_method(
    obj: VALUE,
    name: *const c_char,
    func: usize,
    arity: c_int,
) {
    // For simplicity, treat same as define_method on the object's metaclass
    rb_define_method(obj, name, func, arity);
}

/// Define a module function (both as instance method and module function).
#[no_mangle]
pub unsafe extern "C" fn rb_define_module_function(
    module: VALUE,
    name: *const c_char,
    func: usize,
    arity: c_int,
) {
    rb_define_method(module, name, func, arity);
}

/// Define a global function (available everywhere).
#[no_mangle]
pub unsafe extern "C" fn rb_define_global_function(
    name: *const c_char,
    func: usize,
    arity: c_int,
) {
    // Define on the "kernel" pseudo-class (VALUE 0 = global)
    rb_define_method(0, name, func, arity);
}

// ══════════════════════════════════════════════════════════
// ── Method Dispatch ──────────────────────────────────────
// ══════════════════════════════════════════════════════════

/// Call a method on a receiver with the given arguments.
///
/// C-extension contract:
/// ```c
/// VALUE rb_funcall(VALUE recv, ID mid, int argc, ...);
/// ```
///
/// # Safety
///
/// The variadic arguments must be valid VALUES matching `argc`.
#[no_mangle]
pub unsafe extern "C" fn rb_funcallv(
    recv: VALUE,
    mid: ID,
    argc: c_int,
    argv: *const VALUE,
) -> VALUE {
    let args = if argc > 0 && !argv.is_null() {
        std::slice::from_raw_parts(argv, argc as usize)
    } else {
        &[]
    };

    // Look up the method name
    let method_name = method_table::rb_id2name_str(mid)
        .unwrap_or_default();

    // Look up in method table
    let entry = method_table::with_method_table(|tbl| {
        tbl.lookup_method(recv, &method_name).cloned()
    });

    if let Some(entry) = entry {
        // Dispatch through the C function pointer based on arity
        dispatch_c_method(entry.func, entry.arity, recv, args)
    } else {
        // Method not found — in production, call `method_missing`
        RUBY_QNIL
    }
}

/// Low-level dispatch through a C function pointer.
///
/// This is the hot path for C-extension method calls. We reconstruct
/// the correct calling convention based on the registered arity.
unsafe fn dispatch_c_method(
    func_ptr: usize,
    arity: i32,
    recv: VALUE,
    args: &[VALUE],
) -> VALUE {
    match arity {
        0 => {
            let f: extern "C" fn(VALUE) -> VALUE = std::mem::transmute(func_ptr);
            f(recv)
        }
        1 => {
            let f: extern "C" fn(VALUE, VALUE) -> VALUE = std::mem::transmute(func_ptr);
            let a1 = args.first().copied().unwrap_or(RUBY_QNIL);
            f(recv, a1)
        }
        2 => {
            let f: extern "C" fn(VALUE, VALUE, VALUE) -> VALUE = std::mem::transmute(func_ptr);
            let a1 = args.first().copied().unwrap_or(RUBY_QNIL);
            let a2 = args.get(1).copied().unwrap_or(RUBY_QNIL);
            f(recv, a1, a2)
        }
        3 => {
            let f: extern "C" fn(VALUE, VALUE, VALUE, VALUE) -> VALUE = std::mem::transmute(func_ptr);
            let a1 = args.first().copied().unwrap_or(RUBY_QNIL);
            let a2 = args.get(1).copied().unwrap_or(RUBY_QNIL);
            let a3 = args.get(2).copied().unwrap_or(RUBY_QNIL);
            f(recv, a1, a2, a3)
        }
        -1 => {
            // Variadic: func(int argc, VALUE *argv, VALUE self)
            let f: extern "C" fn(c_int, *const VALUE, VALUE) -> VALUE =
                std::mem::transmute(func_ptr);
            f(args.len() as c_int, args.as_ptr(), recv)
        }
        -2 => {
            // Array-style: func(VALUE self, VALUE args_array)
            let f: extern "C" fn(VALUE, VALUE) -> VALUE = std::mem::transmute(func_ptr);
            let ary = rb_ary_new(); // TODO: populate the array
            f(recv, ary)
        }
        _ => {
            // Unsupported arity, return nil
            RUBY_QNIL
        }
    }
}

// ══════════════════════════════════════════════════════════
// ── Class & Module Definition ────────────────────────────
// ══════════════════════════════════════════════════════════

/// Define a new class under Object.
#[no_mangle]
pub unsafe extern "C" fn rb_define_class(
    name: *const c_char,
    super_klass: VALUE,
) -> VALUE {
    let cstr = CStr::from_ptr(name);
    let name_str = cstr.to_str().unwrap_or("Object");
    method_table::with_method_table(|tbl| {
        tbl.define_class(name_str, super_klass)
    })
}

/// Define a new module.
#[no_mangle]
pub unsafe extern "C" fn rb_define_module(name: *const c_char) -> VALUE {
    let cstr = CStr::from_ptr(name);
    let name_str = cstr.to_str().unwrap_or("Module");
    method_table::with_method_table(|tbl| {
        tbl.define_class(name_str, 0) // modules have no superclass
    })
}

// ══════════════════════════════════════════════════════════
// ── Type Checking ────────────────────────────────────────
// ══════════════════════════════════════════════════════════

/// Check if a VALUE is nil.
#[no_mangle]
pub extern "C" fn rb_nil_p_api(v: VALUE) -> c_int {
    if rb_nil_p(v) { 1 } else { 0 }
}

/// Test truthiness (everything except false and nil).
#[no_mangle]
pub extern "C" fn rb_test_api(v: VALUE) -> c_int {
    if rb_test(v) { 1 } else { 0 }
}

/// Check if a VALUE is a Fixnum.
#[no_mangle]
pub extern "C" fn rb_fixnum_p_api(v: VALUE) -> c_int {
    if rb_fixnum_p(v) { 1 } else { 0 }
}

// ══════════════════════════════════════════════════════════
// ── I/O ──────────────────────────────────────────────────
// ══════════════════════════════════════════════════════════

/// Ruby `puts` — print value + newline.
#[no_mangle]
pub extern "C" fn rb_io_puts(val: VALUE) {
    let rv = bridge::value_to_jdruby(val);
    println!("{}", rv.to_ruby_string());
}

/// Ruby `p` — print inspected value.
#[no_mangle]
pub extern "C" fn rb_p_api(val: VALUE) -> VALUE {
    let rv = bridge::value_to_jdruby(val);
    println!("{}", rv.inspect());
    val
}

/// Ruby `raise` — raise an exception (simplified).
#[no_mangle]
pub unsafe extern "C" fn rb_raise_api(
    _exc_class: VALUE,
    msg: *const c_char,
) {
    let cstr = CStr::from_ptr(msg);
    let msg_str = cstr.to_str().unwrap_or("unknown error");
    eprintln!("RuntimeError: {}", msg_str);
    std::process::abort();
}

// ══════════════════════════════════════════════════════════
// ── Instance Variables ───────────────────────────────────
// ══════════════════════════════════════════════════════════

/// Get an instance variable.
#[no_mangle]
pub extern "C" fn rb_ivar_get(obj: VALUE, id: ID) -> VALUE {
    // Simplified: return nil for now
    RUBY_QNIL
}

/// Set an instance variable.
#[no_mangle]
pub extern "C" fn rb_ivar_set(obj: VALUE, id: ID, val: VALUE) -> VALUE {
    // Simplified: no-op, return val
    val
}

// ══════════════════════════════════════════════════════════
// ── Hash Operations ──────────────────────────────────────
// ══════════════════════════════════════════════════════════

/// Create a new empty hash.
#[no_mangle]
pub extern "C" fn rb_hash_new() -> VALUE {
    let rv = jdruby_runtime::value::RubyValue::Hash(
        jdruby_runtime::value::RubyHash::new()
    );
    bridge::jdruby_to_value(&rv)
}

// ══════════════════════════════════════════════════════════
// ── Conversion Functions ─────────────────────────────────
// ══════════════════════════════════════════════════════════

/// Convert a VALUE to a Ruby string (calls #to_s).
#[no_mangle]
pub extern "C" fn rb_obj_as_string(val: VALUE) -> VALUE {
    let rv = bridge::value_to_jdruby(val);
    let s = rv.to_ruby_string();
    bridge::str_to_value(&s)
}

/// Get the class name of an object.
#[no_mangle]
pub extern "C" fn rb_obj_classname_api(obj: VALUE) -> *const c_char {
    static EMPTY: &[u8] = b"Object\0";
    EMPTY.as_ptr() as *const c_char
}

// ══════════════════════════════════════════════════════════
// ── Runtime entry points (for compiled code) ─────────────
// ══════════════════════════════════════════════════════════

/// These are the functions that our LLVM IR codegen calls.
/// They bridge from the compiled native code to the runtime.

#[no_mangle]
pub extern "C" fn jdruby_int_new(val: i64) -> VALUE {
    rb_int2fix(val)
}

#[no_mangle]
pub extern "C" fn jdruby_float_new(val: f64) -> VALUE {
    let rv = jdruby_runtime::value::RubyValue::Float(val);
    bridge::jdruby_to_value(&rv)
}

#[no_mangle]
pub unsafe extern "C" fn jdruby_str_new(ptr: *const c_char, len: i64) -> VALUE {
    rb_str_new(ptr, len as c_long)
}

#[no_mangle]
pub unsafe extern "C" fn jdruby_sym_intern(ptr: *const c_char) -> VALUE {
    let id = rb_intern(ptr);
    rb_id2sym(id)
}

#[no_mangle]
pub extern "C" fn jdruby_bool(val: bool) -> VALUE {
    if val { RUBY_QTRUE } else { RUBY_QFALSE }
}

#[no_mangle]
pub extern "C" fn jdruby_puts(val: VALUE) {
    rb_io_puts(val);
}

#[no_mangle]
pub extern "C" fn jdruby_print(val: VALUE) {
    let rv = bridge::value_to_jdruby(val);
    print!("{}", rv.to_ruby_string());
}

#[no_mangle]
pub extern "C" fn jdruby_p(val: VALUE) -> VALUE {
    rb_p_api(val)
}

#[no_mangle]
pub extern "C" fn jdruby_truthy(val: VALUE) -> bool {
    rb_test(val)
}

#[no_mangle]
pub extern "C" fn jdruby_eq(a: VALUE, b: VALUE) -> bool {
    a == b
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
        let bv = rb_fix2long(b);
        if bv != 0 {
            rb_int2fix(rb_fix2long(a) / bv)
        } else {
            RUBY_QNIL // ZeroDivisionError
        }
    } else {
        RUBY_QNIL
    }
}

#[no_mangle]
pub extern "C" fn jdruby_int_mod(a: VALUE, b: VALUE) -> VALUE {
    if rb_fixnum_p(a) && rb_fixnum_p(b) {
        let bv = rb_fix2long(b);
        if bv != 0 {
            rb_int2fix(rb_fix2long(a) % bv)
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
        let bv = rb_fix2long(b);
        if bv >= 0 {
            rb_int2fix(rb_fix2long(a).pow(bv as u32))
        } else {
            RUBY_QNIL // Would return Float in Ruby
        }
    } else {
        RUBY_QNIL
    }
}

/// Dynamic method dispatch from compiled code.
#[no_mangle]
pub unsafe extern "C" fn jdruby_send(
    recv: VALUE,
    method_name: *const c_char,
    argc: c_int,
    // ... variadic VALUE args follow
) -> VALUE {
    // For safety, we can't portably extract variadic args in Rust.
    // The compiled code should use jdruby_send with a fixed signature.
    // This is a fallback.
    RUBY_QNIL
}

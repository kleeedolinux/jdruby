//! # Runtime API — jdruby_* Functions
//!
//! JDRuby-specific runtime entry points.

use std::ffi::CStr;
use std::os::raw::{c_char, c_int};
use crate::core::{VALUE, RUBY_QNIL, RUBY_QTRUE, RUBY_QFALSE, rb_fixnum_p, rb_fix2long, rb_int2fix};
use crate::bridge::conversion::{jdruby_to_value, value_to_jdruby};
use crate::bridge::dedup::str_to_value;
use crate::storage::class_table::with_class_table;
use crate::storage::method_storage::with_method_storage;
use crate::storage::ivar_storage::with_ivar_storage;
use crate::storage::symbol_table::rb_intern_str;
use crate::bridge::registry::init_bridge;
use crate::bridge::allocator::init_allocator;

/// Initialize the bridge (runtime entry point).
#[no_mangle]
pub extern "C" fn jdruby_init_bridge() {
    init_allocator();
    init_bridge();
}

/// Create a new integer.
#[no_mangle]
pub extern "C" fn jdruby_int_new(val: i64) -> VALUE {
    rb_int2fix(val)
}

/// Create a new float.
#[no_mangle]
pub extern "C" fn jdruby_float_new(val: f64) -> VALUE {
    use crate::bridge::conversion::allocate_float;
    allocate_float(val).unwrap_or(RUBY_QNIL)
}

/// Create a new string.
#[no_mangle]
pub unsafe extern "C" fn jdruby_str_new(ptr: *const c_char, len: i64) -> VALUE {
    use crate::capi::string::rb_str_new;
    rb_str_new(ptr, len as i64)
}

/// Intern a symbol.
#[no_mangle]
pub unsafe extern "C" fn jdruby_sym_intern(name: *const c_char) -> VALUE {
    use crate::core::rb_id2sym;
    rb_id2sym(rb_intern_str(CStr::from_ptr(name).to_str().unwrap_or("")))
}

/// Create a new array.
#[no_mangle]
pub extern "C" fn jdruby_ary_new(_len: i32) -> VALUE {
    jdruby_to_value(&jdruby_runtime::value::RubyValue::Array(Vec::new()))
}

/// Create a new hash.
#[no_mangle]
pub extern "C" fn jdruby_hash_new(_len: i32) -> VALUE {
    jdruby_to_value(&jdruby_runtime::value::RubyValue::Hash(jdruby_runtime::value::RubyHash::new()))
}

/// Create a boolean VALUE.
#[no_mangle]
pub extern "C" fn jdruby_bool(val: bool) -> VALUE {
    if val { RUBY_QTRUE } else { RUBY_QFALSE }
}

/// Concatenate two values as strings.
#[no_mangle]
pub extern "C" fn jdruby_str_concat(a: VALUE, b: VALUE) -> VALUE {
    let s1 = value_to_jdruby(a).to_ruby_string();
    let s2 = value_to_jdruby(b).to_ruby_string();
    let concat = format!("{}{}", s1, s2);
    str_to_value(&concat)
}

/// Convert to string.
#[no_mangle]
pub extern "C" fn jdruby_to_s(val: VALUE) -> VALUE {
    let s = value_to_jdruby(val).to_ruby_string();
    str_to_value(&s)
}

/// Send a method call.
#[no_mangle]
pub unsafe extern "C" fn jdruby_send(
    recv: VALUE,
    method: *const c_char,
    argc: c_int,
    argv: *const VALUE,
) -> VALUE {
    if method.is_null() { return RUBY_QNIL; }
    let method_name = CStr::from_ptr(method).to_str().unwrap_or("");
    if method_name.is_empty() { return RUBY_QNIL; }
    
    let collected_args: Vec<VALUE> = if argc > 0 && !argv.is_null() {
        std::slice::from_raw_parts(argv, argc as usize).to_vec()
    } else {
        Vec::new()
    };
    
    // Look up method
    let entry = with_method_storage(|storage| {
        storage.lookup(recv, method_name).cloned()
    });
    
    if let Some(entry) = entry {
        use crate::capi::class::dispatch_c_method;
        dispatch_c_method(entry.func, entry.arity, recv, &collected_args)
    } else {
        // Built-in dispatch
        match method_name {
            "puts" => {
                for arg in &collected_args {
                    use crate::capi::io::rb_io_puts;
                    rb_io_puts(*arg);
                }
                RUBY_QNIL
            }
            "print" => {
                for arg in &collected_args {
                    print!("{}", value_to_jdruby(*arg).to_ruby_string());
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
                jdruby_to_value(&jdruby_runtime::value::RubyValue::Class((recv & 0xFFF0) as u64))
            }
            _ => RUBY_QNIL,
        }
    }
}

/// Print a value.
#[no_mangle]
pub extern "C" fn jdruby_puts(val: VALUE) {
    use crate::capi::io::rb_io_puts;
    rb_io_puts(val);
}

/// Test truthiness.
#[no_mangle]
pub extern "C" fn jdruby_truthy(val: VALUE) -> bool {
    use crate::core::rb_test;
    rb_test(val)
}

/// Create a new class.
#[no_mangle]
pub extern "C" fn jdruby_class_new(name: *const c_char, superclass: VALUE) -> VALUE {
    if name.is_null() { return RUBY_QNIL; }
    let class_name = unsafe { CStr::from_ptr(name).to_str().unwrap_or("Anonymous") };
    with_class_table(|tbl| tbl.define_class(class_name, superclass))
}

/// Define a method.
#[no_mangle]
pub extern "C" fn jdruby_def_method(class: VALUE, name: *const c_char, func: *const c_char) {
    if name.is_null() || func.is_null() { return; }
    let method_name = unsafe { CStr::from_ptr(name).to_str().unwrap_or("") };
    let func_name = unsafe { CStr::from_ptr(func).to_str().unwrap_or("") };
    
    let func_id = rb_intern_str(func_name);
    
    with_method_storage(|storage| {
        storage.define_method(class, method_name, func_id as usize, 0);
    });
}

/// Get a constant by name.
#[no_mangle]
pub extern "C" fn jdruby_const_get(name: *const c_char) -> VALUE {
    if name.is_null() { return RUBY_QNIL; }
    let const_name = unsafe { CStr::from_ptr(name).to_str().unwrap_or("Object") };
    with_class_table(|tbl| tbl.class_by_name(const_name).unwrap_or(RUBY_QNIL))
}

/// Get an instance variable.
#[no_mangle]
pub extern "C" fn jdruby_ivar_get(obj: VALUE, name: *const c_char) -> VALUE {
    if name.is_null() { return RUBY_QNIL; }
    let ivar_name = unsafe { CStr::from_ptr(name).to_str().unwrap_or("") };
    if ivar_name.is_empty() { return RUBY_QNIL; }
    
    with_ivar_storage(|storage| storage.get(obj, ivar_name))
}

/// Set an instance variable.
#[no_mangle]
pub extern "C" fn jdruby_ivar_set(obj: VALUE, name: *const c_char, val: VALUE) {
    if name.is_null() { return; }
    let ivar_name = unsafe { CStr::from_ptr(name).to_str().unwrap_or("") };
    if ivar_name.is_empty() { return; }
    
    with_ivar_storage(|storage| {
        storage.set(obj, ivar_name, val);
    });
}

// ═════════════════════════════════════════════════════════════════════════════
// Arithmetic Operations
// ═════════════════════════════════════════════════════════════════════════════

/// Integer addition.
#[no_mangle]
pub extern "C" fn jdruby_int_add(a: VALUE, b: VALUE) -> VALUE {
    if rb_fixnum_p(a) && rb_fixnum_p(b) {
        rb_int2fix(rb_fix2long(a) + rb_fix2long(b))
    } else {
        RUBY_QNIL
    }
}

/// Integer subtraction.
#[no_mangle]
pub extern "C" fn jdruby_int_sub(a: VALUE, b: VALUE) -> VALUE {
    if rb_fixnum_p(a) && rb_fixnum_p(b) {
        rb_int2fix(rb_fix2long(a) - rb_fix2long(b))
    } else {
        RUBY_QNIL
    }
}

/// Integer multiplication.
#[no_mangle]
pub extern "C" fn jdruby_int_mul(a: VALUE, b: VALUE) -> VALUE {
    if rb_fixnum_p(a) && rb_fixnum_p(b) {
        rb_int2fix(rb_fix2long(a) * rb_fix2long(b))
    } else {
        RUBY_QNIL
    }
}

/// Integer division.
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

/// Integer modulo.
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

/// Integer power.
#[no_mangle]
pub extern "C" fn jdruby_int_pow(a: VALUE, b: VALUE) -> VALUE {
    if rb_fixnum_p(a) && rb_fixnum_p(b) {
        let base = rb_fix2long(a);
        let exp = rb_fix2long(b);
        if exp >= 0 && exp < 20 {
            rb_int2fix(base.pow(exp as u32))
        } else {
            RUBY_QNIL
        }
    } else {
        RUBY_QNIL
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// Comparison Operations
// ═════════════════════════════════════════════════════════════════════════════

/// Equality comparison.
#[no_mangle]
pub extern "C" fn jdruby_eq(a: VALUE, b: VALUE) -> bool {
    if rb_fixnum_p(a) && rb_fixnum_p(b) {
        rb_fix2long(a) == rb_fix2long(b)
    } else {
        a == b
    }
}

/// Less than.
#[no_mangle]
pub extern "C" fn jdruby_lt(a: VALUE, b: VALUE) -> bool {
    if rb_fixnum_p(a) && rb_fixnum_p(b) {
        rb_fix2long(a) < rb_fix2long(b)
    } else {
        false
    }
}

/// Greater than.
#[no_mangle]
pub extern "C" fn jdruby_gt(a: VALUE, b: VALUE) -> bool {
    if rb_fixnum_p(a) && rb_fixnum_p(b) {
        rb_fix2long(a) > rb_fix2long(b)
    } else {
        false
    }
}

/// Less than or equal.
#[no_mangle]
pub extern "C" fn jdruby_le(a: VALUE, b: VALUE) -> bool {
    if rb_fixnum_p(a) && rb_fixnum_p(b) {
        rb_fix2long(a) <= rb_fix2long(b)
    } else {
        false
    }
}

/// Greater than or equal.
#[no_mangle]
pub extern "C" fn jdruby_ge(a: VALUE, b: VALUE) -> bool {
    if rb_fixnum_p(a) && rb_fixnum_p(b) {
        rb_fix2long(a) >= rb_fix2long(b)
    } else {
        false
    }
}

/// Print without newline.
#[no_mangle]
pub extern "C" fn jdruby_print(val: VALUE) {
    print!("{}", value_to_jdruby(val).to_ruby_string());
}

/// Print and return value.
#[no_mangle]
pub extern "C" fn jdruby_p(val: VALUE) -> VALUE {
    let s = value_to_jdruby(val).to_ruby_string();
    println!("{}", s);
    val
}

/// Raise an exception.
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

/// Call a function (placeholder).
#[no_mangle]
pub unsafe extern "C" fn jdruby_call(_func: *const c_char, _argc: c_int, _argv: *const VALUE) -> VALUE {
    RUBY_QNIL
}

/// Yield to block (placeholder).
#[no_mangle]
pub unsafe extern "C" fn jdruby_yield(_argc: c_int, _argv: *const VALUE) -> VALUE {
    RUBY_QNIL
}

/// Check if block given (placeholder).
#[no_mangle]
pub extern "C" fn jdruby_block_given() -> bool {
    false
}

/// Set a constant (placeholder).
#[no_mangle]
pub extern "C" fn jdruby_const_set(_name: *const c_char, _val: VALUE) {
    // TODO: Implement proper constant setting
}

// ═════════════════════════════════════════════════════════════════════════════
// Additional MIR Runtime Functions
// ═════════════════════════════════════════════════════════════════════════════

/// Create a new module (placeholder).
#[no_mangle]
pub extern "C" fn jdruby_module_new(_name: *const c_char) -> VALUE {
    RUBY_QNIL
}

/// Get singleton class (placeholder).
#[no_mangle]
pub extern "C" fn jdruby_singleton_class_get(_obj: VALUE) -> VALUE {
    RUBY_QNIL
}

/// Prepend module (placeholder).
#[no_mangle]
pub extern "C" fn jdruby_prepend_module(_class: VALUE, _module_name: *const c_char) {
}

/// Extend module (placeholder).
#[no_mangle]
pub extern "C" fn jdruby_extend_module(_obj: VALUE, _module_name: *const c_char) {
}

/// Create a block (placeholder).
#[no_mangle]
pub extern "C" fn jdruby_block_create(_func_symbol: *const c_char, _argc: i32, _argv: *const VALUE) -> VALUE {
    RUBY_QNIL
}

/// Create a proc (placeholder).
#[no_mangle]
pub extern "C" fn jdruby_proc_create(_block: VALUE) -> VALUE {
    RUBY_QNIL
}

/// Create a lambda (placeholder).
#[no_mangle]
pub extern "C" fn jdruby_lambda_create(_block: VALUE) -> VALUE {
    RUBY_QNIL
}

/// Yield to block (placeholder).
#[no_mangle]
pub extern "C" fn jdruby_block_yield(_block: VALUE, _argc: i32, _argv: *const VALUE) -> VALUE {
    RUBY_QNIL
}

/// Get current block (placeholder).
#[no_mangle]
pub extern "C" fn jdruby_current_block() -> VALUE {
    RUBY_QNIL
}

/// Define method dynamically (placeholder).
#[no_mangle]
pub extern "C" fn jdruby_define_method_dynamic(_class: VALUE, _name: VALUE, _func: *const c_char, _visibility: i32) -> VALUE {
    RUBY_QNIL
}

/// Undefine method (placeholder).
#[no_mangle]
pub extern "C" fn jdruby_undef_method(_class: VALUE, _name: VALUE) -> VALUE {
    RUBY_QNIL
}

/// Remove method (placeholder).
#[no_mangle]
pub extern "C" fn jdruby_remove_method(_class: VALUE, _name: VALUE) -> VALUE {
    RUBY_QNIL
}

/// Alias method (placeholder).
#[no_mangle]
pub extern "C" fn jdruby_alias_method(_class: VALUE, _new_name: VALUE, _old_name: VALUE) -> VALUE {
    RUBY_QNIL
}

/// Set method visibility (placeholder).
#[no_mangle]
pub extern "C" fn jdruby_set_visibility(_class: VALUE, _visibility: i32, _argc: i32, _argv: *const VALUE) -> VALUE {
    RUBY_QNIL
}

/// Eval code (placeholder).
#[no_mangle]
pub extern "C" fn jdruby_eval(_code: VALUE, _binding: VALUE, _filename: VALUE, _line: VALUE) -> VALUE {
    RUBY_QNIL
}

/// Instance eval (placeholder).
#[no_mangle]
pub extern "C" fn jdruby_instance_eval(_obj: VALUE, _code: VALUE, _binding: VALUE) -> VALUE {
    RUBY_QNIL
}

/// Class eval (placeholder).
#[no_mangle]
pub extern "C" fn jdruby_class_eval(_class: VALUE, _code: VALUE, _binding: VALUE) -> VALUE {
    RUBY_QNIL
}

/// Module eval (placeholder).
#[no_mangle]
pub extern "C" fn jdruby_module_eval(_module: VALUE, _code: VALUE, _binding: VALUE) -> VALUE {
    RUBY_QNIL
}

/// Get binding (placeholder).
#[no_mangle]
pub extern "C" fn jdruby_binding_get() -> VALUE {
    RUBY_QNIL
}

/// Send method dynamically (placeholder).
#[no_mangle]
pub extern "C" fn jdruby_send_dynamic(_obj: VALUE, _name: VALUE, _argc: i32, _argv: *const VALUE, _block: VALUE) -> VALUE {
    RUBY_QNIL
}

/// Public send (placeholder).
#[no_mangle]
pub extern "C" fn jdruby_public_send(_obj: VALUE, _name: VALUE, _argc: i32, _argv: *const VALUE, _block: VALUE) -> VALUE {
    RUBY_QNIL
}

/// Respond to (placeholder).
#[no_mangle]
pub extern "C" fn jdruby_respond_to(_obj: VALUE, _name: VALUE, _include_private: bool) -> VALUE {
    RUBY_QFALSE
}

/// Get method object (placeholder).
#[no_mangle]
pub extern "C" fn jdruby_method_get(_obj: VALUE, _name: VALUE) -> VALUE {
    RUBY_QNIL
}

/// Get instance method (placeholder).
#[no_mangle]
pub extern "C" fn jdruby_instance_method_get(_class: VALUE, _name: VALUE) -> VALUE {
    RUBY_QNIL
}

/// Call method object (placeholder).
#[no_mangle]
pub extern "C" fn jdruby_method_object_call(_method: VALUE, _receiver: VALUE, _argc: i32, _argv: *const VALUE, _block: VALUE) -> VALUE {
    RUBY_QNIL
}

/// Bind method (placeholder).
#[no_mangle]
pub extern "C" fn jdruby_method_bind(_method: VALUE, _obj: VALUE) -> VALUE {
    RUBY_QNIL
}

/// Dynamic ivar get (placeholder).
#[no_mangle]
pub extern "C" fn jdruby_ivar_get_dynamic(_obj: VALUE, _name: VALUE) -> VALUE {
    RUBY_QNIL
}

/// Dynamic ivar set (placeholder).
#[no_mangle]
pub extern "C" fn jdruby_ivar_set_dynamic(_obj: VALUE, _name: VALUE, _value: VALUE) {
}

/// Dynamic cvar get (placeholder).
#[no_mangle]
pub extern "C" fn jdruby_cvar_get_dynamic(_class: VALUE, _name: VALUE) -> VALUE {
    RUBY_QNIL
}

/// Dynamic cvar set (placeholder).
#[no_mangle]
pub extern "C" fn jdruby_cvar_set_dynamic(_class: VALUE, _name: VALUE, _value: VALUE) {
}

/// Dynamic const get (placeholder).
#[no_mangle]
pub extern "C" fn jdruby_const_get_dynamic(_class: VALUE, _name: VALUE, _inherit: bool) -> VALUE {
    RUBY_QNIL
}

/// Dynamic const set (placeholder).
#[no_mangle]
pub extern "C" fn jdruby_const_set_dynamic(_class: VALUE, _name: VALUE, _value: VALUE) {
}

/// Method missing (placeholder).
#[no_mangle]
pub extern "C" fn jdruby_method_missing(_obj: VALUE, _name: VALUE, _argc: i32, _argv: *const VALUE, _block: VALUE) -> VALUE {
    RUBY_QNIL
}

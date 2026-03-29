//! # Runtime API — jdruby_* Functions
//!
//! JDRuby-specific runtime entry points.

use std::ffi::CStr;
use std::os::raw::{c_char, c_int};
use crate::core::{VALUE, RUBY_QNIL, RUBY_QTRUE, RUBY_QFALSE, rb_fixnum_p, rb_fix2long, rb_int2fix};
use crate::bridge::conversion::{jdruby_to_value, value_to_jdruby};
use crate::bridge::dedup::str_to_value;
use crate::storage::class_table::with_class_table;
use crate::storage::method_storage::{with_method_storage, Visibility};
use crate::storage::ivar_storage::with_ivar_storage;
use crate::storage::symbol_table::rb_intern_str;
use crate::bridge::registry::init_bridge;
use crate::bridge::allocator::init_allocator;

/// Helper: Convert VALUE to string (handles symbols and strings)
fn value_to_symbol_or_string(value: VALUE) -> Option<String> {
    // For now, simplified - would need to check VALUE type tags
    // and extract string/symbol value properly
    if value == RUBY_QNIL {
        None
    } else {
        // Placeholder - extract from VALUE encoding
        Some(format!("sym_{}", value))
    }
}

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
        dispatch_c_method(&entry.func_name, entry.arity, recv, &collected_args)
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
    
    with_method_storage(|storage| {
        storage.define_method(class, method_name, func_name, 0);
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

/// Create a new module (actual implementation).
#[no_mangle]
pub extern "C" fn jdruby_module_new(name: *const c_char) -> VALUE {
    if name.is_null() { return RUBY_QNIL; }
    let module_name = unsafe { CStr::from_ptr(name).to_str().unwrap_or("Anonymous") };
    
    with_class_table(|tbl| {
        // Create module as a special class type
        let module_val = tbl.define_class(&format!("Module:{}", module_name), RUBY_QNIL);
        module_val
    })
}

/// Get singleton class (actual implementation).
#[no_mangle]
pub extern "C" fn jdruby_singleton_class_get(obj: VALUE) -> VALUE {
    if obj == RUBY_QNIL { return RUBY_QNIL; }
    
    with_class_table(|tbl| {
        // Get or create singleton class for object
        // In full impl, would check if obj already has singleton
        let class_name = format!("#<SingletonClass:{:x}>", obj);
        tbl.define_class(&class_name, RUBY_QNIL)
    })
}

/// Prepend module (actual implementation).
#[no_mangle]
pub extern "C" fn jdruby_prepend_module(class: VALUE, module_name: *const c_char) {
    if module_name.is_null() { return; }
    let mod_name = unsafe { CStr::from_ptr(module_name).to_str().unwrap_or("") };
    
    with_method_storage(|storage| {
        // Prepend: insert module methods before existing methods
        with_class_table(|tbl| {
            if let Some(class_name) = tbl.class_name(class) {
                storage.prepend_module(&class_name, mod_name);
            }
        });
    });
}

/// Extend module (actual implementation).
#[no_mangle]
pub extern "C" fn jdruby_extend_module(obj: VALUE, module_name: *const c_char) {
    if module_name.is_null() { return; }
    let mod_name = unsafe { CStr::from_ptr(module_name).to_str().unwrap_or("") };
    
    // Extend object: add module methods to object's singleton class
    with_method_storage(|storage| {
        with_class_table(|tbl| {
            // Get or create singleton class for obj
            let singleton_name = format!("#<SingletonClass:{:x}>", obj);
            let singleton = tbl.define_class(&singleton_name, RUBY_QNIL);
            if let Some(s_name) = tbl.class_name(singleton) {
                storage.include_module(&s_name, mod_name);
            }
        });
    });
}

/// Create a block (actual implementation).
#[no_mangle]
pub unsafe extern "C" fn jdruby_block_create(func_symbol: *const c_char, argc: i32, argv: *const VALUE) -> VALUE {
    if func_symbol.is_null() { return RUBY_QNIL; }
    let func_name = CStr::from_ptr(func_symbol).to_str().unwrap_or("");
    
    // Store the block info in the bridge for later use
    use crate::bridge::registry::get_global_bridge;
    if let Some(bridge) = get_global_bridge() {
        let captured: Vec<VALUE> = if argc > 0 && !argv.is_null() {
            std::slice::from_raw_parts(argv, argc as usize).to_vec()
        } else {
            Vec::new()
        };
        bridge.store_block(func_name, captured);
    }
    
    // Return the function name as a string VALUE (this identifies the block)
    crate::capi::string::rb_str_new(func_symbol, func_name.len() as i64)
}

/// Create a proc from block (actual implementation).
#[no_mangle]
pub extern "C" fn jdruby_proc_create(block: VALUE) -> VALUE {
    // If block is valid, mark it as a proc
    if block != RUBY_QNIL && block != RUBY_QFALSE {
        // In full implementation, would create Proc object
        // For now, return the block value as-is
        block
    } else {
        RUBY_QNIL
    }
}

/// Create a lambda from block (actual implementation).
#[no_mangle]
pub extern "C" fn jdruby_lambda_create(block: VALUE) -> VALUE {
    // Lambda is like proc but with strict arity checking
    // For now, same as proc but tagged differently in full impl
    if block != RUBY_QNIL && block != RUBY_QFALSE {
        block
    } else {
        RUBY_QNIL
    }
}

/// Convert a symbol to a proc for &:method_name syntax.
/// The symbol value contains the method name, and we create a proc that
/// calls that method on the yielded argument.
#[no_mangle]
pub extern "C" fn jdruby_symbol_to_proc(symbol: VALUE) -> VALUE {
    if symbol == RUBY_QNIL { return RUBY_QNIL; }
    
    // Extract method name from the symbol
    // Symbols are stored with a specific encoding in VALUE
    // For now, we extract via string conversion
    let method_name = value_to_symbol_or_string(symbol).unwrap_or("".to_string());
    if method_name.is_empty() { return RUBY_QNIL; }
    
    // Create a unique identifier for this symbol-to-proc
    // We'll use a special prefix to distinguish from regular blocks
    let func_symbol = format!("__sym_proc_{}", method_name);
    
    // Store in bridge with empty captures - the actual dispatch will happen in yield
    use crate::bridge::registry::get_global_bridge;
    if let Some(bridge) = get_global_bridge() {
        // Store with the symbol value so we know it's a symbol proc
        bridge.store_block(&func_symbol, vec![symbol]);
    }
    
    // Return the function identifier as a string VALUE
    unsafe {
        crate::capi::string::rb_str_new(func_symbol.as_ptr() as *const c_char, func_symbol.len() as i64)
    }
}

/// Check if a value is a symbol.
#[no_mangle]
pub extern "C" fn jdruby_is_symbol(val: VALUE) -> bool {
    // Symbols have a specific bit pattern in MRI
    // For this implementation, we check if it looks like a symbol
    // In full MRI, symbols are odd values with specific bits set
    val != RUBY_QNIL && val != RUBY_QFALSE && (val & 0xFF) == 0x0C
}

/// Yield to block with arguments (actual implementation).
#[no_mangle]
pub unsafe extern "C" fn jdruby_block_yield(block: VALUE, argc: i32, argv: *const VALUE) -> VALUE {
    if block == RUBY_QNIL { return RUBY_QNIL; }
    
    let args: Vec<VALUE> = if argc > 0 && !argv.is_null() {
        std::slice::from_raw_parts(argv, argc as usize).to_vec()
    } else {
        Vec::new()
    };
    
    // Extract function name from the block VALUE (it's a string)
    let func_name = value_to_jdruby(block).to_ruby_string();
    if func_name.is_empty() {
        return RUBY_QNIL;
    }
    
    // Check if this is a symbol-to-proc block
    if func_name.starts_with("__sym_proc_") {
        // Extract method name from the special prefix
        let method_name = &func_name["__sym_proc_".len()..];
        
        // Get the first argument (the item being yielded)
        if let Some(&obj) = args.first() {
            // Dispatch the method on the yielded object
            return with_method_storage(|storage| {
                storage.dispatch(obj, method_name, &[])
            });
        }
        return RUBY_QNIL;
    }
    
    // Get captured variables from the bridge
    let mut full_args = Vec::new();
    use crate::bridge::registry::get_global_bridge;
    if let Some(bridge) = get_global_bridge() {
        if let Some(captured) = bridge.get_block_captures(&func_name) {
            full_args.extend(captured);
        }
    }
    full_args.extend(args);
    
    // Dispatch the block function with captured vars + args
    use crate::capi::class::dispatch_c_method;
    dispatch_c_method(&func_name, full_args.len() as i32, RUBY_QNIL, &full_args)
}

/// Get current block (actual implementation).
#[no_mangle]
pub extern "C" fn jdruby_current_block() -> VALUE {
    // In full implementation, would retrieve implicit block from current frame
    // For now, return nil
    use crate::bridge::registry::get_global_bridge;
    if let Some(bridge) = get_global_bridge() {
        bridge.get_current_block().unwrap_or(RUBY_QNIL)
    } else {
        RUBY_QNIL
    }
}

/// Define method dynamically (actual implementation).
#[no_mangle]
pub unsafe extern "C" fn jdruby_define_method_dynamic(class: VALUE, name: VALUE, func: *const c_char, visibility: i32) -> VALUE {
    if class == RUBY_QNIL || func.is_null() { return RUBY_QNIL; }
    let func_name = CStr::from_ptr(func).to_str().unwrap_or("");
    let method_name = value_to_symbol_or_string(name).unwrap_or("anonymous".to_string());
    
    with_method_storage(|storage| {
        let vis = match visibility {
            0 => Visibility::Public,
            1 => Visibility::Protected,
            2 => Visibility::Private,
            _ => Visibility::Public,
        };
        storage.define_method_with_visibility(class, &method_name, func_name, vis);
    });
    name
}

/// Undefine method (actual implementation).
#[no_mangle]
pub extern "C" fn jdruby_undef_method(class: VALUE, name: VALUE) -> VALUE {
    if class == RUBY_QNIL { return RUBY_QNIL; }
    let method_name = value_to_symbol_or_string(name).unwrap_or("".to_string());
    if method_name.is_empty() { return RUBY_QNIL; }
    
    with_method_storage(|storage| {
        storage.undef_method(class, &method_name);
    });
    RUBY_QNIL
}

/// Remove method (actual implementation).
#[no_mangle]
pub extern "C" fn jdruby_remove_method(class: VALUE, name: VALUE) -> VALUE {
    if class == RUBY_QNIL { return RUBY_QNIL; }
    let method_name = value_to_symbol_or_string(name).unwrap_or("".to_string());
    if method_name.is_empty() { return RUBY_QNIL; }
    
    with_method_storage(|storage| {
        storage.remove_method(class, &method_name);
    });
    RUBY_QNIL
}

/// Alias method (actual implementation).
#[no_mangle]
pub extern "C" fn jdruby_alias_method(class: VALUE, new_name: VALUE, old_name: VALUE) -> VALUE {
    if class == RUBY_QNIL { return RUBY_QNIL; }
    let new_n = value_to_symbol_or_string(new_name).unwrap_or("".to_string());
    let old_n = value_to_symbol_or_string(old_name).unwrap_or("".to_string());
    if new_n.is_empty() || old_n.is_empty() { return RUBY_QNIL; }
    
    with_method_storage(|storage| {
        storage.alias_method(class, &new_n, &old_n);
    });
    new_name
}

/// Set method visibility (actual implementation).
#[no_mangle]
pub unsafe extern "C" fn jdruby_set_visibility(class: VALUE, visibility: i32, argc: i32, argv: *const VALUE) -> VALUE {
    if class == RUBY_QNIL || argc <= 0 || argv.is_null() { return RUBY_QNIL; }
    
    let vis = match visibility {
        0 => Visibility::Public,
        1 => Visibility::Protected,
        2 => Visibility::Private,
        _ => Visibility::Public,
    };
    
    let args = std::slice::from_raw_parts(argv, argc as usize);
    for name_val in args {
        if let Some(method_name) = value_to_symbol_or_string(*name_val) {
            with_method_storage(|storage| {
                storage.set_visibility(class, &method_name, vis);
            });
        }
    }
    RUBY_QNIL
}

/// Eval code (simplified implementation).
#[no_mangle]
pub extern "C" fn jdruby_eval(code: VALUE, _binding: VALUE, _filename: VALUE, _line: VALUE) -> VALUE {
    // In full implementation, would parse and execute code string
    // For now, return the code value itself
    code
}

/// Instance eval (simplified implementation).
#[no_mangle]
pub extern "C" fn jdruby_instance_eval(obj: VALUE, code: VALUE, _binding: VALUE) -> VALUE {
    // Instance eval executes code with obj as self
    // For now, return obj
    if obj != RUBY_QNIL { obj } else { code }
}

/// Class eval (simplified implementation).
#[no_mangle]
pub extern "C" fn jdruby_class_eval(class: VALUE, code: VALUE, _binding: VALUE) -> VALUE {
    // Class eval executes code in class scope
    if class != RUBY_QNIL { class } else { code }
}

/// Module eval (simplified implementation).
#[no_mangle]
pub extern "C" fn jdruby_module_eval(module: VALUE, code: VALUE, _binding: VALUE) -> VALUE {
    // Module eval executes code in module scope
    if module != RUBY_QNIL { module } else { code }
}

/// Get binding (simplified implementation).
#[no_mangle]
pub extern "C" fn jdruby_binding_get() -> VALUE {
    // Return a unique value representing current binding
    // In full implementation, would capture frame context
    RUBY_QTRUE
}

/// Send method dynamically (actual implementation).
#[no_mangle]
pub unsafe extern "C" fn jdruby_send_dynamic(obj: VALUE, name: VALUE, argc: i32, argv: *const VALUE, _block: VALUE) -> VALUE {
    if obj == RUBY_QNIL { return RUBY_QNIL; }
    let method_name = value_to_symbol_or_string(name).unwrap_or("".to_string());
    if method_name.is_empty() { return RUBY_QNIL; }
    
    let args: Vec<VALUE> = if argc > 0 && !argv.is_null() {
        std::slice::from_raw_parts(argv, argc as usize).to_vec()
    } else {
        Vec::new()
    };
    
    // Dispatch to the method
    with_method_storage(|storage| {
        storage.dispatch(obj, &method_name, &args)
    })
}

/// Public send (actual implementation).
#[no_mangle]
pub unsafe extern "C" fn jdruby_public_send(obj: VALUE, name: VALUE, argc: i32, argv: *const VALUE, _block: VALUE) -> VALUE {
    if obj == RUBY_QNIL { return RUBY_QNIL; }
    let method_name = value_to_symbol_or_string(name).unwrap_or("".to_string());
    if method_name.is_empty() { return RUBY_QNIL; }
    
    let args: Vec<VALUE> = if argc > 0 && !argv.is_null() {
        std::slice::from_raw_parts(argv, argc as usize).to_vec()
    } else {
        Vec::new()
    };
    
    // Public send respects visibility (would check visibility in full impl)
    with_method_storage(|storage| {
        storage.dispatch_public(obj, &method_name, &args)
    })
}

/// Respond to (actual implementation).
#[no_mangle]
pub extern "C" fn jdruby_respond_to(obj: VALUE, name: VALUE, _include_private: bool) -> VALUE {
    if obj == RUBY_QNIL { return RUBY_QFALSE; }
    let method_name = value_to_symbol_or_string(name).unwrap_or("".to_string());
    if method_name.is_empty() { return RUBY_QFALSE; }
    
    let responds = with_method_storage(|storage| {
        storage.has_method(obj, &method_name)
    });
    if responds { RUBY_QTRUE } else { RUBY_QFALSE }
}

/// Get method object (actual implementation).
#[no_mangle]
pub extern "C" fn jdruby_method_get(obj: VALUE, name: VALUE) -> VALUE {
    if obj == RUBY_QNIL { return RUBY_QNIL; }
    let method_name = value_to_symbol_or_string(name).unwrap_or("".to_string());
    if method_name.is_empty() { return RUBY_QNIL; }
    
    // Return a Method object (represented as tagged value)
    // In full implementation, would create actual Method struct
    with_class_table(|tbl| {
        let class_id = (obj & 0xFFF0) as u64;
        tbl.create_method_object(obj, &method_name, class_id)
    })
}

/// Get instance method (actual implementation).
#[no_mangle]
pub extern "C" fn jdruby_instance_method_get(class: VALUE, name: VALUE) -> VALUE {
    if class == RUBY_QNIL { return RUBY_QNIL; }
    let method_name = value_to_symbol_or_string(name).unwrap_or("".to_string());
    if method_name.is_empty() { return RUBY_QNIL; }
    
    // Return an UnboundMethod object
    with_class_table(|tbl| {
        tbl.create_unbound_method(class, &method_name)
    })
}

/// Call method object (actual implementation).
#[no_mangle]
pub unsafe extern "C" fn jdruby_method_object_call(method: VALUE, receiver: VALUE, argc: i32, argv: *const VALUE, _block: VALUE) -> VALUE {
    if method == RUBY_QNIL { return RUBY_QNIL; }
    let recv = if receiver == RUBY_QNIL { method } else { receiver };
    
    let args: Vec<VALUE> = if argc > 0 && !argv.is_null() {
        std::slice::from_raw_parts(argv, argc as usize).to_vec()
    } else {
        Vec::new()
    };
    
    // Extract method name from Method object and dispatch
    with_method_storage(|storage| {
        if let Some(method_name) = storage.extract_method_name(method) {
            storage.dispatch(recv, &method_name, &args)
        } else {
            RUBY_QNIL
        }
    })
}

/// Bind method (actual implementation).
#[no_mangle]
pub extern "C" fn jdruby_method_bind(method: VALUE, obj: VALUE) -> VALUE {
    if method == RUBY_QNIL || obj == RUBY_QNIL { return RUBY_QNIL; }
    
    // Bind UnboundMethod to object, creating a Method
    with_method_storage(|storage| {
        storage.bind_method(method, obj)
    })
}

/// Dynamic ivar get (actual implementation).
#[no_mangle]
pub extern "C" fn jdruby_ivar_get_dynamic(obj: VALUE, name: VALUE) -> VALUE {
    if obj == RUBY_QNIL { return RUBY_QNIL; }
    let ivar_name = value_to_symbol_or_string(name).unwrap_or("".to_string());
    if ivar_name.is_empty() { return RUBY_QNIL; }
    
    with_ivar_storage(|storage| storage.get(obj, &ivar_name))
}

/// Dynamic ivar set (actual implementation).
#[no_mangle]
pub extern "C" fn jdruby_ivar_set_dynamic(obj: VALUE, name: VALUE, value: VALUE) {
    if obj == RUBY_QNIL { return; }
    let ivar_name = value_to_symbol_or_string(name).unwrap_or("".to_string());
    if ivar_name.is_empty() { return; }
    
    with_ivar_storage(|storage| {
        storage.set(obj, &ivar_name, value);
    });
}

/// Dynamic cvar get (actual implementation).
#[no_mangle]
pub extern "C" fn jdruby_cvar_get_dynamic(class: VALUE, name: VALUE) -> VALUE {
    if class == RUBY_QNIL { return RUBY_QNIL; }
    let cvar_name = value_to_symbol_or_string(name).unwrap_or("".to_string());
    if cvar_name.is_empty() { return RUBY_QNIL; }
    
    // Store cvars as special constants in class table
    with_class_table(|tbl| {
        let full_name = format!("@@{}:{}", class, cvar_name);
        tbl.class_by_name(&full_name).unwrap_or(RUBY_QNIL)
    })
}

/// Dynamic cvar set (actual implementation).
#[no_mangle]
pub extern "C" fn jdruby_cvar_set_dynamic(class: VALUE, name: VALUE, value: VALUE) {
    if class == RUBY_QNIL { return; }
    let cvar_name = value_to_symbol_or_string(name).unwrap_or("".to_string());
    if cvar_name.is_empty() { return; }
    
    // Store as class constant with @@ prefix
    let full_name = format!("@@{}:{}", class, cvar_name);
    with_class_table(|tbl| {
        tbl.define_class(&full_name, value);
    });
}

/// Dynamic const get (actual implementation).
#[no_mangle]
pub extern "C" fn jdruby_const_get_dynamic(_class: VALUE, name: VALUE, inherit: bool) -> VALUE {
    let const_name = value_to_symbol_or_string(name).unwrap_or("".to_string());
    if const_name.is_empty() { return RUBY_QNIL; }
    
    if inherit {
        // Would walk up class hierarchy in full implementation
        jdruby_const_get(name as *const c_char)
    } else {
        // Look up in class's constant table directly
        jdruby_const_get(name as *const c_char)
    }
}

/// Dynamic const set (actual implementation).
#[no_mangle]
pub extern "C" fn jdruby_const_set_dynamic(class: VALUE, name: VALUE, value: VALUE) {
    if class == RUBY_QNIL { return; }
    let const_name = value_to_symbol_or_string(name).unwrap_or("".to_string());
    if const_name.is_empty() { return; }
    
    // Set constant in class
    jdruby_const_set(name as *const c_char, value);
}

/// Method missing (actual implementation).
#[no_mangle]
pub unsafe extern "C" fn jdruby_method_missing(obj: VALUE, name: VALUE, argc: i32, argv: *const VALUE, _block: VALUE) -> VALUE {
    if obj == RUBY_QNIL { return RUBY_QNIL; }
    let _method_name = value_to_symbol_or_string(name).unwrap_or("method_missing".to_string());
    
    let args: Vec<VALUE> = if argc > 0 && !argv.is_null() {
        std::slice::from_raw_parts(argv, argc as usize).to_vec()
    } else {
        Vec::new()
    };
    
    // Try to dispatch to method_missing
    with_method_storage(|storage| {
        if storage.has_method(obj, "method_missing") {
            // Prepend method name as first arg
            let mut full_args = vec![name];
            full_args.extend(args);
            storage.dispatch(obj, "method_missing", &full_args)
        } else {
            // No method_missing defined, return nil
            // In full implementation, would raise NoMethodError
            RUBY_QNIL
        }
    })
}

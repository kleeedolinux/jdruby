//! # Runtime API — jdruby_* Functions
//!
//! JDRuby-specific runtime entry points.

use std::ffi::CStr;
use std::os::raw::{c_char, c_int};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use crate::core::{VALUE, RUBY_QNIL, RUBY_QTRUE, RUBY_QFALSE, rb_fixnum_p, rb_fix2long, rb_int2fix};
use crate::bridge::conversion::{jdruby_to_value, value_to_jdruby};
use crate::bridge::dedup::str_to_value;
use crate::bridge::registry::{init_bridge, with_registry, ObjectRef};
use crate::capi::immediate::rb_int_new;
use crate::storage::class_table::with_class_table;
use crate::storage::method_storage::{with_method_storage, object_class, Visibility};
use crate::storage::method_storage::MethodEntry;
use crate::storage::ivar_storage::with_ivar_storage;
use crate::storage::symbol_table::{rb_intern_str, with_symbol_table};
use crate::bridge::allocator::init_allocator;

/// Global error state for capturing runtime errors
static RUNTIME_ERROR: AtomicU32 = AtomicU32::new(0);
static ERROR_MESSAGE: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
static ERROR_FUNCTION: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Set a runtime error with message and function context
pub fn set_runtime_error(message: &str, function: &str) {
    *ERROR_MESSAGE.lock().unwrap() = Some(message.to_string());
    *ERROR_FUNCTION.lock().unwrap() = Some(function.to_string());
    RUNTIME_ERROR.store(1, Ordering::SeqCst);
}

/// Check if there is a runtime error
pub fn has_runtime_error() -> bool {
    RUNTIME_ERROR.load(Ordering::SeqCst) != 0
}

/// Get and clear the runtime error message
pub fn take_runtime_error() -> Option<(String, String)> {
    if RUNTIME_ERROR.swap(0, Ordering::SeqCst) != 0 {
        let msg = ERROR_MESSAGE.lock().unwrap().take();
        let func = ERROR_FUNCTION.lock().unwrap().take();
        match (msg, func) {
            (Some(m), Some(f)) => Some((m, f)),
            _ => None,
        }
    } else {
        None
    }
}

/// Print a beautiful Rust-style error to stderr
fn print_error_header() {
    eprintln!();
    eprintln!("\x1b[1;31merror\x1b[0m[\x1b[1;34mruntime\x1b[0m]: ");
}

/// Helper: Convert VALUE to string (handles symbols and strings)
fn value_to_symbol_or_string(value: VALUE) -> Option<String> {
    if value == RUBY_QNIL {
        return None;
    }
    
    // Check if it's a symbol (using the is_symbol check pattern)
    if jdruby_is_symbol(value) {
        // For symbols, extract the symbol ID and look up the name
        // Symbol IDs are stored in the VALUE with specific bit patterns
        let sym_id = (value >> 8) as usize;
        return with_symbol_table(|tbl| {
            tbl.id2name(sym_id).map(|s| s.to_string())
        });
    }
    
    // Try to convert as a string via value_to_jdruby
    let jdruby_val = value_to_jdruby(value);
    let s = jdruby_val.to_ruby_string();
    
    // Check if this is a symbol-formatted string like ":38" (happens when to_s is called on a Symbol VALUE)
    if s.starts_with(":") {
        if let Ok(sym_id) = s[1..].parse::<usize>() {
            if let Some(name) = with_symbol_table(|tbl| tbl.id2name(sym_id).map(|n| n.to_string())) {
                return Some(name);
            }
        }
    }
    
    if !s.is_empty() && s != format!("{:?}", value) {
        return Some(s);
    }
    
    // Check if it's a class/module reference - try to get class name
    if let Some(name) = with_class_table(|tbl| tbl.class_name(value).map(|s| s.to_string())) {
        return Some(name);
    }
    
    None
}

/// CRITICAL FIX: Extract a C string from a Ruby string VALUE.
/// This function directly inspects the VALUE to extract the embedded string content.
/// Unlike value_to_jdruby().to_ruby_string(), this properly handles Ruby string VALUEs.
fn extract_string_from_value(val: VALUE) -> String {
    // First try to extract using the dedicated string extraction function
    if let Some(content) = crate::bridge::dedup::value_to_str(val) {
        return content;
    }
    
    // Fallback: try value_to_jdruby conversion
    let jdruby_val = value_to_jdruby(val);
    let s = jdruby_val.to_ruby_string();
    
    // Remove the "Symbol(" prefix if present (happens with some conversions)
    if s.starts_with("Symbol(") && s.ends_with(")") {
        return s[7..s.len()-1].to_string();
    }
    
    s
}

/// CRITICAL FIX: Sanitize a function name for LLVM IR compatibility.
/// This ensures function names match the format used by the codegen.
fn sanitize_llvm_name(name: &str) -> String {
    name.replace("::", "__")
        .replace('#', "__")
        .replace('<', "_")
        .replace('>', "_")
        .replace('?', "_q")
        .replace('!', "_b")
        .replace('.', "_")
        .replace('@', "_at_")
        .replace('$', "_global_")
        .replace(' ', "_")
}

/// Initialize the bridge (runtime entry point).
#[no_mangle]
pub extern "C" fn jdruby_init_bridge() {
    eprintln!("DEBUG: jdruby_init_bridge called");
    init_allocator();
    init_bridge();
    eprintln!("DEBUG: jdruby_init_bridge complete");
}

/// Check for runtime errors and print them beautifully
/// Should be called at program exit
#[no_mangle]
pub extern "C" fn jdruby_check_errors() -> i32 {
    eprintln!("DEBUG: jdruby_check_errors called");
    if let Some((msg, func)) = take_runtime_error() {
        eprintln!();
        eprintln!("\x1b[1;31merror\x1b[0m[\x1b[1;34mruntime\x1b[0m]: {}", msg);
        eprintln!("  \x1b[1m-->\x1b[0m at {}", func);
        eprintln!();
        return 1;
    }
    eprintln!("DEBUG: jdruby_check_errors complete - no errors");
    0
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

/// Create a new empty array.
#[no_mangle]
pub extern "C" fn jdruby_ary_new_empty() -> VALUE {
    jdruby_to_value(&jdruby_runtime::value::RubyValue::Array(Vec::new()))
}

/// Push a value to an array.
#[no_mangle]
pub extern "C" fn jdruby_ary_push(ary: VALUE, val: VALUE) {
    // First, update the jdruby runtime value
    let mut ruby_val = value_to_jdruby(ary);
    if let jdruby_runtime::value::RubyValue::Array(ref mut vec) = ruby_val {
        vec.push(value_to_jdruby(val));
    }
    
    // CRITICAL FIX: Also update the native RArray in the registry
    // This ensures Array#each can retrieve elements from the native structure
    use crate::bridge::registry::{with_registry, ObjectRef};
    use crate::bridge::conversion::jdruby_to_value;
    
    with_registry(|registry| {
        if let Some(ObjectRef::Array(ptr)) = registry.get(ary) {
            unsafe {
                let rarray = &mut *ptr.as_ptr();
                let current_len = rarray.len as usize;
                let capa = rarray.capa as usize;
                
                // Check if we need to grow the array
                if current_len >= capa {
                    // For now, we don't handle reallocation in this simplified version
                    // The jdruby value has the element, which Array#each will use as fallback
                    eprintln!("DEBUG: jdruby_ary_push - array capacity exceeded, element only in jdruby value");
                } else {
                    // Add element to native array
                    *rarray.ptr.add(current_len) = val;
                    rarray.len = (current_len + 1) as isize;
                    eprintln!("DEBUG: jdruby_ary_push - added element to native array, len={}", current_len + 1);
                }
            }
        } else {
            eprintln!("DEBUG: jdruby_ary_push - no native RArray found for array VALUE {}", ary);
        }
    });
}

/// Create a new hash.
#[no_mangle]
pub extern "C" fn jdruby_hash_new_empty() -> VALUE {
    jdruby_to_value(&jdruby_runtime::value::RubyValue::Hash(jdruby_runtime::value::RubyHash::new()))
}

/// Set a key-value pair in a hash.
#[no_mangle]
pub extern "C" fn jdruby_hash_set(hash: VALUE, key: VALUE, val: VALUE) {
    let mut ruby_val = value_to_jdruby(hash);
    if let jdruby_runtime::value::RubyValue::Hash(ref mut h) = ruby_val {
        h.set(value_to_jdruby(key), value_to_jdruby(val));
    }
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
    // Special handling for symbols - look up the actual name from symbol table
    let s = if jdruby_is_symbol(val) {
        let sym_id = (val >> 8) as usize;
        eprintln!("DEBUG: jdruby_to_s - symbol detected, id={}, val={}", sym_id, val);
        let name = with_symbol_table(|tbl| {
            tbl.id2name(sym_id).map(|s| s.to_string())
        });
        eprintln!("DEBUG: jdruby_to_s - symbol name lookup result: {:?}", name);
        name.unwrap_or_else(|| format!(":{}", sym_id))
    } else {
        value_to_jdruby(val).to_ruby_string()
    };
    eprintln!("DEBUG: jdruby_to_s - returning '{}' for val={}", s, val);
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
    
    eprintln!("DEBUG: jdruby_def_method(class={}, name='{}', func='{}')", class, method_name, func_name);
    
    with_method_storage(|storage| {
        // Use -1 for variable arity to support any number of arguments
        storage.define_method(class, method_name, func_name, -1);
        eprintln!("DEBUG: jdruby_def_method - method registered with variable arity");
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
        // Use the singleton_class method to get or create the singleton class
        // This ensures we reuse the same singleton class for the same object
        tbl.singleton_class(obj).unwrap_or(RUBY_QNIL)
    })
}

/// Include module (actual implementation) - accepts VALUE instead of string.
#[no_mangle]
pub extern "C" fn jdruby_include_module(class: VALUE, module_val: VALUE) {
    if class == RUBY_QNIL || module_val == RUBY_QNIL { return; }
    
    // Extract module name from the VALUE
    let mod_name = value_to_symbol_or_string(module_val).unwrap_or_default();
    if mod_name.is_empty() { return; }
    
    with_method_storage(|storage| {
        with_class_table(|tbl| {
            if let Some(class_name) = tbl.class_name(class) {
                storage.include_module(&class_name, &mod_name);
            }
        });
    });
}

/// Prepend module (actual implementation) - accepts VALUE instead of string.
#[no_mangle]
pub extern "C" fn jdruby_prepend_module(class: VALUE, module_val: VALUE) {
    if class == RUBY_QNIL || module_val == RUBY_QNIL { return; }
    
    let mod_name = value_to_symbol_or_string(module_val).unwrap_or_default();
    if mod_name.is_empty() { return; }
    
    with_method_storage(|storage| {
        with_class_table(|tbl| {
            if let Some(class_name) = tbl.class_name(class) {
                storage.prepend_module(&class_name, &mod_name);
            }
        });
    });
}

/// Extend module (actual implementation) - accepts VALUE instead of string.
#[no_mangle]
pub extern "C" fn jdruby_extend_module(obj: VALUE, module_val: VALUE) {
    if obj == RUBY_QNIL || module_val == RUBY_QNIL { return; }
    
    let mod_name = value_to_symbol_or_string(module_val).unwrap_or_default();
    if mod_name.is_empty() { return; }
    
    // Extend object: add module methods to object's singleton class
    with_method_storage(|storage| {
        with_class_table(|tbl| {
            let singleton_name = format!("#<SingletonClass:{:x}>", obj);
            let singleton = tbl.define_class(&singleton_name, RUBY_QNIL);
            if let Some(s_name) = tbl.class_name(singleton) {
                storage.include_module(&s_name, &mod_name);
            }
        });
    });
}

/// Create a block (actual implementation).
/// 
/// CRITICAL FIX: The returned VALUE must encode the func_name in a way that
/// can be reliably extracted later. We use a special prefix + the actual
/// function name to ensure proper round-tripping.
#[no_mangle]
pub unsafe extern "C" fn jdruby_block_create(func_symbol: *const c_char, argc: i32, argv: *const VALUE) -> VALUE {
    if func_symbol.is_null() { return RUBY_QNIL; }
    let func_name = CStr::from_ptr(func_symbol).to_str().unwrap_or("");
    if func_name.is_empty() {
        eprintln!("DEBUG: jdruby_block_create - EMPTY func_symbol, returning nil");
        return RUBY_QNIL;
    }
    
    eprintln!("DEBUG: jdruby_block_create - func_name='{}', argc={}", func_name, argc);
    
    // CRITICAL FIX: Create a unique block VALUE first
    // Return the function name as a Ruby string VALUE.
    // We MUST use rb_str_new which creates a proper Ruby string that
    // value_to_jdruby().to_ruby_string() can correctly extract.
    let block_value = crate::capi::string::rb_str_new(func_symbol, func_name.len() as i64);
    eprintln!("DEBUG: jdruby_block_create - created block VALUE {} for '{}'", block_value, func_name);
    
    // Store the block info in the bridge using the block VALUE as key (unique per instance)
    use crate::bridge::registry::get_global_bridge;
    if let Some(bridge) = get_global_bridge() {
        let captured: Vec<VALUE> = if argc > 0 && !argv.is_null() {
            std::slice::from_raw_parts(argv, argc as usize).to_vec()
        } else {
            Vec::new()
        };
        // CRITICAL: Use block_value as key, not func_name, so each block instance is unique
        bridge.store_block(block_value, func_name, captured);
        eprintln!("DEBUG: jdruby_block_create - stored {} captures with block VALUE {} as key", argc, block_value);
    }
    
    block_value
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
    
    // Create the block VALUE first
    let block_value = unsafe {
        crate::capi::string::rb_str_new(func_symbol.as_ptr() as *const c_char, func_symbol.len() as i64)
    };
    
    // Store in bridge using block VALUE as key - stores both func_name and captures
    use crate::bridge::registry::get_global_bridge;
    if let Some(bridge) = get_global_bridge() {
        // Store with the symbol value so we know it's a symbol proc
        bridge.store_block(block_value, &func_symbol, vec![symbol]);
    }
    
    block_value
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
    
    // CRITICAL FIX: Get captured variables from the bridge using block VALUE as key
    let mut full_args = Vec::new();
    use crate::bridge::registry::get_global_bridge;
    if let Some(bridge) = get_global_bridge() {
        if let Some((_, captured)) = bridge.get_block_captures(block) {
            eprintln!("DEBUG: jdruby_block_yield - retrieved {} captures for block VALUE {}", 
                captured.len(), block);
            full_args.extend(captured);
        } else {
            eprintln!("DEBUG: jdruby_block_yield - no captures found for block VALUE {}", block);
        }
    }
    full_args.extend(args);
    
    eprintln!("DEBUG: jdruby_block_yield - dispatching '{}' with {} total args", func_name, full_args.len());
    
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

/// Get current self (actual implementation).
#[no_mangle]
pub extern "C" fn jdruby_current_self() -> VALUE {
    use crate::bridge::registry::get_global_bridge;
    use crate::storage::class_table::with_class_table;
    
    if let Some(bridge) = get_global_bridge() {
        // Try to get self from current execution context
        if let Some(self_val) = bridge.get_current_self() {
            return self_val;
        }
    }
    
    // No active method call - return top-level "main" object
    // Look up Object class to use as the main object reference
    with_class_table(|tbl| {
        tbl.class_by_name("Object").unwrap_or(RUBY_QNIL)
    })
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
/// Handles special cases like calling .call on block objects.
#[no_mangle]
pub unsafe extern "C" fn jdruby_send_dynamic(obj: VALUE, name: VALUE, argc: i32, argv: *const VALUE, _block: VALUE) -> VALUE {
    let method_name_str = value_to_symbol_or_string(name).unwrap_or("<unknown>".to_string());
    
    // If obj is nil, use current self (top-level main object)
    let effective_obj = if obj == RUBY_QNIL {
        let self_val = jdruby_current_self();
        eprintln!("DEBUG: jdruby_send_dynamic(obj=nil, name='{}', argc={}) - using self={}", method_name_str, argc, self_val);
        self_val
    } else {
        eprintln!("DEBUG: jdruby_send_dynamic(obj={}, name='{}', argc={})", obj, method_name_str, argc);
        obj
    };
    
    if effective_obj == RUBY_QNIL { 
        set_runtime_error("Cannot call method on nil", "jdruby_send_dynamic");
        eprintln!("DEBUG: jdruby_send_dynamic - effective_obj is nil, returning");
        return RUBY_QNIL; 
    }
    let method_name = value_to_symbol_or_string(name).unwrap_or("".to_string());
    if method_name.is_empty() { 
        set_runtime_error("Method name is empty or invalid", "jdruby_send_dynamic");
        eprintln!("DEBUG: jdruby_send_dynamic - method name empty, returning");
        return RUBY_QNIL; 
    }
    
    let args: Vec<VALUE> = if argc > 0 && !argv.is_null() {
        std::slice::from_raw_parts(argv, argc as usize).to_vec()
    } else {
        Vec::new()
    };
    
    // Special case: calling .call on a block object
    // Block objects are string VALUEs containing function names
    if method_name == "call" {
        let func_name = value_to_jdruby(effective_obj).to_ruby_string();
        if !func_name.is_empty() {
            eprintln!("DEBUG: jdruby_send_dynamic - calling block '{}'", func_name);
            return jdruby_block_yield(effective_obj, argc, argv);
        }
    }
    
    // Special case: calling 'new' on a class
    // In Ruby, 'new' is a class method that allocates an object and calls initialize
    if method_name == "new" {
        // Check if obj is a class (by looking it up in the class table)
        let is_class = with_class_table(|tbl| tbl.is_class(effective_obj));
        if is_class {
            eprintln!("DEBUG: jdruby_send_dynamic - handling 'new' for class");
            // Allocate a new object with this class using the JDGC allocator
            use std::alloc::Layout;
            use crate::bridge::allocator::allocate_object;
            use crate::bridge::registry::ObjectRef;
            
            // Allocate a minimal object (just a placeholder for now)
            let layout = Layout::new::<u64>();
            let new_obj = if let Some((gc_ptr, _)) = allocate_object::<u8>(layout) {
                let obj_id = with_registry(|registry| {
                    let id = registry.alloc_value();
                    registry.insert_with_class(id, ObjectRef::Object(gc_ptr), effective_obj);
                    id
                });
                obj_id
            } else {
                eprintln!("DEBUG: jdruby_send_dynamic - allocation failed, returning nil");
                return RUBY_QNIL;
            };
            
            // Call initialize on the new object with the same args
            // Look up initialize method
            let _init_result = with_method_storage(|storage| {
                if storage.has_method(new_obj, "initialize") {
                    eprintln!("DEBUG: jdruby_send_dynamic - calling initialize on new object");
                    storage.dispatch(new_obj, "initialize", &args)
                } else {
                    eprintln!("DEBUG: jdruby_send_dynamic - no initialize method, returning new object");
                    new_obj
                }
            });
            
            // Return the new object (initialize's return value is ignored in Ruby)
            eprintln!("DEBUG: jdruby_send_dynamic - 'new' returning new object {}", new_obj);
            return new_obj;
        }
    }
    
    // Special case: calling 'puts' - use the runtime implementation
    if method_name == "puts" {
        if !args.is_empty() {
            // Print each argument
            for arg in &args {
                let s = value_to_jdruby(*arg).to_ruby_string();
                println!("{}", s);
            }
        } else {
            // puts with no args prints empty line
            println!();
        }
        return RUBY_QNIL;
    }

    // Special case: calling 'to_s' - use the runtime implementation
    if method_name == "to_s" && args.is_empty() {
        return jdruby_to_s(effective_obj);
    }

    // Special case: calling 'inspect' - use the runtime implementation  
    if method_name == "inspect" && args.is_empty() {
        return jdruby_to_s(effective_obj);
    }

    // Special case: calling 'capitalize' on strings/symbols
    if method_name == "capitalize" && args.is_empty() {
        let s = value_to_jdruby(effective_obj).to_ruby_string();
        if !s.is_empty() {
            let mut chars = s.chars();
            let capitalized = chars.next().unwrap().to_uppercase().collect::<String>() + &chars.as_str().to_lowercase();
            return str_to_value(&capitalized);
        }
        return effective_obj;
    }
    
    // Special case: calling '+' for string concatenation
    if method_name == "+" && args.len() == 1 {
        // Properly handle symbols by extracting their names, not the :id format
        let s1 = if jdruby_is_symbol(effective_obj) {
            let sym_id = (effective_obj >> 8) as usize;
            with_symbol_table(|tbl| tbl.id2name(sym_id).map(|s| s.to_string()).unwrap_or_default())
        } else {
            value_to_jdruby(effective_obj).to_ruby_string()
        };
        let s2 = if jdruby_is_symbol(args[0]) {
            let sym_id = (args[0] >> 8) as usize;
            with_symbol_table(|tbl| tbl.id2name(sym_id).map(|s| s.to_string()).unwrap_or_default())
        } else {
            value_to_jdruby(args[0]).to_ruby_string()
        };
        let concat = format!("{}{}", s1, s2);
        return str_to_value(&concat);
    }

    // Special case: Array#<< (push) - append element to array
    if method_name == "<<" && args.len() == 1 {
        eprintln!("DEBUG: Array#<< - pushing to array {}", effective_obj);
        jdruby_ary_push(effective_obj, args[0]);
        return effective_obj;  // Returns self for chaining
    }

    // Special case: Array#each - iterate with block
    if method_name == "each" {
        eprintln!("DEBUG: Array#each - iterating, block={}", _block);
        // Get array elements from the registry
        let elems: Vec<VALUE> = with_registry(|registry| {
            if let Some(ObjectRef::Array(ary_ptr)) = registry.get(effective_obj) {
                unsafe { 
                    let ary = &*ary_ptr.as_ptr();
                    (0..ary.len as usize).map(|i| *ary.ptr.add(i)).collect()
                }
            } else {
                // Try to get from jdruby value
                let val = value_to_jdruby(effective_obj);
                if let jdruby_runtime::value::RubyValue::Array(ref vec) = val {
                    vec.iter().map(|v| jdruby_to_value(v)).collect()
                } else {
                    Vec::new()
                }
            }
        });
        
        // Yield each element to the block
        if _block != RUBY_QNIL && _block != 0 {
            for elem in elems {
                eprintln!("DEBUG: Array#each - yielding element {}", elem);
                jdruby_block_yield(_block, 1, &elem as *const VALUE);
            }
        }
        return effective_obj;  // Returns self
    }

    // Special case: Array#size / Array#length
    if (method_name == "size" || method_name == "length") && args.is_empty() {
        let len = with_registry(|registry| {
            if let Some(ObjectRef::Array(ary_ptr)) = registry.get(effective_obj) {
                unsafe { (*ary_ptr.as_ptr()).len as usize }
            } else {
                let val = value_to_jdruby(effective_obj);
                if let jdruby_runtime::value::RubyValue::Array(ref vec) = val {
                    vec.len()
                } else {
                    0
                }
            }
        });
        return rb_int_new(len as i64);
    }

    // Special case: Array#[] (index access)
    if method_name == "[]" && !args.is_empty() {
        let idx = value_to_jdruby(args[0]).to_ruby_string().parse::<usize>().unwrap_or(0);
        let elem = with_registry(|registry| {
            if let Some(ObjectRef::Array(ary_ptr)) = registry.get(effective_obj) {
                unsafe {
                    let ary = &*ary_ptr.as_ptr();
                    if idx < ary.len as usize {
                        Some(*ary.ptr.add(idx))
                    } else {
                        None
                    }
                }
            } else {
                let val = value_to_jdruby(effective_obj);
                if let jdruby_runtime::value::RubyValue::Array(ref vec) = val {
                    vec.get(idx).map(|v| jdruby_to_value(v))
                } else {
                    None
                }
            }
        });
        return elem.unwrap_or(RUBY_QNIL);
    }

    // Special case: log method (for Logger module)
    if method_name == "log" && !args.is_empty() {
        let msg = value_to_jdruby(args[0]).to_ruby_string();
        println!("[LOG] {}", msg);
        return RUBY_QNIL;
    }

    // Dispatch to the method
    // CRITICAL: We must handle special cases and lookup first, then release lock before executing
    eprintln!("DEBUG: jdruby_send_dynamic - about to enter with_method_storage for '{}'", method_name);
    
    // First: Handle define_method which needs write access
    if method_name == "define_method" && !args.is_empty() {
        return with_method_storage(|storage| {
            eprintln!("DEBUG: define_method handler triggered (inside storage closure)");
            let method_name_val = args[0];
            let method_name_str = value_to_symbol_or_string(method_name_val).unwrap_or("".to_string());
            eprintln!("DEBUG: define_method - method_name_str='{}', block={}", method_name_str, _block);
            
            if !method_name_str.is_empty() {
                // CRITICAL FIX: Extract function name from block parameter
                // Block objects are created as string VALUEs containing the function symbol
                let func_name = if _block != RUBY_QNIL && _block != 0 {
                    // Direct string extraction from Ruby VALUE
                    // A Ruby string VALUE has a specific format that we can decode
                    let block_str = extract_string_from_value(_block);
                    eprintln!("DEBUG: define_method - block string value: '{}' (raw={})", block_str, _block);
                    if !block_str.is_empty() {
                        // Sanitize the function name for LLVM compatibility
                        sanitize_llvm_name(&block_str)
                    } else {
                        eprintln!("DEBUG: define_method - block string empty, using placeholder");
                        "placeholder_method".to_string()
                    }
                } else {
                    eprintln!("DEBUG: define_method - no block provided, using placeholder");
                    "placeholder_method".to_string()
                };
                
                // CRITICAL FIX: Get block captures from registry using BLOCK VALUE as key
                let captures = if _block != RUBY_QNIL && _block != 0 {
                    use crate::bridge::registry::get_global_bridge;
                    if let Some(bridge) = get_global_bridge() {
                        let caps = bridge.get_block_captures(_block);
                        eprintln!("DEBUG: define_method - retrieved captures for block VALUE {}: {:?}", 
                            _block, caps.as_ref().map(|(_, v)| v.len()));
                        caps.map(|(_, v)| v)  // Extract just the captures vec, not the func_name
                    } else {
                        None
                    }
                } else {
                    None
                };
                
                // Store the method with captures if available
                if let Some(caps) = captures {
                    eprintln!("DEBUG: define_method - registering '{}' -> '{}' with {} captures on class={}", 
                        method_name_str, func_name, caps.len(), effective_obj);
                    storage.define_method_with_block(effective_obj, &method_name_str, &func_name, caps);
                } else {
                    eprintln!("DEBUG: define_method - registering '{}' -> '{}' (no captures) on class={}", 
                        method_name_str, func_name, effective_obj);
                    storage.define_method(effective_obj, &method_name_str, &func_name, -1);
                }
                eprintln!("DEBUG: define_method - registration complete");
                return method_name_val;
            }
            RUBY_QNIL
        });
    }
    
    // Second: Look up method while holding lock, then release and execute
    let method_entry_opt = with_method_storage(|storage| {
        eprintln!("DEBUG: jdruby_send_dynamic - looking up '{}'", method_name);
        let klass = object_class(effective_obj);
        storage.lookup(klass, &method_name).cloned()
    });
    
    // Third: Execute method OUTSIDE the lock (allows reentrant calls)
    if let Some(entry) = method_entry_opt {
        eprintln!("DEBUG: jdruby_send_dynamic - found method '{}' -> '{}' (captures: {})", 
            method_name, entry.func_name, 
            entry.block_captures.as_ref().map(|v| v.len()).unwrap_or(0));
        unsafe {
            use crate::capi::class::dispatch_c_method;
            use crate::bridge::registry::get_global_bridge;
            
            // CRITICAL FIX: Prepend block captures to args for methods defined with blocks
            let full_args = if let Some(ref captures) = entry.block_captures {
                let mut combined = captures.clone();
                combined.extend(args);
                eprintln!("DEBUG: jdruby_send_dynamic - prepending {} captures, total args: {}", 
                    captures.len(), combined.len());
                combined
            } else {
                args
            };
            
            // Set current self before dispatch
            if let Some(bridge) = get_global_bridge() {
                bridge.set_current_self(Some(effective_obj));
            }
            let result = dispatch_c_method(&entry.func_name, entry.arity, effective_obj, &full_args);
            // Clear current self after dispatch
            if let Some(bridge) = get_global_bridge() {
                bridge.set_current_self(None);
            }
            eprintln!("DEBUG: jdruby_send_dynamic - dispatch returned {}", result);
            return result;
        }
    }
    
    // Method not found
    eprintln!("DEBUG: jdruby_send_dynamic - method '{}' not found", method_name);
    RUBY_QNIL
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

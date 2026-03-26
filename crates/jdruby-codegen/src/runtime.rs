//! Runtime FFI bindings — integrates with jdruby-ffi crate.
//!
//! This module defines the LLVM IR declarations that correspond to
//! the actual C-ABI functions exported by jdruby-ffi and jdruby-runtime.

// use jdruby_ffi::value::{RUBY_QNIL, RUBY_QTRUE, RUBY_QFALSE};

/// Runtime function signature for LLVM IR declaration.
pub struct RuntimeFn {
    pub name: &'static str,
    pub ret: &'static str,
    pub params: &'static [&'static str],
    pub variadic: bool,
}

/// All runtime functions available to generated code.
/// These must match the `#[no_mangle]` extern "C" functions in jdruby-ffi.
pub static RUNTIME_FNS: &[RuntimeFn] = &[
    // Value constructors (from jdruby-ffi/ruby_capi.rs)
    RuntimeFn { name: "jdruby_int_new", ret: "i64", params: &["i64"], variadic: false },
    RuntimeFn { name: "jdruby_float_new", ret: "i64", params: &["double"], variadic: false },
    RuntimeFn { name: "jdruby_str_new", ret: "i64", params: &["i8*", "i64"], variadic: false },
    RuntimeFn { name: "jdruby_sym_intern", ret: "i64", params: &["i8*"], variadic: false },
    RuntimeFn { name: "jdruby_ary_new", ret: "i64", params: &["i32"], variadic: true },
    RuntimeFn { name: "jdruby_hash_new", ret: "i64", params: &["i32"], variadic: true },
    RuntimeFn { name: "jdruby_bool", ret: "i64", params: &["i1"], variadic: false },
    
    // Method dispatch
    RuntimeFn { name: "jdruby_send", ret: "i64", params: &["i64", "i8*", "i32"], variadic: true },
    RuntimeFn { name: "jdruby_call", ret: "i64", params: &["i8*", "i32"], variadic: true },
    RuntimeFn { name: "jdruby_yield", ret: "i64", params: &["i32"], variadic: true },
    RuntimeFn { name: "jdruby_block_given", ret: "i1", params: &[], variadic: false },
    
    // I/O
    RuntimeFn { name: "jdruby_puts", ret: "void", params: &["i64"], variadic: false },
    RuntimeFn { name: "jdruby_print", ret: "void", params: &["i64"], variadic: false },
    RuntimeFn { name: "jdruby_p", ret: "i64", params: &["i64"], variadic: false },
    RuntimeFn { name: "jdruby_raise", ret: "void", params: &["i8*"], variadic: true },
    
    // Arithmetic
    RuntimeFn { name: "jdruby_int_add", ret: "i64", params: &["i64", "i64"], variadic: false },
    RuntimeFn { name: "jdruby_int_sub", ret: "i64", params: &["i64", "i64"], variadic: false },
    RuntimeFn { name: "jdruby_int_mul", ret: "i64", params: &["i64", "i64"], variadic: false },
    RuntimeFn { name: "jdruby_int_div", ret: "i64", params: &["i64", "i64"], variadic: false },
    RuntimeFn { name: "jdruby_int_mod", ret: "i64", params: &["i64", "i64"], variadic: false },
    RuntimeFn { name: "jdruby_int_pow", ret: "i64", params: &["i64", "i64"], variadic: false },
    
    // Comparison
    RuntimeFn { name: "jdruby_eq", ret: "i1", params: &["i64", "i64"], variadic: false },
    RuntimeFn { name: "jdruby_lt", ret: "i1", params: &["i64", "i64"], variadic: false },
    RuntimeFn { name: "jdruby_gt", ret: "i1", params: &["i64", "i64"], variadic: false },
    RuntimeFn { name: "jdruby_le", ret: "i1", params: &["i64", "i64"], variadic: false },
    RuntimeFn { name: "jdruby_ge", ret: "i1", params: &["i64", "i64"], variadic: false },
    RuntimeFn { name: "jdruby_truthy", ret: "i1", params: &["i64"], variadic: false },
    
    // Class/module
    RuntimeFn { name: "jdruby_class_new", ret: "i64", params: &["i8*", "i64"], variadic: false },
    RuntimeFn { name: "jdruby_def_method", ret: "void", params: &["i64", "i8*", "i8*"], variadic: false },
    RuntimeFn { name: "jdruby_const_get", ret: "i64", params: &["i8*"], variadic: false },
    RuntimeFn { name: "jdruby_const_set", ret: "void", params: &["i8*", "i64"], variadic: false },
    RuntimeFn { name: "jdruby_ivar_get", ret: "i64", params: &["i64", "i8*"], variadic: false },
    RuntimeFn { name: "jdruby_ivar_set", ret: "void", params: &["i64", "i8*", "i64"], variadic: false },
    
    // MRI C API compatibility (from jdruby-ffi/ruby_capi.rs)
    RuntimeFn { name: "rb_int_new", ret: "i64", params: &["i64"], variadic: false },
    RuntimeFn { name: "rb_str_new", ret: "i64", params: &["i8*", "i64"], variadic: false },
    RuntimeFn { name: "rb_ary_new", ret: "i64", params: &[], variadic: false },
    RuntimeFn { name: "rb_hash_new", ret: "i64", params: &[], variadic: false },
    RuntimeFn { name: "rb_intern", ret: "i64", params: &["i8*"], variadic: false },
    RuntimeFn { name: "rb_funcallv", ret: "i64", params: &["i64", "i64", "i32", "i64*"], variadic: false },
    RuntimeFn { name: "rb_define_class", ret: "i64", params: &["i8*", "i64"], variadic: false },
    RuntimeFn { name: "rb_define_method", ret: "void", params: &["i64", "i8*", "i64", "i32"], variadic: false },
    RuntimeFn { name: "rb_iv_get", ret: "i64", params: &["i64", "i8*"], variadic: false },
    RuntimeFn { name: "rb_iv_set", ret: "i64", params: &["i64", "i8*", "i64"], variadic: false },
    RuntimeFn { name: "rb_const_get", ret: "i64", params: &["i64", "i64"], variadic: false },
    RuntimeFn { name: "rb_const_set", ret: "void", params: &["i64", "i64", "i64"], variadic: false },
    RuntimeFn { name: "rb_gc_mark", ret: "void", params: &["i64"], variadic: false },
];

/// External globals from jdruby-ffi - these use the actual VALUES from the FFI crate.
pub fn runtime_globals() -> Vec<(&'static str, &'static str)> {
    vec! [
        ("JDRUBY_NIL", "i64"),
        ("JDRUBY_TRUE", "i64"),
        ("JDRUBY_FALSE", "i64"),
        // Also emit MRI-compatible names
        ("Qnil", "i64"),
        ("Qtrue", "i64"),
        ("Qfalse", "i64"),
    ]
}

/// Emit all runtime declarations as LLVM IR.
pub fn emit_runtime_decls(out: &mut String) {
    for (name, ty) in runtime_globals() {
        out.push_str(&format!("@{} = external global {}\n", name, ty));
    }
    out.push('\n');
    
    for func in RUNTIME_FNS {
        let params = match func.params {
            &["i64", "i64"] => "i64, i64",
            _ => &func.params.join(", "),
        };
        let variadic = if func.variadic { ", ..." } else { "" };
        out.push_str(&format!(
            "declare {} @{}({}{})\n",
            func.ret, func.name, params, variadic
        ));
    }
}

/// Check if a function name is a known runtime function.
pub fn is_runtime_fn(name: &str) -> bool {
    RUNTIME_FNS.iter().any(|f| f.name == name)
}

/// Get runtime function signature if it exists.
pub fn get_runtime_fn(name: &str) -> Option<&'static RuntimeFn> {
    RUNTIME_FNS.iter().find(|f| f.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_runtime_fn_lookup() {
        assert!(is_runtime_fn("jdruby_int_new"));
        assert!(is_runtime_fn("jdruby_send"));
        assert!(!is_runtime_fn("nonexistent"));
    }
    
    #[test]
    fn test_get_runtime_fn() {
        let func = get_runtime_fn("jdruby_int_add").unwrap();
        assert_eq!(func.name, "jdruby_int_add");
        assert_eq!(func.ret, "i64");
        assert_eq!(func.params, &["i64", "i64"]);
        assert!(!func.variadic);
    }
    
    #[test]
    fn test_emit_runtime_decls() {
        let mut output = String::new();
        emit_runtime_decls(&mut output);
        assert!(output.contains("declare i64 @jdruby_int_new(i64)"));
        assert!(output.contains("@JDRUBY_NIL = external global i64"));
        assert!(output.contains("declare i1 @jdruby_truthy(i64)"));
    }
}

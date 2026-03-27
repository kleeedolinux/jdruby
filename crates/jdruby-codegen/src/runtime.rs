//! Runtime FFI bindings — integrates with jdruby-ffi crate using Inkwell.
//!
//! This module defines the LLVM IR declarations that correspond to
//! the actual C-ABI functions exported by jdruby-ffi and jdruby-runtime.

use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::types::{BasicMetadataTypeEnum, BasicType, BasicTypeEnum};
use inkwell::values::{BasicValueEnum, FunctionValue, GlobalValue};
use inkwell::AddressSpace;

/// Runtime function signature for Inkwell.
pub struct RuntimeFn {
    pub name: &'static str,
    pub ret_type: RuntimeType,
    pub param_types: &'static [RuntimeType],
    pub variadic: bool,
}

/// Runtime type representation for Inkwell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeType {
    Void,
    I1,   // bool
    I32,  // int32
    I64,  // Ruby VALUE (int64)
    F64,  // double
    Ptr,  // i8* (pointer)
}

impl RuntimeType {
    /// Convert to Inkwell BasicTypeEnum.
    pub fn to_inkwell_type<'ctx>(&self, ctx: &'ctx Context) -> BasicTypeEnum<'ctx> {
        match self {
            RuntimeType::I1 => ctx.bool_type().into(),
            RuntimeType::I32 => ctx.i32_type().into(),
            RuntimeType::I64 => ctx.i64_type().into(),
            RuntimeType::F64 => ctx.f64_type().into(),
            RuntimeType::Ptr => ctx.ptr_type(AddressSpace::default()).into(),
            RuntimeType::Void => panic!("Void is not a BasicTypeEnum, handle separately"),
        }
    }

    /// Check if this is void.
    pub fn is_void(&self) -> bool {
        matches!(self, RuntimeType::Void)
    }
}

/// All runtime functions available to generated code.
/// These must match the `#[no_mangle]` extern "C" functions in jdruby-ffi.
pub static RUNTIME_FNS: &[RuntimeFn] = &[
    // Runtime initialization
    RuntimeFn { name: "jdruby_init_bridge", ret_type: RuntimeType::Void, param_types: &[], variadic: false },
    
    // Value constructors (from jdruby-ffi/ruby_capi.rs)
    RuntimeFn { name: "jdruby_int_new", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_float_new", ret_type: RuntimeType::I64, param_types: &[RuntimeType::F64], variadic: false },
    RuntimeFn { name: "jdruby_str_new", ret_type: RuntimeType::I64, param_types: &[RuntimeType::Ptr, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_sym_intern", ret_type: RuntimeType::I64, param_types: &[RuntimeType::Ptr], variadic: false },
    RuntimeFn { name: "jdruby_ary_new", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I32], variadic: false },
    RuntimeFn { name: "jdruby_hash_new", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I32], variadic: false },
    RuntimeFn { name: "jdruby_bool", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I1], variadic: false },
    
    // String operations
    RuntimeFn { name: "jdruby_str_concat", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_to_s", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_send", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::Ptr, RuntimeType::I32, RuntimeType::Ptr], variadic: false },
    RuntimeFn { name: "jdruby_call", ret_type: RuntimeType::I64, param_types: &[RuntimeType::Ptr, RuntimeType::I32, RuntimeType::Ptr], variadic: false },
    RuntimeFn { name: "jdruby_yield", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I32, RuntimeType::Ptr], variadic: false },
    RuntimeFn { name: "jdruby_block_given", ret_type: RuntimeType::I1, param_types: &[], variadic: false },
    
    // I/O
    RuntimeFn { name: "jdruby_puts", ret_type: RuntimeType::Void, param_types: &[RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_print", ret_type: RuntimeType::Void, param_types: &[RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_p", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_raise", ret_type: RuntimeType::Void, param_types: &[RuntimeType::I64, RuntimeType::Ptr, RuntimeType::I32, RuntimeType::Ptr], variadic: false },
    
    // Arithmetic
    RuntimeFn { name: "jdruby_int_add", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_int_sub", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_int_mul", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_int_div", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_int_mod", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_int_pow", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I64], variadic: false },
    
    // Comparison
    RuntimeFn { name: "jdruby_eq", ret_type: RuntimeType::I1, param_types: &[RuntimeType::I64, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_lt", ret_type: RuntimeType::I1, param_types: &[RuntimeType::I64, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_gt", ret_type: RuntimeType::I1, param_types: &[RuntimeType::I64, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_le", ret_type: RuntimeType::I1, param_types: &[RuntimeType::I64, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_ge", ret_type: RuntimeType::I1, param_types: &[RuntimeType::I64, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_truthy", ret_type: RuntimeType::I1, param_types: &[RuntimeType::I64], variadic: false },
    
    // Class/module
    RuntimeFn { name: "jdruby_class_new", ret_type: RuntimeType::I64, param_types: &[RuntimeType::Ptr, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_module_new", ret_type: RuntimeType::I64, param_types: &[RuntimeType::Ptr], variadic: false },
    RuntimeFn { name: "jdruby_singleton_class_get", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_def_method", ret_type: RuntimeType::Void, param_types: &[RuntimeType::I64, RuntimeType::Ptr, RuntimeType::Ptr], variadic: false },
    RuntimeFn { name: "jdruby_prepend_module", ret_type: RuntimeType::Void, param_types: &[RuntimeType::I64, RuntimeType::Ptr], variadic: false },
    RuntimeFn { name: "jdruby_extend_module", ret_type: RuntimeType::Void, param_types: &[RuntimeType::I64, RuntimeType::Ptr], variadic: false },
    RuntimeFn { name: "jdruby_const_get", ret_type: RuntimeType::I64, param_types: &[RuntimeType::Ptr], variadic: false },
    RuntimeFn { name: "jdruby_const_set", ret_type: RuntimeType::Void, param_types: &[RuntimeType::Ptr, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_ivar_get", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::Ptr], variadic: false },
    RuntimeFn { name: "jdruby_ivar_set", ret_type: RuntimeType::Void, param_types: &[RuntimeType::I64, RuntimeType::Ptr, RuntimeType::I64], variadic: false },
    
    // Block/closure operations
    RuntimeFn { name: "jdruby_block_create", ret_type: RuntimeType::I64, param_types: &[RuntimeType::Ptr, RuntimeType::I32, RuntimeType::Ptr], variadic: false },
    RuntimeFn { name: "jdruby_proc_create", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_lambda_create", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_block_yield", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I32, RuntimeType::Ptr], variadic: false },
    RuntimeFn { name: "jdruby_current_block", ret_type: RuntimeType::I64, param_types: &[], variadic: false },
    
    // Dynamic method operations
    RuntimeFn { name: "jdruby_define_method_dynamic", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I64, RuntimeType::Ptr, RuntimeType::I32], variadic: false },
    RuntimeFn { name: "jdruby_undef_method", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_remove_method", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_alias_method", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I64, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_set_visibility", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I32, RuntimeType::I32, RuntimeType::Ptr], variadic: false },
    
    // Dynamic evaluation
    RuntimeFn { name: "jdruby_eval", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I64, RuntimeType::I64, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_instance_eval", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I64, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_class_eval", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I64, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_module_eval", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I64, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_binding_get", ret_type: RuntimeType::I64, param_types: &[], variadic: false },
    
    // Reflection
    RuntimeFn { name: "jdruby_send_dynamic", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I64, RuntimeType::I32, RuntimeType::Ptr, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_public_send", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I64, RuntimeType::I32, RuntimeType::Ptr, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_respond_to", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I64, RuntimeType::I1], variadic: false },
    RuntimeFn { name: "jdruby_method_get", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_instance_method_get", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_method_object_call", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I64, RuntimeType::I32, RuntimeType::Ptr, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_method_bind", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I64], variadic: false },
    
    // Dynamic variable access
    RuntimeFn { name: "jdruby_ivar_get_dynamic", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_ivar_set_dynamic", ret_type: RuntimeType::Void, param_types: &[RuntimeType::I64, RuntimeType::I64, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_cvar_get_dynamic", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_cvar_set_dynamic", ret_type: RuntimeType::Void, param_types: &[RuntimeType::I64, RuntimeType::I64, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_const_get_dynamic", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I64, RuntimeType::I1], variadic: false },
    RuntimeFn { name: "jdruby_const_set_dynamic", ret_type: RuntimeType::Void, param_types: &[RuntimeType::I64, RuntimeType::I64, RuntimeType::I64], variadic: false },
    
    // Method missing
    RuntimeFn { name: "jdruby_method_missing", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I64, RuntimeType::I32, RuntimeType::Ptr, RuntimeType::I64], variadic: false },
    
    // MRI C API compatibility (from jdruby-ffi/ruby_capi.rs)
    RuntimeFn { name: "rb_int_new", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64], variadic: false },
    RuntimeFn { name: "rb_str_new", ret_type: RuntimeType::I64, param_types: &[RuntimeType::Ptr, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "rb_ary_new", ret_type: RuntimeType::I64, param_types: &[], variadic: false },
    RuntimeFn { name: "rb_hash_new", ret_type: RuntimeType::I64, param_types: &[], variadic: false },
    RuntimeFn { name: "rb_intern", ret_type: RuntimeType::I64, param_types: &[RuntimeType::Ptr], variadic: false },
    RuntimeFn { name: "rb_funcallv", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I64, RuntimeType::I32, RuntimeType::Ptr], variadic: false },
    RuntimeFn { name: "rb_define_class", ret_type: RuntimeType::I64, param_types: &[RuntimeType::Ptr, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "rb_define_method", ret_type: RuntimeType::Void, param_types: &[RuntimeType::I64, RuntimeType::Ptr, RuntimeType::I64, RuntimeType::I32], variadic: false },
    RuntimeFn { name: "rb_iv_get", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::Ptr], variadic: false },
    RuntimeFn { name: "rb_iv_set", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::Ptr, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "rb_const_get", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "rb_const_set", ret_type: RuntimeType::Void, param_types: &[RuntimeType::I64, RuntimeType::I64, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "rb_gc_mark", ret_type: RuntimeType::Void, param_types: &[RuntimeType::I64], variadic: false },
];

/// Runtime global variables.
pub static RUNTIME_GLOBALS: &[(&str, RuntimeType)] = &[
    ("JDRUBY_NIL", RuntimeType::I64),
    ("JDRUBY_TRUE", RuntimeType::I64),
    ("JDRUBY_FALSE", RuntimeType::I64),
    ("Qnil", RuntimeType::I64),
    ("Qtrue", RuntimeType::I64),
    ("Qfalse", RuntimeType::I64),
];

/// Emit all runtime declarations into an Inkwell module.
pub fn emit_runtime_decls<'ctx>(ctx: &'ctx Context, module: &Module<'ctx>) {
    let i64_type = ctx.i64_type();
    
    // Emit global variables as ptr (opaque pointer)
    for (name, ty) in RUNTIME_GLOBALS {
        let llvm_ty: BasicTypeEnum<'ctx> = match ty {
            RuntimeType::I64 => i64_type.into(),
            _ => i64_type.into(),
        };
        // Add global with i64 type, but load/store will use ptr_type
        module.add_global(llvm_ty, None, name);
    }
    
    // Emit function declarations
    for func in RUNTIME_FNS {
        let fn_type = create_fn_type(ctx, func);
        module.add_function(func.name, fn_type, None);
    }
}

/// Create an Inkwell function type from a RuntimeFn.
fn create_fn_type<'ctx>(ctx: &'ctx Context, func: &RuntimeFn) -> inkwell::types::FunctionType<'ctx> {
    let param_types: Vec<BasicMetadataTypeEnum> = func.param_types
        .iter()
        .map(|t| t.to_inkwell_type(ctx).into())
        .collect();
    
    if func.ret_type.is_void() {
        ctx.void_type().fn_type(param_types.as_slice(), func.variadic)
    } else {
        func.ret_type.to_inkwell_type(ctx).fn_type(param_types.as_slice(), func.variadic)
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

/// Get a runtime function value from the module.
pub fn get_runtime_fn_value<'ctx>(module: &Module<'ctx>, name: &str) -> Option<FunctionValue<'ctx>> {
    module.get_function(name)
}

/// Get a runtime global value from the module.
pub fn get_runtime_global<'ctx>(module: &Module<'ctx>, name: &str) -> Option<GlobalValue<'ctx>> {
    module.get_global(name)
}

/// Load a runtime global as a value (for use in instructions).
pub fn load_runtime_global<'ctx>(
    builder: &Builder<'ctx>,
    module: &Module<'ctx>,
    ctx: &'ctx Context,
    name: &str,
) -> Option<BasicValueEnum<'ctx>> {
    let global = module.get_global(name)?;
    let i64_type = ctx.i64_type();
    Some(builder.build_load(i64_type, global.as_pointer_value(), name).ok()?.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use inkwell::context::Context;
    
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
        assert_eq!(func.ret_type, RuntimeType::I64);
        assert_eq!(func.param_types, &[RuntimeType::I64, RuntimeType::I64]);
        assert!(!func.variadic);
    }
    
    #[test]
    fn test_emit_runtime_decls() {
        let ctx = Context::create();
        let module = ctx.create_module("test");
        emit_runtime_decls(&ctx, &module);
        
        // Check that functions were declared
        let func = module.get_function("jdruby_int_new");
        assert!(func.is_some());
        
        // Check that globals were declared
        let global = module.get_global("JDRUBY_NIL");
        assert!(global.is_some());
        
        // Verify the IR contains our declarations
        let ir = module.print_to_string().to_string();
        assert!(ir.contains("declare i64 @jdruby_int_new(i64)"));
        assert!(ir.contains("@JDRUBY_NIL"));
    }
}

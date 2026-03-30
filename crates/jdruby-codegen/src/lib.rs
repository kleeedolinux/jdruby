//! # JDRuby Codegen — LLVM IR Code Generation using Inkwell
//!
//! Translates MIR to LLVM IR for native compilation using the real JDRuby runtime.
//!
//! This crate provides a modular code generation system with:
//! - **IR Module**: Core LLVM IR building infrastructure
//! - **Register Module**: Virtual register allocation and tracking
//! - **Constants Module**: String interning and constant deduplication
//! - **Selection Module**: Pattern-based instruction selection for optimization
//! - **Optimize Module**: LLVM pass management and optimization

pub mod ir;
pub mod register;
pub mod constants;
pub mod selection;
pub mod optimize;

/// Utility functions for name sanitization and other helpers.
pub mod utils {
    pub use crate::sanitize_name;
}

/// Runtime function declarations and types.
pub mod runtime {
    pub use crate::{emit_runtime_decls, predeclare_globals, RuntimeFn, RuntimeType, RUNTIME_FNS, RUNTIME_GLOBALS};
}

use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::targets::{TargetMachine, TargetTriple};
use inkwell::builder::Builder;
use inkwell::values::{BasicValueEnum, FunctionValue};
use inkwell::types::{BasicMetadataTypeEnum, BasicType, BasicTypeEnum};
use inkwell::AddressSpace;
use jdruby_common::{Diagnostic, ErrorReporter, SourceSpan};
use jdruby_mir::{MirModule, MirFunction, MirInst, MirTerminator, MirConst, MirBinOp};
use std::collections::HashMap;

use crate::ir::{FunctionCodegen, RubyType, TypedValue};
use crate::constants::{StringPool, ConstantTable};
use crate::selection::{PatternRegistry};
use crate::selection::patterns::InstructionSelector;
use crate::optimize::OptLevel as PassOptLevel;

/// Optimization levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptLevel {
    O0,
    O1,
    O2,
    O3,
    Os,
    Oz,
}

impl Default for OptLevel {
    fn default() -> Self {
        OptLevel::O2
    }
}

impl From<OptLevel> for PassOptLevel {
    fn from(level: OptLevel) -> Self {
        match level {
            OptLevel::O0 => PassOptLevel::None,
            OptLevel::O1 => PassOptLevel::Less,
            OptLevel::O2 => PassOptLevel::Default,
            OptLevel::O3 => PassOptLevel::Aggressive,
            OptLevel::Os => PassOptLevel::Default,
            OptLevel::Oz => PassOptLevel::Aggressive,
        }
    }
}

/// Output format for generated code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    LlvmIr,
    Bitcode,
    Assembly,
    Object,
}

impl Default for OutputFormat {
    fn default() -> Self {
        OutputFormat::LlvmIr
    }
}

/// Configuration for code generation.
#[derive(Debug, Clone)]
pub struct CodegenConfig {
    pub target_triple: String,
    pub opt_level: OptLevel,
    pub debug_info: bool,
    pub output_format: OutputFormat,
}

impl Default for CodegenConfig {
    fn default() -> Self {
        Self {
            target_triple: TargetMachine::get_default_triple()
                .as_str()
                .to_str()
                .unwrap_or("x86_64-unknown-linux-gnu")
                .to_string(),
            opt_level: OptLevel::O2,
            debug_info: false,
            output_format: OutputFormat::LlvmIr,
        }
    }
}

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
    I1,
    I32,
    I64,
    F64,
    Ptr,
}

impl RuntimeType {
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

    pub fn is_void(&self) -> bool {
        matches!(self, RuntimeType::Void)
    }
}

/// All runtime functions available to generated code.
pub static RUNTIME_FNS: &[RuntimeFn] = &[
    RuntimeFn { name: "jdruby_init_bridge", ret_type: RuntimeType::Void, param_types: &[], variadic: false },
    RuntimeFn { name: "jdruby_int_new", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_float_new", ret_type: RuntimeType::I64, param_types: &[RuntimeType::F64], variadic: false },
    RuntimeFn { name: "jdruby_str_new", ret_type: RuntimeType::I64, param_types: &[RuntimeType::Ptr, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_sym_intern", ret_type: RuntimeType::I64, param_types: &[RuntimeType::Ptr], variadic: false },
    RuntimeFn { name: "jdruby_ary_new", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I32], variadic: false },
    RuntimeFn { name: "jdruby_hash_new", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I32], variadic: false },
    RuntimeFn { name: "jdruby_bool", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I1], variadic: false },
    RuntimeFn { name: "jdruby_str_concat", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_to_s", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_send", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::Ptr, RuntimeType::I32, RuntimeType::Ptr], variadic: false },
    RuntimeFn { name: "jdruby_call", ret_type: RuntimeType::I64, param_types: &[RuntimeType::Ptr, RuntimeType::I32, RuntimeType::Ptr], variadic: false },
    RuntimeFn { name: "jdruby_yield", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I32, RuntimeType::Ptr], variadic: false },
    RuntimeFn { name: "jdruby_block_given", ret_type: RuntimeType::I1, param_types: &[], variadic: false },
    RuntimeFn { name: "jdruby_puts", ret_type: RuntimeType::Void, param_types: &[RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_print", ret_type: RuntimeType::Void, param_types: &[RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_p", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_raise", ret_type: RuntimeType::Void, param_types: &[RuntimeType::I64, RuntimeType::Ptr, RuntimeType::I32, RuntimeType::Ptr], variadic: false },
    RuntimeFn { name: "jdruby_int_add", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_int_sub", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_int_mul", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_int_div", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_int_mod", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_int_pow", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_eq", ret_type: RuntimeType::I1, param_types: &[RuntimeType::I64, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_lt", ret_type: RuntimeType::I1, param_types: &[RuntimeType::I64, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_gt", ret_type: RuntimeType::I1, param_types: &[RuntimeType::I64, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_le", ret_type: RuntimeType::I1, param_types: &[RuntimeType::I64, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_ge", ret_type: RuntimeType::I1, param_types: &[RuntimeType::I64, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_truthy", ret_type: RuntimeType::I1, param_types: &[RuntimeType::I64], variadic: false },
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
    RuntimeFn { name: "jdruby_block_create", ret_type: RuntimeType::I64, param_types: &[RuntimeType::Ptr, RuntimeType::I32, RuntimeType::Ptr], variadic: false },
    RuntimeFn { name: "jdruby_proc_create", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_lambda_create", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_block_yield", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I32, RuntimeType::Ptr], variadic: false },
    RuntimeFn { name: "jdruby_current_block", ret_type: RuntimeType::I64, param_types: &[], variadic: false },
    RuntimeFn { name: "jdruby_symbol_to_proc", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_is_symbol", ret_type: RuntimeType::I1, param_types: &[RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_define_method_dynamic", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I64, RuntimeType::Ptr, RuntimeType::I32], variadic: false },
    RuntimeFn { name: "jdruby_undef_method", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_remove_method", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_alias_method", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I64, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_set_visibility", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I32, RuntimeType::I32, RuntimeType::Ptr], variadic: false },
    RuntimeFn { name: "jdruby_eval", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I64, RuntimeType::I64, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_instance_eval", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I64, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_class_eval", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I64, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_module_eval", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I64, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_binding_get", ret_type: RuntimeType::I64, param_types: &[], variadic: false },
    RuntimeFn { name: "jdruby_send_dynamic", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I64, RuntimeType::I32, RuntimeType::Ptr, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_public_send", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I64, RuntimeType::I32, RuntimeType::Ptr, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_respond_to", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I64, RuntimeType::I1], variadic: false },
    RuntimeFn { name: "jdruby_method_get", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_instance_method_get", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_method_object_call", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I64, RuntimeType::I32, RuntimeType::Ptr, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_method_bind", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_ivar_get_dynamic", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_ivar_set_dynamic", ret_type: RuntimeType::Void, param_types: &[RuntimeType::I64, RuntimeType::I64, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_cvar_get_dynamic", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_cvar_set_dynamic", ret_type: RuntimeType::Void, param_types: &[RuntimeType::I64, RuntimeType::I64, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_const_get_dynamic", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I64, RuntimeType::I1], variadic: false },
    RuntimeFn { name: "jdruby_const_set_dynamic", ret_type: RuntimeType::Void, param_types: &[RuntimeType::I64, RuntimeType::I64, RuntimeType::I64], variadic: false },
    RuntimeFn { name: "jdruby_method_missing", ret_type: RuntimeType::I64, param_types: &[RuntimeType::I64, RuntimeType::I64, RuntimeType::I32, RuntimeType::Ptr, RuntimeType::I64], variadic: false },
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

/// Sanitize a Ruby identifier for LLVM IR.
pub fn sanitize_name(name: &str) -> String {
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

/// Pre-declare all global variables referenced in Load/Store instructions.
///
/// This ensures global variables exist in the module before function emission.
/// Note: 
/// - Instance variables (starting with @) use ivar_get/ivar_set
/// - Local variables (starting with lowercase or _) use alloca (stack)
/// - Only constants (uppercase) and Ruby globals ($) are declared as LLVM globals
pub fn predeclare_globals<'ctx>(ctx: &'ctx Context, module: &Module<'ctx>, mir_module: &MirModule) {
    let i64_type = ctx.i64_type();

    for func in &mir_module.functions {
        for block in &func.blocks {
            for inst in &block.instructions {
                match inst {
                    MirInst::Load(_, name) | MirInst::Store(name, _) => {
                        // Skip instance variables - they use ivar_get/ivar_set
                        if name.starts_with('@') {
                            continue;
                        }
                        // Skip local variables (start with lowercase or _) - they use alloca
                        if !name.is_empty() {
                            let first_char = name.chars().next().unwrap();
                            if first_char.is_ascii_lowercase() || first_char == '_' {
                                continue;
                            }
                        }
                        let global_name = sanitize_name(name);
                        // Only create if it doesn't already exist
                        if module.get_global(&global_name).is_none() {
                            let global = module.add_global(i64_type, None, &global_name);
                            global.set_linkage(inkwell::module::Linkage::Internal);
                            global.set_initializer(&i64_type.const_int(0, false));
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

/// Emit all runtime declarations into an Inkwell module.
pub fn emit_runtime_decls<'ctx>(ctx: &'ctx Context, module: &Module<'ctx>) {
    let i64_type = ctx.i64_type();

    for (name, ty) in RUNTIME_GLOBALS {
        let llvm_ty: BasicTypeEnum<'ctx> = match ty {
            RuntimeType::I64 => i64_type.into(),
            _ => i64_type.into(),
        };
        let global = module.add_global(llvm_ty, None, name);
        global.set_linkage(inkwell::module::Linkage::External);
    }

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

/// Tracks state during LLVM IR generation with Inkwell.
pub struct CodegenContext<'ctx> {
    module_name: String,
    diagnostics: Vec<Diagnostic>,
    globals: HashMap<String, inkwell::values::GlobalValue<'ctx>>,
}

impl<'ctx> CodegenContext<'ctx> {
    pub fn new() -> Self {
        Self {
            module_name: String::new(),
            diagnostics: Vec::new(),
            globals: HashMap::new(),
        }
    }

    pub fn clear(&mut self) {
        self.module_name.clear();
        self.diagnostics.clear();
        self.globals.clear();
    }

    pub fn set_module_name(&mut self, name: &str) {
        self.module_name = name.to_string();
    }

    pub fn module_name(&self) -> &str {
        &self.module_name
    }

    pub fn prescan_function(&mut self, _func: &MirFunction) {
        // Pre-scanning is now handled by StringPool and ConstantTable
    }

    pub fn has_errors(&self) -> bool {
        !self.diagnostics.is_empty()
    }

    pub fn take_diagnostics(&mut self) -> Vec<Diagnostic> {
        std::mem::take(&mut self.diagnostics)
    }

    pub fn add_diagnostic(&mut self, diag: Diagnostic) {
        self.diagnostics.push(diag);
    }
}

impl<'ctx> Default for CodegenContext<'ctx> {
    fn default() -> Self {
        Self::new()
    }
}

/// Main code generator for LLVM IR using Inkwell.
pub struct CodeGenerator<'ctx> {
    config: CodegenConfig,
    context: CodegenContext<'ctx>,
    llvm_context: &'ctx Context,
}

impl<'ctx> CodeGenerator<'ctx> {
    pub fn new(config: CodegenConfig, llvm_context: &'ctx Context) -> Self {
        Self {
            context: CodegenContext::new(),
            config,
            llvm_context,
        }
    }

    /// Generate LLVM IR from a MIR module, returning Result for compatibility.
    pub fn generate(&mut self, module: &MirModule) -> Result<String, Vec<Diagnostic>> {
        let (output, mut reporter) = self.generate_with_errors(module);
        if reporter.has_errors() {
            Err(reporter.take_diagnostics())
        } else {
            Ok(output)
        }
    }

    /// Generate LLVM IR from a MIR module with detailed error reporting.
    pub fn generate_with_errors(&mut self, module: &MirModule) -> (String, ErrorReporter) {
        self.context.clear();
        self.context.set_module_name(&module.name);

        let mut reporter = ErrorReporter::new();

        let llvm_module = self.llvm_context.create_module(&module.name);
        let target_triple = TargetTriple::create(&self.config.target_triple);
        llvm_module.set_triple(&target_triple);

        let builder = self.llvm_context.create_builder();

        let mut string_pool = StringPool::new(self.llvm_context, &llvm_module);
        let mut constant_table = ConstantTable::new(self.llvm_context, &llvm_module);

        string_pool.predeclare_strings(module);
        constant_table.predeclare_constants(module);

        predeclare_globals(self.llvm_context, &llvm_module, module);

        emit_runtime_decls(self.llvm_context, &llvm_module);

        let pattern_registry = PatternRegistry::with_defaults();

        let i64_type = self.llvm_context.i64_type();
        for func in &module.functions {
            let total_params = if func.name.starts_with("block_") || func.name.starts_with("block_in_") || func.name.starts_with("__sym_proc_") {
                func.params.len() + func.captured_vars.len()
            } else {
                func.params.len()
            };
            let fn_type = i64_type.fn_type(&vec![i64_type.into(); total_params], false);
            let fn_name = sanitize_name(&func.name);
            llvm_module.add_function(&fn_name, fn_type, None);
        }

        for (_i, func) in module.functions.iter().enumerate() {
            if let Err(diagnostics) = emit_function(
                func,
                &mut string_pool,
                &mut constant_table,
                &pattern_registry,
                self.llvm_context,
                &llvm_module,
                &builder,
            ) {
                for diag in diagnostics {
                    reporter.report_diagnostic(diag);
                }
            }
        }

        if self.context.has_errors() {
            for diag in self.context.take_diagnostics() {
                reporter.report_diagnostic(diag);
            }
        }

        let output = llvm_module.print_to_string().to_string();
        (output, reporter)
    }

    /// Generate LLVM module for JIT compilation (returns the module directly).
    pub fn generate_module(&mut self, module: &MirModule) -> Result<Module<'ctx>, Vec<Diagnostic>> {
        self.context.clear();
        self.context.set_module_name(&module.name);

        let llvm_module = self.llvm_context.create_module(&module.name);
        let target_triple = TargetTriple::create(&self.config.target_triple);
        llvm_module.set_triple(&target_triple);

        let builder = self.llvm_context.create_builder();

        let mut string_pool = StringPool::new(self.llvm_context, &llvm_module);
        let mut constant_table = ConstantTable::new(self.llvm_context, &llvm_module);

        string_pool.predeclare_strings(module);
        constant_table.predeclare_constants(module);

        predeclare_globals(self.llvm_context, &llvm_module, module);

        emit_runtime_decls(self.llvm_context, &llvm_module);

        let pattern_registry = PatternRegistry::with_defaults();

        let i64_type = self.llvm_context.i64_type();
        for func in &module.functions {
            let total_params = if func.name.starts_with("block_") || func.name.starts_with("block_in_") || func.name.starts_with("__sym_proc_") {
                func.params.len() + func.captured_vars.len()
            } else {
                func.params.len()
            };
            let fn_type = i64_type.fn_type(&vec![i64_type.into(); total_params], false);
            let fn_name = sanitize_name(&func.name);
            llvm_module.add_function(&fn_name, fn_type, None);
        }

        for func in &module.functions {
            if let Err(diagnostics) = emit_function(
                func,
                &mut string_pool,
                &mut constant_table,
                &pattern_registry,
                self.llvm_context,
                &llvm_module,
                &builder,
            ) {
                for diag in diagnostics {
                    self.context.add_diagnostic(diag);
                }
            }
        }

        if self.context.has_errors() {
            return Err(self.context.take_diagnostics());
        }

        if let Err(err) = llvm_module.verify() {
            return Err(vec![Diagnostic::error(
                format!("Module verification failed: {}", err),
                SourceSpan::default(),
            )]);
        }

        Ok(llvm_module)
    }
}

/// Emit a single function to LLVM IR.
fn emit_function<'ctx>(
    func: &MirFunction,
    string_pool: &mut StringPool<'ctx, '_>,
    constant_table: &mut ConstantTable<'ctx, '_>,
    pattern_registry: &PatternRegistry,
    ctx: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
) -> Result<(), Vec<Diagnostic>> {
    let fn_name = sanitize_name(&func.name);
    let function = module.get_function(&fn_name)
        .ok_or_else(|| vec![Diagnostic::error(
            format!("Function {} not found in module", fn_name),
            func.span,
        )])?;

    let mut func_codegen = FunctionCodegen::new(
        func.name.clone(),
        ctx,
        module,
    );

    let ir_builder = builder;

    let entry_block = func_codegen.get_or_create_block("entry", function);
    builder.position_at_end(entry_block);

    // Emit jdruby_init_bridge() call at the start of main()
    if fn_name == "main" {
        if let Some(init_fn) = module.get_function("jdruby_init_bridge") {
            let _ = builder.build_call(init_fn, &[], "init_bridge");
        }
    }

    for (i, &param_reg) in func.params.iter().enumerate() {
        let param = function.get_nth_param(i as u32)
            .ok_or_else(|| vec![Diagnostic::error(
                format!("Parameter {} not found", i),
                func.span,
            )])?;
        let typed_val = TypedValue::new(param, RubyType::Unknown, None);
        func_codegen.set_register(param_reg, typed_val);
        func_codegen.record_register_def(param_reg, RubyType::Unknown, 0, i as u32);
    }

    // Handle captured variables for block functions - they are passed as additional parameters
    let num_params = func.params.len();
    for (i, captured_var_name) in func.captured_vars.iter().enumerate() {
        let param_idx = (num_params + i) as u32;
        let param = function.get_nth_param(param_idx)
            .ok_or_else(|| vec![Diagnostic::error(
                format!("Captured var parameter {} not found", param_idx),
                func.span,
            )])?;
        // Store the captured variable value to its local alloca
        let ptr = func_codegen.get_or_create_local(captured_var_name, builder);
        builder.build_store(ptr, param)
            .map_err(|e| vec![Diagnostic::error(format!("Store captured var failed: {:?}", e), func.span)])?;
        // Also set up a register mapping so Load instructions work correctly
        // Find the register that was assigned to this captured var in the MIR
        if let Some(&reg_id) = func.params.get(param_idx as usize) {
            let typed_val = TypedValue::new(param, RubyType::Unknown, None);
            func_codegen.set_register(reg_id, typed_val);
            func_codegen.record_register_def(reg_id, RubyType::Unknown, 0, param_idx);
        }
    }

    for (block_idx, block) in func.blocks.iter().enumerate() {
        if block_idx == 0 {
            continue;
        }
        func_codegen.get_or_create_block(&block.label, function);
    }

    let selector = InstructionSelector::new(pattern_registry, &MirModule {
        name: module.get_name().to_str().unwrap_or("unknown").to_string(),
        functions: vec![func.clone()],
    });
    let selected_ops = selector.select_function(func);

    for (block_idx, block) in func.blocks.iter().enumerate() {
        let block_label = if block_idx == 0 { "entry".to_string() } else { block.label.clone() };
        let current_block = func_codegen.get_or_create_block(&block_label, function);
        builder.position_at_end(current_block);
        func_codegen.set_current_block(current_block);

        let ops_for_block: Vec<_> = selected_ops.iter()
            .filter(|op| op.block_idx == block_idx)
            .collect();

        for op in ops_for_block {
            for (inst_idx, inst) in op.instructions.iter().enumerate() {
                if let Err(diag) = emit_instruction(
                    inst,
                    block_idx as u32,
                    inst_idx as u32,
                    &ir_builder,
                    &mut func_codegen,
                    string_pool,
                    constant_table,
                    ctx,
                    module,
                    function,
                ) {
                    return Err(vec![diag]);
                }
            }
        }

        emit_terminator(&block.terminator, &ir_builder, &mut func_codegen, constant_table, ctx, function)?;
    }

    Ok(())
}

/// Emit a single MIR instruction to LLVM IR.
fn emit_instruction<'ctx>(
    inst: &MirInst,
    block_idx: u32,
    inst_idx: u32,
    builder: &Builder<'ctx>,
    func_codegen: &mut FunctionCodegen<'ctx, '_>,
    string_pool: &mut StringPool<'ctx, '_>,
    constant_table: &mut ConstantTable<'ctx, '_>,
    ctx: &'ctx Context,
    module: &Module<'ctx>,
    _function: FunctionValue<'ctx>,
) -> Result<(), Diagnostic> {
    use MirInst::*;

    match inst {
        LoadConst(dest, konst) => {
            let value = match konst {
                MirConst::Integer(n) => constant_table.get_integer(*n),
                MirConst::Float(f) => constant_table.get_float(*f),
                MirConst::String(s) => {
                    let ptr = string_pool.get_string_ptr(builder, s);
                    let len = ctx.i64_type().const_int(s.len() as u64, false);
                    let fn_val = module.get_function("jdruby_str_new")
                        .ok_or_else(|| Diagnostic::error("jdruby_str_new not found".to_string(), SourceSpan::default()))?;
                    let call = match builder.build_call(fn_val, &[ptr.into(), len.into()], "str_new") {
                        Ok(c) => c,
                        Err(e) => return Err(Diagnostic::error(format!("Call failed: {:?}", e), SourceSpan::default())),
                    };
                    call.try_as_basic_value().unwrap_basic()
                }
                MirConst::Symbol(s) => constant_table.get_symbol(s),
                MirConst::Bool(b) => constant_table.get_bool(*b),
                MirConst::Nil => constant_table.get_nil(),
            };
            let ruby_type = match konst {
                MirConst::Integer(_) => RubyType::Integer,
                MirConst::Float(_) => RubyType::Float,
                MirConst::String(_) => RubyType::String,
                MirConst::Symbol(_) => RubyType::Symbol,
                MirConst::Bool(_) => RubyType::Boolean,
                MirConst::Nil => RubyType::Nil,
            };
            let typed_val = TypedValue::new(value, ruby_type, None);
            func_codegen.set_register(*dest, typed_val);
            func_codegen.record_register_def(*dest, ruby_type, block_idx, inst_idx);
        }

        Load(dest, name) => {
            // Check if this is an instance variable (starts with @)
            if name.starts_with('@') {
                // Load self (the current object)
                let self_val = if let Some(self_reg) = func_codegen.get_register(0) {
                    self_reg.llvm_value()
                } else {
                    // Get self from the first parameter
                    let fn_val = module.get_function(&sanitize_name(func_codegen.name()))
                        .ok_or_else(|| Diagnostic::error(format!("Function {} not found", func_codegen.name()), SourceSpan::default()))?;
                    fn_val.get_nth_param(0)
                        .ok_or_else(|| Diagnostic::error("self parameter not found".to_string(), SourceSpan::default()))?
                };
                // Get the ivar name without the @ prefix
                let ivar_name = &name[1..];
                let name_ptr = string_pool.get_cstring_ptr(builder, ivar_name);
                let fn_val = module.get_function("jdruby_ivar_get")
                    .ok_or_else(|| Diagnostic::error("jdruby_ivar_get not found".to_string(), SourceSpan::default()))?;
                let call = match builder.build_call(fn_val, &[self_val.into(), name_ptr.into()], &format!("ivar_get_{}", ivar_name)) {
                    Ok(c) => c,
                    Err(e) => return Err(Diagnostic::error(format!("Ivar get failed: {:?}", e), SourceSpan::default())),
                };
                let value = call.try_as_basic_value().unwrap_basic();
                let typed_val = TypedValue::new(value, RubyType::Unknown, None);
                func_codegen.set_register(*dest, typed_val);
                func_codegen.record_register_def(*dest, RubyType::Unknown, block_idx, inst_idx);
            } else {
                // Local variable - use stack allocation (alloca)
                let ptr = func_codegen.get_or_create_local(name, builder);
                let i64_type = ctx.i64_type();
                let loaded = builder.build_load(i64_type, ptr, &format!("load_{}", name))
                    .map_err(|e| Diagnostic::error(format!("Load failed: {:?}", e), SourceSpan::default()))?;
                let value = loaded.into();
                let typed_val = TypedValue::new(value, RubyType::Unknown, None);
                func_codegen.set_register(*dest, typed_val);
                func_codegen.record_register_def(*dest, RubyType::Unknown, block_idx, inst_idx);
            }
        }

        Store(name, src) => {
            let value = func_codegen.get_register_or_nil(*src, constant_table).llvm_value();
            // Check if this is an instance variable (starts with @)
            if name.starts_with('@') {
                // Load self (the current object)
                let self_val = if let Some(self_reg) = func_codegen.get_register(0) {
                    self_reg.llvm_value()
                } else {
                    // Get self from the first parameter
                    let fn_val = module.get_function(&sanitize_name(func_codegen.name()))
                        .ok_or_else(|| Diagnostic::error(format!("Function {} not found", func_codegen.name()), SourceSpan::default()))?;
                    fn_val.get_nth_param(0)
                        .ok_or_else(|| Diagnostic::error("self parameter not found".to_string(), SourceSpan::default()))?
                };
                // Get the ivar name without the @ prefix
                let ivar_name = &name[1..];
                let name_ptr = string_pool.get_cstring_ptr(builder, ivar_name);
                let fn_val = module.get_function("jdruby_ivar_set")
                    .ok_or_else(|| Diagnostic::error("jdruby_ivar_set not found".to_string(), SourceSpan::default()))?;
                match builder.build_call(fn_val, &[self_val.into(), name_ptr.into(), value.into()], &format!("ivar_set_{}", ivar_name)) {
                    Ok(_) => {},
                    Err(e) => return Err(Diagnostic::error(format!("Ivar set failed: {:?}", e), SourceSpan::default())),
                };
            } else {
                // Local variable - use stack allocation (alloca)
                let ptr = func_codegen.get_or_create_local(name, builder);
                builder.build_store(ptr, value)
                    .map_err(|e| Diagnostic::error(format!("Store failed: {:?}", e), SourceSpan::default()))?;
            }
        }

        Copy(dest, src) => {
            let value = func_codegen.get_register_or_nil(*src, constant_table);
            func_codegen.set_register(*dest, value.clone());
            func_codegen.record_register_def(*dest, value.ruby_type(), block_idx, inst_idx);
            func_codegen.record_register_use(*src, block_idx, inst_idx);
        }

        BinOp(dest, op, left, right) => {
            let left_val = func_codegen.get_register_or_nil(*left, constant_table).llvm_value();
            let right_val = func_codegen.get_register_or_nil(*right, constant_table).llvm_value();

            let result = emit_binop(*op, left_val, right_val, builder, ctx, module)?;
            let result_type = infer_binop_type(*op, func_codegen.get_register(*left), func_codegen.get_register(*right));
            let typed_val = TypedValue::new(result, result_type, None);
            func_codegen.set_register(*dest, typed_val);
            func_codegen.record_register_def(*dest, result_type, block_idx, inst_idx);
            func_codegen.record_register_use(*left, block_idx, inst_idx);
            func_codegen.record_register_use(*right, block_idx, inst_idx);
        }

        Call(dest, name, args) => {
            let fn_name = sanitize_name(name);
            let (result, is_void) = if let Some(fn_val) = module.get_function(&fn_name) {
                let arg_values: Vec<_> = args.iter()
                    .map(|&r| func_codegen.get_register_or_nil(r, constant_table).llvm_value().into())
                    .collect();
                let call = match builder.build_call(fn_val, &arg_values, &format!("call_{}", fn_name)) {
                    Ok(c) => c,
                    Err(e) => return Err(Diagnostic::error(format!("Call failed: {:?}", e), SourceSpan::default())),
                };
                let is_void = fn_val.get_type().get_return_type().is_none();
                let val = if is_void {
                    constant_table.get_nil()
                } else {
                    call.try_as_basic_value().unwrap_basic()
                };
                (val, is_void)
            } else {
                let name_ptr = string_pool.get_cstring_ptr(builder, name);
                let len = ctx.i32_type().const_int(args.len() as u64, false);
                let args_array = if args.is_empty() {
                    ctx.ptr_type(AddressSpace::default()).const_null()
                } else {
                    let i64_type = ctx.i64_type();
                    let args_ptr = builder.build_alloca(
                        i64_type.array_type(args.len() as u32),
                        "args_array",
                    ).map_err(|e| Diagnostic::error(format!("Alloca failed: {:?}", e), SourceSpan::default()))?;
                    for (i, &arg) in args.iter().enumerate() {
                        let arg_val = func_codegen.get_register_or_nil(arg, constant_table).llvm_value();
                        let idx = ctx.i64_type().const_int(i as u64, false);
                        let elem_ptr = unsafe {
                            builder.build_gep(
                                i64_type,
                                args_ptr,
                                &[idx],
                                &format!("arg_{}", i),
                            ).map_err(|e| Diagnostic::error(format!("GEP failed: {:?}", e), SourceSpan::default()))?
                        };
                        builder.build_store(elem_ptr, arg_val)
                            .map_err(|e| Diagnostic::error(format!("Store failed: {:?}", e), SourceSpan::default()))?;
                    }
                    args_ptr
                };
                let fn_val = module.get_function("jdruby_call")
                    .ok_or_else(|| Diagnostic::error("jdruby_call not found".to_string(), SourceSpan::default()))?;
                let call = match builder.build_call(fn_val, &[name_ptr.into(), len.into(), args_array.into()], &format!("runtime_call_{}", name)) {
                    Ok(c) => c,
                    Err(e) => return Err(Diagnostic::error(format!("Call failed: {:?}", e), SourceSpan::default())),
                };
                (call.try_as_basic_value().unwrap_basic(), false)
            };
            let result_type = if is_void { RubyType::Nil } else { RubyType::Unknown };
            let typed_val = TypedValue::new(result, result_type, None);
            func_codegen.set_register(*dest, typed_val);
            func_codegen.record_register_def(*dest, result_type, block_idx, inst_idx);
            for &arg in args {
                func_codegen.record_register_use(arg, block_idx, inst_idx);
            }
        }

        MethodCall(dest, obj, method, args) => {
            let obj_val = func_codegen.get_register_or_nil(*obj, constant_table).llvm_value();
            let method_ptr = string_pool.get_cstring_ptr(builder, method);
            let len = ctx.i32_type().const_int(args.len() as u64, false);
            let args_array = if args.is_empty() {
                ctx.ptr_type(AddressSpace::default()).const_null()
            } else {
                let i64_type = ctx.i64_type();
                let args_ptr = builder.build_alloca(
                    i64_type.array_type(args.len() as u32),
                    "method_args",
                ).map_err(|e| Diagnostic::error(format!("Alloca failed: {:?}", e), SourceSpan::default()))?;
                for (i, &arg) in args.iter().enumerate() {
                    let arg_val = func_codegen.get_register_or_nil(arg, constant_table).llvm_value();
                    let idx = ctx.i64_type().const_int(i as u64, false);
                    let elem_ptr = unsafe {
                        builder.build_gep(
                            i64_type,
                            args_ptr,
                            &[idx],
                            &format!("m_arg_{}", i),
                        ).map_err(|e| Diagnostic::error(format!("GEP failed: {:?}", e), SourceSpan::default()))?
                    };
                    builder.build_store(elem_ptr, arg_val)
                        .map_err(|e| Diagnostic::error(format!("Store failed: {:?}", e), SourceSpan::default()))?;
                }
                args_ptr
            };
            let fn_val = module.get_function("jdruby_send")
                .ok_or_else(|| Diagnostic::error("jdruby_send not found".to_string(), SourceSpan::default()))?;
            let call = match builder.build_call(fn_val, &[obj_val.into(), method_ptr.into(), len.into(), args_array.into()], &format!("send_{}", method)) {
                Ok(c) => c,
                Err(e) => return Err(Diagnostic::error(format!("Send failed: {:?}", e), SourceSpan::default())),
            };
            let result = call.try_as_basic_value().unwrap_basic();
            let typed_val = TypedValue::new(result, RubyType::Unknown, None);
            func_codegen.set_register(*dest, typed_val);
            func_codegen.record_register_def(*dest, RubyType::Unknown, block_idx, inst_idx);
            func_codegen.record_register_use(*obj, block_idx, inst_idx);
            for &arg in args {
                func_codegen.record_register_use(arg, block_idx, inst_idx);
            }
        }

        ClassNew(dest, name, superclass) => {
            let name_ptr = string_pool.get_cstring_ptr(builder, name);
            let super_val = if let Some(sc) = superclass {
                let sc_ptr = string_pool.get_cstring_ptr(builder, sc);
                let const_get_fn = module.get_function("jdruby_const_get")
                    .ok_or_else(|| Diagnostic::error("jdruby_const_get not found".to_string(), SourceSpan::default()))?;
                let call = match builder.build_call(const_get_fn, &[sc_ptr.into()], "get_super") {
                    Ok(c) => c,
                    Err(e) => return Err(Diagnostic::error(format!("Call failed: {:?}", e), SourceSpan::default())),
                };
                call.try_as_basic_value().unwrap_basic()
            } else {
                constant_table.get_nil()
            };
            let fn_val = module.get_function("jdruby_class_new")
                .ok_or_else(|| Diagnostic::error("jdruby_class_new not found".to_string(), SourceSpan::default()))?;
            let call = match builder.build_call(fn_val, &[name_ptr.into(), super_val.into()], "class_new") {
                Ok(c) => c,
                Err(e) => return Err(Diagnostic::error(format!("Call failed: {:?}", e), SourceSpan::default())),
            };
            let result = call.try_as_basic_value().unwrap_basic();
            let typed_val = TypedValue::new(result, RubyType::Class, None);
            func_codegen.set_register(*dest, typed_val);
            func_codegen.record_register_def(*dest, RubyType::Class, block_idx, inst_idx);
        }

        DefMethod(class_reg, method_name, func_name) => {
            let class_val = func_codegen.get_register_or_nil(*class_reg, constant_table).llvm_value();
            let method_ptr = string_pool.get_cstring_ptr(builder, method_name);
            let func_ptr = string_pool.get_cstring_ptr(builder, func_name);
            let fn_val = module.get_function("jdruby_def_method")
                .ok_or_else(|| Diagnostic::error("jdruby_def_method not found".to_string(), SourceSpan::default()))?;
            match builder.build_call(fn_val, &[class_val.into(), method_ptr.into(), func_ptr.into()], "def_method") {
                Ok(_) => {},
                Err(e) => return Err(Diagnostic::error(format!("Call failed: {:?}", e), SourceSpan::default())),
            };
            func_codegen.record_register_use(*class_reg, block_idx, inst_idx);
        }

        IncludeModule(class_reg, module_name) => {
            let class_val = func_codegen.get_register_or_nil(*class_reg, constant_table).llvm_value();
            let module_ptr = string_pool.get_cstring_ptr(builder, module_name);
            let fn_val = module.get_function("jdruby_prepend_module")
                .ok_or_else(|| Diagnostic::error("jdruby_prepend_module not found".to_string(), SourceSpan::default()))?;
            match builder.build_call(fn_val, &[class_val.into(), module_ptr.into()], "include_module") {
                Ok(_) => {},
                Err(e) => return Err(Diagnostic::error(format!("Call failed: {:?}", e), SourceSpan::default())),
            };
            func_codegen.record_register_use(*class_reg, block_idx, inst_idx);
        }

        BlockCreate { dest, func_symbol, captured_vars, is_lambda: _ } => {
            // Get the function pointer
            let fn_name = sanitize_name(func_symbol);
            let func_val = module.get_function(&fn_name)
                .ok_or_else(|| Diagnostic::error(format!("Block function {} not found", fn_name), SourceSpan::default()))?;
            
            // Get jdruby_block_create function
            // signature: jdruby_block_create(func_ptr, captured_vars_array, num_captures)
            let block_fn = module.get_function("jdruby_block_create")
                .ok_or_else(|| Diagnostic::error("jdruby_block_create not found".to_string(), SourceSpan::default()))?;
            
            // Build captured vars array if any
            let captures_ptr = if captured_vars.is_empty() {
                ctx.ptr_type(AddressSpace::default()).const_null()
            } else {
                let i64_type = ctx.i64_type();
                let captures_array = builder.build_alloca(
                    i64_type.array_type(captured_vars.len() as u32),
                    "block_captures",
                ).map_err(|e| Diagnostic::error(format!("Alloca failed: {:?}", e), SourceSpan::default()))?;
                for (i, &cap_reg) in captured_vars.iter().enumerate() {
                    let cap_val = func_codegen.get_register_or_nil(cap_reg, constant_table).llvm_value();
                    let idx = ctx.i64_type().const_int(i as u64, false);
                    let elem_ptr = unsafe {
                        builder.build_gep(
                            i64_type,
                            captures_array,
                            &[idx],
                            &format!("capture_{}", i),
                        ).map_err(|e| Diagnostic::error(format!("GEP failed: {:?}", e), SourceSpan::default()))?
                    };
                    builder.build_store(elem_ptr, cap_val)
                        .map_err(|e| Diagnostic::error(format!("Store failed: {:?}", e), SourceSpan::default()))?;
                }
                captures_array
            };
            
            let num_captures = ctx.i32_type().const_int(captured_vars.len() as u64, false);
            
            let call = match builder.build_call(
                block_fn,
                &[func_val.as_global_value().as_pointer_value().into(), num_captures.into(), captures_ptr.into()],
                "block_create"
            ) {
                Ok(c) => c,
                Err(e) => return Err(Diagnostic::error(format!("BlockCreate failed: {:?}", e), SourceSpan::default())),
            };
            
            let result = call.try_as_basic_value().unwrap_basic();
            let typed_val = TypedValue::new(result, RubyType::Block, None);
            func_codegen.set_register(*dest, typed_val);
            func_codegen.record_register_def(*dest, RubyType::Block, block_idx, inst_idx);
            for &cap in captured_vars {
                func_codegen.record_register_use(cap, block_idx, inst_idx);
            }
        }

        Send { dest, obj_reg, name_reg, args, block_reg } => {
            let obj_val = func_codegen.get_register_or_nil(*obj_reg, constant_table).llvm_value();
            let name_val = func_codegen.get_register_or_nil(*name_reg, constant_table).llvm_value();
            let len = ctx.i32_type().const_int(args.len() as u64, false);
            
            // Build args array
            let args_array = if args.is_empty() {
                ctx.ptr_type(AddressSpace::default()).const_null()
            } else {
                let i64_type = ctx.i64_type();
                let args_ptr = builder.build_alloca(
                    i64_type.array_type(args.len() as u32),
                    "send_args",
                ).map_err(|e| Diagnostic::error(format!("Alloca failed: {:?}", e), SourceSpan::default()))?;
                for (i, &arg) in args.iter().enumerate() {
                    let arg_val = func_codegen.get_register_or_nil(arg, constant_table).llvm_value();
                    let idx = ctx.i64_type().const_int(i as u64, false);
                    let elem_ptr = unsafe {
                        builder.build_gep(
                            i64_type,
                            args_ptr,
                            &[idx],
                            &format!("send_arg_{}", i),
                        ).map_err(|e| Diagnostic::error(format!("GEP failed: {:?}", e), SourceSpan::default()))?
                    };
                    builder.build_store(elem_ptr, arg_val)
                        .map_err(|e| Diagnostic::error(format!("Store failed: {:?}", e), SourceSpan::default()))?;
                }
                args_ptr
            };
            
            // Use jdruby_send_dynamic which takes method name as i64 (symbol value)
            // signature: jdruby_send_dynamic(obj, method_name, argc, argv, block)
            let fn_val = module.get_function("jdruby_send_dynamic")
                .ok_or_else(|| Diagnostic::error("jdruby_send_dynamic not found".to_string(), SourceSpan::default()))?;
            
            // Use the provided block if available, otherwise pass nil (Qnil = 4)
            let block_val = if let Some(block) = block_reg {
                func_codegen.get_register_or_nil(*block, constant_table).llvm_value()
            } else {
                ctx.i64_type().const_int(0x04, false).into()
            };
            
            let call = match builder.build_call(
                fn_val, 
                &[obj_val.into(), name_val.into(), len.into(), args_array.into(), block_val.into()], 
                "send_dynamic"
            ) {
                Ok(c) => c,
                Err(e) => return Err(Diagnostic::error(format!("Send failed: {:?}", e), SourceSpan::default())),
            };
            let result = call.try_as_basic_value().unwrap_basic();
            let typed_val = TypedValue::new(result, RubyType::Unknown, None);
            func_codegen.set_register(*dest, typed_val);
            func_codegen.record_register_def(*dest, RubyType::Unknown, block_idx, inst_idx);
            func_codegen.record_register_use(*obj_reg, block_idx, inst_idx);
            func_codegen.record_register_use(*name_reg, block_idx, inst_idx);
            for &arg in args {
                func_codegen.record_register_use(arg, block_idx, inst_idx);
            }
        }

        _ => {}
    }

    Ok(())
}

/// Emit a binary operation.
fn emit_binop<'ctx>(
    op: MirBinOp,
    left: BasicValueEnum<'ctx>,
    right: BasicValueEnum<'ctx>,
    builder: &Builder<'ctx>,
    _ctx: &'ctx Context,
    module: &Module<'ctx>,
) -> Result<BasicValueEnum<'ctx>, Diagnostic> {
    use MirBinOp::*;

    match op {
        Add | Sub | Mul | Div | Mod => {
            let fn_name = match op {
                Add => "jdruby_int_add",
                Sub => "jdruby_int_sub",
                Mul => "jdruby_int_mul",
                Div => "jdruby_int_div",
                Mod => "jdruby_int_mod",
                _ => unreachable!(),
            };
            let fn_val = module.get_function(fn_name)
                .ok_or_else(|| Diagnostic::error(format!("{} not found", fn_name), SourceSpan::default()))?;
            let call = match builder.build_call(fn_val, &[left.into(), right.into()], &format!("binop_{}", fn_name)) {
                Ok(c) => c,
                Err(e) => return Err(Diagnostic::error(format!("Call failed: {:?}", e), SourceSpan::default())),
            };
            Ok(call.try_as_basic_value().unwrap_basic())
        }
        Eq | Lt | Gt | LtEq | GtEq => {
            let fn_name = match op {
                Eq => "jdruby_eq",
                Lt => "jdruby_lt",
                Gt => "jdruby_gt",
                LtEq => "jdruby_le",
                GtEq => "jdruby_ge",
                _ => unreachable!(),
            };
            let fn_val = module.get_function(fn_name)
                .ok_or_else(|| Diagnostic::error(format!("{} not found", fn_name), SourceSpan::default()))?;
            let call = match builder.build_call(fn_val, &[left.into(), right.into()], &format!("cmp_{}", fn_name)) {
                Ok(c) => c,
                Err(e) => return Err(Diagnostic::error(format!("Call failed: {:?}", e), SourceSpan::default())),
            };
            let result = call.try_as_basic_value().unwrap_basic().into_int_value();
            let bool_fn = module.get_function("jdruby_bool")
                .ok_or_else(|| Diagnostic::error("jdruby_bool not found".to_string(), SourceSpan::default()))?;
            let bool_call = match builder.build_call(bool_fn, &[result.into()], "to_ruby_bool") {
                Ok(c) => c,
                Err(e) => return Err(Diagnostic::error(format!("Call failed: {:?}", e), SourceSpan::default())),
            };
            Ok(bool_call.try_as_basic_value().unwrap_basic())
        }
        _ => Ok(left),
    }
}

/// Infer the result type of a binary operation.
fn infer_binop_type<'ctx>(
    op: MirBinOp,
    left: Option<&TypedValue<'ctx>>,
    right: Option<&TypedValue<'ctx>>,
) -> RubyType {
    use MirBinOp::*;
    
    match op {
        Add | Sub | Mul | Div | Mod => {
            match (left.map(|v| v.ruby_type()), right.map(|v| v.ruby_type())) {
                (Some(RubyType::Integer), Some(RubyType::Integer)) => RubyType::Integer,
                (Some(RubyType::Float), _) | (_, Some(RubyType::Float)) => RubyType::Float,
                (Some(RubyType::String), _) | (_, Some(RubyType::String)) => RubyType::String,
                _ => RubyType::Unknown,
            }
        }
        Eq | Lt | Gt | LtEq | GtEq => RubyType::Boolean,
        And | Or => RubyType::Boolean,
        _ => RubyType::Unknown,
    }
}

/// Emit a terminator instruction.
fn emit_terminator<'ctx>(
    term: &MirTerminator,
    builder: &Builder<'ctx>,
    func_codegen: &mut FunctionCodegen<'ctx, '_>,
    constant_table: &ConstantTable<'ctx, '_>,
    ctx: &'ctx Context,
    _function: FunctionValue<'ctx>,
) -> Result<(), Vec<Diagnostic>> {
    use MirTerminator::*;

    match term {
        Return(Some(reg)) => {
            let value = func_codegen.get_register_or_nil(*reg, constant_table).llvm_value();
            builder.build_return(Some(&value))
                .map_err(|e| vec![Diagnostic::error(format!("Return failed: {:?}", e), SourceSpan::default())])?;
        }
        Return(None) => {
            let nil_val = ctx.i64_type().const_int(0x04, false);
            builder.build_return(Some(&nil_val))
                .map_err(|e| vec![Diagnostic::error(format!("Return failed: {:?}", e), SourceSpan::default())])?;
        }
        Branch(target) => {
            let target_block = func_codegen.get_block(target)
                .ok_or_else(|| vec![Diagnostic::error(format!("Block {} not found", target), SourceSpan::default())])?;
            builder.build_unconditional_branch(target_block)
                .map_err(|e| vec![Diagnostic::error(format!("Branch failed: {:?}", e), SourceSpan::default())])?;
        }
        CondBranch(cond, then_target, else_target) => {
            let cond_i64 = func_codegen.get_register_or_nil(*cond, constant_table).llvm_value().into_int_value();
            // Convert i64 Ruby value to i1 boolean by comparing with 0 (Qfalse)
            let zero = ctx.i64_type().const_int(0, false);
            let cond_i1 = builder.build_int_compare(
                inkwell::IntPredicate::NE,
                cond_i64,
                zero,
                "cond_bool"
            ).map_err(|e| vec![Diagnostic::error(format!("Compare failed: {:?}", e), SourceSpan::default())])?;
            let then_block = func_codegen.get_block(then_target)
                .ok_or_else(|| vec![Diagnostic::error(format!("Block {} not found", then_target), SourceSpan::default())])?;
            let else_block = func_codegen.get_block(else_target)
                .ok_or_else(|| vec![Diagnostic::error(format!("Block {} not found", else_target), SourceSpan::default())])?;
            builder.build_conditional_branch(cond_i1, then_block, else_block)
                .map_err(|e| vec![Diagnostic::error(format!("Conditional branch failed: {:?}", e), SourceSpan::default())])?;
        }
        Unreachable => {
            builder.build_unreachable()
                .map_err(|e| vec![Diagnostic::error(format!("Unreachable failed: {:?}", e), SourceSpan::default())])?;
        }
    }

    Ok(())
}

/// Generate LLVM IR with default configuration.
pub fn generate_ir(module: &MirModule) -> Result<String, Vec<Diagnostic>> {
    let llvm_context = Context::create();
    let mut codegen = CodeGenerator::new(CodegenConfig::default(), &llvm_context);
    codegen.generate(module)
}

/// Generate LLVM module for JIT compilation.
pub fn generate_module<'ctx>(
    module: &MirModule,
    llvm_context: &'ctx Context,
) -> Result<Module<'ctx>, Vec<Diagnostic>> {
    let mut codegen = CodeGenerator::new(CodegenConfig::default(), llvm_context);
    codegen.generate_module(module)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jdruby_mir::{MirFunction, MirBlock, MirInst, MirTerminator, MirConst, MirBinOp};

    fn create_simple_module() -> MirModule {
        MirModule {
            name: "test".to_string(),
            functions: vec![MirFunction {
                name: "main".to_string(),
                params: vec![],
                blocks: vec![MirBlock {
                    label: "entry".to_string(),
                    instructions: vec![
                        MirInst::LoadConst(0, MirConst::Integer(42)),
                        MirInst::Call(1, "jdruby_puts".to_string(), vec![0]),
                    ],
                    terminator: MirTerminator::Return(Some(0)),
                }],
                next_reg: 2,
                span: jdruby_common::SourceSpan::default(),
                captured_vars: vec![],
            }],
        }
    }

    #[test]
    fn test_codegen_new() {
        let llvm_context = Context::create();
        let codegen = CodeGenerator::new(CodegenConfig::default(), &llvm_context);
        assert_eq!(codegen.config.opt_level, OptLevel::O2);
    }

    #[test]
    fn test_generate_simple_module() {
        let module = create_simple_module();
        let result = generate_ir(&module);
        assert!(result.is_ok());
        
        let ir = result.unwrap();
        assert!(ir.contains("; ModuleID = 'test'"));
        assert!(ir.contains("declare i64 @jdruby_int_new(i64)"));
        assert!(ir.contains("declare void @jdruby_puts(i64)"));
        assert!(ir.contains("define i64 @main()"));
    }

    #[test]
    fn test_string_constant_generation() {
        let mut module = create_simple_module();
        module.functions[0].blocks[0].instructions.insert(
            0,
            MirInst::LoadConst(2, MirConst::String("hello".to_string())),
        );
        
        let result = generate_ir(&module);
        assert!(result.is_ok());
        
        let ir = result.unwrap();
        assert!(ir.contains("private") || ir.contains("constant"), "Missing string constant attributes");
        assert!(ir.contains("call i64 @jdruby_str_new"));
    }

    #[test]
    fn test_global_generation() {
        let mut module = create_simple_module();
        module.functions[0].blocks[0].instructions.push(
            MirInst::Load(3, "$global_var".to_string()),
        );
        
        let result = generate_ir(&module);
        assert!(result.is_ok());
        
        let ir = result.unwrap();
        assert!(ir.contains("@_global_"));
    }

    #[test]
    fn test_generate_module_for_jit() {
        let module = create_simple_module();
        let llvm_context = Context::create();
        let result = generate_module(&module, &llvm_context);
        assert!(result.is_ok());
        
        let llvm_module = result.unwrap();
        let main_fn = llvm_module.get_function("main");
        assert!(main_fn.is_some());
    }

    #[test]
    fn test_ir_properly_terminated() {
        let module = create_simple_module();
        let result = generate_ir(&module);
        assert!(result.is_ok());
        
        let ir = result.unwrap();
        assert!(ir.ends_with("}\n"), "IR must end with }}\\n, got: {:?}", &ir[ir.len().saturating_sub(10)..]);
        assert!(!ir.ends_with("\n\n"), "IR has multiple trailing newlines");
    }

    #[test]
    fn test_ir_contains_valid_function_structure() {
        let module = create_simple_module();
        let result = generate_ir(&module);
        assert!(result.is_ok());
        
        let ir = result.unwrap();
        assert!(ir.contains("define i64 @main()"), "Missing function definition");
        assert!(ir.contains("}"), "Missing closing brace");
        
        let open_count = ir.matches('{').count();
        let close_count = ir.matches('}').count();
        assert_eq!(open_count, close_count, "Unbalanced braces: {} open, {} close", open_count, close_count);
    }

    #[test]
    fn test_ir_opaque_pointer_syntax() {
        let module = create_simple_module();
        let result = generate_ir(&module);
        assert!(result.is_ok());
        
        let ir = result.unwrap();
        if ir.contains("i8*") || ir.contains("i64*") {
            panic!("IR contains typed pointer syntax (i8*/i64*) instead of opaque pointers (ptr)");
        }
    }

    #[test]
    fn test_ir_global_declarations() {
        let mut module = create_simple_module();
        module.functions[0].blocks[0].instructions.push(
            MirInst::Load(4, "GlobalVar".to_string()),
        );
        
        let result = generate_ir(&module);
        assert!(result.is_ok());
        
        let ir = result.unwrap();
        assert!(ir.contains("@GlobalVar"), "Missing global variable declaration");
    }

    #[test]
    fn test_ir_runtime_function_declarations() {
        let module = create_simple_module();
        let result = generate_ir(&module);
        assert!(result.is_ok());
        
        let ir = result.unwrap();
        assert!(ir.contains("declare i64 @jdruby_int_new"), "Missing runtime function declaration");
        assert!(ir.contains("declare void @jdruby_puts"), "Missing puts declaration");
    }

    #[test]
    fn test_ir_no_duplicate_newlines_in_headers() {
        let module = create_simple_module();
        let result = generate_ir(&module);
        assert!(result.is_ok());
        
        let ir = result.unwrap();
        let header_end = ir.find("declare").unwrap_or(ir.len());
        let header = &ir[..header_end];
        let double_newlines = header.matches("\n\n").count();
        assert!(double_newlines <= 2, "Too many blank lines in IR header: {}", double_newlines);
    }

    #[test]
    fn test_ir_module_id_present() {
        let module = create_simple_module();
        let result = generate_ir(&module);
        assert!(result.is_ok());
        
        let ir = result.unwrap();
        assert!(ir.contains("; ModuleID = 'test'"), "Missing ModuleID comment");
        assert!(ir.contains("source_filename"), "Missing source_filename");
    }

    #[test]
    fn test_ir_function_has_terminator() {
        let module = create_simple_module();
        let result = generate_ir(&module);
        assert!(result.is_ok());
        
        let ir = result.unwrap();
        let main_start = ir.find("define i64 @main()").expect("main function not found");
        let main_end = ir[main_start..].find("}\n").map(|i| main_start + i).expect("main function closing brace not found");
        let main_body = &ir[main_start..main_end];
        
        assert!(main_body.contains("ret "), "main function missing ret terminator");
    }

    #[test]
    fn test_ir_string_constant_format() {
        let mut module = create_simple_module();
        module.functions[0].blocks[0].instructions.insert(
            0,
            MirInst::LoadConst(2, MirConst::String("test string".to_string())),
        );
        
        let result = generate_ir(&module);
        assert!(result.is_ok());
        
        let ir = result.unwrap();
        assert!(ir.contains("private") || ir.contains("constant"), "Missing string constant attributes");
        assert!(ir.contains("test string") || ir.contains("[6 x i8]"), "Missing string constant content");
    }

    #[test]
    fn test_ir_basic_block_labels() {
        let module = create_simple_module();
        let result = generate_ir(&module);
        assert!(result.is_ok());
        
        let ir = result.unwrap();
        assert!(ir.contains(":") && (ir.contains("entry") || ir.contains("define")), "Missing basic block labels");
    }

    #[test]
    fn test_complex_module_with_multiple_functions() {
        let mut module = create_simple_module();
        module.functions.push(MirFunction {
            name: "helper".to_string(),
            params: vec![10],
            blocks: vec![MirBlock {
                label: "entry".to_string(),
                instructions: vec![
                    MirInst::LoadConst(11, MirConst::Integer(100)),
                    MirInst::BinOp(12, MirBinOp::Add, 10, 11),
                ],
                terminator: MirTerminator::Return(Some(12)),
            }],
            next_reg: 13,
            span: jdruby_common::SourceSpan::default(),
            captured_vars: vec![],
        });
        
        let result = generate_ir(&module);
        assert!(result.is_ok(), "Failed to generate IR for multi-function module");
        
        let ir = result.unwrap();
        
        assert!(ir.contains("define i64 @main()"), "Missing main function");
        assert!(ir.contains("define i64 @helper(i64 %0)"), "Missing helper function");
        
        let open_count = ir.matches('{').count();
        let close_count = ir.matches('}').count();
        assert_eq!(open_count, close_count, "Unbalanced braces in multi-function module");
    }

    #[test]
    fn test_class_definition_ir() {
        let mut module = create_simple_module();
        module.functions[0].blocks[0].instructions.insert(
            0,
            MirInst::ClassNew(10, "TestClass".to_string(), Some("Object".to_string())),
        );
        
        let result = generate_ir(&module);
        assert!(result.is_ok(), "Failed to generate IR for class definition");
        
        let ir = result.unwrap();
        assert!(ir.contains("TestClass"), "Missing class name in IR");
        assert!(ir.contains("jdruby_class_new"), "Missing class_new call");
    }

    #[test]
    fn test_method_definition_ir() {
        let mut module = create_simple_module();
        module.functions[0].blocks[0].instructions.insert(
            0,
            MirInst::ClassNew(10, "MyClass".to_string(), None),
        );
        module.functions[0].blocks[0].instructions.insert(
            1,
            MirInst::DefMethod(10, "test_method".to_string(), "MyClass#test_method".to_string()),
        );
        
        let result = generate_ir(&module);
        assert!(result.is_ok(), "Failed to generate IR for method definition");
        
        let ir = result.unwrap();
        assert!(ir.contains("test_method"), "Missing method name in IR");
        assert!(ir.contains("jdruby_def_method"), "Missing def_method call");
    }

    #[test]
    fn test_module_include_ir() {
        let mut module = create_simple_module();
        module.functions[0].blocks[0].instructions.insert(
            0,
            MirInst::ClassNew(10, "MyClass".to_string(), None),
        );
        module.functions[0].blocks[0].instructions.insert(
            1,
            MirInst::IncludeModule(10, "Enumerable".to_string()),
        );
        
        let result = generate_ir(&module);
        assert!(result.is_ok(), "Failed to generate IR for module include");
        
        let ir = result.unwrap();
        assert!(ir.contains("Enumerable"), "Missing module name in IR");
    }

    #[test]
    fn test_multiple_string_constants_ir() {
        let mut module = create_simple_module();
        module.functions[0].blocks[0].instructions.insert(
            0,
            MirInst::LoadConst(10, MirConst::String("first".to_string())),
        );
        module.functions[0].blocks[0].instructions.insert(
            1,
            MirInst::LoadConst(11, MirConst::String("second".to_string())),
        );
        module.functions[0].blocks[0].instructions.insert(
            2,
            MirInst::Call(12, "puts".to_string(), vec![10]),
        );
        
        let result = generate_ir(&module);
        assert!(result.is_ok(), "Failed to generate IR for multiple strings");
        
        let ir = result.unwrap();
        assert!(ir.contains("first"), "Missing first string constant");
        assert!(ir.contains("second"), "Missing second string constant");
        let str_new_count = ir.matches("jdruby_str_new").count();
        assert!(str_new_count >= 2, "Expected at least 2 str_new calls, found {}", str_new_count);
    }

    #[test]
    fn test_global_variable_ir() {
        let mut module = create_simple_module();
        module.functions[0].blocks[0].instructions.insert(
            0,
            MirInst::LoadConst(10, MirConst::Integer(42)),
        );
        module.functions[0].blocks[0].instructions.insert(
            1,
            MirInst::Store("$global_var".to_string(), 10),
        );
        module.functions[0].blocks[0].instructions.insert(
            2,
            MirInst::Load(11, "$global_var".to_string()),
        );
        
        let result = generate_ir(&module);
        assert!(result.is_ok(), "Failed to generate IR for global variable");
        
        let ir = result.unwrap();
        assert!(ir.contains("global_var"), "Missing global variable in IR");
    }

    #[test]
    fn test_method_call_with_self_ir() {
        let mut module = create_simple_module();
        module.functions[0].params.push(100);
        module.functions[0].blocks[0].instructions.insert(
            0,
            MirInst::LoadConst(10, MirConst::String("test".to_string())),
        );
        module.functions[0].blocks[0].instructions.insert(
            1,
            MirInst::MethodCall(11, 100, "puts".to_string(), vec![10]),
        );
        
        let result = generate_ir(&module);
        assert!(result.is_ok(), "Failed to generate IR for method call with self");
        
        let ir = result.unwrap();
        assert!(ir.contains("jdruby_send"), "Missing jdruby_send call");
    }

    #[test]
    fn test_ir_valid_module_structure() {
        let module = create_simple_module();
        let result = generate_ir(&module);
        assert!(result.is_ok());
        
        let ir = result.unwrap();
        
        assert!(ir.starts_with("; ModuleID = "), "IR must start with ModuleID");
        assert!(ir.contains("source_filename"), "Missing source_filename");
        assert!(ir.contains("target triple"), "Missing target triple");
        
        let first_declare = ir.find("declare").unwrap_or(0);
        let first_define = ir.find("define").unwrap_or(ir.len());
        assert!(first_declare < first_define, "Declarations must come before definitions");
        
        assert!(ir.trim_end().ends_with("}"), "IR must end with closing brace");
    }

    #[test]
    fn test_nested_function_calls_ir() {
        let mut module = create_simple_module();
        module.functions[0].blocks[0].instructions = vec![
            MirInst::LoadConst(10, MirConst::Integer(1)),
            MirInst::LoadConst(11, MirConst::Integer(2)),
            MirInst::BinOp(12, MirBinOp::Add, 10, 11),
            MirInst::Call(13, "puts".to_string(), vec![12]),
        ];
        
        let result = generate_ir(&module);
        assert!(result.is_ok(), "Failed to generate IR for nested calls");
        
        let ir = result.unwrap();
        assert!(ir.contains("jdruby_int_add"), "Missing int_add for nested expression");
        assert!(ir.contains("jdruby_puts"), "Missing puts call");
    }

    #[test]
    fn test_boolean_constants_ir() {
        let mut module = create_simple_module();
        module.functions[0].blocks[0].instructions = vec![
            MirInst::LoadConst(10, MirConst::Bool(true)),
            MirInst::LoadConst(11, MirConst::Bool(false)),
        ];
        module.functions[0].blocks[0].terminator = MirTerminator::Return(None);
        
        let result = generate_ir(&module);
        assert!(result.is_ok(), "Failed to generate IR for boolean constants");
        
        let ir = result.unwrap();
        assert!(ir.contains("JDRUBY_TRUE"), "Missing JDRUBY_TRUE");
        assert!(ir.contains("JDRUBY_FALSE"), "Missing JDRUBY_FALSE");
    }

    #[test]
    fn test_nil_constant_ir() {
        let mut module = create_simple_module();
        module.functions[0].blocks[0].instructions = vec![
            MirInst::LoadConst(10, MirConst::Nil),
        ];
        module.functions[0].blocks[0].terminator = MirTerminator::Return(Some(10));
        
        let result = generate_ir(&module);
        assert!(result.is_ok(), "Failed to generate IR for nil constant");
        
        let ir = result.unwrap();
        assert!(ir.contains("JDRUBY_NIL"), "Missing JDRUBY_NIL");
    }

    #[test]
    fn test_comparison_operations_ir() {
        let mut module = create_simple_module();
        module.functions[0].blocks[0].instructions = vec![
            MirInst::LoadConst(10, MirConst::Integer(5)),
            MirInst::LoadConst(11, MirConst::Integer(3)),
            MirInst::BinOp(12, MirBinOp::Eq, 10, 11),
            MirInst::BinOp(13, MirBinOp::Lt, 10, 11),
            MirInst::BinOp(14, MirBinOp::Gt, 10, 11),
        ];
        module.functions[0].blocks[0].terminator = MirTerminator::Return(None);
        
        let result = generate_ir(&module);
        assert!(result.is_ok(), "Failed to generate IR for comparisons");
        
        let ir = result.unwrap();
        assert!(ir.contains("jdruby_eq"), "Missing eq comparison");
        assert!(ir.contains("jdruby_lt"), "Missing lt comparison");
        assert!(ir.contains("jdruby_gt"), "Missing gt comparison");
    }

    #[test]
    fn test_arithmetic_operations_ir() {
        let mut module = create_simple_module();
        module.functions[0].blocks[0].instructions = vec![
            MirInst::LoadConst(10, MirConst::Integer(10)),
            MirInst::LoadConst(11, MirConst::Integer(3)),
            MirInst::BinOp(12, MirBinOp::Add, 10, 11),
            MirInst::BinOp(13, MirBinOp::Sub, 12, 11),
            MirInst::BinOp(14, MirBinOp::Mul, 13, 11),
            MirInst::BinOp(15, MirBinOp::Div, 14, 11),
        ];
        module.functions[0].blocks[0].terminator = MirTerminator::Return(Some(15));
        
        let result = generate_ir(&module);
        assert!(result.is_ok(), "Failed to generate IR for arithmetic");
        
        let ir = result.unwrap();
        assert!(ir.contains("jdruby_int_add"), "Missing int_add");
        assert!(ir.contains("jdruby_int_sub"), "Missing int_sub");
        assert!(ir.contains("jdruby_int_mul"), "Missing int_mul");
        assert!(ir.contains("jdruby_int_div"), "Missing int_div");
    }
}

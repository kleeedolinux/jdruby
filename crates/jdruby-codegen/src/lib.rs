//! # JDRuby Codegen — LLVM IR Code Generation using Inkwell
//!
//! Translates MIR to LLVM IR for native compilation using the real JDRuby runtime.

pub mod context;
pub mod instructions;
pub mod runtime;
pub mod utils;

use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::targets::{TargetMachine, TargetTriple};
use context::CodegenContext;
use jdruby_common::{Diagnostic, ErrorReporter};
use jdruby_mir::MirModule;

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
            target_triple: TargetMachine::get_default_triple().to_string(),
            opt_level: OptLevel::O2,
            debug_info: false,
            output_format: OutputFormat::LlvmIr,
        }
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
    pub fn generate_with_errors(&mut self, module: &MirModule) -> (String, jdruby_common::ErrorReporter) {
        self.context.clear();
        self.context.set_module_name(&module.name);

        // Prescan functions to collect string constants
        for func in &module.functions {
            self.context.prescan_function(func);
        }

        let mut reporter = ErrorReporter::new();

        // Create Inkwell module
        let llvm_module = self.llvm_context.create_module(&module.name);
        
        // Set target triple
        let target_triple = TargetTriple::create(&self.config.target_triple);
        llvm_module.set_triple(&target_triple);

        // Create builder
        let builder = self.llvm_context.create_builder();

        // Emit runtime declarations
        runtime::emit_runtime_decls(self.llvm_context, &llvm_module);

        // Emit all functions
        for func in &module.functions {
            if let Err(diagnostics) = instructions::emit_function(
                func,
                &self.context,
                self.llvm_context,
                &llvm_module,
                &builder,
            ) {
                for diag in diagnostics {
                    reporter.report_diagnostic(diag);
                }
            }
        }

        // Collect any context errors
        if self.context.has_errors() {
            for diag in self.context.take_diagnostics() {
                reporter.report_diagnostic(diag);
            }
        }

        // Get output as string
        let output = llvm_module.print_to_string().to_string();
        (output, reporter)
    }

    /// Generate LLVM module for JIT compilation (returns the module directly).
    pub fn generate_module(&mut self, module: &MirModule) -> Result<Module<'ctx>, Vec<Diagnostic>> {
        self.context.clear();
        self.context.set_module_name(&module.name);

        // Prescan functions
        for func in &module.functions {
            self.context.prescan_function(func);
        }

        // Create Inkwell module
        let llvm_module = self.llvm_context.create_module(&module.name);
        
        // Set target triple
        let target_triple = TargetTriple::create(&self.config.target_triple);
        llvm_module.set_triple(&target_triple);

        // Create builder
        let builder = self.llvm_context.create_builder();

        // Emit runtime declarations
        runtime::emit_runtime_decls(self.llvm_context, &llvm_module);

        // Emit all functions
        for func in &module.functions {
            if let Err(diagnostics) = instructions::emit_function(
                func,
                &self.context,
                self.llvm_context,
                &llvm_module,
                &builder,
            ) {
                return Err(diagnostics);
            }
        }

        // Check for context errors
        if self.context.has_errors() {
            return Err(self.context.take_diagnostics());
        }

        // Verify the module
        if let Err(err) = llvm_module.verify() {
            return Err(vec![Diagnostic::error(
                format!("Module verification failed: {}", err),
                jdruby_common::SourceSpan::default(),
            )]);
        }

        Ok(llvm_module)
    }
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
    use jdruby_mir::{MirFunction, MirBlock, MirInst, MirConst, MirTerminator};

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
                        MirInst::Call(1, "puts".to_string(), vec![0]),
                    ],
                    terminator: MirTerminator::Return(Some(0)),
                }],
                next_reg: 2,
                span: jdruby_common::SourceSpan::default(),
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
        // Inkwell generates slightly different constant names
        assert!(ir.contains("private unnamed_addr constant"));
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
        // Inkwell generates globals differently
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
}

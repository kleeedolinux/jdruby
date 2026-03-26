//! # JDRuby Codegen — LLVM IR Code Generation
//!
//! Translates MIR to LLVM IR for native compilation using the real JDRuby runtime.

pub mod context;
pub mod instructions;
pub mod runtime;
pub mod utils;

use context::CodegenContext;
use jdruby_common::Diagnostic;
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
            target_triple: "x86_64-unknown-linux-gnu".into(),
            opt_level: OptLevel::O2,
            debug_info: false,
            output_format: OutputFormat::LlvmIr,
        }
    }
}

/// Main code generator for LLVM IR.
pub struct CodeGenerator {
    config: CodegenConfig,
    context: CodegenContext,
}

impl CodeGenerator {
    pub fn new(config: CodegenConfig) -> Self {
        Self {
            context: CodegenContext::new(),
            config,
        }
    }

    /// Generate LLVM IR from a MIR module.
    pub fn generate(&mut self, module: &MirModule) -> Result<String, Vec<Diagnostic>> {
        self.context.clear();
        self.context.set_module_name(&module.name);

        for func in &module.functions {
            self.context.prescan_function(func);
        }

        let mut output = String::with_capacity(16384);

        self.emit_header(&mut output);
        self.emit_data_section(&mut output);
        runtime::emit_runtime_decls(&mut output);

        for func in &module.functions {
            instructions::emit_function(func, &self.context, &mut output)?;
        }

        if self.context.has_errors() {
            Err(self.context.take_diagnostics())
        } else {
            Ok(output)
        }
    }

    fn emit_header(&self, out: &mut String) {
        use std::fmt::Write;

        let _ = writeln!(out, "; ModuleID = '{}'", self.context.module_name());
        let _ = writeln!(out, "source_filename = \"{}\"", self.context.module_name());
        let _ = writeln!(out, "target triple = \"{}\"", self.config.target_triple);
        let _ = writeln!(out);
    }

    fn emit_data_section(&self, out: &mut String) {
        let strings = self.context.get_string_constants();
        if !strings.is_empty() {
            out.push_str(&strings);
            out.push('\n');
        }

        let globals = self.context.get_global_decls();
        if !globals.is_empty() {
            out.push_str(&globals);
            out.push('\n');
        }
    }
}

/// Generate LLVM IR with default configuration.
pub fn generate_ir(module: &MirModule) -> Result<String, Vec<Diagnostic>> {
    let mut codegen = CodeGenerator::new(CodegenConfig::default());
    codegen.generate(module)
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
        let codegen = CodeGenerator::new(CodegenConfig::default());
        assert_eq!(codegen.config.opt_level, OptLevel::O2);
        assert_eq!(codegen.config.target_triple, "x86_64-unknown-linux-gnu");
    }

    #[test]
    fn test_generate_simple_module() {
        let module = create_simple_module();
        let result = generate_ir(&module);
        assert!(result.is_ok());
        
        let ir = result.unwrap();
        assert!(ir.contains("ModuleID = 'test'"));
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
        assert!(ir.contains("@.str.0 = private unnamed_addr constant"));
        assert!(ir.contains("c\"hello\\00\""));
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
        assert!(ir.contains("@_global_global_var = internal global i64 0"));
    }
}

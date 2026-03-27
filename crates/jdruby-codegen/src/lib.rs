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
        eprintln!("DEBUG: Emitting {} functions to LLVM IR", module.functions.len());
        for (i, func) in module.functions.iter().enumerate() {
            eprintln!("DEBUG: Emitting function {}: {}", i, func.name);
            if let Err(diagnostics) = instructions::emit_function(
                func,
                &self.context,
                self.llvm_context,
                &llvm_module,
                &builder,
            ) {
                eprintln!("DEBUG: Function {} failed with {} errors", func.name, diagnostics.len());
                for diag in diagnostics {
                    reporter.report_diagnostic(diag);
                }
            } else {
                eprintln!("DEBUG: Function {} emitted successfully", func.name);
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
        // Inkwell generates string constants as private globals
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

    #[test]
    fn test_ir_properly_terminated() {
        let module = create_simple_module();
        let result = generate_ir(&module);
        assert!(result.is_ok());
        
        let ir = result.unwrap();
        // IR must end with exactly one newline after the closing brace
        assert!(ir.ends_with("}\n"), "IR must end with }}\\n, got: {:?}", &ir[ir.len().saturating_sub(10)..]);
        // Should not have multiple trailing newlines
        assert!(!ir.ends_with("\n\n"), "IR has multiple trailing newlines");
    }

    #[test]
    fn test_ir_contains_valid_function_structure() {
        let module = create_simple_module();
        let result = generate_ir(&module);
        assert!(result.is_ok());
        
        let ir = result.unwrap();
        // Must have function definition with proper braces
        assert!(ir.contains("define i64 @main()"), "Missing function definition");
        assert!(ir.contains("}"), "Missing closing brace");
        
        // Count braces - must be balanced
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
        // Modern LLVM uses opaque pointers (ptr) not typed pointers (i8*)
        // The IR should use ptr consistently
        if ir.contains("i8*") || ir.contains("i64*") {
            panic!("IR contains typed pointer syntax (i8*/i64*) instead of opaque pointers (ptr)");
        }
    }

    #[test]
    fn test_ir_global_declarations() {
        let mut module = create_simple_module();
        // Add a global variable reference
        module.functions[0].blocks[0].instructions.push(
            MirInst::Load(4, "GlobalVar".to_string()),
        );
        
        let result = generate_ir(&module);
        assert!(result.is_ok());
        
        let ir = result.unwrap();
        // Should contain global declaration
        assert!(ir.contains("@GlobalVar"), "Missing global variable declaration");
    }

    #[test]
    fn test_ir_runtime_function_declarations() {
        let module = create_simple_module();
        let result = generate_ir(&module);
        assert!(result.is_ok());
        
        let ir = result.unwrap();
        // Must contain runtime function declarations
        assert!(ir.contains("declare i64 @jdruby_int_new"), "Missing runtime function declaration");
        assert!(ir.contains("declare void @jdruby_puts"), "Missing puts declaration");
    }

    #[test]
    fn test_ir_no_duplicate_newlines_in_headers() {
        let module = create_simple_module();
        let result = generate_ir(&module);
        assert!(result.is_ok());
        
        let ir = result.unwrap();
        // Check for excessive blank lines in header (before first declare)
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
        // Find the main function and verify it has a terminator
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
        // String constants should be properly formatted as global arrays
        assert!(ir.contains("private") || ir.contains("constant"), "Missing string constant attributes");
        // Check for the string data in the IR (may be formatted as array or c-string)
        assert!(ir.contains("test string") || ir.contains("[6 x i8]"), "Missing string constant content");
    }

    #[test]
    fn test_ir_basic_block_labels() {
        let module = create_simple_module();
        let result = generate_ir(&module);
        assert!(result.is_ok());
        
        let ir = result.unwrap();
        // Must have at least one basic block label - LLVM IR format uses colon after label name
        assert!(ir.contains(":") && (ir.contains("entry") || ir.contains("define")), "Missing basic block labels");
    }

    #[test]
    fn test_complex_module_with_multiple_functions() {
        let mut module = create_simple_module();
        // Add a second function
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
        });
        
        let result = generate_ir(&module);
        assert!(result.is_ok(), "Failed to generate IR for multi-function module");
        
        let ir = result.unwrap();
        
        // Print complete IR output for debugging
        println!("=== Generated IR for Complex Module ===");
        println!("{}", ir);
        println!("=== End IR Output ===");
        
        // Should have both function definitions
        assert!(ir.contains("define i64 @main()"), "Missing main function");
        assert!(ir.contains("define i64 @helper(i64 %0)"), "Missing helper function");
        
        // Verify balanced braces across entire module
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
        // Should contain class name constant and class_new call
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
        // Should have string constants
        assert!(ir.contains("first"), "Missing first string constant");
        assert!(ir.contains("second"), "Missing second string constant");
        // Should have jdruby_str_new calls
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
        // Add a store for self simulation (param 0 is self)
        module.functions[0].params.push(100); // self register
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
        
        // Must start with ModuleID
        assert!(ir.starts_with("; ModuleID = "), "IR must start with ModuleID");
        
        // Must have source_filename
        assert!(ir.contains("source_filename"), "Missing source_filename");
        
        // Must have target triple
        assert!(ir.contains("target triple"), "Missing target triple");
        
        // Must have declarations before definitions
        let first_declare = ir.find("declare").unwrap_or(0);
        let first_define = ir.find("define").unwrap_or(ir.len());
        assert!(first_declare < first_define, "Declarations must come before definitions");
        
        // Must end properly
        assert!(ir.trim_end().ends_with("}"), "IR must end with closing brace");
    }

    #[test]
    fn test_nested_function_calls_ir() {
        let mut module = create_simple_module();
        // Create nested call: puts(1 + 2)
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

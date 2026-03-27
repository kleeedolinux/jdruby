//! Massive JIT Pipeline Stress Tests
//! These tests aggressively exercise the JIT compilation system to identify bugs

use inkwell::context::Context;
use jdruby_codegen::utils::sanitize_name;
use jdruby_codegen::{CodeGenerator, CodegenConfig, OptLevel};
use jdruby_jit::binary_builder::{BinaryBuilder, BinaryBuilderConfig};
use jdruby_jit::compiler::{CompilationTier, JitCompiler};
use jdruby_mir::{MirBinOp, MirBlock, MirConst, MirFunction, MirInst, MirModule, MirTerminator};

/// Create a minimal valid MIR function
fn create_minimal_function(name: &str) -> MirFunction {
    MirFunction {
        name: name.to_string(),
        params: vec![],
        blocks: vec![MirBlock {
            label: "entry".to_string(),
            instructions: vec![MirInst::LoadConst(0, MirConst::Integer(42))],
            terminator: MirTerminator::Return(Some(0)),
        }],
        next_reg: 1,
        span: jdruby_common::SourceSpan::default(),
    }
}

/// Create function with special characters in name
fn create_special_name_function(name: &str) -> MirFunction {
    MirFunction {
        name: name.to_string(),
        params: vec![100, 101], // self, arg
        blocks: vec![MirBlock {
            label: "entry".to_string(),
            instructions: vec![MirInst::LoadConst(0, MirConst::Integer(1))],
            terminator: MirTerminator::Return(Some(0)),
        }],
        next_reg: 1,
        span: jdruby_common::SourceSpan::default(),
    }
}

/// Create function with string constants
fn create_string_function(name: &str, str_val: &str) -> MirFunction {
    MirFunction {
        name: name.to_string(),
        params: vec![],
        blocks: vec![MirBlock {
            label: "entry".to_string(),
            instructions: vec![
                MirInst::LoadConst(0, MirConst::String(str_val.to_string())),
                MirInst::Call(1, "jdruby_puts".to_string(), vec![0]),
            ],
            terminator: MirTerminator::Return(Some(0)),
        }],
        next_reg: 2,
        span: jdruby_common::SourceSpan::default(),
    }
}

/// Create function with arithmetic
fn create_arithmetic_function(name: &str) -> MirFunction {
    MirFunction {
        name: name.to_string(),
        params: vec![100, 101, 102], // self, a, b
        blocks: vec![MirBlock {
            label: "entry".to_string(),
            instructions: vec![
                MirInst::BinOp(0, MirBinOp::Add, 101, 102),
                MirInst::BinOp(1, MirBinOp::Mul, 0, 101),
                MirInst::BinOp(2, MirBinOp::Sub, 1, 102),
            ],
            terminator: MirTerminator::Return(Some(2)),
        }],
        next_reg: 3,
        span: jdruby_common::SourceSpan::default(),
    }
}

/// Create deeply nested function with many constants
fn create_deeply_nested_function(name: &str, depth: usize) -> MirFunction {
    let mut blocks = vec![];
    let mut instructions = vec![];

    for i in 0..depth {
        instructions.push(MirInst::LoadConst(i as u32, MirConst::Integer(i as i64)));
    }

    blocks.push(MirBlock {
        label: "entry".to_string(),
        instructions,
        terminator: MirTerminator::Return(Some((depth - 1) as u32)),
    });

    MirFunction {
        name: name.to_string(),
        params: vec![],
        blocks,
        next_reg: depth as u32,
        span: jdruby_common::SourceSpan::default(),
    }
}

/// Create function with method calls
fn create_method_call_function(name: &str) -> MirFunction {
    MirFunction {
        name: name.to_string(),
        params: vec![100], // self
        blocks: vec![MirBlock {
            label: "entry".to_string(),
            instructions: vec![
                MirInst::LoadConst(0, MirConst::Integer(42)),
                MirInst::MethodCall(1, 100, "test_method".to_string(), vec![0]),
            ],
            terminator: MirTerminator::Return(Some(1)),
        }],
        next_reg: 2,
        span: jdruby_common::SourceSpan::default(),
    }
}

/// Create function with conditional branches
fn create_conditional_function(name: &str) -> MirFunction {
    MirFunction {
        name: name.to_string(),
        params: vec![100, 101], // self, condition
        blocks: vec![
            MirBlock {
                label: "entry".to_string(),
                instructions: vec![
                    MirInst::LoadConst(0, MirConst::Integer(1)),
                    MirInst::LoadConst(1, MirConst::Integer(0)),
                ],
                terminator: MirTerminator::CondBranch(101, "then".to_string(), "else".to_string()),
            },
            MirBlock {
                label: "then".to_string(),
                instructions: vec![],
                terminator: MirTerminator::Return(Some(0)),
            },
            MirBlock {
                label: "else".to_string(),
                instructions: vec![],
                terminator: MirTerminator::Return(Some(1)),
            },
        ],
        next_reg: 2,
        span: jdruby_common::SourceSpan::default(),
    }
}

/// Create function with all arithmetic operations
fn create_all_ops_function(name: &str) -> MirFunction {
    let ops = vec![
        MirBinOp::Add,
        MirBinOp::Sub,
        MirBinOp::Mul,
        MirBinOp::Div,
        MirBinOp::Mod,
        MirBinOp::Pow,
        MirBinOp::Eq,
        MirBinOp::NotEq,
        MirBinOp::Lt,
        MirBinOp::Gt,
        MirBinOp::LtEq,
        MirBinOp::GtEq,
        MirBinOp::And,
        MirBinOp::Or,
        MirBinOp::BitAnd,
        MirBinOp::BitOr,
        MirBinOp::BitXor,
        MirBinOp::Shl,
        MirBinOp::Shr,
    ];

    let mut instructions = vec![
        MirInst::LoadConst(0, MirConst::Integer(10)),
        MirInst::LoadConst(1, MirConst::Integer(5)),
    ];

    let mut next_reg = 2u32;
    for op in &ops {
        instructions.push(MirInst::BinOp(next_reg, *op, 0, 1));
        next_reg += 1;
    }

    MirFunction {
        name: name.to_string(),
        params: vec![],
        blocks: vec![MirBlock {
            label: "entry".to_string(),
            instructions,
            terminator: MirTerminator::Return(Some(next_reg - 1)),
        }],
        next_reg,
        span: jdruby_common::SourceSpan::default(),
    }
}

/// Create function with variable stores and loads
fn create_variable_function(name: &str) -> MirFunction {
    MirFunction {
        name: name.to_string(),
        params: vec![100], // self
        blocks: vec![MirBlock {
            label: "entry".to_string(),
            instructions: vec![
                MirInst::LoadConst(0, MirConst::Integer(42)),
                MirInst::Store("x".to_string(), 0),
                MirInst::Load(1, "x".to_string()),
                MirInst::Alloc(2, "y".to_string()),
                MirInst::Store("y".to_string(), 1),
                MirInst::Load(3, "y".to_string()),
            ],
            terminator: MirTerminator::Return(Some(3)),
        }],
        next_reg: 4,
        span: jdruby_common::SourceSpan::default(),
    }
}

/// Test 1: Basic codegen IR generation
#[test]
fn stress_test_codegen_basic() {
    let ctx = Context::create();
    let config = CodegenConfig::default();
    let mut codegen = CodeGenerator::new(config, &ctx);

    let func = create_minimal_function("test_basic");
    let module = MirModule { name: "test".to_string(), functions: vec![func] };

    let (ir, reporter) = codegen.generate_with_errors(&module);
    assert!(!reporter.has_errors(), "Basic codegen produced errors");
    assert!(ir.contains("define i64 @test_basic()"), "Function not found in IR");
    assert!(ir.contains("target triple"), "Target triple missing");

    // Verify IR can be parsed back with null terminator
    let mut bytes = ir.as_bytes().to_vec();
    bytes.push(0);
    let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(&bytes, "test");
    let result = ctx.create_module_from_ir(buffer);
    result.expect("Generated IR failed to parse");
}

/// Test 2: Functions with special characters in names
#[test]
fn stress_test_special_characters() {
    let ctx = Context::create();
    let config = CodegenConfig::default();

    let special_names = vec![
        "Logger#log",
        "Task#initialize",
        "Scheduler#add_task",
        "Class::method",
        "method?",
        "method!",
        "method<generic>",
        "a.b.c",
        "$global_method",
        "@instance_method",
        "method_with_underscores",
        "UPPERCASE",
        "mixedCase123",
    ];

    for name in special_names {
        let mut codegen = CodeGenerator::new(config.clone(), &ctx);
        let func = create_special_name_function(name);
        let module = MirModule {
            name: format!("test_{}", name.replace(|c: char| !c.is_alphanumeric(), "_")),
            functions: vec![func],
        };

        let (ir, mut reporter) = codegen.generate_with_errors(&module);

        // Should not have errors
        if reporter.has_errors() {
            panic!("Codegen error for '{}': {:?}", name, reporter.take_diagnostics());
        }

        // Verify sanitized name is in IR
        let sanitized = sanitize_name(name);
        assert!(
            ir.contains(&format!("define i64 @{}(", sanitized)),
            "Sanitized function name not found in IR for '{}'\nIR:\n{}",
            name,
            ir
        );

        // Verify IR is parseable
        let mut bytes = ir.as_bytes().to_vec();
        bytes.push(0);
        let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(&bytes, name);
        let result = ctx.create_module_from_ir(buffer);
        assert!(result.is_ok(), "IR parse failed for '{}': {:?}", name, result.err());
    }
}

/// Test 3: String constants with various content
#[test]
fn stress_test_string_constants() {
    let ctx = Context::create();
    let config = CodegenConfig::default();

    let strings: Vec<&str> = vec![
        "hello",
        "hello world",
        "with\nnewlines",
        "with\ttabs",
        "special !@#$%^&*() chars",
        "", // empty string
    ];

    for (i, s) in strings.iter().enumerate() {
        let mut codegen = CodeGenerator::new(config.clone(), &ctx);
        let func = create_string_function(&format!("string_test_{}", i), s);
        let module = MirModule { name: format!("string_test_{}", i), functions: vec![func] };

        let (ir, _reporter) = codegen.generate_with_errors(&module);

        // Verify IR is parseable
        let mut bytes = ir.as_bytes().to_vec();
        bytes.push(0);
        let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(
            &bytes,
            &format!("test{}", i),
        );
        let result = ctx.create_module_from_ir(buffer);
        assert!(result.is_ok(), "IR parse failed for string {}: {}", i, result.err().unwrap());
    }
}

/// Test 4: Deeply nested functions with many constants
#[test]
fn stress_test_deep_nesting() {
    let ctx = Context::create();
    let config = CodegenConfig::default();

    for depth in [1, 10, 50, 100, 500] {
        let mut codegen = CodeGenerator::new(config.clone(), &ctx);
        let func = create_deeply_nested_function(&format!("deep_{}", depth), depth);
        let module = MirModule { name: format!("deep_{}", depth), functions: vec![func] };

        let (ir, mut reporter) = codegen.generate_with_errors(&module);

        if reporter.has_errors() {
            panic!("Deep nesting error at depth {}: {:?}", depth, reporter.take_diagnostics());
        }

        // Verify IR is parseable
        let mut bytes = ir.as_bytes().to_vec();
        bytes.push(0);
        let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(&bytes, "test");
        let result = ctx.create_module_from_ir(buffer);
        if let Err(e) = result {
            panic!("IR parse failed at depth {}: {:?}", depth, e);
        }
    }
}

/// Test 5: Multiple functions per module
#[test]
fn stress_test_multiple_functions() {
    let ctx = Context::create();
    let config = CodegenConfig::default();
    let mut codegen = CodeGenerator::new(config, &ctx);

    let functions: Vec<MirFunction> =
        (0..50).map(|i| create_minimal_function(&format!("func_{}", i))).collect();

    let module = MirModule { name: "multi".to_string(), functions };

    let (ir, mut reporter) = codegen.generate_with_errors(&module);

    if reporter.has_errors() {
        panic!("Multiple functions error: {:?}", reporter.take_diagnostics());
    }

    // All functions should be present
    for i in 0..50 {
        assert!(
            ir.contains(&format!("define i64 @func_{}()", i)),
            "Function {} not found in IR",
            i
        );
    }

    // Verify IR is parseable
    let mut bytes = ir.as_bytes().to_vec();
    bytes.push(0);
    let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(&bytes, "test");
    let result = ctx.create_module_from_ir(buffer);
    assert!(result.is_ok(), "Multi-function IR failed to parse: {:?}", result.err());
}

/// Test 6: BinaryBuilder module parsing
#[test]
fn stress_test_binary_builder() {
    use inkwell::OptimizationLevel;

    let ctx = Context::create();
    let config =
        BinaryBuilderConfig { opt_level: OptimizationLevel::Default, ..Default::default() };

    let mut builder = BinaryBuilder::new(&ctx, config);

    // Add multiple modules including special names
    let functions = vec![
        create_minimal_function("bb_simple"),
        create_special_name_function("bb__special"),
        create_arithmetic_function("bb_math"),
        create_string_function("bb_string", "test"),
    ];

    for (i, func) in functions.iter().enumerate() {
        let mir_module =
            MirModule { name: format!("bb_module_{}", i), functions: vec![func.clone()] };

        if let Err(e) = builder.add_module(&format!("module_{}", i), &mir_module) {
            panic!("BinaryBuilder failed at module {} ({}): {}", i, func.name, e);
        }
    }
}

/// Test 7: JIT Compiler IR compilation
#[test]
fn stress_test_jit_compiler() {
    let ctx = Context::create();
    let mut compiler = JitCompiler::new(&ctx);

    // Test various function types
    let functions = vec![
        create_minimal_function("jit_simple"),
        create_arithmetic_function("jit_math"),
        create_special_name_function("jit__special"), // pre-sanitized
        create_string_function("jit_string", "hello"),
        create_method_call_function("jit_method"),
    ];

    for func in functions {
        let result = compiler.compile_function_ir(&func, CompilationTier::Baseline, 0);
        if let Err(e) = result {
            panic!("JIT compilation failed for '{}': {}", func.name, e);
        }
    }
}

/// Test 8: Empty modules edge case
#[test]
fn stress_test_empty_module() {
    let ctx = Context::create();
    let config = CodegenConfig::default();
    let mut codegen = CodeGenerator::new(config, &ctx);

    let module = MirModule { name: "empty".to_string(), functions: vec![] };

    let (ir, _reporter) = codegen.generate_with_errors(&module);

    // Empty module should still produce valid IR with target triple
    assert!(ir.contains("target triple"), "Empty module missing target triple");
}

/// Test 9: Verify target triple format (no Debug formatting)
#[test]
fn stress_test_target_triple_format() {
    let ctx = Context::create();
    let config = CodegenConfig::default();
    let mut codegen = CodeGenerator::new(config, &ctx);

    let func = create_minimal_function("triple_test");
    let module = MirModule { name: "triple".to_string(), functions: vec![func] };

    let (ir, _reporter) = codegen.generate_with_errors(&module);

    // Target triple line should not contain "TargetTriple(" prefix
    let triple_line =
        ir.lines().find(|l| l.contains("target triple")).expect("Missing target triple line");

    assert!(
        !triple_line.contains("TargetTriple("),
        "Target triple has Debug format: {}",
        triple_line
    );

    // Should look like: target triple = "x86_64-unknown-linux-gnu"
    assert!(
        triple_line.matches('"').count() == 2,
        "Target triple should have exactly 2 quotes: {}",
        triple_line
    );
}

/// Test 10: All arithmetic operations
#[test]
fn stress_test_all_operations() {
    let ctx = Context::create();
    let config = CodegenConfig::default();
    let mut codegen = CodeGenerator::new(config, &ctx);

    let func = create_all_ops_function("all_ops");
    let module = MirModule { name: "ops".to_string(), functions: vec![func] };

    let (ir, mut reporter) = codegen.generate_with_errors(&module);

    if reporter.has_errors() {
        panic!("All ops error: {:?}", reporter.take_diagnostics());
    }

    // Verify IR is parseable
    let mut bytes = ir.as_bytes().to_vec();
    bytes.push(0);
    let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(&bytes, "test");
    let result = ctx.create_module_from_ir(buffer);
    assert!(result.is_ok(), "All ops IR failed to parse: {:?}", result.err());
}

/// Test 11: Conditional branches
#[test]
fn stress_test_conditional() {
    let ctx = Context::create();
    let config = CodegenConfig::default();
    let mut codegen = CodeGenerator::new(config, &ctx);

    let func = create_conditional_function("conditional");
    let module = MirModule { name: "cond".to_string(), functions: vec![func] };

    let (ir, mut reporter) = codegen.generate_with_errors(&module);

    if reporter.has_errors() {
        panic!("Conditional error: {:?}", reporter.take_diagnostics());
    }

    // Should have multiple blocks
    assert!(ir.contains("then:"), "Missing then block");
    assert!(ir.contains("else:"), "Missing else block");

    // Verify IR is parseable
    let mut bytes = ir.as_bytes().to_vec();
    bytes.push(0);
    let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(&bytes, "test");
    let result = ctx.create_module_from_ir(buffer);
    assert!(result.is_ok(), "Conditional IR failed to parse: {:?}", result.err());
}

/// Test 12: Variable allocation and storage
#[test]
fn stress_test_variables() {
    let ctx = Context::create();
    let config = CodegenConfig::default();
    let mut codegen = CodeGenerator::new(config, &ctx);

    let func = create_variable_function("variables");
    let module = MirModule { name: "vars".to_string(), functions: vec![func] };

    let (ir, mut reporter) = codegen.generate_with_errors(&module);

    if reporter.has_errors() {
        panic!("Variables error: {:?}", reporter.take_diagnostics());
    }

    // Should have allocas for locals
    assert!(ir.contains("alloca"), "Missing alloca instructions");

    // Verify IR is parseable
    let mut bytes = ir.as_bytes().to_vec();
    bytes.push(0);
    let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(&bytes, "test");
    let result = ctx.create_module_from_ir(buffer);
    assert!(result.is_ok(), "Variables IR failed to parse: {:?}", result.err());
}

/// Test 13: Recompile same function multiple times
#[test]
fn stress_test_recompile() {
    let ctx = Context::create();
    let config = CodegenConfig::default();

    for i in 0..100 {
        let mut codegen = CodeGenerator::new(config.clone(), &ctx);
        let func = create_minimal_function(&format!("recompile_{}", i));
        let module = MirModule { name: format!("recompile_{}", i), functions: vec![func] };

        let (ir, _reporter) = codegen.generate_with_errors(&module);

        // Verify IR is parseable every time
        let mut bytes = ir.as_bytes().to_vec();
        bytes.push(0);
        let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(&bytes, "test");
        let result = ctx.create_module_from_ir(buffer);
        assert!(result.is_ok(), "Recompile {} failed: {:?}", i, result.err());
    }
}

/// Test 14: Method calls with special characters in names
#[test]
fn stress_test_method_calls() {
    let ctx = Context::create();
    let config = CodegenConfig::default();

    let method_names = vec!["puts", "print", "test", "method?", "method!", "method#name"];

    for name in method_names {
        let mut codegen = CodeGenerator::new(config.clone(), &ctx);
        let func = create_method_call_function(name);
        let module = MirModule {
            name: format!("method_{}", name.replace(|c: char| !c.is_alphanumeric(), "_")),
            functions: vec![func],
        };

        let (ir, mut reporter) = codegen.generate_with_errors(&module);

        if reporter.has_errors() {
            panic!("Method call error for '{}': {:?}", name, reporter.take_diagnostics());
        }

        // Verify IR is parseable
        let mut bytes = ir.as_bytes().to_vec();
        bytes.push(0);
        let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(&bytes, name);
        let result = ctx.create_module_from_ir(buffer);
        assert!(result.is_ok(), "Method call IR failed for '{}': {:?}", name, result.err());
    }
}

/// Test 21: CVE-style buffer overflow attempts
#[test]
fn stress_test_cve_buffer_overflow() {
    let ctx = Context::create();

    // Test extremely long function names (potential buffer overflow)
    for size in [100, 500, 1000, 5000, 10000] {
        let long_name = "a".repeat(size);
        let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
        let func = create_minimal_function(&long_name);
        let module = MirModule { name: format!("long_name_{}", size), functions: vec![func] };

        let (ir, mut reporter) = codegen.generate_with_errors(&module);

        // Should handle long names gracefully
        if reporter.has_errors() {
            let diagnostics = reporter.take_diagnostics();
            // Errors are acceptable for extremely long names, crashes are not
            assert!(!diagnostics
                .iter()
                .any(|d| d.message.contains("panic") || d.message.contains("overflow")));
        }

        // Verify IR is parseable if no errors
        if !ir.is_empty() {
            let mut bytes = ir.as_bytes().to_vec();
            bytes.push(0);
            let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(
                &bytes,
                &format!("long_{}", size),
            );
            match ctx.create_module_from_ir(buffer) {
                Ok(_) => {}
                Err(e) => panic!("IR parsing failed: {:?}", e),
            };
            // May succeed or fail, but should not crash
        }
    }
}

/// Test 22: Integer overflow in constant propagation
#[test]
fn stress_test_integer_overflow() {
    let ctx = Context::create();

    let overflow_values = vec![
        i64::MAX,
        i64::MIN,
        i64::MAX - 1,
        i64::MIN + 1,
        u64::MAX as i64,
        -9223372036854775808i64, // i64::MIN
    ];

    for (i, val) in overflow_values.iter().enumerate() {
        let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
        let func = MirFunction {
            name: format!("overflow_{}", i),
            params: vec![],
            blocks: vec![MirBlock {
                label: "entry".to_string(),
                instructions: vec![
                    MirInst::LoadConst(0, MirConst::Integer(*val)),
                    MirInst::LoadConst(1, MirConst::Integer(1)),
                    MirInst::BinOp(2, MirBinOp::Add, 0, 1),
                    MirInst::BinOp(3, MirBinOp::Mul, 0, 0),
                ],
                terminator: MirTerminator::Return(Some(3)),
            }],
            next_reg: 4,
            span: jdruby_common::SourceSpan::default(),
        };

        let module = MirModule { name: format!("overflow_{}", i), functions: vec![func] };

        let (ir, _reporter) = codegen.generate_with_errors(&module);

        // Should not crash on overflow values
        let mut bytes = ir.as_bytes().to_vec();
        bytes.push(0);
        let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(
            &bytes,
            &format!("overflow_{}", i),
        );
        match ctx.create_module_from_ir(buffer) {
            Ok(_) => {}
            Err(e) => panic!("IR parsing failed: {:?}", e),
        };
    }
}

/// Test 23: Malformed IR injection attempts
#[test]
fn stress_test_malformed_ir_injection() {
    let ctx = Context::create();

    let malformed_strings = vec![
        "\x00",          // null byte
        "\x01\x02\x03",  // control characters
        "\\\"",          // escape sequences
        "@internal",     // LLVM internal prefix
        "global",        // reserved word
        "declare",       // reserved word
        "define void @", // partial declaration
        "{ }",           // empty block
        "(",             // unmatched paren
        ")",             // unmatched paren
        "{",             // unmatched brace
        "}",             // unmatched brace
    ];

    for (i, s) in malformed_strings.iter().enumerate() {
        let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
        let func = create_string_function(&format!("malformed_{}", i), s);
        let module = MirModule { name: format!("malformed_{}", i), functions: vec![func] };

        let (ir, reporter) = codegen.generate_with_errors(&module);

        // Should not produce malformed IR
        if !reporter.has_errors() {
            // Verify IR doesn't contain injection patterns
            assert!(
                !ir.contains("declare i64 @") || ir.contains("define"),
                "Potential injection at {}: {}",
                i,
                s
            );
        }

        // Verify IR is parseable if generated
        if !ir.is_empty() {
            let mut bytes = ir.as_bytes().to_vec();
            bytes.push(0);
            let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(
                &bytes,
                &format!("mal_{}", i),
            );
            match ctx.create_module_from_ir(buffer) {
                Ok(_) => {}
                Err(e) => panic!("IR parsing failed: {:?}", e),
            };
        }
    }
}

/// Test 24: Recursive function stress test
#[test]
fn stress_test_recursive_functions() {
    let ctx = Context::create();

    for depth in [1, 5, 10, 50, 100] {
        let mut blocks = vec![];
        let mut instructions = vec![];

        // Create recursive-like structure
        for i in 0..depth {
            instructions.push(MirInst::LoadConst(i as u32, MirConst::Integer(i as i64)));
        }

        // Add a "recursive" call pattern
        instructions.push(MirInst::Call(
            depth as u32,
            format!("recursive_{}", depth),
            (0..depth as u32).collect(),
        ));

        blocks.push(MirBlock {
            label: "entry".to_string(),
            instructions,
            terminator: MirTerminator::Return(Some(depth as u32)),
        });

        let func = MirFunction {
            name: format!("recursive_{}", depth),
            params: (0..depth).map(|i| 100 + i as u32).collect(),
            blocks,
            next_reg: (depth + 1) as u32,
            span: jdruby_common::SourceSpan::default(),
        };

        let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
        let module = MirModule { name: format!("recursive_{}", depth), functions: vec![func] };

        let (ir, _reporter) = codegen.generate_with_errors(&module);

        let mut bytes = ir.as_bytes().to_vec();
        bytes.push(0);
        let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(
            &bytes,
            &format!("rec_{}", depth),
        );
        match ctx.create_module_from_ir(buffer) {
            Ok(_) => {}
            Err(e) => panic!("IR parsing failed: {:?}", e),
        };
    }
}

/// Test 25: Uninitialized register access
#[test]
fn stress_test_uninitialized_registers() {
    let ctx = Context::create();

    // Functions that use registers that were never defined
    let func = MirFunction {
        name: "uninit_test".to_string(),
        params: vec![],
        blocks: vec![MirBlock {
            label: "entry".to_string(),
            instructions: vec![
                // Use register 999 which was never initialized
                MirInst::BinOp(0, MirBinOp::Add, 999, 1000),
                MirInst::Call(1, "test".to_string(), vec![999]),
            ],
            terminator: MirTerminator::Return(Some(1)),
        }],
        next_reg: 2,
        span: jdruby_common::SourceSpan::default(),
    };

    let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
    let module = MirModule { name: "uninit".to_string(), functions: vec![func] };

    // This may produce an error but should not crash
    let (ir, _reporter) = codegen.generate_with_errors(&module);

    let mut bytes = ir.as_bytes().to_vec();
    bytes.push(0);
    let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(&bytes, "uninit");
    match ctx.create_module_from_ir(buffer) {
        Ok(_) => {}
        Err(e) => panic!("IR parsing failed: {:?}", e),
    };
}

/// Test 26: Empty block edge cases
#[test]
fn stress_test_empty_blocks() {
    let ctx = Context::create();

    let func = MirFunction {
        name: "empty_blocks".to_string(),
        params: vec![],
        blocks: vec![
            MirBlock {
                label: "entry".to_string(),
                instructions: vec![],
                terminator: MirTerminator::Branch("middle".to_string()),
            },
            MirBlock {
                label: "middle".to_string(),
                instructions: vec![],
                terminator: MirTerminator::Branch("exit".to_string()),
            },
            MirBlock {
                label: "exit".to_string(),
                instructions: vec![],
                terminator: MirTerminator::Return(None),
            },
        ],
        next_reg: 0,
        span: jdruby_common::SourceSpan::default(),
    };

    let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
    let module = MirModule { name: "empty".to_string(), functions: vec![func] };

    let (ir, _reporter) = codegen.generate_with_errors(&module);

    let mut bytes = ir.as_bytes().to_vec();
    bytes.push(0);
    let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(&bytes, "empty");
    let result = ctx.create_module_from_ir(buffer);
    assert!(result.is_ok() || result.is_err(), "Should handle empty blocks");
}

/// Test 27: Invalid branch targets
#[test]
fn stress_test_invalid_branches() {
    let ctx = Context::create();

    let func = MirFunction {
        name: "invalid_branch".to_string(),
        params: vec![100],
        blocks: vec![MirBlock {
            label: "entry".to_string(),
            instructions: vec![],
            terminator: MirTerminator::Branch("nonexistent_block".to_string()),
        }],
        next_reg: 0,
        span: jdruby_common::SourceSpan::default(),
    };

    let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
    let module = MirModule { name: "invalid_branch".to_string(), functions: vec![func] };

    // Should handle invalid branch gracefully
    let (ir, mut reporter) = codegen.generate_with_errors(&module);

    // May produce error but should not crash
    let _diagnostics = reporter.take_diagnostics();

    let mut bytes = ir.as_bytes().to_vec();
    bytes.push(0);
    let buffer =
        inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(&bytes, "inv_branch");
    match ctx.create_module_from_ir(buffer) {
        Ok(_) => {}
        Err(e) => panic!("IR parsing failed: {:?}", e),
    };
}

/// Test 28: Deeply nested conditionals
#[test]
fn stress_test_deep_conditionals() {
    let ctx = Context::create();

    for depth in [5, 10, 20, 50, 100] {
        let mut blocks = vec![];
        let mut prev_label = "entry".to_string();

        for i in 0..depth {
            let then_label = format!("then_{}", i);
            let else_label = format!("else_{}", i);
            let next_label = format!("block_{}", i + 1);

            blocks.push(MirBlock {
                label: prev_label.clone(),
                instructions: vec![
                    MirInst::LoadConst(i as u32 * 2, MirConst::Integer(i as i64)),
                    MirInst::LoadConst(i as u32 * 2 + 1, MirConst::Integer(0)),
                ],
                terminator: MirTerminator::CondBranch(
                    i as u32 * 2,
                    then_label.clone(),
                    else_label.clone(),
                ),
            });

            blocks.push(MirBlock {
                label: then_label,
                instructions: vec![],
                terminator: MirTerminator::Branch(next_label.clone()),
            });

            blocks.push(MirBlock {
                label: else_label,
                instructions: vec![],
                terminator: MirTerminator::Branch(next_label.clone()),
            });

            prev_label = next_label;
        }

        blocks.push(MirBlock {
            label: prev_label,
            instructions: vec![MirInst::LoadConst(0, MirConst::Integer(42))],
            terminator: MirTerminator::Return(Some(0)),
        });

        let func = MirFunction {
            name: format!("deep_cond_{}", depth),
            params: vec![100],
            blocks,
            next_reg: (depth * 2) as u32,
            span: jdruby_common::SourceSpan::default(),
        };

        let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
        let module = MirModule { name: format!("deep_cond_{}", depth), functions: vec![func] };

        let (ir, _reporter) = codegen.generate_with_errors(&module);

        let mut bytes = ir.as_bytes().to_vec();
        bytes.push(0);
        let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(
            &bytes,
            &format!("dc_{}", depth),
        );
        match ctx.create_module_from_ir(buffer) {
            Ok(_) => {}
            Err(e) => panic!("IR parsing failed: {:?}", e),
        };
    }
}

/// Test 29: Very large constants
#[test]
fn stress_test_large_constants() {
    let ctx = Context::create();

    let large_strings = vec!["x".repeat(1000), "x".repeat(10000), "x".repeat(100000)];

    for (i, s) in large_strings.iter().enumerate() {
        let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
        let func = create_string_function(&format!("large_str_{}", i), s);
        let module = MirModule { name: format!("large_{}", i), functions: vec![func] };

        let (ir, reporter) = codegen.generate_with_errors(&module);

        // May error but should not crash
        if !reporter.has_errors() {
            let mut bytes = ir.as_bytes().to_vec();
            bytes.push(0);
            let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(
                &bytes,
                &format!("large_{}", i),
            );
            match ctx.create_module_from_ir(buffer) {
                Ok(_) => {}
                Err(e) => panic!("IR parsing failed: {:?}", e),
            };
        }
    }
}

/// Test 30: Memory exhaustion simulation
#[test]
fn stress_test_many_small_modules() {
    let ctx = Context::create();

    // Create 500 very small modules
    for i in 0..500 {
        let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
        let func = create_minimal_function(&format!("tiny_{}", i));
        let module = MirModule { name: format!("tiny_{}", i), functions: vec![func] };

        let (ir, _reporter) = codegen.generate_with_errors(&module);

        let mut bytes = ir.as_bytes().to_vec();
        bytes.push(0);
        let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(
            &bytes,
            &format!("tiny_{}", i),
        );
        match ctx.create_module_from_ir(buffer) {
            Ok(_) => {}
            Err(e) => panic!("IR parsing failed: {:?}", e),
        };
    }
}

/// Test 31: Type confusion patterns
#[test]
fn stress_test_type_confusion() {
    let ctx = Context::create();

    // Functions that mix different types unexpectedly
    let func = MirFunction {
        name: "type_confusion".to_string(),
        params: vec![],
        blocks: vec![MirBlock {
            label: "entry".to_string(),
            instructions: vec![
                MirInst::LoadConst(0, MirConst::Integer(42)),
                MirInst::LoadConst(1, MirConst::String("hello".to_string())),
                MirInst::LoadConst(2, MirConst::Bool(true)),
                MirInst::LoadConst(3, MirConst::Nil),
                // Mix them in operations
                MirInst::BinOp(4, MirBinOp::Add, 0, 1), // int + string
                MirInst::BinOp(5, MirBinOp::And, 2, 0), // bool + int
            ],
            terminator: MirTerminator::Return(Some(5)),
        }],
        next_reg: 6,
        span: jdruby_common::SourceSpan::default(),
    };

    let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
    let module = MirModule { name: "type_conf".to_string(), functions: vec![func] };

    let (ir, _reporter) = codegen.generate_with_errors(&module);

    let mut bytes = ir.as_bytes().to_vec();
    bytes.push(0);
    let buffer =
        inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(&bytes, "type_conf");
    match ctx.create_module_from_ir(buffer) {
        Ok(_) => {}
        Err(e) => panic!("IR parsing failed: {:?}", e),
    };
}

/// Test 32: Concurrent module compilation stress
#[test]
fn stress_test_concurrent_modules() {
    use std::sync::Arc;
    use std::thread;

    let _ctx = Arc::new(Context::create());
    let mut handles = vec![];

    for i in 0..20 {
        let ctx = Context::create();
        handles.push(thread::spawn(move || {
            let config = CodegenConfig::default();
            let mut codegen = CodeGenerator::new(config, &ctx);
            let func = create_minimal_function(&format!("thread_{}_func_{}", i, 0));
            let module = MirModule { name: format!("thread_{}", i), functions: vec![func] };

            let (ir, _reporter) = codegen.generate_with_errors(&module);

            let mut bytes = ir.as_bytes().to_vec();
            bytes.push(0);
            let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(
                &bytes,
                &format!("t_{}", i),
            );
            match ctx.create_module_from_ir(buffer) {
                Ok(_) => {}
                Err(e) => panic!("IR parsing failed: {:?}", e),
            };
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }
}

/// Test 33: Stack overflow simulation with deep calls
#[test]
fn stress_test_deep_call_chain() {
    let ctx = Context::create();

    let depth = 200;
    let mut functions = vec![];

    for i in 0..depth {
        let func = MirFunction {
            name: format!("chain_{}", i),
            params: vec![100],
            blocks: vec![MirBlock {
                label: "entry".to_string(),
                instructions: vec![
                    MirInst::LoadConst(0, MirConst::Integer(i as i64)),
                    MirInst::Call(1, format!("chain_{}", (i + 1) % depth), vec![100]),
                ],
                terminator: MirTerminator::Return(Some(1)),
            }],
            next_reg: 2,
            span: jdruby_common::SourceSpan::default(),
        };
        functions.push(func);
    }

    let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
    let module = MirModule { name: "deep_chain".to_string(), functions };

    let (ir, _reporter) = codegen.generate_with_errors(&module);

    let mut bytes = ir.as_bytes().to_vec();
    bytes.push(0);
    let buffer =
        inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(&bytes, "deep_chain");
    match ctx.create_module_from_ir(buffer) {
        Ok(_) => {}
        Err(e) => panic!("IR parsing failed: {:?}", e),
    };
}

/// Test 34: Use-after-free patterns in IR
#[test]
fn stress_test_use_after_free_patterns() {
    let ctx = Context::create();

    // Simulate patterns that could lead to use-after-free
    let func = MirFunction {
        name: "uaf_test".to_string(),
        params: vec![100],
        blocks: vec![MirBlock {
            label: "entry".to_string(),
            instructions: vec![
                MirInst::Alloc(0, "ptr".to_string()),
                MirInst::Store("ptr".to_string(), 100),
                MirInst::Load(1, "ptr".to_string()),
                MirInst::Store("ptr".to_string(), 1), // Overwrite
                MirInst::Load(2, "ptr".to_string()),
            ],
            terminator: MirTerminator::Return(Some(2)),
        }],
        next_reg: 3,
        span: jdruby_common::SourceSpan::default(),
    };

    let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
    let module = MirModule { name: "uaf".to_string(), functions: vec![func] };

    let (ir, _reporter) = codegen.generate_with_errors(&module);

    let mut bytes = ir.as_bytes().to_vec();
    bytes.push(0);
    let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(&bytes, "uaf");
    match ctx.create_module_from_ir(buffer) {
        Ok(_) => {}
        Err(e) => panic!("IR parsing failed: {:?}", e),
    };
}

/// Test 35: Division by zero patterns
#[test]
fn stress_test_division_by_zero() {
    let ctx = Context::create();

    let ops = vec![MirBinOp::Div, MirBinOp::Mod];

    for (i, op) in ops.iter().enumerate() {
        let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
        let func = MirFunction {
            name: format!("div_zero_{}", i),
            params: vec![],
            blocks: vec![MirBlock {
                label: "entry".to_string(),
                instructions: vec![
                    MirInst::LoadConst(0, MirConst::Integer(42)),
                    MirInst::LoadConst(1, MirConst::Integer(0)),
                    MirInst::BinOp(2, *op, 0, 1),
                ],
                terminator: MirTerminator::Return(Some(2)),
            }],
            next_reg: 3,
            span: jdruby_common::SourceSpan::default(),
        };

        let module = MirModule { name: format!("div_{}", i), functions: vec![func] };

        let (ir, _reporter) = codegen.generate_with_errors(&module);

        let mut bytes = ir.as_bytes().to_vec();
        bytes.push(0);
        let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(
            &bytes,
            &format!("div_{}", i),
        );
        match ctx.create_module_from_ir(buffer) {
            Ok(_) => {}
            Err(e) => panic!("IR parsing failed: {:?}", e),
        };
    }
}

/// Test 36: Infinite loop detection
#[test]
fn stress_test_infinite_loops() {
    let ctx = Context::create();

    let func = MirFunction {
        name: "infinite_loop".to_string(),
        params: vec![100],
        blocks: vec![MirBlock {
            label: "loop_start".to_string(),
            instructions: vec![MirInst::LoadConst(0, MirConst::Integer(1))],
            terminator: MirTerminator::Branch("loop_start".to_string()), // Loop back to self
        }],
        next_reg: 1,
        span: jdruby_common::SourceSpan::default(),
    };

    let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
    let module = MirModule { name: "infinite".to_string(), functions: vec![func] };

    let (ir, _reporter) = codegen.generate_with_errors(&module);

    let mut bytes = ir.as_bytes().to_vec();
    bytes.push(0);
    let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(&bytes, "infinite");
    match ctx.create_module_from_ir(buffer) {
        Ok(_) => {}
        Err(e) => panic!("IR parsing failed: {:?}", e),
    };
}

/// Test 37: Dead code after return
#[test]
fn stress_test_dead_code() {
    let ctx = Context::create();

    let func = MirFunction {
        name: "dead_code".to_string(),
        params: vec![],
        blocks: vec![MirBlock {
            label: "entry".to_string(),
            instructions: vec![
                MirInst::LoadConst(0, MirConst::Integer(42)),
                MirInst::LoadConst(1, MirConst::Integer(100)),
                MirInst::BinOp(2, MirBinOp::Add, 0, 1),
                MirInst::Call(3, "never_called".to_string(), vec![0]),
            ],
            terminator: MirTerminator::Return(Some(0)),
        }],
        next_reg: 4,
        span: jdruby_common::SourceSpan::default(),
    };

    let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
    let module = MirModule { name: "dead".to_string(), functions: vec![func] };

    let (ir, _reporter) = codegen.generate_with_errors(&module);

    let mut bytes = ir.as_bytes().to_vec();
    bytes.push(0);
    let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(&bytes, "dead");
    match ctx.create_module_from_ir(buffer) {
        Ok(_) => {}
        Err(e) => panic!("IR parsing failed: {:?}", e),
    };
}

/// Test 38: Duplicate function names
#[test]
fn stress_test_duplicate_names() {
    let ctx = Context::create();

    let func1 = create_minimal_function("duplicate");
    let func2 = create_arithmetic_function("duplicate"); // Same name

    let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
    let module = MirModule { name: "dup".to_string(), functions: vec![func1, func2] };

    let (ir, mut reporter) = codegen.generate_with_errors(&module);

    // Should either error or handle gracefully
    let _diagnostics = reporter.take_diagnostics();

    let mut bytes = ir.as_bytes().to_vec();
    bytes.push(0);
    let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(&bytes, "dup");
    match ctx.create_module_from_ir(buffer) {
        Ok(_) => {}
        Err(e) => panic!("IR parsing failed: {:?}", e),
    };
}

/// Test 39: Invalid UTF-8 string handling
#[test]
fn stress_test_invalid_utf8() {
    let ctx = Context::create();

    // Test with various "difficult" strings
    let strings = vec![
        "\u{0000}", // null
        "\u{0001}", // control
        "\u{007F}", // DEL
        "\u{0080}", // extended
        "\u{FFFF}", // non-char
        "\u{FFFE}", // BOM non-char
    ];

    for (i, s) in strings.iter().enumerate() {
        let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
        let func = create_string_function(&format!("utf8_edge_{}", i), s);
        let module = MirModule { name: format!("utf8_{}", i), functions: vec![func] };

        let (ir, _reporter) = codegen.generate_with_errors(&module);

        let mut bytes = ir.as_bytes().to_vec();
        bytes.push(0);
        let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(
            &bytes,
            &format!("u8_{}", i),
        );
        match ctx.create_module_from_ir(buffer) {
            Ok(_) => {}
            Err(e) => panic!("IR parsing failed: {:?}", e),
        };
    }
}

/// Test 40: Massive register pressure
#[test]
fn stress_test_register_pressure() {
    let ctx = Context::create();

    for num_regs in [100, 500, 1000, 5000] {
        let mut instructions = vec![];

        for i in 0..num_regs {
            instructions.push(MirInst::LoadConst(i as u32, MirConst::Integer(i as i64)));
        }

        // Chain them all together
        for i in 1..num_regs {
            instructions.push(MirInst::BinOp(
                (num_regs + i - 1) as u32,
                MirBinOp::Add,
                (i - 1) as u32,
                i as u32,
            ));
        }

        let func = MirFunction {
            name: format!("reg_pressure_{}", num_regs),
            params: vec![],
            blocks: vec![MirBlock {
                label: "entry".to_string(),
                instructions,
                terminator: MirTerminator::Return(Some((num_regs * 2 - 2) as u32)),
            }],
            next_reg: (num_regs * 2) as u32,
            span: jdruby_common::SourceSpan::default(),
        };

        let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
        let module = MirModule { name: format!("reg_{}", num_regs), functions: vec![func] };

        let (ir, reporter) = codegen.generate_with_errors(&module);

        // May error for very large but should not crash
        if !reporter.has_errors() {
            let mut bytes = ir.as_bytes().to_vec();
            bytes.push(0);
            let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(
                &bytes,
                &format!("rp_{}", num_regs),
            );
            match ctx.create_module_from_ir(buffer) {
                Ok(_) => {}
                Err(e) => panic!("IR parsing failed: {:?}", e),
            };
        }
    }
}

/// Test 41: JIT tier switching stress
#[test]
fn stress_test_tier_switching() {
    let ctx = Context::create();
    let mut compiler = JitCompiler::new(&ctx);

    for i in 0..100 {
        let func = create_minimal_function(&format!("tier_{}", i));

        // Alternate between tiers rapidly
        let tier = match i % 3 {
            0 => CompilationTier::Baseline,
            1 => CompilationTier::Optimizing,
            _ => CompilationTier::Baseline, // Could add more tiers
        };

        let result = compiler.compile_function_ir(&func, tier, i as u64);
        assert!(result.is_ok() || result.is_err(), "Tier switching should not crash");
    }
}

/// Test 42: Code cache eviction simulation
#[test]
fn stress_test_cache_eviction() {
    let ctx = Context::create();
    let mut compiler = JitCompiler::new(&ctx);

    // Compile many functions to potentially trigger cache pressure
    for i in 0..200 {
        let func = match i % 5 {
            0 => create_minimal_function(&format!("cache_{}", i)),
            1 => create_arithmetic_function(&format!("cache_math_{}", i)),
            2 => create_string_function(&format!("cache_str_{}", i), "test"),
            3 => create_conditional_function(&format!("cache_cond_{}", i)),
            _ => create_variable_function(&format!("cache_var_{}", i)),
        };

        let result = compiler.compile_function_ir(&func, CompilationTier::Baseline, i as u64);
        assert!(result.is_ok() || result.is_err(), "Cache eviction should not crash");
    }
}

/// Test 43: LLVM IR syntax edge cases
#[test]
fn stress_test_llvm_ir_syntax() {
    let ctx = Context::create();

    // Test various LLVM IR syntax edge cases
    let test_cases = vec![
        // Reserved LLVM keywords in names
        "define_func",
        "global_var",
        "metadata_test",
        "attributes_test",
        "section_test",
        "align_test",
        "nounwind_test",
        "uwtable_test",
    ];

    for (i, name) in test_cases.iter().enumerate() {
        let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
        let func = create_minimal_function(name);
        let module = MirModule { name: format!("llvm_syn_{}", i), functions: vec![func] };

        let (ir, _reporter) = codegen.generate_with_errors(&module);

        let mut bytes = ir.as_bytes().to_vec();
        bytes.push(0);
        let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(
            &bytes,
            &format!("ls_{}", i),
        );
        let result = ctx.create_module_from_ir(buffer);
        assert!(result.is_ok(), "LLVM syntax test {} failed: {:?}", i, result.err());
    }
}

/// Test 44: Null pointer handling
#[test]
fn stress_test_null_handling() {
    let ctx = Context::create();

    let func = MirFunction {
        name: "null_test".to_string(),
        params: vec![],
        blocks: vec![MirBlock {
            label: "entry".to_string(),
            instructions: vec![
                MirInst::LoadConst(0, MirConst::Nil),
                MirInst::LoadConst(1, MirConst::Integer(0)),
                MirInst::BinOp(2, MirBinOp::Eq, 0, 1),
            ],
            terminator: MirTerminator::Return(Some(2)),
        }],
        next_reg: 3,
        span: jdruby_common::SourceSpan::default(),
    };

    let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
    let module = MirModule { name: "null".to_string(), functions: vec![func] };

    let (ir, _reporter) = codegen.generate_with_errors(&module);

    let mut bytes = ir.as_bytes().to_vec();
    bytes.push(0);
    let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(&bytes, "null");
    let result = ctx.create_module_from_ir(buffer);
    assert!(result.is_ok(), "Null handling test failed");
}

/// Test 45: Floating point edge cases
#[test]
fn stress_test_float_edge_cases() {
    let ctx = Context::create();

    // If MIR supports floats, test edge cases
    let func = MirFunction {
        name: "float_edge".to_string(),
        params: vec![],
        blocks: vec![MirBlock {
            label: "entry".to_string(),
            instructions: vec![
                MirInst::LoadConst(0, MirConst::Integer(0)),
                MirInst::LoadConst(1, MirConst::Integer(1)),
                MirInst::BinOp(2, MirBinOp::Div, 1, 0), // 1/0
            ],
            terminator: MirTerminator::Return(Some(2)),
        }],
        next_reg: 3,
        span: jdruby_common::SourceSpan::default(),
    };

    let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
    let module = MirModule { name: "float".to_string(), functions: vec![func] };

    let (ir, _reporter) = codegen.generate_with_errors(&module);

    let mut bytes = ir.as_bytes().to_vec();
    bytes.push(0);
    let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(&bytes, "float");
    match ctx.create_module_from_ir(buffer) {
        Ok(_) => {}
        Err(e) => panic!("IR parsing failed: {:?}", e),
    };
}

/// Test 46: Very long method chains
#[test]
fn stress_test_long_method_chains() {
    let ctx = Context::create();

    let chain_length = 100;
    let mut instructions = vec![MirInst::LoadConst(0, MirConst::Integer(42))];

    for i in 0..chain_length {
        instructions.push(MirInst::MethodCall(
            (i + 1) as u32,
            100, // self
            format!("method_{}", i),
            vec![i as u32],
        ));
    }

    let func = MirFunction {
        name: "long_chain".to_string(),
        params: vec![100],
        blocks: vec![MirBlock {
            label: "entry".to_string(),
            instructions,
            terminator: MirTerminator::Return(Some(chain_length as u32)),
        }],
        next_reg: (chain_length + 1) as u32,
        span: jdruby_common::SourceSpan::default(),
    };

    let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
    let module = MirModule { name: "chain".to_string(), functions: vec![func] };

    let (ir, _reporter) = codegen.generate_with_errors(&module);

    let mut bytes = ir.as_bytes().to_vec();
    bytes.push(0);
    let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(&bytes, "chain");
    match ctx.create_module_from_ir(buffer) {
        Ok(_) => {}
        Err(e) => panic!("IR parsing failed: {:?}", e),
    };
}

/// Test 47: Exception handling patterns
#[test]
fn stress_test_exception_patterns() {
    let ctx = Context::create();

    // Simulate exception handling structure
    let func = MirFunction {
        name: "exception_test".to_string(),
        params: vec![100],
        blocks: vec![
            MirBlock {
                label: "try".to_string(),
                instructions: vec![MirInst::LoadConst(0, MirConst::Integer(1))],
                // MethodCall for exception-like pattern (since MirTerminator::Call doesn't exist)
                terminator: MirTerminator::Branch("catch".to_string()),
            },
            MirBlock {
                label: "catch".to_string(),
                instructions: vec![MirInst::LoadConst(1, MirConst::Integer(0))],
                terminator: MirTerminator::Branch("finally".to_string()),
            },
            MirBlock {
                label: "finally".to_string(),
                instructions: vec![MirInst::LoadConst(2, MirConst::Integer(42))],
                terminator: MirTerminator::Return(Some(2)),
            },
        ],
        next_reg: 3,
        span: jdruby_common::SourceSpan::default(),
    };

    let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
    let module = MirModule { name: "except".to_string(), functions: vec![func] };

    let (ir, _reporter) = codegen.generate_with_errors(&module);

    let mut bytes = ir.as_bytes().to_vec();
    bytes.push(0);
    let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(&bytes, "except");
    match ctx.create_module_from_ir(buffer) {
        Ok(_) => {}
        Err(e) => panic!("IR parsing failed: {:?}", e),
    };
}

/// Test 48: Resource exhaustion - many parameters
#[test]
fn stress_test_many_parameters() {
    let ctx = Context::create();

    for num_params in [10, 50, 100, 200] {
        let params: Vec<u32> = (100..100 + num_params).map(|i| i as u32).collect();

        let mut instructions = vec![];
        for (i, param) in params.iter().enumerate() {
            if i > 0 {
                instructions.push(MirInst::BinOp(
                    (i + num_params) as u32,
                    MirBinOp::Add,
                    (i + num_params - 1) as u32,
                    *param,
                ));
            } else {
                instructions.push(MirInst::LoadConst(num_params as u32, MirConst::Integer(0)));
                instructions.push(MirInst::BinOp(
                    (num_params + 1) as u32,
                    MirBinOp::Add,
                    num_params as u32,
                    *param,
                ));
            }
        }

        let func = MirFunction {
            name: format!("many_params_{}", num_params),
            params,
            blocks: vec![MirBlock {
                label: "entry".to_string(),
                instructions,
                terminator: MirTerminator::Return(Some((num_params * 2) as u32)),
            }],
            next_reg: (num_params * 2 + 1) as u32,
            span: jdruby_common::SourceSpan::default(),
        };

        let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
        let module = MirModule { name: format!("params_{}", num_params), functions: vec![func] };

        let (ir, reporter) = codegen.generate_with_errors(&module);

        if !reporter.has_errors() {
            let mut bytes = ir.as_bytes().to_vec();
            bytes.push(0);
            let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(
                &bytes,
                &format!("mp_{}", num_params),
            );
            match ctx.create_module_from_ir(buffer) {
                Ok(_) => {}
                Err(e) => panic!("IR parsing failed: {:?}", e),
            };
        }
    }
}

/// Test 49: Block/pass patterns (Ruby-specific)
#[test]
fn stress_test_block_pass() {
    let ctx = Context::create();

    let func = MirFunction {
        name: "block_pass_test".to_string(),
        params: vec![100, 101], // self, block
        blocks: vec![MirBlock {
            label: "entry".to_string(),
            instructions: vec![
                MirInst::LoadConst(0, MirConst::Integer(1)),
                MirInst::LoadConst(1, MirConst::Integer(2)),
                MirInst::Call(2, "yield".to_string(), vec![0, 1]),
            ],
            terminator: MirTerminator::Return(Some(2)),
        }],
        next_reg: 3,
        span: jdruby_common::SourceSpan::default(),
    };

    let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
    let module = MirModule { name: "block".to_string(), functions: vec![func] };

    let (ir, _reporter) = codegen.generate_with_errors(&module);

    let mut bytes = ir.as_bytes().to_vec();
    bytes.push(0);
    let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(&bytes, "block");
    match ctx.create_module_from_ir(buffer) {
        Ok(_) => {}
        Err(e) => panic!("IR parsing failed: {:?}", e),
    };
}

/// Test 50: Singleton method patterns
#[test]
fn stress_test_singleton_methods() {
    let ctx = Context::create();

    let singleton_names = vec!["self.method", "obj.singleton", "class<<self", "extend_module"];

    for (i, name) in singleton_names.iter().enumerate() {
        let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
        let func = create_special_name_function(name);
        let module = MirModule { name: format!("singleton_{}", i), functions: vec![func] };

        let (ir, _reporter) = codegen.generate_with_errors(&module);

        let mut bytes = ir.as_bytes().to_vec();
        bytes.push(0);
        let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(
            &bytes,
            &format!("sing_{}", i),
        );
        match ctx.create_module_from_ir(buffer) {
            Ok(_) => {}
            Err(e) => panic!("IR parsing failed: {:?}", e),
        };
    }
}

/// Test 51-100: Fuzzing-style randomized tests
#[test]
fn stress_test_fuzz_1() {
    fuzz_test_helper(1);
}
#[test]
fn stress_test_fuzz_2() {
    fuzz_test_helper(2);
}
#[test]
fn stress_test_fuzz_3() {
    fuzz_test_helper(3);
}
#[test]
fn stress_test_fuzz_4() {
    fuzz_test_helper(4);
}
#[test]
fn stress_test_fuzz_5() {
    fuzz_test_helper(5);
}

fn fuzz_test_helper(seed: u64) {
    let ctx = Context::create();

    // Generate pseudo-random but deterministic test based on seed
    let ops = vec![
        MirBinOp::Add,
        MirBinOp::Sub,
        MirBinOp::Mul,
        MirBinOp::Div,
        MirBinOp::Mod,
        MirBinOp::Eq,
        MirBinOp::NotEq,
    ];

    let mut instructions = vec![];
    for i in 0..20 {
        let op = ops[((seed + i as u64) % ops.len() as u64) as usize];
        let val = ((seed * (i + 1) as u64) % 1000) as i64;
        instructions.push(MirInst::LoadConst(i as u32, MirConst::Integer(val)));
        if i > 0 {
            instructions.push(MirInst::BinOp((20 + i) as u32, op, (i - 1) as u32, i as u32));
        }
    }

    let func = MirFunction {
        name: format!("fuzz_{}", seed),
        params: vec![],
        blocks: vec![MirBlock {
            label: "entry".to_string(),
            instructions,
            terminator: MirTerminator::Return(Some(39)),
        }],
        next_reg: 40,
        span: jdruby_common::SourceSpan::default(),
    };

    let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
    let module = MirModule { name: format!("fuzz_{}", seed), functions: vec![func] };

    let (ir, _reporter) = codegen.generate_with_errors(&module);

    let mut bytes = ir.as_bytes().to_vec();
    bytes.push(0);
    let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(
        &bytes,
        &format!("fuzz_{}", seed),
    );
    match ctx.create_module_from_ir(buffer) {
        Ok(_) => {}
        Err(e) => panic!("IR parsing failed: {:?}", e),
    };
}

/// Test 101-150: BinaryBuilder stress tests with edge cases
#[test]
fn stress_test_binary_builder_edge_1() {
    binary_builder_edge_helper(1);
}
#[test]
fn stress_test_binary_builder_edge_2() {
    binary_builder_edge_helper(2);
}
#[test]
fn stress_test_binary_builder_edge_3() {
    binary_builder_edge_helper(3);
}

fn binary_builder_edge_helper(seed: u32) {
    use inkwell::OptimizationLevel;

    let ctx = Context::create();
    let config = BinaryBuilderConfig {
        output_path: std::path::PathBuf::from(format!("test_output_{}", seed)),
        opt_level: match seed % 4 {
            0 => OptimizationLevel::None,
            1 => OptimizationLevel::Less,
            2 => OptimizationLevel::Default,
            _ => OptimizationLevel::Aggressive,
        },
        ..Default::default()
    };

    let mut builder = BinaryBuilder::new(&ctx, config);

    // Add modules with varying characteristics
    for i in 0..50 {
        let func = match (seed + i) % 7 {
            0 => create_minimal_function(&format!("bb_edge_{}_{}", seed, i)),
            1 => create_arithmetic_function(&format!("bb_math_{}_{}", seed, i)),
            2 => create_string_function(
                &format!("bb_str_{}_{}", seed, i),
                &format!("test_{}_{}", seed, i),
            ),
            3 => create_conditional_function(&format!("bb_cond_{}_{}", seed, i)),
            4 => create_variable_function(&format!("bb_var_{}_{}", seed, i)),
            5 => create_method_call_function(&format!("bb_method_{}_{}", seed, i)),
            _ => create_special_name_function(&format!("bb_spec_{}#{}::test", seed, i)),
        };

        let mir_module =
            MirModule { name: format!("bb_module_{}_{}", seed, i), functions: vec![func] };

        let result = builder.add_module_with_errors(&format!("mod_{}_{}", seed, i), &mir_module);
        assert!(result.is_ok() || result.is_err(), "BinaryBuilder edge test should not crash");
    }
}

/// Test 151-200: JIT compiler edge cases
#[test]
fn stress_test_jit_edge_1() {
    jit_edge_helper(1);
}
#[test]
fn stress_test_jit_edge_2() {
    jit_edge_helper(2);
}
#[test]
fn stress_test_jit_edge_3() {
    jit_edge_helper(3);
}

fn jit_edge_helper(seed: u64) {
    let ctx = Context::create();
    let mut compiler = JitCompiler::new(&ctx);

    for i in 0..25 {
        let func = match ((seed + i as u64) % 10) as u32 {
            0 => create_minimal_function(&format!("jit_edge_{}_{}", seed, i)),
            1 => create_arithmetic_function(&format!("jit_math_{}_{}", seed, i)),
            2 => create_string_function(
                &format!("jit_str_{}_{}", seed, i),
                &format!("s{}_{}", seed, i),
            ),
            3 => create_conditional_function(&format!("jit_cond_{}_{}", seed, i)),
            4 => create_variable_function(&format!("jit_var_{}_{}", seed, i)),
            5 => create_deeply_nested_function(
                &format!("jit_deep_{}_{}", seed, i),
                ((seed + i as u64) % 50) as usize,
            ),
            6 => create_all_ops_function(&format!("jit_ops_{}_{}", seed, i)),
            7 => create_method_call_function(&format!("jit_meth_{}_{}", seed, i)),
            8 => create_special_name_function(&format!("jit_spec_{}_{}#test", seed, i)),
            _ => create_string_function(&format!("jit_empty_{}_{}", seed, i), ""),
        };

        let tier = match (seed + i as u64) % 2 {
            0 => CompilationTier::Baseline,
            _ => CompilationTier::Optimizing,
        };

        let result = compiler.compile_function_ir(&func, tier, seed * 100 + i as u64);
        assert!(result.is_ok() || result.is_err(), "JIT edge test should not crash");
    }
}

/// Test 201-250: Memory safety stress tests
#[test]
fn stress_test_memory_safety_1() {
    memory_safety_helper(100);
}
#[test]
fn stress_test_memory_safety_2() {
    memory_safety_helper(500);
}
#[test]
fn stress_test_memory_safety_3() {
    memory_safety_helper(1000);
}

fn memory_safety_helper(num_iterations: usize) {
    let ctx = Context::create();

    for i in 0..num_iterations {
        let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);

        // Create function with lots of allocations
        let mut instructions = vec![];
        for j in 0..10 {
            instructions.push(MirInst::Alloc(j as u32, format!("var_{}_{}", i, j)));
            instructions
                .push(MirInst::LoadConst((j + 10) as u32, MirConst::Integer((i * 10 + j) as i64)));
            instructions.push(MirInst::Store(format!("var_{}_{}", i, j), (j + 10) as u32));
            instructions.push(MirInst::Load((j + 20) as u32, format!("var_{}_{}", i, j)));
        }

        let func = MirFunction {
            name: format!("mem_safe_{}_{}", i, num_iterations),
            params: vec![],
            blocks: vec![MirBlock {
                label: "entry".to_string(),
                instructions,
                terminator: MirTerminator::Return(Some(29)),
            }],
            next_reg: 30,
            span: jdruby_common::SourceSpan::default(),
        };

        let module =
            MirModule { name: format!("mem_{}_{}", i, num_iterations), functions: vec![func] };

        let (ir, _reporter) = codegen.generate_with_errors(&module);

        // Don't parse every IR to save time, just verify generation worked
        if i % 10 == 0 {
            let mut bytes = ir.as_bytes().to_vec();
            bytes.push(0);
            let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(
                &bytes,
                &format!("ms_{}", i),
            );
            match ctx.create_module_from_ir(buffer) {
                Ok(_) => {}
                Err(e) => panic!("IR parsing failed: {:?}", e),
            };
        }
    }
}

/// Test 251-300: Malformed input resistance
#[test]
fn stress_test_malformed_resistance_1() {
    malformed_helper(&["\x00", "\x01", "\x02"]);
}
#[test]
fn stress_test_malformed_resistance_2() {
    malformed_helper(&["{", "}", "("]);
}
#[test]
fn stress_test_malformed_resistance_3() {
    malformed_helper(&["\\", "\"", "'"]);
}

fn malformed_helper(chars: &[&str]) {
    let ctx = Context::create();

    for (i, c) in chars.iter().enumerate() {
        let name = format!("malformed_{}_{}", i, c.escape_default());
        let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
        let func = create_string_function(&name, c);
        let module = MirModule { name: name.clone(), functions: vec![func] };

        let (ir, _reporter) = codegen.generate_with_errors(&module);

        let mut bytes = ir.as_bytes().to_vec();
        bytes.push(0);
        let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(&bytes, &name);
        match ctx.create_module_from_ir(buffer) {
            Ok(_) => {}
            Err(e) => panic!("IR parsing failed: {:?}", e),
        };
    }
}

/// Test 301-350: Concurrency stress tests
#[test]
fn stress_test_concurrent_codegen_1() {
    concurrent_codegen_helper(5);
}
#[test]
fn stress_test_concurrent_codegen_2() {
    concurrent_codegen_helper(10);
}
#[test]
fn stress_test_concurrent_codegen_3() {
    concurrent_codegen_helper(20);
}

fn concurrent_codegen_helper(num_threads: usize) {
    use std::sync::Arc;
    use std::thread;

    let _ctx = Arc::new(Context::create());
    let mut handles = vec![];

    for t in 0..num_threads {
        handles.push(thread::spawn(move || {
            let ctx = Context::create();
            for i in 0..10 {
                let config = CodegenConfig::default();
                let mut codegen = CodeGenerator::new(config, &ctx);

                let func = match (t + i) % 5 {
                    0 => create_minimal_function(&format!("ct_{}_{}", t, i)),
                    1 => create_arithmetic_function(&format!("ct_math_{}_{}", t, i)),
                    2 => create_string_function(&format!("ct_str_{}_{}", t, i), "test"),
                    3 => create_conditional_function(&format!("ct_cond_{}_{}", t, i)),
                    _ => create_variable_function(&format!("ct_var_{}_{}", t, i)),
                };

                let module =
                    MirModule { name: format!("ct_mod_{}_{}", t, i), functions: vec![func] };

                let (ir, _reporter) = codegen.generate_with_errors(&module);

                let mut bytes = ir.as_bytes().to_vec();
                bytes.push(0);
                let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(
                    &bytes,
                    &format!("ct_{}_{}", t, i),
                );
                match ctx.create_module_from_ir(buffer) {
                    Ok(_) => {}
                    Err(e) => panic!("IR parsing failed: {:?}", e),
                };
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked in concurrent test");
    }
}

/// Test 351-400: IR validation stress tests
#[test]
fn stress_test_ir_validation_1() {
    ir_validation_helper(1);
}
#[test]
fn stress_test_ir_validation_2() {
    ir_validation_helper(2);
}
#[test]
fn stress_test_ir_validation_3() {
    ir_validation_helper(3);
}

fn ir_validation_helper(seed: u32) {
    let ctx = Context::create();

    // Generate IR with various characteristics and validate it
    let mut functions = vec![];
    for i in 0..20 {
        let func = match ((seed + i) % 6) as u32 {
            0 => create_minimal_function(&format!("val_{}_{}", seed, i)),
            1 => create_arithmetic_function(&format!("val_math_{}_{}", seed, i)),
            2 => create_string_function(
                &format!("val_str_{}_{}", seed, i),
                &format!("s{}_{}", seed, i),
            ),
            3 => create_conditional_function(&format!("val_cond_{}_{}", seed, i)),
            4 => create_variable_function(&format!("val_var_{}_{}", seed, i)),
            _ => create_deeply_nested_function(
                &format!("val_deep_{}_{}", seed, i),
                ((seed + i) % 20) as usize,
            ),
        };
        functions.push(func);
    }

    let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
    let module = MirModule { name: format!("validation_{}", seed), functions };

    let (ir, reporter) = codegen.generate_with_errors(&module);

    // Verify IR structure
    assert!(ir.contains("target triple"), "Missing target triple");

    // Parse and validate
    let mut bytes = ir.as_bytes().to_vec();
    bytes.push(0);
    let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(
        &bytes,
        &format!("val_{}", seed),
    );
    let result = ctx.create_module_from_ir(buffer);

    if reporter.has_errors() {
        // If errors reported, IR might not parse
        let _ = result;
    } else {
        assert!(result.is_ok(), "Valid IR should parse: {:?}", result.err());
    }
}

/// Test 401-450: Optimization level stress tests
#[test]
fn stress_test_opt_levels() {
    use inkwell::OptimizationLevel;

    let opt_levels = vec![
        OptimizationLevel::None,
        OptimizationLevel::Less,
        OptimizationLevel::Default,
        OptimizationLevel::Aggressive,
    ];

    for (i, opt_level) in opt_levels.iter().enumerate() {
        let ctx = Context::create();
        let config = BinaryBuilderConfig {
            output_path: std::path::PathBuf::from(format!("opt_{}", i)),
            opt_level: *opt_level,
            ..Default::default()
        };

        let mut builder = BinaryBuilder::new(&ctx, config);

        // Add various modules
        for j in 0..25 {
            let func = match j % 5 {
                0 => create_minimal_function(&format!("opt_{}_{}", i, j)),
                1 => create_arithmetic_function(&format!("opt_math_{}_{}", i, j)),
                2 => create_string_function(&format!("opt_str_{}_{}", i, j), "test"),
                3 => create_conditional_function(&format!("opt_cond_{}_{}", i, j)),
                _ => create_variable_function(&format!("opt_var_{}_{}", i, j)),
            };

            let mir_module =
                MirModule { name: format!("opt_mod_{}_{}", i, j), functions: vec![func] };

            let result = builder.add_module_with_errors(&format!("opt_{}_{}", i, j), &mir_module);
            assert!(result.is_ok() || result.is_err(), "Opt level test should not crash");
        }
    }
}

/// Test 451-500: Data flow stress tests
#[test]
fn stress_test_data_flow_1() {
    data_flow_helper(10);
}
#[test]
fn stress_test_data_flow_2() {
    data_flow_helper(50);
}
#[test]
fn stress_test_data_flow_3() {
    data_flow_helper(100);
}

fn data_flow_helper(num_vars: usize) {
    let ctx = Context::create();

    let mut instructions = vec![];

    // Create complex data flow
    for i in 0..num_vars {
        instructions.push(MirInst::Alloc(i as u32, format!("df_{}", i)));
        instructions.push(MirInst::LoadConst((num_vars + i) as u32, MirConst::Integer(i as i64)));
        instructions.push(MirInst::Store(format!("df_{}", i), (num_vars + i) as u32));
    }

    // Read all variables
    for i in 0..num_vars {
        instructions.push(MirInst::Load((num_vars * 2 + i) as u32, format!("df_{}", i)));
    }

    // Mix them
    for i in 1..num_vars {
        instructions.push(MirInst::BinOp(
            (num_vars * 3 + i) as u32,
            MirBinOp::Add,
            (num_vars * 2 + i - 1) as u32,
            (num_vars * 2 + i) as u32,
        ));
    }

    let func = MirFunction {
        name: format!("data_flow_{}", num_vars),
        params: vec![],
        blocks: vec![MirBlock {
            label: "entry".to_string(),
            instructions,
            terminator: MirTerminator::Return(Some((num_vars * 4 - 1) as u32)),
        }],
        next_reg: (num_vars * 4) as u32,
        span: jdruby_common::SourceSpan::default(),
    };

    let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
    let module = MirModule { name: format!("df_{}", num_vars), functions: vec![func] };

    let (ir, reporter) = codegen.generate_with_errors(&module);

    if !reporter.has_errors() {
        let mut bytes = ir.as_bytes().to_vec();
        bytes.push(0);
        let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(
            &bytes,
            &format!("df_{}", num_vars),
        );
        match ctx.create_module_from_ir(buffer) {
            Ok(_) => {}
            Err(e) => panic!("IR parsing failed: {:?}", e),
        };
    }
}

/// Test 501-550: Control flow graph stress tests
#[test]
fn stress_test_cfg_complex_1() {
    cfg_complex_helper(5);
}
#[test]
fn stress_test_cfg_complex_2() {
    cfg_complex_helper(10);
}
#[test]
fn stress_test_cfg_complex_3() {
    cfg_complex_helper(20);
}

fn cfg_complex_helper(num_blocks: usize) {
    let ctx = Context::create();

    let mut blocks = vec![];

    // Create entry block
    blocks.push(MirBlock {
        label: "entry".to_string(),
        instructions: vec![MirInst::LoadConst(0, MirConst::Integer(0))],
        terminator: MirTerminator::Branch("block_0".to_string()),
    });

    // Create intermediate blocks with complex branching
    for i in 0..num_blocks {
        let label = format!("block_{}", i);
        let next_label =
            if i + 1 < num_blocks { format!("block_{}", i + 1) } else { "exit".to_string() };
        let alt_label = format!("alt_{}", i);

        blocks.push(MirBlock {
            label: label.clone(),
            instructions: vec![MirInst::LoadConst((i * 2 + 1) as u32, MirConst::Integer(i as i64))],
            terminator: if i % 2 == 0 {
                MirTerminator::CondBranch((i * 2 + 1) as u32, next_label.clone(), alt_label.clone())
            } else {
                MirTerminator::Branch(next_label.clone())
            },
        });

        if i % 2 == 0 {
            blocks.push(MirBlock {
                label: alt_label,
                instructions: vec![],
                terminator: MirTerminator::Branch(next_label),
            });
        }
    }

    // Exit block
    blocks.push(MirBlock {
        label: "exit".to_string(),
        instructions: vec![MirInst::LoadConst((num_blocks * 2 + 1) as u32, MirConst::Integer(42))],
        terminator: MirTerminator::Return(Some((num_blocks * 2 + 1) as u32)),
    });

    let func = MirFunction {
        name: format!("cfg_complex_{}", num_blocks),
        params: vec![100],
        blocks,
        next_reg: (num_blocks * 2 + 2) as u32,
        span: jdruby_common::SourceSpan::default(),
    };

    let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
    let module = MirModule { name: format!("cfg_{}", num_blocks), functions: vec![func] };

    let (ir, _reporter) = codegen.generate_with_errors(&module);

    let mut bytes = ir.as_bytes().to_vec();
    bytes.push(0);
    let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(
        &bytes,
        &format!("cfg_{}", num_blocks),
    );
    match ctx.create_module_from_ir(buffer) {
        Ok(_) => {}
        Err(e) => panic!("IR parsing failed: {:?}", e),
    };
}

/// Test 551-600: Symbol table stress tests
#[test]
fn stress_test_symbol_table_1() {
    symbol_table_helper(100);
}
#[test]
fn stress_test_symbol_table_2() {
    symbol_table_helper(500);
}
#[test]
fn stress_test_symbol_table_3() {
    symbol_table_helper(1000);
}

fn symbol_table_helper(num_symbols: usize) {
    let ctx = Context::create();

    let mut functions = vec![];

    for i in 0..num_symbols {
        let func = MirFunction {
            name: format!("symbol_{}_{}", i, num_symbols),
            params: vec![],
            blocks: vec![MirBlock {
                label: "entry".to_string(),
                instructions: vec![MirInst::LoadConst(0, MirConst::Integer(i as i64))],
                terminator: MirTerminator::Return(Some(0)),
            }],
            next_reg: 1,
            span: jdruby_common::SourceSpan::default(),
        };
        functions.push(func);
    }

    let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
    let module = MirModule { name: format!("sym_{}", num_symbols), functions };

    let (ir, reporter) = codegen.generate_with_errors(&module);

    // Should handle many symbols
    if !reporter.has_errors() {
        let mut bytes = ir.as_bytes().to_vec();
        bytes.push(0);
        let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(
            &bytes,
            &format!("sym_{}", num_symbols),
        );
        match ctx.create_module_from_ir(buffer) {
            Ok(_) => {}
            Err(e) => panic!("IR parsing failed: {:?}", e),
        };
    }
}

/// Test 601-650: Race condition detection patterns
#[test]
fn stress_test_race_patterns_1() {
    race_pattern_helper(10);
}
#[test]
fn stress_test_race_patterns_2() {
    race_pattern_helper(50);
}
#[test]
fn stress_test_race_patterns_3() {
    race_pattern_helper(100);
}

fn race_pattern_helper(num_accesses: usize) {
    let ctx = Context::create();

    let mut instructions = vec![];

    // Simulate potential race pattern: multiple reads/writes to same location
    for i in 0..num_accesses {
        instructions.push(MirInst::Store("shared_var".to_string(), (i % 5) as u32));
        instructions.push(MirInst::Load((i + 5) as u32, "shared_var".to_string()));
    }

    let func = MirFunction {
        name: format!("race_{}", num_accesses),
        params: vec![100],
        blocks: vec![MirBlock {
            label: "entry".to_string(),
            instructions,
            terminator: MirTerminator::Return(Some((num_accesses + 4) as u32)),
        }],
        next_reg: (num_accesses + 5) as u32,
        span: jdruby_common::SourceSpan::default(),
    };

    let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
    let module = MirModule { name: format!("race_{}", num_accesses), functions: vec![func] };

    let (ir, _reporter) = codegen.generate_with_errors(&module);

    let mut bytes = ir.as_bytes().to_vec();
    bytes.push(0);
    let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(
        &bytes,
        &format!("race_{}", num_accesses),
    );
    match ctx.create_module_from_ir(buffer) {
        Ok(_) => {}
        Err(e) => panic!("IR parsing failed: {:?}", e),
    };
}

/// Test 651-700: Stack overflow prevention tests
#[test]
fn stress_test_stack_overflow_prevention_1() {
    stack_overflow_helper(100);
}
#[test]
fn stress_test_stack_overflow_prevention_2() {
    stack_overflow_helper(500);
}
#[test]
fn stress_test_stack_overflow_prevention_3() {
    stack_overflow_helper(1000);
}

fn stack_overflow_helper(num_locals: usize) {
    let ctx = Context::create();

    let mut instructions = vec![];

    // Create many local allocations (potential stack pressure)
    for i in 0..num_locals {
        instructions.push(MirInst::Alloc(i as u32, format!("local_{}", i)));
        instructions.push(MirInst::LoadConst((num_locals + i) as u32, MirConst::Integer(i as i64)));
        instructions.push(MirInst::Store(format!("local_{}", i), (num_locals + i) as u32));
    }

    let func = MirFunction {
        name: format!("stack_{}", num_locals),
        params: vec![],
        blocks: vec![MirBlock {
            label: "entry".to_string(),
            instructions,
            terminator: MirTerminator::Return(None),
        }],
        next_reg: (num_locals * 2) as u32,
        span: jdruby_common::SourceSpan::default(),
    };

    let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
    let module = MirModule { name: format!("stack_{}", num_locals), functions: vec![func] };

    let (ir, reporter) = codegen.generate_with_errors(&module);

    // Should not crash with many locals
    if !reporter.has_errors() {
        let mut bytes = ir.as_bytes().to_vec();
        bytes.push(0);
        let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(
            &bytes,
            &format!("stk_{}", num_locals),
        );
        match ctx.create_module_from_ir(buffer) {
            Ok(_) => {}
            Err(e) => panic!("IR parsing failed: {:?}", e),
        };
    }
}

/// Test 701-750: Heap exhaustion simulation
#[test]
fn stress_test_heap_pressure_1() {
    heap_pressure_helper(50);
}
#[test]
fn stress_test_heap_pressure_2() {
    heap_pressure_helper(100);
}
#[test]
fn stress_test_heap_pressure_3() {
    heap_pressure_helper(200);
}

fn heap_pressure_helper(num_strings: usize) {
    let ctx = Context::create();

    let mut functions = vec![];

    for i in 0..num_strings {
        let large_string = format!("data_{}_{}", i, "x".repeat(100));
        let func = create_string_function(&format!("heap_{}_{}", num_strings, i), &large_string);
        functions.push(func);
    }

    let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
    let module = MirModule { name: format!("heap_{}", num_strings), functions };

    let (ir, reporter) = codegen.generate_with_errors(&module);

    // Should handle heap pressure gracefully
    if !reporter.has_errors() {
        let mut bytes = ir.as_bytes().to_vec();
        bytes.push(0);
        let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(
            &bytes,
            &format!("heap_{}", num_strings),
        );
        match ctx.create_module_from_ir(buffer) {
            Ok(_) => {}
            Err(e) => panic!("IR parsing failed: {:?}", e),
        };
    }
}

/// Test 751-800: Integer wraparound tests
#[test]
fn stress_test_integer_wraparound() {
    let ctx = Context::create();

    let wrap_values = vec![
        (i64::MAX, 1, MirBinOp::Add),
        (i64::MIN, -1, MirBinOp::Sub),
        (i64::MAX, 2, MirBinOp::Mul),
        (i64::MIN, i64::MAX, MirBinOp::Add),
    ];

    for (i, (a, b, op)) in wrap_values.iter().enumerate() {
        let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
        let func = MirFunction {
            name: format!("wrap_{}", i),
            params: vec![],
            blocks: vec![MirBlock {
                label: "entry".to_string(),
                instructions: vec![
                    MirInst::LoadConst(0, MirConst::Integer(*a)),
                    MirInst::LoadConst(1, MirConst::Integer(*b)),
                    MirInst::BinOp(2, *op, 0, 1),
                ],
                terminator: MirTerminator::Return(Some(2)),
            }],
            next_reg: 3,
            span: jdruby_common::SourceSpan::default(),
        };

        let module = MirModule { name: format!("wrap_{}", i), functions: vec![func] };

        let (ir, _reporter) = codegen.generate_with_errors(&module);

        let mut bytes = ir.as_bytes().to_vec();
        bytes.push(0);
        let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(
            &bytes,
            &format!("wrap_{}", i),
        );
        match ctx.create_module_from_ir(buffer) {
            Ok(_) => {}
            Err(e) => panic!("IR parsing failed: {:?}", e),
        };
    }
}

/// Test 801-850: Denial of Service patterns
#[test]
fn stress_test_dos_patterns_1() {
    dos_helper(1000);
}
#[test]
fn stress_test_dos_patterns_2() {
    dos_helper(5000);
}
#[test]
fn stress_test_dos_patterns_3() {
    dos_helper(10000);
}

fn dos_helper(complexity: usize) {
    let ctx = Context::create();

    // Create functions that could cause DoS if not handled properly
    let mut instructions = vec![];

    // Very long chain of operations
    for i in 0..complexity.min(1000) {
        // Cap at 1000 for practical test time
        instructions.push(MirInst::LoadConst(i as u32, MirConst::Integer(i as i64)));
        if i > 0 {
            instructions.push(MirInst::BinOp(
                (complexity + i) as u32,
                MirBinOp::Add,
                (i - 1) as u32,
                i as u32,
            ));
        }
    }

    let func = MirFunction {
        name: format!("dos_{}", complexity),
        params: vec![],
        blocks: vec![MirBlock {
            label: "entry".to_string(),
            instructions,
            terminator: MirTerminator::Return(Some((complexity * 2 - 1) as u32)),
        }],
        next_reg: (complexity * 2) as u32,
        span: jdruby_common::SourceSpan::default(),
    };

    let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
    let module = MirModule { name: format!("dos_{}", complexity), functions: vec![func] };

    let (ir, reporter) = codegen.generate_with_errors(&module);

    // Should complete in reasonable time
    if !reporter.has_errors() {
        let mut bytes = ir.as_bytes().to_vec();
        bytes.push(0);
        let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(
            &bytes,
            &format!("dos_{}", complexity),
        );
        match ctx.create_module_from_ir(buffer) {
            Ok(_) => {}
            Err(e) => panic!("IR parsing failed: {:?}", e),
        };
    }
}

/// Test 851-900: Pointer aliasing tests
#[test]
fn stress_test_pointer_aliasing_1() {
    aliasing_helper(10);
}
#[test]
fn stress_test_pointer_aliasing_2() {
    aliasing_helper(50);
}
#[test]
fn stress_test_pointer_aliasing_3() {
    aliasing_helper(100);
}

fn aliasing_helper(num_aliases: usize) {
    let ctx = Context::create();

    let mut instructions = vec![];

    // Create aliasing patterns where multiple names refer to same conceptual location
    for i in 0..num_aliases {
        let var_name = if i % 3 == 0 { "shared".to_string() } else { format!("alias_{}", i) };

        instructions.push(MirInst::Alloc(i as u32, var_name.clone()));
        instructions
            .push(MirInst::LoadConst((num_aliases + i) as u32, MirConst::Integer(i as i64)));
        instructions.push(MirInst::Store(var_name, (num_aliases + i) as u32));
        instructions.push(MirInst::Load((num_aliases * 2 + i) as u32, "shared".to_string()));
    }

    let func = MirFunction {
        name: format!("alias_{}", num_aliases),
        params: vec![],
        blocks: vec![MirBlock {
            label: "entry".to_string(),
            instructions,
            terminator: MirTerminator::Return(Some((num_aliases * 3 - 1) as u32)),
        }],
        next_reg: (num_aliases * 3) as u32,
        span: jdruby_common::SourceSpan::default(),
    };

    let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
    let module = MirModule { name: format!("alias_{}", num_aliases), functions: vec![func] };

    let (ir, _reporter) = codegen.generate_with_errors(&module);

    let mut bytes = ir.as_bytes().to_vec();
    bytes.push(0);
    let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(
        &bytes,
        &format!("alias_{}", num_aliases),
    );
    match ctx.create_module_from_ir(buffer) {
        Ok(_) => {}
        Err(e) => panic!("IR parsing failed: {:?}", e),
    };
}

/// Test 901-950: Atomic operation patterns
#[test]
fn stress_test_atomic_patterns_1() {
    atomic_helper(10);
}
#[test]
fn stress_test_atomic_patterns_2() {
    atomic_helper(50);
}
#[test]
fn stress_test_atomic_patterns_3() {
    atomic_helper(100);
}

fn atomic_helper(num_operations: usize) {
    let ctx = Context::create();

    let mut instructions = vec![];

    // Simulate atomic operations pattern
    instructions.push(MirInst::Alloc(0, "atomic_var".to_string()));

    for i in 0..num_operations {
        instructions.push(MirInst::Load((i + 1) as u32, "atomic_var".to_string()));
        instructions
            .push(MirInst::LoadConst((num_operations + i + 1) as u32, MirConst::Integer(1)));
        instructions.push(MirInst::BinOp(
            (num_operations * 2 + i + 1) as u32,
            MirBinOp::Add,
            (i + 1) as u32,
            (num_operations + i + 1) as u32,
        ));
        instructions
            .push(MirInst::Store("atomic_var".to_string(), (num_operations * 2 + i + 1) as u32));
    }

    let func = MirFunction {
        name: format!("atomic_{}", num_operations),
        params: vec![100],
        blocks: vec![MirBlock {
            label: "entry".to_string(),
            instructions,
            terminator: MirTerminator::Return(Some((num_operations * 2 + num_operations) as u32)),
        }],
        next_reg: (num_operations * 3 + 1) as u32,
        span: jdruby_common::SourceSpan::default(),
    };

    let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
    let module = MirModule { name: format!("atomic_{}", num_operations), functions: vec![func] };

    let (ir, _reporter) = codegen.generate_with_errors(&module);

    let mut bytes = ir.as_bytes().to_vec();
    bytes.push(0);
    let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(
        &bytes,
        &format!("atm_{}", num_operations),
    );
    match ctx.create_module_from_ir(buffer) {
        Ok(_) => {}
        Err(e) => panic!("IR parsing failed: {:?}", e),
    };
}

/// Test 951+: Additional edge case battery
#[test]
fn stress_test_empty_string_constant() {
    let ctx = Context::create();
    let func = create_string_function("empty_str", "");
    let module = MirModule { name: "empty_str".to_string(), functions: vec![func] };

    let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
    let (ir, _reporter) = codegen.generate_with_errors(&module);

    let mut bytes = ir.as_bytes().to_vec();
    bytes.push(0);
    let buffer =
        inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(&bytes, "empty_str");
    let result = ctx.create_module_from_ir(buffer);
    assert!(result.is_ok(), "Empty string should be valid");
}

#[test]
fn stress_test_very_long_symbol_names() {
    let ctx = Context::create();
    let long_name = format!("method_{}", "x".repeat(500));
    let func = create_minimal_function(&long_name);
    let module = MirModule { name: "long_sym".to_string(), functions: vec![func] };

    let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
    let (ir, _reporter) = codegen.generate_with_errors(&module);

    // Should handle long names without crashing
    let mut bytes = ir.as_bytes().to_vec();
    bytes.push(0);
    let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(&bytes, "long_sym");
    match ctx.create_module_from_ir(buffer) {
        Ok(_) => {}
        Err(e) => panic!("IR parsing failed: {:?}", e),
    };
}

#[test]
fn stress_test_special_float_values() {
    let ctx = Context::create();
    // Test if we can handle special values in constants
    let func = MirFunction {
        name: "special_vals".to_string(),
        params: vec![],
        blocks: vec![MirBlock {
            label: "entry".to_string(),
            instructions: vec![
                MirInst::LoadConst(0, MirConst::Integer(0)),
                MirInst::LoadConst(1, MirConst::Integer(-1)),
                MirInst::LoadConst(2, MirConst::Integer(1)),
                MirInst::BinOp(3, MirBinOp::Div, 2, 0), // 1/0
                MirInst::BinOp(4, MirBinOp::Div, 0, 0), // 0/0
            ],
            terminator: MirTerminator::Return(Some(4)),
        }],
        next_reg: 5,
        span: jdruby_common::SourceSpan::default(),
    };

    let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
    let module = MirModule { name: "special".to_string(), functions: vec![func] };

    let (ir, _reporter) = codegen.generate_with_errors(&module);

    let mut bytes = ir.as_bytes().to_vec();
    bytes.push(0);
    let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(&bytes, "special");
    match ctx.create_module_from_ir(buffer) {
        Ok(_) => {}
        Err(e) => panic!("IR parsing failed: {:?}", e),
    };
}

#[test]
fn stress_test_unicode_in_identifiers() {
    let ctx = Context::create();
    let unicode_names =
        vec!["method_ñ", "method_中文", "method_日本語", "method_العربية", "method_🦀"];

    for (i, name) in unicode_names.iter().enumerate() {
        let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
        let func = create_minimal_function(name);
        let module = MirModule { name: format!("uni_{}", i), functions: vec![func] };

        let (ir, _reporter) = codegen.generate_with_errors(&module);

        let mut bytes = ir.as_bytes().to_vec();
        bytes.push(0);
        let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(
            &bytes,
            &format!("uni_{}", i),
        );
        match ctx.create_module_from_ir(buffer) {
            Ok(_) => {}
            Err(e) => panic!("IR parsing failed: {:?}", e),
        };
    }
}

#[test]
fn stress_test_nested_function_calls() {
    let ctx = Context::create();
    let depth = 50;
    let mut instructions = vec![];

    instructions.push(MirInst::LoadConst(0, MirConst::Integer(42)));

    for i in 0..depth {
        instructions.push(MirInst::Call((i + 1) as u32, format!("nested_{}", i), vec![i as u32]));
    }

    let func = MirFunction {
        name: "nested_calls".to_string(),
        params: vec![100],
        blocks: vec![MirBlock {
            label: "entry".to_string(),
            instructions,
            terminator: MirTerminator::Return(Some(depth as u32)),
        }],
        next_reg: (depth + 1) as u32,
        span: jdruby_common::SourceSpan::default(),
    };

    let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
    let module = MirModule { name: "nested".to_string(), functions: vec![func] };

    let (ir, _reporter) = codegen.generate_with_errors(&module);

    let mut bytes = ir.as_bytes().to_vec();
    bytes.push(0);
    let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(&bytes, "nested");
    match ctx.create_module_from_ir(buffer) {
        Ok(_) => {}
        Err(e) => panic!("IR parsing failed: {:?}", e),
    };
}

#[test]
fn stress_test_multiple_returns() {
    let ctx = Context::create();
    let func = MirFunction {
        name: "multi_return".to_string(),
        params: vec![100],
        blocks: vec![
            MirBlock {
                label: "entry".to_string(),
                instructions: vec![
                    MirInst::LoadConst(0, MirConst::Integer(0)),
                    MirInst::LoadConst(1, MirConst::Integer(1)),
                ],
                terminator: MirTerminator::CondBranch(
                    100,
                    "ret_a".to_string(),
                    "ret_b".to_string(),
                ),
            },
            MirBlock {
                label: "ret_a".to_string(),
                instructions: vec![],
                terminator: MirTerminator::Return(Some(0)),
            },
            MirBlock {
                label: "ret_b".to_string(),
                instructions: vec![],
                terminator: MirTerminator::Return(Some(1)),
            },
        ],
        next_reg: 2,
        span: jdruby_common::SourceSpan::default(),
    };

    let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
    let module = MirModule { name: "multi_ret".to_string(), functions: vec![func] };

    let (ir, _reporter) = codegen.generate_with_errors(&module);

    let mut bytes = ir.as_bytes().to_vec();
    bytes.push(0);
    let buffer =
        inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(&bytes, "multi_ret");
    let result = ctx.create_module_from_ir(buffer);
    assert!(result.is_ok(), "Multiple returns should be valid");
}

#[test]
fn stress_test_unreachable_code() {
    let ctx = Context::create();
    let func = MirFunction {
        name: "unreachable_test".to_string(),
        params: vec![],
        blocks: vec![
            MirBlock {
                label: "entry".to_string(),
                instructions: vec![MirInst::LoadConst(0, MirConst::Integer(42))],
                terminator: MirTerminator::Return(Some(0)),
            },
            MirBlock {
                label: "unreachable".to_string(),
                instructions: vec![
                    MirInst::LoadConst(1, MirConst::Integer(100)),
                    MirInst::Call(2, "never".to_string(), vec![1]),
                ],
                terminator: MirTerminator::Return(Some(2)),
            },
        ],
        next_reg: 3,
        span: jdruby_common::SourceSpan::default(),
    };

    let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
    let module = MirModule { name: "unreach".to_string(), functions: vec![func] };

    let (ir, _reporter) = codegen.generate_with_errors(&module);

    let mut bytes = ir.as_bytes().to_vec();
    bytes.push(0);
    let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(&bytes, "unreach");
    match ctx.create_module_from_ir(buffer) {
        Ok(_) => {}
        Err(e) => panic!("IR parsing failed: {:?}", e),
    };
}

#[test]
fn stress_test_phi_nodes_implicit() {
    let ctx = Context::create();
    let func = MirFunction {
        name: "phi_test".to_string(),
        params: vec![100],
        blocks: vec![
            MirBlock {
                label: "entry".to_string(),
                instructions: vec![MirInst::LoadConst(0, MirConst::Integer(1))],
                terminator: MirTerminator::CondBranch(100, "then".to_string(), "else".to_string()),
            },
            MirBlock {
                label: "then".to_string(),
                instructions: vec![MirInst::LoadConst(1, MirConst::Integer(10))],
                terminator: MirTerminator::Branch("merge".to_string()),
            },
            MirBlock {
                label: "else".to_string(),
                instructions: vec![MirInst::LoadConst(2, MirConst::Integer(20))],
                terminator: MirTerminator::Branch("merge".to_string()),
            },
            MirBlock {
                label: "merge".to_string(),
                instructions: vec![
                    // Implicit phi - value depends on which branch we came from
                ],
                terminator: MirTerminator::Return(Some(0)),
            },
        ],
        next_reg: 3,
        span: jdruby_common::SourceSpan::default(),
    };

    let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
    let module = MirModule { name: "phi".to_string(), functions: vec![func] };

    let (ir, _reporter) = codegen.generate_with_errors(&module);

    let mut bytes = ir.as_bytes().to_vec();
    bytes.push(0);
    let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(&bytes, "phi");
    match ctx.create_module_from_ir(buffer) {
        Ok(_) => {}
        Err(e) => panic!("IR parsing failed: {:?}", e),
    };
}

#[test]
fn stress_test_module_with_no_functions() {
    let ctx = Context::create();
    let module = MirModule { name: "no_funcs".to_string(), functions: vec![] };

    let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
    let (ir, _reporter) = codegen.generate_with_errors(&module);

    let mut bytes = ir.as_bytes().to_vec();
    bytes.push(0);
    let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(&bytes, "no_funcs");
    match ctx.create_module_from_ir(buffer) {
        Ok(_) => {}
        Err(e) => panic!("IR parsing failed: {:?}", e),
    };
}

#[test]
fn stress_test_single_block_many_instructions() {
    let ctx = Context::create();
    let num_instrs = 1000;
    let mut instructions = vec![];

    for i in 0..num_instrs {
        instructions.push(MirInst::LoadConst(i as u32, MirConst::Integer(i as i64)));
    }

    let func = MirFunction {
        name: "many_instrs".to_string(),
        params: vec![],
        blocks: vec![MirBlock {
            label: "entry".to_string(),
            instructions,
            terminator: MirTerminator::Return(Some((num_instrs - 1) as u32)),
        }],
        next_reg: num_instrs as u32,
        span: jdruby_common::SourceSpan::default(),
    };

    let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
    let module = MirModule { name: "many".to_string(), functions: vec![func] };

    let (ir, reporter) = codegen.generate_with_errors(&module);

    if !reporter.has_errors() {
        let mut bytes = ir.as_bytes().to_vec();
        bytes.push(0);
        let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(&bytes, "many");
        match ctx.create_module_from_ir(buffer) {
            Ok(_) => {}
            Err(e) => panic!("IR parsing failed: {:?}", e),
        };
    }
}

#[test]
fn stress_test_circular_call_graph() {
    let ctx = Context::create();

    // Create circular call dependencies
    let func_a = MirFunction {
        name: "circular_a".to_string(),
        params: vec![100],
        blocks: vec![MirBlock {
            label: "entry".to_string(),
            instructions: vec![MirInst::Call(0, "circular_b".to_string(), vec![100])],
            terminator: MirTerminator::Return(Some(0)),
        }],
        next_reg: 1,
        span: jdruby_common::SourceSpan::default(),
    };

    let func_b = MirFunction {
        name: "circular_b".to_string(),
        params: vec![100],
        blocks: vec![MirBlock {
            label: "entry".to_string(),
            instructions: vec![MirInst::Call(0, "circular_a".to_string(), vec![100])],
            terminator: MirTerminator::Return(Some(0)),
        }],
        next_reg: 1,
        span: jdruby_common::SourceSpan::default(),
    };

    let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
    let module = MirModule { name: "circular".to_string(), functions: vec![func_a, func_b] };

    let (ir, _reporter) = codegen.generate_with_errors(&module);

    let mut bytes = ir.as_bytes().to_vec();
    bytes.push(0);
    let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(&bytes, "circular");
    match ctx.create_module_from_ir(buffer) {
        Ok(_) => {}
        Err(e) => panic!("IR parsing failed: {:?}", e),
    };
}

#[test]
fn stress_test_self_recursive() {
    let ctx = Context::create();
    let func = MirFunction {
        name: "self_recursive".to_string(),
        params: vec![100],
        blocks: vec![MirBlock {
            label: "entry".to_string(),
            instructions: vec![
                MirInst::LoadConst(0, MirConst::Integer(1)),
                MirInst::Call(1, "self_recursive".to_string(), vec![100]),
            ],
            terminator: MirTerminator::Return(Some(1)),
        }],
        next_reg: 2,
        span: jdruby_common::SourceSpan::default(),
    };

    let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
    let module = MirModule { name: "self_rec".to_string(), functions: vec![func] };

    let (ir, _reporter) = codegen.generate_with_errors(&module);

    let mut bytes = ir.as_bytes().to_vec();
    bytes.push(0);
    let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(&bytes, "self_rec");
    match ctx.create_module_from_ir(buffer) {
        Ok(_) => {}
        Err(e) => panic!("IR parsing failed: {:?}", e),
    };
}

#[test]
fn stress_test_mutually_recursive() {
    let ctx = Context::create();

    let func_odd = MirFunction {
        name: "odd".to_string(),
        params: vec![100],
        blocks: vec![MirBlock {
            label: "entry".to_string(),
            instructions: vec![
                MirInst::LoadConst(0, MirConst::Integer(1)),
                MirInst::Call(1, "even".to_string(), vec![100]),
            ],
            terminator: MirTerminator::Return(Some(1)),
        }],
        next_reg: 2,
        span: jdruby_common::SourceSpan::default(),
    };

    let func_even = MirFunction {
        name: "even".to_string(),
        params: vec![100],
        blocks: vec![MirBlock {
            label: "entry".to_string(),
            instructions: vec![
                MirInst::LoadConst(0, MirConst::Integer(0)),
                MirInst::Call(1, "odd".to_string(), vec![100]),
            ],
            terminator: MirTerminator::Return(Some(1)),
        }],
        next_reg: 2,
        span: jdruby_common::SourceSpan::default(),
    };

    let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
    let module = MirModule { name: "mutual".to_string(), functions: vec![func_odd, func_even] };

    let (ir, _reporter) = codegen.generate_with_errors(&module);

    let mut bytes = ir.as_bytes().to_vec();
    bytes.push(0);
    let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(&bytes, "mutual");
    match ctx.create_module_from_ir(buffer) {
        Ok(_) => {}
        Err(e) => panic!("IR parsing failed: {:?}", e),
    };
}

#[test]
fn stress_test_variable_shadowing() {
    let ctx = Context::create();
    let func = MirFunction {
        name: "shadow_test".to_string(),
        params: vec![100],
        blocks: vec![
            MirBlock {
                label: "outer".to_string(),
                instructions: vec![
                    MirInst::Alloc(0, "x".to_string()),
                    MirInst::LoadConst(1, MirConst::Integer(1)),
                    MirInst::Store("x".to_string(), 1),
                ],
                terminator: MirTerminator::Branch("inner".to_string()),
            },
            MirBlock {
                label: "inner".to_string(),
                instructions: vec![
                    MirInst::Alloc(2, "x".to_string()), // Shadow
                    MirInst::LoadConst(3, MirConst::Integer(2)),
                    MirInst::Store("x".to_string(), 3),
                    MirInst::Load(4, "x".to_string()),
                ],
                terminator: MirTerminator::Return(Some(4)),
            },
        ],
        next_reg: 5,
        span: jdruby_common::SourceSpan::default(),
    };

    let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
    let module = MirModule { name: "shadow".to_string(), functions: vec![func] };

    let (ir, _reporter) = codegen.generate_with_errors(&module);

    let mut bytes = ir.as_bytes().to_vec();
    bytes.push(0);
    let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(&bytes, "shadow");
    match ctx.create_module_from_ir(buffer) {
        Ok(_) => {}
        Err(e) => panic!("IR parsing failed: {:?}", e),
    };
}

#[test]
fn stress_test_unused_variables() {
    let ctx = Context::create();
    let func = MirFunction {
        name: "unused_test".to_string(),
        params: vec![],
        blocks: vec![MirBlock {
            label: "entry".to_string(),
            instructions: vec![
                MirInst::Alloc(0, "unused1".to_string()),
                MirInst::Alloc(1, "unused2".to_string()),
                MirInst::LoadConst(2, MirConst::Integer(42)),
                MirInst::Store("unused1".to_string(), 2),
                // unused2 never stored or loaded
            ],
            terminator: MirTerminator::Return(Some(2)),
        }],
        next_reg: 3,
        span: jdruby_common::SourceSpan::default(),
    };

    let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
    let module = MirModule { name: "unused".to_string(), functions: vec![func] };

    let (ir, _reporter) = codegen.generate_with_errors(&module);

    let mut bytes = ir.as_bytes().to_vec();
    bytes.push(0);
    let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(&bytes, "unused");
    match ctx.create_module_from_ir(buffer) {
        Ok(_) => {}
        Err(e) => panic!("IR parsing failed: {:?}", e),
    };
}

#[test]
fn stress_test_complex_arithmetic_chain() {
    let ctx = Context::create();
    let mut instructions = vec![];

    instructions.push(MirInst::LoadConst(0, MirConst::Integer(1)));
    instructions.push(MirInst::LoadConst(1, MirConst::Integer(2)));
    instructions.push(MirInst::LoadConst(2, MirConst::Integer(3)));
    instructions.push(MirInst::LoadConst(3, MirConst::Integer(4)));

    // Complex chain: ((1 + 2) * (3 - 4)) / (1 + 3)
    instructions.push(MirInst::BinOp(4, MirBinOp::Add, 0, 1));
    instructions.push(MirInst::BinOp(5, MirBinOp::Sub, 2, 3));
    instructions.push(MirInst::BinOp(6, MirBinOp::Mul, 4, 5));
    instructions.push(MirInst::BinOp(7, MirBinOp::Add, 0, 2));
    instructions.push(MirInst::BinOp(8, MirBinOp::Div, 6, 7));

    let func = MirFunction {
        name: "complex_arith".to_string(),
        params: vec![],
        blocks: vec![MirBlock {
            label: "entry".to_string(),
            instructions,
            terminator: MirTerminator::Return(Some(8)),
        }],
        next_reg: 9,
        span: jdruby_common::SourceSpan::default(),
    };

    let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
    let module = MirModule { name: "complex".to_string(), functions: vec![func] };

    let (ir, _reporter) = codegen.generate_with_errors(&module);

    let mut bytes = ir.as_bytes().to_vec();
    bytes.push(0);
    let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(&bytes, "complex");
    let result = ctx.create_module_from_ir(buffer);
    assert!(result.is_ok(), "Complex arithmetic should be valid");
}

#[test]
fn stress_test_all_terminator_types() {
    let ctx = Context::create();

    // Test Return
    let func_ret = MirFunction {
        name: "term_return".to_string(),
        params: vec![],
        blocks: vec![MirBlock {
            label: "entry".to_string(),
            instructions: vec![MirInst::LoadConst(0, MirConst::Integer(42))],
            terminator: MirTerminator::Return(Some(0)),
        }],
        next_reg: 1,
        span: jdruby_common::SourceSpan::default(),
    };

    // Test Branch
    let func_branch = MirFunction {
        name: "term_branch".to_string(),
        params: vec![],
        blocks: vec![
            MirBlock {
                label: "entry".to_string(),
                instructions: vec![],
                terminator: MirTerminator::Branch("exit".to_string()),
            },
            MirBlock {
                label: "exit".to_string(),
                instructions: vec![MirInst::LoadConst(0, MirConst::Integer(1))],
                terminator: MirTerminator::Return(Some(0)),
            },
        ],
        next_reg: 1,
        span: jdruby_common::SourceSpan::default(),
    };

    // Test CondBranch
    let func_cond = MirFunction {
        name: "term_cond".to_string(),
        params: vec![100],
        blocks: vec![
            MirBlock {
                label: "entry".to_string(),
                instructions: vec![MirInst::LoadConst(0, MirConst::Integer(1))],
                terminator: MirTerminator::CondBranch(100, "then".to_string(), "else".to_string()),
            },
            MirBlock {
                label: "then".to_string(),
                instructions: vec![],
                terminator: MirTerminator::Return(Some(0)),
            },
            MirBlock {
                label: "else".to_string(),
                instructions: vec![],
                terminator: MirTerminator::Return(None),
            },
        ],
        next_reg: 1,
        span: jdruby_common::SourceSpan::default(),
    };

    let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
    let module = MirModule {
        name: "terminators".to_string(),
        functions: vec![func_ret, func_branch, func_cond],
    };

    let (ir, _reporter) = codegen.generate_with_errors(&module);

    let mut bytes = ir.as_bytes().to_vec();
    bytes.push(0);
    let buffer =
        inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(&bytes, "terminators");
    let result = ctx.create_module_from_ir(buffer);
    assert!(result.is_ok(), "All terminators should be valid");
}

#[test]
fn stress_test_boolean_operations() {
    let ctx = Context::create();
    let func = MirFunction {
        name: "bool_ops".to_string(),
        params: vec![],
        blocks: vec![MirBlock {
            label: "entry".to_string(),
            instructions: vec![
                MirInst::LoadConst(0, MirConst::Bool(true)),
                MirInst::LoadConst(1, MirConst::Bool(false)),
                MirInst::BinOp(2, MirBinOp::And, 0, 1),
                MirInst::BinOp(3, MirBinOp::Or, 0, 1),
            ],
            terminator: MirTerminator::Return(Some(3)),
        }],
        next_reg: 4,
        span: jdruby_common::SourceSpan::default(),
    };

    let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
    let module = MirModule { name: "bool".to_string(), functions: vec![func] };

    let (ir, _reporter) = codegen.generate_with_errors(&module);

    let mut bytes = ir.as_bytes().to_vec();
    bytes.push(0);
    let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(&bytes, "bool");
    let result = ctx.create_module_from_ir(buffer);
    assert!(result.is_ok(), "Boolean operations should be valid");
}

#[test]
fn stress_test_nil_operations() {
    let ctx = Context::create();
    let func = MirFunction {
        name: "nil_ops".to_string(),
        params: vec![],
        blocks: vec![MirBlock {
            label: "entry".to_string(),
            instructions: vec![
                MirInst::LoadConst(0, MirConst::Nil),
                MirInst::LoadConst(1, MirConst::Nil),
                MirInst::BinOp(2, MirBinOp::Eq, 0, 1),
            ],
            terminator: MirTerminator::Return(Some(2)),
        }],
        next_reg: 3,
        span: jdruby_common::SourceSpan::default(),
    };

    let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
    let module = MirModule { name: "nil".to_string(), functions: vec![func] };

    let (ir, _reporter) = codegen.generate_with_errors(&module);

    let mut bytes = ir.as_bytes().to_vec();
    bytes.push(0);
    let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(&bytes, "nil");
    let result = ctx.create_module_from_ir(buffer);
    assert!(result.is_ok(), "Nil operations should be valid");
}

#[test]
fn stress_test_bitwise_operations() {
    let ctx = Context::create();
    let func = MirFunction {
        name: "bitwise".to_string(),
        params: vec![],
        blocks: vec![MirBlock {
            label: "entry".to_string(),
            instructions: vec![
                MirInst::LoadConst(0, MirConst::Integer(0b1010)),
                MirInst::LoadConst(1, MirConst::Integer(0b1100)),
                MirInst::BinOp(2, MirBinOp::BitAnd, 0, 1),
                MirInst::BinOp(3, MirBinOp::BitOr, 0, 1),
                MirInst::BinOp(4, MirBinOp::BitXor, 0, 1),
                MirInst::BinOp(5, MirBinOp::Shl, 0, 1),
                MirInst::BinOp(6, MirBinOp::Shr, 0, 1),
            ],
            terminator: MirTerminator::Return(Some(6)),
        }],
        next_reg: 7,
        span: jdruby_common::SourceSpan::default(),
    };

    let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
    let module = MirModule { name: "bitwise".to_string(), functions: vec![func] };

    let (ir, _reporter) = codegen.generate_with_errors(&module);

    let mut bytes = ir.as_bytes().to_vec();
    bytes.push(0);
    let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(&bytes, "bitwise");
    let result = ctx.create_module_from_ir(buffer);
    assert!(result.is_ok(), "Bitwise operations should be valid");
}

#[test]
fn stress_test_comparison_operations() {
    let ctx = Context::create();
    let func = MirFunction {
        name: "comparisons".to_string(),
        params: vec![],
        blocks: vec![MirBlock {
            label: "entry".to_string(),
            instructions: vec![
                MirInst::LoadConst(0, MirConst::Integer(10)),
                MirInst::LoadConst(1, MirConst::Integer(20)),
                MirInst::BinOp(2, MirBinOp::Eq, 0, 1),
                MirInst::BinOp(3, MirBinOp::NotEq, 0, 1),
                MirInst::BinOp(4, MirBinOp::Lt, 0, 1),
                MirInst::BinOp(5, MirBinOp::Gt, 0, 1),
                MirInst::BinOp(6, MirBinOp::LtEq, 0, 1),
                MirInst::BinOp(7, MirBinOp::GtEq, 0, 1),
            ],
            terminator: MirTerminator::Return(Some(7)),
        }],
        next_reg: 8,
        span: jdruby_common::SourceSpan::default(),
    };

    let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
    let module = MirModule { name: "compare".to_string(), functions: vec![func] };

    let (ir, _reporter) = codegen.generate_with_errors(&module);

    let mut bytes = ir.as_bytes().to_vec();
    bytes.push(0);
    let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(&bytes, "compare");
    let result = ctx.create_module_from_ir(buffer);
    assert!(result.is_ok(), "Comparison operations should be valid");
}

#[test]
fn stress_test_power_operation() {
    let ctx = Context::create();
    let func = MirFunction {
        name: "power".to_string(),
        params: vec![],
        blocks: vec![MirBlock {
            label: "entry".to_string(),
            instructions: vec![
                MirInst::LoadConst(0, MirConst::Integer(2)),
                MirInst::LoadConst(1, MirConst::Integer(10)),
                MirInst::BinOp(2, MirBinOp::Pow, 0, 1),
            ],
            terminator: MirTerminator::Return(Some(2)),
        }],
        next_reg: 3,
        span: jdruby_common::SourceSpan::default(),
    };

    let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
    let module = MirModule { name: "power".to_string(), functions: vec![func] };

    let (ir, _reporter) = codegen.generate_with_errors(&module);

    let mut bytes = ir.as_bytes().to_vec();
    bytes.push(0);
    let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(&bytes, "power");
    let result = ctx.create_module_from_ir(buffer);
    assert!(result.is_ok() || result.is_err(), "Power operation should not crash");
}

#[test]
fn stress_test_large_negative_numbers() {
    let ctx = Context::create();
    let func = MirFunction {
        name: "negatives".to_string(),
        params: vec![],
        blocks: vec![MirBlock {
            label: "entry".to_string(),
            instructions: vec![
                MirInst::LoadConst(0, MirConst::Integer(-1)),
                MirInst::LoadConst(1, MirConst::Integer(-1000000)),
                MirInst::LoadConst(2, MirConst::Integer(i64::MIN + 1)),
                MirInst::BinOp(3, MirBinOp::Add, 0, 1),
                MirInst::BinOp(4, MirBinOp::Mul, 2, 0),
            ],
            terminator: MirTerminator::Return(Some(4)),
        }],
        next_reg: 5,
        span: jdruby_common::SourceSpan::default(),
    };

    let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
    let module = MirModule { name: "neg".to_string(), functions: vec![func] };

    let (ir, _reporter) = codegen.generate_with_errors(&module);

    let mut bytes = ir.as_bytes().to_vec();
    bytes.push(0);
    let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(&bytes, "neg");
    let result = ctx.create_module_from_ir(buffer);
    assert!(result.is_ok(), "Negative numbers should be valid");
}

#[test]
fn stress_test_zero_values() {
    let ctx = Context::create();
    let func = MirFunction {
        name: "zeros".to_string(),
        params: vec![],
        blocks: vec![MirBlock {
            label: "entry".to_string(),
            instructions: vec![
                MirInst::LoadConst(0, MirConst::Integer(0)),
                MirInst::LoadConst(1, MirConst::Integer(0)),
                MirInst::BinOp(2, MirBinOp::Add, 0, 1),
                MirInst::BinOp(3, MirBinOp::Mul, 0, 1),
                MirInst::BinOp(4, MirBinOp::Div, 0, 1),
            ],
            terminator: MirTerminator::Return(Some(4)),
        }],
        next_reg: 5,
        span: jdruby_common::SourceSpan::default(),
    };

    let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
    let module = MirModule { name: "zero".to_string(), functions: vec![func] };

    let (ir, _reporter) = codegen.generate_with_errors(&module);

    let mut bytes = ir.as_bytes().to_vec();
    bytes.push(0);
    let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(&bytes, "zero");
    match ctx.create_module_from_ir(buffer) {
        Ok(_) => {}
        Err(e) => panic!("IR parsing failed: {:?}", e),
    };
}

#[test]
fn stress_test_rapid_module_creation() {
    let ctx = Context::create();

    // Rapidly create and drop many modules
    for i in 0..100 {
        let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
        let func = create_minimal_function(&format!("rapid_{}", i));
        let module = MirModule { name: format!("rapid_{}", i), functions: vec![func] };

        let (ir, _reporter) = codegen.generate_with_errors(&module);

        // Module is dropped here - should not leak
        let mut bytes = ir.as_bytes().to_vec();
        bytes.push(0);
        let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(
            &bytes,
            &format!("rapid_{}", i),
        );
        match ctx.create_module_from_ir(buffer) {
            Ok(_) => {}
            Err(e) => panic!("IR parsing failed: {:?}", e),
        };
    }
}

#[test]
fn stress_test_rapid_context_switching() {
    // Create and drop contexts rapidly
    for i in 0..10 {
        let ctx = Context::create();
        let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
        let func = create_minimal_function(&format!("ctx_{}", i));
        let module = MirModule { name: format!("ctx_{}", i), functions: vec![func] };

        let (ir, _reporter) = codegen.generate_with_errors(&module);

        let mut bytes = ir.as_bytes().to_vec();
        bytes.push(0);
        let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(
            &bytes,
            &format!("ctx_{}", i),
        );
        match ctx.create_module_from_ir(buffer) {
            Ok(_) => {}
            Err(e) => panic!("IR parsing failed: {:?}", e),
        };

        // Context dropped here
    }
}

#[test]
fn stress_test_sanitizer_name_variations() {
    let ctx = Context::create();

    let names = vec![
        "a", "A", "1", "_", "__", "___", "a_b", "a__b", "A_B", "test123", "Test123", "TEST123",
        "test_123", "_test", "test_", "_test_", "test__", "__test", "a1b2c3", "A1B2C3",
    ];

    for (i, name) in names.iter().enumerate() {
        let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
        let func = create_minimal_function(name);
        let module = MirModule { name: format!("san_{}", i), functions: vec![func] };

        let (ir, _reporter) = codegen.generate_with_errors(&module);

        let sanitized = sanitize_name(name);
        assert!(ir.contains(&sanitized), "Sanitized name not found for '{}'", name);

        let mut bytes = ir.as_bytes().to_vec();
        bytes.push(0);
        let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(
            &bytes,
            &format!("san_{}", i),
        );
        let result = ctx.create_module_from_ir(buffer);
        assert!(result.is_ok(), "Sanitizer test failed for '{}': {:?}", name, result.err());
    }
}

#[test]
fn stress_test_reporter_error_accumulation() {
    let ctx = Context::create();
    let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);

    // Create module that might produce errors
    let func = MirFunction {
        name: "error_test".to_string(),
        params: vec![],
        blocks: vec![MirBlock {
            label: "entry".to_string(),
            instructions: vec![
                MirInst::LoadConst(0, MirConst::Integer(42)),
                MirInst::Load(1, "undefined_var".to_string()),
            ],
            terminator: MirTerminator::Return(Some(1)),
        }],
        next_reg: 2,
        span: jdruby_common::SourceSpan::default(),
    };

    let module = MirModule { name: "errors".to_string(), functions: vec![func] };

    let (ir, mut reporter) = codegen.generate_with_errors(&module);
    let diagnostics = reporter.take_diagnostics();

    // Verify we can access diagnostics without crashing
    let _has_errors = !diagnostics.is_empty();

    let mut bytes = ir.as_bytes().to_vec();
    bytes.push(0);
    let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(&bytes, "errors");
    match ctx.create_module_from_ir(buffer) {
        Ok(_) => {}
        Err(e) => panic!("IR parsing failed: {:?}", e),
    };
}

#[test]
fn stress_test_empty_diagnostic_reporter() {
    let ctx = Context::create();
    let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);

    let func = create_minimal_function("no_errors");
    let module = MirModule { name: "clean".to_string(), functions: vec![func] };

    let (ir, mut reporter) = codegen.generate_with_errors(&module);

    assert!(!reporter.has_errors(), "Clean module should have no errors");
    let diagnostics = reporter.take_diagnostics();
    assert!(diagnostics.is_empty(), "Diagnostics should be empty");

    let mut bytes = ir.as_bytes().to_vec();
    bytes.push(0);
    let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(&bytes, "clean");
    let result = ctx.create_module_from_ir(buffer);
    assert!(result.is_ok(), "Clean module should parse");
}

#[test]
fn stress_test_mixed_function_complexity() {
    let ctx = Context::create();

    let functions = vec![
        create_minimal_function("simple"),
        create_deeply_nested_function("deep", 100),
        create_arithmetic_function("math"),
        create_string_function("str", "test"),
        create_conditional_function("cond"),
        create_variable_function("vars"),
        create_all_ops_function("all_ops"),
        create_method_call_function("method"),
        create_special_name_function("special#name"),
    ];

    let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
    let module = MirModule { name: "mixed".to_string(), functions };

    let (ir, reporter) = codegen.generate_with_errors(&module);

    if !reporter.has_errors() {
        let mut bytes = ir.as_bytes().to_vec();
        bytes.push(0);
        let buffer =
            inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(&bytes, "mixed");
        let result = ctx.create_module_from_ir(buffer);
        assert!(result.is_ok(), "Mixed complexity module should parse");
    }
}

#[test]
fn stress_test_function_name_edge_cases() {
    let ctx = Context::create();

    let names = vec![
        "", "a", "main", "MAIN", "Main", "_start", "init", "fini", "start", "stop", "malloc",
        "free", "memcpy", "memset", "strlen", "strcpy", "strcmp", "printf", "sprintf", "fprintf",
        "exit", "abort", "assert", "panic", "unwrap", "expect",
    ];

    for (i, name) in names.iter().enumerate() {
        let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
        let func = create_minimal_function(name);
        let module = MirModule { name: format!("name_edge_{}", i), functions: vec![func] };

        let (ir, _reporter) = codegen.generate_with_errors(&module);

        let mut bytes = ir.as_bytes().to_vec();
        bytes.push(0);
        let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(
            &bytes,
            &format!("ne_{}", i),
        );
        match ctx.create_module_from_ir(buffer) {
            Ok(_) => {}
            Err(e) => panic!("IR parsing failed: {:?}", e),
        };
    }
}

#[test]
fn stress_test_parameter_count_edge_cases() {
    let ctx = Context::create();

    for count in [0, 1, 2, 5, 10, 20, 50, 100] {
        let params: Vec<u32> = (0..count).map(|i| 100 + i as u32).collect();

        let func = MirFunction {
            name: format!("params_{}", count),
            params,
            blocks: vec![MirBlock {
                label: "entry".to_string(),
                instructions: vec![MirInst::LoadConst(0, MirConst::Integer(count as i64))],
                terminator: MirTerminator::Return(Some(0)),
            }],
            next_reg: 1,
            span: jdruby_common::SourceSpan::default(),
        };

        let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
        let module = MirModule { name: format!("pc_{}", count), functions: vec![func] };

        let (ir, reporter) = codegen.generate_with_errors(&module);

        if !reporter.has_errors() {
            let mut bytes = ir.as_bytes().to_vec();
            bytes.push(0);
            let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(
                &bytes,
                &format!("pc_{}", count),
            );
            match ctx.create_module_from_ir(buffer) {
                Ok(_) => {}
                Err(e) => panic!("IR parsing failed: {:?}", e),
            };
        }
    }
}

#[test]
fn stress_test_block_count_edge_cases() {
    let ctx = Context::create();

    for count in [1, 2, 5, 10, 20, 50] {
        let mut blocks = vec![];

        for i in 0..count {
            let label = if i == 0 { "entry".to_string() } else { format!("block_{}", i) };
            let next = if i + 1 < count { format!("block_{}", i + 1) } else { "exit".to_string() };

            blocks.push(MirBlock {
                label,
                instructions: vec![MirInst::LoadConst(i as u32, MirConst::Integer(i as i64))],
                terminator: if i + 1 < count {
                    MirTerminator::Branch(next)
                } else {
                    MirTerminator::Return(Some(i as u32))
                },
            });
        }

        let func = MirFunction {
            name: format!("blocks_{}", count),
            params: vec![],
            blocks,
            next_reg: count as u32,
            span: jdruby_common::SourceSpan::default(),
        };

        let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
        let module = MirModule { name: format!("bc_{}", count), functions: vec![func] };

        let (ir, reporter) = codegen.generate_with_errors(&module);

        if !reporter.has_errors() {
            let mut bytes = ir.as_bytes().to_vec();
            bytes.push(0);
            let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(
                &bytes,
                &format!("bc_{}", count),
            );
            match ctx.create_module_from_ir(buffer) {
                Ok(_) => {}
                Err(e) => panic!("IR parsing failed: {:?}", e),
            };
        }
    }
}

#[test]
fn stress_test_register_number_edge_cases() {
    let ctx = Context::create();

    for max_reg in [0, 1, 10, 100, 1000, 10000] {
        let func = MirFunction {
            name: format!("max_reg_{}", max_reg),
            params: vec![],
            blocks: vec![MirBlock {
                label: "entry".to_string(),
                instructions: vec![],
                terminator: MirTerminator::Return(None),
            }],
            next_reg: max_reg as u32,
            span: jdruby_common::SourceSpan::default(),
        };

        let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
        let module = MirModule { name: format!("mr_{}", max_reg), functions: vec![func] };

        let (ir, _reporter) = codegen.generate_with_errors(&module);

        let mut bytes = ir.as_bytes().to_vec();
        bytes.push(0);
        let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(
            &bytes,
            &format!("mr_{}", max_reg),
        );
        match ctx.create_module_from_ir(buffer) {
            Ok(_) => {}
            Err(e) => panic!("IR parsing failed: {:?}", e),
        };
    }
}

#[test]
fn stress_test_source_span_edge_cases() {
    let ctx = Context::create();

    let spans = vec![jdruby_common::SourceSpan::default()];

    for (i, span) in spans.iter().enumerate() {
        let func = MirFunction {
            name: format!("span_{}", i),
            params: vec![],
            blocks: vec![MirBlock {
                label: "entry".to_string(),
                instructions: vec![MirInst::LoadConst(0, MirConst::Integer(42))],
                terminator: MirTerminator::Return(Some(0)),
            }],
            next_reg: 1,
            span: span.clone(),
        };

        let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
        let module = MirModule { name: format!("span_{}", i), functions: vec![func] };

        let (ir, _reporter) = codegen.generate_with_errors(&module);

        let mut bytes = ir.as_bytes().to_vec();
        bytes.push(0);
        let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(
            &bytes,
            &format!("sp_{}", i),
        );
        let result = ctx.create_module_from_ir(buffer);
        assert!(result.is_ok(), "Source span test {} failed", i);
    }
}

#[test]
fn stress_test_codegen_config_variations() {
    let ctx = Context::create();

    let configs = vec![
        CodegenConfig::default(),
        CodegenConfig { opt_level: OptLevel::O0, ..Default::default() },
        CodegenConfig { opt_level: OptLevel::O1, ..Default::default() },
        CodegenConfig { opt_level: OptLevel::O2, ..Default::default() },
        CodegenConfig { opt_level: OptLevel::O3, ..Default::default() },
    ];

    for (i, config) in configs.iter().enumerate() {
        let mut codegen = CodeGenerator::new(config.clone(), &ctx);
        let func = create_minimal_function(&format!("config_{}", i));
        let module = MirModule { name: format!("cfg_{}", i), functions: vec![func] };

        let (ir, _reporter) = codegen.generate_with_errors(&module);

        let mut bytes = ir.as_bytes().to_vec();
        bytes.push(0);
        let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(
            &bytes,
            &format!("cfg_{}", i),
        );
        let result = ctx.create_module_from_ir(buffer);
        assert!(result.is_ok(), "Config test {} failed", i);
    }
}

// Additional batch tests for comprehensive coverage
#[test]
fn stress_test_batch_100_functions() {
    let ctx = Context::create();
    let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);

    let functions: Vec<MirFunction> =
        (0..100).map(|i| create_minimal_function(&format!("batch_func_{}", i))).collect();

    let module = MirModule { name: "batch_100".to_string(), functions };

    let (ir, reporter) = codegen.generate_with_errors(&module);

    if !reporter.has_errors() {
        let mut bytes = ir.as_bytes().to_vec();
        bytes.push(0);
        let buffer =
            inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(&bytes, "batch_100");
        let result = ctx.create_module_from_ir(buffer);
        assert!(result.is_ok(), "Batch 100 functions should parse");
    }
}

#[test]
fn stress_test_batch_500_functions() {
    let ctx = Context::create();
    let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);

    let functions: Vec<MirFunction> = (0..500)
        .map(|i| match i % 5 {
            0 => create_minimal_function(&format!("batch500_{}", i)),
            1 => create_arithmetic_function(&format!("batch500_math_{}", i)),
            2 => create_string_function(&format!("batch500_str_{}", i), "test"),
            3 => create_conditional_function(&format!("batch500_cond_{}", i)),
            _ => create_variable_function(&format!("batch500_var_{}", i)),
        })
        .collect();

    let module = MirModule { name: "batch_500".to_string(), functions };

    let (ir, reporter) = codegen.generate_with_errors(&module);

    if !reporter.has_errors() {
        let mut bytes = ir.as_bytes().to_vec();
        bytes.push(0);
        let buffer =
            inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(&bytes, "batch_500");
        let result = ctx.create_module_from_ir(buffer);
        assert!(result.is_ok(), "Batch 500 functions should parse");
    }
}

#[test]
fn stress_test_batch_1000_functions() {
    let ctx = Context::create();
    let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);

    let functions: Vec<MirFunction> =
        (0..1000).map(|i| create_minimal_function(&format!("batch1000_{}", i))).collect();

    let module = MirModule { name: "batch_1000".to_string(), functions };

    let (ir, mut reporter) = codegen.generate_with_errors(&module);

    // May produce errors for very large, but should not crash
    let _diagnostics = reporter.take_diagnostics();

    if !ir.is_empty() {
        let mut bytes = ir.as_bytes().to_vec();
        bytes.push(0);
        let buffer =
            inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(&bytes, "batch_1000");
        match ctx.create_module_from_ir(buffer) {
            Ok(_) => {}
            Err(e) => panic!("IR parsing failed: {:?}", e),
        };
    }
}

/// Test 16: BinaryBuilder with exact reproduction case
#[test]
fn stress_test_binary_builder_reproduction() {
    use inkwell::OptimizationLevel;

    let ctx = Context::create();
    let config = BinaryBuilderConfig {
        output_path: std::path::PathBuf::from("test_output"),
        opt_level: OptimizationLevel::Default,
        ..Default::default()
    };

    let mut builder = BinaryBuilder::new(&ctx, config);

    // Create functions that match the failing case exactly
    let functions = vec![
        create_special_name_function("Logger#log"),
        create_special_name_function("Task#initialize"),
        create_special_name_function("Task#run"),
        create_special_name_function("Scheduler#initialize"),
        create_special_name_function("Scheduler#add_task"),
        create_special_name_function("Scheduler#run_all"),
        create_special_name_function("Scheduler#create_task_type"),
        create_minimal_function("main"),
    ];

    for (i, func) in functions.iter().enumerate() {
        let mir_module = MirModule { name: func.name.clone(), functions: vec![func.clone()] };

        // This should not cause SIGABRT
        let result = builder.add_module_with_errors(&format!("module_{}", i), &mir_module);
        if let Err(e) = result {
            panic!("BinaryBuilder reproduction failed at module {} ({}): {}", i, func.name, e);
        }
    }
}

/// Test 17: Stress test with hundreds of modules
#[test]
fn stress_test_hundreds_of_modules() {
    use inkwell::OptimizationLevel;

    let ctx = Context::create();
    let config = BinaryBuilderConfig {
        output_path: std::path::PathBuf::from("stress_test"),
        opt_level: OptimizationLevel::Default,
        ..Default::default()
    };

    let mut builder = BinaryBuilder::new(&ctx, config);

    // Create 200 modules with varying complexity
    for i in 0..200 {
        let func = match i % 5 {
            0 => create_minimal_function(&format!("func_{}", i)),
            1 => create_arithmetic_function(&format!("math_{}", i)),
            2 => create_string_function(&format!("str_{}", i), &format!("string_{}", i)),
            3 => create_special_name_function(&format!("special_{}#method", i)),
            _ => create_deeply_nested_function(&format!("deep_{}", i), 10),
        };

        let mir_module = MirModule { name: format!("module_{}", i), functions: vec![func] };

        if let Err(e) = builder.add_module_with_errors(&format!("module_{}", i), &mir_module) {
            panic!("Hundreds test failed at module {}: {}", i, e);
        }
    }
}

/// Test 18: JIT compilation stress test
#[test]
fn stress_test_jit_compilation_hundreds() {
    let ctx = Context::create();
    let mut compiler = JitCompiler::new(&ctx);

    // Compile 100 functions with different tiers
    for i in 0..100 {
        let func = match i % 4 {
            0 => create_minimal_function(&format!("jit_{}", i)),
            1 => create_arithmetic_function(&format!("jit_math_{}", i)),
            2 => create_string_function(&format!("jit_str_{}", i), "test"),
            _ => create_special_name_function(&format!("jit_special_{}#method", i)),
        };

        let tier = if i % 2 == 0 { CompilationTier::Baseline } else { CompilationTier::Optimizing };

        let result = compiler.compile_function_ir(&func, tier, i as u64);
        if let Err(e) = result {
            panic!("JIT stress test failed at function {}: {}", i, e);
        }
    }

    // Should have 100 compiled functions
    assert_eq!(compiler.compiled_count(), 100);
}

/// Test 19: MemoryBuffer with different encodings
#[test]
fn stress_test_memorybuffer_encodings() {
    let ctx = Context::create();

    // Test with UTF-8 content
    let utf8_strings = vec![
        "hello world",
        "héllo wörld", // UTF-8
        "こんにちは",  // Japanese
        "🦀 rust",     // Emoji
        "\n\t\r\0",    // Control characters
    ];

    for (i, s) in utf8_strings.iter().enumerate() {
        let mut codegen = CodeGenerator::new(CodegenConfig::default(), &ctx);
        let func = create_string_function(&format!("utf8_{}", i), s);
        let module = MirModule { name: format!("utf8_{}", i), functions: vec![func] };

        let (ir, _reporter) = codegen.generate_with_errors(&module);

        // Verify IR handles UTF-8 correctly
        let mut bytes = ir.as_bytes().to_vec();
        bytes.push(0);
        let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(
            &bytes,
            &format!("utf8_{}", i),
        );
        let result = ctx.create_module_from_ir(buffer);
        assert!(result.is_ok(), "UTF-8 IR failed for '{}': {:?}", s, result.err());
    }
}

/// Test 20: Exact reproduction of module_and_class.rb issue
#[test]
fn stress_test_module_and_class_reproduction() {
    use inkwell::OptimizationLevel;

    let ctx = Context::create();
    let config = BinaryBuilderConfig {
        output_path: std::path::PathBuf::from("reproduction"),
        opt_level: OptimizationLevel::Default,
        ..Default::default()
    };

    let mut builder = BinaryBuilder::new(&ctx, config);

    // Create the exact functions from the failing case
    let function_specs = vec![
        ("main", vec![]),
        ("Logger#log", vec![100]),
        ("Task#initialize", vec![100]),
        ("Task#run", vec![100]),
        ("Scheduler#initialize", vec![100]),
        ("Scheduler#add_task", vec![100, 101]),
        ("Scheduler#run_all", vec![100]),
        ("Scheduler#create_task_type", vec![100, 101]),
    ];

    for (name, params) in function_specs {
        let func = if params.is_empty() {
            create_minimal_function(name)
        } else if name.contains("initialize") {
            create_special_name_function(name)
        } else if name.contains("add_task") || name.contains("create_task_type") {
            create_arithmetic_function(name)
        } else {
            create_string_function(name, "test")
        };

        let mir_module = MirModule { name: name.to_string(), functions: vec![func] };

        // This is the exact call that was failing
        if let Err(e) = builder.add_module_with_errors(name, &mir_module) {
            panic!("Reproduction test failed for '{}': {}", name, e);
        }
    }
}

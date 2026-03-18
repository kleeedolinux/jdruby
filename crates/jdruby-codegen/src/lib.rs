//! # JDRuby Codegen — LLVM Code Generation
//!
//! Translates MIR to LLVM IR for native compilation.
//! Currently stubs LLVM calls until inkwell is configured.

use jdruby_common::{Diagnostic, SourceSpan};
use jdruby_mir::{MirModule, MirFunction, MirBlock, MirInst, MirTerminator, MirConst, MirBinOp, MirUnOp};

/// Code generation configuration.
#[derive(Debug, Clone)]
pub struct CodegenConfig {
    pub target_triple: String,
    pub opt_level: OptLevel,
    pub debug_info: bool,
    pub output_format: OutputFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptLevel { O0, O1, O2, O3 }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat { Object, Assembly, LlvmIr, Bitcode }

impl Default for CodegenConfig {
    fn default() -> Self {
        Self {
            target_triple: "x86_64-unknown-linux-gnu".into(),
            opt_level: OptLevel::O2,
            debug_info: false,
            output_format: OutputFormat::Object,
        }
    }
}

pub struct CodeGenerator {
    config: CodegenConfig,
    diagnostics: Vec<Diagnostic>,
    /// Generated LLVM IR text (for --emit=llvm-ir)
    ir_text: String,
}

impl CodeGenerator {
    pub fn new(config: CodegenConfig) -> Self {
        Self { config, diagnostics: Vec::new(), ir_text: String::new() }
    }

    /// Generate LLVM IR text from a MIR module.
    pub fn generate(&mut self, module: &MirModule) -> Result<String, Vec<Diagnostic>> {
        self.ir_text.clear();

        // Emit LLVM IR as textual representation
        self.emit_header(&module.name);
        self.emit_runtime_declarations();

        for func in &module.functions {
            self.emit_function(func);
        }

        if self.diagnostics.is_empty() {
            Ok(self.ir_text.clone())
        } else {
            Err(std::mem::take(&mut self.diagnostics))
        }
    }

    fn emit_header(&mut self, module_name: &str) {
        self.ir_text.push_str(&format!("; ModuleID = '{}'\n", module_name));
        self.ir_text.push_str(&format!("source_filename = \"{}\"\n", module_name));
        self.ir_text.push_str(&format!("target triple = \"{}\"\n\n", self.config.target_triple));
    }

    fn emit_runtime_declarations(&mut self) {
        // Declare runtime functions that the compiled code will call
        self.ir_text.push_str("; Runtime declarations\n");
        self.ir_text.push_str("declare i64 @rb_int_new(i64)\n");
        self.ir_text.push_str("declare double @rb_float_new(double)\n");
        self.ir_text.push_str("declare i8* @rb_str_new(i8*, i64)\n");
        self.ir_text.push_str("declare i8* @rb_sym_new(i8*)\n");
        self.ir_text.push_str("declare i64 @rb_ary_new(...)\n");
        self.ir_text.push_str("declare i64 @rb_hash_new(...)\n");
        self.ir_text.push_str("declare i64 @rb_yield(...)\n");
        self.ir_text.push_str("declare void @rb_puts(i64)\n");
        self.ir_text.push_str("declare void @rb_print(i64)\n");
        self.ir_text.push_str("declare void @rb_p(i64)\n");
        self.ir_text.push_str("declare void @rb_raise(i8*, ...)\n");
        self.ir_text.push_str("declare i64 @rb_funcall(i64, i64, i32, ...)\n");
        self.ir_text.push_str("\n");
    }

    fn emit_function(&mut self, func: &MirFunction) {
        let ret_type = "i64";
        let params: Vec<String> = func.params.iter()
            .map(|r| format!("i64 %r{}", r))
            .collect();
        self.ir_text.push_str(&format!(
            "define {} @{}({}) {{\n",
            ret_type, sanitize_name(&func.name), params.join(", ")
        ));

        for block in &func.blocks {
            self.emit_block(block);
        }

        self.ir_text.push_str("}\n\n");
    }

    fn emit_block(&mut self, block: &MirBlock) {
        self.ir_text.push_str(&format!("{}:\n", block.label));

        for inst in &block.instructions {
            self.emit_instruction(inst);
        }

        self.emit_terminator(&block.terminator);
    }

    fn emit_instruction(&mut self, inst: &MirInst) {
        match inst {
            MirInst::LoadConst(reg, c) => {
                let val = match c {
                    MirConst::Integer(v) => format!("  %r{} = add i64 {}, 0", reg, v),
                    MirConst::Float(v) => format!("  %r{} = fadd double {:.6}, 0.0", reg, v),
                    MirConst::Bool(true) => format!("  %r{} = add i64 1, 0", reg),
                    MirConst::Bool(false) => format!("  %r{} = add i64 0, 0", reg),
                    MirConst::Nil => format!("  %r{} = add i64 0, 0 ; nil", reg),
                    MirConst::String(s) => {
                        format!("  ; string literal: {:?}\n  %r{} = call i64 @rb_int_new(i64 0) ; TODO: string alloc", s, reg)
                    }
                    MirConst::Symbol(s) => {
                        format!("  ; symbol: :{}\n  %r{} = call i64 @rb_int_new(i64 0) ; TODO: symbol intern", s, reg)
                    }
                };
                self.ir_text.push_str(&val);
                self.ir_text.push('\n');
            }
            MirInst::Copy(dest, src) => {
                self.ir_text.push_str(&format!("  %r{} = add i64 %r{}, 0\n", dest, src));
            }
            MirInst::BinOp(dest, op, left, right) => {
                let op_str = match op {
                    MirBinOp::Add => "add",
                    MirBinOp::Sub => "sub",
                    MirBinOp::Mul => "mul",
                    MirBinOp::Div => "sdiv",
                    MirBinOp::Mod => "srem",
                    MirBinOp::BitAnd => "and",
                    MirBinOp::BitOr => "or",
                    MirBinOp::BitXor => "xor",
                    MirBinOp::Shl => "shl",
                    MirBinOp::Shr => "ashr",
                    _ => "add ; TODO: complex op",
                };
                self.ir_text.push_str(&format!(
                    "  %r{} = {} i64 %r{}, %r{}\n", dest, op_str, left, right
                ));
            }
            MirInst::UnOp(dest, op, src) => {
                match op {
                    MirUnOp::Neg => {
                        self.ir_text.push_str(&format!("  %r{} = sub i64 0, %r{}\n", dest, src));
                    }
                    MirUnOp::Not => {
                        self.ir_text.push_str(&format!("  %r{} = xor i64 %r{}, 1\n", dest, src));
                    }
                    MirUnOp::BitNot => {
                        self.ir_text.push_str(&format!("  %r{} = xor i64 %r{}, -1\n", dest, src));
                    }
                }
            }
            MirInst::Call(dest, name, args) => {
                let arg_list: Vec<String> = args.iter().map(|r| format!("i64 %r{}", r)).collect();
                self.ir_text.push_str(&format!(
                    "  %r{} = call i64 @{}({})\n",
                    dest, sanitize_name(name), arg_list.join(", ")
                ));
            }
            MirInst::MethodCall(dest, recv, method, args) => {
                let mut all_args = vec![format!("i64 %r{}", recv)];
                all_args.extend(args.iter().map(|r| format!("i64 %r{}", r)));
                self.ir_text.push_str(&format!(
                    "  ; method call: .{}\n  %r{} = call i64 @rb_funcall({})\n",
                    method, dest, all_args.join(", ")
                ));
            }
            MirInst::Load(reg, name) => {
                self.ir_text.push_str(&format!(
                    "  %r{} = load i64, i64* @{}, align 8\n",
                    reg, sanitize_name(name)
                ));
            }
            MirInst::Store(name, reg) => {
                self.ir_text.push_str(&format!(
                    "  store i64 %r{}, i64* @{}, align 8\n",
                    reg, sanitize_name(name)
                ));
            }
            MirInst::Alloc(reg, name) => {
                self.ir_text.push_str(&format!(
                    "  %r{} = alloca i64, align 8 ; {}\n", reg, name
                ));
            }
            MirInst::Nop => {}
        }
    }

    fn emit_terminator(&mut self, term: &MirTerminator) {
        match term {
            MirTerminator::Return(Some(reg)) => {
                self.ir_text.push_str(&format!("  ret i64 %r{}\n", reg));
            }
            MirTerminator::Return(None) => {
                self.ir_text.push_str("  ret i64 0\n");
            }
            MirTerminator::Branch(label) => {
                self.ir_text.push_str(&format!("  br label %{}\n", label));
            }
            MirTerminator::CondBranch(reg, then_l, else_l) => {
                let cmp = format!("  %cmp_{} = icmp ne i64 %r{}, 0\n", reg, reg);
                let br = format!("  br i1 %cmp_{}, label %{}, label %{}\n", reg, then_l, else_l);
                self.ir_text.push_str(&cmp);
                self.ir_text.push_str(&br);
            }
            MirTerminator::Unreachable => {
                self.ir_text.push_str("  unreachable\n");
            }
        }
    }

    /// Get the generated IR text.
    pub fn ir_text(&self) -> &str { &self.ir_text }
}

fn sanitize_name(name: &str) -> String {
    name.replace("::", "__")
        .replace('#', "__")
        .replace('<', "_")
        .replace('>', "_")
        .replace('?', "_q")
        .replace('!', "_b")
        .replace('.', "_")
}

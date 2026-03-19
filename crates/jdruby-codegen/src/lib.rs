//! # JDRuby Codegen — LLVM IR Code Generation
//!
//! Translates MIR to LLVM IR text for native compilation.
//! Generates proper string constants, symbol interning, method dispatch,
//! and global variable declarations.

use std::collections::{HashMap, HashSet};
use jdruby_common::Diagnostic;
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
    ir_text: String,
    /// String constant pool: content → constant name
    string_pool: HashMap<String, String>,
    /// Next string constant ID
    next_str_id: u32,
    /// Global variables discovered during emission
    globals: HashSet<String>,
    /// String constants section (emitted at top of module)
    string_constants: String,
    /// Global variable declarations
    global_decls: String,
}

impl CodeGenerator {
    pub fn new(config: CodegenConfig) -> Self {
        Self {
            config,
            diagnostics: Vec::new(),
            ir_text: String::new(),
            string_pool: HashMap::new(),
            next_str_id: 0,
            globals: HashSet::new(),
            string_constants: String::new(),
            global_decls: String::new(),
        }
    }

    /// Generate LLVM IR text from a MIR module.
    pub fn generate(&mut self, module: &MirModule) -> Result<String, Vec<Diagnostic>> {
        self.ir_text.clear();
        self.string_pool.clear();
        self.next_str_id = 0;
        self.globals.clear();
        self.string_constants.clear();
        self.global_decls.clear();

        // Pre-scan all functions to collect string constants and globals
        for func in &module.functions {
            self.prescan_function(func);
        }

        // Build the final IR
        let mut output = String::with_capacity(8192);

        // Header
        output.push_str(&format!("; ModuleID = '{}'\n", module.name));
        output.push_str(&format!("source_filename = \"{}\"\n", module.name));
        output.push_str(&format!("target triple = \"{}\"\n\n", self.config.target_triple));

        // String constants
        if !self.string_constants.is_empty() {
            output.push_str("; ── String Constants ──\n");
            output.push_str(&self.string_constants);
            output.push('\n');
        }

        // Global variable declarations
        if !self.global_decls.is_empty() {
            output.push_str("; ── Global Variables ──\n");
            output.push_str(&self.global_decls);
            output.push('\n');
        }

        // Runtime declarations
        self.emit_runtime_declarations(&mut output);

        // Functions
        for func in &module.functions {
            self.emit_function(func, &mut output);
        }

        if self.diagnostics.is_empty() {
            Ok(output)
        } else {
            Err(std::mem::take(&mut self.diagnostics))
        }
    }

    /// Pre-scan a function to collect all string constants and global variables.
    fn prescan_function(&mut self, func: &MirFunction) {
        for block in &func.blocks {
            for inst in &block.instructions {
                match inst {
                    MirInst::LoadConst(_, MirConst::String(s)) => {
                        self.intern_string(s);
                    }
                    MirInst::LoadConst(_, MirConst::Symbol(s)) => {
                        self.intern_string(s);
                    }
                    MirInst::MethodCall(_, _, method, _) => {
                        self.intern_string(method);
                    }
                    MirInst::Load(_, name) | MirInst::Store(name, _) => {
                        let sname = sanitize_name(name);
                        if self.globals.insert(sname.clone()) {
                            self.global_decls.push_str(&format!(
                                "@{} = internal global i64 0, align 8\n", sname
                            ));
                        }
                    }
                    MirInst::ClassNew(_, name, superclass) => {
                        self.intern_string(name);
                        if let Some(sc) = superclass {
                            self.intern_string(sc);
                        }
                    }
                    MirInst::DefMethod(_, method_name, func_name) => {
                        self.intern_string(method_name);
                        self.intern_string(func_name);
                    }
                    MirInst::IncludeModule(_, module_name) => {
                        self.intern_string(module_name);
                        self.intern_string("include");
                    }
                    _ => {}
                }
            }
        }
    }

    /// Intern a string constant and return its LLVM global name.
    fn intern_string(&mut self, s: &str) -> String {
        if let Some(existing) = self.string_pool.get(s) {
            return existing.clone();
        }
        let id = self.next_str_id;
        self.next_str_id += 1;
        let name = format!(".str.{}", id);

        // Escape the string for LLVM IR
        let escaped = llvm_escape_string(s);
        let byte_len = s.len() + 1; // +1 for null terminator

        self.string_constants.push_str(&format!(
            "@{} = private unnamed_addr constant [{} x i8] c\"{}\\00\", align 1\n",
            name, byte_len, escaped
        ));
        self.string_pool.insert(s.to_string(), name.clone());
        name
    }

    fn emit_runtime_declarations(&self, out: &mut String) {
        out.push_str("; ── Runtime Declarations ──\n");
        out.push_str(";\n");
        out.push_str("; Value representation: all Ruby values are i64 (tagged pointers).\n");
        out.push_str("; Integers use tagged fixnum encoding (value << 1 | 1).\n");
        out.push_str("; Objects are heap pointers (always even, tag bit = 0).\n");
        out.push_str(";\n\n");

        // Core value constructors
        out.push_str("; Value constructors\n");
        out.push_str("declare i64 @jdruby_int_new(i64)              ; create tagged integer\n");
        out.push_str("declare i64 @jdruby_float_new(double)          ; box a float\n");
        out.push_str("declare i64 @jdruby_str_new(i8*, i64)          ; create string from ptr+len\n");
        out.push_str("declare i64 @jdruby_sym_intern(i8*)            ; intern a symbol\n");
        out.push_str("declare i64 @jdruby_ary_new(i32, ...)          ; create array (argc, elems...)\n");
        out.push_str("declare i64 @jdruby_hash_new(i32, ...)         ; create hash (npairs, k, v...)\n");
        out.push_str("declare i64 @jdruby_bool(i1)                   ; box boolean\n");
        out.push_str("\n");

        // Constants
        out.push_str("; Well-known constants\n");
        out.push_str("@JDRUBY_NIL   = external global i64              ; nil value\n");
        out.push_str("@JDRUBY_TRUE  = external global i64              ; true value\n");
        out.push_str("@JDRUBY_FALSE = external global i64              ; false value\n");
        out.push_str("\n");

        // Method dispatch
        out.push_str("; Method dispatch\n");
        out.push_str("declare i64 @jdruby_send(i64, i8*, i32, ...)   ; receiver, method_name, argc, args...\n");
        out.push_str("declare i64 @jdruby_call(i8*, i32, ...)        ; func_name, argc, args...\n");
        out.push_str("declare i64 @jdruby_yield(i32, ...)            ; argc, args...\n");
        out.push_str("declare i64 @jdruby_block_given()              ; check if block given\n");
        out.push_str("\n");

        // I/O builtins
        out.push_str("; I/O builtins\n");
        out.push_str("declare void @jdruby_puts(i64)                 ; puts(value)\n");
        out.push_str("declare void @jdruby_print(i64)                ; print(value)\n");
        out.push_str("declare i64  @jdruby_p(i64)                    ; p(value) → value\n");
        out.push_str("declare void @jdruby_raise(i8*, ...)           ; raise exception\n");
        out.push_str("\n");

        // Arithmetic intrinsics (for optimized integer paths)
        out.push_str("; Arithmetic intrinsics (fast path for tagged integers)\n");
        out.push_str("declare i64 @jdruby_int_add(i64, i64)\n");
        out.push_str("declare i64 @jdruby_int_sub(i64, i64)\n");
        out.push_str("declare i64 @jdruby_int_mul(i64, i64)\n");
        out.push_str("declare i64 @jdruby_int_div(i64, i64)\n");
        out.push_str("declare i64 @jdruby_int_mod(i64, i64)\n");
        out.push_str("declare i64 @jdruby_int_pow(i64, i64)\n");
        out.push_str("\n");

        // Comparison intrinsics
        out.push_str("; Comparison\n");
        out.push_str("declare i1  @jdruby_eq(i64, i64)\n");
        out.push_str("declare i1  @jdruby_lt(i64, i64)\n");
        out.push_str("declare i1  @jdruby_gt(i64, i64)\n");
        out.push_str("declare i1  @jdruby_le(i64, i64)\n");
        out.push_str("declare i1  @jdruby_ge(i64, i64)\n");
        out.push_str("declare i1  @jdruby_truthy(i64)                ; test Ruby truthiness\n");
        out.push_str("\n");

        // Class/module
        out.push_str("; Class/module support\n");
        out.push_str("declare i64 @jdruby_class_new(i8*, i64)       ; name, superclass\n");
        out.push_str("declare void @jdruby_def_method(i64, i8*, i8*) ; class, name, func_ptr\n");
        out.push_str("declare i64 @jdruby_const_get(i8*)             ; get constant by name\n");
        out.push_str("declare void @jdruby_const_set(i8*, i64)       ; set constant\n");
        out.push_str("\n");
    }

    fn emit_function(&self, func: &MirFunction, out: &mut String) {
        let params: Vec<String> = func.params.iter()
            .map(|r| format!("i64 %r{}", r))
            .collect();

        out.push_str(&format!(
            "define i64 @{}({}) {{\n",
            sanitize_name(&func.name), params.join(", ")
        ));

        for (i, block) in func.blocks.iter().enumerate() {
            self.emit_block(block, out, i == 0);
        }

        out.push_str("}\n\n");
    }

    fn emit_block(&self, block: &MirBlock, out: &mut String, _is_entry: bool) {
        out.push_str(&format!("{}:\n", block.label));
        for inst in &block.instructions {
            self.emit_instruction(inst, out);
        }
        self.emit_terminator(&block.terminator, out);
    }

    fn emit_instruction(&self, inst: &MirInst, out: &mut String) {
        match inst {
            MirInst::LoadConst(reg, c) => {
                match c {
                    MirConst::Integer(v) => {
                        out.push_str(&format!(
                            "  %r{} = call i64 @jdruby_int_new(i64 {})\n", reg, v
                        ));
                    }
                    MirConst::Float(v) => {
                        out.push_str(&format!(
                            "  %r{} = call i64 @jdruby_float_new(double {:.15e})\n", reg, v
                        ));
                    }
                    MirConst::Bool(true) => {
                        out.push_str(&format!(
                            "  %r{} = load i64, i64* @JDRUBY_TRUE, align 8\n", reg
                        ));
                    }
                    MirConst::Bool(false) => {
                        out.push_str(&format!(
                            "  %r{} = load i64, i64* @JDRUBY_FALSE, align 8\n", reg
                        ));
                    }
                    MirConst::Nil => {
                        out.push_str(&format!(
                            "  %r{} = load i64, i64* @JDRUBY_NIL, align 8\n", reg
                        ));
                    }
                    MirConst::String(s) => {
                        let const_name = self.string_pool.get(s.as_str()).unwrap();
                        let byte_len = s.len();
                        out.push_str(&format!(
                            "  %str_ptr_{reg} = getelementptr inbounds [{len} x i8], [{len} x i8]* @{name}, i64 0, i64 0\n",
                            reg = reg, len = byte_len + 1, name = const_name
                        ));
                        out.push_str(&format!(
                            "  %r{} = call i64 @jdruby_str_new(i8* %str_ptr_{}, i64 {})\n",
                            reg, reg, byte_len
                        ));
                    }
                    MirConst::Symbol(s) => {
                        let const_name = self.string_pool.get(s.as_str()).unwrap();
                        let byte_len = s.len();
                        out.push_str(&format!(
                            "  %sym_ptr_{reg} = getelementptr inbounds [{len} x i8], [{len} x i8]* @{name}, i64 0, i64 0\n",
                            reg = reg, len = byte_len + 1, name = const_name
                        ));
                        out.push_str(&format!(
                            "  %r{} = call i64 @jdruby_sym_intern(i8* %sym_ptr_{})\n",
                            reg, reg
                        ));
                    }
                }
            }
            MirInst::Copy(dest, src) => {
                out.push_str(&format!("  %r{} = add i64 %r{}, 0\n", dest, src));
            }
            MirInst::BinOp(dest, op, left, right) => {
                match op {
                    MirBinOp::Add => {
                        out.push_str(&format!(
                            "  %r{} = call i64 @jdruby_int_add(i64 %r{}, i64 %r{})\n",
                            dest, left, right
                        ));
                    }
                    MirBinOp::Sub => {
                        out.push_str(&format!(
                            "  %r{} = call i64 @jdruby_int_sub(i64 %r{}, i64 %r{})\n",
                            dest, left, right
                        ));
                    }
                    MirBinOp::Mul => {
                        out.push_str(&format!(
                            "  %r{} = call i64 @jdruby_int_mul(i64 %r{}, i64 %r{})\n",
                            dest, left, right
                        ));
                    }
                    MirBinOp::Div => {
                        out.push_str(&format!(
                            "  %r{} = call i64 @jdruby_int_div(i64 %r{}, i64 %r{})\n",
                            dest, left, right
                        ));
                    }
                    MirBinOp::Mod => {
                        out.push_str(&format!(
                            "  %r{} = call i64 @jdruby_int_mod(i64 %r{}, i64 %r{})\n",
                            dest, left, right
                        ));
                    }
                    MirBinOp::Pow => {
                        out.push_str(&format!(
                            "  %r{} = call i64 @jdruby_int_pow(i64 %r{}, i64 %r{})\n",
                            dest, left, right
                        ));
                    }
                    MirBinOp::Eq => {
                        out.push_str(&format!(
                            "  %eq_{} = call i1 @jdruby_eq(i64 %r{}, i64 %r{})\n", dest, left, right
                        ));
                        out.push_str(&format!(
                            "  %r{} = call i64 @jdruby_bool(i1 %eq_{})\n", dest, dest
                        ));
                    }
                    MirBinOp::NotEq => {
                        out.push_str(&format!(
                            "  %neq_{} = call i1 @jdruby_eq(i64 %r{}, i64 %r{})\n", dest, left, right
                        ));
                        out.push_str(&format!(
                            "  %neq_inv_{} = xor i1 %neq_{}, true\n", dest, dest
                        ));
                        out.push_str(&format!(
                            "  %r{} = call i64 @jdruby_bool(i1 %neq_inv_{})\n", dest, dest
                        ));
                    }
                    MirBinOp::Lt => {
                        out.push_str(&format!(
                            "  %lt_{} = call i1 @jdruby_lt(i64 %r{}, i64 %r{})\n", dest, left, right
                        ));
                        out.push_str(&format!(
                            "  %r{} = call i64 @jdruby_bool(i1 %lt_{})\n", dest, dest
                        ));
                    }
                    MirBinOp::Gt => {
                        out.push_str(&format!(
                            "  %gt_{} = call i1 @jdruby_gt(i64 %r{}, i64 %r{})\n", dest, left, right
                        ));
                        out.push_str(&format!(
                            "  %r{} = call i64 @jdruby_bool(i1 %gt_{})\n", dest, dest
                        ));
                    }
                    MirBinOp::LtEq => {
                        out.push_str(&format!(
                            "  %le_{} = call i1 @jdruby_le(i64 %r{}, i64 %r{})\n", dest, left, right
                        ));
                        out.push_str(&format!(
                            "  %r{} = call i64 @jdruby_bool(i1 %le_{})\n", dest, dest
                        ));
                    }
                    MirBinOp::GtEq => {
                        out.push_str(&format!(
                            "  %ge_{} = call i1 @jdruby_ge(i64 %r{}, i64 %r{})\n", dest, left, right
                        ));
                        out.push_str(&format!(
                            "  %r{} = call i64 @jdruby_bool(i1 %ge_{})\n", dest, dest
                        ));
                    }
                    MirBinOp::And => {
                        // Short-circuit: if left is truthy, result = right; else result = left
                        out.push_str(&format!(
                            "  %and_test_{} = call i1 @jdruby_truthy(i64 %r{})\n", dest, left
                        ));
                        out.push_str(&format!(
                            "  %r{} = select i1 %and_test_{}, i64 %r{}, i64 %r{}\n",
                            dest, dest, right, left
                        ));
                    }
                    MirBinOp::Or => {
                        // Short-circuit: if left is truthy, result = left; else result = right
                        out.push_str(&format!(
                            "  %or_test_{} = call i1 @jdruby_truthy(i64 %r{})\n", dest, left
                        ));
                        out.push_str(&format!(
                            "  %r{} = select i1 %or_test_{}, i64 %r{}, i64 %r{}\n",
                            dest, dest, left, right
                        ));
                    }
                    MirBinOp::BitAnd => {
                        out.push_str(&format!("  %r{} = and i64 %r{}, %r{}\n", dest, left, right));
                    }
                    MirBinOp::BitOr => {
                        out.push_str(&format!("  %r{} = or i64 %r{}, %r{}\n", dest, left, right));
                    }
                    MirBinOp::BitXor => {
                        out.push_str(&format!("  %r{} = xor i64 %r{}, %r{}\n", dest, left, right));
                    }
                    MirBinOp::Shl => {
                        out.push_str(&format!("  %r{} = shl i64 %r{}, %r{}\n", dest, left, right));
                    }
                    MirBinOp::Shr => {
                        out.push_str(&format!("  %r{} = ashr i64 %r{}, %r{}\n", dest, left, right));
                    }
                    MirBinOp::Cmp => {
                        // Spaceship operator: returns -1, 0, or 1
                        out.push_str(&format!(
                            "  %cmp_lt_{d} = call i1 @jdruby_lt(i64 %r{l}, i64 %r{r})\n",
                            d = dest, l = left, r = right
                        ));
                        out.push_str(&format!(
                            "  %cmp_gt_{d} = call i1 @jdruby_gt(i64 %r{l}, i64 %r{r})\n",
                            d = dest, l = left, r = right
                        ));
                        out.push_str(&format!(
                            "  %cmp_sel1_{d} = select i1 %cmp_lt_{d}, i64 -1, i64 0\n",
                            d = dest
                        ));
                        out.push_str(&format!(
                            "  %r{d} = select i1 %cmp_gt_{d}, i64 1, i64 %cmp_sel1_{d}\n",
                            d = dest
                        ));
                    }
                }
            }
            MirInst::UnOp(dest, op, src) => {
                match op {
                    MirUnOp::Neg => {
                        out.push_str(&format!("  %r{} = sub i64 0, %r{}\n", dest, src));
                    }
                    MirUnOp::Not => {
                        out.push_str(&format!(
                            "  %not_{d} = call i1 @jdruby_truthy(i64 %r{s})\n",
                            d = dest, s = src
                        ));
                        out.push_str(&format!(
                            "  %not_inv_{d} = xor i1 %not_{d}, true\n", d = dest
                        ));
                        out.push_str(&format!(
                            "  %r{d} = call i64 @jdruby_bool(i1 %not_inv_{d})\n", d = dest
                        ));
                    }
                    MirUnOp::BitNot => {
                        out.push_str(&format!("  %r{} = xor i64 %r{}, -1\n", dest, src));
                    }
                }
            }
            MirInst::Call(dest, name, args) => {
                // Builtins with known signatures
                match name.as_str() {
                    "puts" => {
                        for &arg_reg in args {
                            out.push_str(&format!("  call void @jdruby_puts(i64 %r{})\n", arg_reg));
                        }
                        out.push_str(&format!("  %r{} = load i64, i64* @JDRUBY_NIL, align 8\n", dest));
                    }
                    "print" => {
                        for &arg_reg in args {
                            out.push_str(&format!("  call void @jdruby_print(i64 %r{})\n", arg_reg));
                        }
                        out.push_str(&format!("  %r{} = load i64, i64* @JDRUBY_NIL, align 8\n", dest));
                    }
                    "p" => {
                        if let Some(&first) = args.first() {
                            out.push_str(&format!("  %r{} = call i64 @jdruby_p(i64 %r{})\n", dest, first));
                        } else {
                            out.push_str(&format!("  %r{} = load i64, i64* @JDRUBY_NIL, align 8\n", dest));
                        }
                    }
                    _ => {
                        // User-defined function call
                        let arg_list: Vec<String> = args.iter()
                            .map(|r| format!("i64 %r{}", r))
                            .collect();
                        let sname = sanitize_name(name);

                        // Try direct call first (for known functions)
                        out.push_str(&format!(
                            "  %r{} = call i64 @{}({})\n",
                            dest, sname, arg_list.join(", ")
                        ));
                    }
                }
            }
            MirInst::MethodCall(dest, recv, method, args) => {
                // Use jdruby_send for dynamic dispatch
                let method_str = self.string_pool.get(method.as_str());

                if let Some(mname) = method_str {
                    let mlen = method.len() + 1;
                    out.push_str(&format!(
                        "  %meth_ptr_{d} = getelementptr inbounds [{l} x i8], [{l} x i8]* @{n}, i64 0, i64 0\n",
                        d = dest, l = mlen, n = mname
                    ));
                } else {
                    // Inline the method name string
                    let _escaped = llvm_escape_string(method);
                    let _mlen = method.len() + 1;
                    out.push_str(&format!(
                        "  ; method: {}\n", method
                    ));
                    // Fall back to direct call with mangled name
                    let mut all_args: Vec<String> = vec![format!("i64 %r{}", recv)];
                    all_args.extend(args.iter().map(|r| format!("i64 %r{}", r)));
                    let mangled = sanitize_name(method);
                    out.push_str(&format!(
                        "  %r{} = call i64 @jdruby_send_{}({})\n",
                        dest, mangled, all_args.join(", ")
                    ));
                    return;
                }

                // Dynamic send: jdruby_send(receiver, method_name, argc, args...)
                let argc = args.len() as i32;
                let mut arg_str = format!("i64 %r{}, i8* %meth_ptr_{}, i32 {}", recv, dest, argc);
                for arg_reg in args {
                    arg_str.push_str(&format!(", i64 %r{}", arg_reg));
                }
                out.push_str(&format!(
                    "  %r{} = call i64 (i64, i8*, i32, ...) @jdruby_send({})\n",
                    dest, arg_str
                ));
            }
            MirInst::Load(reg, name) => {
                out.push_str(&format!(
                    "  %r{} = load i64, i64* @{}, align 8\n",
                    reg, sanitize_name(name)
                ));
            }
            MirInst::Store(name, reg) => {
                out.push_str(&format!(
                    "  store i64 %r{}, i64* @{}, align 8\n",
                    reg, sanitize_name(name)
                ));
            }
            MirInst::Alloc(reg, name) => {
                out.push_str(&format!(
                    "  %alloca_{} = alloca i64, align 8 ; {}\n", reg, name
                ));
                out.push_str(&format!(
                    "  %r{} = ptrtoint i64* %alloca_{} to i64\n", reg, reg
                ));
            }
            MirInst::ClassNew(dest, name, superclass) => {
                let name_const = self.string_pool.get(name.as_str()).unwrap();
                let name_len = name.len() + 1;
                out.push_str(&format!(
                    "  %cls_name_{d} = getelementptr inbounds [{l} x i8], [{l} x i8]* @{n}, i64 0, i64 0\n",
                    d = dest, l = name_len, n = name_const
                ));
                let super_val = if let Some(sc) = superclass {
                    let sc_const = self.string_pool.get(sc.as_str()).unwrap();
                    let sc_len = sc.len() + 1;
                    out.push_str(&format!(
                        "  %cls_super_{d} = getelementptr inbounds [{l} x i8], [{l} x i8]* @{n}, i64 0, i64 0\n",
                        d = dest, l = sc_len, n = sc_const
                    ));
                    out.push_str(&format!(
                        "  %cls_super_val_{d} = call i64 @jdruby_const_get(i8* %cls_super_{d})\n",
                        d = dest
                    ));
                    format!("%cls_super_val_{}", dest)
                } else {
                    let nil_reg = format!("cls_nil_{}", dest);
                    out.push_str(&format!(
                        "  %{} = load i64, i64* @JDRUBY_NIL, align 8\n", nil_reg
                    ));
                    format!("%{}", nil_reg)
                };
                out.push_str(&format!(
                    "  %r{} = call i64 @jdruby_class_new(i8* %cls_name_{}, i64 {})\n",
                    dest, dest, super_val
                ));
            }
            MirInst::DefMethod(class_reg, method_name, func_name) => {
                let meth_const = self.string_pool.get(method_name.as_str()).unwrap();
                let meth_len = method_name.len() + 1;
                let func_const = self.string_pool.get(func_name.as_str()).unwrap();
                let func_len = func_name.len() + 1;
                // Use a unique suffix based on method+func names to avoid LLVM SSA conflicts
                let uid = format!("{}_{}", sanitize_name(method_name), sanitize_name(func_name));
                out.push_str(&format!(
                    "  %def_meth_{u} = getelementptr inbounds [{l} x i8], [{l} x i8]* @{n}, i64 0, i64 0\n",
                    u = uid, l = meth_len, n = meth_const
                ));
                out.push_str(&format!(
                    "  %def_func_{u} = getelementptr inbounds [{l} x i8], [{l} x i8]* @{n}, i64 0, i64 0\n",
                    u = uid, l = func_len, n = func_const
                ));
                out.push_str(&format!(
                    "  call void @jdruby_def_method(i64 %r{}, i8* %def_meth_{}, i8* %def_func_{})\n",
                    class_reg, uid, uid
                ));
            }
            MirInst::IncludeModule(class_reg, module_name) => {
                let mod_const = self.string_pool.get(module_name.as_str()).unwrap();
                let mod_len = module_name.len() + 1;
                let uid = sanitize_name(module_name);
                out.push_str(&format!(
                    "  %inc_mod_{u} = getelementptr inbounds [{l} x i8], [{l} x i8]* @{n}, i64 0, i64 0\n",
                    u = uid, l = mod_len, n = mod_const
                ));
                out.push_str(&format!(
                    "  %inc_mod_val_{u} = call i64 @jdruby_const_get(i8* %inc_mod_{u})\n",
                    u = uid
                ));
                // Use jdruby_send to call include
                let incl_str = self.string_pool.get("include").unwrap().clone();
                let incl_len = "include".len() + 1;
                out.push_str(&format!(
                    "  %inc_name_{u} = getelementptr inbounds [{l} x i8], [{l} x i8]* @{n}, i64 0, i64 0\n",
                    u = uid, l = incl_len, n = incl_str
                ));
                out.push_str(&format!(
                    "  call i64 (i64, i8*, i32, ...) @jdruby_send(i64 %r{}, i8* %inc_name_{}, i32 1, i64 %inc_mod_val_{})\n",
                    class_reg, uid, uid
                ));
            }
            MirInst::Nop => {}
        }
    }

    fn emit_terminator(&self, term: &MirTerminator, out: &mut String) {
        match term {
            MirTerminator::Return(Some(reg)) => {
                out.push_str(&format!("  ret i64 %r{}\n", reg));
            }
            MirTerminator::Return(None) => {
                out.push_str("  %ret_nil = load i64, i64* @JDRUBY_NIL, align 8\n");
                out.push_str("  ret i64 %ret_nil\n");
            }
            MirTerminator::Branch(label) => {
                out.push_str(&format!("  br label %{}\n", label));
            }
            MirTerminator::CondBranch(reg, then_l, else_l) => {
                out.push_str(&format!(
                    "  %br_cond_{} = call i1 @jdruby_truthy(i64 %r{})\n", reg, reg
                ));
                out.push_str(&format!(
                    "  br i1 %br_cond_{}, label %{}, label %{}\n", reg, then_l, else_l
                ));
            }
            MirTerminator::Unreachable => {
                out.push_str("  unreachable\n");
            }
        }
    }

    /// Get the generated IR text.
    pub fn ir_text(&self) -> &str { &self.ir_text }
}

/// Sanitize a Ruby identifier for LLVM IR.
fn sanitize_name(name: &str) -> String {
    name.replace("::", "__")
        .replace('#', "__")
        .replace('<', "_")
        .replace('>', "_")
        .replace('?', "_q")
        .replace('!', "_b")
        .replace('.', "_")
        .replace('@', "_at_")
        .replace(' ', "_")
}

/// Escape a string for LLVM IR constant representation.
fn llvm_escape_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for byte in s.bytes() {
        match byte {
            b'\\' => out.push_str("\\5C"),
            b'"' => out.push_str("\\22"),
            b'\n' => out.push_str("\\0A"),
            b'\r' => out.push_str("\\0D"),
            b'\t' => out.push_str("\\09"),
            b'\0' => out.push_str("\\00"),
            0x20..=0x7E => out.push(byte as char),
            _ => out.push_str(&format!("\\{:02X}", byte)),
        }
    }
    out
}

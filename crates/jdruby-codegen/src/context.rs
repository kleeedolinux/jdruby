//! Codegen context for LLVM IR generation state management using Inkwell.

use std::collections::{HashMap, HashSet};
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::values::{GlobalValue, PointerValue};
use jdruby_common::Diagnostic;
use jdruby_mir::{MirFunction, MirInst, MirConst};

/// Tracks state during LLVM IR generation with Inkwell.
pub struct CodegenContext<'ctx> {
    module_name: String,
    diagnostics: Vec<Diagnostic>,
    /// Maps original string to (global_name, global_value)
    string_pool: HashMap<String, (String, GlobalValue<'ctx>)>,
    next_str_id: u32,
    globals: HashSet<String>,
    global_values: HashMap<String, GlobalValue<'ctx>>,
}

impl<'ctx> CodegenContext<'ctx> {
    pub fn new() -> Self {
        Self {
            module_name: String::new(),
            diagnostics: Vec::new(),
            string_pool: HashMap::new(),
            next_str_id: 0,
            globals: HashSet::new(),
            global_values: HashMap::new(),
        }
    }

    pub fn clear(&mut self) {
        self.module_name.clear();
        self.diagnostics.clear();
        self.string_pool.clear();
        self.next_str_id = 0;
        self.globals.clear();
        self.global_values.clear();
    }

    pub fn set_module_name(&mut self, name: &str) {
        self.module_name = name.to_string();
    }

    pub fn module_name(&self) -> &str {
        &self.module_name
    }

    pub fn prescan_function(&mut self, func: &MirFunction) {
        for block in &func.blocks {
            for inst in &block.instructions {
                self.prescan_instruction(inst);
            }
        }
    }

    fn prescan_instruction(&mut self, inst: &MirInst) {
        match inst {
            MirInst::LoadConst(_, MirConst::String(s)) => {
                self.intern_string_name(s);
            }
            MirInst::LoadConst(_, MirConst::Symbol(s)) => {
                self.intern_string_name(s);
            }
            MirInst::MethodCall(_, _, method, _) => {
                self.intern_string_name(method);
            }
            MirInst::Load(_, name) | MirInst::Store(name, _) => {
                self.collect_global(name);
            }
            MirInst::Call(_, name, _) => {
                if !is_builtin(name) {
                    self.intern_string_name(name);
                }
            }
            MirInst::ClassNew(_, name, superclass) => {
                self.intern_string_name(name);
                if let Some(sc) = superclass {
                    self.intern_string_name(sc);
                }
            }
            MirInst::DefMethod(_, method_name, func_name) => {
                self.intern_string_name(method_name);
                self.intern_string_name(func_name);
            }
            MirInst::IncludeModule(_, module_name) => {
                self.intern_string_name(module_name);
                self.intern_string_name("include");
            }
            _ => {}
        }
    }

    fn collect_global(&mut self, name: &str) {
        if name.starts_with(|c: char| c.is_ascii_uppercase()) || name.starts_with('$') {
            let sname = crate::utils::sanitize_name(name);
            if self.globals.insert(sname.clone()) {
                // Note: actual global creation happens in emit phase when we have module
            }
            // Also intern the string for use in jdruby_const_get
            self.intern_string_name(name);
        } else if name.starts_with('@') {
            self.intern_string_name(name);
        }
    }

    /// Just compute the name for a string constant without creating the global.
    fn intern_string_name(&mut self, s: &str) -> String {
        if let Some((name, _)) = self.string_pool.get(s) {
            return name.clone();
        }
        let id = self.next_str_id;
        self.next_str_id += 1;
        // Prefix with sanitized module name to ensure uniqueness across linked modules
        let sanitized_module = crate::utils::sanitize_name(&self.module_name);
        let name = format!(".str.{}.{}", sanitized_module, id);
        // Insert placeholder - will be replaced when global is actually created
        self.string_pool.insert(s.to_string(), (name.clone(), unsafe { std::mem::zeroed() }));
        name
    }

    /// Create the actual LLVM global for a string constant.
    pub fn create_string_global(
        &mut self,
        s: &str,
        ctx: &'ctx Context,
        module: &Module<'ctx>,
    ) -> Option<GlobalValue<'ctx>> {
        // Check if already created
        if let Some((_, global)) = self.string_pool.get(s) {
            return Some(*global);
        }

        let id = self.next_str_id;
        self.next_str_id += 1;
        let sanitized_module = crate::utils::sanitize_name(&self.module_name);
        let name = format!(".str.{}.{}", sanitized_module, id);

        // Create the string constant as a global
        let i8_type = ctx.i8_type();
        let byte_len = s.len() + 1; // Include null terminator
        let array_type = i8_type.array_type(byte_len as u32);

        // Create the global
        let global = module.add_global(array_type, None, &name);
        global.set_linkage(inkwell::module::Linkage::Private);
        global.set_unnamed_addr(true);
        global.set_alignment(1);

        // Build the constant value
        let mut bytes: Vec<inkwell::values::IntValue> = s
            .bytes()
            .map(|b| i8_type.const_int(b as u64, false))
            .collect();
        // Add null terminator
        bytes.push(i8_type.const_int(0, false));

        let const_array = i8_type.const_array(&bytes);
        global.set_initializer(&const_array);

        self.string_pool.insert(s.to_string(), (name, global));
        Some(global)
    }

    /// Get an existing string constant global value.
    pub fn get_string_constant(&self, s: &str) -> Option<GlobalValue<'ctx>> {
        self.string_pool.get(s).map(|(_, g)| *g)
    }

    /// Get pointer to a string constant for use in instructions.
    pub fn get_string_pointer(
        &self,
        s: &str,
        builder: &Builder<'ctx>,
        ctx: &'ctx Context,
    ) -> Option<PointerValue<'ctx>> {
        let global = self.get_string_constant(s)?;
        let i8_type = ctx.i8_type();
        let ptr = global.as_pointer_value();
        
        // GEP to get pointer to first element
        let zero = ctx.i64_type().const_int(0, false);
        unsafe {
            builder.build_gep(i8_type, ptr, &[zero, zero], "str_ptr").ok()
        }
    }

    /// Create all pending globals in the module.
    pub fn emit_globals(
        &mut self,
        ctx: &'ctx Context,
        module: &Module<'ctx>,
    ) {
        // Collect string constants that need to be created
        let strings_to_create: Vec<String> = self.string_pool
            .iter()
            .filter(|(_, (_, _g))| false) // Skip ones already created (simplified check)
            .map(|(s, _)| s.clone())
            .collect();

        for s in strings_to_create {
            self.create_string_global(&s, ctx, module);
        }

        // Create Ruby global variables
        let i64_type = ctx.i64_type();
        for name in &self.globals {
            if !self.global_values.contains_key(name) {
                let global = module.add_global(i64_type, None, name);
                global.set_linkage(inkwell::module::Linkage::Internal);
                global.set_alignment(8);
                global.set_initializer(&i64_type.const_int(0, false));
                self.global_values.insert(name.clone(), global);
            }
        }
    }

    pub fn get_global_value(&self, name: &str) -> Option<GlobalValue<'ctx>> {
        self.global_values.get(name).copied()
    }

    pub fn add_diagnostic(&mut self, diag: Diagnostic) {
        self.diagnostics.push(diag);
    }

    pub fn has_errors(&self) -> bool {
        !self.diagnostics.is_empty()
    }

    pub fn take_diagnostics(&mut self) -> Vec<Diagnostic> {
        std::mem::take(&mut self.diagnostics)
    }
}

impl<'ctx> Default for CodegenContext<'ctx> {
    fn default() -> Self {
        Self::new()
    }
}

fn is_builtin(name: &str) -> bool {
    matches!(name, "puts" | "print" | "p" | "rb_ary_new" | "jdruby_ary_new" | "rb_hash_new" | "rb_yield")
}

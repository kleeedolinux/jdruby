//! Codegen context for LLVM IR generation state management.

use std::collections::{HashMap, HashSet};
use jdruby_common::Diagnostic;
use jdruby_mir::{MirFunction, MirInst, MirConst};

/// Tracks state during LLVM IR generation.
pub struct CodegenContext {
    module_name: String,
    diagnostics: Vec<Diagnostic>,
    string_pool: HashMap<String, String>,
    next_str_id: u32,
    globals: HashSet<String>,
    string_constants: String,
    global_decls: String,
}

impl CodegenContext {
    pub fn new() -> Self {
        Self {
            module_name: String::new(),
            diagnostics: Vec::new(),
            string_pool: HashMap::new(),
            next_str_id: 0,
            globals: HashSet::new(),
            string_constants: String::new(),
            global_decls: String::new(),
        }
    }

    pub fn clear(&mut self) {
        self.module_name.clear();
        self.diagnostics.clear();
        self.string_pool.clear();
        self.next_str_id = 0;
        self.globals.clear();
        self.string_constants.clear();
        self.global_decls.clear();
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
                self.intern_string(s);
            }
            MirInst::LoadConst(_, MirConst::Symbol(s)) => {
                self.intern_string(s);
            }
            MirInst::MethodCall(_, _, method, _) => {
                self.intern_string(method);
            }
            MirInst::Load(_, name) | MirInst::Store(name, _) => {
                self.collect_global(name);
            }
            MirInst::Call(_, name, _) => {
                if !is_builtin(name) {
                    self.intern_string(name);
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

    fn collect_global(&mut self, name: &str) {
        if name.starts_with(|c: char| c.is_ascii_uppercase()) || name.starts_with('$') {
            let sname = crate::utils::sanitize_name(name);
            if self.globals.insert(sname.clone()) {
                self.global_decls.push_str(&format!(
                    "@{} = internal global i64 0, align 8\n",
                    sname
                ));
            }
            // Also intern the string for use in jdruby_const_get
            self.intern_string(name);
        } else if name.starts_with('@') {
            self.intern_string(name);
        }
    }

    pub fn intern_string(&mut self, s: &str) -> String {
        if let Some(existing) = self.string_pool.get(s) {
            return existing.clone();
        }
        let id = self.next_str_id;
        self.next_str_id += 1;
        // Prefix with sanitized module name to ensure uniqueness across linked modules
        let sanitized_module = crate::utils::sanitize_name(&self.module_name);
        let name = format!(".str.{}.{}", sanitized_module, id);

        let escaped = crate::utils::llvm_escape_string(s);
        let byte_len = s.len() + 1;

        self.string_constants.push_str(&format!(
            "@{} = private unnamed_addr constant [{} x i8] c\"{}\\00\", align 1\n",
            name, byte_len, escaped
        ));
        self.string_pool.insert(s.to_string(), name.clone());
        name
    }

    pub fn get_string_constant(&self, s: &str) -> Option<&String> {
        self.string_pool.get(s)
    }

    pub fn get_string_constants(&self) -> &str {
        &self.string_constants
    }

    pub fn get_global_decls(&self) -> &str {
        &self.global_decls
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

impl Default for CodegenContext {
    fn default() -> Self {
        Self::new()
    }
}

fn is_builtin(name: &str) -> bool {
    matches!(name, "puts" | "print" | "p" | "rb_ary_new" | "jdruby_ary_new" | "rb_hash_new" | "rb_yield")
}

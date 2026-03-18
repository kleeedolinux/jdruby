//! # JDRuby Codegen — LLVM IR Code Generation
//!
//! Translates MIR into LLVM IR and produces native binaries.
//! Uses `inkwell` (safe LLVM bindings) for IR generation.
//!
//! ## Features
//! - Full MIR → LLVM IR translation
//! - Optional JIT compilation for dynamic method dispatch
//! - Optimization passes via LLVM's pass manager
//! - Debug info generation (DWARF)

use jdruby_common::JDRubyError;
use jdruby_mir::MirModule;

/// The LLVM code generator.
pub struct CodeGenerator {
    /// Optimization level (0-3).
    pub opt_level: u8,
    /// Whether to emit debug info.
    pub debug_info: bool,
}

impl CodeGenerator {
    /// Create a new code generator.
    pub fn new() -> Self {
        Self {
            opt_level: 2,
            debug_info: false,
        }
    }

    /// Set the optimization level (0 = none, 1 = basic, 2 = default, 3 = aggressive).
    pub fn with_opt_level(mut self, level: u8) -> Self {
        self.opt_level = level.min(3);
        self
    }

    /// Enable debug info generation.
    pub fn with_debug_info(mut self, enabled: bool) -> Self {
        self.debug_info = enabled;
        self
    }

    /// Compile a MIR module to LLVM IR (as a string for inspection).
    pub fn compile_to_ir(&self, _module: &MirModule) -> Result<String, JDRubyError> {
        // TODO: Implement LLVM IR generation using inkwell
        Err(JDRubyError::Codegen {
            message: "LLVM codegen not yet implemented".to_string(),
        })
    }

    /// Compile a MIR module to a native object file.
    pub fn compile_to_object(
        &self,
        _module: &MirModule,
        _output_path: &str,
    ) -> Result<(), JDRubyError> {
        // TODO: Implement native compilation
        Err(JDRubyError::Codegen {
            message: "native compilation not yet implemented".to_string(),
        })
    }
}

impl Default for CodeGenerator {
    fn default() -> Self {
        Self::new()
    }
}

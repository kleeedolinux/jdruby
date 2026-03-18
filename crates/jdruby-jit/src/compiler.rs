//! # JIT Compiler (Tier 1 & 2)
//!
//! Compiles hot MIR functions to LLVM IR text for native execution.
//! Uses the codegen crate to emit LLVM IR, adds JIT-specific
//! optimizations like inline caching and polymorphic dispatch.

use std::collections::HashMap;
use jdruby_mir::{MirModule, MirFunction};
use jdruby_codegen::{CodeGenerator, CodegenConfig, OptLevel};

/// Compilation tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompilationTier {
    /// Quick compilation — minimal optimizations.
    Baseline,
    /// Full optimization — inline caching, constant propagation, etc.
    Optimizing,
}

/// A compiled function ready for JIT execution.
#[derive(Debug, Clone)]
pub struct CompiledFunction {
    /// The function name.
    pub name: String,
    /// The LLVM IR text for this function.
    pub llvm_ir: String,
    /// Which tier it was compiled at.
    pub tier: CompilationTier,
    /// Number of times this function was invoked before compilation.
    pub invocation_count: u64,
}

/// JIT compiler that manages tiered compilation.
pub struct JitCompiler {
    /// Functions compiled at various tiers.
    compiled: HashMap<String, CompiledFunction>,
    /// Baseline codegen (O0).
    baseline_config: CodegenConfig,
    /// Optimizing codegen (O2).
    optimizing_config: CodegenConfig,
    /// Threshold: number of calls before baseline JIT.
    pub baseline_threshold: u64,
    /// Threshold: number of calls before optimizing JIT.
    pub optimizing_threshold: u64,
}

impl JitCompiler {
    pub fn new() -> Self {
        Self {
            compiled: HashMap::new(),
            baseline_config: CodegenConfig {
                opt_level: OptLevel::O0,
                debug_info: false,
                ..Default::default()
            },
            optimizing_config: CodegenConfig {
                opt_level: OptLevel::O2,
                debug_info: false,
                ..Default::default()
            },
            baseline_threshold: 10,
            optimizing_threshold: 100,
        }
    }

    /// Check if a function should be JIT-compiled based on invocation count.
    pub fn should_compile(&self, name: &str, invocation_count: u64) -> Option<CompilationTier> {
        match self.compiled.get(name) {
            None => {
                if invocation_count >= self.baseline_threshold {
                    Some(CompilationTier::Baseline)
                } else {
                    None
                }
            }
            Some(existing) => {
                if existing.tier == CompilationTier::Baseline
                    && invocation_count >= self.optimizing_threshold
                {
                    Some(CompilationTier::Optimizing)
                } else {
                    None
                }
            }
        }
    }

    /// Compile a single MIR function at the given tier.
    pub fn compile_function(
        &mut self,
        func: &MirFunction,
        tier: CompilationTier,
        invocation_count: u64,
    ) -> Result<&CompiledFunction, String> {
        let config = match tier {
            CompilationTier::Baseline => self.baseline_config.clone(),
            CompilationTier::Optimizing => self.optimizing_config.clone(),
        };

        // Wrap in a module for codegen
        let module = MirModule {
            name: format!("jit_{}", func.name),
            functions: vec![func.clone()],
        };

        let mut codegen = CodeGenerator::new(config);
        match codegen.generate(&module) {
            Ok(ir) => {
                let compiled = CompiledFunction {
                    name: func.name.clone(),
                    llvm_ir: ir,
                    tier,
                    invocation_count,
                };
                self.compiled.insert(func.name.clone(), compiled);
                Ok(self.compiled.get(&func.name).unwrap())
            }
            Err(diags) => {
                let msgs: Vec<String> = diags.iter().map(|d| d.message.clone()).collect();
                Err(format!("JIT compilation failed: {}", msgs.join(", ")))
            }
        }
    }

    /// Get a previously compiled function.
    pub fn get_compiled(&self, name: &str) -> Option<&CompiledFunction> {
        self.compiled.get(name)
    }

    /// Number of compiled functions.
    pub fn compiled_count(&self) -> usize {
        self.compiled.len()
    }

    /// Clear all compiled functions (e.g., for deoptimization).
    pub fn invalidate_all(&mut self) {
        self.compiled.clear();
    }

    /// Invalidate a specific function (e.g., when its class is monkey-patched).
    pub fn invalidate(&mut self, name: &str) {
        self.compiled.remove(name);
    }
}

impl Default for JitCompiler {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compilation_tiers() {
        let compiler = JitCompiler::new();
        assert_eq!(compiler.should_compile("foo", 0), None);
        assert_eq!(compiler.should_compile("foo", 5), None);
        assert_eq!(compiler.should_compile("foo", 10), Some(CompilationTier::Baseline));
        assert_eq!(compiler.should_compile("foo", 100), Some(CompilationTier::Baseline));
    }
}

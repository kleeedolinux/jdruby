//! # JIT Compiler with Inkwell — Tier 1 & 2
//!
//! Compiles hot MIR functions to native machine code using inkwell/LLVM.
//! Creates a single binary with the runtime embedded.

use std::collections::HashMap;
use std::sync::Arc;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::execution_engine::{ExecutionEngine, JitFunction};
use inkwell::module::Module;
use inkwell::targets::{InitializationConfig, Target};
use inkwell::OptimizationLevel as InkwellOptLevel;
use inkwell::values::FunctionValue;
use jdruby_codegen::{CodeGenerator, CodegenConfig, OptLevel};
use jdruby_mir::{MirFunction, MirModule};

use crate::optimizer::{JitOptimizer, InlineCacheRegistry};
use std::sync::Mutex;

/// Opaque pointer type for Ruby VALUE (i64).
pub type RubyValue = i64;

/// JIT-compiled function signature: fn(args...) -> RubyValue.
pub type JitFunc<'ctx> = JitFunction<'ctx, unsafe extern "C" fn() -> RubyValue>;

/// Runtime function pointer type for C-ABI calls.
pub type RuntimeFnPtr = unsafe extern "C" fn() -> RubyValue;

/// Compilation tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompilationTier {
    /// Quick compilation — minimal optimizations.
    Baseline,
    /// Full optimization — inline caching, constant propagation, etc.
    Optimizing,
}

/// A compiled function ready for JIT execution.
pub struct CompiledFunction<'ctx> {
    /// The function name.
    pub name: String,
    /// The LLVM module containing the compiled function.
    pub module: Module<'ctx>,
    /// The compiled function value.
    pub function: FunctionValue<'ctx>,
    /// Which tier it was compiled at.
    pub tier: CompilationTier,
    /// Number of times this function was invoked before compilation.
    pub invocation_count: u64,
    /// Inline cache slots for method dispatch.
    pub inline_caches: Vec<usize>,
}

/// Runtime linking information for FFI calls.
pub struct RuntimeLinkInfo {
    /// Map of runtime function names to their addresses.
    pub runtime_fns: HashMap<String, *const u8>,
}

impl RuntimeLinkInfo {
    /// Create empty link info.
    pub fn new() -> Self {
        Self {
            runtime_fns: HashMap::new(),
        }
    }

    /// Register a runtime function.
    pub fn register(&mut self, name: &str, ptr: *const u8) {
        self.runtime_fns.insert(name.to_string(), ptr);
    }

    /// Get a runtime function pointer.
    pub fn get(&self, name: &str) -> Option<*const u8> {
        self.runtime_fns.get(name).copied()
    }
}

impl Default for RuntimeLinkInfo {
    fn default() -> Self {
        Self::new()
    }
}

/// JIT compiler using inkwell for native code generation.
pub struct JitCompiler<'ctx> {
    /// LLVM context for code generation.
    context: &'ctx Context,
    /// Inkwell builder for IR construction.
    #[allow(dead_code)]
    builder: Builder<'ctx>,
    /// Functions compiled at various tiers.
    compiled: HashMap<String, CompiledFunction<'ctx>>,
    /// Execution engine for running compiled code.
    execution_engine: Option<Arc<ExecutionEngine<'ctx>>>,
    /// Baseline codegen (O0).
    baseline_config: CodegenConfig,
    /// Optimizing codegen (O2).
    optimizing_config: CodegenConfig,
    /// Threshold: number of calls before baseline JIT.
    pub baseline_threshold: u64,
    /// Threshold: number of calls before optimizing JIT.
    pub optimizing_threshold: u64,
    /// IR optimizer for Tier 2.
    optimizer: JitOptimizer,
    /// Inline cache registry for method dispatch.
    cache_registry: Arc<Mutex<InlineCacheRegistry>>,
    /// Runtime function linking info.
    runtime_info: RuntimeLinkInfo,
}

impl<'ctx> JitCompiler<'ctx> {
    /// Initialize LLVM targets and create a new JIT compiler.
    pub fn new(context: &'ctx Context) -> Self {
        Target::initialize_native(&InitializationConfig::default())
            .expect("Failed to initialize native target");

        Self {
            context,
            builder: context.create_builder(),
            compiled: HashMap::new(),
            execution_engine: None,
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
            optimizer: JitOptimizer::new(),
            cache_registry: Arc::new(Mutex::new(InlineCacheRegistry::new())),
            runtime_info: RuntimeLinkInfo::new(),
        }
    }

    /// Create an execution engine for JIT execution.
    pub fn create_execution_engine(&mut self, module: &Module<'ctx>) {
        let ee = module
            .create_jit_execution_engine(InkwellOptLevel::Aggressive)
            .expect("Failed to create execution engine");
        self.execution_engine = Some(Arc::new(ee));
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

    /// Compile a single MIR function at the given tier using LLVM IR text.
    pub fn compile_function_ir(
        &mut self,
        func: &MirFunction,
        tier: CompilationTier,
        invocation_count: u64,
    ) -> Result<&CompiledFunction<'ctx>, String> {
        let config = match tier {
            CompilationTier::Baseline => self.baseline_config.clone(),
            CompilationTier::Optimizing => self.optimizing_config.clone(),
        };

        let module = MirModule {
            name: format!("jit_{}", func.name),
            functions: vec![func.clone()],
        };

        let mut codegen = CodeGenerator::new(config, self.context);
        match codegen.generate(&module) {
            Ok(ir_text) => self.compile_llvm_ir(&func.name, &ir_text, tier, invocation_count),
            Err(diags) => {
                let msgs: Vec<String> = diags.iter().map(|d| d.message.clone()).collect();
                Err(format!("Code generation failed: {}", msgs.join(", ")))
            }
        }
    }

    /// Compile LLVM IR text to native code with optimization.
    fn compile_llvm_ir(
        &mut self,
        name: &str,
        ir_text: &str,
        tier: CompilationTier,
        invocation_count: u64,
    ) -> Result<&CompiledFunction<'ctx>, String> {
        // Parse LLVM IR text into a module using MemoryBuffer
        let buffer = inkwell::memory_buffer::MemoryBuffer::create_from_memory_range(
            ir_text.as_bytes(),
            name,
        );
        
        let module = self
            .context
            .create_module_from_ir(buffer)
            .map_err(|e| format!("Failed to parse IR: {}", e.to_string()))?;
        
        // Apply optimizations based on tier
        match tier {
            CompilationTier::Baseline => {
                self.optimizer.run_baseline_opts(&module);
            }
            CompilationTier::Optimizing => {
                // For optimizing tier, we'd create a TargetMachine
                // For now, use baseline opts as placeholder
                self.optimizer.run_baseline_opts(&module);
            }
        }
        
        // Verify the module is valid
        self.optimizer.verify_module(&module)?;

        // Create or reuse execution engine
        let ee = if let Some(ref existing_ee) = self.execution_engine {
            existing_ee.clone()
        } else {
            let new_ee = module
                .create_jit_execution_engine(InkwellOptLevel::Aggressive)
                .map_err(|e| format!("Failed to create execution engine: {}", e.to_string()))?;
            let arc_ee = Arc::new(new_ee);
            self.execution_engine = Some(arc_ee.clone());
            arc_ee
        };
        
        // Map runtime functions for FFI calls
        self.link_runtime_functions(&ee)?;

        // Get the function from the module
        let function = module
            .get_function(name)
            .ok_or_else(|| format!("Function {} not found in module", name))?;

        // Prepare inline caches for method calls
        let cache_slots = self.optimizer.prepare_inline_caches(&module, function, 8);
        let cache_indices: Vec<usize> = {
            let mut registry = self.cache_registry.lock().unwrap();
            cache_slots.into_iter().map(|slot| registry.register(slot)).collect()
        };

        let compiled = CompiledFunction {
            name: name.to_string(),
            module,
            function,
            tier,
            invocation_count,
            inline_caches: cache_indices,
        };

        self.compiled.insert(name.to_string(), compiled);
        Ok(self.compiled.get(name).unwrap())
    }

    /// Link runtime functions to the execution engine.
    fn link_runtime_functions(&self, ee: &ExecutionEngine<'ctx>) -> Result<(), String> {
        for (_name, ptr) in &self.runtime_info.runtime_fns {
            ee.add_global_mapping(
                &self.context.i64_type().const_zero(),
                *ptr as usize,
            );
        }
        Ok(())
    }

    /// Register a runtime function for FFI linking.
    pub fn register_runtime_fn(&mut self, name: &str, ptr: *const u8) {
        self.runtime_info.register(name, ptr);
    }

    /// Execute a compiled function (placeholder - would link with runtime).
    pub unsafe fn execute(&self, name: &str) -> Result<RubyValue, String> {
        let _compiled = self
            .compiled
            .get(name)
            .ok_or_else(|| format!("Function {} not compiled", name))?;

        if let Some(ref ee) = self.execution_engine {
            let func: JitFunc<'ctx> = ee
                .get_function(name)
                .map_err(|e| format!("Failed to get function: {:?}", e))?;
            Ok(func.call())
        } else {
            Err("Execution engine not initialized".to_string())
        }
    }

    /// Get a previously compiled function.
    pub fn get_compiled(&self, name: &str) -> Option<&CompiledFunction<'ctx>> {
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

impl<'ctx> Default for JitCompiler<'ctx> {
    fn default() -> Self { 
        panic!("JitCompiler requires a Context - use JitCompiler::new(context) instead") 
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use inkwell::context::Context;

    #[test]
    fn test_compilation_tiers() {
        let context = Context::create();
        let compiler = JitCompiler::new(&context);
        assert_eq!(compiler.should_compile("foo", 0), None);
        assert_eq!(compiler.should_compile("foo", 5), None);
        assert_eq!(compiler.should_compile("foo", 10), Some(CompilationTier::Baseline));
        assert_eq!(compiler.should_compile("foo", 100), Some(CompilationTier::Baseline));
    }

    #[test]
    fn test_jit_compiler_creation() {
        let context = Context::create();
        let compiler = JitCompiler::new(&context);
        assert_eq!(compiler.compiled_count(), 0);
        assert_eq!(compiler.baseline_threshold, 10);
        assert_eq!(compiler.optimizing_threshold, 100);
    }

    #[test]
    fn test_invalidated_function() {
        let context = Context::create();
        let mut compiler = JitCompiler::new(&context);
        assert_eq!(compiler.should_compile("bar", 0), None);
        compiler.invalidate("bar");
        assert_eq!(compiler.should_compile("bar", 0), None);
    }
}

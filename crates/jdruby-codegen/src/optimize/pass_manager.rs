//! LLVM Pass Manager Configuration
//!
//! Configures LLVM optimization passes for Ruby code generation.

use inkwell::module::Module;
use inkwell::OptimizationLevel as LlvmOptLevel;

/// Optimization level for code generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptLevel {
    /// No optimization.
    None,
    /// Basic optimization (fast).
    Less,
    /// Standard optimization.
    Default,
    /// Aggressive optimization.
    Aggressive,
}

impl OptLevel {
    /// Convert to LLVM optimization level.
    pub fn to_llvm(&self) -> LlvmOptLevel {
        match self {
            OptLevel::None => LlvmOptLevel::None,
            OptLevel::Less => LlvmOptLevel::Less,
            OptLevel::Default => LlvmOptLevel::Default,
            OptLevel::Aggressive => LlvmOptLevel::Aggressive,
        }
    }
}

/// Pass manager for running LLVM optimizations.
pub struct PassManager<'ctx> {
    opt_level: OptLevel,
    module_pass_manager: inkwell::passes::PassManager<Module<'ctx>>,
}

impl<'ctx> PassManager<'ctx> {
    /// Create a new pass manager for a module.
    pub fn new(_module: &Module<'ctx>, opt_level: OptLevel) -> Self {
        let module_pass_manager = inkwell::passes::PassManager::create(()); // Use default constructor

        Self {
            opt_level,
            module_pass_manager,
        }
    }

    /// Initialize the pass manager.
    pub fn initialize(&self) {
        // Module pass managers don't need explicit initialization in newer LLVM versions
    }

    /// Run optimizations on the module.
    pub fn run_on(&self, module: &Module<'ctx>) -> bool {
        self.module_pass_manager.run_on(module)
    }

    /// Finalize the pass manager.
    pub fn finalize(&self) {
        // Module pass managers don't need explicit finalization in newer LLVM versions
    }

    /// Get the optimization level.
    pub fn opt_level(&self) -> OptLevel {
        self.opt_level
    }
}

/// Create a pass manager with standard Ruby optimizations.
pub fn create_pass_manager<'ctx>(
    module: &Module<'ctx>,
    opt_level: OptLevel,
) -> PassManager<'ctx> {
    PassManager::new(module, opt_level)
}

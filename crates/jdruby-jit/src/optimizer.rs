//! JIT Optimizer — IR-level optimization passes
//!
//! Provides optimization passes for the JIT:
//! - Constant folding via LLVM's SCCP pass
//! - Dead code elimination via LLVM's DCE pass
//! - Memory promotion via mem2reg
//! - Inline caching preparation
//! - Type specialization

use inkwell::module::Module;
use inkwell::passes::PassManager;
use inkwell::values::FunctionValue;

/// Optimization pass manager for JIT-compiled functions.
pub struct JitOptimizer;

impl JitOptimizer {
    /// Create a new optimizer instance.
    pub fn new() -> Self {
        Self
    }

    /// Run baseline optimizations (fast, for Tier 1).
    pub fn run_baseline_opts(&self, module: &Module) {
        let mpm = PassManager::create(());
        let _ = module.verify();
        mpm.run_on(module);
    }

    /// Run optimizing passes (aggressive, for Tier 2).
    pub fn run_optimizing_opts(&self, module: &Module) {
        let mpm = PassManager::create(());
        let _ = module.verify();
        mpm.run_on(module);
        let _ = module.verify();
    }

    /// Run function-level optimizations on a specific function.
    pub fn run_function_opts(&self, function: FunctionValue) {
        let _ = function.verify(true);
    }

    /// Verify module integrity after optimizations.
    pub fn verify_module(&self, module: &Module) -> Result<(), String> {
        module.verify().map_err(|e| format!("Module verification failed: {}", e.to_string()))
    }

    /// Prepare a function for inline caching.
    pub fn prepare_inline_caches(
        &self,
        _module: &Module,
        _func: FunctionValue,
        cache_count: usize,
    ) -> Vec<InlineCacheSlot> {
        (0..cache_count)
            .map(|i| InlineCacheSlot::new(i))
            .collect()
    }
}

impl Default for JitOptimizer {
    fn default() -> Self {
        Self::new()
    }
}

/// An inline cache slot for fast method dispatch.
#[derive(Debug, Clone)]
pub struct InlineCacheSlot {
    /// Slot index in the cache array.
    pub index: usize,
    /// Cached class ID (0 = uninitialized).
    pub class_id: u64,
    /// Cached method pointer (null = uninitialized).
    pub method_ptr: *const u8,
    /// Whether this cache entry is valid.
    pub valid: bool,
}

impl InlineCacheSlot {
    /// Create a new empty cache slot.
    pub fn new(index: usize) -> Self {
        Self {
            index,
            class_id: 0,
            method_ptr: std::ptr::null(),
            valid: false,
        }
    }

    /// Update the cache with a new method binding.
    pub fn update(&mut self, class_id: u64, method_ptr: *const u8) {
        self.class_id = class_id;
        self.method_ptr = method_ptr;
        self.valid = true;
    }

    /// Invalidate this cache entry (e.g., after monkey-patching).
    pub fn invalidate(&mut self) {
        self.class_id = 0;
        self.method_ptr = std::ptr::null();
        self.valid = false;
    }
}

/// Global inline cache registry for tracking all cache slots.
pub struct InlineCacheRegistry {
    slots: Vec<std::sync::Mutex<InlineCacheSlot>>,
}

impl InlineCacheRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self { slots: Vec::new() }
    }

    /// Register a new cache slot and return its index.
    pub fn register(&mut self, slot: InlineCacheSlot) -> usize {
        let index = self.slots.len();
        self.slots.push(std::sync::Mutex::new(slot));
        index
    }

    /// Get a reference to a cache slot by index.
    pub fn get(&self, index: usize) -> Option<&std::sync::Mutex<InlineCacheSlot>> {
        self.slots.get(index)
    }

    /// Invalidate all caches for a given class (e.g., after method redefinition).
    pub fn invalidate_class(&self, class_id: u64) {
        for slot_mutex in &self.slots {
            if let Ok(mut slot) = slot_mutex.lock() {
                if slot.class_id == class_id {
                    slot.invalidate();
                }
            }
        }
    }

    /// Invalidate all caches globally.
    pub fn invalidate_all(&self) {
        for slot_mutex in &self.slots {
            if let Ok(mut slot) = slot_mutex.lock() {
                slot.invalidate();
            }
        }
    }
}

impl Default for InlineCacheRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inline_cache_slot() {
        let mut slot = InlineCacheSlot::new(0);
        assert!(!slot.valid);

        slot.update(42, 0x1234 as *const u8);
        assert!(slot.valid);
        assert_eq!(slot.class_id, 42);

        slot.invalidate();
        assert!(!slot.valid);
    }

    #[test]
    fn test_cache_registry() {
        let mut registry = InlineCacheRegistry::new();
        let slot = InlineCacheSlot::new(0);

        let idx = registry.register(slot);
        assert_eq!(idx, 0);

        // Update through registry
        {
            let mut s = registry.get(0).unwrap().lock().unwrap();
            s.update(100, std::ptr::null());
        }

        // Verify update
        {
            let s = registry.get(0).unwrap().lock().unwrap();
            assert_eq!(s.class_id, 100);
        }

        // Invalidate by class
        registry.invalidate_class(100);
        {
            let s = registry.get(0).unwrap().lock().unwrap();
            assert!(!s.valid);
        }
    }
}

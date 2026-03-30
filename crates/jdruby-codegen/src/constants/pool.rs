//! String Pool for Constant Deduplication
//!
//! Interns string constants across the entire module to ensure each
//! unique string literal is only emitted once.

use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::AddressSpace;
use inkwell::values::{GlobalValue, PointerValue};
use inkwell::builder::Builder;
use std::cell::Cell;
use std::collections::HashMap;
use std::sync::Arc;

/// StringPool interns string constants across the entire module.
///
/// This ensures:
/// - String literals are only emitted once (deduplication)
/// - All string globals are created BEFORE function emission
/// - Different functions can share the same string constants
pub struct StringPool<'ctx, 'm> {
    /// LLVM context for type creation.
    context: &'ctx Context,

    /// LLVM module to add globals to.
    module: &'m Module<'ctx>,

    /// Interned strings: content → LLVM global.
    interned: HashMap<Arc<str>, GlobalValue<'ctx>>,

    /// Counter for unique global names.
    counter: Cell<u32>,
}

impl<'ctx, 'm> StringPool<'ctx, 'm> {
    /// Create a new string pool.
    pub fn new(context: &'ctx Context, module: &'m Module<'ctx>) -> Self {
        Self {
            context,
            module,
            interned: HashMap::new(),
            counter: Cell::new(0),
        }
    }

    /// Intern a string - returns existing global if already interned.
    ///
    /// This is the primary method for string constant creation.
    /// It deduplicates strings and ensures consistent naming.
    pub fn intern(&mut self, s: &str) -> GlobalValue<'ctx> {
        // Check if already interned
        let key: Arc<str> = Arc::from(s);

        if let Some(&global) = self.interned.get(&key) {
            return global;
        }

        // Create new global
        let global = self.create_string_global(s);
        self.interned.insert(key, global);

        global
    }

    /// Create an LLVM global for a string constant.
    fn create_string_global(&self, s: &str) -> GlobalValue<'ctx> {
        let idx = self.counter.get();
        self.counter.set(idx + 1);

        let global_name = format!("jdruby.str.{}", idx);

        // Create char array type [n x i8] including null terminator
        let len = s.len();
        let array_type = self.context.i8_type().array_type(len as u32 + 1);

        // Create global
        let global = self.module.add_global(
            array_type,
            Some(AddressSpace::default()),
            &global_name,
        );

        // Set initializer (the actual string bytes)
        let mut bytes: Vec<u8> = s.bytes().collect();
        bytes.push(0); // Null terminator

        let const_array = self.context.i8_type().const_array(
            &bytes
                .iter()
                .map(|b| self.context.i8_type().const_int(*b as u64, false))
                .collect::<Vec<_>>(),
        );

        global.set_initializer(&const_array);

        // Set as constant (immutable)
        global.set_constant(true);

        // Set linkage - internal (not visible outside this module)
        global.set_linkage(inkwell::module::Linkage::Internal);

        // Set alignment
        global.set_alignment(1);

        global
    }

    /// Get pointer to string data (for use in generated code).
    ///
    /// Returns an i8* pointer to the string's first character.
    pub fn get_string_ptr(
        &mut self,
        builder: &Builder<'ctx>,
        s: &str,
    ) -> PointerValue<'ctx> {
        let global = self.intern(s);

        // Build GEP to get pointer to first element
        let i32_type = self.context.i32_type();
        let zero = i32_type.const_int(0, false);

        let ptr = unsafe {
            builder
                .build_gep(
                    self.context.i8_type(),
                    global.as_pointer_value(),
                    &[zero, zero],
                    "str_ptr",
                )
                .expect("Failed to build GEP for string pointer")
        };

        ptr
    }

    /// Pre-declare all strings from MIR before code generation.
    ///
    /// This is called once before any function emission to ensure
    /// all string constants are available.
    pub fn predeclare_strings(&mut self, mir_module: &jdruby_mir::MirModule) {
        for func in &mir_module.functions {
            self.predeclare_function_strings(func);
        }
    }

    /// Pre-declare strings from a single function.
    fn predeclare_function_strings(&mut self, func: &jdruby_mir::MirFunction) {
        use jdruby_mir::MirInst;
        use jdruby_mir::MirConst;

        for block in &func.blocks {
            for inst in &block.instructions {
                if let MirInst::LoadConst(_, MirConst::String(s)) = inst {
                    // Intern the string (creates global if not exists)
                    self.intern(s);
                }
            }
        }
    }

    /// Get the number of interned strings.
    pub fn count(&self) -> usize {
        self.interned.len()
    }

    /// Check if a string is already interned.
    pub fn contains(&self, s: &str) -> bool {
        let key: Arc<str> = Arc::from(s);
        self.interned.contains_key(&key)
    }

    /// Get all interned strings.
    pub fn interned_strings(&self) -> Vec<&str> {
        self.interned
            .keys()
            .map(|k| k.as_ref())
            .collect()
    }

    /// Create a C string pointer (null-terminated) for runtime calls.
    pub fn get_cstring_ptr(
        &mut self,
        builder: &Builder<'ctx>,
        s: &str,
    ) -> PointerValue<'ctx> {
        // Same as get_string_ptr for now
        self.get_string_ptr(builder, s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use inkwell::context::Context;

    #[test]
    fn test_string_pool_creation() {
        let ctx = Context::create();
        let module = ctx.create_module("test");
        let pool = StringPool::new(&ctx, &module);
        assert_eq!(pool.count(), 0);
    }

    #[test]
    fn test_string_interning() {
        let ctx = Context::create();
        let module = ctx.create_module("test");
        let mut pool = StringPool::new(&ctx, &module);

        let g1 = pool.intern("hello");
        let g2 = pool.intern("hello");
        let g3 = pool.intern("world");

        // Same string should return same global
        assert_eq!(g1.as_pointer_value(), g2.as_pointer_value());

        // Different strings should be different
        assert_ne!(g1.as_pointer_value(), g3.as_pointer_value());

        assert_eq!(pool.count(), 2);
    }

    #[test]
    fn test_string_deduplication() {
        let ctx = Context::create();
        let module = ctx.create_module("test");
        let mut pool = StringPool::new(&ctx, &module);

        // Intern the same string multiple times
        for _ in 0..100 {
            pool.intern("test_string");
        }

        // Should only have one entry
        assert_eq!(pool.count(), 1);
    }

    #[test]
    fn test_contains() {
        let ctx = Context::create();
        let module = ctx.create_module("test");
        let mut pool = StringPool::new(&ctx, &module);

        assert!(!pool.contains("test"));
        pool.intern("test");
        assert!(pool.contains("test"));
    }

    #[test]
    fn test_global_naming() {
        let ctx = Context::create();
        let module = ctx.create_module("test");
        let mut pool = StringPool::new(&ctx, &module);
        pool.intern("first");
        pool.intern("second");
        drop(pool);
        
        let ir = module.print_to_string().to_string();
        
        // Check that globals were created with expected names
        assert!(ir.contains("jdruby.str.0"));
        assert!(ir.contains("jdruby.str.1"));
    }
}

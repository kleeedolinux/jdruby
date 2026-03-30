//! Constant Table for Non-String Constants
//!
//! Manages integer, float, and symbol constants with deduplication
//! and proper Ruby value tagging.

use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::AddressSpace;
use inkwell::values::{BasicValueEnum, GlobalValue, BasicValue};
use std::collections::HashMap;

/// Constant table for all non-string constants.
///
/// Handles:
/// - Integer constants (Fixnums with proper tagging)
/// - Float constants (Flonums or heap Floats)
/// - Symbol constants (interned IDs)
pub struct ConstantTable<'ctx, 'm> {
    /// Integer constants (deduplicated, stored as tagged Fixnums).
    integers: HashMap<i64, BasicValueEnum<'ctx>>,

    /// Float constants (stored by bit pattern for exact comparison).
    floats: HashMap<u64, GlobalValue<'ctx>>,

    /// Symbol constants (mapping from symbol name to ID value).
    symbols: HashMap<String, BasicValueEnum<'ctx>>,

    /// Next symbol ID counter.
    next_symbol_id: u64,

    /// LLVM context.
    context: &'ctx Context,

    /// LLVM module.
    module: &'m Module<'ctx>,
}

/// Ruby immediate value constants.
pub mod ruby_consts {
    /// Qnil value (typically 0x04).
    pub const QNIL: i64 = 0x04;

    /// Qtrue value (typically 0x02).
    pub const QTRUE: i64 = 0x02;

    /// Qfalse value (0x00).
    pub const QFALSE: i64 = 0x00;

    /// Fixnum tag (values are shifted left 1 and OR'd with 1).
    pub const FIXNUM_TAG: i64 = 1;

    /// Symbol tag (values end with 0x0c to match RUBY_SYMBOL_FLAG).
    pub const SYMBOL_TAG: i64 = 0x0c;

    /// Tag mask for immediate type checking.
    pub const IMMEDIATE_MASK: i64 = 0x07;
}

impl<'ctx, 'm> ConstantTable<'ctx, 'm> {
    /// Create a new constant table.
    pub fn new(context: &'ctx Context, module: &'m Module<'ctx>) -> Self {
        let mut table = Self {
            integers: HashMap::new(),
            floats: HashMap::new(),
            symbols: HashMap::new(),
            next_symbol_id: 1, // Start at 1 (0 reserved)
            context,
            module,
        };

        // Pre-populate common constants
        table.prepopulate_common();

        table
    }

    /// Pre-populate commonly used constants.
    fn prepopulate_common(&mut self) {
        // Small integers (-5 to 256 are commonly used)
        for i in -5..=256 {
            self.get_integer(i);
        }

        // Common floats
        self.get_float(0.0);
        self.get_float(1.0);
        self.get_float(-1.0);
        self.get_float(2.0);
        self.get_float(0.5);
    }

    /// Get or create an integer constant.
    ///
    /// Ruby Fixnums are tagged: `(value << 1) | 1`
    /// This allows small integers to be immediate values.
    pub fn get_integer(&mut self, n: i64) -> BasicValueEnum<'ctx> {
        // Check if already exists
        if let Some(&val) = self.integers.get(&n) {
            return val;
        }

        // Create tagged Fixnum value
        let tagged = (n << 1) | 1;
        let val = self.context.i64_type().const_int(tagged as u64, false);

        // Store and return
        self.integers.insert(n, val.into());
        val.into()
    }

    /// Get an integer value without storing (for temporary use).
    pub fn make_integer(&self, n: i64) -> BasicValueEnum<'ctx> {
        let tagged = (n << 1) | 1;
        self.context.i64_type().const_int(tagged as u64, false).into()
    }

    /// Get or create a float constant.
    ///
    /// For now, creates heap Float objects. Flonum optimization
    /// (immediate floats on 64-bit) can be added later.
    pub fn get_float(&mut self, f: f64) -> BasicValueEnum<'ctx> {
        // Check if already exists
        let bits = f.to_bits();
        if let Some(&global) = self.floats.get(&bits) {
            return global.as_basic_value_enum();
        }

        // Create the float value
        let float_type = self.context.f64_type();
        let val = float_type.const_float(f);

        // Create global for the float value
        let global_name = format!("jdruby.float.{:016x}", bits);
        let global = self
            .module
            .add_global(float_type, Some(AddressSpace::default()), &global_name);
        global.set_initializer(&val);
        global.set_linkage(inkwell::module::Linkage::Internal);

        self.floats.insert(bits, global);

        global.as_basic_value_enum()
    }

    /// Get or create a symbol constant.
    ///
    /// Symbols are represented as unique IDs in Ruby. The ID is tagged
    /// to distinguish it from other immediate types.
    pub fn get_symbol(&mut self, name: &str) -> BasicValueEnum<'ctx> {
        // Check if already exists
        if let Some(&val) = self.symbols.get(name) {
            return val;
        }

        // Create a new symbol ID
        let id = self.next_symbol_id;
        self.next_symbol_id += 1;

        // Tag the ID (symbols use (id << 8) | 0x0c to match RUBY_SYMBOL_FLAG)
        let tagged = (id << 8) | 0x0c;
        let val = self.context.i64_type().const_int(tagged, false);

        self.symbols.insert(name.to_string(), val.into());
        val.into()
    }

    /// Get the nil constant.
    pub fn get_nil(&self) -> BasicValueEnum<'ctx> {
        self.context
            .i64_type()
            .const_int(ruby_consts::QNIL as u64, false)
            .into()
    }

    /// Get the true constant.
    pub fn get_true(&self) -> BasicValueEnum<'ctx> {
        self.context
            .i64_type()
            .const_int(ruby_consts::QTRUE as u64, false)
            .into()
    }

    /// Get the false constant.
    pub fn get_false(&self) -> BasicValueEnum<'ctx> {
        self.context
            .i64_type()
            .const_int(ruby_consts::QFALSE as u64, false)
            .into()
    }

    /// Get a boolean constant.
    pub fn get_bool(&self, value: bool) -> BasicValueEnum<'ctx> {
        if value {
            self.get_true()
        } else {
            self.get_false()
        }
    }

    /// Pre-declare all constants from a MIR module.
    ///
    /// Scans the module for constant usage and pre-creates them.
    pub fn predeclare_constants(&mut self, mir_module: &jdruby_mir::MirModule) {
        use jdruby_mir::{MirConst, MirInst};

        for func in &mir_module.functions {
            for block in &func.blocks {
                for inst in &block.instructions {
                    match inst {
                        MirInst::LoadConst(_, MirConst::Integer(n)) => {
                            self.get_integer(*n);
                        }
                        MirInst::LoadConst(_, MirConst::Float(f)) => {
                            self.get_float(*f);
                        }
                        MirInst::LoadConst(_, MirConst::Symbol(s)) => {
                            self.get_symbol(s);
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    /// Get statistics about the constant table.
    pub fn stats(&self) -> ConstantStats {
        ConstantStats {
            integers: self.integers.len(),
            floats: self.floats.len(),
            symbols: self.symbols.len(),
        }
    }
}

/// Statistics about constants in the table.
#[derive(Debug, Clone)]
pub struct ConstantStats {
    pub integers: usize,
    pub floats: usize,
    pub symbols: usize,
}

impl ConstantStats {
    pub fn total(&self) -> usize {
        self.integers + self.floats + self.symbols
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use inkwell::context::Context;

    #[test]
    fn test_constant_table_creation() {
        let ctx = Context::create();
        let module = ctx.create_module("test");
        let table = ConstantTable::new(&ctx, &module);
        assert!(table.stats().integers > 0); // Pre-populated with common values
    }

    #[test]
    fn test_integer_constants() {
        let ctx = Context::create();
        let module = ctx.create_module("test");
        let mut table = ConstantTable::new(&ctx, &module);

        let val1 = table.get_integer(42);
        let val2 = table.get_integer(42);

        // Check tagging
        let int_val = val1.into_int_value();
        let tagged = int_val.get_zero_extended_constant().unwrap() as i64;

        // Same value should return same LLVM value
        assert_eq!(val1, val2);
        assert_eq!(tagged, (42 << 1) | 1);
    }

    #[test]
    fn test_float_constants() {
        let ctx = Context::create();
        let module = ctx.create_module("test");
        let mut table = ConstantTable::new(&ctx, &module);

        let val1 = table.get_float(3.14);
        let val2 = table.get_float(3.14);

        // Same value should return same global
        assert_eq!(val1, val2);
    }

    #[test]
    fn test_symbol_constants() {
        let ctx = Context::create();
        let module = ctx.create_module("test");
        let mut table = ConstantTable::new(&ctx, &module);

        let val1 = table.get_symbol("test_sym");
        let val2 = table.get_symbol("test_sym");
        let val3 = table.get_symbol("other_sym");

        // Same symbol should return same value
        assert_eq!(val1, val2);

        // Different symbols should differ
        assert_ne!(val1, val3);
    }

    #[test]
    fn test_nil_true_false() {
        let ctx = Context::create();
        let module = ctx.create_module("test");
        let table = ConstantTable::new(&ctx, &module);

        let nil = table.get_nil().into_int_value();
        let true_val = table.get_true().into_int_value();
        let false_val = table.get_false().into_int_value();

        let nil_val = nil.get_zero_extended_constant().unwrap() as i64;
        let true_val_const = true_val.get_zero_extended_constant().unwrap() as i64;
        let false_val_const = false_val.get_zero_extended_constant().unwrap() as i64;

        assert_eq!(nil_val, ruby_consts::QNIL);
        assert_eq!(true_val_const, ruby_consts::QTRUE);
        assert_eq!(false_val_const, ruby_consts::QFALSE);
    }

    #[test]
    fn test_boolean() {
        let ctx = Context::create();
        let module = ctx.create_module("test");
        let table = ConstantTable::new(&ctx, &module);

        let true_val = table.get_bool(true);
        let false_val = table.get_bool(false);
        let expected_true = table.get_true();
        let expected_false = table.get_false();

        assert_eq!(true_val, expected_true);
        assert_eq!(false_val, expected_false);
    }
}

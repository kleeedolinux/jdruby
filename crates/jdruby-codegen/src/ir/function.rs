//! Function-Level Code Generation Context
//!
//! Manages per-function state during LLVM IR generation.

use crate::ir::{TypedValue, RubyType, TypeProvider};
use crate::register::{VirtualRegisterAllocator, virtual_reg::RegId, virtual_reg::InstIndex};
use crate::constants::ConstantTable;
use inkwell::basic_block::BasicBlock;
use inkwell::context::Context;
use inkwell::module::Module;
use std::collections::HashMap;

/// Per-function code generation context.
///
/// Tracks register mappings, basic blocks, and other function-level
/// state during LLVM IR generation.
pub struct FunctionCodegen<'ctx, 'm> {
    /// Function name.
    name: String,

    /// Virtual register allocator.
    vreg_allocator: VirtualRegisterAllocator,

    /// Map from MIR register IDs to typed LLVM values.
    register_values: HashMap<RegId, TypedValue<'ctx>>,

    /// Map from block labels to LLVM basic blocks.
    blocks: HashMap<String, BasicBlock<'ctx>>,

    /// Current block being generated.
    current_block: Option<BasicBlock<'ctx>>,

    /// LLVM context.
    llvm_context: &'ctx Context,

    /// LLVM module.
    llvm_module: &'m Module<'ctx>,
}

impl<'ctx, 'm> FunctionCodegen<'ctx, 'm> {
    /// Create a new function codegen context.
    pub fn new(
        name: String,
        llvm_context: &'ctx Context,
        llvm_module: &'m Module<'ctx>,
    ) -> Self {
        Self {
            name,
            vreg_allocator: VirtualRegisterAllocator::new(),
            register_values: HashMap::new(),
            blocks: HashMap::new(),
            current_block: None,
            llvm_context,
            llvm_module,
        }
    }

    /// Get the function name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the LLVM context.
    pub fn llvm_context(&self) -> &'ctx Context {
        self.llvm_context
    }

    /// Get the LLVM module.
    pub fn llvm_module(&self) -> &'m Module<'ctx> {
        self.llvm_module
    }

    /// Set a register value.
    pub fn set_register(&mut self, reg: RegId, value: TypedValue<'ctx>) {
        self.register_values.insert(reg, value);
    }

    /// Get a register value.
    pub fn get_register(&self, reg: RegId) -> Option<&TypedValue<'ctx>> {
        self.register_values.get(&reg)
    }

    /// Get a register value or return a default (nil).
    pub fn get_register_or_nil(&self, reg: RegId, constant_table: &ConstantTable<'ctx, 'm>) -> TypedValue<'ctx> {
        self.get_register(reg)
            .cloned()
            .unwrap_or_else(|| {
                TypedValue::new(
                    constant_table.get_nil(),
                    RubyType::Nil,
                    None,
                )
            })
    }

    /// Check if a register has a value.
    pub fn has_register(&self, reg: RegId) -> bool {
        self.register_values.contains_key(&reg)
    }

    /// Get or create a basic block.
    pub fn get_or_create_block(
        &mut self,
        name: &str,
        function: inkwell::values::FunctionValue<'ctx>,
    ) -> BasicBlock<'ctx> {
        if let Some(&block) = self.blocks.get(name) {
            block
        } else {
            let block = self.llvm_context.append_basic_block(function, name);
            self.blocks.insert(name.to_string(), block);
            block
        }
    }

    /// Get a block by name.
    pub fn get_block(&self, name: &str) -> Option<BasicBlock<'ctx>> {
        self.blocks.get(name).copied()
    }

    /// Set the current block.
    pub fn set_current_block(&mut self, block: BasicBlock<'ctx>) {
        self.current_block = Some(block);
    }

    /// Get the current block.
    pub fn current_block(&self) -> Option<BasicBlock<'ctx>> {
        self.current_block
    }

    /// Get the virtual register allocator.
    pub fn vreg_allocator(&mut self) -> &mut VirtualRegisterAllocator {
        &mut self.vreg_allocator
    }

    /// Record a register definition.
    pub fn record_register_def(
        &mut self,
        reg: RegId,
        ty: RubyType,
        block_idx: u32,
        inst_idx: u32,
    ) {
        let is_param = self.vreg_allocator.contains(reg);
        self.vreg_allocator.allocate(reg, ty, InstIndex::new(block_idx, inst_idx), is_param);
    }

    /// Record a register use.
    pub fn record_register_use(&mut self, reg: RegId, block_idx: u32, inst_idx: u32) {
        self.vreg_allocator.record_use(reg, InstIndex::new(block_idx, inst_idx));
    }
}

impl<'ctx, 'm> TypeProvider for FunctionCodegen<'ctx, 'm> {
    fn get_type(&self, reg_id: u32) -> Option<RubyType> {
        self.register_values.get(&reg_id).map(|v| v.ruby_type())
    }
}

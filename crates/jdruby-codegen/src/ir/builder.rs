//! IR Builder for LLVM Code Generation
//!
//! Wraps Inkwell's Builder with Ruby-specific operations.

use crate::ir::{TypedValue, RubyType};
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::values::IntValue;

/// IR Builder wraps Inkwell's Builder with Ruby-specific operations.
///
/// This provides a higher-level interface for generating LLVM IR
/// that understands Ruby semantics like tagged Fixnums, type guards,
/// and runtime calls.
pub struct IrBuilder<'ctx> {
    builder: Builder<'ctx>,
}

impl<'ctx> IrBuilder<'ctx> {
    /// Create a new IR builder.
    pub fn new(builder: Builder<'ctx>) -> Self {
        Self { builder }
    }

    /// Get the underlying Inkwell builder.
    pub fn inner(&self) -> &Builder<'ctx> {
        &self.builder
    }

    /// Build a Ruby tagged integer (Fixnum).
    ///
    /// Ruby Fixnums are tagged: `(value << 1) | 1`
    pub fn build_fixnum(&self, ctx: &'ctx Context, value: i64) -> TypedValue<'ctx> {
        let tagged = (value << 1) | 1;
        let llvm_val = ctx.i64_type().const_int(tagged as u64, false);
        TypedValue::new(llvm_val.into(), RubyType::Integer, None)
    }

    /// Build an untag operation for a Fixnum.
    ///
    /// Converts a tagged Fixnum to its raw integer value: `value >> 1`
    pub fn build_untag_fixnum(
        &self,
        ctx: &'ctx Context,
        value: IntValue<'ctx>,
    ) -> IntValue<'ctx> {
        let one = ctx.i64_type().const_int(1, false);
        self.builder
            .build_right_shift(value, one, false, "untag")
            .expect("Failed to build untag")
    }

    /// Build a tag operation for a Fixnum.
    ///
    /// Converts a raw integer to a tagged Fixnum: `(value << 1) | 1`
    pub fn build_tag_fixnum(
        &self,
        ctx: &'ctx Context,
        value: IntValue<'ctx>,
    ) -> IntValue<'ctx> {
        let one = ctx.i64_type().const_int(1, false);
        let shifted = self.builder
            .build_left_shift(value, one, "tag_shift")
            .expect("Failed to build tag shift");
        self.builder
            .build_or(shifted, one, "tag_or")
            .expect("Failed to build tag or")
    }

    /// Build a type check for a Fixnum.
    ///
    /// Checks if the LSB is 1: `(value & 1) == 1`
    pub fn build_is_fixnum(
        &self,
        ctx: &'ctx Context,
        value: IntValue<'ctx>,
    ) -> inkwell::values::IntValue<'ctx> {
        let one = ctx.i64_type().const_int(1, false);
        let tag = self.builder
            .build_and(value, one, "tag")
            .expect("Failed to build tag extract");
        self.builder
            .build_int_compare(
                inkwell::IntPredicate::EQ,
                tag,
                one,
                "is_fixnum",
            )
            .expect("Failed to build comparison")
    }

    /// Build an integer addition with overflow checking.
    pub fn build_int_add(
        &self,
        left: IntValue<'ctx>,
        right: IntValue<'ctx>,
        name: &str,
    ) -> IntValue<'ctx> {
        self.builder
            .build_int_add(left, right, name)
            .expect("Failed to build add")
    }

    /// Build an integer subtraction.
    pub fn build_int_sub(
        &self,
        left: IntValue<'ctx>,
        right: IntValue<'ctx>,
        name: &str,
    ) -> IntValue<'ctx> {
        self.builder
            .build_int_sub(left, right, name)
            .expect("Failed to build sub")
    }

    /// Build an integer multiplication.
    pub fn build_int_mul(
        &self,
        left: IntValue<'ctx>,
        right: IntValue<'ctx>,
        name: &str,
    ) -> IntValue<'ctx> {
        self.builder
            .build_int_mul(left, right, name)
            .expect("Failed to build mul")
    }

    /// Build a conditional branch.
    pub fn build_conditional_branch(
        &self,
        condition: IntValue<'ctx>,
        then_block: inkwell::basic_block::BasicBlock<'ctx>,
        else_block: inkwell::basic_block::BasicBlock<'ctx>,
    ) {
        self.builder
            .build_conditional_branch(condition, then_block, else_block)
            .expect("Failed to build conditional branch");
    }

    /// Build an unconditional branch.
    pub fn build_unconditional_branch(
        &self,
        block: inkwell::basic_block::BasicBlock<'ctx>,
    ) {
        self.builder
            .build_unconditional_branch(block)
            .expect("Failed to build unconditional branch");
    }

    /// Get the current insertion block.
    pub fn get_insert_block(&self) -> Option<inkwell::basic_block::BasicBlock<'ctx>> {
        self.builder.get_insert_block()
    }

    /// Position the builder at the end of a block.
    pub fn position_at_end(&self, block: inkwell::basic_block::BasicBlock<'ctx>) {
        self.builder.position_at_end(block);
    }
}

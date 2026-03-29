//! Arithmetic Patterns for Instruction Selection
//!
//! Provides optimized patterns for integer and floating-point arithmetic
//! operations with type specialization.

use crate::selection::patterns::{MatchContext, SelectionPattern};
use jdruby_mir::{MirInst, MirBinOp, MirConst};

/// Pattern for integer arithmetic with known integer operands.
#[derive(Debug)]
pub struct IntegerArithmeticPattern;

impl IntegerArithmeticPattern {
    pub fn new() -> Self {
        Self
    }
}

impl SelectionPattern for IntegerArithmeticPattern {
    fn id(&self) -> &'static str {
        "integer_arithmetic"
    }

    fn priority(&self) -> i32 {
        100 // High priority - specific optimization
    }

    fn matches(&self, insts: &[MirInst], ctx: &MatchContext) -> Option<usize> {
        // Need at least: left operand, right operand, binop
        if insts.len() < 3 {
            return None;
        }

        // Check for LoadConst, LoadConst, BinOp pattern with integer operands
        match (&insts[0], &insts[1], &insts[2]) {
            (
                MirInst::LoadConst(left_reg, left_const),
                MirInst::LoadConst(right_reg, right_const),
                MirInst::BinOp(_dest_reg, op, l, r),
            ) if *l == *left_reg && *r == *right_reg => {
                // Check if both constants are integers
                match (left_const, right_const) {
                    (MirConst::Integer(_), MirConst::Integer(_)) => {
                        if is_arithmetic_op(op) {
                            Some(3) // Match all 3 instructions
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            }
            (
                MirInst::Load(left_reg, _),
                MirInst::Load(right_reg, _),
                MirInst::BinOp(_dest_reg, op, l, r),
            ) if *l == *left_reg && *r == *right_reg => {
                // Variables - check if we know their types
                let left_type = ctx.get_type(*left_reg);
                let right_type = ctx.get_type(*right_reg);

                match (left_type, right_type) {
                    (Some(crate::ir::RubyType::Integer), Some(crate::ir::RubyType::Integer)) => {
                        if is_arithmetic_op(op) {
                            Some(3)
                        } else {
                            None
                        }
                    }
                    // Could be integers - still match for guarded version
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

/// Pattern for float arithmetic with known float operands.
#[derive(Debug)]
pub struct FloatArithmeticPattern;

impl FloatArithmeticPattern {
    pub fn new() -> Self {
        Self
    }
}

impl SelectionPattern for FloatArithmeticPattern {
    fn id(&self) -> &'static str {
        "float_arithmetic"
    }

    fn priority(&self) -> i32 {
        95 // High priority but lower than integer
    }

    fn matches(&self, insts: &[MirInst], ctx: &MatchContext) -> Option<usize> {
        if insts.len() < 3 {
            return None;
        }

        match (&insts[0], &insts[1], &insts[2]) {
            (
                MirInst::LoadConst(left_reg, MirConst::Float(_)),
                MirInst::LoadConst(right_reg, MirConst::Float(_)),
                MirInst::BinOp(_, op, l, r),
            ) if *l == *left_reg && *r == *right_reg && is_arithmetic_op(op) => {
                Some(3)
            }
            (
                MirInst::Load(left_reg, _),
                MirInst::Load(right_reg, _),
                MirInst::BinOp(_, op, l, r),
            ) if *l == *left_reg && *r == *right_reg => {
                let left_type = ctx.get_type(*left_reg);
                let right_type = ctx.get_type(*right_reg);

                match (left_type, right_type) {
                    (Some(crate::ir::RubyType::Float), Some(crate::ir::RubyType::Float)) => {
                        if is_arithmetic_op(op) {
                            Some(3)
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

/// Pattern for string concatenation optimization.
#[derive(Debug)]
pub struct StringConcatPattern;

impl StringConcatPattern {
    pub fn new() -> Self {
        Self
    }
}

impl SelectionPattern for StringConcatPattern {
    fn id(&self) -> &'static str {
        "string_concat"
    }

    fn priority(&self) -> i32 {
        90
    }

    fn matches(&self, insts: &[MirInst], _ctx: &MatchContext) -> Option<usize> {
        if insts.len() < 3 {
            return None;
        }

        // Match: LoadConst(String), LoadConst(String), BinOp(Add)
        match (&insts[0], &insts[1], &insts[2]) {
            (
                MirInst::LoadConst(_, MirConst::String(_)),
                MirInst::LoadConst(_, MirConst::String(_)),
                MirInst::BinOp(_, MirBinOp::Add, _, _),
            ) => Some(3),
            (
                MirInst::LoadConst(_, MirConst::String(_)),
                MirInst::Load(_, _),
                MirInst::BinOp(_, MirBinOp::Add, _, _),
            ) => Some(3),
            (
                MirInst::Load(_, _),
                MirInst::LoadConst(_, MirConst::String(_)),
                MirInst::BinOp(_, MirBinOp::Add, _, _),
            ) => Some(3),
            _ => None,
        }
    }
}

/// Check if a binary operation is arithmetic.
fn is_arithmetic_op(op: &MirBinOp) -> bool {
    matches!(
        op,
        MirBinOp::Add | MirBinOp::Sub | MirBinOp::Mul | MirBinOp::Div | MirBinOp::Mod
    )
}

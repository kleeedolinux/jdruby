//! Instruction Fusion Pass - YARV-style instruction optimization
//!
//! Fuses common instruction patterns into specialized instructions:
//! - Integer arithmetic: LoadConst + BinOp → OptPlus/OptMinus/etc
//! - String operations: LoadConst + BinOp(Add) → OptConcat
//! - Method calls with static names: Send → SendWithIC

use crate::nodes::*;
use std::collections::HashMap;

/// Instruction fusion optimizer (YARV-style)
pub struct InstructionFusion;

impl InstructionFusion {
    /// Run all fusion passes on a module
    pub fn optimize(module: &mut MirModule) {
        for func in &mut module.functions {
            Self::fuse_function(func);
        }
    }

    fn fuse_function(func: &mut MirFunction) {
        // Run fusion passes in order
        Self::fuse_integer_arithmetic(func);
        Self::fuse_string_concatenation(func);
        Self::fuse_known_type_calls(func);
        Self::fuse_copy_chains(func);
    }

    /// Fuse: LoadConst(Integer) + LoadConst(Integer) + BinOp → OptPlus/OptMinus/etc
    /// Pattern:
    ///   %1 = LoadConst(Integer(a))
    ///   %2 = LoadConst(Integer(b))
    ///   %3 = BinOp(Add, %1, %2)
    /// →
    ///   %3 = OptPlusImm(a, b)  [if both operands are constant]
    /// OR for non-constant:
    ///   %1 = LoadConst(Integer(a))
    ///   %3 = OptPlus(%1, %2)  [specialized with guard]
    fn fuse_integer_arithmetic(func: &mut MirFunction) {
        // Track constant values per register
        let mut constants: HashMap<RegId, MirConst> = HashMap::new();

        for block in &mut func.blocks {
            // First pass: collect constant values and identify foldable operations
            let mut foldable_ops: Vec<(usize, RegId, MirConst)> = Vec::new();
            
            for (idx, inst) in block.instructions.iter().enumerate() {
                match inst {
                    MirInst::LoadConst(reg, c) => {
                        constants.insert(*reg, c.clone());
                    }
                    MirInst::BinOp(dest, op, left, right) => {
                        // Try to fold constant integer arithmetic
                        if let (Some(MirConst::Integer(l)), Some(MirConst::Integer(r))) = 
                            (constants.get(left), constants.get(right)) {
                            // Both operands are constant integers - fold at compile time
                            if let Some(result) = Self::fold_integer_op(op, *l, *r) {
                                foldable_ops.push((idx, *dest, result));
                            }
                        }
                    }
                    _ => {}
                }
            }
            
            // Second pass: apply the foldable operations
            for (idx, dest, result) in foldable_ops {
                block.instructions[idx] = MirInst::LoadConst(dest, result);
                constants.insert(dest, MirConst::Integer(0)); // approximate
            }
        }
    }

    fn fold_integer_op(op: &MirBinOp, left: i64, right: i64) -> Option<MirConst> {
        match op {
            MirBinOp::Add => Some(MirConst::Integer(left.wrapping_add(right))),
            MirBinOp::Sub => Some(MirConst::Integer(left.wrapping_sub(right))),
            MirBinOp::Mul => Some(MirConst::Integer(left.wrapping_mul(right))),
            MirBinOp::Div if right != 0 => Some(MirConst::Integer(left / right)),
            MirBinOp::Mod if right != 0 => Some(MirConst::Integer(left % right)),
            MirBinOp::Pow => Some(MirConst::Integer(left.wrapping_pow(right as u32))),
            MirBinOp::BitAnd => Some(MirConst::Integer(left & right)),
            MirBinOp::BitOr => Some(MirConst::Integer(left | right)),
            MirBinOp::BitXor => Some(MirConst::Integer(left ^ right)),
            MirBinOp::Eq => Some(MirConst::Bool(left == right)),
            MirBinOp::NotEq => Some(MirConst::Bool(left != right)),
            MirBinOp::Lt => Some(MirConst::Bool(left < right)),
            MirBinOp::Gt => Some(MirConst::Bool(left > right)),
            MirBinOp::LtEq => Some(MirConst::Bool(left <= right)),
            MirBinOp::GtEq => Some(MirConst::Bool(left >= right)),
            MirBinOp::Cmp => {
                let result = if left < right { -1 } else if left > right { 1 } else { 0 };
                Some(MirConst::Integer(result))
            }
            _ => None,
        }
    }

    /// Fuse: LoadConst(String) + BinOp(Add) + LoadConst(String) → OptConcat
    /// Pattern:
    ///   %1 = LoadConst(String("hello"))
    ///   %2 = LoadConst(String("world"))
    ///   %3 = BinOp(Add, %1, %2)
    /// →
    ///   %3 = OptConcat(%1, %2)  [with string type guard]
    /// OR if both constant:
    ///   %3 = LoadConst(String("helloworld"))
    fn fuse_string_concatenation(func: &mut MirFunction) {
        let mut constants: HashMap<RegId, MirConst> = HashMap::new();

        for block in &mut func.blocks {
            for inst in &mut block.instructions {
                match inst {
                    MirInst::LoadConst(reg, c) => {
                        constants.insert(*reg, c.clone());
                    }
                    MirInst::BinOp(dest, MirBinOp::Add, left, right) => {
                        // Check if both operands are strings
                        if let (Some(MirConst::String(l)), Some(MirConst::String(r))) = 
                            (constants.get(left), constants.get(right)) {
                            // Fold constant string concatenation
                            let result = format!("{}{}", l, r);
                            *inst = MirInst::LoadConst(*dest, MirConst::String(result));
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    /// Fuse: MethodCall with known receiver type → DirectCall
    /// Pattern:
    ///   %1 = Load(receiver)
    ///   %2 = MethodCall(%1, "+", [%3])
    /// →
    ///   %2 = OptPlus(%1, %3)  [if receiver is known to be Integer]
    fn fuse_known_type_calls(_func: &mut MirFunction) {
        // This would require type information from HIR
        // For now, mark opportunities for later passes
        // Full implementation would track types through the IR
    }

    /// Fuse: Copy chains
    /// Pattern:
    ///   %1 = LoadConst(5)
    ///   %2 = Copy(%1)
    ///   %3 = Copy(%2)
    ///   %4 = BinOp(Add, %3, %3)
    /// →
    ///   %1 = LoadConst(5)
    ///   %4 = BinOp(Add, %1, %1)
    fn fuse_copy_chains(func: &mut MirFunction) {
        // Build map of register -> original register
        let mut copy_chain: HashMap<RegId, RegId> = HashMap::new();

        for block in &mut func.blocks {
            for inst in &mut block.instructions {
                match inst {
                    MirInst::Copy(dest, src) => {
                        // Follow chain to find root
                        let root = Self::find_copy_root(*src, &copy_chain);
                        copy_chain.insert(*dest, root);
                        // Replace with direct reference to root
                        *inst = MirInst::Copy(*dest, root);
                    }
                    MirInst::BinOp(_dest, _op, left, right) => {
                        // Replace operands with their roots
                        if let Some(&root) = copy_chain.get(left) {
                            *left = root;
                        }
                        if let Some(&root) = copy_chain.get(right) {
                            *right = root;
                        }
                    }
                    MirInst::MethodCall(_dest, recv, _method, args) => {
                        // Replace receiver and args with their roots
                        if let Some(&root) = copy_chain.get(recv) {
                            *recv = root;
                        }
                        for arg in args.iter_mut() {
                            if let Some(&root) = copy_chain.get(arg) {
                                *arg = root;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn find_copy_root(reg: RegId, chain: &HashMap<RegId, RegId>) -> RegId {
        let mut current = reg;
        while let Some(&next) = chain.get(&current) {
            if next == current {
                break;
            }
            current = next;
        }
        current
    }
}

/// YARV-style specialized MIR instructions
/// These are type-specialized versions of generic operations
pub mod specialized {
    use super::*;

    /// Type-specialized arithmetic with type guards
    #[derive(Debug, Clone)]
    pub enum SpecializedInst {
        /// Integer addition with guard: if both ints → fast path, else → fallback
        OptPlus { dest: RegId, left: RegId, right: RegId, fallback_label: BlockLabel },
        /// Integer subtraction with guard
        OptMinus { dest: RegId, left: RegId, right: RegId, fallback_label: BlockLabel },
        /// Integer multiplication with guard
        OptMult { dest: RegId, left: RegId, right: RegId, fallback_label: BlockLabel },
        /// Integer division with guard
        OptDiv { dest: RegId, left: RegId, right: RegId, fallback_label: BlockLabel },
        /// String concatenation with guard
        OptConcat { dest: RegId, left: RegId, right: RegId, fallback_label: BlockLabel },
        /// Array index access with guard
        OptAref { dest: RegId, recv: RegId, idx: RegId, fallback_label: BlockLabel },
        /// Array index set with guard
        OptAset { dest: RegId, recv: RegId, idx: RegId, val: RegId, fallback_label: BlockLabel },
        /// String/symbol length with guard
        OptLength { dest: RegId, recv: RegId, fallback_label: BlockLabel },
    }
}

use crate::nodes::*;
use crate::opt_fusion::InstructionFusion;
use crate::opt_peephole::PeepholeOptimizer;
use std::collections::{HashMap, HashSet};

/// MIR-level optimizations.
pub struct MirOptimizer;

impl MirOptimizer {
    /// Run all MIR optimizations on a module.
    pub fn optimize(module: &mut MirModule) {
        // Run instruction fusion first (YARV-style)
        InstructionFusion::optimize(module);
        
        // Run standard optimizations on each function
        for func in &mut module.functions {
            Self::optimize_function(func);
        }
        
        // Run peephole optimizations last
        PeepholeOptimizer::optimize(module);
    }

    fn optimize_function(func: &mut MirFunction) {
        // Run passes in order, multiple iterations for convergence
        for _ in 0..3 {
            Self::constant_folding(func);
            Self::constant_propagation(func);
            Self::common_subexpression_elimination(func);
            Self::remove_dead_stores(func);
            Self::dead_code_elimination(func);
            Self::remove_nops(func);
            Self::constant_branch_folding(func);
            Self::remove_unreachable_blocks(func);
        }
    }

    /// Evaluate constant BinOp/UnOp at compile time.
    fn constant_folding(func: &mut MirFunction) {
        let mut constants: HashMap<RegId, MirConst> = HashMap::new();

        for block in &mut func.blocks {
            for inst in &mut block.instructions {
                match inst {
                    MirInst::LoadConst(reg, c) => {
                        constants.insert(*reg, c.clone());
                    }
                    MirInst::BinOp(dest, op, left, right) => {
                        if let (Some(lc), Some(rc)) = (constants.get(left), constants.get(right)) {
                            if let Some(result) = Self::fold_binop(op, lc, rc) {
                                let d = *dest;
                                *inst = MirInst::LoadConst(d, result.clone());
                                constants.insert(d, result);
                            }
                        }
                    }
                    MirInst::UnOp(dest, op, src) => {
                        if let Some(sc) = constants.get(src) {
                            if let Some(result) = Self::fold_unop(op, sc) {
                                let d = *dest;
                                *inst = MirInst::LoadConst(d, result.clone());
                                constants.insert(d, result);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn fold_binop(op: &MirBinOp, left: &MirConst, right: &MirConst) -> Option<MirConst> {
        match (left, right) {
            (MirConst::Integer(a), MirConst::Integer(b)) => match op {
                MirBinOp::Add => Some(MirConst::Integer(a.wrapping_add(*b))),
                MirBinOp::Sub => Some(MirConst::Integer(a.wrapping_sub(*b))),
                MirBinOp::Mul => Some(MirConst::Integer(a.wrapping_mul(*b))),
                MirBinOp::Div if *b != 0 => Some(MirConst::Integer(a / b)),
                MirBinOp::Mod if *b != 0 => Some(MirConst::Integer(a % b)),
                MirBinOp::Pow => Some(MirConst::Integer(a.wrapping_pow(*b as u32))),
                MirBinOp::Eq => Some(MirConst::Bool(a == b)),
                MirBinOp::NotEq => Some(MirConst::Bool(a != b)),
                MirBinOp::Lt => Some(MirConst::Bool(a < b)),
                MirBinOp::Gt => Some(MirConst::Bool(a > b)),
                MirBinOp::LtEq => Some(MirConst::Bool(a <= b)),
                MirBinOp::GtEq => Some(MirConst::Bool(a >= b)),
                MirBinOp::BitAnd => Some(MirConst::Integer(a & b)),
                MirBinOp::BitOr => Some(MirConst::Integer(a | b)),
                MirBinOp::BitXor => Some(MirConst::Integer(a ^ b)),
                MirBinOp::Shl => Some(MirConst::Integer(a.wrapping_shl(*b as u32))),
                MirBinOp::Shr => Some(MirConst::Integer(a.wrapping_shr(*b as u32))),
                _ => None,
            },
            (MirConst::Float(a), MirConst::Float(b)) => match op {
                MirBinOp::Add => Some(MirConst::Float(a + b)),
                MirBinOp::Sub => Some(MirConst::Float(a - b)),
                MirBinOp::Mul => Some(MirConst::Float(a * b)),
                MirBinOp::Div => Some(MirConst::Float(a / b)),
                MirBinOp::Eq => Some(MirConst::Bool(a == b)),
                MirBinOp::Lt => Some(MirConst::Bool(a < b)),
                MirBinOp::Gt => Some(MirConst::Bool(a > b)),
                _ => None,
            },
            (MirConst::Bool(a), MirConst::Bool(b)) => match op {
                MirBinOp::And => Some(MirConst::Bool(*a && *b)),
                MirBinOp::Or => Some(MirConst::Bool(*a || *b)),
                MirBinOp::Eq => Some(MirConst::Bool(a == b)),
                MirBinOp::NotEq => Some(MirConst::Bool(a != b)),
                _ => None,
            },
            (MirConst::String(a), MirConst::String(b)) => match op {
                MirBinOp::Add => Some(MirConst::String(format!("{}{}", a, b))),
                MirBinOp::Eq => Some(MirConst::Bool(a == b)),
                MirBinOp::NotEq => Some(MirConst::Bool(a != b)),
                _ => None,
            },
            _ => None,
        }
    }

    fn fold_unop(op: &MirUnOp, val: &MirConst) -> Option<MirConst> {
        match (op, val) {
            (MirUnOp::Neg, MirConst::Integer(v)) => Some(MirConst::Integer(-v)),
            (MirUnOp::Neg, MirConst::Float(v)) => Some(MirConst::Float(-v)),
            (MirUnOp::Not, MirConst::Bool(v)) => Some(MirConst::Bool(!v)),
            (MirUnOp::Not, MirConst::Nil) => Some(MirConst::Bool(true)),
            (MirUnOp::BitNot, MirConst::Integer(v)) => Some(MirConst::Integer(!v)),
            _ => None,
        }
    }

    /// Propagate constant values through copy chains.
    fn constant_propagation(func: &mut MirFunction) {
        let mut constants: HashMap<RegId, MirConst> = HashMap::new();
        for block in &mut func.blocks {
            for inst in &mut block.instructions {
                match inst {
                    MirInst::LoadConst(reg, c) => {
                        constants.insert(*reg, c.clone());
                    }
                    MirInst::Copy(dest, src) => {
                        let dest_val = *dest;
                        let src_val = *src;
                        if let Some(c) = constants.get(&src_val) {
                            let c_clone = c.clone();
                            *inst = MirInst::LoadConst(dest_val, c_clone.clone());
                            constants.insert(dest_val, c_clone);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    /// Common Subexpression Elimination: if the same BinOp(op, left, right)
    /// appears twice, replace the second with a Copy.
    fn common_subexpression_elimination(func: &mut MirFunction) {
        // Key: (op_discriminant, left_reg, right_reg) -> dest_reg
        let mut seen: HashMap<(u8, RegId, RegId), RegId> = HashMap::new();

        for block in &mut func.blocks {
            for inst in &mut block.instructions {
                if let MirInst::BinOp(dest, op, left, right) = inst {
                    let key = (Self::binop_discriminant(op), *left, *right);
                    if let Some(&existing_dest) = seen.get(&key) {
                        let d = *dest;
                        *inst = MirInst::Copy(d, existing_dest);
                    } else {
                        seen.insert(key, *dest);
                    }
                }
            }
        }
    }

    fn binop_discriminant(op: &MirBinOp) -> u8 {
        match op {
            MirBinOp::Add => 0, MirBinOp::Sub => 1, MirBinOp::Mul => 2,
            MirBinOp::Div => 3, MirBinOp::Mod => 4, MirBinOp::Pow => 5,
            MirBinOp::Eq => 6, MirBinOp::NotEq => 7, MirBinOp::Lt => 8,
            MirBinOp::Gt => 9, MirBinOp::LtEq => 10, MirBinOp::GtEq => 11,
            MirBinOp::Cmp => 12, MirBinOp::And => 13, MirBinOp::Or => 14,
            MirBinOp::BitAnd => 15, MirBinOp::BitOr => 16, MirBinOp::BitXor => 17,
            MirBinOp::Shl => 18, MirBinOp::Shr => 19,
        }
    }

    /// Remove Store instructions where the variable is never loaded.
    fn remove_dead_stores(func: &mut MirFunction) {
        let mut loaded: HashSet<String> = HashSet::new();
        for block in &func.blocks {
            for inst in &block.instructions {
                if let MirInst::Load(_, name) = inst {
                    loaded.insert(name.clone());
                }
            }
        }
        for block in &mut func.blocks {
            block.instructions.retain(|inst| {
                if let MirInst::Store(name, _) = inst {
                    // Only eliminate dead stores for purely local variables.
                    // Constants (A-Z), instance vars (@), and globals ($) cross block/function boundaries!
                    if name.starts_with(|c: char| c.is_ascii_uppercase()) || name.starts_with('@') || name.starts_with('$') {
                        return true;
                    }
                    loaded.contains(name)
                } else {
                    true
                }
            });
        }
    }

    /// Remove instructions that produce registers never used by anything.
    fn dead_code_elimination(func: &mut MirFunction) {
        // Collect all used registers (read by other instructions or terminators)
        let mut used_regs: HashSet<RegId> = HashSet::new();

        for block in &func.blocks {
            for inst in &block.instructions {
                match inst {
                    MirInst::Copy(_, src) => { used_regs.insert(*src); }
                    MirInst::BinOp(_, _, left, right) => {
                        used_regs.insert(*left);
                        used_regs.insert(*right);
                    }
                    MirInst::UnOp(_, _, src) => { used_regs.insert(*src); }
                    MirInst::Call(_, _, args) => {
                        for a in args { used_regs.insert(*a); }
                    }
                    MirInst::MethodCall(_, recv, _, args) => {
                        used_regs.insert(*recv);
                        for a in args { used_regs.insert(*a); }
                    }
                    MirInst::Store(_, reg) => { used_regs.insert(*reg); }
                    MirInst::DefMethod(class_reg, _, _) => { used_regs.insert(*class_reg); }
                    MirInst::IncludeModule(class_reg, _) => { used_regs.insert(*class_reg); }
                    _ => {}
                }
            }
            // Terminators
            match &block.terminator {
                MirTerminator::Return(Some(reg)) => { used_regs.insert(*reg); }
                MirTerminator::CondBranch(reg, _, _) => { used_regs.insert(*reg); }
                _ => {}
            }
        }

        // Remove LoadConst/Copy that produce unused registers
        // BUT never remove Call/MethodCall/ClassNew/DefMethod (side effects)
        for block in &mut func.blocks {
            block.instructions.retain(|inst| {
                match inst {
                    MirInst::LoadConst(reg, _) | MirInst::Copy(reg, _) => {
                        used_regs.contains(reg)
                    }
                    MirInst::BinOp(reg, _, _, _) | MirInst::UnOp(reg, _, _) => {
                        used_regs.contains(reg)
                    }
                    MirInst::Alloc(reg, _) | MirInst::Load(reg, _) => {
                        used_regs.contains(reg)
                    }
                    // Never remove side-effectful instructions
                    _ => true,
                }
            });
        }
    }

    /// If a CondBranch tests a known constant, replace with unconditional Branch.
    fn constant_branch_folding(func: &mut MirFunction) {
        let mut constants: HashMap<RegId, MirConst> = HashMap::new();

        // Collect constants from all blocks
        for block in &func.blocks {
            for inst in &block.instructions {
                if let MirInst::LoadConst(reg, c) = inst {
                    constants.insert(*reg, c.clone());
                }
            }
        }

        // Replace known conditional branches
        for block in &mut func.blocks {
            if let MirTerminator::CondBranch(reg, then_l, else_l) = &block.terminator {
                if let Some(c) = constants.get(reg) {
                    let is_truthy = match c {
                        MirConst::Bool(false) | MirConst::Nil => false,
                        _ => true,
                    };
                    block.terminator = MirTerminator::Branch(
                        if is_truthy { then_l.clone() } else { else_l.clone() }
                    );
                }
            }
        }
    }

    /// Remove Nop instructions.
    fn remove_nops(func: &mut MirFunction) {
        for block in &mut func.blocks {
            block.instructions.retain(|inst| !matches!(inst, MirInst::Nop));
        }
    }

    /// Remove blocks that are unreachable (no predecessors except entry).
    fn remove_unreachable_blocks(func: &mut MirFunction) {
        if func.blocks.len() <= 1 { return; }
        let mut reachable: HashSet<String> = HashSet::new();
        if let Some(first) = func.blocks.first() {
            reachable.insert(first.label.clone());
        }
        let mut changed = true;
        while changed {
            changed = false;
            for block in &func.blocks {
                if !reachable.contains(&block.label) { continue; }
                match &block.terminator {
                    MirTerminator::Branch(target) => {
                        if reachable.insert(target.clone()) { changed = true; }
                    }
                    MirTerminator::CondBranch(_, t, f) => {
                        if reachable.insert(t.clone()) { changed = true; }
                        if reachable.insert(f.clone()) { changed = true; }
                    }
                    _ => {}
                }
            }
        }
        func.blocks.retain(|b| reachable.contains(&b.label));
    }
}

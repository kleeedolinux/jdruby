//! Peephole Optimization Pass for MIR
//!
//! Performs local pattern matching and replacement on instruction sequences.
//! These are simple, local optimizations that don't require global analysis.

use crate::nodes::*;
use std::collections::HashMap;

/// Peephole optimizer for MIR - performs local pattern matching
pub struct PeepholeOptimizer;

impl PeepholeOptimizer {
    /// Run peephole optimizations on a module
    pub fn optimize(module: &mut MirModule) {
        for func in &mut module.functions {
            Self::optimize_function(func);
        }
    }

    fn optimize_function(func: &mut MirFunction) {
        // Run passes until no more changes
        let mut changed = true;
        while changed {
            changed = false;
            changed |= Self::eliminate_copy_chains(func);
            changed |= Self::eliminate_redundant_loads(func);
            changed |= Self::eliminate_dead_branches_to_fallthrough(func);
            changed |= Self::simplify_identity_ops(func);
        }
    }

    /// Eliminate copy chains: %2 = Copy(%1), %3 = Copy(%2) → %3 = Copy(%1)
    fn eliminate_copy_chains(func: &mut MirFunction) -> bool {
        let mut changed = false;
        let mut copy_map: HashMap<RegId, RegId> = HashMap::new();

        // Build map of register -> ultimate source
        for block in &func.blocks {
            for inst in &block.instructions {
                if let MirInst::Copy(dest, src) = inst {
                    let mut root = *src;
                    while let Some(&next) = copy_map.get(&root) {
                        if next == root { break; }
                        root = next;
                    }
                    copy_map.insert(*dest, root);
                }
            }
        }

        // Replace all copy chains with direct copies
        for block in &mut func.blocks {
            for inst in &mut block.instructions {
                if let MirInst::Copy(dest, src) = inst {
                    if let Some(&root) = copy_map.get(src) {
                        if root != *src {
                            *inst = MirInst::Copy(*dest, root);
                            changed = true;
                        }
                    }
                }
            }
        }

        changed
    }

    /// Eliminate redundant loads: Load after Store of same variable
    fn eliminate_redundant_loads(func: &mut MirFunction) -> bool {
        let mut changed = false;

        for block in &mut func.blocks {
            let mut last_store: HashMap<String, RegId> = HashMap::new();

            for inst in &mut block.instructions {
                match inst {
                    MirInst::Store(name, reg) => {
                        last_store.insert(name.clone(), *reg);
                    }
                    MirInst::Load(dest, name) => {
                        if let Some(&src) = last_store.get(name) {
                            *inst = MirInst::Copy(*dest, src);
                            changed = true;
                        }
                    }
                    MirInst::Call(_, _, _) | MirInst::MethodCall(_, _, _, _, _) => {
                        last_store.clear();
                    }
                    _ => {}
                }
            }
        }

        changed
    }

    /// Eliminate dead branches to fallthrough
    fn eliminate_dead_branches_to_fallthrough(func: &mut MirFunction) -> bool {
        let mut changed = false;
        let block_labels: Vec<String> = func.blocks.iter().map(|b| b.label.clone()).collect();

        for i in 0..func.blocks.len() {
            if let MirTerminator::Branch(target) = &func.blocks[i].terminator {
                if i + 1 < block_labels.len() && *target == block_labels[i + 1] {
                    func.blocks[i].terminator = MirTerminator::Unreachable;
                    changed = true;
                }
            }
        }

        changed
    }

    /// Simplify identity operations
    fn simplify_identity_ops(func: &mut MirFunction) -> bool {
        let mut changed = false;
        let mut constants: HashMap<RegId, MirConst> = HashMap::new();

        for block in &mut func.blocks {
            for inst in &mut block.instructions {
                if let MirInst::LoadConst(reg, c) = inst {
                    constants.insert(*reg, c.clone());
                }

                if let MirInst::BinOp(dest, op, left, right) = inst {
                    let right_const = constants.get(right);

                    let is_identity = match op {
                        MirBinOp::Add => right_const.map_or(false, |c| matches!(c, MirConst::Integer(0))),
                        MirBinOp::Sub => right_const.map_or(false, |c| matches!(c, MirConst::Integer(0))),
                        MirBinOp::Mul => right_const.map_or(false, |c| matches!(c, MirConst::Integer(1))),
                        MirBinOp::Div => right_const.map_or(false, |c| matches!(c, MirConst::Integer(1))),
                        _ => false,
                    };

                    if is_identity {
                        *inst = MirInst::Copy(*dest, *left);
                        changed = true;
                    }
                }
            }
        }

        changed
    }
}

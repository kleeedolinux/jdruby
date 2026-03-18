use crate::nodes::*;

/// MIR-level optimizations.
pub struct MirOptimizer;

impl MirOptimizer {
    /// Run all MIR optimizations on a module.
    pub fn optimize(module: &mut MirModule) {
        for func in &mut module.functions {
            Self::optimize_function(func);
        }
    }

    fn optimize_function(func: &mut MirFunction) {
        Self::remove_dead_stores(func);
        Self::constant_propagation(func);
        Self::remove_nops(func);
        Self::remove_unreachable_blocks(func);
    }

    /// Remove Store instructions where the variable is never loaded.
    fn remove_dead_stores(func: &mut MirFunction) {
        // Collect all loaded variables
        let mut loaded: std::collections::HashSet<String> = std::collections::HashSet::new();
        for block in &func.blocks {
            for inst in &block.instructions {
                if let MirInst::Load(_, name) = inst {
                    loaded.insert(name.clone());
                }
            }
        }
        // Remove stores to variables that are never loaded (except function params)
        for block in &mut func.blocks {
            block.instructions.retain(|inst| {
                if let MirInst::Store(name, _) = inst {
                    loaded.contains(name)
                } else {
                    true
                }
            });
        }
    }

    /// Propagate constant values through copy chains.
    fn constant_propagation(func: &mut MirFunction) {
        let mut constants: std::collections::HashMap<RegId, MirConst> = std::collections::HashMap::new();
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

    /// Remove Nop instructions.
    fn remove_nops(func: &mut MirFunction) {
        for block in &mut func.blocks {
            block.instructions.retain(|inst| !matches!(inst, MirInst::Nop));
        }
    }

    /// Remove blocks that are unreachable (no predecessors except entry).
    fn remove_unreachable_blocks(func: &mut MirFunction) {
        if func.blocks.len() <= 1 { return; }
        let mut reachable: std::collections::HashSet<String> = std::collections::HashSet::new();
        if let Some(first) = func.blocks.first() {
            reachable.insert(first.label.clone());
        }
        // Follow terminators to find reachable blocks
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

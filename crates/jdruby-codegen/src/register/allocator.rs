//! Virtual Register Allocator
//!
//! Manages virtual register allocation within a function and provides
//! liveness analysis hints to LLVM's register allocator.

use super::virtual_reg::{VirtualRegister, RegId, InstIndex};
use crate::ir::types::RubyType;
use std::collections::HashMap;

/// Allocator for virtual registers within a function.
/// 
/// This tracks all virtual registers in a function, their types,
/// and their liveness information.
#[derive(Debug)]
pub struct VirtualRegisterAllocator {
    /// All allocated registers, keyed by MIR register ID.
    registers: HashMap<RegId, VirtualRegister>,
    /// Next virtual register number (for internal allocation).
    next_internal_id: u32,
}

impl VirtualRegisterAllocator {
    /// Create a new allocator.
    pub fn new() -> Self {
        Self {
            registers: HashMap::new(),
            next_internal_id: 1000, // Reserve 0-999 for MIR registers
        }
    }

    /// Allocate a virtual register for a MIR register.
    pub fn allocate(
        &mut self,
        mir_reg: RegId,
        ty: RubyType,
        defined_at: InstIndex,
        is_parameter: bool,
    ) -> &VirtualRegister {
        let vreg = VirtualRegister::new(mir_reg, ty, defined_at, is_parameter);
        self.registers.insert(mir_reg, vreg);
        self.registers.get(&mir_reg).unwrap()
    }

    /// Allocate an internal temporary register.
    pub fn allocate_temp(
        &mut self,
        ty: RubyType,
        defined_at: InstIndex,
    ) -> RegId {
        let id = self.next_internal_id;
        self.next_internal_id += 1;
        self.allocate(id, ty, defined_at, false);
        id
    }

    /// Get a register by ID.
    pub fn get(&self, id: RegId) -> Option<&VirtualRegister> {
        self.registers.get(&id)
    }

    /// Get a mutable reference to a register.
    pub fn get_mut(&mut self, id: RegId) -> Option<&mut VirtualRegister> {
        self.registers.get_mut(&id)
    }

    /// Record a use of a register.
    pub fn record_use(&mut self, id: RegId, at: InstIndex) {
        if let Some(reg) = self.registers.get_mut(&id) {
            reg.record_use(at);
        }
    }

    /// Check if a register exists.
    pub fn contains(&self, id: RegId) -> bool {
        self.registers.contains_key(&id)
    }

    /// Get all registers.
    pub fn all_registers(&self) -> &HashMap<RegId, VirtualRegister> {
        &self.registers
    }

    /// Get registers of a specific type.
    pub fn registers_of_type(&self, ty: RubyType) -> Vec<&VirtualRegister> {
        self.registers
            .values()
            .filter(|r| r.value_type() == ty)
            .collect()
    }

    /// Get the type of a register (if known).
    pub fn get_type(&self, id: RegId) -> Option<RubyType> {
        self.registers.get(&id).map(|r| r.value_type())
    }

    /// Compute liveness information for all registers.
    /// 
    /// This performs a simple liveness analysis to determine which
    /// registers are live at block boundaries.
    pub fn compute_liveness(&mut self, block_count: u32) -> LivenessResult {
        let mut result = LivenessResult::new(block_count);

        for (id, reg) in &self.registers {
            if let Some((def, last)) = reg.live_range() {
                // Register spans from definition block to last use block
                for block_id in def.block_id..=last.block_id {
                    result.mark_live_in(block_id, *id);
                }

                // If spans multiple blocks, mark as such
                if def.block_id != last.block_id {
                    result.mark_spans_blocks(*id);
                }
            }
        }

        result
    }

    /// Get registers that should be kept in physical registers.
    /// 
    /// These are heavily used or short-lived registers where spilling
    /// would be expensive.
    pub fn priority_registers(&self) -> Vec<RegId> {
        self.registers
            .values()
            .filter(|r| {
                r.is_heavily_used() || r.is_short_lived()
            })
            .map(|r| r.id())
            .collect()
    }

    /// Clear all registers (for reuse).
    pub fn clear(&mut self) {
        self.registers.clear();
        self.next_internal_id = 1000;
    }

    /// Get the count of allocated registers.
    pub fn count(&self) -> usize {
        self.registers.len()
    }
}

impl Default for VirtualRegisterAllocator {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of liveness analysis.
#[derive(Debug)]
pub struct LivenessResult {
    /// Number of blocks in the function.
    #[allow(dead_code)]
    block_count: u32,
    /// Registers live at entry to each block.
    live_in: Vec<Vec<RegId>>,
    /// Registers live at exit from each block.
    live_out: Vec<Vec<RegId>>,
    /// Registers that span multiple blocks.
    multi_block_regs: Vec<RegId>,
}

impl LivenessResult {
    fn new(block_count: u32) -> Self {
        Self {
            block_count,
            live_in: vec![vec![]; block_count as usize],
            live_out: vec![vec![]; block_count as usize],
            multi_block_regs: vec![],
        }
    }

    /// Mark a register as live at block entry.
    fn mark_live_in(&mut self, block_id: u32, reg: RegId) {
        if let Some(block_live) = self.live_in.get_mut(block_id as usize) {
            if !block_live.contains(&reg) {
                block_live.push(reg);
            }
        }
    }

    /// Mark a register as live at block exit.
    pub fn mark_live_out(&mut self, block_id: u32, reg: RegId) {
        if let Some(block_live) = self.live_out.get_mut(block_id as usize) {
            if !block_live.contains(&reg) {
                block_live.push(reg);
            }
        }
    }

    /// Mark a register as spanning multiple blocks.
    fn mark_spans_blocks(&mut self, reg: RegId) {
        if !self.multi_block_regs.contains(&reg) {
            self.multi_block_regs.push(reg);
        }
    }

    /// Get registers live at block entry.
    pub fn live_in(&self, block_id: u32) -> &[RegId] {
        self.live_in
            .get(block_id as usize)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Get registers live at block exit.
    pub fn live_out(&self, block_id: u32) -> &[RegId] {
        self.live_out
            .get(block_id as usize)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Check if a register spans multiple blocks.
    pub fn spans_blocks(&self, reg: RegId) -> bool {
        self.multi_block_regs.contains(&reg)
    }

    /// Get all registers that span blocks.
    pub fn multi_block_registers(&self) -> &[RegId] {
        &self.multi_block_regs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allocator_creation() {
        let allocator = VirtualRegisterAllocator::new();
        assert_eq!(allocator.count(), 0);
    }

    #[test]
    fn test_allocate_register() {
        let mut allocator = VirtualRegisterAllocator::new();
        let reg = allocator.allocate(0, RubyType::Integer, InstIndex::new(0, 0), false);

        assert_eq!(reg.id(), 0);
        assert_eq!(reg.value_type(), RubyType::Integer);
        assert_eq!(allocator.count(), 1);
    }

    #[test]
    fn test_allocate_temp() {
        let mut allocator = VirtualRegisterAllocator::new();
        let temp_id = allocator.allocate_temp(RubyType::Float, InstIndex::new(0, 5));

        assert_eq!(temp_id, 1000); // First internal ID
        assert_eq!(allocator.count(), 1);

        let temp_id2 = allocator.allocate_temp(RubyType::Integer, InstIndex::new(0, 6));
        assert_eq!(temp_id2, 1001);
    }

    #[test]
    fn test_record_use() {
        let mut allocator = VirtualRegisterAllocator::new();
        allocator.allocate(0, RubyType::Integer, InstIndex::new(0, 0), false);

        allocator.record_use(0, InstIndex::new(0, 5));
        allocator.record_use(0, InstIndex::new(0, 10));

        let reg = allocator.get(0).unwrap();
        assert_eq!(reg.use_count(), 2);
    }

    #[test]
    fn test_priority_registers() {
        let mut allocator = VirtualRegisterAllocator::new();
        allocator.allocate(0, RubyType::Integer, InstIndex::new(0, 0), false);

        // Record many uses
        for i in 1..=5 {
            allocator.record_use(0, InstIndex::new(0, i));
        }

        let priority = allocator.priority_registers();
        assert!(priority.contains(&0));
    }

    #[test]
    fn test_liveness_result() {
        let mut result = LivenessResult::new(3);

        result.mark_live_in(0, 1);
        result.mark_live_in(0, 2);
        result.mark_live_in(1, 2);

        assert_eq!(result.live_in(0), &[1, 2]);
        assert_eq!(result.live_in(1), &[2]);
        assert_eq!(result.live_in(2), &[]);
    }
}

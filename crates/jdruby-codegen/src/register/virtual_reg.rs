//! Virtual Register Definition
//!
//! Virtual registers represent values before physical register allocation.
//! They carry type information and liveness metadata to help LLVM's
//! register allocator make better decisions.

use crate::ir::types::{RubyType, RegisterClass};

/// Unique identifier for a register.
pub type RegId = u32;

/// A virtual register represents a value in the code generator.
/// 
/// Virtual registers are mapped to LLVM values during code generation,
/// and eventually to physical registers by LLVM's allocator.
#[derive(Debug, Clone)]
pub struct VirtualRegister {
    /// Unique register ID (from MIR).
    id: RegId,

    /// The type of value this register holds.
    value_type: RubyType,

    /// Register class (hints for LLVM's register allocator).
    reg_class: RegisterClass,

    /// Definition location (instruction index in block).
    defined_at: InstIndex,

    /// Last use location.
    last_used_at: Option<InstIndex>,

    /// Number of uses (for optimization decisions).
    use_count: u32,

    /// Is this a function parameter?
    is_parameter: bool,

    /// Liveness information.
    liveness: LivenessInfo,
}

/// Instruction index (block_id, instruction_id).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InstIndex {
    pub block_id: u32,
    pub inst_id: u32,
}

impl InstIndex {
    pub fn new(block_id: u32, inst_id: u32) -> Self {
        Self { block_id, inst_id }
    }
}

/// Liveness information for a register.
#[derive(Debug, Clone, Default)]
pub struct LivenessInfo {
    /// Live-in blocks (registers alive at block entry).
    pub live_in: Vec<u32>,
    /// Live-out blocks (registers alive at block exit).
    pub live_out: Vec<u32>,
    /// Whether this register spans multiple basic blocks.
    pub spans_blocks: bool,
}

impl VirtualRegister {
    /// Create a new virtual register.
    pub fn new(
        id: RegId,
        value_type: RubyType,
        defined_at: InstIndex,
        is_parameter: bool,
    ) -> Self {
        let reg_class = value_type.register_class();

        Self {
            id,
            value_type,
            reg_class,
            defined_at,
            last_used_at: None,
            use_count: 0,
            is_parameter,
            liveness: LivenessInfo::default(),
        }
    }

    /// Get the register ID.
    pub fn id(&self) -> RegId {
        self.id
    }

    /// Get the value type.
    pub fn value_type(&self) -> RubyType {
        self.value_type
    }

    /// Get the register class.
    pub fn register_class(&self) -> RegisterClass {
        self.reg_class
    }

    /// Check if this is a parameter.
    pub fn is_parameter(&self) -> bool {
        self.is_parameter
    }

    /// Get the definition location.
    pub fn defined_at(&self) -> InstIndex {
        self.defined_at
    }

    /// Record a use of this register.
    pub fn record_use(&mut self, at: InstIndex) {
        self.use_count += 1;
        self.last_used_at = Some(at);
    }

    /// Get the number of uses.
    pub fn use_count(&self) -> u32 {
        self.use_count
    }

    /// Get the last use location.
    pub fn last_used_at(&self) -> Option<InstIndex> {
        self.last_used_at
    }

    /// Check if this register is used.
    pub fn is_used(&self) -> bool {
        self.use_count > 0
    }

    /// Get the live range (from definition to last use).
    pub fn live_range(&self) -> Option<(InstIndex, InstIndex)> {
        self.last_used_at.map(|last| (self.defined_at, last))
    }

    /// Check if this is a short-lived register (good for keeping in register).
    pub fn is_short_lived(&self) -> bool {
        if let Some((def, last)) = self.live_range() {
            // Short if in same block and few instructions
            def.block_id == last.block_id && (last.inst_id - def.inst_id) <= 5
        } else {
            false // Parameters or unused
        }
    }

    /// Check if this register is heavily used (good for keeping in register).
    pub fn is_heavily_used(&self) -> bool {
        self.use_count > 3
    }

    /// Get allocation hints based on usage patterns.
    pub fn allocation_hints(&self) -> AllocationHints {
        let mut hints = AllocationHints::default();

        if self.is_short_lived() {
            hints.keep_in_register = true;
        }

        if self.is_heavily_used() {
            hints.keep_in_register = true;
        }

        if self.value_type.is_immediate() {
            hints.prefer_immediate_encoding = true;
        }

        hints
    }

    /// Update liveness information.
    pub fn set_liveness(&mut self, liveness: LivenessInfo) {
        self.liveness = liveness;
    }
}

/// Allocation hints for the register allocator.
#[derive(Debug, Clone, Default)]
pub struct AllocationHints {
    /// Prefer to keep this value in a register.
    pub keep_in_register: bool,
    /// Can this value be encoded as an immediate?
    pub prefer_immediate_encoding: bool,
    /// Suggested physical register (if any).
    pub suggested_reg: Option<&'static str>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_virtual_register_creation() {
        let reg = VirtualRegister::new(
            0,
            RubyType::Integer,
            InstIndex::new(0, 0),
            false,
        );

        assert_eq!(reg.id(), 0);
        assert_eq!(reg.value_type(), RubyType::Integer);
        assert!(!reg.is_parameter());
    }

    #[test]
    fn test_parameter_register() {
        let reg = VirtualRegister::new(
            1,
            RubyType::Object("Foo"),
            InstIndex::new(0, 0),
            true,
        );

        assert!(reg.is_parameter());
        assert_eq!(reg.register_class(), RegisterClass::General);
    }

    #[test]
    fn test_use_tracking() {
        let mut reg = VirtualRegister::new(
            0,
            RubyType::Integer,
            InstIndex::new(0, 0),
            false,
        );

        assert_eq!(reg.use_count(), 0);
        assert!(!reg.is_used());

        reg.record_use(InstIndex::new(0, 5));
        assert_eq!(reg.use_count(), 1);
        assert!(reg.is_used());

        reg.record_use(InstIndex::new(0, 10));
        assert_eq!(reg.use_count(), 2);
    }

    #[test]
    fn test_short_lived_detection() {
        let mut reg = VirtualRegister::new(
            0,
            RubyType::Integer,
            InstIndex::new(0, 0),
            false,
        );

        // Not short lived if never used
        assert!(!reg.is_short_lived());

        // Short lived: same block, 3 instructions
        reg.record_use(InstIndex::new(0, 3));
        assert!(reg.is_short_lived());

        // Not short lived: spans blocks
        let mut reg2 = VirtualRegister::new(
            1,
            RubyType::Integer,
            InstIndex::new(0, 0),
            false,
        );
        reg2.record_use(InstIndex::new(1, 0));
        assert!(!reg2.is_short_lived());
    }

    #[test]
    fn test_allocation_hints() {
        let mut reg = VirtualRegister::new(
            0,
            RubyType::Integer,
            InstIndex::new(0, 0),
            false,
        );

        // Record many uses
        for i in 1..=5 {
            reg.record_use(InstIndex::new(0, i));
        }

        let hints = reg.allocation_hints();
        assert!(hints.keep_in_register);
    }
}

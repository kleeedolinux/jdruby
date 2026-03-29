//! Virtual Register Management
//!
//! Provides virtual register allocation and tracking before LLVM's
//! physical register allocation. This enables better code generation
//! decisions and liveness hints.

pub mod virtual_reg;
pub mod allocator;

pub use virtual_reg::{VirtualRegister, LivenessInfo};
pub use crate::ir::types::RegisterClass;
pub use allocator::VirtualRegisterAllocator;

//! Method Call Patterns for Instruction Selection
//!
//! Provides optimized patterns for method dispatch with inline caching.

use crate::selection::patterns::{MatchContext, SelectionPattern};
use jdruby_mir::MirInst;

/// Pattern for direct method calls when receiver type is known.
#[derive(Debug)]
pub struct DirectCallPattern;

impl DirectCallPattern {
    pub fn new() -> Self {
        Self
    }
}

impl SelectionPattern for DirectCallPattern {
    fn id(&self) -> &'static str {
        "direct_call"
    }

    fn priority(&self) -> i32 {
        110 // Very high priority for known types
    }

    fn matches(&self, insts: &[MirInst], ctx: &MatchContext) -> Option<usize> {
        // Match MethodCall with known receiver type
        match insts.first() {
            Some(MirInst::MethodCall(_, obj_reg, _, _, _)) => {
                let obj_type = ctx.get_type(*obj_reg);

                // Only match if we know the exact type
                match obj_type {
                    Some(crate::ir::RubyType::Object(_)) => Some(1),
                    Some(crate::ir::RubyType::String) => Some(1),
                    Some(crate::ir::RubyType::Array) => Some(1),
                    Some(crate::ir::RubyType::Hash) => Some(1),
                    _ => None,
                }
            }
            _ => None,
        }
    }
}

/// Pattern for polymorphic method calls with inline cache.
#[derive(Debug)]
pub struct PolymorphicCallPattern;

impl PolymorphicCallPattern {
    pub fn new() -> Self {
        Self
    }
}

impl SelectionPattern for PolymorphicCallPattern {
    fn id(&self) -> &'static str {
        "polymorphic_call"
    }

    fn priority(&self) -> i32 {
        80
    }

    fn matches(&self, insts: &[MirInst], _ctx: &MatchContext) -> Option<usize> {
        // Match SendWithIC (method call with inline cache slot)
        match insts.first() {
            Some(MirInst::SendWithIC { .. }) => Some(1),
            _ => None,
        }
    }
}

/// Pattern for dynamic send with unknown method name.
#[derive(Debug)]
pub struct DynamicSendPattern;

impl DynamicSendPattern {
    pub fn new() -> Self {
        Self
    }
}

impl SelectionPattern for DynamicSendPattern {
    fn id(&self) -> &'static str {
        "dynamic_send"
    }

    fn priority(&self) -> i32 {
        50 // Low priority - fallback
    }

    fn matches(&self, insts: &[MirInst], _ctx: &MatchContext) -> Option<usize> {
        // Match PublicSend (dynamic method name)
        match insts.first() {
            Some(MirInst::PublicSend { .. }) => Some(1),
            _ => None,
        }
    }
}

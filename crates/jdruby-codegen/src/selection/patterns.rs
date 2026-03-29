//! Pattern Definition and Matching Framework
//!
//! Defines the SelectionPattern trait and pattern matching infrastructure
//! used for MIR → LLVM IR translation with optimization.

use crate::ir::RubyType;
use crate::register::virtual_reg::RegId;
use jdruby_mir::{MirInst, MirModule, MirFunction};
use std::collections::HashMap;

/// A pattern that matches a sequence of MIR instructions and can emit optimized LLVM.
///
/// This is the core abstraction for instruction selection. Patterns:
/// 1. Match sequences of MIR instructions
/// 2. Emit optimized LLVM IR for matched sequences
/// 3. Provide priority ordering for pattern selection
///
/// # Example Pattern Implementation
/// ```ignore
/// pub struct IntegerAddPattern;
///
/// impl SelectionPattern for IntegerAddPattern {
///     fn id(&self) -> &'static str { "integer_add" }
///     
///     fn priority(&self) -> i32 { 100 }
///     
///     fn matches(&self, insts: &[MirInst], ctx: &MatchContext) -> Option<usize> {
///         // Match LoadConst, LoadConst, BinOp(Add) sequence
///         if insts.len() < 3 { return None; }
///         
///         match (&insts[0], &insts[1], &insts[2]) {
///             (MirInst::LoadConst(_, _), MirInst::LoadConst(_, _), MirInst::BinOp(_, op, _, _))
///                 if *op == MirBinOp::Add => Some(3),
///             _ => None,
///         }
///     }
///     
///     fn emit<'ctx>(&self, matched: &[MirInst], ctx: &mut EmitContext<'ctx>) -> EmitResult<'ctx> {
///         // Emit optimized LLVM for the matched sequence
///         // ...
///     }
/// }
/// ```
pub trait SelectionPattern: Send + Sync + std::fmt::Debug {
    /// Unique pattern identifier.
    fn id(&self) -> &'static str;

    /// Check if this pattern matches the instruction sequence.
    ///
    /// Returns `Some(n)` if the first `n` instructions match this pattern,
    /// `None` otherwise. The pattern selector will try patterns in priority
    /// order and use the first matching pattern.
    ///
    /// # Arguments
    /// * `insts` - The instruction sequence starting from current position
    /// * `ctx` - Context providing type information and other metadata
    fn matches(&self, insts: &[MirInst], ctx: &MatchContext) -> Option<usize>;

    /// Pattern priority (higher = checked first).
    ///
    /// Patterns are checked in descending priority order. Specific patterns
    /// that optimize common cases should have higher priority than generic
    /// fallbacks.
    fn priority(&self) -> i32;

    /// Can this pattern handle the given result type?
    ///
    /// Some patterns only work for certain result types (e.g., integer
    /// arithmetic patterns only work when the result is used as an integer).
    fn can_produce(&self, _ty: RubyType) -> bool {
        // Default: patterns work for any type
        true
    }
}

/// Context for pattern matching.
///
/// Provides type information, register mappings, and other metadata
/// needed by patterns to make matching decisions.
#[derive(Debug)]
pub struct MatchContext<'a> {
    /// Known types of registers at this point.
    type_map: &'a HashMap<RegId, RubyType>,

    /// The function being compiled.
    function: &'a MirFunction,

    /// Current block index.
    block_idx: usize,

    /// Current instruction index within block.
    inst_idx: usize,
}

impl<'a> MatchContext<'a> {
    /// Create a new match context.
    pub fn new(
        type_map: &'a HashMap<RegId, RubyType>,
        function: &'a MirFunction,
        block_idx: usize,
        inst_idx: usize,
    ) -> Self {
        Self {
            type_map,
            function,
            block_idx,
            inst_idx,
        }
    }

    /// Get the type of a register (if known).
    pub fn get_type(&self, reg: RegId) -> Option<RubyType> {
        self.type_map.get(&reg).copied()
    }

    /// Check if a register has a known type.
    pub fn has_known_type(&self, reg: RegId) -> bool {
        self.type_map.contains_key(&reg)
    }

    /// Check if a register has a specific type.
    pub fn is_type(&self, reg: RegId, ty: RubyType) -> bool {
        self.get_type(reg) == Some(ty)
    }

    /// Get the function being compiled.
    pub fn function(&self) -> &MirFunction {
        self.function
    }

    /// Get the current block index.
    pub fn block_idx(&self) -> usize {
        self.block_idx
    }

    /// Get the current instruction index.
    pub fn inst_idx(&self) -> usize {
        self.inst_idx
    }
}

/// Result of a pattern match.
#[derive(Debug)]
pub struct SelectionResult<'a> {
    /// The matched pattern.
    pub pattern: &'a dyn SelectionPattern,

    /// Number of instructions matched.
    pub match_len: usize,

    /// The matched instructions.
    pub instructions: &'a [MirInst],
}

/// Registry of selection patterns.
///
/// Maintains a prioritized list of patterns and provides pattern matching
/// functionality for instruction selection.
pub struct PatternRegistry {
    patterns: Vec<Box<dyn SelectionPattern>>,
}

impl PatternRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self { patterns: vec![] }
    }

    /// Create with default patterns.
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();
        registry.add_defaults();
        registry
    }

    /// Add default patterns.
    fn add_defaults(&mut self) {
        // Arithmetic patterns
        self.add(Box::new(crate::selection::arithmetic::IntegerArithmeticPattern));
        self.add(Box::new(crate::selection::arithmetic::FloatArithmeticPattern));
        self.add(Box::new(crate::selection::arithmetic::StringConcatPattern));

        // Call patterns
        self.add(Box::new(crate::selection::calls::DirectCallPattern));
        self.add(Box::new(crate::selection::calls::PolymorphicCallPattern));
        self.add(Box::new(crate::selection::calls::DynamicSendPattern));

        // Metaprogramming patterns
        self.add(Box::new(crate::selection::metaprogramming::BlockCreatePattern));
        self.add(Box::new(crate::selection::metaprogramming::DefineMethodPattern));
        self.add(Box::new(crate::selection::metaprogramming::EvalPattern));
    }

    /// Add a pattern to the registry.
    pub fn add(&mut self, pattern: Box<dyn SelectionPattern>) {
        self.patterns.push(pattern);
        // Re-sort by priority
        self.patterns.sort_by_key(|p| -p.priority());
    }

    /// Find the best matching pattern for an instruction sequence.
    ///
    /// Returns the first pattern (by priority) that matches the sequence,
    /// or None if no pattern matches.
    pub fn find_match<'a, 'b>(
        &'a self,
        insts: &'b [MirInst],
        ctx: &MatchContext,
    ) -> Option<SelectionResult<'b>>
    where
        'a: 'b,
    {
        for pattern in &self.patterns {
            if let Some(n) = pattern.matches(insts, ctx) {
                return Some(SelectionResult {
                    pattern: pattern.as_ref(),
                    match_len: n,
                    instructions: &insts[..n],
                });
            }
        }
        None
    }

    /// Get all patterns.
    pub fn patterns(&self) -> &[Box<dyn SelectionPattern>] {
        &self.patterns
    }

    /// Get the number of patterns.
    pub fn len(&self) -> usize {
        self.patterns.len()
    }

    /// Check if registry is empty.
    pub fn is_empty(&self) -> bool {
        self.patterns.is_empty()
    }
}

impl Default for PatternRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Selection driver that orchestrates pattern matching for a function.
pub struct InstructionSelector<'a> {
    registry: &'a PatternRegistry,
    type_map: HashMap<RegId, RubyType>,
}

impl<'a> InstructionSelector<'a> {
    /// Create a new instruction selector.
    pub fn new(registry: &'a PatternRegistry, mir_module: &MirModule) -> Self {
        // Build type map from MIR
        let type_map = Self::build_type_map(mir_module);

        Self {
            registry,
            type_map,
        }
    }

    /// Build type map from MIR module.
    fn build_type_map(mir_module: &MirModule) -> HashMap<RegId, RubyType> {
        let mut map = HashMap::new();

        for func in &mir_module.functions {
            for block in &func.blocks {
                for inst in &block.instructions {
                    Self::infer_types(inst, &mut map);
                }
            }
        }

        map
    }

    /// Infer types from a single instruction.
    fn infer_types(inst: &MirInst, map: &mut HashMap<RegId, RubyType>) {
        use jdruby_mir::{MirConst, MirBinOp};

        match inst {
            MirInst::LoadConst(reg, MirConst::Integer(_)) => {
                map.insert(*reg, RubyType::Integer);
            }
            MirInst::LoadConst(reg, MirConst::Float(_)) => {
                map.insert(*reg, RubyType::Float);
            }
            MirInst::LoadConst(reg, MirConst::String(_)) => {
                map.insert(*reg, RubyType::String);
            }
            MirInst::LoadConst(reg, MirConst::Symbol(_)) => {
                map.insert(*reg, RubyType::Symbol);
            }
            MirInst::BinOp(dest, op, left, right) => {
                // Infer result type from operands
                let left_ty = map.get(left).copied();
                let right_ty = map.get(right).copied();

                let result_ty = match (left_ty, right_ty, op) {
                    (Some(RubyType::Integer), Some(RubyType::Integer), _) => RubyType::Integer,
                    (Some(RubyType::Float), _, _) | (_, Some(RubyType::Float), _) => RubyType::Float,
                    (Some(RubyType::String), Some(RubyType::String), MirBinOp::Add) => RubyType::String,
                    _ => RubyType::Unknown,
                };

                map.insert(*dest, result_ty);
            }
            _ => {}
        }
    }

    /// Select instructions for a function.
    ///
    /// Returns a sequence of selected operations with optimization applied.
    pub fn select_function(&self, function: &MirFunction) -> Vec<SelectedOp> {
        let mut result = vec![];

        for (block_idx, block) in function.blocks.iter().enumerate() {
            let mut inst_idx = 0;

            while inst_idx < block.instructions.len() {
                let ctx = MatchContext::new(
                    &self.type_map,
                    function,
                    block_idx,
                    inst_idx,
                );

                let remaining = &block.instructions[inst_idx..];

                match self.registry.find_match(remaining, &ctx) {
                    Some(selection) => {
                        // Pattern matched - add selected op
                        result.push(SelectedOp {
                            pattern_id: selection.pattern.id(),
                            instructions: selection.instructions.to_vec(),
                            block_idx,
                            inst_idx,
                        });
                        inst_idx += selection.match_len;
                    }
                    None => {
                        // No pattern matched - use generic fallback
                        result.push(SelectedOp {
                            pattern_id: "generic",
                            instructions: vec![remaining[0].clone()],
                            block_idx,
                            inst_idx,
                        });
                        inst_idx += 1;
                    }
                }
            }
        }

        result
    }
}

/// A selected operation ready for LLVM emission.
#[derive(Debug, Clone)]
pub struct SelectedOp {
    /// The pattern that selected this operation.
    pub pattern_id: &'static str,

    /// The matched MIR instructions.
    pub instructions: Vec<MirInst>,

    /// Block index in original function.
    pub block_idx: usize,

    /// Instruction index in original block.
    pub inst_idx: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct TestPattern {
        id: &'static str,
        priority: i32,
        match_len: usize,
    }

    impl SelectionPattern for TestPattern {
        fn id(&self) -> &'static str {
            self.id
        }

        fn priority(&self) -> i32 {
            self.priority
        }

        fn matches(&self, insts: &[MirInst], _ctx: &MatchContext) -> Option<usize> {
            if !insts.is_empty() {
                Some(self.match_len.min(insts.len()))
            } else {
                None
            }
        }
    }

    #[test]
    fn test_pattern_registry() {
        let mut registry = PatternRegistry::new();

        registry.add(Box::new(TestPattern {
            id: "low",
            priority: 10,
            match_len: 1,
        }));

        registry.add(Box::new(TestPattern {
            id: "high",
            priority: 100,
            match_len: 2,
        }));

        // Should be sorted by priority
        assert_eq!(registry.patterns()[0].id(), "high");
        assert_eq!(registry.patterns()[1].id(), "low");
    }

    #[test]
    fn test_match_context() {
        let type_map = HashMap::new();
        let func = MirFunction {
            name: "test".to_string(),
            params: vec![],
            blocks: vec![],
            next_reg: 0,
            span: jdruby_common::SourceSpan::default(),
            captured_vars: vec![],
        };

        let ctx = MatchContext::new(&type_map, &func, 0, 5);
        assert_eq!(ctx.block_idx(), 0);
        assert_eq!(ctx.inst_idx(), 5);
    }
}

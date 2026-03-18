//! # JDRuby MIR — Mid-Level Intermediate Representation
//!
//! Low-level IR with minimal metadata, ready for translation to LLVM IR.
//! Uses a flat, register-based instruction set.
//!
//! ## Planned Optimizations
//! - **Loop Unrolling**: Unroll small fixed-iteration loops
//! - **Instruction Simplification**: Strength reduction, algebraic simplification
//! - **Register Allocation Hints**: Prepare for efficient LLVM IR generation
//! - **Tail Call Optimization**: Convert tail-recursive calls to loops

use jdruby_common::SourceSpan;

/// A virtual register ID.
pub type RegId = u32;

/// A MIR basic block.
#[derive(Debug, Clone)]
pub struct MirBlock {
    /// Block label/identifier.
    pub label: String,
    /// Instructions in this block.
    pub instructions: Vec<MirInst>,
    /// Block terminator.
    pub terminator: MirTerminator,
}

/// A MIR instruction.
#[derive(Debug, Clone)]
pub enum MirInst {
    /// Load a constant value into a register.
    LoadConst(RegId, MirConst),
    /// Copy from one register to another.
    Copy(RegId, RegId),
    /// Binary operation: `dest = left op right`
    BinOp(RegId, MirBinOp, RegId, RegId),
    /// Unary operation: `dest = op operand`
    UnOp(RegId, MirUnOp, RegId),
    /// Call a function: `dest = func(args...)`
    Call(RegId, String, Vec<RegId>),
    /// Call a method on a receiver: `dest = receiver.method(args...)`
    MethodCall(RegId, RegId, String, Vec<RegId>),
    /// Load from a variable.
    Load(RegId, String),
    /// Store to a variable.
    Store(String, RegId),
    /// Allocate a Ruby object.
    Alloc(RegId, MirType),
    /// No operation (placeholder).
    Nop,
}

/// A MIR block terminator.
#[derive(Debug, Clone)]
pub enum MirTerminator {
    /// Return from function.
    Return(Option<RegId>),
    /// Unconditional branch.
    Branch(String),
    /// Conditional branch.
    CondBranch(RegId, String, String),
    /// Unreachable (dead code).
    Unreachable,
}

/// A MIR constant value.
#[derive(Debug, Clone)]
pub enum MirConst {
    Integer(i64),
    Float(f64),
    String(String),
    Symbol(String),
    Bool(bool),
    Nil,
}

/// Binary operation codes for MIR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MirBinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Shl,
    Shr,
    BitAnd,
    BitOr,
    BitXor,
    Eq,
    NotEq,
    Lt,
    Gt,
    LtEq,
    GtEq,
}

/// Unary operation codes for MIR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MirUnOp {
    Neg,
    Not,
    BitNot,
}

/// MIR type annotations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MirType {
    Integer,
    Float,
    String,
    Symbol,
    Bool,
    Nil,
    Array,
    Hash,
    Object(String),
    Any,
}

/// A MIR function definition.
#[derive(Debug, Clone)]
pub struct MirFunction {
    pub name: String,
    pub params: Vec<RegId>,
    pub blocks: Vec<MirBlock>,
    pub span: SourceSpan,
}

/// A MIR module (compilation unit).
#[derive(Debug, Clone)]
pub struct MirModule {
    pub name: String,
    pub functions: Vec<MirFunction>,
}

/// Trait for lowering HIR to MIR.
pub trait HirToMir {
    fn lower(&self, hir_nodes: &[jdruby_hir::HirNode]) -> MirModule;
}

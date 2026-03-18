//! # JDRuby HIR — High-Level Intermediate Representation
//!
//! Simplified AST preserving type information and variable names.
//! Used as the first IR in the compilation pipeline for high-level optimizations.
//!
//! ## Planned Optimizations
//! - **Constant Folding**: Evaluate constant expressions at compile time
//! - **Dead Code Elimination**: Remove unreachable code paths
//! - **Function Inlining**: Inline small method bodies at call sites
//! - **Escape Analysis**: Determine if objects can be stack-allocated

use jdruby_common::SourceSpan;

/// A HIR node — simplified representation of a Ruby expression/statement.
#[derive(Debug, Clone)]
pub enum HirNode {
    /// A literal value (integer, float, string, bool, nil).
    Literal(HirLiteral),
    /// A variable reference.
    VarRef(HirVarRef),
    /// A binary operation.
    BinOp(Box<HirBinOp>),
    /// A unary operation.
    UnOp(Box<HirUnOp>),
    /// A method call.
    Call(Box<HirCall>),
    /// An assignment.
    Assign(Box<HirAssign>),
    /// A conditional branch.
    Branch(Box<HirBranch>),
    /// A loop.
    Loop(Box<HirLoop>),
    /// A return from a method.
    Return(Box<HirReturn>),
    /// A function/method definition.
    FuncDef(Box<HirFuncDef>),
    /// A sequence of nodes (block body).
    Seq(Vec<HirNode>),
}

#[derive(Debug, Clone)]
pub struct HirLiteral {
    pub value: HirLiteralValue,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub enum HirLiteralValue {
    Integer(i64),
    Float(f64),
    String(String),
    Symbol(String),
    Bool(bool),
    Nil,
}

#[derive(Debug, Clone)]
pub struct HirVarRef {
    pub name: String,
    pub scope: VarScope,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VarScope {
    Local,
    Instance,
    Class,
    Global,
}

#[derive(Debug, Clone)]
pub struct HirBinOp {
    pub left: HirNode,
    pub op: String,
    pub right: HirNode,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct HirUnOp {
    pub op: String,
    pub operand: HirNode,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct HirCall {
    pub receiver: Option<HirNode>,
    pub method: String,
    pub args: Vec<HirNode>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct HirAssign {
    pub target: HirVarRef,
    pub value: HirNode,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct HirBranch {
    pub condition: HirNode,
    pub then_body: HirNode,
    pub else_body: Option<HirNode>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct HirLoop {
    pub condition: HirNode,
    pub body: HirNode,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct HirReturn {
    pub value: Option<HirNode>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct HirFuncDef {
    pub name: String,
    pub params: Vec<String>,
    pub body: HirNode,
    pub span: SourceSpan,
}

/// Trait for lowering AST to HIR.
pub trait AstToHir {
    /// Lower an AST Program to a sequence of HIR nodes.
    fn lower(&self, program: &jdruby_ast::Program) -> Vec<HirNode>;
}

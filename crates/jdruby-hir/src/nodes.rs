use jdruby_common::SourceSpan;

/// A HIR node — simplified Ruby IR keeping type info and names.
#[derive(Debug, Clone)]
pub enum HirNode {
    Literal(HirLiteral),
    VarRef(HirVarRef),
    BinOp(Box<HirBinOp>),
    UnOp(Box<HirUnOp>),
    Call(Box<HirCall>),
    Assign(Box<HirAssign>),
    Branch(Box<HirBranch>),
    Loop(Box<HirLoop>),
    Return(Box<HirReturn>),
    FuncDef(Box<HirFuncDef>),
    ClassDef(Box<HirClassDef>),
    Seq(Vec<HirNode>),
    Yield(Vec<HirNode>),
    Break,
    Next,
    Nop,
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
    Array(Vec<HirNode>),
    Hash(Vec<(HirNode, HirNode)>),
}

#[derive(Debug, Clone)]
pub struct HirVarRef {
    pub name: String,
    pub scope: VarScope,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VarScope { Local, Instance, Class, Global }

#[derive(Debug, Clone)]
pub struct HirBinOp {
    pub left: HirNode,
    pub op: HirOp,
    pub right: HirNode,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirOp {
    Add, Sub, Mul, Div, Mod, Pow,
    Eq, NotEq, Lt, Gt, LtEq, GtEq, Cmp,
    And, Or,
    BitAnd, BitOr, BitXor, Shl, Shr,
}

#[derive(Debug, Clone)]
pub struct HirUnOp {
    pub op: HirUnaryOp,
    pub operand: HirNode,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirUnaryOp { Neg, Not, BitNot }

#[derive(Debug, Clone)]
pub struct HirCall {
    pub receiver: Option<HirNode>,
    pub method: String,
    pub args: Vec<HirNode>,
    pub block: Option<HirBlock>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct HirBlock {
    pub params: Vec<String>,
    pub body: Vec<HirNode>,
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
    pub then_body: Vec<HirNode>,
    pub else_body: Vec<HirNode>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct HirLoop {
    pub condition: HirNode,
    pub body: Vec<HirNode>,
    pub is_while: bool,
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
    pub body: Vec<HirNode>,
    pub is_class_method: bool,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct HirClassDef {
    pub name: String,
    pub superclass: Option<String>,
    pub body: Vec<HirNode>,
    pub span: SourceSpan,
}

/// A HIR module (compilation unit).
#[derive(Debug, Clone)]
pub struct HirModule {
    pub name: String,
    pub nodes: Vec<HirNode>,
}

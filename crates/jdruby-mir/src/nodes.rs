use jdruby_common::SourceSpan;

pub type RegId = u32;
pub type BlockLabel = String;

#[derive(Debug, Clone)]
pub struct MirModule {
    pub name: String,
    pub functions: Vec<MirFunction>,
}

#[derive(Debug, Clone)]
pub struct MirFunction {
    pub name: String,
    pub params: Vec<RegId>,
    pub blocks: Vec<MirBlock>,
    pub next_reg: RegId,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct MirBlock {
    pub label: BlockLabel,
    pub instructions: Vec<MirInst>,
    pub terminator: MirTerminator,
}

#[derive(Debug, Clone)]
pub enum MirInst {
    LoadConst(RegId, MirConst),
    Copy(RegId, RegId),
    BinOp(RegId, MirBinOp, RegId, RegId),
    UnOp(RegId, MirUnOp, RegId),
    Call(RegId, String, Vec<RegId>),
    MethodCall(RegId, RegId, String, Vec<RegId>),
    Load(RegId, String),
    Store(String, RegId),
    Alloc(RegId, String),
    Nop,
}

#[derive(Debug, Clone)]
pub enum MirTerminator {
    Return(Option<RegId>),
    Branch(BlockLabel),
    CondBranch(RegId, BlockLabel, BlockLabel),
    Unreachable,
}

#[derive(Debug, Clone)]
pub enum MirConst {
    Integer(i64),
    Float(f64),
    String(String),
    Symbol(String),
    Bool(bool),
    Nil,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MirBinOp {
    Add, Sub, Mul, Div, Mod, Pow,
    Eq, NotEq, Lt, Gt, LtEq, GtEq, Cmp,
    And, Or,
    BitAnd, BitOr, BitXor, Shl, Shr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MirUnOp { Neg, Not, BitNot }

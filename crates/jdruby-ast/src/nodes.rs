use jdruby_common::SourceSpan;

/// The root node of a Ruby program.
#[derive(Debug, Clone)]
pub struct Program {
    /// Top-level statements/expressions in the program.
    pub body: Vec<Stmt>,
    /// The span covering the entire program.
    pub span: SourceSpan,
}

// ═══════════════════════════════════════════════════════════════
//  Statements
// ═══════════════════════════════════════════════════════════════

/// A Ruby statement.
#[derive(Debug, Clone)]
pub enum Stmt {
    /// An expression used as a statement.
    Expr(ExprStmt),
    /// A method definition: `def name(params) ... end`
    MethodDef(MethodDef),
    /// A class definition: `class Name < Super ... end`
    ClassDef(ClassDef),
    /// A module definition: `module Name ... end`
    ModuleDef(ModuleDef),
    /// An `if`/`elsif`/`else`/`end` statement.
    If(IfStmt),
    /// An `unless` statement.
    Unless(UnlessStmt),
    /// A `while` loop.
    While(WhileStmt),
    /// An `until` loop.
    Until(UntilStmt),
    /// A `for` loop: `for var in expr ... end`
    For(ForStmt),
    /// A `case`/`when` statement.
    Case(CaseStmt),
    /// A `begin`/`rescue`/`ensure`/`end` block.
    BeginRescue(BeginRescueStmt),
    /// A `return` statement.
    Return(ReturnStmt),
    /// A `yield` statement.
    Yield(YieldStmt),
    /// A `break` statement.
    Break(BreakStmt),
    /// A `next` statement.
    Next(NextStmt),
    /// A local variable or constant assignment.
    Assignment(AssignmentStmt),
    /// A compound assignment: `x += 1`, `x ||= default`
    CompoundAssignment(CompoundAssignmentStmt),
    /// An `alias` statement.
    Alias(AliasStmt),
    /// A `require` / `require_relative` statement.
    Require(RequireStmt),
    /// `attr_reader`, `attr_writer`, `attr_accessor`.
    AttrDecl(AttrDeclStmt),
    /// An `include`/`extend`/`prepend` statement.
    MixinStmt(MixinStmt),
}

/// An expression used as a statement.
#[derive(Debug, Clone)]
pub struct ExprStmt {
    pub expr: Expr,
    pub span: SourceSpan,
}

// ═══════════════════════════════════════════════════════════════
//  Expressions
// ═══════════════════════════════════════════════════════════════

/// A Ruby expression.
#[derive(Debug, Clone)]
pub enum Expr {
    // ── Literals ─────────────────────────────────────────
    /// Integer literal: `42`, `0xFF`
    IntegerLit(IntegerLit),
    /// Float literal: `3.14`
    FloatLit(FloatLit),
    /// String literal: `"hello"`, `'world'`
    StringLit(StringLit),
    /// Interpolated string: `"hello #{name}"`
    InterpolatedString(InterpolatedString),
    /// Symbol: `:foo`
    SymbolLit(SymbolLit),
    /// Boolean: `true`, `false`
    BoolLit(BoolLit),
    /// Nil: `nil`
    NilLit(NilLit),
    /// Array: `[1, 2, 3]`
    ArrayLit(ArrayLit),
    /// Hash: `{a: 1, b: 2}`
    HashLit(HashLit),
    /// Range: `1..10`, `1...10`
    RangeLit(RangeLit),
    /// Regex: `/pattern/flags`
    RegexLit(RegexLit),

    // ── Variables ────────────────────────────────────────
    /// Local variable reference: `foo`
    LocalVar(LocalVar),
    /// Instance variable: `@foo`
    InstanceVar(InstanceVarExpr),
    /// Class variable: `@@foo`
    ClassVar(ClassVarExpr),
    /// Global variable: `$foo`
    GlobalVar(GlobalVarExpr),
    /// Constant reference: `Foo`, `Foo::Bar`
    ConstRef(ConstRef),
    /// `self`
    SelfExpr(SelfExpr),

    // ── Operations ───────────────────────────────────────
    /// Binary operation: `a + b`, `x == y`
    BinaryOp(BinaryOp),
    /// Unary operation: `-x`, `!flag`, `~bits`
    UnaryOp(UnaryOp),

    // ── Calls ────────────────────────────────────────────
    /// Method call: `obj.method(args)` or `method(args)`
    MethodCall(MethodCall),
    /// Block call: `method { |x| ... }` or `method do |x| ... end`
    BlockCall(BlockCall),
    /// `super` call
    SuperCall(SuperCallExpr),
    /// `yield` expression
    YieldExpr(YieldExprNode),

    // ── Blocks & Lambdas ─────────────────────────────────
    /// Block/lambda: `-> (x) { ... }` or `lambda { |x| ... }`
    Lambda(LambdaExpr),
    /// Proc: `proc { |x| ... }` or `Proc.new { |x| ... }`
    Proc(ProcExpr),

    // ── Pattern Matching (Ruby 3.x+) ─────────────────────
    /// Pattern match expression: `expr in pattern`
    PatternMatch(PatternMatchExpr),

    // ── Ternary ──────────────────────────────────────────
    /// Ternary: `cond ? then : else`
    Ternary(TernaryExpr),

    // ── Defined? ─────────────────────────────────────────
    /// `defined?(expr)`
    Defined(DefinedExpr),
}

// ═══════════════════════════════════════════════════════════════
//  Literal Nodes
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct IntegerLit {
    pub value: i64,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct FloatLit {
    pub value: f64,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct StringLit {
    pub value: String,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct InterpolatedString {
    pub parts: Vec<StringPart>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub enum StringPart {
    Literal(String),
    Interpolation(Box<Expr>),
}

#[derive(Debug, Clone)]
pub struct SymbolLit {
    pub name: String,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct BoolLit {
    pub value: bool,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct NilLit {
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct ArrayLit {
    pub elements: Vec<Expr>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct HashLit {
    pub entries: Vec<(Expr, Expr)>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct RangeLit {
    pub start: Box<Expr>,
    pub end: Box<Expr>,
    /// `true` for `...` (exclusive), `false` for `..` (inclusive).
    pub exclusive: bool,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct RegexLit {
    pub pattern: String,
    pub flags: String,
    pub span: SourceSpan,
}

// ═══════════════════════════════════════════════════════════════
//  Variable Nodes
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct LocalVar {
    pub name: String,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct InstanceVarExpr {
    pub name: String,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct ClassVarExpr {
    pub name: String,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct GlobalVarExpr {
    pub name: String,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct ConstRef {
    pub path: Vec<String>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct SelfExpr {
    pub span: SourceSpan,
}

// ═══════════════════════════════════════════════════════════════
//  Operation Nodes
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct BinaryOp {
    pub left: Box<Expr>,
    pub op: BinOperator,
    pub right: Box<Expr>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOperator {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
    Eq,
    NotEq,
    Lt,
    Gt,
    LtEq,
    GtEq,
    Spaceship,
    CaseEq,
    Match,
    NotMatch,
    And,
    Or,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    Range,
    RangeExcl,
}

#[derive(Debug, Clone)]
pub struct UnaryOp {
    pub op: UnOperator,
    pub operand: Box<Expr>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOperator {
    Neg,
    Not,
    BitNot,
    Pos,
}

// ═══════════════════════════════════════════════════════════════
//  Call Nodes
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct MethodCall {
    /// The receiver (None for bare method calls).
    pub receiver: Option<Box<Expr>>,
    /// The method name.
    pub method: String,
    /// Positional arguments.
    pub args: Vec<Expr>,
    /// Keyword arguments.
    pub kwargs: Vec<(String, Expr)>,
    /// Block argument (`&block`).
    pub block_arg: Option<Box<Expr>>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct BlockCall {
    /// The method call this block is attached to.
    pub call: Box<MethodCall>,
    /// Block parameters: `|x, y|`
    pub params: Vec<Param>,
    /// The block body.
    pub body: Vec<Stmt>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct SuperCallExpr {
    pub args: Vec<Expr>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct YieldExprNode {
    pub args: Vec<Expr>,
    pub span: SourceSpan,
}

// ═══════════════════════════════════════════════════════════════
//  Definitions
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct MethodDef {
    /// Method name.
    pub name: String,
    /// Parameters.
    pub params: Vec<Param>,
    /// Method body.
    pub body: Vec<Stmt>,
    /// Whether this is a class-level method (`def self.method`).
    pub is_class_method: bool,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct Param {
    /// Parameter name.
    pub name: String,
    /// Default value (if any).
    pub default: Option<Expr>,
    /// Parameter kind.
    pub kind: ParamKind,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamKind {
    /// Normal positional parameter.
    Required,
    /// Optional parameter with default value.
    Optional,
    /// Splat/rest parameter: `*args`
    Rest,
    /// Double-splat/keyword rest: `**opts`
    KeywordRest,
    /// Block parameter: `&block`
    Block,
    /// Keyword parameter: `name:`
    Keyword,
}

#[derive(Debug, Clone)]
pub struct ClassDef {
    /// Class name.
    pub name: String,
    /// Superclass (if any).
    pub superclass: Option<Box<Expr>>,
    /// Class body.
    pub body: Vec<Stmt>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct ModuleDef {
    /// Module name.
    pub name: String,
    /// Module body.
    pub body: Vec<Stmt>,
    pub span: SourceSpan,
}

// ═══════════════════════════════════════════════════════════════
//  Control Flow Statements
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct IfStmt {
    pub condition: Expr,
    pub then_body: Vec<Stmt>,
    pub elsif_clauses: Vec<ElsifClause>,
    pub else_body: Option<Vec<Stmt>>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct ElsifClause {
    pub condition: Expr,
    pub body: Vec<Stmt>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct UnlessStmt {
    pub condition: Expr,
    pub body: Vec<Stmt>,
    pub else_body: Option<Vec<Stmt>>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct WhileStmt {
    pub condition: Expr,
    pub body: Vec<Stmt>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct UntilStmt {
    pub condition: Expr,
    pub body: Vec<Stmt>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct ForStmt {
    pub var: String,
    pub iterable: Expr,
    pub body: Vec<Stmt>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct CaseStmt {
    pub subject: Option<Expr>,
    pub when_clauses: Vec<WhenClause>,
    pub else_body: Option<Vec<Stmt>>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct WhenClause {
    pub patterns: Vec<Expr>,
    pub body: Vec<Stmt>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct BeginRescueStmt {
    pub body: Vec<Stmt>,
    pub rescue_clauses: Vec<RescueClause>,
    pub else_body: Option<Vec<Stmt>>,
    pub ensure_body: Option<Vec<Stmt>>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct RescueClause {
    /// Exception classes to catch.
    pub exceptions: Vec<Expr>,
    /// Variable to bind the exception to.
    pub var: Option<String>,
    pub body: Vec<Stmt>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct ReturnStmt {
    pub value: Option<Expr>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct YieldStmt {
    pub args: Vec<Expr>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct BreakStmt {
    pub value: Option<Expr>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct NextStmt {
    pub value: Option<Expr>,
    pub span: SourceSpan,
}

// ═══════════════════════════════════════════════════════════════
//  Assignment Statements
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct AssignmentStmt {
    pub target: AssignTarget,
    pub value: Expr,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub enum AssignTarget {
    LocalVar(String),
    InstanceVar(String),
    ClassVar(String),
    GlobalVar(String),
    Constant(String),
    Index(Box<Expr>, Box<Expr>),
    Attribute(Box<Expr>, String),
}

#[derive(Debug, Clone)]
pub struct CompoundAssignmentStmt {
    pub target: AssignTarget,
    pub op: BinOperator,
    pub value: Expr,
    pub span: SourceSpan,
}

// ═══════════════════════════════════════════════════════════════
//  Other Statements
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct AliasStmt {
    pub new_name: String,
    pub old_name: String,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct RequireStmt {
    pub path: String,
    pub is_relative: bool,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct AttrDeclStmt {
    pub kind: AttrKind,
    pub names: Vec<String>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttrKind {
    Reader,
    Writer,
    Accessor,
}

#[derive(Debug, Clone)]
pub struct MixinStmt {
    pub kind: MixinKind,
    pub module: Expr,
    pub span: SourceSpan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MixinKind {
    Include,
    Extend,
    Prepend,
}

// ═══════════════════════════════════════════════════════════════
//  Lambda / Proc / Pattern Match / Ternary / Defined
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct LambdaExpr {
    pub params: Vec<Param>,
    pub body: Vec<Stmt>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct ProcExpr {
    pub params: Vec<Param>,
    pub body: Vec<Stmt>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct PatternMatchExpr {
    pub subject: Box<Expr>,
    pub pattern: Box<Expr>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct TernaryExpr {
    pub condition: Box<Expr>,
    pub then_expr: Box<Expr>,
    pub else_expr: Box<Expr>,
    pub span: SourceSpan,
}

#[derive(Debug, Clone)]
pub struct DefinedExpr {
    pub expr: Box<Expr>,
    pub span: SourceSpan,
}

use jdruby_common::SourceSpan;

/// A HIR node — high-level Ruby IR keeping type info and names.
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

    // =========================================================================
    // EXCEPTION HANDLING
    // =========================================================================
    /// Exception handling block: begin/rescue/ensure
    ExceptionBegin(Box<HirExceptionBegin>),
    /// Block definition: `{ |args| body }` or `do |args| body end`
    BlockDef(Box<HirBlockDef>),
    /// Proc definition: `Proc.new { }` or `proc { }`
    ProcDef(Box<HirProcDef>),
    /// Lambda definition: `->(args) { }` or `lambda { }`
    LambdaDef(Box<HirLambdaDef>),

    // Module/Class Metaprogramming
    /// Module definition: `module M; end`
    ModuleDef(Box<HirModuleDef>),
    /// Singleton class: `class << obj; end`
    SingletonClass(Box<HirSingletonClass>),

    // Dynamic Method Operations
    /// Dynamic method definition: `define_method(name) { body }`
    DefineMethod(Box<HirDefineMethod>),
    /// Undefine method: `undef :method`
    UndefMethod(Box<HirUndefMethod>),
    /// Method aliasing: `alias new_name old_name`
    AliasMethod(Box<HirAliasMethod>),
    /// Remove method: `remove_method :name`
    RemoveMethod(Box<HirRemoveMethod>),

    // Dynamic Evaluation
    /// instance_eval: `obj.instance_eval { }` or `obj.instance_eval("code")`
    InstanceEval(Box<HirEval>),
    /// class_eval: `Klass.class_eval { }` or `Klass.class_eval("code")`
    ClassEval(Box<HirEval>),
    /// module_eval: `Mod.module_eval { }` or `Mod.module_eval("code")`
    ModuleEval(Box<HirEval>),
    /// eval: `eval("code")` or `eval("code", binding)`
    Eval(Box<HirEval>),
    /// Binding capture: `binding`
    BindingGet(HirBindingGet),

    // Method Missing Hook
    /// Method missing call: implicit call when method not found
    MethodMissing(Box<HirMethodMissingCall>),

    // Reflection
    /// respond_to?: `obj.respond_to?(:method)`
    RespondTo(Box<HirRespondTo>),
    /// method: `obj.method(:name)`
    MethodObj(Box<HirMethodObj>),
    /// instance_method: `Klass.instance_method(:name)`
    InstanceMethod(Box<HirInstanceMethod>),
    /// send: `obj.send(:method, args)`
    Send(Box<HirSend>),
    /// public_send: `obj.public_send(:method, args)`
    PublicSend(Box<HirSend>),
    /// __send__: `obj.__send__(:method, args)`
    InternalSend(Box<HirSend>),

    // Variable Access (Dynamic)
    /// instance_variable_get: `obj.instance_variable_get(:@name)`
    IvarGetDynamic(Box<HirDynamicVarAccess>),
    /// instance_variable_set: `obj.instance_variable_set(:@name, value)`
    IvarSetDynamic(Box<HirDynamicVarAccess>),
    /// class_variable_get: `Klass.class_variable_get(:@@name)`
    CvarGetDynamic(Box<HirDynamicVarAccess>),
    /// class_variable_set: `Klass.class_variable_set(:@@name, value)`
    CvarSetDynamic(Box<HirDynamicVarAccess>),
    /// const_get: `Klass.const_get(:Name)`
    ConstGetDynamic(Box<HirDynamicConstAccess>),
    /// const_set: `Klass.const_set(:Name, value)`
    ConstSetDynamic(Box<HirDynamicConstAccess>),

    // Method Visibility
    /// public: `public :method` or `public` (scope)
    VisibilitySet(Box<HirVisibilitySet>),

    // Method Object Operations
    /// Method#call or UnboundMethod#bind_call
    MethodCall(Box<HirMethodCall>),
    /// UnboundMethod#bind
    MethodBind(Box<HirMethodBind>),

    // Include/Extend/Prepend operations (dynamic)
    /// Dynamic include: `include Mod`
    Include(Box<HirInclude>),
    /// Dynamic extend: `extend Mod`
    Extend(Box<HirInclude>),
    /// Dynamic prepend: `prepend Mod`
    Prepend(Box<HirInclude>),
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
    pub params: Vec<HirBlockParam>,
    pub body: Vec<HirNode>,
    pub captured_vars: Vec<String>,
    pub captures_self: bool,
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

// ============================================================================
// METAPROGRAMMING STRUCT DEFINITIONS
// ============================================================================

// ----------------------------------------------------------------------------
// Block and Closure Definitions
// ----------------------------------------------------------------------------

/// Parameter for a block with optional default and splat support
#[derive(Debug, Clone)]
pub struct HirBlockParam {
    pub name: String,
    pub default_value: Option<HirNode>,
    pub splat: bool,
    pub block: bool,
    pub span: SourceSpan,
}

/// Block definition: `{ |args| body }` or `do |args| body end`
#[derive(Debug, Clone)]
pub struct HirBlockDef {
    pub params: Vec<HirBlockParam>,
    pub body: Vec<HirNode>,
    pub is_lambda: bool,
    pub captures_self: bool,
    pub captured_vars: Vec<String>,
    pub span: SourceSpan,
}

/// Proc definition: `Proc.new { }` or `proc { }`
#[derive(Debug, Clone)]
pub struct HirProcDef {
    pub params: Vec<HirBlockParam>,
    pub body: Vec<HirNode>,
    pub captures_self: bool,
    pub captured_vars: Vec<String>,
    pub span: SourceSpan,
}

/// Lambda definition: `->(args) { }` or `lambda { }`
#[derive(Debug, Clone)]
pub struct HirLambdaDef {
    pub params: Vec<HirBlockParam>,
    pub body: Vec<HirNode>,
    pub captures_self: bool,
    pub captured_vars: Vec<String>,
    pub span: SourceSpan,
}

// ----------------------------------------------------------------------------
// Module and Class Metaprogramming
// ----------------------------------------------------------------------------

/// Module definition: `module M; end`
#[derive(Debug, Clone)]
pub struct HirModuleDef {
    pub name: String,
    pub body: Vec<HirNode>,
    pub nesting_path: Vec<String>,
    pub span: SourceSpan,
}

/// Singleton class: `class << obj; end`
#[derive(Debug, Clone)]
pub struct HirSingletonClass {
    pub receiver: HirNode,
    pub body: Vec<HirNode>,
    pub span: SourceSpan,
}

// ----------------------------------------------------------------------------
// Dynamic Method Operations
// ----------------------------------------------------------------------------

/// Dynamic method definition: `define_method(name) { body }`
#[derive(Debug, Clone)]
pub struct HirDefineMethod {
    pub target_class: Option<HirNode>,
    pub name: HirNode,
    pub body: HirBlockDef,
    pub visibility: Option<Visibility>,
    pub span: SourceSpan,
}

/// Undefine method: `undef :method`
#[derive(Debug, Clone)]
pub struct HirUndefMethod {
    pub target_class: Option<HirNode>,
    pub name: HirNode,
    pub span: SourceSpan,
}

/// Method aliasing: `alias new_name old_name`
#[derive(Debug, Clone)]
pub struct HirAliasMethod {
    pub target_class: Option<HirNode>,
    pub new_name: HirNode,
    pub old_name: HirNode,
    pub span: SourceSpan,
}

/// Remove method: `remove_method :name`
#[derive(Debug, Clone)]
pub struct HirRemoveMethod {
    pub target_class: Option<HirNode>,
    pub name: HirNode,
    pub span: SourceSpan,
}

/// Method visibility levels
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Public,
    Protected,
    Private,
    ModuleFunction,
}

// ----------------------------------------------------------------------------
// Dynamic Evaluation
// ----------------------------------------------------------------------------

/// Kind of eval operation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvalKind {
    InstanceEval,
    ClassEval,
    ModuleEval,
    Eval,
}

/// Source for eval: string or block
#[derive(Debug, Clone)]
pub enum HirEvalSource {
    String(HirNode),
    Block(HirBlockDef),
}

/// Eval operations: instance_eval, class_eval, module_eval, eval
#[derive(Debug, Clone)]
pub struct HirEval {
    pub kind: EvalKind,
    pub receiver: Option<HirNode>,
    pub source: HirEvalSource,
    pub binding: Option<HirNode>,
    pub filename: Option<String>,
    pub line: Option<u32>,
    pub span: SourceSpan,
}

/// Binding capture: `binding`
#[derive(Debug, Clone)]
pub struct HirBindingGet {
    pub span: SourceSpan,
}

// ----------------------------------------------------------------------------
// Method Missing Hook
// ----------------------------------------------------------------------------

/// Method missing call: implicit when method not found
#[derive(Debug, Clone)]
pub struct HirMethodMissingCall {
    pub receiver: HirNode,
    pub method_name: String,
    pub args: Vec<HirNode>,
    pub block: Option<HirBlockDef>,
    pub original_call: Option<Box<HirCall>>,
    pub span: SourceSpan,
}

// ----------------------------------------------------------------------------
// Reflection
// ----------------------------------------------------------------------------

/// respond_to?: `obj.respond_to?(:method)`
#[derive(Debug, Clone)]
pub struct HirRespondTo {
    pub receiver: HirNode,
    pub method_name: HirNode,
    pub include_private: bool,
    pub span: SourceSpan,
}

/// method: `obj.method(:name)`
#[derive(Debug, Clone)]
pub struct HirMethodObj {
    pub receiver: HirNode,
    pub method_name: HirNode,
    pub span: SourceSpan,
}

/// instance_method: `Klass.instance_method(:name)`
#[derive(Debug, Clone)]
pub struct HirInstanceMethod {
    pub target_class: HirNode,
    pub method_name: HirNode,
    pub span: SourceSpan,
}

/// send operations: send, public_send, __send__
#[derive(Debug, Clone)]
pub struct HirSend {
    pub receiver: HirNode,
    pub method_name: HirNode,
    pub args: Vec<HirNode>,
    pub block: Option<HirBlockDef>,
    pub span: SourceSpan,
}

// ----------------------------------------------------------------------------
// Dynamic Variable Access
// ----------------------------------------------------------------------------

/// Dynamic variable access: instance_variable_get/set, class_variable_get/set
#[derive(Debug, Clone)]
pub struct HirDynamicVarAccess {
    pub target: HirNode,
    pub name: HirNode,
    pub value: Option<HirNode>,
    pub span: SourceSpan,
}

/// Dynamic constant access: const_get, const_set
#[derive(Debug, Clone)]
pub struct HirDynamicConstAccess {
    pub target_class: HirNode,
    pub name: HirNode,
    pub value: Option<HirNode>,
    pub inherit: bool,
    pub span: SourceSpan,
}

// ----------------------------------------------------------------------------
// Method Visibility Operations
// ----------------------------------------------------------------------------

/// Visibility setting: public/protected/private :method_name
#[derive(Debug, Clone)]
pub struct HirVisibilitySet {
    pub visibility: Visibility,
    pub target_class: Option<HirNode>,
    pub method_names: Vec<HirNode>,
    pub span: SourceSpan,
}

// ----------------------------------------------------------------------------
// Method Object Operations
// ----------------------------------------------------------------------------

/// Method call on Method/UnboundMethod object
#[derive(Debug, Clone)]
pub struct HirMethodCall {
    pub method_obj: HirNode,
    pub receiver: Option<HirNode>,
    pub args: Vec<HirNode>,
    pub block: Option<HirBlockDef>,
    pub span: SourceSpan,
}

/// Method binding: UnboundMethod#bind(obj)
#[derive(Debug, Clone)]
pub struct HirMethodBind {
    pub method_obj: HirNode,
    pub receiver: HirNode,
    pub span: SourceSpan,
}

// ----------------------------------------------------------------------------
// Include/Extend/Prepend Operations
// ----------------------------------------------------------------------------

/// Include/Extend/Prepend operation
#[derive(Debug, Clone)]
pub struct HirInclude {
    pub target_class: Option<HirNode>,
    pub module: HirNode,
    pub span: SourceSpan,
}

// ----------------------------------------------------------------------------
// Exception Handling
// ----------------------------------------------------------------------------

/// Exception handling block: begin/rescue/ensure/end
#[derive(Debug, Clone)]
pub struct HirExceptionBegin {
    /// Body of the begin block (protected code)
    pub body: Vec<HirNode>,
    /// Rescue clauses for exception handling
    pub rescue_clauses: Vec<HirRescueClause>,
    /// Else body (executed if no exception was raised)
    pub else_body: Option<Vec<HirNode>>,
    /// Ensure body (always executed)
    pub ensure_body: Option<Vec<HirNode>>,
    pub span: SourceSpan,
}

/// A single rescue clause
#[derive(Debug, Clone)]
pub struct HirRescueClause {
    /// Exception types to catch (empty = catch all StandardError)
    pub exceptions: Vec<HirNode>,
    /// Variable to bind exception to
    pub var: Option<String>,
    /// Handler body
    pub body: Vec<HirNode>,
    pub span: SourceSpan,
}

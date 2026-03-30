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
    /// For block functions: names of captured variables that will be passed as extra params
    pub captured_vars: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct MirBlock {
    pub label: BlockLabel,
    pub instructions: Vec<MirInst>,
    pub terminator: MirTerminator,
}

#[derive(Debug, Clone)]
pub enum MirInst {
    // =========================================================================
    // CORE INSTRUCTIONS
    // =========================================================================
    LoadConst(RegId, MirConst),
    Copy(RegId, RegId),
    BinOp(RegId, MirBinOp, RegId, RegId),
    UnOp(RegId, MirUnOp, RegId),
    Call(RegId, String, Vec<RegId>),
    MethodCall(RegId, RegId, String, Vec<RegId>, Option<RegId>),
    Load(RegId, String),
    Store(String, RegId),
    Alloc(RegId, String),
    Nop,

    // =========================================================================
    // CLASS/MODULE OPERATIONS
    // =========================================================================
    /// Create a new class: dest = class_new(name, superclass_name)
    ClassNew(RegId, String, Option<String>),
    /// Create a new module: dest = module_new(name)
    ModuleNew(RegId, String),
    /// Get singleton class: dest = singleton_class_of(obj)
    SingletonClassGet(RegId, RegId),
    /// Register a method on a class: def_method(class_reg, method_name, func_name)
    DefMethod(RegId, String, String),
    /// Register a singleton method on an object: def_singleton_method(obj_reg, method_name, func_name)
    DefSingletonMethod(RegId, String, String),
    /// Include a module into a class: include_module(class_reg, module_reg)
    IncludeModule(RegId, RegId),
    /// Prepend a module into a class: prepend_module(class_reg, module_reg)
    PrependModule(RegId, RegId),
    /// Extend object with module: extend_module(obj_reg, module_reg)
    ExtendModule(RegId, RegId),

    // =========================================================================
    // BLOCK/CLOSURE OPERATIONS
    // =========================================================================
    /// Create a block: dest = block_create(func_symbol, captured_vars)
    BlockCreate {
        dest: RegId,
        func_symbol: String,
        captured_vars: Vec<RegId>,
        is_lambda: bool,
    },
    /// Create a proc from block: dest = proc_create(block_reg)
    ProcCreate {
        dest: RegId,
        block_reg: RegId,
    },
    /// Create a lambda from block: dest = lambda_create(block_reg)
    LambdaCreate {
        dest: RegId,
        block_reg: RegId,
    },
    /// Yield to block: dest = block_yield(block_reg, args)
    BlockYield {
        dest: RegId,
        block_reg: RegId,
        args: Vec<RegId>,
    },
    /// Invoke block/proc/lambda with full argument handling: dest = block_invoke(block_reg, args, splat_arg)
    BlockInvoke {
        dest: RegId,
        block_reg: RegId,
        args: Vec<RegId>,
        splat_arg: Option<RegId>,
        block_arg: Option<RegId>,
    },
    /// Check if block given: dest = block_given?()
    BlockGiven {
        dest: RegId,
    },
    /// Get current block: dest = current_block()
    CurrentBlock {
        dest: RegId,
    },
    /// Convert symbol to proc: dest = symbol_to_proc(symbol_reg)
    SymbolToProc {
        dest: RegId,
        symbol_reg: RegId,
    },

    // =========================================================================
    // DYNAMIC METHOD OPERATIONS
    // =========================================================================
    /// Dynamic method definition: define_method(class_reg, name_reg, method_ptr, block_reg)
    DefineMethodDynamic {
        dest: RegId,
        class_reg: RegId,
        name_reg: RegId,
        method_func: String,
        visibility: MirVisibility,
        block_reg: Option<RegId>,  // Block to associate with the method
    },
    /// Undefine method: undef_method(class_reg, name_reg)
    UndefMethod {
        dest: RegId,
        class_reg: RegId,
        name_reg: RegId,
    },
    /// Remove method: remove_method(class_reg, name_reg)
    RemoveMethod {
        dest: RegId,
        class_reg: RegId,
        name_reg: RegId,
    },
    /// Alias method: alias_method(class_reg, new_name_reg, old_name_reg)
    AliasMethod {
        dest: RegId,
        class_reg: RegId,
        new_name_reg: RegId,
        old_name_reg: RegId,
    },
    /// Set method visibility: set_visibility(class_reg, visibility, method_names)
    SetVisibility {
        dest: RegId,
        class_reg: RegId,
        visibility: MirVisibility,
        method_names: Vec<RegId>,
    },

    // =========================================================================
    // DYNAMIC EVALUATION
    // =========================================================================
    /// Eval string: dest = eval(code_reg, binding_reg, filename, line)
    Eval {
        dest: RegId,
        code_reg: RegId,
        binding_reg: Option<RegId>,
        filename_reg: Option<RegId>,
        line_reg: Option<RegId>,
    },
    /// Instance eval: dest = instance_eval(obj_reg, code_reg_or_block)
    InstanceEval {
        dest: RegId,
        obj_reg: RegId,
        code_reg: RegId,
        binding_reg: Option<RegId>,
    },
    /// Class eval: dest = class_eval(class_reg, code_reg_or_block)
    ClassEval {
        dest: RegId,
        class_reg: RegId,
        code_reg: RegId,
        binding_reg: Option<RegId>,
    },
    /// Module eval: dest = module_eval(module_reg, code_reg_or_block)
    ModuleEval {
        dest: RegId,
        module_reg: RegId,
        code_reg: RegId,
        binding_reg: Option<RegId>,
    },
    /// Get current binding: dest = binding_get()
    BindingGet {
        dest: RegId,
    },

    // =========================================================================
    // REFLECTION
    // =========================================================================
    /// Send: dest = send(obj_reg, name_reg, args, block_reg)
    Send {
        dest: RegId,
        obj_reg: RegId,
        name_reg: RegId,
        args: Vec<RegId>,
        block_reg: Option<RegId>,
    },
    /// Send with inline cache: optimized for static method names
    SendWithIC {
        dest: RegId,
        obj_reg: RegId,
        method_name: String,      // known at compile time
        args: Vec<RegId>,
        block_reg: Option<RegId>,
        cache_slot: u32,          // IC slot for fast path
    },
    /// Public send: dest = public_send(obj_reg, name_reg, args, block_reg)
    PublicSend {
        dest: RegId,
        obj_reg: RegId,
        name_reg: RegId,
        args: Vec<RegId>,
        block_reg: Option<RegId>,
    },
    /// respond_to?: dest = respond_to?(obj_reg, name_reg, include_private)
    RespondTo {
        dest: RegId,
        obj_reg: RegId,
        name_reg: RegId,
        include_private: bool,
    },
    /// method: dest = method(obj_reg, name_reg)
    MethodGet {
        dest: RegId,
        obj_reg: RegId,
        name_reg: RegId,
    },
    /// instance_method: dest = instance_method(class_reg, name_reg)
    InstanceMethodGet {
        dest: RegId,
        class_reg: RegId,
        name_reg: RegId,
    },
    /// Call method object: dest = method_call(method_reg, receiver, args)
    MethodObjectCall {
        dest: RegId,
        method_reg: RegId,
        receiver_reg: Option<RegId>,
        args: Vec<RegId>,
        block_reg: Option<RegId>,
    },
    /// Bind method: dest = method_bind(method_reg, obj_reg)
    MethodBind {
        dest: RegId,
        method_reg: RegId,
        obj_reg: RegId,
    },

    // =========================================================================
    // DYNAMIC VARIABLE ACCESS
    // =========================================================================
    /// Dynamic instance variable get: dest = ivar_get(obj_reg, name_reg)
    IvarGetDynamic {
        dest: RegId,
        obj_reg: RegId,
        name_reg: RegId,
    },
    /// Dynamic instance variable set: ivar_set(obj_reg, name_reg, value_reg)
    IvarSetDynamic {
        obj_reg: RegId,
        name_reg: RegId,
        value_reg: RegId,
    },
    /// Dynamic class variable get: dest = cvar_get(class_reg, name_reg)
    CvarGetDynamic {
        dest: RegId,
        class_reg: RegId,
        name_reg: RegId,
    },
    /// Dynamic class variable set: cvar_set(class_reg, name_reg, value_reg)
    CvarSetDynamic {
        class_reg: RegId,
        name_reg: RegId,
        value_reg: RegId,
    },
    /// Dynamic constant get: dest = const_get(class_reg, name_reg, inherit)
    ConstGetDynamic {
        dest: RegId,
        class_reg: RegId,
        name_reg: RegId,
        inherit: bool,
    },
    /// Dynamic constant set: const_set(class_reg, name_reg, value_reg)
    ConstSetDynamic {
        class_reg: RegId,
        name_reg: RegId,
        value_reg: RegId,
    },

    // =========================================================================
    // METHOD MISSING
    // =========================================================================
    /// Method missing call: dest = method_missing(obj_reg, name_reg, args, block_reg)
    MethodMissing {
        dest: RegId,
        obj_reg: RegId,
        name_reg: RegId,
        args: Vec<RegId>,
        block_reg: Option<RegId>,
    },
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

/// Method visibility levels in MIR
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MirVisibility {
    Public,
    Protected,
    Private,
    ModuleFunction,
}

/// A block compiled to a standalone function for MIR
#[derive(Debug, Clone)]
pub struct MirBlockFunc {
    pub symbol_name: String,
    pub params: Vec<RegId>,
    pub captures: Vec<RegId>,
    pub body: Vec<MirBlock>,
    pub span: SourceSpan,
}

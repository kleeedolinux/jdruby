//! MRI-Compatible Type Definitions
//!
//! These structures mirror CRuby's internal representation for FFI compatibility.
//! Layout and field names are designed to match MRI's C headers.

use std::os::raw::{c_int, c_uint, c_void};
use std::mem::ManuallyDrop;

/// Ruby VALUE type - opaque pointer or immediate value
pub type Value = usize;

/// Object ID type
pub type ObjectId = Value;

/// Class ID type
pub type ClassId = Value;

/// Module ID type  
pub type ModuleId = Value;

/// Symbol ID type
pub type SymbolId = Value;

/// Proc ID type
pub type ProcId = Value;

/// Block ID type
pub type BlockId = Value;

/// Binding ID type
pub type BindingId = Value;

/// Method ID type
pub type MethodId = Value;

/// Constant ID type
pub type ConstId = Value;

/// ID type (Ruby internal identifier)
pub type ID = usize;

/// Ruby T_MASK flags (from ruby.h)
pub const T_MASK: u64 = 0x1f;
pub const T_NONE: u64 = 0x00;
pub const T_OBJECT: u64 = 0x01;
pub const T_CLASS: u64 = 0x02;
pub const T_MODULE: u64 = 0x03;
pub const T_FLOAT: u64 = 0x04;
pub const T_STRING: u64 = 0x05;
pub const T_REGEXP: u64 = 0x06;
pub const T_ARRAY: u64 = 0x07;
pub const T_HASH: u64 = 0x08;
pub const T_STRUCT: u64 = 0x09;
pub const T_BIGNUM: u64 = 0x0a;
pub const T_FILE: u64 = 0x0b;
pub const T_DATA: u64 = 0x0c;
pub const T_MATCH: u64 = 0x0d;
pub const T_COMPLEX: u64 = 0x0e;
pub const T_RATIONAL: u64 = 0x0f;
pub const T_NIL: u64 = 0x11;
pub const T_TRUE: u64 = 0x12;
pub const T_FALSE: u64 = 0x13;
pub const T_SYMBOL: u64 = 0x14;
pub const T_FIXNUM: u64 = 0x15;
pub const T_UNDEF: u64 = 0x16;
pub const T_IMEMO: u64 = 0x1a;
pub const T_NODE: u64 = 0x1b;
pub const T_ICLASS: u64 = 0x1c;
pub const T_ZOMBIE: u64 = 0x1d;

/// FL_USER flags (object-level flags)
pub const FL_WB_PROTECTED: u64 = 1 << 5;
pub const FL_PROMOTED0: u64 = 1 << 6;
pub const FL_PROMOTED1: u64 = 1 << 7;
pub const FL_FINALIZE: u64 = 1 << 8;
pub const FL_SHAREABLE: u64 = 1 << 9;
pub const FL_EXIVAR: u64 = 1 << 10;
pub const FL_FREEZE: u64 = 1 << 11;

/// Method visibility levels
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub enum Visibility {
    Public = 0,
    Protected = 1,
    Private = 2,
    ModuleFunction = 3,
}

/// Method type (how the method is defined)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub enum MethodType {
    Iseq = 0,       // Ruby method
    Cfunc = 1,      // C function
    VMDefined = 2,  // VM-defined
    Alias = 3,      // Alias
    Refine = 4,     // Refinement
    Zsuper = 5,     // super
    Missing = 6,    // method_missing
    Optimized = 7,  // optimized
    Bmethod = 8,   // basic method
}

/// =============================================================================
/// RBasic - Base structure for all Ruby objects (matches CRuby RBasic)
/// =============================================================================

#[repr(C)]
pub struct RBasic {
    /// Flags (T_MASK type in lower bits, FL_USER flags in upper)
    pub flags: u64,
    /// Pointer to the class
    pub klass: *const RClass,
}

impl RBasic {
    /// Get the type from flags
    pub fn builtin_type(&self) -> u64 {
        self.flags & T_MASK
    }

    /// Check if object is of a specific type
    pub fn is_type(&self, t: u64) -> bool {
        self.builtin_type() == t
    }

    /// Check if frozen
    pub fn frozen(&self) -> bool {
        (self.flags & FL_FREEZE) != 0
    }

    /// Set frozen flag
    pub fn set_frozen(&mut self) {
        self.flags |= FL_FREEZE;
    }
}

/// =============================================================================
/// RClass - Class/Module structure (matches CRuby RClass)
/// =============================================================================

#[repr(C)]
pub struct RClass {
    pub basic: RBasic,
    /// Superclass pointer
    pub super_: *const RClass,
    /// Method table (m_tbl)
    pub m_tbl: *mut MethodTable,
    /// Instance variable table (iv_tbl)
    pub iv_tbl: *mut IvTable,
    /// Constant table (const_tbl)
    pub const_tbl: *mut ConstTable,
    /// Class-specific data
    pub class_serial: u64,
    /// Refinement class pointer (for refinements)
    pub refined_class: *const RClass,
    /// Include chain (for module inclusion)
    pub include_classes: *mut IClass,
}

/// IClass - Internal class for module inclusion (include chain)
#[repr(C)]
pub struct IClass {
    pub basic: RBasic,
    pub super_: *const RClass,
    pub module: *const RClass,
}

/// =============================================================================
/// Method Table structures
/// =============================================================================

/// Method table (hash map from symbol ID to method entry)
#[repr(C)]
pub struct MethodTable {
    pub num_entries: usize,
    pub entries: *mut MethodEntryNode,
}

/// Node in method table hash chain
#[repr(C)]
pub struct MethodEntryNode {
    pub key: SymbolId,
    pub entry: *mut MethodEntry,
    pub next: *mut MethodEntryNode,
}

/// Method Entry (rb_method_entry_struct)
#[repr(C)]
#[derive(Debug, Clone)]
pub struct MethodEntry {
    /// Flags (visibility, etc.)
    pub flags: u64,
    /// Method definition
    pub def: *mut MethodDef,
    /// Original owner class
    pub owner: *const RClass,
    /// Called ID (may differ via alias)
    pub called_id: ID,
    /// Visibility
    pub vis: Visibility,
    /// Method serial for cache invalidation
    pub method_serial: u64,
}

/// Method Definition (rb_method_definition_struct)
#[repr(C)]
pub union MethodDef {
    pub iseq: ManuallyDrop<MethodDefIseq>,
    pub cfunc: ManuallyDrop<MethodDefCfunc>,
    pub alias: ManuallyDrop<MethodDefAlias>,
    pub refined: ManuallyDrop<MethodDefRefined>,
    pub attr: ManuallyDrop<MethodDefAttr>,
}

/// Ruby method definition (with iseq)
#[repr(C)]
pub struct MethodDefIseq {
    pub method_type: MethodType,
    /// Instruction sequence
    pub iseq: *mut Iseq,
    /// Local variable names
    pub local_table: *const *const u8,
    /// Default parameter values
    pub default_values: *const Value,
}

/// C function method definition
#[repr(C)]
pub struct MethodDefCfunc {
    pub method_type: MethodType,
    /// Function pointer
    pub func: *const c_void,
    /// Arity (-1 for variadic)
    pub arity: c_int,
}

/// Alias method definition
#[repr(C)]
pub struct MethodDefAlias {
    pub method_type: MethodType,
    /// Original method entry
    pub orig_me: *const MethodEntry,
}

/// Refined method definition
#[repr(C)]
pub struct MethodDefRefined {
    pub method_type: MethodType,
    /// Original method entry
    pub orig_me: *const MethodEntry,
    /// Refinement class
    pub refinement: *const RClass,
}

/// Attribute accessor definition
#[repr(C)]
pub struct MethodDefAttr {
    pub method_type: MethodType,
    /// Variable ID
    pub id: ID,
    /// Read or write
    pub read: bool,
}

/// =============================================================================
/// Iseq - Instruction Sequence
/// =============================================================================

#[repr(C)]
pub struct Iseq {
    /// Type marker
    pub type_: u32,
    /// Stack max size
    pub stack_max: c_uint,
    /// Local variable count
    pub local_table_size: c_uint,
    /// Instruction sequence size
    pub iseq_size: c_uint,
    /// Inline cache size
    pub ic_size: c_uint,
    /// Code
    pub iseq_encoded: *mut Value,
    /// Local variable names
    pub local_table: *const *const u8,
    /// Bytecode
    pub bytecode: *const u8,
}

/// =============================================================================
/// Instance Variable Table
/// =============================================================================

#[repr(C)]
pub struct IvTable {
    pub num_entries: usize,
    pub entries: *mut IvEntry,
}

#[repr(C)]
pub struct IvEntry {
    pub key: SymbolId,
    pub value: Value,
}

/// =============================================================================
/// Constant Table
/// =============================================================================

#[repr(C)]
pub struct ConstTable {
    pub num_entries: usize,
    pub entries: *mut ConstEntry,
}

#[repr(C)]
pub struct ConstEntry {
    pub key: SymbolId,
    pub value: Value,
    pub visibility: Visibility,
}

/// =============================================================================
/// Block/Proc structures
/// =============================================================================

/// rb_block_t - Block structure
#[repr(C)]
pub struct Block {
    /// Iseq pointer
    pub iseq: *const Iseq,
    /// Self when block was created
    pub self_: Value,
    /// Environment pointer (captured variables)
    pub ep: *const Value,
    /// Block flags (is_lambda, etc.)
    pub flags: u32,
    /// Proc object if converted
    pub proc: *mut Proc,
}

/// rb_proc_t - Proc structure
#[repr(C)]
pub struct Proc {
    pub basic: RBasic,
    pub block: Block,
    /// Created from method(:name)
    pub is_from_method: bool,
    /// Is lambda
    pub is_lambda: bool,
}

/// rb_binding_t - Binding structure
#[repr(C)]
pub struct Binding {
    pub basic: RBasic,
    /// Environment pointer
    pub ep: *const Value,
    /// Iseq context
    pub iseq: *const Iseq,
    /// Frame thisframe
    pub path: *const u8,
    /// Line number
    pub first_lineno: c_int,
}

/// Environment (for variable capture)
#[repr(C)]
pub struct Env {
    pub basic: RBasic,
    /// Size of environment
    pub env_size: c_uint,
    /// Number of local variables
    pub local_size: c_uint,
    /// Parent environment (for nested blocks)
    pub parent_env: *const Env,
    /// Environment variables
    pub env: *const Value,
}

/// =============================================================================
/// Method Object structures
/// =============================================================================

/// Method object (obj.method(:name))
#[repr(C)]
pub struct Method {
    pub basic: RBasic,
    /// Receiver object
    pub recv: Value,
    /// Method entry
    pub me: *const MethodEntry,
    /// Owner class
    pub owner: *const RClass,
    /// Defined class (for super)
    pub defined_class: *const RClass,
}

/// UnboundMethod object (Klass.instance_method(:name))
#[repr(C)]
pub struct UnboundMethod {
    pub basic: RBasic,
    /// Method entry
    pub me: *const MethodEntry,
    /// Owner class
    pub owner: *const RClass,
    /// Defined class
    pub defined_class: *const RClass,
}

/// =============================================================================
/// Special Values
/// =============================================================================

pub const QNIL: Value = 0x08;
pub const QTRUE: Value = 0x14;
pub const QFALSE: Value = 0x00;

/// Check if value is nil
pub fn nil_p(v: Value) -> bool {
    v == QNIL
}

/// Check if value is true
pub fn true_p(v: Value) -> bool {
    v == QTRUE
}

/// Check if value is false
pub fn false_p(v: Value) -> bool {
    v == QFALSE
}

/// Check if value is immediate (fixnum, symbol, true, false, nil)
pub fn immediate_p(v: Value) -> bool {
    (v & 0x07) != 0x00 || v == QFALSE
}

/// Check if value is a fixnum
pub fn fixnum_p(v: Value) -> bool {
    (v & 0x01) == 0x01
}

/// Check if value is a symbol
pub fn symbol_p(v: Value) -> bool {
    (v & 0x0f) == 0x0e
}

/// Convert fixnum VALUE to i64
pub fn fixnum_to_int(v: Value) -> i64 {
    ((v as i64) >> 1) as i64
}

/// Convert i64 to fixnum VALUE
pub fn int_to_fixnum(i: i64) -> Value {
    ((i << 1) | 0x01) as Value
}

//! Core trait abstractions for the MetaObject Protocol
//!
//! These traits define the uniform interface for all metaprogramming objects,
//! enabling factory patterns and DRY architecture.

use crate::types::*;

/// Uniform interface for all metaprogramming objects
pub trait MetaObject {
    /// Get the class of this object
    fn class(&self) -> ClassId;

    /// Get an instance variable by name
    fn ivar_get(&self, name: SymbolId) -> Option<Value>;

    /// Set an instance variable
    fn ivar_set(&mut self, name: SymbolId, value: Value);

    /// Get the singleton class of this object, creating it if necessary
    fn singleton_class(&mut self) -> ClassId;
}

/// Factory for creating MetaObjects
///
/// This trait defines the abstract factory interface. Concrete implementations
/// (like MRIMetaObjectFactory) provide MRI-compatible behavior.
pub trait MetaObjectFactory {
    type Block: BlockMeta;
    type Class: ClassMeta;
    type Module: ModuleMeta;
    type Proc: ProcMeta;
    type Method: MethodMeta;

    /// Create a block with the given parameters and body
    fn create_block(&self, params: BlockParams, body: BlockBody) -> Self::Block;

    /// Create a class with the given name and optional superclass
    fn create_class(&self, name: &str, superclass: Option<ClassId>) -> Self::Class;

    /// Create a module with the given name
    fn create_module(&self, name: &str) -> Self::Module;

    /// Create a proc from a block
    fn create_proc(&self, block: &Self::Block) -> Self::Proc;

    /// Create a lambda from a block
    fn create_lambda(&self, block: &Self::Block) -> Self::Proc;

    /// Create a method object
    fn create_method(&self, receiver: ObjectId, method_entry: *const MethodEntry) -> Self::Method;
}

/// Block-specific operations
pub trait BlockMeta: MetaObject {
    /// Yield to this block with arguments
    fn yield_values(&self, args: &[Value]) -> Value;

    /// Convert this block to a Proc
    fn to_proc(&self) -> ProcId;

    /// Check if this is a lambda (strict arity checking)
    fn is_lambda(&self) -> bool;

    /// Get the number of parameters
    fn arity(&self) -> i32;

    /// Get captured variables
    fn captures(&self) -> &[Value];
}

/// Class-specific operations
pub trait ClassMeta: MetaObject {
    /// Get the superclass
    fn superclass(&self) -> Option<ClassId>;

    /// Define a method on this class
    fn define_method(&mut self, name: SymbolId, method: MethodImpl);

    /// Undefine a method
    fn undef_method(&mut self, name: SymbolId);

    /// Remove a method
    fn remove_method(&mut self, name: SymbolId);

    /// Alias a method
    fn alias_method(&mut self, new_name: SymbolId, old_name: SymbolId);

    /// Include a module
    fn include(&mut self, module: ModuleId);

    /// Prepend a module
    fn prepend(&mut self, module: ModuleId);

    /// Get the method entry for a method name
    fn method_entry(&self, name: SymbolId) -> Option<*const MethodEntry>;

    /// Set method visibility
    fn set_visibility(&mut self, name: SymbolId, visibility: Visibility);

    /// Get instance method (for Module#instance_method)
    fn instance_method(&self, name: SymbolId) -> Option<*const MethodEntry>;
}

/// Module-specific operations
pub trait ModuleMeta: ClassMeta {
    /// Extend an object with this module's methods
    fn extend_object(&self, obj: ObjectId);

    /// Get the module function (combines instance and module methods)
    fn module_function(&self, name: SymbolId) -> Option<*const MethodEntry>;
}

/// Proc-specific operations
pub trait ProcMeta: MetaObject {
    /// Call this proc/lambda
    fn call(&self, args: &[Value]) -> Value;

    /// Get the underlying block
    fn block(&self) -> &dyn BlockMeta;

    /// Check if this is a lambda
    fn is_lambda(&self) -> bool;

    /// Get binding (for Proc#binding)
    fn binding(&self) -> BindingId;
}

/// Method object operations (for obj.method(:name))
pub trait MethodMeta: MetaObject {
    /// Call the method with a receiver and arguments
    fn call(&self, receiver: ObjectId, args: &[Value]) -> Value;

    /// Bind the method to an object (for UnboundMethod)
    fn bind(&self, obj: ObjectId) -> Box<dyn MethodMeta>;

    /// Get the original owner class
    fn owner(&self) -> ClassId;

    /// Get the method name
    fn name(&self) -> SymbolId;

    /// Get the method arity
    fn arity(&self) -> i32;

    /// Get method parameters info
    fn parameters(&self) -> Vec<MethodParam>;
}

/// Parameters for block creation
#[derive(Debug, Clone)]
pub struct BlockParams {
    pub params: Vec<ParamInfo>,
    pub is_lambda: bool,
    pub captures: Vec<String>,
}

/// Information about a parameter
#[derive(Debug, Clone)]
pub struct ParamInfo {
    pub name: String,
    pub kind: ParamKind,
    pub default: Option<Value>,
}

/// Kinds of parameters
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamKind {
    Required,
    Optional,
    Rest,
    Keyword,
    KeywordRest,
    Block,
}

/// Block body representation
#[derive(Debug, Clone)]
pub struct BlockBody {
    pub instructions: Vec<u8>, // Compiled bytecode
    pub local_count: usize,
    pub stack_size: usize,
}

/// Method implementation variants
#[derive(Debug, Clone)]
pub enum MethodImpl {
    /// Ruby method (iseq)
    Ruby { bytecode: Vec<u8> },
    /// Native C function
    CFunc { ptr: *const (), arity: i32 },
    /// Alias to another method
    Alias { original: SymbolId },
    /// Refinement
    Refined { original: *const MethodEntry, refinement: ClassId },
}

/// Method parameter metadata
#[derive(Debug, Clone)]
pub struct MethodParam {
    pub name: String,
    pub kind: ParamKind,
}

//! MetaObject Factory Pattern Implementation
//!
//! Provides MRI-compatible factory for creating metaprogramming objects.

use crate::traits::*;
use crate::types::*;
use std::collections::HashMap;
use std::os::raw::c_void;
use std::mem::ManuallyDrop;

/// MRI-compatible MetaObject Factory
#[allow(dead_code)]
pub struct MRIMetaObjectFactory {
    /// Global class serial counter
    class_serial: u64,
    /// Method serial counter for cache invalidation
    method_serial: u64,
}

impl MRIMetaObjectFactory {
    pub fn new() -> Self {
        Self {
            class_serial: 0,
            method_serial: 0,
        }
    }

    #[allow(dead_code)]
    fn next_class_serial(&mut self) -> u64 {
        self.class_serial += 1;
        self.class_serial
    }

    #[allow(dead_code)]
    fn next_method_serial(&mut self) -> u64 {
        self.method_serial += 1;
        self.method_serial
    }
}

impl Default for MRIMetaObjectFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl MetaObjectFactory for MRIMetaObjectFactory {
    type Block = MRIBlock;
    type Class = MRIClass;
    type Module = MRIModule;
    type Proc = MRIProc;
    type Method = MRIMethod;

    fn create_block(&self, params: BlockParams, body: BlockBody) -> Self::Block {
        MRIBlock {
            params,
            body,
            iseq_ptr: std::ptr::null(),
            captured_vars: Vec::new(),
            self_capture: None,
        }
    }

    fn create_class(&self, name: &str, superclass: Option<ClassId>) -> Self::Class {
        MRIClass {
            name: name.to_string(),
            superclass,
            methods: HashMap::new(),
            ivars: HashMap::new(),
            constants: HashMap::new(),
            included_modules: Vec::new(),
            prepended_modules: Vec::new(),
            singleton_class: None,
            class_serial: self.class_serial + 1,
        }
    }

    fn create_module(&self, name: &str) -> Self::Module {
        MRIModule {
            name: name.to_string(),
            methods: HashMap::new(),
            ivars: HashMap::new(),
            constants: HashMap::new(),
            included_in: Vec::new(),
            extended_in: Vec::new(),
            singleton_class: None,
        }
    }

    fn create_proc(&self, block: &Self::Block) -> Self::Proc {
        MRIProc {
            block: MRIBlock {
                params: block.params.clone(),
                body: block.body.clone(),
                iseq_ptr: block.iseq_ptr,
                captured_vars: block.captured_vars.clone(),
                self_capture: block.self_capture,
            },
            is_lambda: false,
            binding_id: 0,
            is_from_method: false,
        }
    }

    fn create_lambda(&self, block: &Self::Block) -> Self::Proc {
        MRIProc {
            block: MRIBlock {
                params: block.params.clone(),
                body: block.body.clone(),
                iseq_ptr: block.iseq_ptr,
                captured_vars: block.captured_vars.clone(),
                self_capture: block.self_capture,
            },
            is_lambda: true,
            binding_id: 0,
            is_from_method: false,
        }
    }

    fn create_method(&self, receiver: ObjectId, method_entry: *const MethodEntry) -> Self::Method {
        MRIMethod {
            receiver,
            method_entry,
            is_unbound: false,
        }
    }
}

/// MRI Block implementation
#[derive(Debug, Clone)]
pub struct MRIBlock {
    pub params: BlockParams,
    pub body: BlockBody,
    pub iseq_ptr: *const Iseq,
    pub captured_vars: Vec<Value>,
    pub self_capture: Option<ObjectId>,
}

impl BlockMeta for MRIBlock {
    fn yield_values(&self, _args: &[Value]) -> Value {
        // In real implementation, this would execute the block
        QNIL
    }

    fn to_proc(&self) -> ProcId {
        // Create a proc from this block
        0
    }

    fn is_lambda(&self) -> bool {
        self.params.is_lambda
    }

    fn arity(&self) -> i32 {
        self.params.params.len() as i32
    }

    fn captures(&self) -> &[Value] {
        &self.captured_vars
    }
}

impl MetaObject for MRIBlock {
    fn class(&self) -> ClassId {
        // Block class
        0
    }

    fn ivar_get(&self, _name: SymbolId) -> Option<Value> {
        None
    }

    fn ivar_set(&mut self, _name: SymbolId, _value: Value) {}

    fn singleton_class(&mut self) -> ClassId {
        0
    }
}

/// MRI Class implementation
#[derive(Debug, Clone)]
pub struct MRIClass {
    pub name: String,
    pub superclass: Option<ClassId>,
    pub methods: HashMap<SymbolId, MethodEntry>,
    pub ivars: HashMap<SymbolId, Value>,
    pub constants: HashMap<SymbolId, (Value, Visibility)>,
    pub included_modules: Vec<ModuleId>,
    pub prepended_modules: Vec<ModuleId>,
    pub singleton_class: Option<ClassId>,
    pub class_serial: u64,
}

impl ClassMeta for MRIClass {
    fn superclass(&self) -> Option<ClassId> {
        self.superclass
    }

    fn define_method(&mut self, name: SymbolId, method: MethodImpl) {
        let entry = MethodEntry {
            flags: 0,
            def: Box::into_raw(Box::new(create_method_def(method))),
            owner: std::ptr::null(),
            called_id: name,
            vis: Visibility::Public,
            method_serial: self.class_serial,
        };
        self.methods.insert(name, entry);
    }

    fn undef_method(&mut self, name: SymbolId) {
        self.methods.remove(&name);
    }

    fn remove_method(&mut self, name: SymbolId) {
        self.methods.remove(&name);
    }

    fn alias_method(&mut self, new_name: SymbolId, old_name: SymbolId) {
        if let Some(old_entry) = self.methods.get(&old_name).cloned() {
            let new_entry = MethodEntry {
                flags: old_entry.flags,
                def: old_entry.def,
                owner: old_entry.owner,
                called_id: new_name,
                vis: old_entry.vis,
                method_serial: old_entry.method_serial,
            };
            self.methods.insert(new_name, new_entry);
        }
    }

    fn include(&mut self, module: ModuleId) {
        if !self.included_modules.contains(&module) {
            self.included_modules.push(module);
        }
    }

    fn prepend(&mut self, module: ModuleId) {
        if !self.prepended_modules.contains(&module) {
            self.prepended_modules.push(module);
        }
    }

    fn method_entry(&self, name: SymbolId) -> Option<*const MethodEntry> {
        self.methods.get(&name).map(|e| e as *const MethodEntry)
    }

    fn set_visibility(&mut self, name: SymbolId, visibility: Visibility) {
        if let Some(entry) = self.methods.get_mut(&name) {
            entry.vis = visibility;
        }
    }

    fn instance_method(&self, name: SymbolId) -> Option<*const MethodEntry> {
        self.method_entry(name)
    }
}

impl MetaObject for MRIClass {
    fn class(&self) -> ClassId {
        // Class class
        0
    }

    fn ivar_get(&self, name: SymbolId) -> Option<Value> {
        self.ivars.get(&name).copied()
    }

    fn ivar_set(&mut self, name: SymbolId, value: Value) {
        self.ivars.insert(name, value);
    }

    fn singleton_class(&mut self) -> ClassId {
        self.singleton_class.unwrap_or(0)
    }
}

/// MRI Module implementation
#[derive(Debug, Clone)]
pub struct MRIModule {
    pub name: String,
    pub methods: HashMap<SymbolId, MethodEntry>,
    pub ivars: HashMap<SymbolId, Value>,
    pub constants: HashMap<SymbolId, (Value, Visibility)>,
    pub included_in: Vec<ClassId>,
    pub extended_in: Vec<ObjectId>,
    pub singleton_class: Option<ClassId>,
}

impl ClassMeta for MRIModule {
    fn superclass(&self) -> Option<ClassId> {
        None // Modules don't have superclass
    }

    fn define_method(&mut self, name: SymbolId, method: MethodImpl) {
        let entry = MethodEntry {
            flags: 0,
            def: Box::into_raw(Box::new(create_method_def(method))),
            owner: std::ptr::null(),
            called_id: name,
            vis: Visibility::Public,
            method_serial: 0,
        };
        self.methods.insert(name, entry);
    }

    fn undef_method(&mut self, name: SymbolId) {
        self.methods.remove(&name);
    }

    fn remove_method(&mut self, name: SymbolId) {
        self.methods.remove(&name);
    }

    fn alias_method(&mut self, new_name: SymbolId, old_name: SymbolId) {
        if let Some(old_entry) = self.methods.get(&old_name).cloned() {
            let new_entry = MethodEntry {
                flags: old_entry.flags,
                def: old_entry.def,
                owner: old_entry.owner,
                called_id: new_name,
                vis: old_entry.vis,
                method_serial: old_entry.method_serial,
            };
            self.methods.insert(new_name, new_entry);
        }
    }

    fn include(&mut self, _module: ModuleId) {
        // Modules can include other modules
    }

    fn prepend(&mut self, _module: ModuleId) {
        // Modules can be prepended
    }

    fn method_entry(&self, name: SymbolId) -> Option<*const MethodEntry> {
        self.methods.get(&name).map(|e| e as *const MethodEntry)
    }

    fn set_visibility(&mut self, name: SymbolId, visibility: Visibility) {
        if let Some(entry) = self.methods.get_mut(&name) {
            entry.vis = visibility;
        }
    }

    fn instance_method(&self, name: SymbolId) -> Option<*const MethodEntry> {
        self.method_entry(name)
    }
}

impl ModuleMeta for MRIModule {
    fn extend_object(&self, _obj: ObjectId) {
        // Add module's methods to obj's singleton class
    }

    fn module_function(&self, name: SymbolId) -> Option<*const MethodEntry> {
        self.methods.get(&name).map(|e| e as *const MethodEntry)
    }
}

impl MetaObject for MRIModule {
    fn class(&self) -> ClassId {
        // Module class
        0
    }

    fn ivar_get(&self, name: SymbolId) -> Option<Value> {
        self.ivars.get(&name).copied()
    }

    fn ivar_set(&mut self, name: SymbolId, value: Value) {
        self.ivars.insert(name, value);
    }

    fn singleton_class(&mut self) -> ClassId {
        self.singleton_class.unwrap_or(0)
    }
}

/// MRI Proc implementation
#[derive(Debug, Clone)]
pub struct MRIProc {
    pub block: MRIBlock,
    pub is_lambda: bool,
    pub binding_id: BindingId,
    pub is_from_method: bool,
}

impl ProcMeta for MRIProc {
    fn call(&self, args: &[Value]) -> Value {
        self.block.yield_values(args)
    }

    fn block(&self) -> &dyn BlockMeta {
        &self.block
    }

    fn is_lambda(&self) -> bool {
        self.is_lambda
    }

    fn binding(&self) -> BindingId {
        self.binding_id
    }
}

impl MetaObject for MRIProc {
    fn class(&self) -> ClassId {
        // Proc class
        0
    }

    fn ivar_get(&self, _name: SymbolId) -> Option<Value> {
        None
    }

    fn ivar_set(&mut self, _name: SymbolId, _value: Value) {}

    fn singleton_class(&mut self) -> ClassId {
        0
    }
}

/// MRI Method object implementation
#[derive(Debug, Clone)]
pub struct MRIMethod {
    pub receiver: ObjectId,
    pub method_entry: *const MethodEntry,
    pub is_unbound: bool,
}

impl MethodMeta for MRIMethod {
    fn call(&self, _receiver: ObjectId, _args: &[Value]) -> Value {
        // Execute method
        QNIL
    }

    fn bind(&self, obj: ObjectId) -> Box<dyn MethodMeta> {
        Box::new(MRIMethod {
            receiver: obj,
            method_entry: self.method_entry,
            is_unbound: false,
        })
    }

    fn owner(&self) -> ClassId {
        // Get owner from method entry
        0
    }

    fn name(&self) -> SymbolId {
        // Get name from method entry
        0
    }

    fn arity(&self) -> i32 {
        // Get arity from method entry
        -1
    }

    fn parameters(&self) -> Vec<MethodParam> {
        Vec::new()
    }
}

impl MetaObject for MRIMethod {
    fn class(&self) -> ClassId {
        // Method class
        0
    }

    fn ivar_get(&self, _name: SymbolId) -> Option<Value> {
        None
    }

    fn ivar_set(&mut self, _name: SymbolId, _value: Value) {}

    fn singleton_class(&mut self) -> ClassId {
        0
    }
}

/// Helper function to create MethodDef union
fn create_method_def(impl_: MethodImpl) -> MethodDef {
    match impl_ {
        MethodImpl::Ruby { .. } => MethodDef {
            iseq: ManuallyDrop::new(MethodDefIseq {
                method_type: MethodType::Iseq,
                iseq: std::ptr::null_mut(),
                local_table: std::ptr::null(),
                default_values: std::ptr::null(),
            }),
        },
        MethodImpl::CFunc { ptr, arity } => MethodDef {
            cfunc: ManuallyDrop::new(MethodDefCfunc {
                method_type: MethodType::Cfunc,
                func: ptr as *const c_void,
                arity,
            }),
        },
        MethodImpl::Alias { .. } => MethodDef {
            alias: ManuallyDrop::new(MethodDefAlias {
                method_type: MethodType::Alias,
                orig_me: std::ptr::null(),
            }),
        },
        MethodImpl::Refined { .. } => MethodDef {
            refined: ManuallyDrop::new(MethodDefRefined {
                method_type: MethodType::Refine,
                orig_me: std::ptr::null(),
                refinement: std::ptr::null(),
            }),
        },
    }
}

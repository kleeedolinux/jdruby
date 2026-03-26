//! # Class and Module System
//!
//! MRI-compatible class/module hierarchy with method tables,
//! constant lookup, and include/prepend mechanics.

use std::collections::HashMap;
#[cfg(test)]
use std::ptr::null_mut;
use jdruby_common::ffi_types::{VALUE, ID};
use crate::object::RClass;

// ═════════════════════════════════════════════════════════════════════════════
// Method Entry - MRI-compatible method representation
// ═════════════════════════════════════════════════════════════════════════════

/// Method type - distinguishes between different method implementations
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MethodType {
    Iseq = 0, CFunc = 1, CFunc2 = 2, Jit = 3,
    Alias = 4, Refined = 5, Undefined = 6, Zsuper = 7, Missing = 8,
}

/// Method visibility
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MethodVisibility {
    Public = 0, Private = 1, Protected = 2,
}

/// Method entry - represents a single method in a class/module
#[repr(C)]
pub struct MethodEntry {
    pub flags: u32,
    pub visibility: MethodVisibility,
    pub method_type: MethodType,
    pub def: MethodDef,
    pub defined_class: *mut RClass,
    pub serial: u64,
}

/// Method definition union
#[repr(C)]
pub union MethodDef {
    /// C function pointer (wrapped in Option for safe initialization)
    pub cfunc: Option<unsafe extern "C" fn(VALUE, ...) -> VALUE>,
    /// JIT-compiled function pointer
    pub jit_fn: *mut u8,
    /// Alias target (method name ID)
    pub alias: ID,
    /// Ruby bytecode (iseq)
    pub iseq: *mut u8,
}

/// Method table - maps method names to method entries
pub struct MethodTable {
    entries: HashMap<ID, Box<MethodEntry>>,
}

impl MethodTable {
    pub fn new() -> Self { Self { entries: HashMap::new() } }
    pub fn define(&mut self, mid: ID, entry: Box<MethodEntry>) -> Option<Box<MethodEntry>> {
        self.entries.insert(mid, entry)
    }
    pub fn lookup(&self, mid: ID) -> Option<&MethodEntry> {
        self.entries.get(&mid).map(|e| e.as_ref())
    }
    pub fn remove(&mut self, mid: ID) -> Option<Box<MethodEntry>> {
        self.entries.remove(&mid)
    }
    pub fn contains(&self, mid: ID) -> bool { self.entries.contains_key(&mid) }
}

impl Default for MethodTable {
    fn default() -> Self { Self::new() }
}

// ═════════════════════════════════════════════════════════════════════════════
// Constant Entry
// ═════════════════════════════════════════════════════════════════════════════

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConstVisibility { Public = 0, Private = 1 }

#[repr(C)]
pub struct ConstEntry {
    pub value: VALUE,
    pub visibility: ConstVisibility,
    pub defined_class: *mut RClass,
}

pub struct ConstTable {
    entries: HashMap<ID, Box<ConstEntry>>,
}

impl ConstTable {
    pub fn new() -> Self { Self { entries: HashMap::new() } }
    pub fn define(&mut self, cid: ID, value: VALUE, visibility: ConstVisibility, defined_class: *mut RClass) {
        let entry = Box::new(ConstEntry { value, visibility, defined_class });
        self.entries.insert(cid, entry);
    }
    pub fn lookup(&self, cid: ID) -> Option<&ConstEntry> {
        self.entries.get(&cid).map(|e| e.as_ref())
    }
}

impl Default for ConstTable {
    fn default() -> Self { Self::new() }
}

// ═════════════════════════════════════════════════════════════════════════════
// Class Hierarchy
// ═════════════════════════════════════════════════════════════════════════════

pub struct ClassHierarchy {
    classes: HashMap<u64, ClassData>,
    next_id: u64,
}

pub struct ClassData {
    pub id: u64,
    pub name: String,
    pub superclass_id: Option<u64>,
    pub included_modules: Vec<u64>,
    pub prepended_modules: Vec<u64>,
    pub method_table: MethodTable,
    pub singleton_method_table: MethodTable,
    pub const_table: ConstTable,
    pub ivar_table: HashMap<ID, VALUE>,
    pub is_module: bool,
    pub is_frozen: bool,
}

impl ClassData {
    pub fn new(id: u64, name: String, superclass_id: Option<u64>, is_module: bool) -> Self {
        Self {
            id, name, superclass_id,
            included_modules: Vec::new(),
            prepended_modules: Vec::new(),
            method_table: MethodTable::new(),
            singleton_method_table: MethodTable::new(),
            const_table: ConstTable::new(),
            ivar_table: HashMap::new(),
            is_module, is_frozen: false,
        }
    }
    pub fn include_module(&mut self, module_id: u64) {
        if !self.included_modules.contains(&module_id) {
            self.included_modules.push(module_id);
        }
    }
    pub fn prepend_module(&mut self, module_id: u64) {
        if !self.prepended_modules.contains(&module_id) {
            self.prepended_modules.push(module_id);
        }
    }
}

impl ClassHierarchy {
    pub fn new() -> Self {
        let mut hierarchy = Self { classes: HashMap::new(), next_id: 1 };
        hierarchy.init_builtins();
        hierarchy
    }
    
    fn init_builtins(&mut self) {
        let basic_object = self.define_class("BasicObject", None, false);
        let object = self.define_class("Object", Some(basic_object), false);
        let _kernel = self.define_class("Kernel", Some(object), false);
        
        for name in ["Class", "Module", "NilClass", "TrueClass", "FalseClass",
                     "Integer", "Float", "String", "Symbol", "Array", "Hash",
                     "Regexp", "Range", "Proc", "IO", "File", "Dir", 
                     "Time", "Thread", "Fiber", "Mutex", "Exception"]
        {
            self.define_class(name, Some(object), false);
        }
    }
    
    pub fn define_class(&mut self, name: &str, superclass_id: Option<u64>, is_module: bool) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let class_data = ClassData::new(id, name.to_string(), superclass_id, is_module);
        self.classes.insert(id, class_data);
        id
    }
    
    pub fn get(&self, id: u64) -> Option<&ClassData> { self.classes.get(&id) }
    pub fn get_mut(&mut self, id: u64) -> Option<&mut ClassData> { self.classes.get_mut(&id) }
    
    /// Method lookup with proper hierarchy (prepends -> class -> includes -> super)
    pub fn lookup_method(&self, class_id: u64, mid: ID) -> Option<(&MethodEntry, u64)> {
        let mut visited = std::collections::HashSet::new();
        self.lookup_method_recursive(class_id, mid, &mut visited)
    }
    
    fn lookup_method_recursive(&self, class_id: u64, mid: ID, visited: &mut std::collections::HashSet<u64>) -> Option<(&MethodEntry, u64)> {
        if !visited.insert(class_id) { return None; }
        let class = self.classes.get(&class_id)?;
        
        // 1. Check prepended modules
        for &module_id in class.prepended_modules.iter().rev() {
            if let Some(result) = self.lookup_method_recursive(module_id, mid, visited) {
                return Some(result);
            }
        }
        // 2. Check class method table
        if let Some(entry) = class.method_table.lookup(mid) {
            return Some((entry, class_id));
        }
        // 3. Check included modules
        for &module_id in &class.included_modules {
            if let Some(result) = self.lookup_method_recursive(module_id, mid, visited) {
                return Some(result);
            }
        }
        // 4. Check superclass
        if let Some(super_id) = class.superclass_id {
            return self.lookup_method_recursive(super_id, mid, visited);
        }
        None
    }
    
    /// Constant lookup with lexical and inheritance chain
    pub fn lookup_const(&self, class_id: u64, cid: ID) -> Option<&ConstEntry> {
        let mut visited = std::collections::HashSet::new();
        self.lookup_const_recursive(class_id, cid, &mut visited)
    }
    
    fn lookup_const_recursive(&self, class_id: u64, cid: ID, visited: &mut std::collections::HashSet<u64>) -> Option<&ConstEntry> {
        if !visited.insert(class_id) { return None; }
        let class = self.classes.get(&class_id)?;
        if let Some(entry) = class.const_table.lookup(cid) { return Some(entry); }
        for &module_id in class.included_modules.iter().rev() {
            if let Some(result) = self.lookup_const_recursive(module_id, cid, visited) {
                return Some(result);
            }
        }
        if let Some(super_id) = class.superclass_id {
            return self.lookup_const_recursive(super_id, cid, visited);
        }
        None
    }
    
    pub fn superclass_of(&self, class_id: u64) -> Option<u64> {
        self.classes.get(&class_id)?.superclass_id
    }
    
    pub fn is_subclass(&self, class_id: u64, ancestor_id: u64) -> bool {
        let mut current = Some(class_id);
        while let Some(id) = current {
            if id == ancestor_id { return true; }
            current = self.superclass_of(id);
        }
        false
    }
}

impl Default for ClassHierarchy {
    fn default() -> Self { Self::new() }
}

// ═════════════════════════════════════════════════════════════════════════════
// Symbol Table
// ═════════════════════════════════════════════════════════════════════════════

pub struct SymbolTable {
    by_name: HashMap<String, ID>,
    by_id: Vec<String>,
}

impl SymbolTable {
    pub fn new() -> Self {
        Self { by_name: HashMap::new(), by_id: Vec::new() }
    }
    pub fn intern(&mut self, name: &str) -> ID {
        if let Some(&id) = self.by_name.get(name) { return id; }
        let id = self.by_id.len() + 1;
        let owned = name.to_string();
        self.by_name.insert(owned.clone(), id);
        self.by_id.push(owned);
        id
    }
    pub fn name_of(&self, id: ID) -> Option<&str> {
        self.by_id.get(id.saturating_sub(1)).map(|s| s.as_str())
    }
    pub fn count(&self) -> usize { self.by_id.len() }
}

impl Default for SymbolTable {
    fn default() -> Self { Self::new() }
}

// ═════════════════════════════════════════════════════════════════════════════
// Tests
// ═════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_class_hierarchy_creation() {
        let hierarchy = ClassHierarchy::new();
        let basic_object = hierarchy.get(1);
        assert!(basic_object.is_some());
        assert_eq!(basic_object.unwrap().name, "BasicObject");
        assert!(basic_object.unwrap().superclass_id.is_none());
    }
    
    #[test]
    fn test_method_lookup() {
        let mut hierarchy = ClassHierarchy::new();
        let object_id = hierarchy.get(2).unwrap().id;
        let test_class = hierarchy.define_class("TestClass", Some(object_id), false);
        let mid = 1;
        let entry = Box::new(MethodEntry {
            flags: 0, visibility: MethodVisibility::Public, method_type: MethodType::CFunc,
            def: MethodDef { cfunc: None },
            defined_class: null_mut(), serial: 1,
        });
        hierarchy.get_mut(test_class).unwrap().method_table.define(mid, entry);
        let (found, class_id) = hierarchy.lookup_method(test_class, mid).unwrap();
        assert_eq!(class_id, test_class);
        assert_eq!(found.visibility, MethodVisibility::Public);
    }
    
    #[test]
    fn test_method_inheritance() {
        let mut hierarchy = ClassHierarchy::new();
        let object_id = hierarchy.get(2).unwrap().id;
        let parent = hierarchy.define_class("Parent", Some(object_id), false);
        let mid = 42;
        let entry = Box::new(MethodEntry {
            flags: 0, visibility: MethodVisibility::Public, method_type: MethodType::CFunc,
            def: MethodDef { cfunc: None },
            defined_class: null_mut(), serial: 1,
        });
        hierarchy.get_mut(parent).unwrap().method_table.define(mid, entry);
        let child = hierarchy.define_class("Child", Some(parent), false);
        let (_found, class_id) = hierarchy.lookup_method(child, mid).unwrap();
        assert_eq!(class_id, parent);
    }
    
    #[test]
    fn test_symbol_table() {
        let mut table = SymbolTable::new();
        let id1 = table.intern("foo");
        let id2 = table.intern("bar");
        let id3 = table.intern("foo");
        assert_eq!(id1, id3);
        assert_ne!(id1, id2);
        assert_eq!(table.name_of(id1), Some("foo"));
        assert_eq!(table.count(), 2);
    }
    
    #[test]
    fn test_module_include() {
        let mut hierarchy = ClassHierarchy::new();
        let object_id = hierarchy.get(2).unwrap().id;
        let module = hierarchy.define_class("MyModule", Some(object_id), true);
        let mid = 99;
        let entry = Box::new(MethodEntry {
            flags: 0, visibility: MethodVisibility::Public, method_type: MethodType::CFunc,
            def: MethodDef { cfunc: None },
            defined_class: null_mut(), serial: 1,
        });
        hierarchy.get_mut(module).unwrap().method_table.define(mid, entry);
        let klass = hierarchy.define_class("MyClass", Some(object_id), false);
        hierarchy.get_mut(klass).unwrap().include_module(module);
        let (_found, class_id) = hierarchy.lookup_method(klass, mid).unwrap();
        assert_eq!(class_id, module);
    }
    
    #[test]
    fn test_constant_lookup() {
        let mut hierarchy = ClassHierarchy::new();
        let object_id = hierarchy.get(2).unwrap().id;
        let klass = hierarchy.define_class("Test", Some(object_id), false);
        let cid = 1;
        let value = 42usize;
        hierarchy.get_mut(klass).unwrap().const_table.define(cid, value, ConstVisibility::Public, null_mut());
        let entry = hierarchy.lookup_const(klass, cid).unwrap();
        assert_eq!(entry.value, value);
    }
    
    #[test]
    fn test_is_subclass() {
        let hierarchy = ClassHierarchy::new();
        let basic_object_id = 1;
        let object_id = 2;
        assert!(hierarchy.is_subclass(object_id, basic_object_id));
        assert!(!hierarchy.is_subclass(basic_object_id, object_id));
    }
}

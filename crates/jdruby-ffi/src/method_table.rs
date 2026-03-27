//! # Method Table — Global Registry for C-Extension Methods
//!
//! When a C-extension calls `rb_define_method(klass, name, func, arity)`,
//! we store the function pointer + arity here. When `rb_funcall` is called,
//! we look up the target method in this table and dispatch through the C ABI.

use std::collections::HashMap;
use std::sync::Mutex;
use crate::value::{VALUE, ID, RUBY_QNIL};

/// A C-extension method function pointer.
///
/// In MRI, methods can have different arities:
/// - `arity >= 0`: fixed args → `VALUE func(VALUE self, VALUE arg1, ...)`
/// - `arity == -1`: variadic  → `VALUE func(int argc, VALUE *argv, VALUE self)`
/// - `arity == -2`: Ruby args → `VALUE func(VALUE self, VALUE args_array)`
pub type CMethodFn = unsafe extern "C" fn() -> VALUE;

/// A registered method entry.
#[derive(Clone)]
pub struct MethodEntry {
    /// The C function pointer.
    pub func: usize, // stored as usize for Send+Sync
    /// Expected arity.
    pub arity: i32,
    /// Method name.
    pub name: String,
    /// The class VALUE this method belongs to.
    pub klass: VALUE,
}

/// The global method table.
static METHOD_TABLE: Mutex<Option<MethodTable>> = Mutex::new(None);

pub struct MethodTable {
    /// (class_id, method_name) → MethodEntry
    methods: HashMap<(VALUE, String), MethodEntry>,
    /// Symbol table: name → ID
    symbols: HashMap<String, ID>,
    /// Reverse symbol table: ID → name
    sym_names: HashMap<ID, String>,
    /// Next symbol ID
    next_sym_id: ID,
    /// Class hierarchy: class → superclass
    class_hierarchy: HashMap<VALUE, VALUE>,
    /// Class names: VALUE → name
    class_names: HashMap<VALUE, String>,
    /// Next class ID
    next_class_id: VALUE,
    /// Instance variables: (object, name) → value
    instance_vars: HashMap<(VALUE, String), VALUE>,
    /// Hash storage: (hash_id, key) → value
    hash_storage: HashMap<(VALUE, VALUE), VALUE>,
    /// Constants: (class, name) → value
    constants: HashMap<(VALUE, String), VALUE>,
}

impl MethodTable {
    fn new() -> Self {
        let mut tbl = Self {
            methods: HashMap::new(),
            symbols: HashMap::new(),
            sym_names: HashMap::new(),
            next_sym_id: 1,
            class_hierarchy: HashMap::new(),
            class_names: HashMap::new(),
            next_class_id: 0x1_0000, // class IDs start high to avoid tag collisions
            instance_vars: HashMap::new(),
            hash_storage: HashMap::new(),
            constants: HashMap::new(),
        };
        // Pre-intern common symbols
        for name in ["initialize", "new", "to_s", "inspect", "class",
                      "puts", "print", "p", "raise", "require",
                      "==", "!=", "<", ">", "<=", ">=", "<=>",
                      "+", "-", "*", "/", "%", "**",
                      "[]", "[]=", "<<", "each", "map", "select",
                      "length", "size", "freeze", "frozen?",
                      "nil?", "respond_to?", "send", "method_missing"] {
            tbl.intern(name);
        }
        tbl
    }

    /// Intern a string as a symbol ID.
    pub fn intern(&mut self, name: &str) -> ID {
        if let Some(&id) = self.symbols.get(name) {
            return id;
        }
        let id = self.next_sym_id;
        self.next_sym_id += 1;
        self.symbols.insert(name.to_string(), id);
        self.sym_names.insert(id, name.to_string());
        id
    }

    /// Look up the name of a symbol ID.
    pub fn id2name(&self, id: ID) -> Option<&str> {
        self.sym_names.get(&id).map(|s| s.as_str())
    }

    /// Register a method.
    pub fn define_method(&mut self, klass: VALUE, name: &str, func: usize, arity: i32) {
        let entry = MethodEntry {
            func,
            arity,
            name: name.to_string(),
            klass,
        };
        self.methods.insert((klass, name.to_string()), entry);
    }

    /// Look up a method, walking the class hierarchy.
    pub fn lookup_method(&self, klass: VALUE, name: &str) -> Option<&MethodEntry> {
        let mut current = klass;
        loop {
            if let Some(entry) = self.methods.get(&(current, name.to_string())) {
                return Some(entry);
            }
            // Walk up the superclass chain
            match self.class_hierarchy.get(&current) {
                Some(&super_klass) if super_klass != 0 => {
                    current = super_klass;
                }
                _ => break,
            }
        }
        None
    }

    /// Create a new class, returning its VALUE.
    pub fn define_class(&mut self, name: &str, superclass: VALUE) -> VALUE {
        let id = self.next_class_id;
        self.next_class_id += 8; // Keep aligned
        self.class_hierarchy.insert(id, superclass);
        self.class_names.insert(id, name.to_string());
        id
    }

/// Get a class VALUE by name.
    pub fn class_by_name(&self, name: &str) -> Option<VALUE> {
        self.class_names.iter()
            .find(|(_, n)| n.as_str() == name)
            .map(|(&v, _)| v)
    }

    /// Get instance variable from object
    pub fn get_ivar(&self, obj: VALUE, name: &str) -> VALUE {
        self.instance_vars.get(&(obj, name.to_string())).copied().unwrap_or(RUBY_QNIL)
    }

    /// Set instance variable on object
    pub fn set_ivar(&mut self, obj: VALUE, name: &str, val: VALUE) {
        self.instance_vars.insert((obj, name.to_string()), val);
    }

    /// Hash operations: set key-value pair
    pub fn hash_aset(&mut self, hash: VALUE, key: VALUE, val: VALUE) {
        self.hash_storage.insert((hash, key), val);
    }

    /// Hash operations: get value by key
    pub fn hash_aref(&self, hash: VALUE, key: VALUE) -> VALUE {
        self.hash_storage.get(&(hash, key)).copied().unwrap_or(RUBY_QNIL)
    }

    /// Set a constant on a class/module
    pub fn set_constant(&mut self, klass: VALUE, name: &str, val: VALUE) {
        self.constants.insert((klass, name.to_string()), val);
    }

    /// Look up a constant
    pub fn lookup_constant(&self, klass: VALUE, name: &str) -> Option<VALUE> {
        // First check in the class itself
        if let Some(&val) = self.constants.get(&(klass, name.to_string())) {
            return Some(val);
        }
        // Walk up the class hierarchy
        let mut current = klass;
        loop {
            if let Some(&val) = self.constants.get(&(current, name.to_string())) {
                return Some(val);
            }
            match self.class_hierarchy.get(&current) {
                Some(&super_klass) if super_klass != 0 => {
                    current = super_klass;
                }
                _ => break,
            }
        }
        None
    }

    /// Check if a constant is defined
    pub fn constant_defined(&self, klass: VALUE, name: &str) -> bool {
        self.lookup_constant(klass, name).is_some()
    }
}

/// Access the global method table.
pub fn with_method_table<F, R>(f: F) -> R
where
    F: FnOnce(&mut MethodTable) -> R,
{
    let mut guard = METHOD_TABLE.lock().unwrap();
    let tbl = guard.get_or_insert_with(MethodTable::new);
    f(tbl)
}

/// Intern a symbol name → ID.
pub fn rb_intern_str(name: &str) -> ID {
    with_method_table(|tbl| tbl.intern(name))
}

/// Get the name of a symbol ID.
pub fn rb_id2name_str(id: ID) -> Option<String> {
    with_method_table(|tbl| tbl.id2name(id).map(|s| s.to_string()))
}

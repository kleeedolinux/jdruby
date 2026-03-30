//! # Class Table — Class Hierarchy and Name Tracking
//!
//! Stores class definitions, superclasses, and name-to-class lookups.

use std::collections::HashMap;
use std::sync::RwLock;
use crate::core::VALUE;

/// Global class table instance.
static CLASS_TABLE: RwLock<Option<ClassTable>> = RwLock::new(None);

/// The class table stores class hierarchy and naming information.
pub struct ClassTable {
    /// Class VALUE → superclass VALUE
    hierarchy: HashMap<VALUE, VALUE>,
    /// Class VALUE → class name
    names: HashMap<VALUE, String>,
    /// Class name → class VALUE (reverse lookup)
    name_to_class: HashMap<String, VALUE>,
    /// Class VALUE → singleton class VALUE
    singleton_classes: HashMap<VALUE, VALUE>,
    /// Next available class ID
    next_class_id: VALUE,
}

impl ClassTable {
    fn new() -> Self {
        Self {
            hierarchy: HashMap::new(),
            names: HashMap::new(),
            name_to_class: HashMap::new(),
            singleton_classes: HashMap::new(),
            next_class_id: 0x1_0000, // class IDs start high to avoid tag collisions
        }
    }

    /// Define a new class with the given name and superclass.
    pub fn define_class(&mut self, name: &str, superclass: VALUE) -> VALUE {
        let id = self.next_class_id;
        self.next_class_id += 8; // Keep aligned
        self.hierarchy.insert(id, superclass);
        self.names.insert(id, name.to_string());
        self.name_to_class.insert(name.to_string(), id);
        id
    }

    /// Get the superclass of a class.
    pub fn superclass(&self, klass: VALUE) -> Option<VALUE> {
        self.hierarchy.get(&klass).copied().filter(|&v| v != 0)
    }

    /// Get a class VALUE by name.
    pub fn class_by_name(&self, name: &str) -> Option<VALUE> {
        self.name_to_class.get(name).copied()
    }

    /// Get the name of a class.
    pub fn class_name(&self, klass: VALUE) -> Option<&str> {
        self.names.get(&klass).map(|s| s.as_str())
    }

    /// Check if a VALUE is a known class.
    pub fn is_class(&self, klass: VALUE) -> bool {
        self.names.contains_key(&klass)
    }

    /// Create a Method object bound to a receiver
    pub fn create_method_object(&mut self, receiver: VALUE, method_name: &str, _class_id: u64) -> VALUE {
        // Create a unique VALUE representing the bound method
        // Encode: receiver + method_name hash
        let method_hash = method_name.len() as VALUE;
        receiver.wrapping_add(method_hash << 4)
    }

    /// Create an UnboundMethod object from a class
    pub fn create_unbound_method(&mut self, class: VALUE, method_name: &str) -> VALUE {
        // Create a unique VALUE representing the unbound method
        // Encode: class + method_name hash
        let method_hash = method_name.len() as VALUE;
        class.wrapping_add(method_hash << 4)
    }

    /// Get or create the singleton class for a class.
    /// In Ruby, class methods are defined on the singleton class.
    pub fn singleton_class(&mut self, klass: VALUE) -> Option<VALUE> {
        // Check if we already have a singleton class for this class
        if let Some(&singleton) = self.singleton_classes.get(&klass) {
            eprintln!("DEBUG: singleton_class({}) -> existing {}", klass, singleton);
            return Some(singleton);
        }
        
        // Create a new singleton class
        let singleton_id = self.next_class_id;
        self.next_class_id += 8;
        
        // Get the class name for the singleton class name
        let class_name = self.names.get(&klass).cloned().unwrap_or_else(|| "Anonymous".to_string());
        let singleton_name = format!("#<Class:{}>", class_name);
        
        // The singleton class's superclass is the class's superclass's singleton class
        // For simplicity, we just use the class itself as the "superclass" for method lookup
        let superclass = self.hierarchy.get(&klass).copied().unwrap_or(0);
        
        self.hierarchy.insert(singleton_id, superclass);
        self.names.insert(singleton_id, singleton_name);
        self.singleton_classes.insert(klass, singleton_id);
        
        eprintln!("DEBUG: singleton_class({}) -> created new {}", klass, singleton_id);
        
        Some(singleton_id)
    }
}

/// Access the global class table.
pub fn with_class_table<F, R>(f: F) -> R
where
    F: FnOnce(&mut ClassTable) -> R,
{
    let mut guard = CLASS_TABLE.write().unwrap();
    let tbl = guard.get_or_insert_with(ClassTable::new);
    f(tbl)
}

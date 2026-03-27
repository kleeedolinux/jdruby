//! # Method Storage — Method Definition and Lookup
//!
//! Stores method entries keyed by (class, method_name) with hierarchy traversal.

use std::collections::HashMap;
use std::sync::RwLock;
use crate::core::VALUE;
use super::class_table::with_class_table;

/// Global method storage.
static METHOD_STORAGE: RwLock<Option<MethodStorage>> = RwLock::new(None);

/// A registered method entry.
#[derive(Clone)]
pub struct MethodEntry {
    /// The C function pointer (stored as usize for Send+Sync).
    pub func: usize,
    /// Expected arity.
    pub arity: i32,
    /// Method name.
    pub name: String,
    /// The class VALUE this method belongs to.
    pub klass: VALUE,
}

/// Stores method definitions keyed by (class, method_name).
pub struct MethodStorage {
    /// (class VALUE, method_name) → MethodEntry
    methods: HashMap<(VALUE, String), MethodEntry>,
}

impl MethodStorage {
    fn new() -> Self {
        Self {
            methods: HashMap::new(),
        }
    }

    /// Register a method on a class.
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
    pub fn lookup(&self, klass: VALUE, name: &str) -> Option<&MethodEntry> {
        let mut current = klass;
        loop {
            if let Some(entry) = self.methods.get(&(current, name.to_string())) {
                return Some(entry);
            }
            
            // Walk up the superclass chain
            match with_class_table(|tbl| tbl.superclass(current)) {
                Some(super_klass) if super_klass != 0 => {
                    current = super_klass;
                }
                _ => break,
            }
        }
        None
    }

    /// Check if a method is defined on a specific class (no hierarchy).
    pub fn defined_on(&self, klass: VALUE, name: &str) -> bool {
        self.methods.contains_key(&(klass, name.to_string()))
    }

    /// Get all methods defined on a class.
    pub fn methods_for_class(&self, klass: VALUE) -> Vec<&MethodEntry> {
        self.methods
            .iter()
            .filter(|((k, _), _)| *k == klass)
            .map(|(_, entry)| entry)
            .collect()
    }
}

/// Access the global method storage.
pub fn with_method_storage<F, R>(f: F) -> R
where
    F: FnOnce(&mut MethodStorage) -> R,
{
    let mut guard = METHOD_STORAGE.write().unwrap();
    let storage = guard.get_or_insert_with(MethodStorage::new);
    f(storage)
}

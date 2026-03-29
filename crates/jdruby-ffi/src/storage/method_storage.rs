//! # Method Storage — Method Definition and Lookup
//!
//! Stores method entries keyed by (class, method_name) with hierarchy traversal.

use std::collections::HashMap;
use std::sync::RwLock;
use crate::core::VALUE;
use super::class_table::with_class_table;

/// Visibility enum for method definitions
#[repr(i32)]
#[derive(Clone, Copy)]
pub enum Visibility {
    Public = 0,
    Protected = 1,
    Private = 2,
}

/// Global method storage.
static METHOD_STORAGE: RwLock<Option<MethodStorage>> = RwLock::new(None);

/// A registered method entry.
#[derive(Clone)]
pub struct MethodEntry {
    /// Function name to resolve (for LLVM-compiled functions).
    pub func_name: String,
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

    /// Register a method on a class with function name.
    pub fn define_method(&mut self, klass: VALUE, name: &str, func_name: &str, arity: i32) {
        let entry = MethodEntry {
            func_name: func_name.to_string(),
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

    /// Define method with visibility (metaprogramming support)
    pub fn define_method_with_visibility(&mut self, klass: VALUE, name: &str, func_name: &str, _visibility: Visibility) {
        let entry = MethodEntry {
            func_name: func_name.to_string(),
            arity: -1, // Variable arity
            name: name.to_string(),
            klass,
        };
        self.methods.insert((klass, name.to_string()), entry);
    }

    /// Undefine method (marks as undefined but keeps entry)
    pub fn undef_method(&mut self, klass: VALUE, name: &str) {
        // For now, same as remove - in full impl would mark as undefined
        self.methods.remove(&(klass, name.to_string()));
    }

    /// Remove method completely
    pub fn remove_method(&mut self, klass: VALUE, name: &str) {
        self.methods.remove(&(klass, name.to_string()));
    }

    /// Alias method
    pub fn alias_method(&mut self, klass: VALUE, new_name: &str, old_name: &str) {
        if let Some(old_entry) = self.methods.get(&(klass, old_name.to_string())).cloned() {
            let new_entry = MethodEntry {
                func_name: old_entry.func_name.clone(),
                arity: old_entry.arity,
                name: new_name.to_string(),
                klass,
            };
            self.methods.insert((klass, new_name.to_string()), new_entry);
        }
    }

    /// Set method visibility
    pub fn set_visibility(&mut self, _klass: VALUE, _name: &str, _visibility: Visibility) {
        // In full implementation, would store visibility separately
        // For now, visibility is not enforced
    }

    /// Dispatch method call
    pub fn dispatch(&self, obj: VALUE, name: &str, args: &[VALUE]) -> VALUE {
        // Look up method and call it
        if let Some(entry) = self.lookup(obj, name) {
            unsafe {
                use crate::capi::class::dispatch_c_method;
                dispatch_c_method(&entry.func_name, entry.arity, obj, args)
            }
        } else {
            0 // RUBY_QNIL equivalent
        }
    }

    /// Public dispatch (respects visibility)
    pub fn dispatch_public(&self, obj: VALUE, name: &str, args: &[VALUE]) -> VALUE {
        // Same as dispatch for now - visibility check would be added
        self.dispatch(obj, name, args)
    }

    /// Check if method exists
    pub fn has_method(&self, obj: VALUE, name: &str) -> bool {
        self.lookup(obj, name).is_some()
    }

    /// Extract method name from Method object
    pub fn extract_method_name(&self, method_obj: VALUE) -> Option<String> {
        // Method objects encode method info in their VALUE
        // For now, return a placeholder
        Some(format!("method_{}", method_obj))
    }

    /// Bind unbound method to object
    pub fn bind_method(&mut self, method_obj: VALUE, obj: VALUE) -> VALUE {
        // Create bound method from unbound
        // For now, return combined value
        method_obj.wrapping_add(obj)
    }

    /// Prepend module methods to class (insert before existing)
    pub fn prepend_module(&mut self, class_name: &str, module_name: &str) {
        // In full implementation, would copy module methods to class
        // with prepend flag for method lookup order
        // For now, just track that the module is prepended
        let _ = (class_name, module_name); // Use params
    }

    /// Include module methods in class
    pub fn include_module(&mut self, class_name: &str, module_name: &str) {
        // In full implementation, would copy module methods to class
        // For now, just track that the module is included
        let _ = (class_name, module_name); // Use params
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

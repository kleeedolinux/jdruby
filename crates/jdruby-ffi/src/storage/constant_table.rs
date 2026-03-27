//! # Constant Table — (Class, Name) → Value Mapping with Hierarchy Lookup
//!
//! Stores Ruby constants with proper class hierarchy traversal.

use std::collections::HashMap;
use std::sync::RwLock;
use crate::core::VALUE;
use super::class_table::with_class_table;

/// Global constant storage.
static CONSTANT_TABLE: RwLock<Option<ConstantTable>> = RwLock::new(None);

/// Stores constants keyed by (class, name).
pub struct ConstantTable {
    /// (class VALUE, const name) → value VALUE
    constants: HashMap<(VALUE, String), VALUE>,
}

impl ConstantTable {
    fn new() -> Self {
        Self {
            constants: HashMap::new(),
        }
    }

    /// Set a constant on a class.
    pub fn set(&mut self, klass: VALUE, name: &str, val: VALUE) {
        self.constants.insert((klass, name.to_string()), val);
    }

    /// Look up a constant, walking the class hierarchy.
    pub fn get(&self, klass: VALUE, name: &str) -> Option<VALUE> {
        // First check the class itself
        if let Some(&val) = self.constants.get(&(klass, name.to_string())) {
            return Some(val);
        }
        
        // Walk up the class hierarchy
        let mut current = klass;
        loop {
            if let Some(&val) = self.constants.get(&(current, name.to_string())) {
                return Some(val);
            }
            
            // Move to superclass
            match with_class_table(|tbl| tbl.superclass(current)) {
                Some(super_klass) if super_klass != 0 => {
                    current = super_klass;
                }
                _ => break,
            }
        }
        None
    }

    /// Check if a constant is defined (without hierarchy walk).
    pub fn defined_on(&self, klass: VALUE, name: &str) -> bool {
        self.constants.contains_key(&(klass, name.to_string()))
    }

    /// Check if a constant is defined anywhere in the hierarchy.
    pub fn defined(&self, klass: VALUE, name: &str) -> bool {
        self.get(klass, name).is_some()
    }
}

/// Access the global constant table.
pub fn with_constant_table<F, R>(f: F) -> R
where
    F: FnOnce(&mut ConstantTable) -> R,
{
    let mut guard = CONSTANT_TABLE.write().unwrap();
    let tbl = guard.get_or_insert_with(ConstantTable::new);
    f(tbl)
}

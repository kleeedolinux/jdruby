//! # Instance Variable Storage — (Object, Name) → Value Mapping
//!
//! Stores instance variables for Ruby objects.

use std::collections::HashMap;
use std::sync::RwLock;
use crate::core::{VALUE, RUBY_QNIL};

/// Global instance variable storage.
static IVAR_STORAGE: RwLock<Option<IvarStorage>> = RwLock::new(None);

/// Stores instance variables keyed by (object, variable name).
pub struct IvarStorage {
    /// (object VALUE, ivar name) → value VALUE
    vars: HashMap<(VALUE, String), VALUE>,
}

impl IvarStorage {
    fn new() -> Self {
        Self {
            vars: HashMap::new(),
        }
    }

    /// Get an instance variable from an object.
    pub fn get(&self, obj: VALUE, name: &str) -> VALUE {
        self.vars.get(&(obj, name.to_string())).copied().unwrap_or(RUBY_QNIL)
    }

    /// Set an instance variable on an object.
    pub fn set(&mut self, obj: VALUE, name: &str, val: VALUE) {
        self.vars.insert((obj, name.to_string()), val);
    }

    /// Check if an instance variable is defined.
    pub fn defined(&self, obj: VALUE, name: &str) -> bool {
        self.vars.contains_key(&(obj, name.to_string()))
    }

    /// Remove an instance variable.
    pub fn remove(&mut self, obj: VALUE, name: &str) -> Option<VALUE> {
        self.vars.remove(&(obj, name.to_string()))
    }
}

/// Access the global instance variable storage.
pub fn with_ivar_storage<F, R>(f: F) -> R
where
    F: FnOnce(&mut IvarStorage) -> R,
{
    let mut guard = IVAR_STORAGE.write().unwrap();
    let storage = guard.get_or_insert_with(IvarStorage::new);
    f(storage)
}

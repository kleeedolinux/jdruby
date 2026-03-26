//! # Symbol Table Implementation
//!
//! Global symbol interning - never garbage collected.
//! Follows MRI's symbol.c structure.

use std::collections::HashMap;
use std::sync::{Mutex, LazyLock};

pub type ID = u64;

/// Global symbol table
static SYMBOL_TABLE: LazyLock<Mutex<SymbolTable>> = LazyLock::new(|| {
    Mutex::new(SymbolTable::new())
});

/// Symbol table - maps strings to unique IDs
pub struct SymbolTable {
    name_to_id: HashMap<String, ID>,
    id_to_name: HashMap<ID, String>,
    next_id: ID,
}

impl SymbolTable {
    pub fn new() -> Self {
        Self {
            name_to_id: HashMap::new(),
            id_to_name: HashMap::new(),
            next_id: 1, // IDs start at 1 (0 is reserved)
        }
    }

    /// Intern a string as a symbol
    pub fn intern(&mut self, name: &str) -> ID {
        if let Some(&id) = self.name_to_id.get(name) {
            return id;
        }

        let id = self.next_id;
        self.next_id += 1;

        let owned = name.to_string();
        self.name_to_id.insert(owned.clone(), id);
        self.id_to_name.insert(id, owned);

        id
    }

    /// Look up symbol name by ID
    pub fn name_of(&self, id: ID) -> Option<&str> {
        self.id_to_name.get(&id).map(|s| s.as_str())
    }

    /// Check if symbol exists
    pub fn is_interned(&self, name: &str) -> bool {
        self.name_to_id.contains_key(name)
    }

    /// Total number of interned symbols
    pub fn count(&self) -> usize {
        self.id_to_name.len()
    }
}

impl Default for SymbolTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Intern a symbol name, returning its ID (thread-safe)
pub fn rb_intern(name: &str) -> ID {
    SYMBOL_TABLE.lock().unwrap().intern(name)
}

/// Look up a symbol name by ID (thread-safe)
pub fn rb_id2name(id: ID) -> Option<String> {
    SYMBOL_TABLE.lock().unwrap().name_of(id).map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_intern() {
        let mut table = SymbolTable::new();
        
        let id1 = table.intern("foo");
        let id2 = table.intern("bar");
        let id3 = table.intern("foo"); // Same name
        
        assert_eq!(id1, id3);
        assert_ne!(id1, id2);
        assert_eq!(table.count(), 2);
    }

    #[test]
    fn test_symbol_lookup() {
        let mut table = SymbolTable::new();
        
        let id = table.intern("test_symbol");
        assert_eq!(table.name_of(id), Some("test_symbol"));
        assert_eq!(table.name_of(999), None);
    }

    #[test]
    fn test_global_intern() {
        let id1 = rb_intern("global_foo");
        let id2 = rb_intern("global_foo");
        assert_eq!(id1, id2);
        assert_eq!(rb_id2name(id1), Some("global_foo".to_string()));
    }
}

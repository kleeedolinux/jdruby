//! # Symbol Table — Dedicated Symbol Interning
//!
//! Maps symbol names to IDs and vice versa. Uses RwLock for read-heavy operations.

use std::collections::HashMap;
use std::sync::RwLock;
use crate::core::ID;

/// Global symbol table instance.
static SYMBOL_TABLE: RwLock<Option<SymbolTable>> = RwLock::new(None);

/// The symbol table stores name ↔ ID mappings.
pub struct SymbolTable {
    /// name → ID mapping
    symbols: HashMap<String, ID>,
    /// ID → name mapping (reverse lookup)
    sym_names: HashMap<ID, String>,
    /// Next available symbol ID
    next_sym_id: ID,
}

impl SymbolTable {
    fn new() -> Self {
        let mut tbl = Self {
            symbols: HashMap::new(),
            sym_names: HashMap::new(),
            next_sym_id: 1,
        };
        
        // Pre-intern common symbols for efficiency
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

    /// Intern a string as a symbol ID. Returns existing ID if already interned.
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

    /// Check if a symbol is already interned.
    pub fn is_interned(&self, name: &str) -> bool {
        self.symbols.contains_key(name)
    }
}

/// Initialize the global symbol table.
pub fn init_symbol_table() {
    let mut guard = SYMBOL_TABLE.write().unwrap();
    *guard = Some(SymbolTable::new());
}

/// Access the global symbol table.
pub fn with_symbol_table<F, R>(f: F) -> R
where
    F: FnOnce(&mut SymbolTable) -> R,
{
    let mut guard = SYMBOL_TABLE.write().unwrap();
    let tbl = guard.get_or_insert_with(SymbolTable::new);
    f(tbl)
}

/// Intern a symbol name → ID (convenience function).
pub fn rb_intern_str(name: &str) -> ID {
    with_symbol_table(|tbl| tbl.intern(name))
}

/// Get the name of a symbol ID (convenience function).
pub fn rb_id2name_str(id: ID) -> Option<String> {
    with_symbol_table(|tbl| tbl.id2name(id).map(|s| s.to_string()))
}

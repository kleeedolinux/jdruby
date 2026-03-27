//! Method Table Implementation
//!
//! Provides hash table-based method storage for classes and modules,
//! compatible with MRI's method table layout.

use crate::types::*;
use std::alloc::{alloc, dealloc, Layout};
use std::ptr;

/// Initial capacity for method tables
const INITIAL_CAPACITY: usize = 16;

/// Load factor threshold for rehashing
const LOAD_FACTOR: f64 = 0.75;

/// Method table builder
pub struct MethodTableBuilder {
    entries: Vec<(SymbolId, MethodEntry)>,
}

impl MethodTableBuilder {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Add a method entry
    pub fn add(&mut self, name: SymbolId, entry: MethodEntry) {
        // Remove existing entry if present
        self.entries.retain(|(n, _)| *n != name);
        self.entries.push((name, entry));
    }

    /// Remove a method entry
    pub fn remove(&mut self, name: SymbolId) -> Option<MethodEntry> {
        let pos = self.entries.iter().position(|(n, _)| *n == name)?;
        Some(self.entries.remove(pos).1)
    }

    /// Build the final method table
    pub fn build(self) -> *mut MethodTable {
        let num_entries = self.entries.len();
        let capacity = (num_entries as f64 / LOAD_FACTOR).ceil() as usize;
        let capacity = capacity.max(INITIAL_CAPACITY);
        
        // Allocate method table
        let table_layout = Layout::new::<MethodTable>();
        let table_ptr = unsafe { alloc(table_layout) as *mut MethodTable };
        
        if table_ptr.is_null() {
            panic!("Failed to allocate method table");
        }
        
        unsafe {
            (*table_ptr).num_entries = num_entries;
            (*table_ptr).entries = ptr::null_mut();
        }
        
        // Allocate entry nodes
        if num_entries > 0 {
            let entries_layout = Layout::array::<MethodEntryNode>(capacity).unwrap();
            let entries_ptr = unsafe { alloc(entries_layout) as *mut MethodEntryNode };
            
            if entries_ptr.is_null() {
                panic!("Failed to allocate method table entries");
            }
            
            // Build hash chains (simplified - use single linked list for now)
            // In production, use proper hash buckets
            let mut prev: *mut MethodEntryNode = ptr::null_mut();
            
            for (i, (name, entry)) in self.entries.iter().enumerate() {
                let node = unsafe { entries_ptr.add(i) };
                
                unsafe {
                    (*node).key = *name;
                    
                    // Allocate the method entry separately
                    let entry_layout = Layout::new::<MethodEntry>();
                    let entry_ptr = alloc(entry_layout) as *mut MethodEntry;
                    ptr::write(entry_ptr, entry.clone());
                    (*node).entry = entry_ptr;
                    
                    (*node).next = ptr::null_mut();
                    
                    if !prev.is_null() {
                        (*prev).next = node;
                    }
                }
                
                prev = node;
            }
            
            unsafe {
                (*table_ptr).entries = entries_ptr;
            }
        }
        
        table_ptr
    }
}

impl Default for MethodTableBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Destroy a method table and free all memory
pub unsafe fn destroy_method_table(table: *mut MethodTable) {
    if table.is_null() {
        return;
    }
    
    let num_entries = (*table).num_entries;
    
    if !(*table).entries.is_null() && num_entries > 0 {
        // Free all entry nodes and their method entries
        let mut current = (*table).entries;
        
        for _ in 0..num_entries {
            if current.is_null() {
                break;
            }
            
            let next = (*current).next;
            
            // Free the method entry
            if !(*current).entry.is_null() {
                let entry_layout = Layout::new::<MethodEntry>();
                dealloc((*current).entry as *mut u8, entry_layout);
            }
            
            current = next;
        }
        
        // Free the entries array
        let capacity = (num_entries as f64 / LOAD_FACTOR).ceil() as usize;
        let entries_layout = Layout::array::<MethodEntryNode>(capacity.max(INITIAL_CAPACITY)).unwrap();
        dealloc((*table).entries as *mut u8, entries_layout);
    }
    
    // Free the table itself
    let table_layout = Layout::new::<MethodTable>();
    dealloc(table as *mut u8, table_layout);
}

/// Lookup a method in a method table
pub unsafe fn lookup(table: *const MethodTable, name: SymbolId) -> Option<*const MethodEntry> {
    if table.is_null() {
        return None;
    }
    
    let num_entries = (*table).num_entries;
    if num_entries == 0 {
        return None;
    }
    
    let mut current = (*table).entries;
    
    for _ in 0..num_entries {
        if current.is_null() {
            break;
        }
        
        if (*current).key == name {
            return Some((*current).entry);
        }
        
        current = (*current).next;
    }
    
    None
}

/// Check if a method exists in the table
pub unsafe fn contains(table: *const MethodTable, name: SymbolId) -> bool {
    lookup(table, name).is_some()
}

/// Get the number of entries in the table
pub unsafe fn len(table: *const MethodTable) -> usize {
    if table.is_null() {
        0
    } else {
        (*table).num_entries
    }
}

/// Check if the table is empty
pub unsafe fn is_empty(table: *const MethodTable) -> bool {
    len(table) == 0
}

/// Iterate over all method entries
pub unsafe fn iter(table: *const MethodTable) -> MethodTableIter {
    if table.is_null() {
        MethodTableIter { current: ptr::null_mut() }
    } else {
        MethodTableIter { current: (*table).entries }
    }
}

/// Iterator over method table entries
pub struct MethodTableIter {
    current: *mut MethodEntryNode,
}

impl Iterator for MethodTableIter {
    type Item = (SymbolId, *const MethodEntry);
    
    fn next(&mut self) -> Option<Self::Item> {
        if self.current.is_null() {
            return None;
        }
        
        unsafe {
            let key = (*self.current).key;
            let entry = (*self.current).entry;
            self.current = (*self.current).next;
            Some((key, entry))
        }
    }
}

/// Merge two method tables (for inheritance)
pub unsafe fn merge(base: *const MethodTable, overlay: *const MethodTable) -> *mut MethodTable {
    let mut builder = MethodTableBuilder::new();
    
    // Add all entries from base
    if !base.is_null() {
        for (name, entry) in iter(base) {
            // Clone the entry
            let entry_clone = (*entry).clone();
            builder.add(name, entry_clone);
        }
    }
    
    // Override with entries from overlay
    if !overlay.is_null() {
        for (name, entry) in iter(overlay) {
            let entry_clone = (*entry).clone();
            builder.add(name, entry_clone);
        }
    }
    
    builder.build()
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_empty_table() {
        let builder = MethodTableBuilder::new();
        let table = builder.build();
        
        unsafe {
            assert!(is_empty(table));
            assert_eq!(len(table), 0);
            assert!(lookup(table, 1).is_none());
            destroy_method_table(table);
        }
    }
    
    #[test]
    fn test_add_and_lookup() {
        let mut builder = MethodTableBuilder::new();
        
        let entry = MethodEntry {
            flags: 0,
            def: ptr::null_mut(),
            owner: ptr::null(),
            called_id: 1,
            vis: Visibility::Public,
            method_serial: 1,
        };
        
        builder.add(1, entry);
        let table = builder.build();
        
        unsafe {
            assert!(!is_empty(table));
            assert_eq!(len(table), 1);
            
            let found = lookup(table, 1);
            assert!(found.is_some());
            
            destroy_method_table(table);
        }
    }
}

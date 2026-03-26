//! # Root Scanning and Management

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use crate::header::ObjectHeader;
use crate::allocator::GcPtr;

/// Root handle for registered roots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RootHandle {
    id: usize,
}

/// Root registration error.
#[derive(Debug, Clone, PartialEq)]
pub enum RootError {
    InvalidPointer,
    RootTableFull,
}

/// Set of GC roots.
pub struct RootSet {
    /// Global roots: handle_id → object pointer.
    roots: HashMap<usize, *mut ObjectHeader>,
    /// Next handle ID.
    next_id: AtomicUsize,
}

impl RootSet {
    /// Create new empty root set.
    pub fn new() -> Self {
        Self {
            roots: HashMap::new(),
            next_id: AtomicUsize::new(1),
        }
    }

    /// Register a global root.
    pub fn register<T>(&mut self, ptr: GcPtr<T>) -> Result<RootHandle, RootError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let obj_ptr = ptr.as_ptr() as *mut ObjectHeader;
        
        if obj_ptr.is_null() {
            return Err(RootError::InvalidPointer);
        }
        
        self.roots.insert(id, obj_ptr);
        Ok(RootHandle { id })
    }

    /// Unregister a root.
    pub fn unregister(&mut self, handle: RootHandle) {
        self.roots.remove(&handle.id);
    }

    /// Iterate over all roots.
    pub fn iter_roots<F>(&self, mut f: F)
    where
        F: FnMut(*mut ObjectHeader),
    {
        for ptr in self.roots.values() {
            f(*ptr);
        }
    }

    /// Get root count.
    pub fn len(&self) -> usize {
        self.roots.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.roots.is_empty()
    }

    /// Clear all roots.
    pub fn clear(&mut self) {
        self.roots.clear();
    }
}

impl Default for RootSet {
    fn default() -> Self {
        Self::new()
    }
}

/// Conservative stack scanner.
pub struct StackScanner;

impl StackScanner {
    /// Scan thread stack for potential GC pointers.
    /// This is a conservative scan - may find false positives.
    pub fn scan_stack<F>(stack_bottom: *const u8, stack_top: *const u8, mut callback: F)
    where
        F: FnMut(*mut ObjectHeader),
    {
        // Conservative stack scanning:
        // Walk stack looking for values that could be heap pointers
        let mut ptr = stack_bottom;
        while ptr < stack_top {
            unsafe {
                // Read potential pointer
                let potential_ptr = *(ptr as *const usize) as *mut ObjectHeader;
                
                // Check if it looks like a valid heap object
                if Self::looks_like_heap_pointer(potential_ptr) {
                    callback(potential_ptr);
                }
                
                ptr = ptr.add(std::mem::size_of::<usize>());
            }
        }
    }

    /// Check if pointer looks like heap pointer.
    fn looks_like_heap_pointer(ptr: *mut ObjectHeader) -> bool {
        if ptr.is_null() {
            return false;
        }
        
        // Check alignment
        let addr = ptr as usize;
        if addr & 0b111 != 0 {
            return false;
        }
        
        // Additional heuristics would go here:
        // - Check if in heap address range
        // - Validate header structure
        
        true
    }
}

/// Thread-local roots.
pub struct ThreadLocalRoots {
    /// Local roots for this thread.
    roots: Vec<*mut ObjectHeader>,
}

impl ThreadLocalRoots {
    /// Create new thread-local roots.
    pub fn new() -> Self {
        Self {
            roots: Vec::new(),
        }
    }

    /// Add root.
    pub fn add(&mut self, ptr: *mut ObjectHeader) {
        self.roots.push(ptr);
    }

    /// Remove root.
    pub fn remove(&mut self, ptr: *mut ObjectHeader) {
        self.roots.retain(|p| *p != ptr);
    }

    /// Iterate over roots.
    pub fn iter<F>(&self, mut f: F)
    where
        F: FnMut(*mut ObjectHeader),
    {
        for ptr in &self.roots {
            f(*ptr);
        }
    }
}

impl Default for ThreadLocalRoots {
    fn default() -> Self {
        Self::new()
    }
}

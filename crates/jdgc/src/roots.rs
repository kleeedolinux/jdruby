//! # Root Set Management

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use crate::header::ObjectHeader;
use crate::allocator::GcPtr;

/// Root handle for registered roots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RootHandle {
    id: usize,
}

impl RootHandle {
    /// Get handle ID.
    pub fn id(&self) -> usize {
        self.id
    }
}

/// Root registration error.
#[derive(Debug, Clone, PartialEq)]
pub enum RootError {
    InvalidPointer,
    RootTableFull,
    DuplicateRoot,
}

impl std::fmt::Display for RootError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RootError::InvalidPointer => write!(f, "Invalid root pointer"),
            RootError::RootTableFull => write!(f, "Root table full"),
            RootError::DuplicateRoot => write!(f, "Root already registered"),
        }
    }
}

impl std::error::Error for RootError {}

/// Set of GC roots.
pub struct RootSet {
    /// Global roots: handle_id → object pointer.
    roots: HashMap<usize, *mut ObjectHeader>,
    /// Next handle ID.
    next_id: AtomicUsize,
    /// Maximum number of roots.
    max_roots: usize,
}

impl RootSet {
    /// Maximum default roots.
    pub const DEFAULT_MAX_ROOTS: usize = 10000;

    /// Create new empty root set.
    pub fn new() -> Self {
        Self::with_capacity(Self::DEFAULT_MAX_ROOTS)
    }

    /// Create with specific capacity.
    pub fn with_capacity(max_roots: usize) -> Self {
        Self {
            roots: HashMap::new(),
            next_id: AtomicUsize::new(1),
            max_roots,
        }
    }

    /// Register a global root.
    pub fn register<T>(&mut self, ptr: GcPtr<T>) -> Result<RootHandle, RootError> {
        if self.roots.len() >= self.max_roots {
            return Err(RootError::RootTableFull);
        }
        
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let obj_ptr = ptr.as_ptr() as *mut ObjectHeader;
        
        if obj_ptr.is_null() {
            return Err(RootError::InvalidPointer);
        }
        
        // Check for duplicate
        if self.roots.values().any(|&p| p == obj_ptr) {
            return Err(RootError::DuplicateRoot);
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

    /// Check if contains specific pointer.
    pub fn contains(&self, ptr: *mut ObjectHeader) -> bool {
        self.roots.values().any(|&p| p == ptr)
    }

    /// Clear all roots.
    pub fn clear(&mut self) {
        self.roots.clear();
    }

    /// Get maximum capacity.
    pub fn capacity(&self) -> usize {
        self.max_roots
    }

    /// Get remaining capacity.
    pub fn remaining(&self) -> usize {
        self.max_roots - self.roots.len()
    }

    /// Reserve capacity for additional roots.
    pub fn reserve(&mut self, additional: usize) {
        self.max_roots += additional;
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
    /// Maximum roots.
    max_roots: usize,
}

impl ThreadLocalRoots {
    /// Default max thread-local roots.
    pub const DEFAULT_MAX: usize = 1000;

    /// Create new thread-local roots.
    pub fn new() -> Self {
        Self::with_capacity(Self::DEFAULT_MAX)
    }

    /// Create with specific capacity.
    pub fn with_capacity(max_roots: usize) -> Self {
        Self {
            roots: Vec::with_capacity(max_roots.min(100)),
            max_roots,
        }
    }

    /// Add root.
    pub fn add(&mut self, ptr: *mut ObjectHeader) -> Result<(), RootError> {
        if self.roots.len() >= self.max_roots {
            return Err(RootError::RootTableFull);
        }
        if ptr.is_null() {
            return Err(RootError::InvalidPointer);
        }
        self.roots.push(ptr);
        Ok(())
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

    /// Get count.
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

    /// Pop last root.
    pub fn pop(&mut self) -> Option<*mut ObjectHeader> {
        self.roots.pop()
    }
}

impl Default for ThreadLocalRoots {
    fn default() -> Self {
        Self::new()
    }
}

/// Root entry for managed references.
pub struct RootEntry<T> {
    /// The managed pointer.
    ptr: GcPtr<T>,
    /// Handle for unregistration.
    handle: Option<RootHandle>,
}

impl<T> RootEntry<T> {
    /// Create new root entry.
    pub fn new(ptr: GcPtr<T>) -> Self {
        Self { ptr, handle: None }
    }

    /// Get pointer.
    pub fn ptr(&self) -> &GcPtr<T> {
        &self.ptr
    }

    /// Set handle.
    pub fn set_handle(&mut self, handle: RootHandle) {
        self.handle = Some(handle);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::alloc::{self, Layout};

    #[test]
    fn test_root_set_creation() {
        let roots = RootSet::new();
        assert!(roots.is_empty());
        assert_eq!(roots.len(), 0);
        assert!(roots.capacity() > 0);
    }

    #[test]
    fn test_root_set_with_capacity() {
        let roots = RootSet::with_capacity(100);
        assert_eq!(roots.capacity(), 100);
        assert_eq!(roots.remaining(), 100);
    }

    #[test]
    fn test_root_registration() {
        let mut roots = RootSet::new();
        
        // Create a fake pointer for testing
        let layout = Layout::from_size_align(64, 8).unwrap();
        let ptr = unsafe { alloc::alloc(layout) };
        let gc_ptr = GcPtr::<u8>::from_raw(ptr).unwrap();
        
        let handle = roots.register(gc_ptr).unwrap();
        assert!(handle.id() > 0);
        assert_eq!(roots.len(), 1);
        assert!(roots.contains(ptr as *mut ObjectHeader));
        
        unsafe { alloc::dealloc(ptr, layout) };
    }

    #[test]
    fn test_root_unregistration() {
        let mut roots = RootSet::new();
        
        let layout = Layout::from_size_align(64, 8).unwrap();
        let ptr = unsafe { alloc::alloc(layout) };
        let gc_ptr = GcPtr::<u8>::from_raw(ptr).unwrap();
        
        let handle = roots.register(gc_ptr).unwrap();
        assert_eq!(roots.len(), 1);
        
        roots.unregister(handle);
        assert_eq!(roots.len(), 0);
        assert!(!roots.contains(ptr as *mut ObjectHeader));
        
        unsafe { alloc::dealloc(ptr, layout) };
    }

    #[test]
    fn test_root_iteration() {
        let mut roots = RootSet::new();
        
        let layout = Layout::from_size_align(64, 8).unwrap();
        let ptr1 = unsafe { alloc::alloc(layout) };
        let ptr2 = unsafe { alloc::alloc(layout) };
        
        roots.register(GcPtr::<u8>::from_raw(ptr1).unwrap()).unwrap();
        roots.register(GcPtr::<u8>::from_raw(ptr2).unwrap()).unwrap();
        
        let mut count = 0;
        roots.iter_roots(|_| count += 1);
        assert_eq!(count, 2);
        
        unsafe { alloc::dealloc(ptr1, layout) };
        unsafe { alloc::dealloc(ptr2, layout) };
    }

    #[test]
    fn test_null_pointer_rejected() {
        let _roots = RootSet::new();
        let null_ptr = GcPtr::<u8>::from_raw(std::ptr::null_mut());
        assert!(null_ptr.is_none());
    }

    #[test]
    fn test_duplicate_root_rejected() {
        let mut roots = RootSet::new();
        
        let layout = Layout::from_size_align(64, 8).unwrap();
        let ptr = unsafe { alloc::alloc(layout) };
        let gc_ptr = GcPtr::<u8>::from_raw(ptr).unwrap();
        
        roots.register(gc_ptr).unwrap();
        let result = roots.register(GcPtr::<u8>::from_raw(ptr).unwrap());
        assert_eq!(result, Err(RootError::DuplicateRoot));
        
        unsafe { alloc::dealloc(ptr, layout) };
    }

    #[test]
    fn test_root_set_clear() {
        let mut roots = RootSet::new();
        
        let layout = Layout::from_size_align(64, 8).unwrap();
        let ptr = unsafe { alloc::alloc(layout) };
        roots.register(GcPtr::<u8>::from_raw(ptr).unwrap()).unwrap();
        
        assert_eq!(roots.len(), 1);
        roots.clear();
        assert_eq!(roots.len(), 0);
        
        unsafe { alloc::dealloc(ptr, layout) };
    }

    #[test]
    fn test_thread_local_roots() {
        let mut local = ThreadLocalRoots::new();
        
        let layout = Layout::from_size_align(64, 8).unwrap();
        let ptr = unsafe { alloc::alloc(layout) } as *mut ObjectHeader;
        
        assert!(local.add(ptr).is_ok());
        assert_eq!(local.len(), 1);
        
        local.remove(ptr);
        assert_eq!(local.len(), 0);
        
        unsafe { alloc::dealloc(ptr as *mut u8, layout) };
    }

    #[test]
    fn test_thread_local_roots_full() {
        let mut local = ThreadLocalRoots::with_capacity(2);
        
        let layout = Layout::from_size_align(64, 8).unwrap();
        let ptr1 = unsafe { alloc::alloc(layout) } as *mut ObjectHeader;
        let ptr2 = unsafe { alloc::alloc(layout) } as *mut ObjectHeader;
        let ptr3 = unsafe { alloc::alloc(layout) } as *mut ObjectHeader;
        
        assert!(local.add(ptr1).is_ok());
        assert!(local.add(ptr2).is_ok());
        assert_eq!(local.add(ptr3), Err(RootError::RootTableFull));
        
        unsafe { alloc::dealloc(ptr1 as *mut u8, layout) };
        unsafe { alloc::dealloc(ptr2 as *mut u8, layout) };
        unsafe { alloc::dealloc(ptr3 as *mut u8, layout) };
    }

    #[test]
    fn test_thread_local_roots_iteration() {
        let mut local = ThreadLocalRoots::new();
        
        let layout = Layout::from_size_align(64, 8).unwrap();
        let ptr1 = unsafe { alloc::alloc(layout) } as *mut ObjectHeader;
        let ptr2 = unsafe { alloc::alloc(layout) } as *mut ObjectHeader;
        
        local.add(ptr1).unwrap();
        local.add(ptr2).unwrap();
        
        let mut count = 0;
        local.iter(|_| count += 1);
        assert_eq!(count, 2);
        
        unsafe { alloc::dealloc(ptr1 as *mut u8, layout) };
        unsafe { alloc::dealloc(ptr2 as *mut u8, layout) };
    }

    #[test]
    fn test_root_handle() {
        let handle = RootHandle { id: 42 };
        assert_eq!(handle.id(), 42);
        assert_eq!(handle.id, 42);
    }

    #[test]
    fn test_root_entry() {
        let layout = Layout::from_size_align(64, 8).unwrap();
        let ptr = unsafe { alloc::alloc(layout) };
        let gc_ptr = GcPtr::<u8>::from_raw(ptr).unwrap();
        
        let mut entry = RootEntry::new(gc_ptr);
        assert!(entry.handle.is_none());
        
        entry.set_handle(RootHandle { id: 1 });
        assert!(entry.handle.is_some());
        
        unsafe { alloc::dealloc(ptr, layout) };
    }
}

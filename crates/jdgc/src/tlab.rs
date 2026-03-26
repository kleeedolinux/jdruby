//! # Thread-Local Allocation Buffer (TLAB)

use std::alloc::Layout;
use std::ptr::{self, NonNull};
use crate::util::*;
use crate::region::Region;

/// Thread-Local Allocation Buffer for fast lock-free allocation.
pub struct Tlab {
    /// Current allocation pointer.
    bump: *mut u8,
    /// End of TLAB.
    end: *mut u8,
    /// TLAB size (unused - reserved for future stats).
    _size: usize,
    /// Current region backing this TLAB (unused - reserved for tracking).
    _region: Option<&'static Region>,
    /// Number of objects allocated.
    allocated_count: usize,
    /// Bytes remaining.
    remaining: usize,
}

impl Tlab {
    /// Create new empty TLAB.
    pub fn new() -> Self {
        Self {
            bump: ptr::null_mut(),
            end: ptr::null_mut(),
            _size: 0,
            _region: None,
            allocated_count: 0,
            remaining: 0,
        }
    }

    /// Create TLAB with specific size.
    pub fn with_size(size: usize) -> Self {
        Self {
            bump: ptr::null_mut(),
            end: ptr::null_mut(),
            _size: size,
            _region: None,
            allocated_count: 0,
            remaining: 0,
        }
    }

    /// Allocate object from TLAB.
    pub fn allocate(&mut self, layout: Layout) -> Option<NonNull<u8>> {
        let size = layout.size();
        let align = layout.align();

        if size > MAX_TLAB_ALLOCATION {
            return None; // Too large for TLAB
        }

        if self.bump.is_null() || self.remaining < size {
            return None; // TLAB not initialized or exhausted
        }

        // Align bump pointer
        let aligned_bump = align_up(self.bump as usize, align);
        let new_bump = aligned_bump + size;

        if new_bump > self.end as usize {
            return None; // Not enough space
        }

        let ptr = aligned_bump as *mut u8;
        self.bump = new_bump as *mut u8;
        self.remaining = self.end as usize - new_bump;
        self.allocated_count += 1;

        Some(unsafe { NonNull::new_unchecked(ptr) })
    }

    /// Check if TLAB can accommodate allocation.
    pub fn can_allocate(&self, size: usize) -> bool {
        !self.bump.is_null() && self.remaining >= size
    }

    /// Get remaining bytes.
    pub fn remaining(&self) -> usize {
        self.remaining
    }

    /// Get allocated count.
    pub fn allocated_count(&self) -> usize {
        self.allocated_count
    }

    /// Retire TLAB (return unused space to region).
    pub fn retire(&mut self) {
        self.bump = ptr::null_mut();
        self.end = ptr::null_mut();
        self.remaining = 0;
        self.allocated_count = 0;
    }

    /// Reset TLAB.
    pub fn reset(&mut self) {
        self.retire();
    }

    /// Check if TLAB is active.
    pub fn is_active(&self) -> bool {
        !self.bump.is_null()
    }

    /// Initialize TLAB with memory range.
    pub fn init(&mut self, start: *mut u8, end: *mut u8) {
        self.bump = start;
        self.end = end;
        self.remaining = end as usize - start as usize;
        self.allocated_count = 0;
    }
}

impl Default for Tlab {
    fn default() -> Self {
        Self::new()
    }
}

/// TLAB statistics for a thread.
#[derive(Debug, Default)]
pub struct TlabStats {
    /// Number of TLAB allocations.
    pub allocations: usize,
    /// Number of slow-path allocations (TLAB miss).
    pub slow_path_allocs: usize,
    /// Total bytes allocated.
    pub bytes_allocated: usize,
    /// Number of TLAB refills.
    pub refills: usize,
}

/// Thread-local TLAB state.
pub struct ThreadLocalTlab {
    /// The TLAB.
    pub tlab: Tlab,
    /// Statistics.
    pub stats: TlabStats,
}

impl ThreadLocalTlab {
    /// Create new thread-local TLAB.
    pub fn new() -> Self {
        Self {
            tlab: Tlab::new(),
            stats: TlabStats::default(),
        }
    }

    /// Allocate with statistics tracking.
    pub fn allocate(&mut self, layout: Layout) -> Option<NonNull<u8>> {
        if let Some(ptr) = self.tlab.allocate(layout) {
            self.stats.allocations += 1;
            self.stats.bytes_allocated += layout.size();
            Some(ptr)
        } else {
            self.stats.slow_path_allocs += 1;
            None
        }
    }

    /// Refill TLAB.
    pub fn refill(&mut self, start: *mut u8, end: *mut u8) {
        self.tlab.init(start, end);
        self.stats.refills += 1;
    }
}

impl Default for ThreadLocalTlab {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::alloc::{self, Layout};

    #[test]
    fn test_tlab_creation() {
        let tlab = Tlab::new();
        assert!(!tlab.is_active());
        assert_eq!(tlab.remaining(), 0);
        assert_eq!(tlab.allocated_count(), 0);
    }

    #[test]
    fn test_tlab_allocation() {
        let mut tlab = Tlab::new();
        let size = 1024usize;
        let layout = Layout::from_size_align(size, 8).unwrap();
        
        // Allocate backing memory
        let backing = unsafe { alloc::alloc(Layout::from_size_align(size * 2, 8).unwrap()) };
        tlab.init(backing, unsafe { backing.add(size * 2) });
        
        assert!(tlab.is_active());
        assert_eq!(tlab.remaining(), size * 2);
        
        // Allocate first object
        let obj1 = tlab.allocate(layout);
        assert!(obj1.is_some());
        assert_eq!(tlab.allocated_count(), 1);
        assert!(tlab.remaining() < size * 2);
        
        // Allocate second object
        let obj2 = tlab.allocate(layout);
        assert!(obj2.is_some());
        assert_eq!(tlab.allocated_count(), 2);
        
        // Objects should be different
        assert_ne!(obj1.unwrap().as_ptr(), obj2.unwrap().as_ptr());
        
        tlab.retire();
        assert!(!tlab.is_active());
        
        unsafe { alloc::dealloc(backing, Layout::from_size_align(size * 2, 8).unwrap()) };
    }

    #[test]
    fn test_tlab_alignment() {
        let mut tlab = Tlab::new();
        let size = 1024usize;
        
        let backing = unsafe { alloc::alloc(Layout::from_size_align(size, 8).unwrap()) };
        tlab.init(backing, unsafe { backing.add(size) });
        
        // Allocate with 16-byte alignment
        let layout = Layout::from_size_align(8, 16).unwrap();
        let obj = tlab.allocate(layout).unwrap();
        assert_eq!(obj.as_ptr() as usize % 16, 0);
        
        tlab.retire();
        unsafe { alloc::dealloc(backing, Layout::from_size_align(size, 8).unwrap()) };
    }

    #[test]
    fn test_tlab_exhaustion() {
        let mut tlab = Tlab::new();
        let size = 64usize;
        
        let backing = unsafe { alloc::alloc(Layout::from_size_align(size, 8).unwrap()) };
        tlab.init(backing, unsafe { backing.add(size) });
        
        // First small allocation should succeed
        let layout1 = Layout::from_size_align(32, 8).unwrap();
        assert!(tlab.allocate(layout1).is_some());
        
        // Second allocation should succeed
        let layout2 = Layout::from_size_align(16, 8).unwrap();
        assert!(tlab.allocate(layout2).is_some());
        
        // Large allocation should fail (TLAB exhausted)
        let layout3 = Layout::from_size_align(32, 8).unwrap();
        assert!(tlab.allocate(layout3).is_none());
        
        tlab.retire();
        unsafe { alloc::dealloc(backing, Layout::from_size_align(size, 8).unwrap()) };
    }

    #[test]
    fn test_tlab_oversized() {
        let mut tlab = Tlab::new();
        
        let backing = unsafe { alloc::alloc(Layout::from_size_align(1024 * 1024, 8).unwrap()) };
        tlab.init(backing, unsafe { backing.add(1024 * 1024) });
        
        // Allocation larger than MAX_TLAB_ALLOCATION should fail
        let huge_layout = Layout::from_size_align(MAX_TLAB_ALLOCATION + 1, 8).unwrap();
        assert!(tlab.allocate(huge_layout).is_none());
        
        tlab.retire();
        unsafe { alloc::dealloc(backing, Layout::from_size_align(1024 * 1024, 8).unwrap()) };
    }

    #[test]
    fn test_thread_local_tlab() {
        let mut local = ThreadLocalTlab::new();
        
        let size = 1024usize;
        let backing = unsafe { alloc::alloc(Layout::from_size_align(size, 8).unwrap()) };
        local.refill(backing, unsafe { backing.add(size) });
        
        let layout = Layout::from_size_align(64, 8).unwrap();
        assert!(local.allocate(layout).is_some());
        assert_eq!(local.stats.allocations, 1);
        assert_eq!(local.stats.refills, 1);
        
        // Exhaust TLAB
        for _ in 0..20 {
            let _ = local.allocate(layout);
        }
        
        // After exhaustion, should report slow path
        assert!(local.stats.slow_path_allocs > 0);
        
        local.tlab.retire();
        unsafe { alloc::dealloc(backing, Layout::from_size_align(size, 8).unwrap()) };
    }

    #[test]
    fn test_tlab_can_allocate() {
        let mut tlab = Tlab::new();
        assert!(!tlab.can_allocate(1));
        
        let size = 1024usize;
        let backing = unsafe { alloc::alloc(Layout::from_size_align(size, 8).unwrap()) };
        tlab.init(backing, unsafe { backing.add(size) });
        
        assert!(tlab.can_allocate(100));
        assert!(tlab.can_allocate(512));
        assert!(!tlab.can_allocate(size + 1));
        
        tlab.retire();
        unsafe { alloc::dealloc(backing, Layout::from_size_align(size, 8).unwrap()) };
    }
}

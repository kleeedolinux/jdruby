//! # GC Allocator and TLAB Management

use std::ptr;
use std::alloc::Layout;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use crate::util::*;
use crate::region::{RegionManager, Region};
use crate::header::ObjectHeader;

/// Allocation error types.
#[derive(Debug, Clone, PartialEq)]
pub enum AllocationError {
    OutOfMemory,
    InvalidSize,
    RegionFull,
    InvalidAlignment,
}

/// Thread-Local Allocation Buffer (TLAB).
pub struct TlabAllocator {
    /// Current allocation region (unused - reserved for tracking).
    _region: Option<&'static Region>,
    /// Bump pointer within TLAB.
    bump: *mut u8,
    /// End of TLAB.
    end: *mut u8,
    /// Remaining bytes.
    remaining: usize,
    /// Total TLAB size (unused - reserved for stats).
    _size: usize,
}

impl TlabAllocator {
    /// Create new TLAB allocator.
    pub fn new(size: usize) -> Self {
        Self {
            _region: None,
            bump: ptr::null_mut(),
            end: ptr::null_mut(),
            remaining: 0,
            _size: size,
        }
    }

    /// Allocate from TLAB.
    pub fn allocate(&mut self, layout: Layout) -> Result<NonNull<u8>, AllocationError> {
        let size = layout.size();
        let align = layout.align();

        if size > MAX_TLAB_ALLOCATION {
            return Err(AllocationError::InvalidSize);
        }

        // Align bump pointer
        let aligned_bump = align_up(self.bump as usize, align);
        let new_bump = aligned_bump + size;

        if new_bump > self.end as usize {
            // TLAB exhausted
            return Err(AllocationError::RegionFull);
        }

        let ptr = aligned_bump as *mut u8;
        self.bump = new_bump as *mut u8;
        self.remaining = self.end as usize - new_bump;

        Ok(unsafe { NonNull::new_unchecked(ptr) })
    }

    /// Check if TLAB has space.
    pub fn can_allocate(&self, size: usize) -> bool {
        self.remaining >= size
    }

    /// Reset TLAB.
    pub fn reset(&mut self) {
        self.bump = ptr::null_mut();
        self.end = ptr::null_mut();
        self.remaining = 0;
    }

    /// Retire current TLAB.
    pub fn retire(&mut self) {
        self.reset();
    }
}

/// Global allocator for slow-path allocations.
pub struct Allocator {
    /// Region manager reference.
    regions: Arc<RegionManager>,
    /// Allocated bytes counter.
    allocated_bytes: AtomicUsize,
}

impl Allocator {
    /// Create new allocator.
    pub fn new(regions: Arc<RegionManager>) -> Result<Self, AllocationError> {
        Ok(Self {
            regions,
            allocated_bytes: AtomicUsize::new(0),
        })
    }

    /// Allocate object on global heap.
    pub fn allocate(&self, layout: Layout) -> Result<NonNull<u8>, AllocationError> {
        let size = layout.size();

        if let Some(obj) = self.regions.allocate(size) {
            self.allocated_bytes.fetch_add(size, Ordering::Relaxed);
            Ok(unsafe { NonNull::new_unchecked(obj as *mut u8) })
        } else {
            Err(AllocationError::OutOfMemory)
        }
    }

    /// Get total allocated bytes.
    pub fn allocated_bytes(&self) -> usize {
        self.allocated_bytes.load(Ordering::Relaxed)
    }
}

/// GC-managed pointer.
pub struct GcPtr<T> {
    ptr: NonNull<T>,
}

impl<T> GcPtr<T> {
    /// Create from raw pointer.
    /// Returns None if pointer is null.
    pub fn from_raw(ptr: *mut T) -> Option<Self> {
        NonNull::new(ptr).map(|nn| Self { ptr: nn })
    }

    /// Get raw pointer.
    pub fn as_ptr(&self) -> *mut T {
        self.ptr.as_ptr()
    }

    /// Get object header.
    pub fn header(&self) -> &ObjectHeader {
        unsafe {
            let ptr = self.ptr.as_ptr() as *mut u8;
            let header_ptr = ptr.sub(std::mem::size_of::<ObjectHeader>());
            &*(header_ptr as *const ObjectHeader)
        }
    }
}

unsafe impl<T: Send> Send for GcPtr<T> {}
unsafe impl<T: Sync> Sync for GcPtr<T> {}

/// GC object trait.
pub trait GcObject {
    /// Object size for allocation.
    fn gc_size(&self) -> usize;
    /// Trace references.
    fn trace(&self, marker: &mut crate::marker::Marker);
}

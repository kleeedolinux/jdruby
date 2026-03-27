//! # Allocator — JDGC Heap Allocation Helpers
//!
//! Deduplicated allocation logic for FFI objects.

use std::alloc::Layout;
use std::sync::Arc;
use jdgc::{Allocator, GcPtr, RegionManager};
use std::sync::Mutex;
use lazy_static::lazy_static;

lazy_static! {
    static ref ALLOCATOR: Mutex<Option<Allocator>> = Mutex::new(None);
}

/// Initialize the JDGC allocator.
pub fn init_allocator() {
    let regions = Arc::new(RegionManager::new(64 * 1024 * 1024).expect("Failed to create regions"));
    let allocator = Allocator::new(regions).expect("Failed to create allocator");
    
    let mut guard = ALLOCATOR.lock().unwrap();
    *guard = Some(allocator);
}

/// Allocate an object on the JDGC heap.
/// 
/// Returns a typed GcPtr and the raw data pointer (after ObjectHeader).
pub fn allocate_object<T>(layout: Layout) -> Option<(GcPtr<u8>, *mut u8)> {
    let mut guard = ALLOCATOR.lock().unwrap();
    let allocator = guard.as_mut()?;
    
    let ptr = allocator.allocate(layout).ok()?;
    let gc_ptr = GcPtr::from_raw(ptr.as_ptr())?;
    
    // Calculate data pointer (after ObjectHeader)
    let data_ptr = unsafe {
        ptr.as_ptr().add(std::mem::size_of::<jdgc::ObjectHeader>())
    };
    
    Some((gc_ptr, data_ptr))
}

/// Get the allocator instance.
pub fn with_allocator<F, R>(f: F) -> R
where
    F: FnOnce(&mut Allocator) -> R,
{
    let mut guard = ALLOCATOR.lock().unwrap();
    let allocator = guard.as_mut().expect("Allocator not initialized");
    f(allocator)
}

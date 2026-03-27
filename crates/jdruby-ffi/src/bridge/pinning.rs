//! # Pinning — GC Pin Management for FFI Safety
//!
//! Manages object pinning to prevent GC evacuation during C calls.

use jdgc::{GcPtr, ObjectHeader};

/// Pin an object to prevent GC movement.
pub fn pin_object<T>(ptr: GcPtr<T>) {
    ptr.header().pin();
}

/// Unpin an object, allowing it to be moved/collected.
pub fn unpin_object(ptr: *mut u8) {
    // SAFETY: Caller must ensure ptr is a valid GC-managed pointer with ObjectHeader
    let gc_ptr = GcPtr::from_raw(ptr).unwrap();
    gc_ptr.header().unpin();
}

/// Get the ObjectHeader for a raw pointer.
pub fn header_for(ptr: *mut u8) -> Option<&'static ObjectHeader> {
    // SAFETY: Caller must ensure ptr is a valid GC-managed pointer with ObjectHeader
    unsafe {
        let gc_ptr = GcPtr::<u8>::from_raw(ptr)?;
        // Convert the header pointer to a 'static reference
        let header_ptr = gc_ptr.header() as *const ObjectHeader;
        Some(&*header_ptr)
    }
}

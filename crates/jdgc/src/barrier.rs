//! # Read/Write Barriers for Concurrent GC

use crate::header::ObjectHeader;
use crate::marker::MarkQueue;

/// Barrier type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarrierType {
    /// Dijkstra insertion barrier (for write).
    Insertion,
    /// Yuasa deletion barrier (for read).
    Deletion,
    /// Brooks read barrier (for forwarding).
    Brooks,
}

/// Read barrier - Brooks forwarding pointer resolution.
pub struct ReadBarrier;

impl ReadBarrier {
    /// Apply read barrier to pointer.
    /// Returns the forwarded address if object was evacuated.
    #[inline]
    pub fn apply(ptr: *mut ObjectHeader) -> *mut ObjectHeader {
        if ptr.is_null() {
            return ptr;
        }

        let header = unsafe { &*ptr };
        let forwarding_addr = header.forwarding_address();

        if forwarding_addr == ptr {
            // Not forwarded
            ptr
        } else {
            // Return new location
            forwarding_addr
        }
    }
}

/// Write barrier - Dijkstra insertion barrier.
pub struct WriteBarrier;

impl WriteBarrier {
    /// Apply write barrier when storing reference.
    /// Source is the object being modified, target is the reference being stored.
    #[inline]
    pub fn apply(
        source: &ObjectHeader,
        target: *mut ObjectHeader,
        queue: &MarkQueue,
    ) {
        // Dijkstra insertion barrier:
        // If source is Black and target is White, shade target Gray.
        if source.is_black() && !target.is_null() {
            let target_header = unsafe { &*target };
            if target_header.is_white() {
                if target_header.try_shade_gray() {
                    queue.push(target);
                }
            }
        }
    }
}

/// Convenience function for read barrier.
#[inline]
pub fn read_barrier(ptr: *mut ObjectHeader) -> *mut ObjectHeader {
    ReadBarrier::apply(ptr)
}

/// Convenience function for write barrier.
#[inline]
pub fn write_barrier(
    source: &ObjectHeader,
    target: *mut ObjectHeader,
    queue: &MarkQueue,
) {
    WriteBarrier::apply(source, target, queue);
}

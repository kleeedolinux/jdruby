//! # Object Header
//!
//! 64-bit packed header with tri-color state, pinned flag, and forwarding pointer.
//!
//! ```text
//! ┌────────────────────────────────────────────────────────────────────┐
//! │ Bit 63 ─────────────────────── Bit 3 │ Bit 2  │ Bit 1 │ Bit 0   │
//! │        Forwarding Pointer (61 bits)   │ Pinned │    Color (2b)   │
//! └────────────────────────────────────────────────────────────────────┘
//! ```

use std::sync::atomic::{AtomicU64, Ordering};
use crate::util::*;

/// Tri-color marking state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
    White = 0,
    Gray = 1,
    Black = 2,
}

/// 64-bit atomic object header with packed state.
#[repr(C, align(8))]
pub struct ObjectHeader {
    /// Packed 64-bit header word.
    /// bits 0–1: color (White/Gray/Black)
    /// bit  2: pinned flag
    /// bits 3–63: forwarding pointer (8-byte aligned addr)
    bits: AtomicU64,
    /// Size of object payload (bytes after header).
    pub payload_size: usize,
}

impl ObjectHeader {
    /// Create new header for freshly allocated object.
    #[inline]
    pub fn init_at(self_ptr: *mut ObjectHeader, payload_size: usize) {
        let addr = self_ptr as u64;
        debug_assert_eq!(addr & !FWD_MASK, 0, "object pointer not 8-byte aligned");
        let header = addr | WHITE;
        unsafe {
            (*self_ptr).bits.store(header, Ordering::Release);
            (*self_ptr).payload_size = payload_size;
        }
    }

    /// Load raw header word with Acquire ordering.
    #[inline]
    pub fn load(&self) -> u64 {
        self.bits.load(Ordering::Acquire)
    }

    /// Store raw header word with Release ordering.
    #[inline]
    pub fn store(&self, val: u64) {
        self.bits.store(val, Ordering::Release);
    }

    /// Get color.
    #[inline]
    pub fn color(&self) -> Color {
        match self.load() & COLOR_MASK {
            WHITE => Color::White,
            GRAY => Color::Gray,
            BLACK => Color::Black,
            _ => Color::White, // unreachable
        }
    }

    /// Check if White.
    #[inline]
    pub fn is_white(&self) -> bool {
        (self.load() & COLOR_MASK) == WHITE
    }

    /// Check if Gray.
    #[inline]
    pub fn is_gray(&self) -> bool {
        (self.load() & COLOR_MASK) == GRAY
    }

    /// Check if Black.
    #[inline]
    pub fn is_black(&self) -> bool {
        (self.load() & COLOR_MASK) == BLACK
    }

    /// Try to shade White → Gray. Returns true if successful.
    #[inline]
    pub fn try_shade_gray(&self) -> bool {
        let old = self.load();
        if (old & COLOR_MASK) != WHITE {
            return false;
        }
        let new = (old & !COLOR_MASK) | GRAY;
        self.bits.compare_exchange(old, new, Ordering::AcqRel, Ordering::Relaxed).is_ok()
    }

    /// Shade Gray → Black (only valid transition from Gray).
    #[inline]
    pub fn shade_black(&self) -> bool {
        let old = self.load();
        debug_assert_eq!(old & COLOR_MASK, GRAY, "shade_black called on non-Gray object");
        let new = (old & !COLOR_MASK) | BLACK;
        self.store(new);
        true
    }

    /// Reset Black → White (for next GC cycle).
    #[inline]
    pub fn reset_white(&self) {
        let old = self.load();
        let new = (old & !COLOR_MASK) | WHITE;
        self.store(new);
    }

    /// Check if pinned.
    #[inline]
    pub fn is_pinned(&self) -> bool {
        (self.load() & PINNED_BIT) != 0
    }

    /// Pin object (prevent evacuation).
    #[inline]
    pub fn pin(&self) {
        let old = self.load();
        let new = old | PINNED_BIT;
        self.store(new);
    }

    /// Unpin object.
    #[inline]
    pub fn unpin(&self) {
        let old = self.load();
        let new = old & !PINNED_BIT;
        self.store(new);
    }

    /// Get forwarding address.
    #[inline]
    pub fn forwarding_address(&self) -> *mut ObjectHeader {
        let ptr_bits = self.load() & FWD_MASK;
        ptr_bits as *mut ObjectHeader
    }

    /// Check if forwarded (forwarding address != self).
    #[inline]
    pub fn is_forwarded(&self, self_ptr: *mut ObjectHeader) -> bool {
        self.forwarding_address() != self_ptr
    }

    /// Try to install forwarding pointer (CAS loop).
    /// Returns Ok(new_ptr) on success, Err(winner_ptr) if already forwarded.
    #[inline]
    pub fn try_install_forwarding(
        &self,
        old_ptr: *mut ObjectHeader,
        new_ptr: *mut ObjectHeader,
    ) -> Result<*mut ObjectHeader, *mut ObjectHeader> {
        let new_addr = new_ptr as u64;
        debug_assert_eq!(new_addr & !FWD_MASK, 0, "new pointer not 8-byte aligned");

        let mut current = self.load();
        loop {
            let current_fwd = current & FWD_MASK;
            if current_fwd != (old_ptr as u64) {
                // Already forwarded
                return Err(current_fwd as *mut ObjectHeader);
            }

            let desired = (current & COLOR_MASK) | new_addr;
            match self.bits.compare_exchange(
                current,
                desired,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return Ok(new_ptr),
                Err(actual) => current = actual,
            }
        }
    }

    /// Get payload pointer (after header).
    #[inline]
    pub fn payload_ptr(&self) -> *mut u8 {
        let self_ptr = self as *const ObjectHeader as *mut u8;
        unsafe { self_ptr.add(std::mem::size_of::<ObjectHeader>()) }
    }

    /// Create from pointer.
    #[inline]
    pub unsafe fn from_ptr(ptr: *mut u8) -> *mut ObjectHeader {
        ptr.sub(std::mem::size_of::<ObjectHeader>()) as *mut ObjectHeader
    }

    /// Get total object size (header + payload).
    #[inline]
    pub fn total_size(&self) -> usize {
        std::mem::size_of::<ObjectHeader>() + self.payload_size
    }
}

/// Convenient methods for accessing objects.
pub struct ObjectAccess {
    objref: *mut ObjectHeader,
}

impl ObjectAccess {
    pub fn from_ptr(ptr: *mut ObjectHeader) -> Self {
        Self { objref: ptr }
    }

    pub fn header(&self) -> &ObjectHeader {
        unsafe { &*self.objref }
    }

    pub fn payload(&self) -> *mut u8 {
        self.header().payload_ptr()
    }

    pub fn is_alive(&self) -> bool {
        !self.header().is_white()
    }
}

#[cfg(test)]
mod tests {
    use crate::region::Region;

    #[test]
    fn test_color_transitions() {
        let region = Region::new(0);
        let obj = region.allocate_object(64).unwrap();
        let hdr = unsafe { &*obj };

        assert!(hdr.is_white());
        assert!(!hdr.is_gray());
        assert!(!hdr.is_black());

        assert!(hdr.try_shade_gray());
        assert!(hdr.is_gray());

        assert!(hdr.shade_black());
        assert!(hdr.is_black());

        assert!(!hdr.try_shade_gray());
    }

    #[test]
    fn test_pinning() {
        let region = Region::new(0);
        let obj = region.allocate_object(32).unwrap();
        let hdr = unsafe { &*obj };

        assert!(!hdr.is_pinned());
        hdr.pin();
        assert!(hdr.is_pinned());

        assert!(hdr.try_shade_gray());
        assert!(hdr.is_pinned());
    }

    #[test]
    fn test_forwarding() {
        let region = Region::new(0);
        let old = region.allocate_object(64).unwrap();
        let new = region.allocate_object(64).unwrap();
        let hdr = unsafe { &*old };

        assert_eq!(hdr.forwarding_address(), old);
        assert!(!hdr.is_forwarded(old));

        hdr.try_install_forwarding(old, new).unwrap();
        assert!(hdr.is_forwarded(old));
        assert_eq!(hdr.forwarding_address(), new);
    }
}

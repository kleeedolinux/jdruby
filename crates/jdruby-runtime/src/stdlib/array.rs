//! # Ruby Array Implementation
//!
//! MRI-compatible Array with embedded/heap storage optimization.
//! Follows the structure of MRI's array.c

use std::mem::ManuallyDrop;
use std::alloc::{alloc, dealloc, Layout};
use std::sync::atomic::{AtomicU32, Ordering};

pub const ARRAY_EMBED_MAX_LEN: usize = 3;
pub const ARRAY_DEFAULT_CAPA: usize = 16;

/// Ruby Array flags
pub const ARY_EMBED_FLAG: u32 = 1 << 0;
pub const ARY_SHARED_FLAG: u32 = 1 << 1;
pub const ARY_FROZEN_FLAG: u32 = 1 << 2;

/// Ruby Array - either inline (up to 3 elements) or heap-allocated
#[repr(C)]
pub struct RubyArray {
    pub flags: AtomicU32,
    pub len: usize,
    pub capa: usize,
    pub storage: ArrayStorage,
}

#[repr(C)]
pub union ArrayStorage {
    pub embed: [u64; ARRAY_EMBED_MAX_LEN],
    pub heap: ManuallyDrop<ArrayHeap>,
}

#[repr(C)]
pub struct ArrayHeap {
    pub ptr: *mut u64,
    pub capa: usize,
    pub shared_root: *mut RubyArray,
}

impl RubyArray {
    pub fn new() -> Self {
        Self {
            flags: AtomicU32::new(ARY_EMBED_FLAG),
            len: 0,
            capa: ARRAY_EMBED_MAX_LEN,
            storage: ArrayStorage { embed: [0; ARRAY_EMBED_MAX_LEN] },
        }
    }

    pub fn with_capacity(capa: usize) -> Self {
        let mut arr = Self::new();
        if capa > ARRAY_EMBED_MAX_LEN {
            arr.grow_to(capa);
        }
        arr
    }

    pub fn is_embedded(&self) -> bool {
        self.flags.load(Ordering::Relaxed) & ARY_EMBED_FLAG != 0
    }

    pub fn is_frozen(&self) -> bool {
        self.flags.load(Ordering::Relaxed) & ARY_FROZEN_FLAG != 0
    }

    pub fn freeze(&self) {
        self.flags.fetch_or(ARY_FROZEN_FLAG, Ordering::SeqCst);
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn capacity(&self) -> usize {
        self.capa
    }

    /// Get element at index (handles negative indices)
    pub fn get(&self, idx: isize) -> Option<u64> {
        let idx = if idx < 0 {
            let adjusted = self.len as isize + idx;
            if adjusted < 0 { return None; }
            adjusted as usize
        } else {
            idx as usize
        };
        
        if idx >= self.len {
            return None;
        }

        if self.is_embedded() {
            unsafe { Some(self.storage.embed[idx]) }
        } else {
            unsafe { Some(*self.storage.heap.ptr.add(idx)) }
        }
    }

    /// Set element at index
    pub fn set(&mut self, idx: usize, val: u64) {
        if idx >= self.len {
            panic!("index out of bounds");
        }
        
        if self.is_embedded() {
            unsafe { self.storage.embed[idx] = val; }
        } else {
            unsafe { *self.storage.heap.ptr.add(idx) = val; }
        }
    }

    /// Push element to end
    pub fn push(&mut self, val: u64) {
        if self.len >= self.capa {
            self.grow();
        }
        
        if self.is_embedded() {
            unsafe { self.storage.embed[self.len] = val; }
        } else {
            unsafe { *self.storage.heap.ptr.add(self.len) = val; }
        }
        self.len += 1;
    }

    /// Pop element from end
    pub fn pop(&mut self) -> Option<u64> {
        if self.len == 0 {
            return None;
        }
        self.len -= 1;
        let val = if self.is_embedded() {
            unsafe { self.storage.embed[self.len] }
        } else {
            unsafe { *self.storage.heap.ptr.add(self.len) }
        };
        Some(val)
    }

    /// Shift element from front
    pub fn shift(&mut self) -> Option<u64> {
        if self.len == 0 {
            return None;
        }
        let val = if self.is_embedded() {
            unsafe { self.storage.embed[0] }
        } else {
            unsafe { *self.storage.heap.ptr }
        };
        
        // Shift elements left
        for i in 1..self.len {
            let v = if self.is_embedded() {
                unsafe { self.storage.embed[i] }
            } else {
                unsafe { *self.storage.heap.ptr.add(i) }
            };
            if self.is_embedded() {
                unsafe { self.storage.embed[i - 1] = v; }
            } else {
                unsafe { *self.storage.heap.ptr.add(i - 1) = v; }
            }
        }
        self.len -= 1;
        Some(val)
    }

    /// Unshift element to front
    pub fn unshift(&mut self, val: u64) {
        if self.len >= self.capa {
            self.grow();
        }
        
        // Shift elements right
        for i in (0..self.len).rev() {
            let v = if self.is_embedded() {
                unsafe { self.storage.embed[i] }
            } else {
                unsafe { *self.storage.heap.ptr.add(i) }
            };
            if self.is_embedded() {
                unsafe { self.storage.embed[i + 1] = v; }
            } else {
                unsafe { *self.storage.heap.ptr.add(i + 1) = v; }
            }
        }
        if self.is_embedded() {
            unsafe { self.storage.embed[0] = val; }
        } else {
            unsafe { *self.storage.heap.ptr = val; }
        }
        self.len += 1;
    }

    /// Concatenate another array
    pub fn concat(&mut self, other: &RubyArray) {
        let new_len = self.len + other.len;
        if new_len > self.capa {
            self.grow_to(new_len.next_power_of_two());
        }
        
        for i in 0..other.len {
            if let Some(val) = other.get(i as isize) {
                self.push(val);
            }
        }
    }

    fn grow(&mut self) {
        let new_capa = (self.capa * 2).max(ARRAY_DEFAULT_CAPA);
        self.grow_to(new_capa);
    }

    fn grow_to(&mut self, new_capa: usize) {
        let was_embedded = self.is_embedded();
        
        unsafe {
            let layout = Layout::array::<u64>(new_capa).unwrap();
            let new_ptr = alloc(layout) as *mut u64;
            
            if new_ptr.is_null() {
                panic!("allocation failed");
            }
            
            // Copy existing elements
            for i in 0..self.len {
                let val = if was_embedded {
                    self.storage.embed[i]
                } else {
                    *self.storage.heap.ptr.add(i)
                };
                *new_ptr.add(i) = val;
            }
            
            // Free old heap storage if needed
            if !was_embedded {
                let old_layout = Layout::array::<u64>(self.storage.heap.capa).unwrap();
                dealloc(self.storage.heap.ptr as *mut u8, old_layout);
            }
            
            self.storage.heap = ManuallyDrop::new(ArrayHeap {
                ptr: new_ptr,
                capa: new_capa,
                shared_root: std::ptr::null_mut(),
            });
        }
        
        self.flags.fetch_and(!ARY_EMBED_FLAG, Ordering::SeqCst);
        self.capa = new_capa;
    }
}

impl Drop for RubyArray {
    fn drop(&mut self) {
        if !self.is_embedded() {
            unsafe {
                let layout = Layout::array::<u64>(self.storage.heap.capa).unwrap();
                dealloc(self.storage.heap.ptr as *mut u8, layout);
            }
        }
    }
}

impl Default for RubyArray {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_new_array() {
        let arr = RubyArray::new();
        assert!(arr.is_embedded());
        assert_eq!(arr.len(), 0);
        assert_eq!(arr.capacity(), 3);
    }
    
    #[test]
    fn test_push_pop() {
        let mut arr = RubyArray::new();
        arr.push(1);
        arr.push(2);
        arr.push(3);
        assert_eq!(arr.len(), 3);
        assert!(arr.is_embedded());
        
        arr.push(4); // Triggers heap allocation
        assert!(!arr.is_embedded());
        assert_eq!(arr.len(), 4);
        
        assert_eq!(arr.pop(), Some(4));
        assert_eq!(arr.pop(), Some(3));
    }
    
    #[test]
    fn test_get() {
        let mut arr = RubyArray::new();
        arr.push(10);
        arr.push(20);
        arr.push(30);
        
        assert_eq!(arr.get(0), Some(10));
        assert_eq!(arr.get(1), Some(20));
        assert_eq!(arr.get(2), Some(30));
        assert_eq!(arr.get(3), None);
        assert_eq!(arr.get(-1), Some(30)); // Negative index
        assert_eq!(arr.get(-2), Some(20));
    }
    
    #[test]
    fn test_shift_unshift() {
        let mut arr = RubyArray::new();
        arr.push(1);
        arr.push(2);
        arr.push(3);
        
        assert_eq!(arr.shift(), Some(1));
        assert_eq!(arr.len(), 2);
        
        arr.unshift(0);
        assert_eq!(arr.get(0), Some(0));
        assert_eq!(arr.get(1), Some(2));
    }
}

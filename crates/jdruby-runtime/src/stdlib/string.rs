//! # Ruby String Implementation
//!
//! MRI-compatible String with embedded/heap storage and encoding support.
//! Follows the structure of MRI's string.c

use std::mem::ManuallyDrop;
use std::alloc::{alloc, dealloc, realloc, Layout};
use std::sync::atomic::{AtomicU32, Ordering};

pub const STRING_EMBED_LEN_MAX: usize = 24;

/// String flags
pub const STR_EMBED: u32 = 1 << 0;
pub const STR_SHARED: u32 = 1 << 1;
pub const STR_FROZEN: u32 = 1 << 2;
pub const STR_TMPLOCK: u32 = 1 << 3;

/// Ruby String - either inline (up to 24 bytes) or heap-allocated
#[repr(C)]
pub struct RubyString {
    pub flags: AtomicU32,
    pub len: usize,
    pub encoding: u32, // Encoding index
    pub storage: StringStorage,
}

#[repr(C)]
pub union StringStorage {
    pub embed: [u8; STRING_EMBED_LEN_MAX],
    pub heap: ManuallyDrop<StringHeap>,
}

#[repr(C)]
pub struct StringHeap {
    pub ptr: *mut u8,
    pub capa: usize,
    pub shared: *mut RubyString,
}

impl RubyString {
    pub fn new() -> Self {
        Self {
            flags: AtomicU32::new(STR_EMBED),
            len: 0,
            encoding: 0, // UTF-8
            storage: StringStorage { embed: [0; STRING_EMBED_LEN_MAX] },
        }
    }

    pub fn from_str(s: &str) -> Self {
        let mut str = Self::new();
        str.set_str(s);
        str
    }

    pub fn is_embedded(&self) -> bool {
        self.flags.load(Ordering::Relaxed) & STR_EMBED != 0
    }

    pub fn is_frozen(&self) -> bool {
        self.flags.load(Ordering::Relaxed) & STR_FROZEN != 0
    }

    pub fn freeze(&self) {
        self.flags.fetch_or(STR_FROZEN, Ordering::SeqCst);
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn as_bytes(&self) -> &[u8] {
        unsafe {
            if self.is_embedded() {
                std::slice::from_raw_parts(self.storage.embed.as_ptr(), self.len)
            } else {
                std::slice::from_raw_parts(self.storage.heap.ptr, self.len)
            }
        }
    }

    pub fn as_str(&self) -> &str {
        std::str::from_utf8(self.as_bytes()).unwrap_or("")
    }

    pub fn set_str(&mut self, s: &str) {
        let len = s.len();
        if len <= STRING_EMBED_LEN_MAX {
            unsafe {
                self.storage.embed[..len].copy_from_slice(s.as_bytes());
            }
            self.flags.store(STR_EMBED, Ordering::Relaxed);
        } else {
            unsafe {
                let layout = Layout::array::<u8>(len).unwrap();
                let ptr = alloc(layout);
                if ptr.is_null() {
                    panic!("allocation failed");
                }
                ptr.copy_from_nonoverlapping(s.as_bytes().as_ptr(), len);
                self.storage.heap = ManuallyDrop::new(StringHeap {
                    ptr,
                    capa: len,
                    shared: std::ptr::null_mut(),
                });
            }
            self.flags.store(0, Ordering::Relaxed);
        }
        self.len = len;
    }

    pub fn push_char(&mut self, c: char) {
        let mut buf = [0; 4];
        let bytes = c.encode_utf8(&mut buf);
        self.concat(bytes.as_bytes());
    }

    pub fn concat(&mut self, other: &[u8]) {
        let new_len = self.len + other.len();
        
        if self.is_embedded() && new_len <= STRING_EMBED_LEN_MAX {
            unsafe {
                self.storage.embed[self.len..new_len].copy_from_slice(other);
            }
        } else if self.is_embedded() {
            self.spill_to_heap(new_len);
            unsafe {
                self.storage.heap.ptr.add(self.len).copy_from_nonoverlapping(other.as_ptr(), other.len());
            }
        } else {
            unsafe {
                if new_len > (*self.storage.heap).capa {
                    self.grow_heap(new_len);
                }
                (*self.storage.heap).ptr.add(self.len).copy_from_nonoverlapping(other.as_ptr(), other.len());
            }
        }
        self.len = new_len;
    }

    pub fn clone(&self) -> Self {
        Self::from_str(self.as_str())
    }

    fn spill_to_heap(&mut self, min_capa: usize) {
        let capa = min_capa.next_power_of_two();
        unsafe {
            let layout = Layout::array::<u8>(capa).unwrap();
            let ptr = alloc(layout);
            if ptr.is_null() {
                panic!("allocation failed");
            }
            ptr.copy_from_nonoverlapping(self.storage.embed.as_ptr(), self.len);
            self.storage.heap = ManuallyDrop::new(StringHeap {
                ptr,
                capa,
                shared: std::ptr::null_mut(),
            });
        }
        self.flags.store(0, Ordering::Relaxed);
    }

    fn grow_heap(&mut self, min_capa: usize) {
        let new_capa = min_capa.next_power_of_two();
        unsafe {
            let old_layout = Layout::array::<u8>((*self.storage.heap).capa).unwrap();
            let new_ptr = realloc((*self.storage.heap).ptr, old_layout, new_capa);
            if new_ptr.is_null() {
                panic!("reallocation failed");
            }
            (*self.storage.heap).ptr = new_ptr;
            (*self.storage.heap).capa = new_capa;
        }
    }
}

impl Drop for RubyString {
    fn drop(&mut self) {
        if !self.is_embedded() {
            unsafe {
                let layout = Layout::array::<u8>((*self.storage.heap).capa).unwrap();
                dealloc((*self.storage.heap).ptr, layout);
            }
        }
    }
}

impl Default for RubyString {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_new_string() {
        let s = RubyString::new();
        assert!(s.is_embedded());
        assert_eq!(s.len(), 0);
        assert!(s.is_empty());
    }
    
    #[test]
    fn test_from_str_embedded() {
        let s = RubyString::from_str("hello");
        assert!(s.is_embedded());
        assert_eq!(s.len(), 5);
        assert_eq!(s.as_str(), "hello");
    }
    
    #[test]
    fn test_from_str_heap() {
        let s = RubyString::from_str("this is a longer string that exceeds inline");
        assert!(!s.is_embedded());
        assert_eq!(s.as_str(), "this is a longer string that exceeds inline");
    }
    
    #[test]
    fn test_concat() {
        let mut a = RubyString::from_str("hello");
        let b = " world".as_bytes();
        a.concat(b);
        assert_eq!(a.as_str(), "hello world");
    }
    
    #[test]
    fn test_push_char() {
        let mut s = RubyString::from_str("hello");
        s.push_char('!');
        assert_eq!(s.as_str(), "hello!");
    }
    
    #[test]
    fn test_freeze() {
        let s = RubyString::from_str("frozen");
        assert!(!s.is_frozen());
        s.freeze();
        assert!(s.is_frozen());
    }
}

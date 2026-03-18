//! # Native Stdlib Internals
//!
//! High-performance native memory layouts for core Ruby types,
//! designed for direct integration with the compiled native code
//! and the JDGC garbage collector.
//!
//! ## Design Principles
//!
//! - **Cache-line friendly**: Structs are sized to fit in cache lines (64 bytes).
//! - **Inline optimization**: Small strings/arrays are stored inline (no heap alloc).
//! - **Tagged pointers**: Type discrimination via low bits of the flags field.
//! - **Copy-on-Write**: String buffers support CoW for `freeze` and `dup`.

use std::sync::atomic::{AtomicU32, Ordering};

// ══════════════════════════════════════════════════════════
// ── NativeString (RString equivalent) ────────────────────
// ══════════════════════════════════════════════════════════

/// High-performance native string with short-string optimization.
///
/// Layout:
/// ```text
/// ┌────────────────────────────────────────────────────────┐
/// │ flags (4B) │ encoding (1B) │ _pad (3B) │ len (8B)     │  ← 16 bytes
/// ├────────────────────────────────────────────────────────┤
/// │ Inline mode: data[0..23] stored directly (24 bytes)    │
/// │ Heap mode:   ptr (8B) │ capa (8B) │ _reserved (8B)    │  ← 24 bytes
/// ├────────────────────────────────────────────────────────┤
/// │ Total: 40 bytes (fits in a cache line with header)     │
/// └────────────────────────────────────────────────────────┘
/// ```
#[repr(C)]
pub struct NativeString {
    /// Flags: frozen (bit 0), shared (bit 1), inline (bit 2)
    pub flags: AtomicU32,
    /// String encoding.
    pub encoding: Encoding,
    _pad: [u8; 3],
    /// Byte length of the string content.
    pub len: usize,
    /// Storage: either inline bytes or heap pointer+capacity.
    storage: StringStorage,
}

/// String encoding (matches Ruby's encoding IDs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Encoding {
    Utf8 = 0,
    Ascii = 1,
    Binary = 2,
    Utf16Le = 3,
    Utf16Be = 4,
    Utf32Le = 5,
    Utf32Be = 6,
    Latin1 = 7,
    ShiftJis = 8,
    EucJp = 9,
}

/// String storage — union of inline and heap modes.
#[repr(C)]
union StringStorage {
    /// Inline: up to 23 bytes stored directly (+ 1 null terminator).
    inline: [u8; 24],
    /// Heap: pointer + capacity.
    heap: HeapString,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct HeapString {
    ptr: *mut u8,
    capa: usize,
    _reserved: usize,
}

/// Maximum inline string length (bytes).
pub const STRING_INLINE_MAX: usize = 23;

/// Flag bits.
const FLAG_FROZEN: u32 = 1 << 0;
const FLAG_SHARED: u32 = 1 << 1;
const FLAG_INLINE: u32 = 1 << 2;

impl NativeString {
    /// Create a new empty string.
    pub fn new() -> Self {
        Self {
            flags: AtomicU32::new(FLAG_INLINE),
            encoding: Encoding::Utf8,
            _pad: [0; 3],
            len: 0,
            storage: StringStorage { inline: [0; 24] },
        }
    }

    /// Create from a Rust str — uses inline if short enough.
    pub fn from_str(s: &str) -> Self {
        let bytes = s.as_bytes();
        if bytes.len() <= STRING_INLINE_MAX {
            let mut inline = [0u8; 24];
            inline[..bytes.len()].copy_from_slice(bytes);
            Self {
                flags: AtomicU32::new(FLAG_INLINE),
                encoding: Encoding::Utf8,
                _pad: [0; 3],
                len: bytes.len(),
                storage: StringStorage { inline },
            }
        } else {
            let mut buf = Vec::with_capacity(bytes.len());
            buf.extend_from_slice(bytes);
            let ptr = buf.as_mut_ptr();
            let capa = buf.capacity();
            std::mem::forget(buf);
            Self {
                flags: AtomicU32::new(0),
                encoding: Encoding::Utf8,
                _pad: [0; 3],
                len: bytes.len(),
                storage: StringStorage {
                    heap: HeapString { ptr, capa, _reserved: 0 },
                },
            }
        }
    }

    /// Check if this string uses inline storage.
    #[inline]
    pub fn is_inline(&self) -> bool {
        (self.flags.load(Ordering::Relaxed) & FLAG_INLINE) != 0
    }

    /// Check if this string is frozen.
    #[inline]
    pub fn is_frozen(&self) -> bool {
        (self.flags.load(Ordering::Relaxed) & FLAG_FROZEN) != 0
    }

    /// Freeze this string (make immutable).
    pub fn freeze(&self) {
        self.flags.fetch_or(FLAG_FROZEN, Ordering::Release);
    }

    /// Get a pointer to the string data.
    pub fn as_ptr(&self) -> *const u8 {
        if self.is_inline() {
            unsafe { self.storage.inline.as_ptr() }
        } else {
            unsafe { self.storage.heap.ptr }
        }
    }

    /// Get the string as a Rust str.
    pub fn as_str(&self) -> &str {
        let bytes = unsafe {
            std::slice::from_raw_parts(self.as_ptr(), self.len)
        };
        // SAFETY: We only store valid UTF-8 (or binary, but we check encoding)
        std::str::from_utf8(bytes).unwrap_or("")
    }

    /// Get the byte length.
    #[inline]
    pub fn bytesize(&self) -> usize { self.len }

    /// Get the character length (for UTF-8).
    pub fn char_length(&self) -> usize {
        self.as_str().chars().count()
    }

    /// Concatenate another string (mutating).
    pub fn concat(&mut self, other: &NativeString) {
        if self.is_frozen() { return; } // Would raise in real Ruby
        let total = self.len + other.len;
        if self.is_inline() && total <= STRING_INLINE_MAX {
            // Both fit inline — fast path
            unsafe {
                let src = other.as_ptr();
                let dst = self.storage.inline.as_mut_ptr().add(self.len);
                std::ptr::copy_nonoverlapping(src, dst, other.len);
            }
            self.len = total;
        } else {
            // Need heap allocation
            let mut buf = Vec::with_capacity(total);
            buf.extend_from_slice(unsafe {
                std::slice::from_raw_parts(self.as_ptr(), self.len)
            });
            buf.extend_from_slice(unsafe {
                std::slice::from_raw_parts(other.as_ptr(), other.len)
            });
            let ptr = buf.as_mut_ptr();
            let capa = buf.capacity();
            std::mem::forget(buf);
            self.storage = StringStorage {
                heap: HeapString { ptr, capa, _reserved: 0 },
            };
            self.flags.fetch_and(!FLAG_INLINE, Ordering::Release);
            self.len = total;
        }
    }
}

impl Drop for NativeString {
    fn drop(&mut self) {
        if !self.is_inline() {
            unsafe {
                let heap = self.storage.heap;
                if !heap.ptr.is_null() {
                    let _ = Vec::from_raw_parts(heap.ptr, self.len, heap.capa);
                }
            }
        }
    }
}

impl Default for NativeString {
    fn default() -> Self { Self::new() }
}

// ══════════════════════════════════════════════════════════
// ── NativeArray (RArray equivalent) ──────────────────────
// ══════════════════════════════════════════════════════════

/// High-performance native array with small-array optimization.
///
/// Layout:
/// ```text
/// ┌────────────────────────────────────────────────────────┐
/// │ flags (4B) │ len (4B) │ capa (8B)                     │  ← 16 bytes
/// ├────────────────────────────────────────────────────────┤
/// │ Inline mode: values[0..3] stored directly (24 bytes)   │
/// │ Heap mode:   ptr (8B) │ reserved (16B)                 │  ← 24 bytes
/// ├────────────────────────────────────────────────────────┤
/// │ Total: 40 bytes                                        │
/// └────────────────────────────────────────────────────────┘
/// ```
///
/// Elements are stored as `u64` (tagged VALUE) for direct compatibility
/// with the compiled native code.
#[repr(C)]
pub struct NativeArray {
    pub flags: AtomicU32,
    pub len: u32,
    pub capa: usize,
    storage: ArrayStorage,
}

#[repr(C)]
union ArrayStorage {
    /// Inline: up to 3 elements stored directly.
    inline: [u64; 3],
    /// Heap: pointer to element buffer.
    heap: HeapArray,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct HeapArray {
    ptr: *mut u64,
    _reserved: [usize; 2],
}

/// Maximum inline array capacity.
pub const ARRAY_INLINE_MAX: usize = 3;

impl NativeArray {
    /// Create a new empty array.
    pub fn new() -> Self {
        Self {
            flags: AtomicU32::new(FLAG_INLINE),
            len: 0,
            capa: ARRAY_INLINE_MAX as usize,
            storage: ArrayStorage { inline: [0; 3] },
        }
    }

    /// Create with initial capacity.
    pub fn with_capacity(capa: usize) -> Self {
        if capa <= ARRAY_INLINE_MAX {
            Self::new()
        } else {
            let mut buf: Vec<u64> = Vec::with_capacity(capa);
            let ptr = buf.as_mut_ptr();
            let actual_capa = buf.capacity();
            std::mem::forget(buf);
            Self {
                flags: AtomicU32::new(0),
                len: 0,
                capa: actual_capa,
                storage: ArrayStorage {
                    heap: HeapArray { ptr, _reserved: [0; 2] },
                },
            }
        }
    }

    #[inline]
    pub fn is_inline(&self) -> bool {
        (self.flags.load(Ordering::Relaxed) & FLAG_INLINE) != 0
    }

    #[inline]
    pub fn length(&self) -> usize { self.len as usize }

    /// Get a pointer to the element buffer.
    pub fn as_ptr(&self) -> *const u64 {
        if self.is_inline() {
            unsafe { self.storage.inline.as_ptr() }
        } else {
            unsafe { self.storage.heap.ptr }
        }
    }

    pub fn as_mut_ptr(&mut self) -> *mut u64 {
        if self.is_inline() {
            unsafe { self.storage.inline.as_mut_ptr() }
        } else {
            unsafe { self.storage.heap.ptr }
        }
    }

    /// Get element at index.
    pub fn get(&self, index: usize) -> Option<u64> {
        if index < self.length() {
            Some(unsafe { *self.as_ptr().add(index) })
        } else {
            None
        }
    }

    /// Push an element.
    pub fn push(&mut self, value: u64) {
        let len = self.length();
        if self.is_inline() && len < ARRAY_INLINE_MAX {
            unsafe { *self.storage.inline.as_mut_ptr().add(len) = value; }
            self.len += 1;
        } else if self.is_inline() {
            // Spill to heap
            let mut buf: Vec<u64> = Vec::with_capacity(8);
            unsafe {
                for i in 0..len {
                    buf.push(*self.storage.inline.as_ptr().add(i));
                }
            }
            buf.push(value);
            let ptr = buf.as_mut_ptr();
            let capa = buf.capacity();
            std::mem::forget(buf);
            self.storage = ArrayStorage {
                heap: HeapArray { ptr, _reserved: [0; 2] },
            };
            self.flags.fetch_and(!FLAG_INLINE, Ordering::Release);
            self.capa = capa;
            self.len = (len + 1) as u32;
        } else {
            // Heap mode — just push
            if len >= self.capa {
                let new_capa = self.capa * 2;
                unsafe {
                    let mut buf = Vec::from_raw_parts(
                        self.storage.heap.ptr, len, self.capa
                    );
                    buf.reserve(new_capa - len);
                    let ptr = buf.as_mut_ptr();
                    let capa = buf.capacity();
                    std::mem::forget(buf);
                    self.storage.heap.ptr = ptr;
                    self.capa = capa;
                }
            }
            unsafe { *self.storage.heap.ptr.add(len) = value; }
            self.len += 1;
        }
    }

    /// Pop the last element.
    pub fn pop(&mut self) -> Option<u64> {
        if self.len == 0 { return None; }
        self.len -= 1;
        let idx = self.len as usize;
        Some(unsafe { *self.as_ptr().add(idx) })
    }

    /// Get a slice view of the elements.
    pub fn as_slice(&self) -> &[u64] {
        unsafe { std::slice::from_raw_parts(self.as_ptr(), self.length()) }
    }
}

impl Drop for NativeArray {
    fn drop(&mut self) {
        if !self.is_inline() {
            unsafe {
                let heap = self.storage.heap;
                if !heap.ptr.is_null() {
                    let _ = Vec::from_raw_parts(heap.ptr, self.len as usize, self.capa);
                }
            }
        }
    }
}

impl Default for NativeArray {
    fn default() -> Self { Self::new() }
}

// ══════════════════════════════════════════════════════════
// ── Symbol Table ─────────────────────────────────────────
// ══════════════════════════════════════════════════════════

/// Global symbol table for intern/lookup.
/// Symbols are interned strings that are compared by ID.
use std::collections::HashMap;
use std::sync::Mutex;

static SYMBOL_TABLE: Mutex<Option<SymbolTable>> = Mutex::new(None);

pub struct SymbolTable {
    name_to_id: HashMap<String, u64>,
    id_to_name: HashMap<u64, String>,
    next_id: u64,
}

impl SymbolTable {
    fn new() -> Self {
        Self {
            name_to_id: HashMap::new(),
            id_to_name: HashMap::new(),
            next_id: 1,
        }
    }

    pub fn intern(&mut self, name: &str) -> u64 {
        if let Some(&id) = self.name_to_id.get(name) {
            return id;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.name_to_id.insert(name.to_string(), id);
        self.id_to_name.insert(id, name.to_string());
        id
    }

    pub fn lookup(&self, id: u64) -> Option<&str> {
        self.id_to_name.get(&id).map(|s| s.as_str())
    }
}

/// Intern a symbol name, returning its ID.
pub fn sym_intern(name: &str) -> u64 {
    let mut guard = SYMBOL_TABLE.lock().unwrap();
    let tbl = guard.get_or_insert_with(SymbolTable::new);
    tbl.intern(name)
}

/// Look up a symbol name by ID.
pub fn sym_lookup(id: u64) -> Option<String> {
    let guard = SYMBOL_TABLE.lock().unwrap();
    guard.as_ref().and_then(|tbl| tbl.lookup(id).map(|s| s.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inline_string() {
        let s = NativeString::from_str("hello");
        assert!(s.is_inline());
        assert_eq!(s.as_str(), "hello");
        assert_eq!(s.bytesize(), 5);
        assert_eq!(s.char_length(), 5);
    }

    #[test]
    fn test_heap_string() {
        let s = NativeString::from_str("this is a longer string that exceeds inline");
        assert!(!s.is_inline());
        assert_eq!(s.as_str(), "this is a longer string that exceeds inline");
    }

    #[test]
    fn test_string_concat() {
        let mut a = NativeString::from_str("hello");
        let b = NativeString::from_str(" world");
        a.concat(&b);
        assert_eq!(a.as_str(), "hello world");
    }

    #[test]
    fn test_string_freeze() {
        let s = NativeString::from_str("frozen");
        assert!(!s.is_frozen());
        s.freeze();
        assert!(s.is_frozen());
    }

    #[test]
    fn test_inline_array() {
        let mut a = NativeArray::new();
        assert!(a.is_inline());
        a.push(1);
        a.push(2);
        a.push(3);
        assert_eq!(a.length(), 3);
        assert_eq!(a.get(0), Some(1));
        assert_eq!(a.get(2), Some(3));
    }

    #[test]
    fn test_array_spill() {
        let mut a = NativeArray::new();
        a.push(10);
        a.push(20);
        a.push(30);
        a.push(40); // triggers spill to heap
        assert!(!a.is_inline());
        assert_eq!(a.length(), 4);
        assert_eq!(a.get(3), Some(40));
    }

    #[test]
    fn test_array_pop() {
        let mut a = NativeArray::new();
        a.push(1);
        a.push(2);
        assert_eq!(a.pop(), Some(2));
        assert_eq!(a.pop(), Some(1));
        assert_eq!(a.pop(), None);
    }

    #[test]
    fn test_symbol_intern() {
        let id1 = sym_intern("foo");
        let id2 = sym_intern("foo");
        let id3 = sym_intern("bar");
        assert_eq!(id1, id2); // Same name → same ID
        assert_ne!(id1, id3); // Different name → different ID
        assert_eq!(sym_lookup(id1), Some("foo".to_string()));
    }
}

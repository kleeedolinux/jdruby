//! # VALUE — MRI-Compatible Tagged Pointer Representation
//!
//! Re-exports core types from `jdruby_common::ffi_types` and provides
//! FFI-specific struct layouts (RString, RArray, RHash).

pub use jdruby_common::ffi_types::*;

// ═════════════════════════════════════════════════════════════════════════════
// FFI-Specific Struct Layouts (extended versions for C interop)
// ═════════════════════════════════════════════════════════════════════════════

/// MRI-compatible `RString` layout for the FFI boundary.
///
/// ```c
/// struct RString {
///     struct RBasic basic;
///     union {
///         struct { long len; char *ptr; long capa; } heap;
///         struct { char ary[24]; } embed;
///     } as;
/// };
/// ```
#[repr(C)]
pub struct RString {
    pub basic: RBasic,
    /// Length in bytes.
    pub len: isize,
    /// Pointer to the string buffer (heap-allocated).
    pub ptr: *mut u8,
    /// Capacity of the allocated buffer.
    pub capa: isize,
}

/// Embedded string optimization threshold (bytes).
/// Strings <= this size are stored inline in the object.
pub const RSTRING_EMBED_LEN_MAX: usize = 24;

/// MRI-compatible `RArray` layout for the FFI boundary.
///
/// ```c
/// struct RArray {
///     struct RBasic basic;
///     union {
///         struct { long len; VALUE *ptr; long capa; } heap;
///         VALUE ary[3];
///     } as;
/// };
/// ```
#[repr(C)]
pub struct RArray {
    pub basic: RBasic,
    /// Number of elements.
    pub len: isize,
    /// Pointer to the element buffer.
    pub ptr: *mut VALUE,
    /// Capacity.
    pub capa: isize,
}

/// Embedded array capacity (number of elements stored inline).
pub const RARRAY_EMBED_LEN_MAX: usize = 3;

/// Simplified RHash for FFI bridging.
#[repr(C)]
pub struct RHash {
    pub basic: RBasic,
    /// Number of entries.
    pub size: isize,
    /// Pointer to the hash table entries.
    pub entries: *mut HashEntry,
}

/// A single hash table entry.
#[repr(C)]
pub struct HashEntry {
    pub key: VALUE,
    pub val: VALUE,
    pub next: *mut HashEntry,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixnum_roundtrip() {
        for i in [-100i64, -1, 0, 1, 42, 1000000, i64::MAX >> 2] {
            let v = rb_int2fix(i);
            assert!(rb_fixnum_p(v));
            assert_eq!(rb_fix2long(v), i);
        }
    }

    #[test]
    fn test_special_consts() {
        assert!(rb_nil_p(RUBY_QNIL));
        assert!(rb_true_p(RUBY_QTRUE));
        assert!(rb_false_p(RUBY_QFALSE));
        assert!(!rb_test(RUBY_QFALSE));
        assert!(!rb_test(RUBY_QNIL));
        assert!(rb_test(RUBY_QTRUE));
        assert!(rb_test(rb_int2fix(0))); // 0 is truthy in Ruby!
    }

    #[test]
    fn test_symbol_roundtrip() {
        let id: ID = 12345;
        let sym = rb_id2sym(id);
        assert!(rb_symbol_p(sym));
        assert_eq!(rb_sym2id(sym), id);
    }
}

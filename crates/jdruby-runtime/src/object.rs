//! # MRI-Compatible Object Model
//!
//! Exact memory layout matching MRI Ruby's C structs for ABI compatibility.
//! All structs use #[repr(C)] to match the C memory layout.

use std::os::raw::{c_char, c_int, c_long, c_void};
use std::mem::ManuallyDrop;
use jdruby_common::ffi_types::{VALUE, RBasic, rb_special_const_p};

// ═════════════════════════════════════════════════════════════════════════════
// RObject — Generic Ruby Object
// ═════════════════════════════════════════════════════════════════════════════

/// MRI-compatible RObject struct (from include/ruby/internal/core/robject.h)
#[repr(C)]
pub struct RObject {
    pub basic: RBasic,
    pub as_: RObjectUnion,
}

#[repr(C)]
pub union RObjectUnion {
    pub heap: RObjectHeap,
    pub ary: [VALUE; 1],
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct RObjectHeap {
    pub fields: *mut VALUE,
    pub numiv: u32,
    pub iv_index_tbl: *mut c_void,
}

/// ROBJECT_HEAP flag — indicates external ivar storage
pub const ROBJECT_HEAP: VALUE = 1 << 4;

/// Max embedded instance variables
pub const ROBJECT_EMBED_LEN_MAX: usize = 3;

// ═════════════════════════════════════════════════════════════════════════════
// RString — Ruby String
// ═════════════════════════════════════════════════════════════════════════════

/// MRI-compatible RString struct
#[repr(C)]
pub struct RString {
    pub basic: RBasic,
    pub as_: RStringUnion,
}

#[repr(C)]
pub union RStringUnion {
    pub heap: ManuallyDrop<RStringHeap>,
    pub embed: [u8; RSTRING_EMBED_LEN_MAX],
}

#[repr(C)]
pub struct RStringHeap {
    pub len: c_long,
    pub ptr: *mut c_char,
    pub aux: RStringAux,
}

#[repr(C)]
pub union RStringAux {
    pub capa: c_long,
    pub shared: *mut c_void,
}

pub const RSTRING_NOEMBED: VALUE = 1 << 1;
pub const RSTRING_FSTR: VALUE = 1 << 2;
pub const RSTRING_EMBED_LEN_MAX: usize = 24;

// ═════════════════════════════════════════════════════════════════════════════
// RArray — Ruby Array
// ═════════════════════════════════════════════════════════════════════════════

/// MRI-compatible RArray struct
#[repr(C)]
pub struct RArray {
    pub basic: RBasic,
    pub as_: RArrayUnion,
}

#[repr(C)]
pub union RArrayUnion {
    pub heap: ManuallyDrop<RArrayHeap>,
    pub ary: [VALUE; RARRAY_EMBED_LEN_MAX],
}

#[repr(C)]
pub struct RArrayHeap {
    pub len: c_long,
    pub ptr: *mut VALUE,
    pub aux: RArrayAux,
}

#[repr(C)]
pub union RArrayAux {
    pub capa: c_long,
    pub shared: *mut c_void,
}

pub const RARRAY_EMBED_FLAG: VALUE = 1 << 1;
pub const RARRAY_EMBED_LEN_MAX: usize = 3;

// ═════════════════════════════════════════════════════════════════════════════
// RHash — Ruby Hash
// ═════════════════════════════════════════════════════════════════════════════

#[repr(C)]
pub struct RHash {
    pub basic: RBasic,
    pub tbl: *mut c_void,
    pub iter_lev: c_int,
    pub ifnone: VALUE,
    pub flags: u32,
}

pub const RHASH_PROC_DEFAULT: u32 = 1 << 0;
pub const RHASH_STRICT_MODE: u32 = 1 << 1;

// ═════════════════════════════════════════════════════════════════════════════
// RClass — Ruby Class/Module
// ═════════════════════════════════════════════════════════════════════════════

bitflags::bitflags! {
    #[repr(C)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct RClassFlags: u32 {
        const FL_SINGLETON = 1 << 0;
        const FL_MODULE = 1 << 1;
        const FL_PREPENDED = 1 << 2;
        const FL_INCLUDED = 1 << 3;
        const FL_EXTENDED = 1 << 4;
        const FL_ALLOCATING = 1 << 5;
    }
}

#[repr(C)]
pub struct RClass {
    pub basic: RBasic,
    pub super_: *mut RClass,
    pub flags: RClassFlags,
    pub m_tbl: *mut c_void,
    pub const_tbl: *mut c_void,
    pub iv_tbl: *mut c_void,
    pub class_serial: u64,
    pub subclasses: *mut c_void,
}

// ═════════════════════════════════════════════════════════════════════════════
// RFloat — Ruby Float
// ═════════════════════════════════════════════════════════════════════════════

#[repr(C)]
pub struct RFloat {
    pub basic: RBasic,
    pub float_value: f64,
}

// ═════════════════════════════════════════════════════════════════════════════
// RData — C Extension Data Objects
// ═════════════════════════════════════════════════════════════════════════════

#[repr(C)]
pub struct RData {
    pub basic: RBasic,
    pub dmark: Option<unsafe extern "C" fn(*mut c_void)>,
    pub dfree: Option<unsafe extern "C" fn(*mut c_void)>,
    pub data: *mut c_void,
}

// ═════════════════════════════════════════════════════════════════════════════
// Type Tag Constants
// ═════════════════════════════════════════════════════════════════════════════

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(non_camel_case_types)]
pub enum RubyValueType {
    T_NONE = 0x00, T_OBJECT = 0x01, T_CLASS = 0x02, T_MODULE = 0x03,
    T_FLOAT = 0x04, T_STRING = 0x05, T_REGEXP = 0x06, T_ARRAY = 0x07,
    T_HASH = 0x08, T_STRUCT = 0x09, T_BIGNUM = 0x0a, T_FILE = 0x0b,
    T_DATA = 0x0c, T_MATCH = 0x0d, T_COMPLEX = 0x0e, T_RATIONAL = 0x0f,
    T_NIL = 0x11, T_TRUE = 0x12, T_FALSE = 0x13, T_SYMBOL = 0x14,
    T_FIXNUM = 0x15, T_UNDEF = 0x16, T_IMEMO = 0x18, T_NODE = 0x1b,
    T_ICLASS = 0x1c, T_ZOMBIE = 0x1d, T_MOVED = 0x1e,
}

#[inline(always)]
pub const fn rb_type(flags: VALUE) -> u32 {
    (flags & 0x1f) as u32
}

#[inline(always)]
pub unsafe fn rb_obj_is_type(obj: VALUE, typ: RubyValueType) -> bool {
    if rb_special_const_p(obj) {
        return false;
    }
    let basic = obj as *const RBasic;
    rb_type((*basic).flags) == typ as u32
}

// ═════════════════════════════════════════════════════════════════════════════
// Object Header Accessors
// ═════════════════════════════════════════════════════════════════════════════

#[inline]
pub unsafe fn rb_obj_class(obj: VALUE) -> VALUE {
    let basic = obj as *const RBasic;
    (*basic).klass
}

#[inline]
pub unsafe fn rb_obj_set_class(obj: VALUE, klass: VALUE) {
    let basic = obj as *mut RBasic;
    (*basic).klass = klass;
}

#[inline]
pub unsafe fn rb_obj_flags(obj: VALUE) -> VALUE {
    let basic = obj as *const RBasic;
    (*basic).flags
}

#[inline]
pub unsafe fn rb_obj_set_flags(obj: VALUE, flags: VALUE) {
    let basic = obj as *mut RBasic;
    (*basic).flags = flags;
}

// ═════════════════════════════════════════════════════════════════════════════
// FFI-Safe Object References
// ═════════════════════════════════════════════════════════════════════════════

pub struct FfiRef<T> {
    ptr: *mut T,
    gc_header: *mut jdgc::ObjectHeader,
}

impl<T> FfiRef<T> {
    pub unsafe fn new(ptr: *mut T) -> Self {
        let header = jdgc::ObjectHeader::from_ptr(ptr as *mut u8);
        (*header).pin();
        Self { ptr, gc_header: header }
    }
    pub fn as_ptr(&self) -> *mut T { self.ptr }
}

impl<T> Drop for FfiRef<T> {
    fn drop(&mut self) {
        unsafe { (*self.gc_header).unpin(); }
    }
}

unsafe impl<T> Send for FfiRef<T> {}
unsafe impl<T> Sync for FfiRef<T> {}

// ═════════════════════════════════════════════════════════════════════════════
// Legacy Rust-native types (kept for compatibility during transition)
// ═════════════════════════════════════════════════════════════════════════════

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub enum RubyValue {
    Nil, Bool(bool), Integer(i64), Float(f64), Symbol(u64),
    String(RubyString), Array(Vec<RubyValue>), Hash(RubyHash),
    Object(Box<RubyObject>),
}

#[derive(Debug, Clone)]
pub struct RubyString {
    pub data: String,
    pub frozen: bool,
}

#[derive(Debug, Clone)]
pub struct RubyHash {
    pub entries: Vec<(RubyValue, RubyValue)>,
}

#[derive(Debug, Clone)]
pub struct RubyObject {
    pub class_id: u64,
    pub ivars: HashMap<String, RubyValue>,
    pub frozen: bool,
}

impl RubyValue {
    pub fn is_truthy(&self) -> bool { !matches!(self, Self::Nil | Self::Bool(false)) }
    pub fn is_nil(&self) -> bool { matches!(self, Self::Nil) }
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Nil => "NilClass", Self::Bool(true) => "TrueClass",
            Self::Bool(false) => "FalseClass", Self::Integer(_) => "Integer",
            Self::Float(_) => "Float", Self::Symbol(_) => "Symbol",
            Self::String(_) => "String", Self::Array(_) => "Array",
            Self::Hash(_) => "Hash", Self::Object(_) => "Object",
        }
    }
}

impl Default for RubyValue {
    fn default() -> Self { Self::Nil }
}

// ═════════════════════════════════════════════════════════════════════════════
// Tests
// ═════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_rbasic_size() {
        assert_eq!(std::mem::size_of::<RBasic>(), 16);
    }
    #[test]
    fn test_robject_size() {
        let size = std::mem::size_of::<RObject>();
        assert!(size >= 24, "RObject too small: {}", size);
    }
    #[test]
    fn test_rstring_size() {
        let size = std::mem::size_of::<RString>();
        assert!(size >= 40, "RString too small: {}", size);
    }
    #[test]
    fn test_rarray_size() {
        let size = std::mem::size_of::<RArray>();
        assert!(size >= 40, "RArray too small: {}", size);
    }
}

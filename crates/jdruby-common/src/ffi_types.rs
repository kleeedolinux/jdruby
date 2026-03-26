//! # FFI Type Definitions
//!
//! Core C-ABI types shared between jdruby-runtime and jdruby-ffi.
//! This prevents cyclic dependencies between the runtime and FFI crates.

/// The fundamental C-ABI value type — identical to MRI's `VALUE`.
/// On 64-bit systems this is `unsigned long` / `usize`.
pub type VALUE = usize;

/// MRI-compatible method ID (symbol ID for method lookup).
pub type ID = usize;

// ── Special constants ──────────────────────────────────────

/// `false` — the only falsy value besides nil.
pub const RUBY_QFALSE: VALUE = 0x00;
/// `true` (64-bit MRI uses 0x14)
pub const RUBY_QTRUE: VALUE = 0x14;
/// `nil` (64-bit MRI uses 0x08)
pub const RUBY_QNIL: VALUE = 0x08;
/// Undefined/uninitialized slot.
pub const RUBY_QUNDEF: VALUE = 0x34;

/// Fixnum tag bit.
pub const RUBY_FIXNUM_FLAG: VALUE = 0x01;
/// Symbol tag mask.
pub const RUBY_SYMBOL_FLAG: VALUE = 0x0C;
/// Flonum tag (MRI 64-bit uses 0x02 in lowest 2 bits for Flonum).
pub const RUBY_FLONUM_MASK: VALUE = 0x03;
pub const RUBY_FLONUM_FLAG: VALUE = 0x02;

// ── Tag testing ────────────────────────────────────────────

/// Test if VALUE is a tagged Fixnum.
#[inline(always)]
pub const fn rb_fixnum_p(v: VALUE) -> bool {
    (v & RUBY_FIXNUM_FLAG) != 0
}

/// Test if VALUE is `nil`.
#[inline(always)]
pub const fn rb_nil_p(v: VALUE) -> bool {
    v == RUBY_QNIL
}

/// Test if VALUE is `true`.
#[inline(always)]
pub const fn rb_true_p(v: VALUE) -> bool {
    v == RUBY_QTRUE
}

/// Test if VALUE is `false`.
#[inline(always)]
pub const fn rb_false_p(v: VALUE) -> bool {
    v == RUBY_QFALSE
}

/// Test if VALUE is a Symbol.
#[inline(always)]
pub const fn rb_symbol_p(v: VALUE) -> bool {
    (v & 0xFF) == RUBY_SYMBOL_FLAG
}

/// Test if VALUE is a Flonum (inline float).
#[inline(always)]
pub const fn rb_flonum_p(v: VALUE) -> bool {
    (v & RUBY_FLONUM_MASK) == RUBY_FLONUM_FLAG
}

/// Test if VALUE is a special constant (not a heap pointer).
#[inline(always)]
pub const fn rb_special_const_p(v: VALUE) -> bool {
    rb_fixnum_p(v) || rb_symbol_p(v) || rb_flonum_p(v)
        || v == RUBY_QFALSE || v == RUBY_QTRUE
        || v == RUBY_QNIL || v == RUBY_QUNDEF
}

/// Test Ruby truthiness: everything except `false` and `nil`.
#[inline(always)]
pub const fn rb_test(v: VALUE) -> bool {
    v != RUBY_QFALSE && v != RUBY_QNIL
}

// ── Fixnum encoding/decoding ───────────────────────────────

/// Encode an `i64` as a tagged Fixnum VALUE.
#[inline(always)]
pub const fn rb_int2fix(i: i64) -> VALUE {
    ((i as usize) << 1) | RUBY_FIXNUM_FLAG
}

/// Decode a tagged Fixnum VALUE to `i64`.
#[inline(always)]
pub const fn rb_fix2long(v: VALUE) -> i64 {
    (v as i64) >> 1
}

// ── Symbol encoding/decoding ──────────────────────────────

/// Encode a symbol ID as a tagged Symbol VALUE.
#[inline(always)]
pub const fn rb_id2sym(id: ID) -> VALUE {
    (id << 8) | RUBY_SYMBOL_FLAG
}

/// Decode a tagged Symbol VALUE to its ID.
#[inline(always)]
pub const fn rb_sym2id(v: VALUE) -> ID {
    v >> 8
}

// ── RBasic Header ─────────────────────────────────────────

/// The `RBasic` struct header for all heap objects.
/// Every heap-allocated Ruby object starts with this.
///
/// Layout matches MRI's `RBasic`:
/// ```c
/// struct RBasic {
///     VALUE flags;  // type + GC flags
///     VALUE klass;  // class pointer
/// };
/// ```
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct RBasic {
    /// Object flags: type tag (bits 0-4), GC mark, freeze, etc.
    pub flags: VALUE,
    /// Pointer to the class of this object.
    pub klass: VALUE,
}

/// Ruby built-in type tags (stored in RBasic::flags bits 0-4).
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RubyType {
    None = 0x00, Object = 0x01, Class = 0x02, Module = 0x03,
    Float = 0x04, String = 0x05, Regexp = 0x06, Array = 0x07,
    Hash = 0x08, Struct = 0x09, Bignum = 0x0A, File = 0x0B,
    Data = 0x0C, Match = 0x0D, Complex = 0x0E, Rational = 0x0F,
    Nil = 0x11, True = 0x12, False = 0x13, Symbol = 0x14,
    Fixnum = 0x15, Undef = 0x16, Imemo = 0x18, Node = 0x1B,
    IClass = 0x1C, Zombie = 0x1D, Moved = 0x1E,
}

/// Extract the type tag from RBasic flags.
#[inline(always)]
pub const fn rb_type(flags: VALUE) -> u32 {
    (flags & 0x1f) as u32
}

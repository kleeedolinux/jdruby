//! # Bridge — JDRuby ↔ MRI VALUE Conversion
//!
//! This module provides zero-copy (where possible) conversion between
//! JDRuby's internal `jdruby_runtime::value::RubyValue` (Rust enum) and
//! the MRI-compatible `VALUE` (tagged `usize`) used at the C-ABI boundary.
//!
//! ## Strategy
//!
//! - **Fixnum**: Direct tag encoding. No allocation.
//! - **Bool/Nil**: Direct special constant. No allocation.
//! - **Symbol**: Tag-encode the interned ID. No allocation.
//! - **String**: Allocate an `RString` on the JDRuby heap, return pointer as VALUE.
//! - **Array**: Allocate an `RArray` on the JDRuby heap, return pointer as VALUE.
//! - **Object**: Wrap in `RBasic` header, return pointer as VALUE.
//!
//! The reverse direction (VALUE → RubyValue) inspects the tag bits to
//! determine the type, then reads the data.

use std::collections::HashMap;
use std::sync::Mutex;
use crate::value::*;

// ── Global heap for FFI-bridged objects ───────────────────

/// Objects allocated for FFI bridging. We keep them alive here
/// to prevent Rust from dropping them while C code holds the pointer.
///
/// In production, this would integrate with the JDGC heap allocator.
/// For now, we use a simple arena approach.
static FFI_ARENA: Mutex<Option<FfiArena>> = Mutex::new(None);

struct FfiArena {
    /// String objects: pointer → (RString header + heap buffer)
    strings: HashMap<usize, FfiBridgedString>,
    /// Array objects: pointer → (RArray header + element buffer)
    arrays: HashMap<usize, FfiBridgedArray>,
    /// Generic objects
    objects: HashMap<usize, FfiBridgedObject>,
    next_id: usize,
}

struct FfiBridgedString {
    #[allow(dead_code)]
    header: RBasic,
    data: Vec<u8>,
}

struct FfiBridgedArray {
    #[allow(dead_code)]
    header: RBasic,
    elements: Vec<VALUE>,
}

struct FfiBridgedObject {
    header: RBasic,
    class_name: String,
}

impl FfiArena {
    fn new() -> Self {
        Self {
            strings: HashMap::new(),
            arrays: HashMap::new(),
            objects: HashMap::new(),
            next_id: 0x1000, // Start above special constants
        }
    }

    fn alloc_id(&mut self) -> usize {
        let id = self.next_id;
        self.next_id += 8; // Keep 8-byte alignment (tag bits clear)
        id
    }
}

fn with_arena<F, R>(f: F) -> R
where
    F: FnOnce(&mut FfiArena) -> R,
{
    let mut guard = FFI_ARENA.lock().unwrap();
    let arena = guard.get_or_insert_with(FfiArena::new);
    f(arena)
}

// ── JDRuby → MRI VALUE ──────────────────────────────────

/// Convert a JDRuby runtime value to an MRI-compatible VALUE.
///
/// - Immediates (int, bool, nil, symbol) are tagged inline.
/// - Heap types (string, array, object) are allocated in the FFI arena.
pub fn jdruby_to_value(rv: &jdruby_runtime::value::RubyValue) -> VALUE {
    use jdruby_runtime::value::RubyValue as RV;

    match rv {
        RV::Integer(i) => rb_int2fix(*i),
        RV::Float(f) => {
            // For FFI, we box floats as heap objects.
            // In production this would use flonum encoding if it fits.
            let bits = f.to_bits();
            // Store as a pseudo-pointer (tagged)
            with_arena(|arena| {
                let id = arena.alloc_id();
                // Store float bits in the ID for retrieval
                arena.objects.insert(id, FfiBridgedObject {
                    header: RBasic {
                        flags: RubyType::Float as usize,
                        klass: 0,
                    },
                    class_name: format!("Float:{}", bits),
                });
                id
            })
        }
        RV::True => RUBY_QTRUE,
        RV::False => RUBY_QFALSE,
        RV::Nil => RUBY_QNIL,
        RV::Symbol(id) => rb_id2sym(*id as usize),
        RV::String(rs) => {
            with_arena(|arena| {
                let id = arena.alloc_id();
                arena.strings.insert(id, FfiBridgedString {
                    header: RBasic {
                        flags: RubyType::String as usize,
                        klass: 0,
                    },
                    data: rs.data.as_bytes().to_vec(),
                });
                id
            })
        }
        RV::Array(elements) => {
            let values: Vec<VALUE> = elements.iter().map(|e| jdruby_to_value(e)).collect();
            with_arena(|arena| {
                let id = arena.alloc_id();
                arena.arrays.insert(id, FfiBridgedArray {
                    header: RBasic {
                        flags: RubyType::Array as usize,
                        klass: 0,
                    },
                    elements: values,
                });
                id
            })
        }
        RV::Hash(_) => {
            // Simplified: allocate as generic object
            with_arena(|arena| {
                let id = arena.alloc_id();
                arena.objects.insert(id, FfiBridgedObject {
                    header: RBasic {
                        flags: RubyType::Hash as usize,
                        klass: 0,
                    },
                    class_name: "Hash".into(),
                });
                id
            })
        }
        RV::Object(obj) => {
            with_arena(|arena| {
                let id = arena.alloc_id();
                arena.objects.insert(id, FfiBridgedObject {
                    header: RBasic {
                        flags: RubyType::Object as usize,
                        klass: obj.class_id as usize,
                    },
                    class_name: obj.class_name.clone(),
                });
                id
            })
        }
        // Proc, Range, Class, Module — simplified bridging
        _ => RUBY_QNIL,
    }
}

// ── MRI VALUE → JDRuby ──────────────────────────────────

/// Convert an MRI-compatible VALUE back to a JDRuby runtime value.
pub fn value_to_jdruby(v: VALUE) -> jdruby_runtime::value::RubyValue {
    use jdruby_runtime::value::{RubyValue as RV, RubyString};

    // Check immediate types first (no heap access)
    if v == RUBY_QNIL {
        return RV::Nil;
    }
    if v == RUBY_QTRUE {
        return RV::True;
    }
    if v == RUBY_QFALSE {
        return RV::False;
    }
    if rb_fixnum_p(v) {
        return RV::Integer(rb_fix2long(v));
    }
    if rb_symbol_p(v) {
        return RV::Symbol(rb_sym2id(v) as u64);
    }

    // Heap object — look up in arena
    with_arena(|arena| {
        if let Some(s) = arena.strings.get(&v) {
            let data = String::from_utf8_lossy(&s.data).into_owned();
            return RV::String(RubyString::new(data));
        }
        if let Some(a) = arena.arrays.get(&v) {
            let elements: Vec<RV> = a.elements.iter().map(|e| value_to_jdruby(*e)).collect();
            return RV::Array(elements);
        }
        if let Some(obj) = arena.objects.get(&v) {
            let type_tag = rb_type(obj.header.flags);
            if type_tag == RubyType::Float as u32 {
                // Recover float bits from class_name
                if let Some(bits_str) = obj.class_name.strip_prefix("Float:") {
                    if let Ok(bits) = bits_str.parse::<u64>() {
                        return RV::Float(f64::from_bits(bits));
                    }
                }
            }
            return RV::Nil; // Fallback for unknown objects
        }
        RV::Nil
    })
}

// ── Convenience helpers ──────────────────────────────────

/// Get the string data from a bridged VALUE (if it's a string).
pub fn value_to_str(v: VALUE) -> Option<String> {
    with_arena(|arena| {
        arena.strings.get(&v).map(|s| {
            String::from_utf8_lossy(&s.data).into_owned()
        })
    })
}

/// Get the array length from a bridged VALUE (if it's an array).
pub fn value_ary_len(v: VALUE) -> Option<usize> {
    with_arena(|arena| {
        arena.arrays.get(&v).map(|a| a.elements.len())
    })
}

/// Get array element at index.
pub fn value_ary_entry(v: VALUE, idx: usize) -> Option<VALUE> {
    with_arena(|arena| {
        arena.arrays.get(&v).and_then(|a| a.elements.get(idx).copied())
    })
}

/// Create a new string VALUE from a Rust string.
pub fn str_to_value(s: &str) -> VALUE {
    with_arena(|arena| {
        let id = arena.alloc_id();
        arena.strings.insert(id, FfiBridgedString {
            header: RBasic {
                flags: RubyType::String as usize,
                klass: 0,
            },
            data: s.as_bytes().to_vec(),
        });
        id
    })
}

/// Free all FFI-bridged objects (called during GC sweep).
pub fn ffi_arena_sweep() {
    with_arena(|arena| {
        arena.strings.clear();
        arena.arrays.clear();
        arena.objects.clear();
    });
}

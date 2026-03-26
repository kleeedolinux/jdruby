//! # Bridge — JDGC-Aware JDRuby ↔ MRI VALUE Conversion
//!
//! This module provides zero-copy (where possible) conversion between
//! JDRuby's internal `jdruby_runtime::value::RubyValue` (Rust enum) and
//! the MRI-compatible `VALUE` (tagged `usize`) used at the C-ABI boundary.
//!
//! ## JDGC Integration
//!
//! - Objects are allocated on the JDGC heap via `Allocator`
//! - FFI references use JDGC pinning to prevent evacuation during C calls
//! - Thread-safe global registry using `RwLock` for read-heavy operations
//!
//! ## Strategy
//!
//! - **Fixnum**: Direct tag encoding. No allocation.
//! - **Bool/Nil**: Direct special constant. No allocation.
//! - **Symbol**: Tag-encode the interned ID. No allocation.
//! - **String**: Allocate `RString` on JDGC heap, return pointer as VALUE.
//! - **Array**: Allocate `RArray` on JDGC heap, return pointer as VALUE.
//! - **Object**: Wrap in `RBasic` header, return pointer as VALUE.

use std::alloc::Layout;
use std::collections::HashMap;
use std::sync::{RwLock, Mutex};
use std::sync::atomic::{AtomicU64, Ordering};
use crate::value::*;
use jdgc::{Allocator, GcPtr, ObjectHeader, RegionManager};
use std::sync::Arc;

// ── JDGC-Aware Global Registry ───────────────────────────

/// Thread-safe global registry for FFI-bridged objects.
/// Uses RwLock for read-heavy operations (lookups) and Mutex for write operations.
struct GlobalRegistry {
    /// String objects: VALUE → GcPtr<RString>
    strings: RwLock<HashMap<VALUE, GcPtr<crate::value::RString>>>,
    /// Array objects: VALUE → GcPtr<RArray>
    arrays: RwLock<HashMap<VALUE, GcPtr<crate::value::RArray>>>,
    /// Generic objects: VALUE → (GcPtr<ObjectHeader>, type_tag)
    objects: RwLock<HashMap<VALUE, (GcPtr<u8>, u32)>>,
    /// Allocator for JDGC heap allocation
    allocator: Mutex<Allocator>,
    /// Next unique ID for VALUE generation
    next_id: AtomicU64,
}

impl GlobalRegistry {
    fn new() -> Self {
        let regions = Arc::new(RegionManager::new(64 * 1024 * 1024).expect("Failed to create regions"));
        let allocator = Allocator::new(regions).expect("Failed to create allocator");
        
        Self {
            strings: RwLock::new(HashMap::new()),
            arrays: RwLock::new(HashMap::new()),
            objects: RwLock::new(HashMap::new()),
            allocator: Mutex::new(allocator),
            next_id: AtomicU64::new(0x10000), // Start above special constants, 8-byte aligned
        }
    }

    /// Generate next unique VALUE
    fn alloc_value(&self) -> VALUE {
        self.next_id.fetch_add(8, Ordering::SeqCst) as VALUE
    }

    /// Allocate object on JDGC heap
    fn allocate<T>(&self, layout: Layout) -> Option<GcPtr<T>> {
        let allocator = self.allocator.lock().unwrap();
        allocator.allocate(layout).ok().map(|ptr| {
            GcPtr::from_raw(ptr.as_ptr() as *mut T).unwrap()
        })
    }
}

/// Singleton global registry
static GLOBAL_REGISTRY: RwLock<Option<GlobalRegistry>> = RwLock::new(None);

/// Initialize the global registry (called once at startup)
pub fn init_bridge() {
    let mut guard = GLOBAL_REGISTRY.write().unwrap();
    *guard = Some(GlobalRegistry::new());
}

/// Access the global registry for read operations
fn with_registry_read<F, R>(f: F) -> R
where
    F: FnOnce(&GlobalRegistry) -> R,
{
    let guard = GLOBAL_REGISTRY.read().unwrap();
    let registry = guard.as_ref().expect("Bridge not initialized");
    f(registry)
}

/// Access the global registry for write operations
fn with_registry_write<F, R>(f: F) -> R
where
    F: FnOnce(&GlobalRegistry) -> R,
{
    let guard = GLOBAL_REGISTRY.read().unwrap();
    let registry = guard.as_ref().expect("Bridge not initialized");
    f(registry)
}

// GlobalRegistry contains GC-managed pointers which are safe to share across threads
// The RwLock ensures proper synchronization for the HashMaps
unsafe impl Send for GlobalRegistry {}
unsafe impl Sync for GlobalRegistry {}

// ── JDRuby → MRI VALUE ──────────────────────────────────

/// Convert a JDRuby runtime value to an MRI-compatible VALUE.
///
/// - Immediates (int, bool, nil, symbol) are tagged inline.
/// - Heap types (string, array, object) are allocated on JDGC heap with pinning.
pub fn jdruby_to_value(rv: &jdruby_runtime::value::RubyValue) -> VALUE {
    use jdruby_runtime::value::RubyValue as RV;

    match rv {
        RV::Integer(i) => rb_int2fix(*i),
        RV::Float(f) => {
            // For FFI, we box floats as heap objects on JDGC heap
            let bits = f.to_bits();
            let layout = Layout::new::<(ObjectHeader, u64)>();
            
            with_registry_write(|registry| {
                let ptr = registry.allocate::<u8>(layout)?;
                let value = registry.alloc_value();
                
                // Store float bits in the allocated memory
                let data_ptr = unsafe { ptr.as_ptr().add(std::mem::size_of::<ObjectHeader>()) as *mut u64 };
                unsafe { *data_ptr = bits; }
                
                // Pin the object for FFI safety
                ptr.header().pin();
                
                // Register in objects table with Float type tag
                let mut objects = registry.objects.write().unwrap();
                objects.insert(value, (ptr, RubyType::Float as u32));
                
                Some(value)
            }).unwrap_or(RUBY_QNIL)
        }
        RV::True => RUBY_QTRUE,
        RV::False => RUBY_QFALSE,
        RV::Nil => RUBY_QNIL,
        RV::Symbol(id) => rb_id2sym(*id as usize),
        RV::String(rs) => {
            let _data = rs.data.as_bytes();
            let layout = Layout::from_size_align(
                std::mem::size_of::<ObjectHeader>() + std::mem::size_of::<crate::value::RString>(),
                8
            ).unwrap();
            
            with_registry_write(|registry| {
                let ptr = registry.allocate::<crate::value::RString>(layout)?;
                let value = registry.alloc_value();
                
                // Initialize RString (simplified - would need proper init)
                let rstring_ptr = unsafe { 
                    ptr.as_ptr().add(std::mem::size_of::<ObjectHeader>()) as *mut crate::value::RString 
                };
                
                // Pin for FFI safety
                ptr.header().pin();
                
                // Register
                let mut strings = registry.strings.write().unwrap();
                strings.insert(value, GcPtr::from_raw(rstring_ptr).unwrap());
                
                Some(value)
            }).unwrap_or(RUBY_QNIL)
        }
        RV::Array(_elements) => {
            let _values: Vec<VALUE> = _elements.iter().map(|e| jdruby_to_value(e)).collect();
            
            with_registry_write(|registry| {
                // Allocate space for RArray header + elements
                let layout = Layout::from_size_align(
                    std::mem::size_of::<ObjectHeader>() + std::mem::size_of::<crate::value::RArray>(),
                    8
                ).unwrap();
                
                let ptr = registry.allocate::<crate::value::RArray>(layout)?;
                let value = registry.alloc_value();
                
                // Initialize RArray (simplified)
                let rarray_ptr = unsafe { 
                    ptr.as_ptr().add(std::mem::size_of::<ObjectHeader>()) as *mut crate::value::RArray 
                };
                
                // Pin for FFI safety
                ptr.header().pin();
                
                // Store element count in a side table (would need proper implementation)
                let mut arrays = registry.arrays.write().unwrap();
                arrays.insert(value, GcPtr::from_raw(rarray_ptr).unwrap());
                
                Some(value)
            }).unwrap_or(RUBY_QNIL)
        }
        RV::Hash(_) => {
            // Simplified: allocate as generic object
            with_registry_write(|registry| {
                let layout = Layout::from_size_align(
                    std::mem::size_of::<ObjectHeader>() + 64, // placeholder size
                    8
                ).unwrap();
                
                let ptr = registry.allocate::<u8>(layout)?;
                let value = registry.alloc_value();
                
                // Pin for FFI safety
                ptr.header().pin();
                
                let mut objects = registry.objects.write().unwrap();
                objects.insert(value, (ptr, RubyType::Hash as u32));
                
                Some(value)
            }).unwrap_or(RUBY_QNIL)
        }
        RV::Object(_obj) => {
            with_registry_write(|registry| {
                let layout = Layout::from_size_align(
                    std::mem::size_of::<ObjectHeader>() + std::mem::size_of::<jdruby_runtime::object::RObject>(),
                    8
                ).unwrap();
                
                let ptr = registry.allocate::<u8>(layout)?;
                let value = registry.alloc_value();
                
                // Pin for FFI safety
                ptr.header().pin();
                
                let mut objects = registry.objects.write().unwrap();
                objects.insert(value, (ptr, RubyType::Object as u32));
                
                Some(value)
            }).unwrap_or(RUBY_QNIL)
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

    // Heap object — look up in global registry
    with_registry_read(|registry| {
        // Check strings first
        {
            let strings = registry.strings.read().unwrap();
            if strings.contains_key(&v) {
                // For now return a placeholder - full RString decoding would be complex
                return Some(RV::String(RubyString::new("[string]".to_string())));
            }
        }
        
        // Check arrays
        {
            let arrays = registry.arrays.read().unwrap();
            if arrays.contains_key(&v) {
                return Some(RV::Array(vec![])); // Placeholder
            }
        }
        
        // Check generic objects
        {
            let objects = registry.objects.read().unwrap();
            if let Some((ptr, type_tag)) = objects.get(&v) {
                if *type_tag == RubyType::Float as u32 {
                    // Recover float bits from the allocated memory
                    let data_ptr = unsafe { 
                        ptr.as_ptr().add(std::mem::size_of::<ObjectHeader>()) as *const u64 
                    };
                    let bits = unsafe { *data_ptr };
                    return Some(RV::Float(f64::from_bits(bits)));
                }
                return Some(RV::Nil); // Fallback for other types
            }
        }
        
        Some(RV::Nil)
    }).unwrap_or(RV::Nil)
}

// ── Convenience helpers ──────────────────────────────────

/// Get the string data from a bridged VALUE (if it's a string).
pub fn value_to_str(v: VALUE) -> Option<String> {
    with_registry_read(|registry| {
        let strings = registry.strings.read().unwrap();
        if strings.contains_key(&v) {
            // Simplified - would need proper RString decoding
            Some("[string]".to_string())
        } else {
            None
        }
    })
}

/// Get the array length from a bridged VALUE (if it's an array).
pub fn value_ary_len(v: VALUE) -> Option<usize> {
    with_registry_read(|registry| {
        let arrays = registry.arrays.read().unwrap();
        if arrays.contains_key(&v) {
            Some(0) // Placeholder - would need proper RArray length access
        } else {
            None
        }
    })
}

/// Get array element at index.
pub fn value_ary_entry(v: VALUE, _idx: usize) -> Option<VALUE> {
    with_registry_read(|registry| {
        let arrays = registry.arrays.read().unwrap();
        if arrays.contains_key(&v) {
            None // Placeholder - would need proper RArray access
        } else {
            None
        }
    })
}

/// Create a new string VALUE from a Rust string.
pub fn str_to_value(_s: &str) -> VALUE {
    let layout = Layout::from_size_align(
        std::mem::size_of::<ObjectHeader>() + std::mem::size_of::<crate::value::RString>(),
        8
    ).unwrap();
    
    with_registry_write(|registry| {
        let ptr = registry.allocate::<crate::value::RString>(layout)?;
        let value = registry.alloc_value();
        
        // Pin for FFI safety
        ptr.header().pin();
        
        // Initialize RString (simplified)
        let rstring_ptr = unsafe { 
            ptr.as_ptr().add(std::mem::size_of::<ObjectHeader>()) as *mut crate::value::RString 
        };
        
        let mut strings = registry.strings.write().unwrap();
        strings.insert(value, GcPtr::from_raw(rstring_ptr).unwrap());
        
        Some(value)
    }).unwrap_or(RUBY_QNIL)
}

/// Unpin and remove all FFI-bridged objects (called during GC sweep).
/// This unpins objects so they can be collected, and clears the registry.
pub fn ffi_registry_sweep() {
    with_registry_write(|registry| {
        // Unpin all strings
        {
            let strings = registry.strings.read().unwrap();
            for (_, ptr) in strings.iter() {
                ptr.header().unpin();
            }
        }
        registry.strings.write().unwrap().clear();
        
        // Unpin all arrays
        {
            let arrays = registry.arrays.read().unwrap();
            for (_, ptr) in arrays.iter() {
                ptr.header().unpin();
            }
        }
        registry.arrays.write().unwrap().clear();
        
        // Unpin all generic objects
        {
            let objects = registry.objects.read().unwrap();
            for (_, (ptr, _)) in objects.iter() {
                ptr.header().unpin();
            }
        }
        registry.objects.write().unwrap().clear();
    });
}

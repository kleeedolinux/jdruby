//! # Deduplication — String and Array Value Cache
//!
//! Provides deduplication for frequently created values to reduce allocations.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use crate::core::VALUE;

/// String pool for deduplicating string values.
static STRING_POOL: RwLock<Option<StringPool>> = RwLock::new(None);

/// A pool of interned strings (same content = same VALUE).
pub struct StringPool {
    /// Content → VALUE mapping (weak reference would be ideal, but keeping simple)
    map: HashMap<Arc<str>, VALUE>,
}

impl StringPool {
    fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    /// Register a newly allocated string in the pool.
    fn register(&mut self, content: &str, value: VALUE) {
        let key: Arc<str> = Arc::from(content);
        self.map.insert(key, value);
    }
}

/// Convert a Rust string to a VALUE, using deduplication.
pub fn str_to_value(s: &str) -> VALUE {
    // Fast path: check if already interned
    {
        let pool = STRING_POOL.read().unwrap();
        if let Some(ref p) = *pool {
            if let Some(&value) = p.map.get(&Arc::<str>::from(s)) {
                return value;
            }
        }
    }

    // Need to allocate
    allocate_and_register_string(s)
}

/// Get string content from a VALUE.
pub fn value_to_str(v: VALUE) -> Option<String> {
    use crate::bridge::registry::{with_registry, ObjectRef};
    
    with_registry(|registry| {
        if let Some(ObjectRef::String(ptr)) = registry.get(v) {
            unsafe {
                let rstring = &*ptr.as_ptr();
                let len = rstring.len as usize;
                let data_ptr = rstring.ptr;
                let bytes = std::slice::from_raw_parts(data_ptr, len);
                String::from_utf8(bytes.to_vec()).ok()
            }
        } else {
            None
        }
    })
}

/// CRITICAL FIX: Extract Ruby string content from a VALUE.
/// This is a dedicated function for extracting strings from Ruby string VALUEs.
/// Unlike value_to_jdruby().to_ruby_string(), this properly handles the
/// internal Ruby string representation.
pub fn extract_rstring_content(v: VALUE) -> Option<String> {
    value_to_str(v)
}

/// Get array length from a VALUE.
pub fn value_ary_len(v: VALUE) -> Option<usize> {
    use crate::bridge::registry::{with_registry, ObjectRef};
    
    with_registry(|registry| {
        if let Some(ObjectRef::Array(ptr)) = registry.get(v) {
            unsafe {
                let rarray = &*ptr.as_ptr();
                Some(rarray.len as usize)
            }
        } else {
            None
        }
    })
}

/// Get array element at index.
pub fn value_ary_entry(v: VALUE, idx: usize) -> Option<VALUE> {
    use crate::bridge::registry::{with_registry, ObjectRef};
    
    with_registry(|registry| {
        if let Some(ObjectRef::Array(ptr)) = registry.get(v) {
            unsafe {
                let rarray = &*ptr.as_ptr();
                if idx < rarray.len as usize {
                    let data_ptr = rarray.ptr;
                    Some(*data_ptr.add(idx))
                } else {
                    None
                }
            }
        } else {
            None
        }
    })
}

fn allocate_and_register_string(s: &str) -> VALUE {
    // For now, delegate to conversion module
    // In a full implementation, this would allocate on JDGC heap
    // and register in the pool
    use crate::bridge::conversion::allocate_rstring;
    
    let value = allocate_rstring(s).expect("Failed to allocate string");
    
    // Register in pool
    let mut pool = STRING_POOL.write().unwrap();
    let p = pool.get_or_insert_with(StringPool::new);
    p.register(s, value);
    
    value
}

/// Initialize the deduplication system.
pub fn init_dedup() {
    let mut pool = STRING_POOL.write().unwrap();
    *pool = Some(StringPool::new());
}

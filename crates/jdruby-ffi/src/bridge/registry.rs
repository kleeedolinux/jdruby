//! # Object Registry — Unified Storage for FFI-Bound Objects
//!
//! Replaces the multiple HashMaps in the old GlobalRegistry with a single
//! unified HashMap: VALUE → ObjectRef. This reduces lock contention and
//! simplifies lookup logic.

use std::collections::HashMap;
use std::sync::{RwLock, Mutex};
use std::sync::atomic::{AtomicU64, Ordering};
use jdgc::GcPtr;
use crate::core::VALUE;

// FFI-specific struct layouts
use crate::bridge::pinning::unpin_object;

/// MRI-compatible `RString` layout for the FFI boundary.
#[repr(C)]
pub struct RString {
    pub basic: crate::core::RBasic,
    pub len: isize,
    pub ptr: *mut u8,
    pub capa: isize,
}

// SAFETY: RString is used in FFI context and is thread-safe when properly managed
unsafe impl Send for RString {}
unsafe impl Sync for RString {}

/// MRI-compatible `RArray` layout for the FFI boundary.
#[repr(C)]
pub struct RArray {
    pub basic: crate::core::RBasic,
    pub len: isize,
    pub ptr: *mut VALUE,
    pub capa: isize,
}

// SAFETY: RArray is used in FFI context and is thread-safe when properly managed
unsafe impl Send for RArray {}
unsafe impl Sync for RArray {}

/// MRI-compatible `RBasic` header.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct RBasic {
    pub flags: VALUE,
    pub klass: VALUE,
}

/// Reference to an object stored in the registry.
pub enum ObjectRef {
    String(GcPtr<RString>),
    Array(GcPtr<RArray>),
    Float(GcPtr<u64>),  // Stores float bits
    Object(GcPtr<u8>),  // Generic object with header
}

impl Clone for ObjectRef {
    fn clone(&self) -> Self {
        match self {
            ObjectRef::String(ptr) => ObjectRef::String(GcPtr::from_raw(ptr.as_ptr()).expect("GcPtr should not be null")),
            ObjectRef::Array(ptr) => ObjectRef::Array(GcPtr::from_raw(ptr.as_ptr()).expect("GcPtr should not be null")),
            ObjectRef::Float(ptr) => ObjectRef::Float(GcPtr::from_raw(ptr.as_ptr()).expect("GcPtr should not be null")),
            ObjectRef::Object(ptr) => ObjectRef::Object(GcPtr::from_raw(ptr.as_ptr()).expect("GcPtr should not be null")),
        }
    }
}

/// Unified object registry.
pub struct Registry {
    /// VALUE → Object reference (unified storage)
    objects: RwLock<HashMap<VALUE, ObjectRef>>,
    /// Lock for ID generation
    next_id_lock: Mutex<()>,
    /// Next unique VALUE ID
    next_id: AtomicU64,
}

impl Registry {
    fn new() -> Self {
        Self {
            objects: RwLock::new(HashMap::new()),
            next_id_lock: Mutex::new(()),
            next_id: AtomicU64::new(0x10000), // Start above special constants
        }
    }

    /// Generate next unique VALUE (8-byte aligned).
    pub fn alloc_value(&self) -> VALUE {
        let _guard = self.next_id_lock.lock().unwrap();
        self.next_id.fetch_add(8, Ordering::SeqCst) as VALUE
    }

    /// Insert an object into the registry.
    pub fn insert(&self, value: VALUE, obj_ref: ObjectRef) {
        let mut objects = self.objects.write().unwrap();
        objects.insert(value, obj_ref);
    }

    /// Get a reference to an object.
    pub fn get(&self, value: VALUE) -> Option<ObjectRef> {
        let objects = self.objects.read().unwrap();
        objects.get(&value).cloned()
    }

    /// Remove an object from the registry.
    pub fn remove(&self, value: VALUE) -> Option<ObjectRef> {
        let mut objects = self.objects.write().unwrap();
        objects.remove(&value)
    }

    /// Get all registered values (for sweep operations).
    pub fn all_values(&self) -> Vec<VALUE> {
        let objects = self.objects.read().unwrap();
        objects.keys().copied().collect()
    }

    /// Clear all objects from the registry.
    pub fn clear(&self) {
        let mut objects = self.objects.write().unwrap();
        objects.clear();
    }

    /// Get the count of registered objects.
    pub fn len(&self) -> usize {
        let objects = self.objects.read().unwrap();
        objects.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Global registry singleton.
static REGISTRY: RwLock<Option<Registry>> = RwLock::new(None);

/// Initialize the bridge (called once at startup).
pub fn init_bridge() {
    let mut guard = REGISTRY.write().unwrap();
    *guard = Some(Registry::new());
}

/// Access the global registry.
pub fn with_registry<F, R>(f: F) -> R
where
    F: FnOnce(&Registry) -> R,
{
    let guard = REGISTRY.read().unwrap();
    let registry = guard.as_ref().expect("Bridge not initialized");
    f(registry)
}

/// Unpin and remove all FFI-bridged objects (called during GC sweep).
pub fn ffi_registry_sweep() {
    let values = with_registry(|r| r.all_values());
    
    for value in values {
        if let Some(obj_ref) = with_registry(|r| r.remove(value)) {
            match obj_ref {
                ObjectRef::String(ptr) => unpin_object(ptr.as_ptr() as *mut u8),
                ObjectRef::Array(ptr) => unpin_object(ptr.as_ptr() as *mut u8),
                ObjectRef::Float(ptr) => unpin_object(ptr.as_ptr() as *mut u8),
                ObjectRef::Object(ptr) => unpin_object(ptr.as_ptr()),
            }
        }
    }
}

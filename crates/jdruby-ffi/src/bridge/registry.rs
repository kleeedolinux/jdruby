//! # Object Registry — Unified Storage for FFI-Bound Objects
//!
//! Replaces the multiple HashMaps in the old GlobalRegistry with a single
//! unified HashMap: VALUE → ObjectRef. This reduces lock contention and
//! simplifies lookup logic.

use std::collections::HashMap;
use std::sync::{RwLock, Mutex, LazyLock};
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
    /// VALUE → Class VALUE (object class tracking)
    object_classes: RwLock<HashMap<VALUE, VALUE>>,
    /// Lock for ID generation
    next_id_lock: Mutex<()>,
    /// Next unique VALUE ID
    next_id: AtomicU64,
    /// Block storage for metaprogramming - keyed by block VALUE (not func_name)
    /// This ensures each block instance has its own captures
    blocks: RwLock<HashMap<VALUE, (String, Vec<VALUE>)>>,
    /// Current block for implicit block parameter
    current_block: RwLock<Option<VALUE>>,
    /// Current self (receiver) for method calls
    current_self: RwLock<Option<VALUE>>,
}

impl Registry {
    fn new() -> Self {
        Self {
            objects: RwLock::new(HashMap::new()),
            object_classes: RwLock::new(HashMap::new()),
            next_id_lock: Mutex::new(()),
            next_id: AtomicU64::new(0x10000), // Start above special constants
            blocks: RwLock::new(HashMap::new()),
            current_block: RwLock::new(None),
            current_self: RwLock::new(None),
        }
    }

    /// Generate next unique VALUE (8-byte aligned).
    pub fn alloc_value(&self) -> VALUE {
        let _guard = self.next_id_lock.lock().unwrap();
        self.next_id.fetch_add(8, Ordering::SeqCst) as VALUE
    }

    /// Insert an object into the registry with its class.
    pub fn insert_with_class(&self, value: VALUE, obj_ref: ObjectRef, class: VALUE) {
        let mut objects = self.objects.write().unwrap();
        objects.insert(value, obj_ref);
        drop(objects);
        // Store class separately
        let mut classes = self.object_classes.write().unwrap();
        classes.insert(value, class);
    }

    /// Get the class of an object.
    pub fn get_class(&self, value: VALUE) -> Option<VALUE> {
        let classes = self.object_classes.read().unwrap();
        classes.get(&value).copied()
    }

    /// Get a reference to an object.
    pub fn get(&self, value: VALUE) -> Option<ObjectRef> {
        let objects = self.objects.read().unwrap();
        objects.get(&value).cloned()
    }

    /// Insert an object into the registry (without class info - backward compatible).
    pub fn insert(&self, value: VALUE, obj_ref: ObjectRef) {
        let mut objects = self.objects.write().unwrap();
        objects.insert(value, obj_ref);
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

    /// Store a block with captured variables - uses block VALUE as key for uniqueness
    pub fn store_block(&self, block_value: VALUE, func_symbol: &str, captured: Vec<VALUE>) {
        let mut blocks = self.blocks.write().unwrap();
        blocks.insert(block_value, (func_symbol.to_string(), captured));
    }

    /// Get current block
    pub fn get_current_block(&self) -> Option<VALUE> {
        let block = self.current_block.read().unwrap();
        *block
    }

    /// Set current block
    pub fn set_current_block(&self, block: Option<VALUE>) {
        let mut current = self.current_block.write().unwrap();
        *current = block;
    }

    /// Get current self (receiver)
    pub fn get_current_self(&self) -> Option<VALUE> {
        let self_val = self.current_self.read().unwrap();
        *self_val
    }

    /// Set current self (receiver)
    pub fn set_current_self(&self, self_val: Option<VALUE>) {
        let mut current = self.current_self.write().unwrap();
        *current = self_val;
    }

    /// Get captured variables for a block by its VALUE (not func_name)
    pub fn get_block_captures(&self, block_value: VALUE) -> Option<(String, Vec<VALUE>)> {
        let blocks = self.blocks.read().unwrap();
        blocks.get(&block_value).cloned()
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

/// Get global bridge for FFI functions (simplified access)
pub fn get_global_bridge() -> Option<&'static Registry> {
    // Use a static OnceLock for safe global access
    use std::sync::OnceLock;
    static GLOBAL_ACCESS: OnceLock<&'static Registry> = OnceLock::new();
    
    GLOBAL_ACCESS.get_or_init(|| {
        let guard = REGISTRY.read().unwrap();
        if let Some(registry) = guard.as_ref() {
            let ptr: *const Registry = registry;
            // This is safe because REGISTRY lives for program duration
            unsafe { &*ptr }
        } else {
            // Return a dummy registry if none exists
            static EMPTY_REGISTRY: LazyLock<Registry> = LazyLock::new(|| Registry::new());
            &EMPTY_REGISTRY
        }
    });
    
    Some(GLOBAL_ACCESS.get().copied().unwrap_or_else(|| {
        static EMPTY_REGISTRY: LazyLock<Registry> = LazyLock::new(|| Registry::new());
        &EMPTY_REGISTRY
    }))
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

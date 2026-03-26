//! # Ruby ABI Compatibility Layer
//!
//! Provides compatibility with MRI's C API expectations for GC.
//! Similar to MMTk's abi.rs but adapted for JDGC's architecture.

/// Offset from object start to object reference (payload).
/// JDGC puts header at start, so payload is after header.
pub const OBJREF_OFFSET: usize = 8; // size_of::<ObjectHeader>()

/// Minimum object alignment.
pub const MIN_OBJ_ALIGN: usize = 8;

/// GC thread kinds.
pub const GC_THREAD_KIND_WORKER: i32 = 1;
pub const GC_THREAD_KIND_CONTROLLER: i32 = 2;

/// Hidden header prefix (like MMTk's HiddenHeader).
/// Stores object size for Ruby's GC expectations.
#[repr(C)]
pub struct HiddenHeader {
    /// Size mask for payload.
    pub prefix: usize,
}

/// Mask for valid size bits.
const HIDDEN_SIZE_MASK: usize = 0x0000FFFFFFFFFFFF;

impl HiddenHeader {
    /// Check if header is valid.
    #[inline(always)]
    pub fn is_sane(&self) -> bool {
        self.prefix & !HIDDEN_SIZE_MASK == 0
    }

    /// Get payload size.
    #[inline(always)]
    pub fn payload_size(&self) -> usize {
        debug_assert!(self.is_sane(), "Hidden header corrupted: {:x}", self.prefix);
        self.prefix & HIDDEN_SIZE_MASK
    }

    /// Set payload size.
    #[inline(always)]
    pub fn set_payload_size(&mut self, size: usize) {
        debug_assert!(size <= HIDDEN_SIZE_MASK);
        self.prefix = size & HIDDEN_SIZE_MASK;
    }
}

/// JDGC object access for Ruby FFI.
pub struct JdgcObjectAccess {
    obj_start: *mut u8,
}

impl JdgcObjectAccess {
    /// Create from object reference (payload pointer).
    pub fn from_objref(objref: *mut u8) -> Self {
        Self {
            obj_start: unsafe { objref.sub(OBJREF_OFFSET) },
        }
    }

    /// Get object start (including hidden header).
    pub fn obj_start(&self) -> *mut u8 {
        self.obj_start
    }

    /// Get payload address (what Ruby sees as object).
    pub fn payload_addr(&self) -> *mut u8 {
        unsafe { self.obj_start.add(OBJREF_OFFSET) }
    }

    /// Get hidden header.
    pub fn hidden_header(&self) -> &HiddenHeader {
        unsafe { &*(self.obj_start as *const HiddenHeader) }
    }

    /// Get object size.
    pub fn object_size(&self) -> usize {
        std::mem::size_of::<HiddenHeader>() + self.hidden_header().payload_size()
    }
}

/// GC statistics for Ruby VM.
#[repr(C)]
pub struct JdgcStats {
    /// Total heap size.
    pub heap_size: usize,
    /// Used heap size.
    pub used_size: usize,
    /// Number of GC cycles.
    pub gc_count: usize,
    /// Total GC time (ms).
    pub gc_time_ms: usize,
}

// C-compatible GC interface.
// Functions exported to Ruby VM.
extern "C" {
    /// Initialize JDGC.
    pub fn jdgc_init(heap_size: usize) -> bool;
    
    /// Shutdown JDGC.
    pub fn jdgc_shutdown();
    
    /// Allocate object.
    pub fn jdgc_allocate(size: usize) -> *mut u8;
    
    /// Trigger GC.
    pub fn jdgc_collect();
    
    /// Get GC stats.
    pub fn jdgc_stats() -> JdgcStats;
    
    /// Register root.
    pub fn jdgc_register_root(ptr: *mut u8) -> usize;
    
    /// Unregister root.
    pub fn jdgc_unregister_root(handle: usize);
}

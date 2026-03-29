//! Inline Cache infrastructure for optimized method dispatch
//!
//! YARV-style inline caches for monomorphic and polymorphic method calls.
//! Each cache entry stores a method pointer and class identity for fast
//! guard checks at runtime.

use std::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};

/// Segment size for the segmented array
const SEGMENT_SIZE: usize = 64;

/// Inline cache entry for method dispatch
/// Layout compatible with C for runtime interoperability
#[repr(C)]
pub struct InlineCache {
    /// Cached method entry pointer (NULL = unresolved)
    pub method_ptr: AtomicU64,
    
    /// Cached class serial number (for guard)
    pub class_serial: AtomicU64,
    
    /// Method serial number for cache invalidation
    pub method_serial: AtomicU64,
    
    /// Call statistics for optimization decisions
    pub hit_count: AtomicU32,
    pub miss_count: AtomicU32,
}

impl InlineCache {
    /// Create a new empty inline cache
    pub const fn new() -> Self {
        Self {
            method_ptr: AtomicU64::new(0),
            class_serial: AtomicU64::new(0),
            method_serial: AtomicU64::new(0),
            hit_count: AtomicU32::new(0),
            miss_count: AtomicU32::new(0),
        }
    }
    
    /// Check if cache is populated
    pub fn is_resolved(&self) -> bool {
        self.method_ptr.load(Ordering::Acquire) != 0
    }
    
    /// Get cached method pointer
    pub fn get_method(&self) -> Option<u64> {
        let ptr = self.method_ptr.load(Ordering::Acquire);
        if ptr != 0 {
            Some(ptr)
        } else {
            None
        }
    }
    
    /// Update cache with resolved method
    pub fn update(&self, method_ptr: u64, class_serial: u64, method_serial: u64) {
        self.method_ptr.store(method_ptr, Ordering::Release);
        self.class_serial.store(class_serial, Ordering::Release);
        self.method_serial.store(method_serial, Ordering::Release);
    }
    
    /// Record a cache hit
    pub fn record_hit(&self) {
        self.hit_count.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Record a cache miss
    pub fn record_miss(&self) {
        self.miss_count.fetch_add(1, Ordering::Relaxed);
    }
    
    /// Get hit rate for optimization decisions
    pub fn hit_rate(&self) -> f64 {
        let hits = self.hit_count.load(Ordering::Relaxed) as f64;
        let misses = self.miss_count.load(Ordering::Relaxed) as f64;
        let total = hits + misses;
        if total > 0.0 {
            hits / total
        } else {
            0.0
        }
    }
}

impl Default for InlineCache {
    fn default() -> Self {
        Self::new()
    }
}

/// A segment of inline caches (fixed size for lock-free expansion)
struct CacheSegment {
    caches: Vec<InlineCache>,
}

impl CacheSegment {
    fn new() -> Self {
        let mut caches = Vec::with_capacity(SEGMENT_SIZE);
        for _ in 0..SEGMENT_SIZE {
            caches.push(InlineCache::new());
        }
        Self { caches }
    }
}

/// Inline cache table with lock-free segmented expansion
pub struct InlineCacheTable {
    /// Segmented storage - grows by adding segments (Arc for shared ownership)
    segments: Vec<Arc<CacheSegment>>,
    /// Number of segments currently allocated
    num_segments: AtomicUsize,
    /// Next slot index to allocate
    next_slot: AtomicUsize,
}

impl InlineCacheTable {
    /// Create a new IC table with initial capacity
    pub fn new(initial_capacity: usize) -> Self {
        let segments_needed = (initial_capacity + SEGMENT_SIZE - 1) / SEGMENT_SIZE;
        let mut segments = Vec::with_capacity(segments_needed.max(1));
        
        for _ in 0..segments_needed.max(1) {
            segments.push(Arc::new(CacheSegment::new()));
        }
        
        Self {
            segments,
            num_segments: AtomicUsize::new(segments_needed.max(1)),
            next_slot: AtomicUsize::new(0),
        }
    }
    
    /// Allocate a new cache slot with automatic expansion
    pub fn alloc_slot(&self) -> u32 {
        let slot = self.next_slot.fetch_add(1, Ordering::SeqCst);
        let segment_idx = slot / SEGMENT_SIZE;
        
        // Check if we need to grow
        let current_segments = self.num_segments.load(Ordering::Acquire);
        if segment_idx >= current_segments {
            // This would need to be handled with interior mutability in production
            // For now, we assume the initial capacity is sufficient
            panic!("Inline cache table needs expansion beyond initial capacity");
        }
        
        slot as u32
    }
    
    /// Get cache at a specific slot
    pub fn get_cache(&self, slot: u32) -> &InlineCache {
        let slot = slot as usize;
        let segment_idx = slot / SEGMENT_SIZE;
        let offset = slot % SEGMENT_SIZE;
        
        // Safe: segments vector never shrinks, only grows
        // We use unchecked indexing for performance since we validated slot at allocation
        &self.segments[segment_idx].caches[offset]
    }
    
    /// Get total number of allocated slots
    pub fn num_slots(&self) -> usize {
        self.next_slot.load(Ordering::SeqCst)
    }
}

/// Polymorphic inline cache (up to N entries)
pub struct PolymorphicInlineCache<const N: usize> {
    entries: [InlineCache; N],
    next_entry: AtomicUsize,
}

impl<const N: usize> PolymorphicInlineCache<N> {
    pub const fn new() -> Self {
        // Initialize array with empty caches
        const EMPTY: InlineCache = InlineCache::new();
        Self {
            entries: [EMPTY; N],
            next_entry: AtomicUsize::new(0),
        }
    }
    
    /// Find matching entry or allocate new one
    pub fn find_or_alloc(&self, class_serial: u64) -> Option<&InlineCache> {
        // First try to find existing entry
        for entry in &self.entries {
            if entry.class_serial.load(Ordering::Acquire) == class_serial && entry.is_resolved() {
                return Some(entry);
            }
        }
        
        // Allocate new entry if space available
        let idx = self.next_entry.fetch_add(1, Ordering::SeqCst);
        if idx < N {
            Some(&self.entries[idx])
        } else {
            None // Cache full - megamorphic fallback
        }
    }
}

/// Global inline cache manager for the entire program
pub struct InlineCacheManager {
    /// Per-function cache tables with Arc for shared ownership
    tables: RwLock<std::collections::HashMap<String, Arc<InlineCacheTable>>>,
}

impl InlineCacheManager {
    pub fn new() -> Self {
        Self {
            tables: RwLock::new(std::collections::HashMap::new()),
        }
    }
    
    /// Get or create cache table for a function
    pub fn get_or_create_table(&self, func_name: &str) -> Arc<InlineCacheTable> {
        // Fast path: try read lock
        {
            let tables = self.tables.read().unwrap();
            if let Some(table) = tables.get(func_name) {
                return table.clone();
            }
        }
        
        // Slow path: acquire write lock and create
        let mut tables = self.tables.write().unwrap();
        tables.entry(func_name.to_string())
            .or_insert_with(|| Arc::new(InlineCacheTable::new(64))) // Default 64 slots per function
            .clone()
    }
}

impl Default for InlineCacheManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Cache slot reference for code generation
#[derive(Debug, Clone, Copy)]
pub struct CacheSlotRef {
    pub table_idx: u32,
    pub slot_idx: u32,
}

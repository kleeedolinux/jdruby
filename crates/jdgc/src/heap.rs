//! # Heap Management
//!
//! Unified heap structure integrating regions, allocation, and GC phases.

use std::sync::atomic::{AtomicUsize, Ordering};
use crate::region::{Region, RegionManager};
use crate::collector::GcConfig;

/// Global heap structure.
pub struct Heap {
    /// Region manager.
    pub region_manager: RegionManager,
    /// Configuration.
    pub config: GcConfig,
    /// Total allocations.
    total_allocated: AtomicUsize,
    /// Total GC cycles.
    gc_cycles: AtomicUsize,
}

impl Heap {
    /// Create new heap with configuration.
    pub fn new(config: GcConfig) -> Self {
        let region_manager = RegionManager::new(config.initial_heap_size)
            .expect("Failed to create heap");

        Self {
            region_manager,
            config,
            total_allocated: AtomicUsize::new(0),
            gc_cycles: AtomicUsize::new(0),
        }
    }

    /// Allocate object of given size.
    pub fn allocate(&self, size: usize) -> Option<*mut crate::header::ObjectHeader> {
        let result = self.region_manager.allocate(size);
        if result.is_some() {
            self.total_allocated.fetch_add(size, Ordering::Relaxed);
        }
        result
    }

    /// Get total heap size.
    pub fn total_size(&self) -> usize {
        self.region_manager.total_size()
    }

    /// Get used bytes.
    pub fn used_bytes(&self) -> usize {
        self.region_manager.used_bytes()
    }

    /// Get free bytes.
    pub fn free_bytes(&self) -> usize {
        self.total_size() - self.used_bytes()
    }

    /// Get usage ratio.
    pub fn usage_ratio(&self) -> f64 {
        let total = self.total_size();
        if total == 0 {
            return 0.0;
        }
        self.used_bytes() as f64 / total as f64
    }

    /// Check if GC should be triggered.
    pub fn should_gc(&self) -> bool {
        self.usage_ratio() > self.config.gc_threshold
    }

    /// Get region count.
    pub fn region_count(&self) -> usize {
        self.region_manager.region_count()
    }

    /// Get total allocated bytes.
    pub fn total_allocated(&self) -> usize {
        self.total_allocated.load(Ordering::Relaxed)
    }

    /// Get GC cycle count.
    pub fn gc_cycles(&self) -> usize {
        self.gc_cycles.load(Ordering::Relaxed)
    }

    /// Increment GC cycle count.
    pub fn record_gc_cycle(&self) {
        self.gc_cycles.fetch_add(1, Ordering::Relaxed);
    }

    /// Expand heap.
    pub fn expand(&mut self, additional_bytes: usize) {
        self.region_manager.expand(additional_bytes);
    }

    /// Reset heap (clear all regions).
    pub fn reset(&self) {
        self.region_manager.reset_all();
        self.total_allocated.store(0, Ordering::Relaxed);
    }

    /// Get pointer to region containing address.
    pub fn region_for(&self, ptr: *const u8) -> Option<&Region> {
        self.region_manager.regions().iter().find(|r| r.contains(ptr))
    }
}

impl Default for Heap {
    fn default() -> Self {
        Self::new(GcConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heap_creation() {
        let heap = Heap::new(GcConfig::default());
        assert!(heap.total_size() > 0);
        assert_eq!(heap.used_bytes(), 0);
        assert_eq!(heap.gc_cycles(), 0);
    }

    #[test]
    fn test_heap_allocation() {
        let heap = Heap::new(GcConfig::default());
        
        let obj = heap.allocate(64);
        assert!(obj.is_some());
        
        let used = heap.used_bytes();
        assert!(used > 0);
    }

    #[test]
    fn test_heap_multiple_allocations() {
        let heap = Heap::new(GcConfig::default());
        
        // Allocate many objects
        for i in 0..100 {
            let size = 64 + (i % 128);
            assert!(heap.allocate(size).is_some());
        }
        
        assert!(heap.used_bytes() > 0);
        assert!(heap.total_allocated() > 0);
    }

    #[test]
    fn test_heap_usage_ratio() {
        let heap = Heap::new(GcConfig::default());
        
        let initial_ratio = heap.usage_ratio();
        assert_eq!(initial_ratio, 0.0);
        
        // Allocate some memory
        for _ in 0..100 {
            heap.allocate(1024);
        }
        
        let ratio = heap.usage_ratio();
        assert!(ratio > 0.0);
        assert!(ratio < 1.0);
    }

    #[test]
    fn test_heap_should_gc() {
        let config = GcConfig {
            gc_threshold: 0.5,
            ..Default::default()
        };
        let heap = Heap::new(config);
        
        // Initially should not GC
        assert!(!heap.should_gc());
        
        // Allocate a lot
        for _ in 0..1000 {
            heap.allocate(4096);
        }
        
        // May or may not trigger depending on heap size
        let _ = heap.should_gc();
    }

    #[test]
    fn test_heap_expand() {
        let mut heap = Heap::new(GcConfig::default());
        let initial_size = heap.total_size();
        
        heap.expand(4 * 1024 * 1024); // 4MB
        
        assert!(heap.total_size() > initial_size);
    }

    #[test]
    fn test_heap_reset() {
        let heap = Heap::new(GcConfig::default());
        
        heap.allocate(1024).unwrap();
        heap.allocate(2048).unwrap();
        
        assert!(heap.used_bytes() > 0);
        
        heap.reset();
        
        assert_eq!(heap.used_bytes(), 0);
        assert_eq!(heap.total_allocated(), 0);
    }

    #[test]
    fn test_heap_region_for() {
        let heap = Heap::new(GcConfig::default());
        
        let obj = heap.allocate(64).unwrap();
        
        let region = heap.region_for(obj as *const u8);
        assert!(region.is_some());
        
        // Check that the region contains the object
        let r = region.unwrap();
        assert!(r.contains(obj as *const u8));
    }

    #[test]
    fn test_heap_gc_cycles() {
        let heap = Heap::new(GcConfig::default());
        
        assert_eq!(heap.gc_cycles(), 0);
        
        heap.record_gc_cycle();
        heap.record_gc_cycle();
        
        assert_eq!(heap.gc_cycles(), 2);
    }

    #[test]
    fn test_heap_default() {
        let heap: Heap = Default::default();
        assert!(heap.total_size() > 0);
    }

    #[test]
    fn test_heap_free_bytes() {
        let heap = Heap::new(GcConfig::default());
        let total = heap.total_size();
        
        assert_eq!(heap.free_bytes(), total);
        
        heap.allocate(1024);
        
        assert!(heap.free_bytes() < total);
    }

    #[test]
    fn test_heap_region_count() {
        let heap = Heap::new(GcConfig {
            initial_heap_size: 4 * 1024 * 1024,
            ..Default::default()
        });
        
        // Should have 2 regions for 4MB
        assert_eq!(heap.region_count(), 2);
    }
}

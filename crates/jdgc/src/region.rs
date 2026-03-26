//! # Region-Based Heap Management
//!
//! 2MB regions with bump-pointer allocation and evacuation support.

use std::alloc::{self, Layout};
use crate::allocator::AllocationError;
use std::sync::atomic::{AtomicUsize, Ordering};
use crate::util::*;
use crate::header::ObjectHeader;

/// A 2MB heap region with bump-pointer allocation.
pub struct Region {
    /// Unique region ID.
    pub id: usize,
    /// Base address of region memory.
    base: *mut u8,
    /// Current bump pointer (next free byte).
    bump: AtomicUsize,
    /// Number of live objects.
    live_count: AtomicUsize,
    /// Estimated garbage ratio (0.0 - 1.0), stored as thousandths.
    garbage_ratio: AtomicUsize,
    /// Region is evacuation target.
    is_evacuation_target: AtomicUsize,
}

impl Region {
    /// Create new empty region.
    pub fn new(id: usize) -> Self {
        let layout = Layout::from_size_align(REGION_SIZE, REGION_ALIGNMENT)
            .expect("region layout invalid");
        let base = unsafe { alloc::alloc(layout) };
        if base.is_null() {
            alloc::handle_alloc_error(layout);
        }

        Self {
            id,
            base,
            bump: AtomicUsize::new(0),
            live_count: AtomicUsize::new(0),
            garbage_ratio: AtomicUsize::new(0),
            is_evacuation_target: AtomicUsize::new(0),
        }
    }

    /// Allocate object in this region.
    /// Returns pointer to ObjectHeader, or None if region is full.
    pub fn allocate_object(&self, payload_size: usize) -> Option<*mut ObjectHeader> {
        let total_size = align_up(std::mem::size_of::<ObjectHeader>() + payload_size, OBJ_ALIGN);

        // Fast-path bump allocation
        let offset = self.bump.fetch_add(total_size, Ordering::Relaxed);

        if offset + total_size > REGION_SIZE {
            // Rollback and fail
            self.bump.fetch_sub(total_size, Ordering::Relaxed);
            return None;
        }

        let obj_addr = unsafe { self.base.add(offset) };
        let header = obj_addr as *mut ObjectHeader;

        // Initialize header
        ObjectHeader::init_at(header, payload_size);

        self.live_count.fetch_add(1, Ordering::Relaxed);
        Some(header)
    }

    /// Check if region has enough space.
    pub fn can_allocate(&self, size: usize) -> bool {
        let current = self.bump.load(Ordering::Relaxed);
        current + size <= REGION_SIZE
    }

    /// Get remaining capacity.
    pub fn remaining(&self) -> usize {
        REGION_SIZE - self.bump.load(Ordering::Relaxed)
    }

    /// Get used bytes.
    pub fn used_bytes(&self) -> usize {
        self.bump.load(Ordering::Relaxed)
    }

    /// Get number of live objects.
    pub fn live_count(&self) -> usize {
        self.live_count.load(Ordering::Relaxed)
    }

    /// Check if region is empty.
    pub fn is_empty(&self) -> bool {
        self.live_count() == 0
    }

    /// Check if region is evacuation target.
    pub fn is_evacuation_target(&self) -> bool {
        self.is_evacuation_target.load(Ordering::Relaxed) != 0
    }

    /// Set as evacuation target.
    pub fn set_evacuation_target(&self, value: bool) {
        self.is_evacuation_target.store(if value { 1 } else { 0 }, Ordering::Relaxed);
    }

    /// Update garbage ratio (called after marking).
    pub fn update_garbage_ratio(&self, live_objects: usize) {
        let total = self.live_count.load(Ordering::Relaxed);
        if total > 0 {
            let ratio = (total - live_objects) as f64 / total as f64;
            let ratio_scaled = (ratio * 1000.0) as usize;
            self.garbage_ratio.store(ratio_scaled, Ordering::Relaxed);
        }
    }

    /// Get garbage ratio (0.0 - 1.0).
    pub fn garbage_ratio(&self) -> f64 {
        self.garbage_ratio.load(Ordering::Relaxed) as f64 / 1000.0
    }

    /// Reset region for reuse.
    pub fn reset(&self) {
        self.bump.store(0, Ordering::Relaxed);
        self.live_count.store(0, Ordering::Relaxed);
        self.garbage_ratio.store(0, Ordering::Relaxed);
        self.is_evacuation_target.store(0, Ordering::Relaxed);
    }

    /// Iterate over all objects in region.
    pub fn iter_objects<F>(&self, mut f: F)
    where
        F: FnMut(&ObjectHeader),
    {
        let mut offset = 0;
        let current_bump = self.bump.load(Ordering::Relaxed);

        while offset < current_bump {
            let header = unsafe { &*(self.base.add(offset) as *const ObjectHeader) };
            f(header);
            offset += align_up(header.total_size(), OBJ_ALIGN);
        }
    }

    /// Iterate over all objects mutably.
    pub fn iter_objects_mut<F>(&self, mut f: F)
    where
        F: FnMut(&mut ObjectHeader),
    {
        let mut offset = 0;
        let current_bump = self.bump.load(Ordering::Relaxed);

        while offset < current_bump {
            let header = unsafe { &mut *(self.base.add(offset) as *mut ObjectHeader) };
            f(header);
            offset += align_up(header.total_size(), OBJ_ALIGN);
        }
    }

    /// Get base address.
    pub fn base(&self) -> *mut u8 {
        self.base
    }

    /// Get end address.
    pub fn end(&self) -> *mut u8 {
        unsafe { self.base.add(REGION_SIZE) }
    }

    /// Check if pointer belongs to this region.
    pub fn contains(&self, ptr: *const u8) -> bool {
        let addr = ptr as usize;
        let base = self.base as usize;
        addr >= base && addr < base + REGION_SIZE
    }
}

impl Drop for Region {
    fn drop(&mut self) {
        let layout = Layout::from_size_align(REGION_SIZE, REGION_ALIGNMENT)
            .expect("region layout invalid");
        unsafe {
            alloc::dealloc(self.base, layout);
        }
    }
}

unsafe impl Send for Region {}
unsafe impl Sync for Region {}

/// Manager for heap regions.
pub struct RegionManager {
    /// All regions.
    regions: Vec<Region>,
    /// Current region index for allocation.
    current_region: AtomicUsize,
    /// Total heap size.
    total_size: AtomicUsize,
}

impl RegionManager {
    /// Create region manager with initial heap size.
    pub fn new(initial_size: usize) -> Result<Self, AllocationError> {
        let num_regions = (initial_size + REGION_SIZE - 1) / REGION_SIZE;
        let mut regions = Vec::with_capacity(num_regions);

        for i in 0..num_regions {
            regions.push(Region::new(i));
        }

        Ok(Self {
            regions,
            current_region: AtomicUsize::new(0),
            total_size: AtomicUsize::new(num_regions * REGION_SIZE),
        })
    }

    /// Allocate object in any available region.
    pub fn allocate(&self, size: usize) -> Option<*mut ObjectHeader> {
        // Try current region first
        let current = self.current_region.load(Ordering::Relaxed);
        if let Some(obj) = self.regions[current].allocate_object(size) {
            return Some(obj);
        }

        // Find another region with space
        for (i, region) in self.regions.iter().enumerate() {
            if region.can_allocate(size) {
                self.current_region.store(i, Ordering::Relaxed);
                if let Some(obj) = region.allocate_object(size) {
                    return Some(obj);
                }
            }
        }

        None
    }

    /// Get total heap size.
    pub fn total_size(&self) -> usize {
        self.total_size.load(Ordering::Relaxed)
    }

    /// Get used bytes.
    pub fn used_bytes(&self) -> usize {
        self.regions.iter()
            .map(|r| r.bump.load(Ordering::Relaxed))
            .sum()
    }

    /// Get region count.
    pub fn region_count(&self) -> usize {
        self.regions.len()
    }

    /// Get number of regions.
    pub fn len(&self) -> usize {
        self.regions.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.regions.is_empty()
    }

    /// Add new regions to expand heap.
    pub fn expand(&mut self, additional_size: usize) {
        let num_regions = (additional_size + REGION_SIZE - 1) / REGION_SIZE;
        let start_id = self.regions.len();

        for i in 0..num_regions {
            self.regions.push(Region::new(start_id + i));
        }

        self.total_size.fetch_add(num_regions * REGION_SIZE, Ordering::Relaxed);
    }

    /// Find regions for evacuation (high garbage ratio).
    pub fn find_evacuation_targets(&self) -> Vec<&Region> {
        self.regions.iter()
            .filter(|r| r.garbage_ratio() > EVACUATION_THRESHOLD)
            .collect()
    }

    /// Sweep dead objects across all regions.
    pub fn sweep_unmarked<F>(&self, mut on_free: F)
    where
        F: FnMut(&ObjectHeader),
    {
        for region in &self.regions {
            region.iter_objects(|header| {
                if header.is_white() {
                    on_free(header);
                } else {
                    header.reset_white();
                }
            });
        }
    }

    /// Get all regions.
    pub fn regions(&self) -> &[Region] {
        &self.regions
    }

    /// Get region by ID.
    pub fn get(&self, id: usize) -> Option<&Region> {
        self.regions.get(id)
    }

    /// Get mutable region by ID.
    pub fn get_mut(&mut self, id: usize) -> Option<&mut Region> {
        self.regions.get_mut(id)
    }

    /// Reset all regions.
    pub fn reset_all(&self) {
        for region in &self.regions {
            region.reset();
        }
        self.current_region.store(0, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_region_creation() {
        let region = Region::new(0);
        assert_eq!(region.id, 0);
        assert!(region.is_empty());
        assert_eq!(region.remaining(), REGION_SIZE);
        assert!(!region.is_evacuation_target());
    }

    #[test]
    fn test_region_allocation() {
        let region = Region::new(1);
        let header_size = std::mem::size_of::<ObjectHeader>();

        let obj1 = region.allocate_object(64).unwrap();
        let obj2 = region.allocate_object(128).unwrap();

        // Objects should be at different addresses
        assert_ne!(obj1, obj2);

        // obj2 should be after obj1 + total_size
        let expected_offset = align_up(header_size + 64, OBJ_ALIGN);
        let delta = (obj2 as usize) - (obj1 as usize);
        assert_eq!(delta, expected_offset);

        assert_eq!(region.live_count(), 2);
    }

    #[test]
    fn test_region_oom() {
        let region = Region::new(2);
        // Try to allocate more than region size
        let huge = region.allocate_object(REGION_SIZE + 1);
        assert!(huge.is_none());
    }

    #[test]
    fn test_region_reset() {
        let region = Region::new(3);
        region.allocate_object(64).unwrap();
        region.allocate_object(128).unwrap();
        
        assert_eq!(region.live_count(), 2);
        
        region.reset();
        
        assert_eq!(region.live_count(), 0);
        assert!(region.is_empty());
    }

    #[test]
    fn test_region_contains() {
        let region = Region::new(4);
        let obj = region.allocate_object(64).unwrap();
        
        assert!(region.contains(obj as *const u8));
        
        // Outside region
        let outside = unsafe { (region.base() as *const u8).sub(1) };
        assert!(!region.contains(outside));
    }

    #[test]
    fn test_region_evacuation() {
        let region = Region::new(5);
        assert!(!region.is_evacuation_target());
        
        region.set_evacuation_target(true);
        assert!(region.is_evacuation_target());
        
        region.set_evacuation_target(false);
        assert!(!region.is_evacuation_target());
    }

    #[test]
    fn test_region_garbage_ratio() {
        let region = Region::new(6);
        region.allocate_object(64).unwrap();
        region.allocate_object(64).unwrap();
        
        // Simulate: 1 live, 1 dead
        region.update_garbage_ratio(1);
        
        let ratio = region.garbage_ratio();
        assert!(ratio > 0.49 && ratio < 0.51, "Expected ~0.5, got {}", ratio);
    }

    #[test]
    fn test_region_iteration() {
        let region = Region::new(7);
        let _obj1 = region.allocate_object(64).unwrap();
        let _obj2 = region.allocate_object(128).unwrap();
        
        let mut count = 0;
        region.iter_objects(|header| {
            count += 1;
            assert!(header.payload_size == 64 || header.payload_size == 128);
        });
        
        assert_eq!(count, 2);
    }

    #[test]
    fn test_region_manager_creation() {
        let manager = RegionManager::new(4 * 1024 * 1024).unwrap();
        assert_eq!(manager.region_count(), 2);
        assert_eq!(manager.total_size(), 2 * REGION_SIZE);
    }

    #[test]
    fn test_region_manager_allocate() {
        let manager = RegionManager::new(2 * 1024 * 1024).unwrap();
        
        let obj = manager.allocate(64);
        assert!(obj.is_some());
        
        // Allocate many objects
        for _ in 0..100 {
            assert!(manager.allocate(64).is_some());
        }
    }

    #[test]
    fn test_region_manager_expand() {
        let mut manager = RegionManager::new(2 * 1024 * 1024).unwrap();
        assert_eq!(manager.region_count(), 1);
        
        manager.expand(4 * 1024 * 1024);
        assert_eq!(manager.region_count(), 3);
    }

    #[test]
    fn test_region_manager_find_evacuation_targets() {
        let manager = RegionManager::new(4 * 1024 * 1024).unwrap();
        
        // Set up regions with high garbage
        for region in manager.regions() {
            region.allocate_object(64).unwrap();
            region.allocate_object(64).unwrap();
            region.update_garbage_ratio(0); // All dead
        }
        
        let targets = manager.find_evacuation_targets();
        assert!(!targets.is_empty());
    }

    #[test]
    fn test_region_manager_used_bytes() {
        let manager = RegionManager::new(2 * 1024 * 1024).unwrap();
        let initial = manager.used_bytes();
        
        manager.allocate(64).unwrap();
        let after = manager.used_bytes();
        
        assert!(after > initial);
    }

    #[test]
    fn test_region_manager_reset() {
        let manager = RegionManager::new(2 * 1024 * 1024).unwrap();
        
        manager.allocate(64).unwrap();
        manager.allocate(128).unwrap();
        
        assert!(manager.used_bytes() > 0);
        
        manager.reset_all();
        
        assert_eq!(manager.used_bytes(), 0);
    }
}

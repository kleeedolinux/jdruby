//! Inline Cache Implementation
//!
//! Provides polymorphic inline caching for method dispatch optimization.
//! Supports monomorphic, polymorphic (up to 4 types), and megamorphic fallback.

use crate::types::*;
use crate::resolver::CallType;
use std::collections::HashMap;

/// Cache entry for a resolved method call
#[derive(Debug, Clone)]
pub struct CacheEntry {
    /// Pointer to method entry
    pub method_entry: *const MethodEntry,
    /// Receiver class ID
    pub receiver_class: ClassId,
    /// Type of call
    pub call_type: CallType,
    /// Hit count for statistics
    pub hit_count: u64,
}

/// Information about a call site
#[derive(Debug, Clone)]
pub struct CallSiteInfo {
    /// Call site ID
    pub id: usize,
    /// Number of different receiver classes seen
    pub polymorphic_count: usize,
    /// Is this site megamorphic (>4 types)?
    pub is_megamorphic: bool,
}

/// Polymorphic Inline Cache
/// 
/// Each call site can cache up to 4 different receiver types.
/// Beyond 4, it becomes megamorphic and uses a global hash map.
pub struct InlineCache {
    /// Monomorphic/polymorphic cache: call_site_id -> [(class_id, entry)]
    cache: HashMap<usize, Vec<(ClassId, CacheEntry)>>,
    /// Statistics
    hits: usize,
    misses: usize,
}

/// Maximum number of cached types per call site (polymorphic limit)
pub const POLYMORPHIC_LIMIT: usize = 4;

impl InlineCache {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
            hits: 0,
            misses: 0,
        }
    }

    /// Lookup a method in the cache
    pub fn lookup(&mut self, call_site_id: usize, receiver_class: ClassId) -> Option<CacheEntry> {
        if let Some(entries) = self.cache.get(&call_site_id) {
            for (class, entry) in entries {
                if *class == receiver_class {
                    self.hits += 1;
                    return Some(entry.clone());
                }
            }
        }
        self.misses += 1;
        None
    }

    /// Insert or update a cache entry
    /// 
    /// Returns the call site info after insertion
    pub fn insert(
        &mut self,
        call_site_id: usize,
        receiver_class: ClassId,
        entry: CacheEntry,
    ) -> CallSiteInfo {
        let entries = self.cache.entry(call_site_id).or_default();
        
        // Check if this class is already cached
        for (class, e) in entries.iter_mut() {
            if *class == receiver_class {
                // Update existing entry
                e.hit_count += 1;
                return CallSiteInfo {
                    id: call_site_id,
                    polymorphic_count: entries.len(),
                    is_megamorphic: entries.len() > POLYMORPHIC_LIMIT,
                };
            }
        }
        
        // Add new entry
        entries.push((receiver_class, entry));
        
        // Check if we've exceeded the polymorphic limit
        let is_megamorphic = entries.len() > POLYMORPHIC_LIMIT;
        
        CallSiteInfo {
            id: call_site_id,
            polymorphic_count: entries.len(),
            is_megamorphic,
        }
    }

    /// Invalidate all cache entries for a specific class
    /// Called when methods are redefined
    pub fn invalidate_class(&mut self, class_id: ClassId) {
        for entries in self.cache.values_mut() {
            entries.retain(|(class, _)| *class != class_id);
        }
    }

    /// Invalidate all cache entries for a specific call site
    pub fn invalidate_site(&mut self, call_site_id: usize) {
        self.cache.remove(&call_site_id);
    }

    /// Get information about a call site
    pub fn site_info(&self, call_site_id: usize) -> Option<CallSiteInfo> {
        self.cache.get(&call_site_id).map(|entries| CallSiteInfo {
            id: call_site_id,
            polymorphic_count: entries.len(),
            is_megamorphic: entries.len() > POLYMORPHIC_LIMIT,
        })
    }

    /// Check if a call site is megamorphic
    pub fn is_megamorphic(&self, call_site_id: usize) -> bool {
        self.cache
            .get(&call_site_id)
            .map(|e| e.len() > POLYMORPHIC_LIMIT)
            .unwrap_or(false)
    }

    /// Get all cached types for a call site
    pub fn cached_types(&self, call_site_id: usize) -> Option<Vec<ClassId>> {
        self.cache
            .get(&call_site_id)
            .map(|entries| entries.iter().map(|(c, _)| *c).collect())
    }

    /// Get cache hit count
    pub fn hits(&self) -> usize {
        self.hits
    }

    /// Get cache miss count
    pub fn misses(&self) -> usize {
        self.misses
    }

    /// Get hit rate as percentage
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            (self.hits as f64 / total as f64) * 100.0
        }
    }

    /// Get total number of cached call sites
    pub fn num_sites(&self) -> usize {
        self.cache.len()
    }

    /// Get total number of cached entries
    pub fn num_entries(&self) -> usize {
        self.cache.values().map(|v| v.len()).sum()
    }

    /// Clear the cache
    pub fn clear(&mut self) {
        self.cache.clear();
        self.hits = 0;
        self.misses = 0;
    }

    /// Iterate over all cache entries for debugging
    pub fn iter(&self) -> impl Iterator<Item = (usize, &Vec<(ClassId, CacheEntry)>)> {
        self.cache.iter().map(|(k, v)| (*k, v))
    }
}

impl Default for InlineCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Global megamorphic cache for call sites with >4 receiver types
pub struct MegamorphicCache {
    /// call_site_id -> (receiver_class -> CacheEntry)
    cache: HashMap<usize, HashMap<ClassId, CacheEntry>>,
}

impl MegamorphicCache {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }

    /// Lookup in megamorphic cache
    pub fn lookup(&self, call_site_id: usize, receiver_class: ClassId) -> Option<CacheEntry> {
        self.cache
            .get(&call_site_id)
            .and_then(|entries| entries.get(&receiver_class))
            .cloned()
    }

    /// Insert into megamorphic cache
    pub fn insert(
        &mut self,
        call_site_id: usize,
        receiver_class: ClassId,
        entry: CacheEntry,
    ) {
        self.cache
            .entry(call_site_id)
            .or_default()
            .insert(receiver_class, entry);
    }

    /// Invalidate entries for a class
    pub fn invalidate_class(&mut self, class_id: ClassId) {
        for entries in self.cache.values_mut() {
            entries.remove(&class_id);
        }
    }

    /// Clear a call site
    pub fn invalidate_site(&mut self, call_site_id: usize) {
        self.cache.remove(&call_site_id);
    }
}

impl Default for MegamorphicCache {
    fn default() -> Self {
        Self::new()
    }
}

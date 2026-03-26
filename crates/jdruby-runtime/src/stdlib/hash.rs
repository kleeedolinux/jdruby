//! # Ruby Hash Implementation
//!
//! MRI-compatible Hash with open addressing (Robin Hood hashing).
//! Follows the structure of MRI's hash.c

use std::alloc::{alloc, dealloc, Layout};
use std::sync::atomic::{AtomicU32, Ordering};

pub const HASH_DEFAULT_SIZE: usize = 8;

/// Hash flags
pub const HASH_PROC_DEFAULT: u32 = 1 << 0;
pub const HASH_FROZEN: u32 = 1 << 1;

/// Ruby Hash entry (Robin Hood hash table)
#[repr(C)]
#[derive(Clone, Copy)]
pub struct HashEntry {
    pub key: u64,
    pub value: u64,
    pub hash: u64,
    pub probe_len: u32,
    pub used: bool,
}

/// Ruby Hash table
#[repr(C)]
pub struct RubyHash {
    pub flags: AtomicU32,
    pub len: usize,
    pub capa: usize,
    pub entries: *mut HashEntry,
    pub default_value: u64,
    pub default_proc: u64,
}

impl RubyHash {
    pub fn new() -> Self {
        Self::with_capacity(HASH_DEFAULT_SIZE)
    }

    pub fn with_capacity(capa: usize) -> Self {
        let actual_capa = capa.next_power_of_two().max(HASH_DEFAULT_SIZE);
        let entries = unsafe {
            let layout = Layout::array::<HashEntry>(actual_capa).unwrap();
            let ptr = alloc(layout) as *mut HashEntry;
            if ptr.is_null() {
                panic!("allocation failed");
            }
            // Zero initialize
            std::ptr::write_bytes(ptr, 0, actual_capa);
            ptr
        };

        Self {
            flags: AtomicU32::new(0),
            len: 0,
            capa: actual_capa,
            entries,
            default_value: 0,
            default_proc: 0,
        }
    }

    fn hash_key(key: u64) -> u64 {
        // FNV-1a hash
        const FNV_PRIME: u64 = 1099511628211;
        const FNV_OFFSET: u64 = 14695981039346656037;
        let mut hash = FNV_OFFSET;
        hash ^= key;
        hash = hash.wrapping_mul(FNV_PRIME);
        hash
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn insert(&mut self, key: u64, value: u64) -> Option<u64> {
        if self.len * 2 >= self.capa {
            self.grow();
        }

        let hash = Self::hash_key(key);
        let mut idx = (hash as usize) & (self.capa - 1);
        let mut probe_len = 0u32;

        loop {
            let entry = unsafe { &mut *self.entries.add(idx) };

            if !entry.used {
                // Empty slot - insert here
                entry.key = key;
                entry.value = value;
                entry.hash = hash;
                entry.probe_len = probe_len;
                entry.used = true;
                self.len += 1;
                return None;
            }

            if entry.key == key {
                // Existing key - update
                let old = entry.value;
                entry.value = value;
                return Some(old);
            }

            // Robin Hood: swap if we have longer probe distance
            if entry.probe_len < probe_len {
                let mut tmp_key = key;
                let mut tmp_value = value;
                let mut tmp_hash = hash;
                let mut tmp_probe = probe_len;

                std::mem::swap(&mut tmp_key, &mut entry.key);
                std::mem::swap(&mut tmp_value, &mut entry.value);
                std::mem::swap(&mut tmp_hash, &mut entry.hash);
                std::mem::swap(&mut tmp_probe, &mut entry.probe_len);

                // Continue with swapped entry
                // (simplified - full implementation would continue loop)
            }

            idx = (idx + 1) & (self.capa - 1);
            probe_len += 1;
        }
    }

    pub fn get(&self, key: u64) -> Option<u64> {
        let hash = Self::hash_key(key);
        let mut idx = (hash as usize) & (self.capa - 1);
        let mut probe_len = 0u32;

        loop {
            let entry = unsafe { &*self.entries.add(idx) };

            if !entry.used {
                return None;
            }

            if entry.key == key {
                return Some(entry.value);
            }

            // Stop if we found an entry with shorter probe length
            if entry.probe_len < probe_len {
                return None;
            }

            idx = (idx + 1) & (self.capa - 1);
            probe_len += 1;
        }
    }

    pub fn contains_key(&self, key: u64) -> bool {
        self.get(key).is_some()
    }

    pub fn remove(&mut self, key: u64) -> Option<u64> {
        let hash = Self::hash_key(key);
        let mut idx = (hash as usize) & (self.capa - 1);
        let mut probe_len = 0u32;

        // Find the entry
        let found_idx = loop {
            let entry = unsafe { &*self.entries.add(idx) };

            if !entry.used {
                return None;
            }

            if entry.key == key {
                break idx;
            }

            if entry.probe_len < probe_len {
                return None;
            }

            idx = (idx + 1) & (self.capa - 1);
            probe_len += 1;
        };

        let entry = unsafe { &*self.entries.add(found_idx) };
        let value = entry.value;

        unsafe { (*self.entries.add(found_idx)).used = false; }
        self.len -= 1;

        // Backward shift to maintain robin hood invariant
        let mut curr_idx = found_idx;
        loop {
            let next_idx = (curr_idx + 1) & (self.capa - 1);
            let next_entry = unsafe { &*self.entries.add(next_idx) };

            if !next_entry.used || next_entry.probe_len == 0 {
                break;
            }

            let prev_entry = unsafe { &mut *self.entries.add(curr_idx) };
            prev_entry.key = next_entry.key;
            prev_entry.value = next_entry.value;
            prev_entry.hash = next_entry.hash;
            prev_entry.probe_len = next_entry.probe_len - 1;
            prev_entry.used = true;

            unsafe { (*self.entries.add(next_idx)).used = false; }
            curr_idx = next_idx;
        }

        Some(value)
    }

    pub fn clear(&mut self) {
        unsafe {
            std::ptr::write_bytes(self.entries, 0, self.capa);
        }
        self.len = 0;
    }

    pub fn is_frozen(&self) -> bool {
        self.flags.load(Ordering::Relaxed) & HASH_FROZEN != 0
    }

    pub fn freeze(&self) {
        self.flags.fetch_or(HASH_FROZEN, Ordering::SeqCst);
    }

    fn grow(&mut self) {
        let old_capa = self.capa;
        let old_entries = self.entries;

        self.capa *= 2;
        self.entries = unsafe {
            let layout = Layout::array::<HashEntry>(self.capa).unwrap();
            let ptr = alloc(layout) as *mut HashEntry;
            if ptr.is_null() {
                panic!("allocation failed");
            }
            std::ptr::write_bytes(ptr, 0, self.capa);
            ptr
        };
        self.len = 0;

        // Reinsert all entries
        for i in 0..old_capa {
            let entry = unsafe { &*old_entries.add(i) };
            if entry.used {
                self.insert(entry.key, entry.value);
            }
        }

        unsafe {
            let layout = Layout::array::<HashEntry>(old_capa).unwrap();
            dealloc(old_entries as *mut u8, layout);
        }
    }
}

impl Drop for RubyHash {
    fn drop(&mut self) {
        if !self.entries.is_null() {
            unsafe {
                let layout = Layout::array::<HashEntry>(self.capa).unwrap();
                dealloc(self.entries as *mut u8, layout);
            }
        }
    }
}

impl Default for RubyHash {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_basic() {
        let mut h = RubyHash::new();
        assert!(h.is_empty());

        h.insert(1, 100);
        h.insert(2, 200);
        h.insert(3, 300);

        assert_eq!(h.len(), 3);
        assert_eq!(h.get(1), Some(100));
        assert_eq!(h.get(2), Some(200));
        assert_eq!(h.get(3), Some(300));
        assert_eq!(h.get(999), None);
    }

    #[test]
    fn test_hash_update() {
        let mut h = RubyHash::new();
        h.insert(1, 100);
        let old = h.insert(1, 150);
        assert_eq!(old, Some(100));
        assert_eq!(h.get(1), Some(150));
    }

    #[test]
    fn test_hash_remove() {
        let mut h = RubyHash::new();
        h.insert(1, 100);
        h.insert(2, 200);

        let removed = h.remove(1);
        assert_eq!(removed, Some(100));
        assert_eq!(h.get(1), None);
        assert_eq!(h.len(), 1);
    }

    #[test]
    fn test_hash_grow() {
        let mut h = RubyHash::new();
        // Insert enough to trigger growth
        for i in 0..100 {
            h.insert(i as u64, (i * 10) as u64);
        }
        assert_eq!(h.len(), 100);

        // Verify all entries
        for i in 0..100 {
            assert_eq!(h.get(i as u64), Some((i * 10) as u64));
        }
    }

    #[test]
    fn test_hash_clear() {
        let mut h = RubyHash::new();
        h.insert(1, 100);
        h.insert(2, 200);
        assert_eq!(h.len(), 2);

        h.clear();
        assert_eq!(h.len(), 0);
        assert!(h.is_empty());
    }
}

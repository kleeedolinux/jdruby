//! # Tri-Color Concurrent Marking

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use crate::header::ObjectHeader;
use crate::roots::RootSet;

/// Work queue for marking.
pub struct MarkQueue {
    /// Internal queue.
    queue: Mutex<VecDeque<*mut ObjectHeader>>,
    /// Length counter.
    len: AtomicUsize,
}

impl MarkQueue {
    /// Create new empty queue.
    pub fn new() -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            len: AtomicUsize::new(0),
        }
    }

    /// Push object to queue.
    pub fn push(&self, obj: *mut ObjectHeader) {
        self.queue.lock().unwrap().push_back(obj);
        self.len.fetch_add(1, Ordering::Relaxed);
    }

    /// Pop object from queue.
    pub fn pop(&self) -> Option<*mut ObjectHeader> {
        let obj = self.queue.lock().unwrap().pop_front();
        if obj.is_some() {
            self.len.fetch_sub(1, Ordering::Relaxed);
        }
        obj
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.len.load(Ordering::Relaxed) == 0
    }

    /// Get length.
    pub fn len(&self) -> usize {
        self.len.load(Ordering::Relaxed)
    }

    /// Drain batch of objects.
    pub fn drain_batch(&self, buf: &mut Vec<*mut ObjectHeader>, max: usize) -> usize {
        let mut queue = self.queue.lock().unwrap();
        let count = max.min(queue.len());
        buf.clear();
        buf.reserve(count);
        for _ in 0..count {
            if let Some(obj) = queue.pop_front() {
                buf.push(obj);
            }
        }
        self.len.fetch_sub(count, Ordering::Relaxed);
        count
    }
}

impl Default for MarkQueue {
    fn default() -> Self {
        Self::new()
    }
}

/// Marker statistics.
#[derive(Debug, Default)]
pub struct MarkerStats {
    /// Objects marked.
    pub marked: AtomicUsize,
    /// Objects encountered.
    pub encountered: AtomicUsize,
}

/// Concurrent marker.
pub struct Marker {
    /// Mark queue.
    queue: MarkQueue,
    /// Statistics.
    stats: MarkerStats,
}

impl Marker {
    /// Create new marker.
    pub fn new() -> Self {
        Self {
            queue: MarkQueue::new(),
            stats: MarkerStats::default(),
        }
    }

    /// Seed mark queue from roots.
    pub fn seed_from_roots(&self, roots: &RootSet) {
        roots.iter_roots(|ptr| {
            let header = unsafe { &*ptr };
            if header.try_shade_gray() {
                self.queue.push(ptr);
                self.stats.encountered.fetch_add(1, Ordering::Relaxed);
            }
        });
    }

    /// Mark object (shade Gray → Black).
    pub fn mark_object(&self, obj: *mut ObjectHeader) {
        let header = unsafe { &*obj };

        if !header.is_gray() {
            return;
        }

        // Shade to black
        header.shade_black();
        self.stats.marked.fetch_add(1, Ordering::Relaxed);

        // Trace references - for now simplified
        // In full impl: call trace() on object to find children
    }

    /// Process mark queue.
    pub fn process_queue(&self) {
        while let Some(obj) = self.queue.pop() {
            self.mark_object(obj);
        }
    }

    /// Concurrent marking (spawn worker threads).
    pub fn mark_concurrent(&self) {
        // Spawn marking threads that steal from queue
        // For now, single-threaded
        self.process_queue();
    }

    /// Get marked count.
    pub fn marked_count(&self) -> usize {
        self.stats.marked.load(Ordering::Relaxed)
    }

    /// Reset for next cycle.
    pub fn reset(&self) {
        self.stats.marked.store(0, Ordering::Relaxed);
        self.stats.encountered.store(0, Ordering::Relaxed);
    }
}

impl Default for Marker {
    fn default() -> Self {
        Self::new()
    }
}

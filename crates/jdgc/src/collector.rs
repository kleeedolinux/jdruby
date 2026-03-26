//! # GC Controller and Phase Management

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

/// GC phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GcPhase {
    /// Idle - no GC running.
    Idle,
    /// Marking phase.
    Marking,
    /// Evacuation phase.
    Evacuating,
    /// Sweeping phase.
    Sweeping,
}

/// GC configuration.
#[derive(Debug, Clone, Copy)]
pub struct GcConfig {
    /// Initial heap size in bytes.
    pub initial_heap_size: usize,
    /// Minimum heap size.
    pub min_heap_size: usize,
    /// Maximum heap size.
    pub max_heap_size: usize,
    /// Trigger GC when heap usage exceeds this ratio.
    pub gc_threshold: f64,
    /// Heap growth factor.
    pub growth_factor: f64,
    /// Number of GC worker threads.
    pub worker_threads: usize,
    /// Enable concurrent marking.
    pub concurrent_marking: bool,
    /// Enable concurrent evacuation.
    pub concurrent_evacuation: bool,
}

impl Default for GcConfig {
    fn default() -> Self {
        Self {
            initial_heap_size: 64 * 1024 * 1024, // 64MB
            min_heap_size: 8 * 1024 * 1024,        // 8MB
            max_heap_size: 1024 * 1024 * 1024,     // 1GB
            gc_threshold: 0.75,
            growth_factor: 1.5,
            worker_threads: 2,
            concurrent_marking: true,
            concurrent_evacuation: true,
        }
    }
}

/// Default GC configuration.
pub const DEFAULT_GC_CONFIG: GcConfig = GcConfig {
    initial_heap_size: 64 * 1024 * 1024,
    min_heap_size: 8 * 1024 * 1024,
    max_heap_size: 1024 * 1024 * 1024,
    gc_threshold: 0.75,
    growth_factor: 1.5,
    worker_threads: 2,
    concurrent_marking: true,
    concurrent_evacuation: true,
};

/// Collector statistics.
#[derive(Debug, Default)]
pub struct CollectorStats {
    /// Number of GC cycles.
    pub gc_cycles: AtomicUsize,
    /// Total GC time (nanoseconds).
    pub gc_time_ns: AtomicU64,
    /// Bytes allocated.
    pub bytes_allocated: AtomicU64,
    /// Bytes freed.
    pub bytes_freed: AtomicU64,
}

/// GC Controller - manages GC lifecycle.
pub struct Collector {
    /// Current phase.
    phase: std::sync::RwLock<GcPhase>,
    /// Configuration.
    pub config: GcConfig,
    /// Statistics.
    pub stats: CollectorStats,
    /// Requested GC pause (for soft real-time).
    target_pause_ms: AtomicU64,
}

impl Collector {
    /// Create new collector.
    pub fn new(config: GcConfig) -> Self {
        Self {
            phase: std::sync::RwLock::new(GcPhase::Idle),
            config,
            stats: CollectorStats::default(),
            target_pause_ms: AtomicU64::new(10), // 10ms default
        }
    }

    /// Get current phase.
    pub fn phase(&self) -> GcPhase {
        *self.phase.read().unwrap()
    }

    /// Set phase.
    pub fn set_phase(&self, phase: GcPhase) {
        *self.phase.write().unwrap() = phase;
    }

    /// Check if GC should be triggered.
    pub fn should_collect(&self, used: usize, capacity: usize) -> bool {
        if used == 0 || capacity == 0 {
            return false;
        }
        let ratio = used as f64 / capacity as f64;
        ratio > self.config.gc_threshold
    }

    /// Calculate new heap size after GC.
    pub fn calculate_new_size(&self, live_bytes: usize) -> usize {
        let new_size = (live_bytes as f64 * self.config.growth_factor) as usize;
        new_size.clamp(self.config.min_heap_size, self.config.max_heap_size)
    }

    /// Update target pause time.
    pub fn set_target_pause(&self, ms: u64) {
        self.target_pause_ms.store(ms, Ordering::Relaxed);
    }

    /// Get target pause time.
    pub fn target_pause_ms(&self) -> u64 {
        self.target_pause_ms.load(Ordering::Relaxed)
    }

    /// Record GC cycle completion.
    pub fn record_cycle(&self, duration_ns: u64, bytes_freed: usize) {
        self.stats.gc_cycles.fetch_add(1, Ordering::Relaxed);
        self.stats.gc_time_ns.fetch_add(duration_ns, Ordering::Relaxed);
        self.stats.bytes_freed.fetch_add(bytes_freed as u64, Ordering::Relaxed);
    }
}

impl Default for Collector {
    fn default() -> Self {
        Self::new(DEFAULT_GC_CONFIG)
    }
}

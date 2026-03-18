//! # Profiling & Invocation Counting
//!
//! Tracks method invocation counts to drive tiered JIT compilation
//! decisions. Uses atomic counters for thread safety.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

/// Per-method profiling data.
pub struct MethodProfile {
    /// Number of times this method was invoked.
    pub invocations: AtomicU64,
    /// Total time spent in this method (nanoseconds, approximate).
    pub total_ns: AtomicU64,
    /// Whether this method has been deoptimized.
    pub deoptimized: bool,
}

impl MethodProfile {
    pub fn new() -> Self {
        Self {
            invocations: AtomicU64::new(0),
            total_ns: AtomicU64::new(0),
            deoptimized: false,
        }
    }

    pub fn record_invocation(&self) -> u64 {
        self.invocations.fetch_add(1, Ordering::Relaxed)
    }

    pub fn record_time(&self, ns: u64) {
        self.total_ns.fetch_add(ns, Ordering::Relaxed);
    }

    pub fn count(&self) -> u64 {
        self.invocations.load(Ordering::Relaxed)
    }

    pub fn avg_ns(&self) -> u64 {
        let count = self.count();
        if count == 0 { 0 } else {
            self.total_ns.load(Ordering::Relaxed) / count
        }
    }
}

impl Default for MethodProfile {
    fn default() -> Self { Self::new() }
}

/// Global profiler that tracks all method profiles.
pub struct Profiler {
    profiles: HashMap<String, MethodProfile>,
}

impl Profiler {
    pub fn new() -> Self {
        Self { profiles: HashMap::new() }
    }

    /// Record an invocation of a method. Returns the new count.
    pub fn record(&mut self, method: &str) -> u64 {
        let profile = self.profiles
            .entry(method.to_string())
            .or_insert_with(MethodProfile::new);
        profile.record_invocation()
    }

    /// Get the invocation count for a method.
    pub fn count(&self, method: &str) -> u64 {
        self.profiles.get(method).map(|p| p.count()).unwrap_or(0)
    }

    /// Get all hot methods (above the given threshold).
    pub fn hot_methods(&self, threshold: u64) -> Vec<&str> {
        self.profiles.iter()
            .filter(|(_, p)| p.count() >= threshold)
            .map(|(name, _)| name.as_str())
            .collect()
    }

    /// Mark a method as deoptimized.
    pub fn deoptimize(&mut self, method: &str) {
        if let Some(profile) = self.profiles.get_mut(method) {
            profile.deoptimized = true;
            profile.invocations.store(0, Ordering::Relaxed);
        }
    }

    /// Reset all profiles.
    pub fn reset(&mut self) {
        self.profiles.clear();
    }
}

impl Default for Profiler {
    fn default() -> Self { Self::new() }
}

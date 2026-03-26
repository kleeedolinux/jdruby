//! # JDGC — Julia's Dream Garbage Collector
//!
//! A production-ready, concurrent, region-based garbage collector for Ruby.
//!
//! ## Module Structure (inspired by MMTk)
//!
//! - `abi`: Ruby ABI compatibility layer
//! - `util`: Utilities and constants  
//! - `header`: Object header with packed color/forwarding
//! - `heap`: Region-based heap management
//! - `allocator`: TLAB and bump-pointer allocation
//! - `marker`: Tri-color concurrent marking
//! - `barrier`: Read/write barriers for concurrent GC
//! - `collector`: GC controller and work distribution
//! - `scanning`: Root set and stack scanning

pub mod abi;
pub mod util;
pub mod header;
pub mod region;
pub mod tlab;
pub mod heap;
pub mod allocator;
pub mod marker;
pub mod barrier;
pub mod collector;
pub mod roots;

pub use abi::*;
pub use util::*;
pub use header::{ObjectHeader, Color, ObjectAccess};
pub use region::{Region, RegionManager};
pub use tlab::{Tlab, ThreadLocalTlab, TlabStats};
pub use heap::Heap;
pub use allocator::{Allocator, GcPtr, AllocationError, GcObject};
pub use marker::{Marker, MarkQueue, MarkerStats};
pub use barrier::{ReadBarrier, WriteBarrier, BarrierType};
pub use collector::{Collector, GcPhase, GcConfig, CollectorStats};
pub use roots::{RootSet, RootHandle, RootError, ThreadLocalRoots, StackScanner};

/// JDGC version.
pub const JDGC_VERSION: &str = "0.1.0";

/// Initialize JDGC with default configuration.
pub fn init() -> Heap {
    Heap::new(GcConfig::default())
}

/// Initialize JDGC with custom configuration.
pub fn init_with_config(config: GcConfig) -> Heap {
    Heap::new(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heap_creation() {
        let heap = Heap::new(GcConfig::default());
        // Just verify it doesn't panic
        let _ = heap.total_size();
    }
}

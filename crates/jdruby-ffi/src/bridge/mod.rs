//! # Bridge — JDGC-Aware JDRuby ↔ MRI VALUE Conversion
//!
//! Provides zero-copy (where possible) conversion between JDRuby's internal
//! `RubyValue` and the MRI-compatible `VALUE` used at the C-ABI boundary.

pub mod registry;
pub mod allocator;
pub mod pinning;
pub mod conversion;
pub mod dedup;

pub use registry::{ObjectRef, with_registry, init_bridge, ffi_registry_sweep};
pub use allocator::allocate_object;
pub use pinning::{pin_object, unpin_object};
pub use conversion::{jdruby_to_value, value_to_jdruby};
pub use dedup::{str_to_value, value_to_str, value_ary_len, value_ary_entry};

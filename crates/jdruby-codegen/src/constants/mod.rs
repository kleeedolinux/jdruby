//! Constant Management for Code Generation
//!
//! Provides string interning, constant deduplication, and pre-declaration
//! of constants before function emission.

pub mod pool;
pub mod table;

pub use pool::StringPool;
pub use table::ConstantTable;

//! # Storage — Split Storage Tables for Methods, Symbols, Classes, IVars, Constants
//!
//! Replaces the monolithic MethodTable with focused, single-responsibility storage modules.

pub mod symbol_table;
pub mod class_table;
pub mod ivar_storage;
pub mod constant_table;
pub mod method_storage;

pub use symbol_table::{SymbolTable, with_symbol_table};
pub use class_table::{ClassTable, with_class_table};
pub use ivar_storage::{IvarStorage, with_ivar_storage};
pub use constant_table::{ConstantTable, with_constant_table};
pub use method_storage::{MethodStorage, MethodEntry, with_method_storage};

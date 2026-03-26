//! # JDRuby Standard Library
//!
//! Modular implementation of Ruby's core classes following MRI structure.
//! Each module corresponds to a Ruby class (Array, Hash, String, etc.)

pub mod array;
pub mod hash;
pub mod string;
pub mod symbol;
pub mod time;
pub mod io;
pub mod dir;
pub mod proc;
pub mod range;
pub mod encoding;
pub mod thread_ext;

// Re-export core types for convenience
pub use array::RubyArray;
pub use hash::RubyHash;
pub use string::RubyString;
pub use symbol::{SymbolTable, ID, rb_intern, rb_id2name};
pub use time::RubyTime;
pub use io::{RubyIO, RubyFile};
pub use dir::RubyDir;
pub use proc::{RubyProc, RubyBinding, Block};
pub use range::RubyRange;

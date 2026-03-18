//! Ruby value representation — the core object model.
//!
//! All Ruby values are represented as `RubyValue`, which uses tagged unions
//! for small values (integers, bools, nil, symbols) and heap-allocated
//! objects for larger types (strings, arrays, hashes, custom objects).

use std::collections::HashMap;

/// A Ruby value — the fundamental runtime type.
#[derive(Debug, Clone)]
pub enum RubyValue {
    /// `nil`
    Nil,
    /// `true` or `false`
    Bool(bool),
    /// Integer (Fixnum — fits in i64)
    Integer(i64),
    /// Float
    Float(f64),
    /// Symbol (interned string, stored as ID)
    Symbol(u64),
    /// String
    String(RubyString),
    /// Array
    Array(Vec<RubyValue>),
    /// Hash
    Hash(RubyHash),
    /// A general object instance
    Object(Box<RubyObject>),
}

/// A Ruby string with mutable buffer.
#[derive(Debug, Clone)]
pub struct RubyString {
    pub data: String,
    pub frozen: bool,
}

/// A Ruby hash (ordered map).
#[derive(Debug, Clone)]
pub struct RubyHash {
    pub entries: Vec<(RubyValue, RubyValue)>,
}

/// A general Ruby object.
#[derive(Debug, Clone)]
pub struct RubyObject {
    /// The class ID of this object.
    pub class_id: u64,
    /// Instance variables: `@name` → value
    pub ivars: HashMap<String, RubyValue>,
    /// Whether this object is frozen.
    pub frozen: bool,
}

/// Object header for GC tracking.
#[derive(Debug, Clone, Copy)]
pub struct ObjectHeader {
    /// Flags: marked, pinned, frozen, etc.
    pub flags: u32,
    /// Class information pointer/ID.
    pub class_id: u64,
}

impl RubyValue {
    /// Check if this value is truthy (everything except `nil` and `false`).
    pub fn is_truthy(&self) -> bool {
        !matches!(self, Self::Nil | Self::Bool(false))
    }

    /// Check if this value is `nil`.
    pub fn is_nil(&self) -> bool {
        matches!(self, Self::Nil)
    }

    /// Get the Ruby type name of this value.
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Nil => "NilClass",
            Self::Bool(true) => "TrueClass",
            Self::Bool(false) => "FalseClass",
            Self::Integer(_) => "Integer",
            Self::Float(_) => "Float",
            Self::Symbol(_) => "Symbol",
            Self::String(_) => "String",
            Self::Array(_) => "Array",
            Self::Hash(_) => "Hash",
            Self::Object(_) => "Object",
        }
    }
}

impl Default for RubyValue {
    fn default() -> Self {
        Self::Nil
    }
}

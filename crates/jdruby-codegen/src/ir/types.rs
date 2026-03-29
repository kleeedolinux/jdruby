//! Ruby Type System for LLVM Code Generation
//!
//! Maps Ruby types to LLVM types and provides type-aware code generation support.
//! This is crucial for optimization - if we know types at compile time,
//! we can generate direct LLVM operations instead of runtime calls.

use inkwell::context::Context;
use inkwell::types::BasicTypeEnum;
use inkwell::AddressSpace;

/// Ruby type known at compile time.
/// 
/// This enum represents the type information we can infer or know about
/// Ruby values during compilation. Knowing types enables generating
/// optimized LLVM IR instead of always calling runtime functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RubyType {
    /// Unknown type - must use generic operations.
    /// This is the conservative case when we can't determine the type.
    Unknown,

    /// Fixnum (small integer).
    /// 
    /// In Ruby, Fixnums are immediate values represented as tagged i64:
    /// `value = (integer << 1) | 1`
    /// 
    /// The tag bit (LSB) being 1 indicates a Fixnum.
    Integer,

    /// Flonum (small float) or heap Float.
    /// 
    /// On 64-bit systems, certain floats can be encoded as immediate values
    /// (Flonums). Others require heap allocation as Float objects.
    Float,

    /// String (heap allocated).
    /// 
    /// Ruby strings are mutable, heap-allocated objects with embedded
    /// character data or external pointer.
    String,

    /// Symbol (interned string, immediate).
    /// 
    /// Symbols are represented as unique IDs in Ruby. They're immutable
    /// and interned in a global table.
    Symbol,

    /// Array (heap allocated).
    /// 
    /// Dynamic array with embedded or external buffer.
    Array,

    /// Hash (heap allocated).
    /// 
    /// Hash table with key-value pairs.
    Hash,

    /// Specific class instance.
    /// The string is the class name.
    Object(&'static str),

    /// nil (immediate, special value).
    /// 
    /// Qnil is typically represented as a specific value (e.g., 0x04 or 0x08)
    /// that doesn't conflict with other immediate types.
    Nil,

    /// true/false (immediates).
    /// 
    /// Qtrue and Qfalse are specific immediate values.
    Boolean,

    /// Block/Proc/Lambda.
    /// 
    /// A callable object capturing its environment.
    Block,

    /// Class/Module.
    /// 
    /// The class or module object itself.
    Class,

    /// Method object.
    /// 
    /// A bound or unbound method reference.
    Method,
}

impl RubyType {
    /// Check if this type is immediate (no heap allocation).
    /// 
    /// Immediate values can be stored directly in registers without
    /// pointer indirection. In Ruby, Fixnums, Symbols, true, false,
    /// and nil are all immediate.
    pub fn is_immediate(&self) -> bool {
        matches!(
            self,
            RubyType::Integer
                | RubyType::Float
                | RubyType::Nil
                | RubyType::Boolean
                | RubyType::Symbol
        )
    }

    /// Check if this type is a heap-allocated object.
    pub fn is_heap(&self) -> bool {
        !self.is_immediate() && !matches!(self, RubyType::Unknown)
    }

    /// Get the LLVM type for this Ruby type.
    /// 
    /// Most Ruby values use i64 (the VALUE type). Objects use opaque pointers.
    pub fn llvm_type<'ctx>(&self, ctx: &'ctx Context) -> BasicTypeEnum<'ctx> {
        match self {
            // Most Ruby values are i64 (VALUE type)
            RubyType::Integer
            | RubyType::Nil
            | RubyType::Boolean
            | RubyType::Symbol => ctx.i64_type().into(),

            // Floats are either f64 (unboxed) or i64 (tagged Flonum)
            RubyType::Float => ctx.f64_type().into(),

            // Objects are pointers
            RubyType::String
            | RubyType::Array
            | RubyType::Hash
            | RubyType::Object(_)
            | RubyType::Class
            | RubyType::Block
            | RubyType::Method => ctx.ptr_type(AddressSpace::default()).into(),

            // Unknown defaults to i64 (VALUE)
            RubyType::Unknown => ctx.i64_type().into(),
        }
    }

    /// Get the appropriate register class for this type.
    /// 
    /// This helps LLVM allocate appropriate physical registers.
    pub fn register_class(&self) -> RegisterClass {
        match self {
            RubyType::Integer => RegisterClass::Integer,
            RubyType::Float => RegisterClass::Float,
            RubyType::Nil | RubyType::Boolean | RubyType::Symbol => RegisterClass::Value,
            _ => RegisterClass::General,
        }
    }

    /// Get the tag bit pattern for this type (if immediate).
    /// 
    /// Returns the expected LSB pattern for immediate values.
    pub fn tag_bits(&self) -> Option<u64> {
        match self {
            RubyType::Integer => Some(1), // Fixnums end with 1
            RubyType::Nil => Some(0x04), // Qnil
            RubyType::Boolean => Some(0x02), // Qtrue = 0x02, Qfalse = 0x00
            RubyType::Symbol => Some(0x0e), // Symbols end with 1110
            _ => None, // Heap objects or unknown
        }
    }

    /// Check if two types could potentially be the same at runtime.
    /// 
    /// Used for type guard generation - if types are disjoint,
    /// we can generate early exits or deoptimizations.
    pub fn could_be(&self, other: &RubyType) -> bool {
        match (self, other) {
            (a, b) if a == b => true,
            (RubyType::Unknown, _) | (_, RubyType::Unknown) => true,
            (RubyType::Object(_), RubyType::Object(_)) => true, // Could be same class
            (RubyType::Float, RubyType::Integer) | (RubyType::Integer, RubyType::Float) => {
                // Both are numeric, but distinct types in Ruby 3.x
                false
            }
            _ => false,
        }
    }
}

/// Register class for virtual register allocation.
/// 
/// Different types benefit from different register classes on various
/// architectures. This provides hints to LLVM's register allocator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RegisterClass {
    /// Integer registers (for Fixnums, IDs).
    /// Maps to RAX, RBX, etc. on x86_64.
    Integer,

    /// Floating point (for Flonums).
    /// Maps to XMM registers on x86_64.
    Float,

    /// General purpose (for pointers, objects).
    /// Can be any GPR.
    General,

    /// Special Ruby VALUE register.
    /// Always i64, treated specially.
    Value,
}

impl RegisterClass {
    /// Get the preferred number of registers for this class.
    pub fn preferred_count(&self) -> u32 {
        match self {
            RegisterClass::Integer => 8,
            RegisterClass::Float => 8,
            RegisterClass::General => 16,
            RegisterClass::Value => 8,
        }
    }
}

/// Type inference result for a register.
/// 
/// Combines type information with confidence level.
#[derive(Debug, Clone)]
pub struct InferredType {
    pub ty: RubyType,
    pub confidence: TypeConfidence,
}

impl InferredType {
    pub fn certain(ty: RubyType) -> Self {
        Self {
            ty,
            confidence: TypeConfidence::Certain,
        }
    }

    pub fn speculative(ty: RubyType) -> Self {
        Self {
            ty,
            confidence: TypeConfidence::Speculative,
        }
    }

    /// Check if we should generate optimized code for this type.
    pub fn should_optimize(&self) -> bool {
        matches!(self.confidence, TypeConfidence::Certain)
    }
}

/// Confidence level for type inference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeConfidence {
    /// Type is known with certainty (e.g., from constant).
    Certain,
    /// Type is likely but not guaranteed (e.g., from profiling).
    Speculative,
    /// Type is guessed based on heuristics.
    Guessed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_immediate_types() {
        assert!(RubyType::Integer.is_immediate());
        assert!(RubyType::Nil.is_immediate());
        assert!(RubyType::Boolean.is_immediate());
        assert!(RubyType::Symbol.is_immediate());
        assert!(!RubyType::String.is_immediate());
        assert!(!RubyType::Array.is_immediate());
        assert!(!RubyType::Object("Foo").is_immediate());
    }

    #[test]
    fn test_tag_bits() {
        assert_eq!(RubyType::Integer.tag_bits(), Some(1));
        assert_eq!(RubyType::Nil.tag_bits(), Some(0x04));
        assert_eq!(RubyType::Boolean.tag_bits(), Some(0x02));
        assert_eq!(RubyType::Symbol.tag_bits(), Some(0x0e));
        assert_eq!(RubyType::String.tag_bits(), None);
        assert_eq!(RubyType::Array.tag_bits(), None);
    }

    #[test]
    fn test_type_compatibility() {
        assert!(RubyType::Integer.could_be(&RubyType::Integer));
        assert!(RubyType::Integer.could_be(&RubyType::Unknown));
        assert!(RubyType::Unknown.could_be(&RubyType::String));
        assert!(!RubyType::Integer.could_be(&RubyType::String));
        assert!(RubyType::Object("Foo").could_be(&RubyType::Object("Bar")));
    }

    #[test]
    fn test_register_class() {
        assert_eq!(RubyType::Integer.register_class(), RegisterClass::Integer);
        assert_eq!(RubyType::Float.register_class(), RegisterClass::Float);
        assert_eq!(RubyType::String.register_class(), RegisterClass::General);
        assert_eq!(RubyType::Nil.register_class(), RegisterClass::Value);
    }
}

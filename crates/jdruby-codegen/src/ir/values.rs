//! Typed Value Representation for LLVM Code Generation
//!
//! Wraps LLVM values with Ruby type information, enabling type-aware
//! code generation and optimization.

use crate::ir::types::{RubyType, TypeConfidence};
use inkwell::values::BasicValueEnum;
use jdruby_common::SourceSpan;

/// A Ruby value with compile-time type information.
/// 
/// This struct wraps an LLVM value and tracks:
/// - The Ruby type (if known)
/// - Whether it's an immediate value
/// - Source location for debugging
/// 
/// This enables the code generator to:
/// 1. Generate type-specific LLVM operations
/// 2. Emit type guards when types are uncertain
/// 3. Box/unbox values efficiently
#[derive(Debug, Clone)]
pub struct TypedValue<'ctx> {
    /// The LLVM value (register or constant).
    llvm_value: BasicValueEnum<'ctx>,

    /// The Ruby type (if known).
    ruby_type: RubyType,

    /// Confidence level in the type information.
    confidence: TypeConfidence,

    /// Source location for debugging.
    source_info: Option<SourceSpan>,
}

impl<'ctx> TypedValue<'ctx> {
    /// Create a new typed value with certain type information.
    pub fn new(
        llvm_value: BasicValueEnum<'ctx>,
        ruby_type: RubyType,
        source_info: Option<SourceSpan>,
    ) -> Self {
        Self {
            llvm_value,
            ruby_type,
            confidence: TypeConfidence::Certain,
            source_info,
        }
    }

    /// Create a typed value with speculative type information.
    pub fn speculative(
        llvm_value: BasicValueEnum<'ctx>,
        ruby_type: RubyType,
        source_info: Option<SourceSpan>,
    ) -> Self {
        Self {
            llvm_value,
            ruby_type,
            confidence: TypeConfidence::Speculative,
            source_info,
        }
    }

    /// Create an unknown-typed value.
    pub fn unknown(llvm_value: BasicValueEnum<'ctx>) -> Self {
        Self {
            llvm_value,
            ruby_type: RubyType::Unknown,
            confidence: TypeConfidence::Guessed,
            source_info: None,
        }
    }

    /// Get the LLVM value.
    pub fn llvm_value(&self) -> BasicValueEnum<'ctx> {
        self.llvm_value
    }

    /// Get the Ruby type.
    pub fn ruby_type(&self) -> RubyType {
        self.ruby_type
    }

    /// Check if this value is an immediate.
    pub fn is_immediate(&self) -> bool {
        self.ruby_type.is_immediate()
    }

    /// Check if the type is known with certainty.
    pub fn is_certain(&self) -> bool {
        matches!(self.confidence, TypeConfidence::Certain)
    }

    /// Get the confidence level.
    pub fn confidence(&self) -> TypeConfidence {
        self.confidence
    }

    /// Get source information.
    pub fn source_info(&self) -> Option<SourceSpan> {
        self.source_info
    }

    /// Try to get this value as an integer.
    /// 
    /// Returns Some if the Ruby type is Integer, None otherwise.
    pub fn as_int_value(&self) -> Option<inkwell::values::IntValue<'ctx>> {
        if self.ruby_type == RubyType::Integer {
            Some(self.llvm_value.into_int_value())
        } else {
            None
        }
    }

    /// Try to get this value as a float.
    pub fn as_float_value(&self) -> Option<inkwell::values::FloatValue<'ctx>> {
        if self.ruby_type == RubyType::Float {
            Some(self.llvm_value.into_float_value())
        } else {
            None
        }
    }

    /// Try to get this value as a pointer.
    pub fn as_pointer_value(&self) -> Option<inkwell::values::PointerValue<'ctx>> {
        match self.ruby_type {
            RubyType::String
            | RubyType::Array
            | RubyType::Hash
            | RubyType::Object(_)
            | RubyType::Class
            | RubyType::Block
            | RubyType::Method => Some(self.llvm_value.into_pointer_value()),
            _ => None,
        }
    }

    /// Create a new value with updated type information.
    pub fn with_type(&self, new_type: RubyType) -> Self {
        Self {
            llvm_value: self.llvm_value,
            ruby_type: new_type,
            confidence: self.confidence,
            source_info: self.source_info,
        }
    }

    /// Create a new value with the same type but different LLVM value.
    pub fn with_llvm_value(&self, new_value: BasicValueEnum<'ctx>) -> Self {
        Self {
            llvm_value: new_value,
            ruby_type: self.ruby_type,
            confidence: self.confidence,
            source_info: self.source_info,
        }
    }

    /// Mark this value as requiring a type guard.
    /// 
    /// This is used when we need to emit a runtime type check.
    pub fn needs_guard(&self) -> bool {
        !self.is_certain() && !matches!(self.ruby_type, RubyType::Unknown)
    }

    /// Get the expected tag bits for this value.
    /// 
    /// Returns None if the type doesn't have a tag pattern.
    pub fn expected_tag(&self) -> Option<u64> {
        self.ruby_type.tag_bits()
    }
}

/// A collection of typed values (e.g., function arguments).
#[derive(Debug, Clone)]
pub struct TypedValues<'ctx> {
    values: Vec<TypedValue<'ctx>>,
}

impl<'ctx> TypedValues<'ctx> {
    pub fn new(values: Vec<TypedValue<'ctx>>) -> Self {
        Self { values }
    }

    pub fn empty() -> Self {
        Self { values: vec![] }
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    pub fn get(&self, index: usize) -> Option<&TypedValue<'ctx>> {
        self.values.get(index)
    }

    pub fn iter(&self) -> std::slice::Iter<TypedValue<'ctx>> {
        self.values.iter()
    }

    /// Check if all values have known types.
    pub fn all_types_known(&self) -> bool {
        self.values.iter().all(|v| v.is_certain())
    }

    /// Get the types of all values.
    pub fn types(&self) -> Vec<RubyType> {
        self.values.iter().map(|v| v.ruby_type()).collect()
    }
}

impl<'ctx> IntoIterator for TypedValues<'ctx> {
    type Item = TypedValue<'ctx>;
    type IntoIter = std::vec::IntoIter<TypedValue<'ctx>>;

    fn into_iter(self) -> Self::IntoIter {
        self.values.into_iter()
    }
}

/// Trait for objects that can provide type information.
/// 
/// Used by the code generator to look up register types.
pub trait TypeProvider {
    /// Get the type of a register (by ID).
    fn get_type(&self, reg_id: u32) -> Option<RubyType>;

    /// Check if a register has a known type.
    fn has_known_type(&self, reg_id: u32) -> bool {
        self.get_type(reg_id).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use inkwell::context::Context;

    #[test]
    fn test_typed_value_creation() {
        let ctx = Context::create();
        let i64_val = ctx.i64_type().const_int(42, false);
        let val = TypedValue::new(i64_val.into(), RubyType::Integer, None);

        assert_eq!(val.ruby_type(), RubyType::Integer);
        assert!(val.is_immediate());
        assert!(val.is_certain());
    }

    #[test]
    fn test_unknown_value() {
        let ctx = Context::create();
        let i64_val = ctx.i64_type().const_int(0, false);
        let val = TypedValue::unknown(i64_val.into());

        assert_eq!(val.ruby_type(), RubyType::Unknown);
        assert!(!val.is_certain());
        assert!(!val.needs_guard()); // Unknown values don't need guards
    }

    #[test]
    fn test_as_int_value() {
        let ctx = Context::create();
        let i64_val = ctx.i64_type().const_int(42, false);
        let int_val = TypedValue::new(i64_val.into(), RubyType::Integer, None);
        let str_val = TypedValue::new(
            ctx.ptr_type(inkwell::AddressSpace::default()).const_null().into(),
            RubyType::String,
            None,
        );

        assert!(int_val.as_int_value().is_some());
        assert!(str_val.as_int_value().is_none());
    }

    #[test]
    fn test_typed_values_collection() {
        let ctx = Context::create();
        let values = TypedValues::new(vec![
            TypedValue::new(ctx.i64_type().const_int(1, false).into(), RubyType::Integer, None),
            TypedValue::new(ctx.i64_type().const_int(2, false).into(), RubyType::Integer, None),
        ]);

        assert_eq!(values.len(), 2);
        assert!(values.all_types_known());
    }
}

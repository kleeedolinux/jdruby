//! # Conversion — JDRuby ↔ MRI VALUE Conversion
//!
//! Bidirectional conversion between JDRuby's `RubyValue` enum and MRI's `VALUE`.

use jdgc::GcPtr;
use crate::core::{VALUE, RUBY_QNIL, RUBY_QTRUE, RUBY_QFALSE, rb_int2fix, rb_id2sym, RubyType};
use crate::bridge::registry::{with_registry, ObjectRef, RString, RArray};
use crate::core::RBasic;
use crate::bridge::pinning::pin_object;
use std::alloc::Layout;

/// Convert a JDRuby runtime value to an MRI-compatible VALUE.
pub fn jdruby_to_value(rv: &jdruby_runtime::value::RubyValue) -> VALUE {
    use jdruby_runtime::value::RubyValue as RV;

    match rv {
        RV::Integer(i) => rb_int2fix(*i),
        RV::True => RUBY_QTRUE,
        RV::False => RUBY_QFALSE,
        RV::Nil => RUBY_QNIL,
        RV::Symbol(id) => rb_id2sym(*id as usize),
        RV::String(rs) => allocate_rstring(&rs.data).unwrap_or(RUBY_QNIL),
        RV::Array(elements) => allocate_rarray(elements).unwrap_or(RUBY_QNIL),
        RV::Float(f) => allocate_float(*f).unwrap_or(RUBY_QNIL),
        _ => RUBY_QNIL,
    }
}

/// Convert an MRI-compatible VALUE back to a JDRuby runtime value.
pub fn value_to_jdruby(v: VALUE) -> jdruby_runtime::value::RubyValue {
    use jdruby_runtime::value::{RubyValue as RV, RubyString};
    use crate::core::{RUBY_QNIL, RUBY_QTRUE, RUBY_QFALSE, rb_fixnum_p, rb_fix2long, rb_symbol_p, rb_sym2id};

    // Check immediate types first (no heap access)
    if v == RUBY_QNIL {
        return RV::Nil;
    }
    if v == RUBY_QTRUE {
        return RV::True;
    }
    if v == RUBY_QFALSE {
        return RV::False;
    }
    if rb_fixnum_p(v) {
        return RV::Integer(rb_fix2long(v));
    }
    if rb_symbol_p(v) {
        return RV::Symbol(rb_sym2id(v) as u64);
    }

    // Heap object — look up in registry
    with_registry(|registry| {
        match registry.get(v)? {
            ObjectRef::String(ptr) => {
                unsafe {
                    let rstring = &*ptr.as_ptr();
                    let len = rstring.len as usize;
                    let data_ptr = rstring.ptr;
                    let bytes = std::slice::from_raw_parts(data_ptr, len);
                    if let Ok(s) = String::from_utf8(bytes.to_vec()) {
                        return Some(RV::String(RubyString::new(s)));
                    }
                }
            }
            ObjectRef::Array(ptr) => {
                unsafe {
                    let rarray = &*ptr.as_ptr();
                    let len = rarray.len as usize;
                    let data_ptr = rarray.ptr;
                    let elements = std::slice::from_raw_parts(data_ptr, len);
                    let converted: Vec<jdruby_runtime::value::RubyValue> = elements
                        .iter()
                        .map(|&val| value_to_jdruby(val))
                        .collect();
                    return Some(RV::Array(converted));
                }
            }
            ObjectRef::Float(ptr) => {
                unsafe {
                    let bits = *ptr.as_ptr();
                    return Some(RV::Float(f64::from_bits(bits)));
                }
            }
            _ => {}
        }
        Some(RV::Nil)
    }).unwrap_or(RV::Nil)
}

/// Allocate an RString on the JDGC heap.
pub fn allocate_rstring(s: &str) -> Option<VALUE> {
    use crate::bridge::allocator::allocate_object;
    
    let data = s.as_bytes();
    let len = data.len();
    
    let total_size = std::mem::size_of::<jdgc::ObjectHeader>()
        + std::mem::size_of::<RString>()
        + len;
    
    let layout = Layout::from_size_align(total_size, 8).unwrap();
    
    let (gc_ptr, data_area) = allocate_object::<u8>(layout)?;
    
    let value = with_registry(|r| r.alloc_value());
    
    // Initialize RString fields
    let rstring_ptr = data_area as *mut RString;
    unsafe {
        (*rstring_ptr).basic = RBasic {
            flags: RubyType::String as u32 as VALUE,
            klass: 0,
        };
        (*rstring_ptr).len = len as isize;
        (*rstring_ptr).capa = len as isize;
        (*rstring_ptr).ptr = data_area.add(std::mem::size_of::<RString>());
        
        // Copy string data
        std::ptr::copy_nonoverlapping(
            data.as_ptr(),
            (*rstring_ptr).ptr,
            len
        );
    }
    
    // Pin and register
    pin_object(gc_ptr);
    
    let rstring_gc_ptr = GcPtr::<RString>::from_raw(rstring_ptr).unwrap();
    with_registry(|r| {
        r.insert(value, ObjectRef::String(rstring_gc_ptr));
    });
    
    Some(value)
}

/// Allocate an RArray on the JDGC heap.
pub fn allocate_rarray(elements: &[jdruby_runtime::value::RubyValue]) -> Option<VALUE> {
    use crate::bridge::allocator::allocate_object;
    
    let values: Vec<VALUE> = elements.iter().map(|e| jdruby_to_value(e)).collect();
    let len = values.len();
    
    let total_size = std::mem::size_of::<jdgc::ObjectHeader>()
        + std::mem::size_of::<RArray>()
        + (len * std::mem::size_of::<VALUE>());
    
    let layout = Layout::from_size_align(total_size, 8).unwrap();
    
    let (gc_ptr, data_area) = allocate_object::<u8>(layout)?;
    
    let value = with_registry(|r| r.alloc_value());
    
    // Initialize RArray fields
    let rarray_ptr = data_area as *mut RArray;
    unsafe {
        (*rarray_ptr).basic = RBasic {
            flags: RubyType::Array as u32 as VALUE,
            klass: 0,
        };
        (*rarray_ptr).len = len as isize;
        (*rarray_ptr).capa = len as isize;
        (*rarray_ptr).ptr = data_area.add(std::mem::size_of::<RArray>()) as *mut VALUE;
        
        // Copy element data
        std::ptr::copy_nonoverlapping(
            values.as_ptr(),
            (*rarray_ptr).ptr,
            len
        );
    }
    
    // Pin and register
    pin_object(gc_ptr);
    
    let rarray_gc_ptr = GcPtr::<RArray>::from_raw(rarray_ptr).unwrap();
    with_registry(|r| {
        r.insert(value, ObjectRef::Array(rarray_gc_ptr));
    });
    
    Some(value)
}

/// Allocate a Float on the JDGC heap.
pub fn allocate_float(f: f64) -> Option<VALUE> {
    use crate::bridge::allocator::allocate_object;
    
    let bits = f.to_bits();
    let layout = Layout::new::<(jdgc::ObjectHeader, u64)>();
    
    let (gc_ptr, data_area) = allocate_object::<u8>(layout)?;
    
    let value = with_registry(|r| r.alloc_value());
    
    // Store float bits
    unsafe {
        let bits_ptr = data_area as *mut u64;
        *bits_ptr = bits;
    }
    
    // Pin and register
    pin_object(gc_ptr);
    
    let float_gc_ptr = GcPtr::<u64>::from_raw(data_area as *mut u64).unwrap();
    with_registry(|r| {
        r.insert(value, ObjectRef::Float(float_gc_ptr));
    });
    
    Some(value)
}

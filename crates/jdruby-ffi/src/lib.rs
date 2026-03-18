//! # JDRuby FFI — C-API Compatibility Layer
//!
//! Provides an ABI-compatible C-API shim (`ruby.h` equivalent) so that
//! native C-extensions can be dynamically loaded and interact with
//! JDRuby's compiled object model without knowing they're off MRI.
//!
//! ## Architecture
//!
//! - **VALUE**: MRI-compatible tagged pointer (`usize`).
//!   - Bit 0 = 1: Fixnum (`value >> 1`)
//!   - `0x00`: `Qfalse`
//!   - `0x02`: `Qtrue`
//!   - `0x04`: `Qnil`
//!   - `0x06`: `Qundef`
//!   - Even pointer: heap object (`RBasic*`)
//!
//! - **Bridge Layer**: Converts between JDRuby's `RubyValue` (Rust enum)
//!   and MRI's `VALUE` (tagged `usize`).
//!
//! - **Method Table**: Global registry where `rb_define_method` stores
//!   C-function pointers, and `rb_funcall` dispatches through.

pub mod value;
pub mod bridge;
pub mod ruby_capi;
pub mod method_table;

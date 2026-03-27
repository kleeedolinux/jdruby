//! # I/O API — rb_io_puts, etc.
//!
//! I/O and printing functions.

use crate::core::VALUE;
use crate::bridge::conversion::value_to_jdruby;

/// Print a value with newline.
#[no_mangle]
pub extern "C" fn rb_io_puts(val: VALUE) {
    println!("{}", value_to_jdruby(val).to_ruby_string());
}

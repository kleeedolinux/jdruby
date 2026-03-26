//! # JDGC Utilities and Constants

/// Region size: 2 MiB.
pub const REGION_SIZE: usize = 2 * 1024 * 1024;

/// Region alignment.
pub const REGION_ALIGNMENT: usize = 2 * 1024 * 1024;

/// Minimum object alignment (8 bytes).
pub const OBJ_ALIGN: usize = 8;

/// Bitmask for the tri-color state (bits 0–1).
pub const COLOR_MASK: u64 = 0b11;

/// Bit position of the pinned flag.
pub const PINNED_BIT: u64 = 1 << 2;

/// Bitmask for the forwarding pointer (bits 3–63).
pub const FWD_MASK: u64 = !0b111u64;

/// Bitmask for GC flags.
pub const GC_FLAGS_MASK: u64 = 0b111;

/// Bitmask for forwarding pointer.
pub const FORWARDING_MASK: u64 = !0b111u64;

/// Tri-color states.
pub const WHITE: u64 = 0b00;
pub const GRAY: u64 = 0b01;
pub const BLACK: u64 = 0b10;

/// GC work queue batch size.
pub const WORK_QUEUE_BATCH_SIZE: usize = 64;

/// Evacuation threshold percentage.
pub const EVACUATION_THRESHOLD: f64 = 0.85;

/// GC threshold for triggering collection.
pub const GC_THRESHOLD: f64 = 0.75;

/// GC heap growth factor.
pub const GC_GROWTH_FACTOR: f64 = 1.5;

/// Maximum TLAB allocation size.
pub const MAX_TLAB_ALLOCATION: usize = REGION_SIZE / 8;

/// Minimum TLAB size.
pub const MIN_TLAB_SIZE: usize = 64 * 1024;

/// Default TLAB size.
pub const DEFAULT_TLAB_SIZE: usize = 256 * 1024;

/// GC alignment.
pub const GC_ALIGNMENT: usize = 8;

/// Round `val` up to the next multiple of `align`.
/// `align` must be a power of two.
#[inline]
pub const fn align_up(val: usize, align: usize) -> usize {
    (val + align - 1) & !(align - 1)
}

/// Round `val` down to the previous multiple of `align`.
/// `align` must be a power of two.
#[inline]
pub const fn align_down(val: usize, align: usize) -> usize {
    val & !(align - 1)
}

/// Check if value is aligned.
#[inline]
pub const fn is_aligned(val: usize, align: usize) -> bool {
    val & (align - 1) == 0
}

/// Calculate object size with padding.
#[inline]
pub const fn padded_size(size: usize, align: usize) -> usize {
    align_up(size, align)
}

/// Log2 of a power-of-2 number.
#[inline]
pub const fn log2(val: usize) -> u32 {
    val.trailing_zeros()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_align_up() {
        assert_eq!(align_up(0, 8), 0);
        assert_eq!(align_up(1, 8), 8);
        assert_eq!(align_up(7, 8), 8);
        assert_eq!(align_up(8, 8), 8);
        assert_eq!(align_up(9, 8), 16);
        assert_eq!(align_up(16, 16), 16);
        assert_eq!(align_up(17, 16), 32);
        assert_eq!(align_up(1023, 1024), 1024);
        assert_eq!(align_up(1024, 1024), 1024);
        assert_eq!(align_up(1025, 1024), 2048);
    }

    #[test]
    fn test_align_down() {
        assert_eq!(align_down(0, 8), 0);
        assert_eq!(align_down(7, 8), 0);
        assert_eq!(align_down(8, 8), 8);
        assert_eq!(align_down(9, 8), 8);
        assert_eq!(align_down(15, 8), 8);
        assert_eq!(align_down(16, 8), 16);
        assert_eq!(align_down(1023, 1024), 0);
        assert_eq!(align_down(1024, 1024), 1024);
        assert_eq!(align_down(2047, 1024), 1024);
    }

    #[test]
    fn test_is_aligned() {
        assert!(is_aligned(0, 8));
        assert!(is_aligned(8, 8));
        assert!(is_aligned(16, 8));
        assert!(!is_aligned(1, 8));
        assert!(!is_aligned(7, 8));
        assert!(!is_aligned(9, 8));
        assert!(is_aligned(1024, 1024));
        assert!(!is_aligned(1023, 1024));
    }

    #[test]
    fn test_constants() {
        assert_eq!(REGION_SIZE, 2 * 1024 * 1024);
        assert_eq!(OBJ_ALIGN, 8);
        assert_eq!(COLOR_MASK, 0b11);
        assert_eq!(PINNED_BIT, 0b100);
        assert_eq!(FWD_MASK, !0b111u64);
        assert_eq!(WHITE, 0);
        assert_eq!(GRAY, 1);
        assert_eq!(BLACK, 2);
    }

    #[test]
    fn test_padded_size() {
        assert_eq!(padded_size(1, 8), 8);
        assert_eq!(padded_size(8, 8), 8);
        assert_eq!(padded_size(9, 8), 16);
        assert_eq!(padded_size(1024, 1024), 1024);
        assert_eq!(padded_size(1025, 1024), 2048);
    }

    #[test]
    fn test_log2() {
        assert_eq!(log2(1), 0);
        assert_eq!(log2(2), 1);
        assert_eq!(log2(4), 2);
        assert_eq!(log2(8), 3);
        assert_eq!(log2(1024), 10);
        assert_eq!(log2(2048), 11);
    }

    #[test]
    fn test_gc_constants() {
        assert!(GC_THRESHOLD < 1.0);
        assert!(GC_THRESHOLD > 0.0);
        assert!(GC_GROWTH_FACTOR > 1.0);
        assert!(EVACUATION_THRESHOLD < 1.0);
        assert!(EVACUATION_THRESHOLD > 0.0);
        assert_eq!(MAX_TLAB_ALLOCATION, 256 * 1024);
        assert_eq!(MIN_TLAB_SIZE, 64 * 1024);
        assert_eq!(DEFAULT_TLAB_SIZE, 256 * 1024);
    }
}

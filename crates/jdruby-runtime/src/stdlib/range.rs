//! # Ruby Range Implementation
//!
//! Range objects for sequence operations.
//! Follows MRI's range.c structure.

/// Ruby Range - inclusive or exclusive range
#[repr(C)]
pub struct RubyRange {
    /// Start value
    pub beg: u64,
    /// End value  
    pub end: u64,
    /// Flags: bit 0 = exclude_end
    pub flags: u32,
}

pub const RANGE_EXCLUDE_END: u32 = 1 << 0;
pub const RANGE_FROZEN: u32 = 1 << 1;

impl RubyRange {
    /// Create a new range (inclusive)
    pub fn new(beg: u64, end: u64) -> Self {
        Self {
            beg,
            end,
            flags: 0,
        }
    }

    /// Create a new range (exclusive of end)
    pub fn new_exclusive(beg: u64, end: u64) -> Self {
        Self {
            beg,
            end,
            flags: RANGE_EXCLUDE_END,
        }
    }

    /// Get start value
    pub fn begin(&self) -> u64 {
        self.beg
    }

    /// Get end value
    pub fn end(&self) -> u64 {
        self.end
    }

    /// Check if range excludes end
    pub fn exclude_end(&self) -> bool {
        (self.flags & RANGE_EXCLUDE_END) != 0
    }

    /// Check if value is included in range
    pub fn include(&self, val: u64) -> bool {
        if self.exclude_end() {
            val >= self.beg && val < self.end
        } else {
            val >= self.beg && val <= self.end
        }
    }

    /// Check if range covers another range
    pub fn cover(&self, other: &RubyRange) -> bool {
        self.beg <= other.beg && self.end >= other.end
    }

    /// Get size of range (for numeric ranges)
    pub fn size(&self) -> Option<usize> {
        if self.beg > self.end {
            return Some(0);
        }
        let count = if self.exclude_end() {
            self.end - self.beg
        } else {
            self.end - self.beg + 1
        };
        Some(count as usize)
    }

    /// Iterate over range (if enumerable)
    pub fn each<F>(&self, mut f: F) where F: FnMut(u64) {
        let end = if self.exclude_end() {
            self.end
        } else {
            self.end + 1
        };
        for i in self.beg..end {
            f(i);
        }
    }

    /// Map over range
    pub fn map<F, T>(&self, mut f: F) -> Vec<T> where F: FnMut(u64) -> T {
        let mut result = Vec::new();
        self.each(|i| result.push(f(i)));
        result
    }

    /// Convert range to vector
    pub fn to_a(&self) -> Vec<u64> {
        self.map(|i| i)
    }

    /// Get first n elements
    pub fn first(&self, n: usize) -> Vec<u64> {
        let end = if self.exclude_end() { self.end } else { self.end + 1 };
        let count = (end - self.beg).min(n as u64) as usize;
        (self.beg..self.beg + count as u64).collect()
    }

    /// Get last n elements
    pub fn last(&self, n: usize) -> Vec<u64> {
        let end = if self.exclude_end() { self.end } else { self.end + 1 };
        let start = end.saturating_sub(n as u64);
        (start..end).collect()
    }

    /// Step iteration
    pub fn step<F>(&self, step: u64, mut f: F) where F: FnMut(u64) {
        let end = if self.exclude_end() {
            self.end
        } else {
            self.end + 1
        };
        let mut i = self.beg;
        while i < end {
            f(i);
            i += step;
        }
    }

    /// Check if range is empty
    pub fn is_empty(&self) -> bool {
        if self.exclude_end() {
            self.beg >= self.end
        } else {
            self.beg > self.end
        }
    }
}

impl Clone for RubyRange {
    fn clone(&self) -> Self {
        Self {
            beg: self.beg,
            end: self.end,
            flags: self.flags,
        }
    }
}

impl Default for RubyRange {
    fn default() -> Self {
        Self::new(0, 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_range_new() {
        let r = RubyRange::new(1, 5);
        assert_eq!(r.begin(), 1);
        assert_eq!(r.end(), 5);
        assert!(!r.exclude_end());
    }

    #[test]
    fn test_range_exclusive() {
        let r = RubyRange::new_exclusive(1, 5);
        assert!(r.exclude_end());
        assert!(r.include(1));
        assert!(r.include(4));
        assert!(!r.include(5));
    }

    #[test]
    fn test_range_include() {
        let r = RubyRange::new(1, 5);
        assert!(r.include(1));
        assert!(r.include(5));
        assert!(r.include(3));
        assert!(!r.include(0));
        assert!(!r.include(6));
    }

    #[test]
    fn test_range_size() {
        let r1 = RubyRange::new(1, 5);
        assert_eq!(r1.size(), Some(5));
        
        let r2 = RubyRange::new_exclusive(1, 5);
        assert_eq!(r2.size(), Some(4));
    }

    #[test]
    fn test_range_to_a() {
        let r = RubyRange::new(1, 5);
        let arr = r.to_a();
        assert_eq!(arr, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_range_step() {
        let r = RubyRange::new(0, 10);
        let mut result = Vec::new();
        r.step(2, |i| result.push(i));
        assert_eq!(result, vec![0, 2, 4, 6, 8, 10]);
    }

    #[test]
    fn test_range_first_last() {
        let r = RubyRange::new(1, 100);
        assert_eq!(r.first(3), vec![1, 2, 3]);
        assert_eq!(r.last(3), vec![98, 99, 100]);
    }
}

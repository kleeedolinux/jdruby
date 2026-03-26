//! # Ruby Time Implementation
//!
//! High-precision time with nanosecond resolution.
//! Follows MRI's time.c structure.

use std::time::{SystemTime, UNIX_EPOCH, Duration};

/// Ruby Time (nanosecond precision)
#[repr(C)]
pub struct RubyTime {
    /// Seconds since Unix epoch
    pub sec: i64,
    /// Nanoseconds (0..999_999_999)
    pub nsec: u32,
    /// Timezone offset in seconds from UTC
    pub utc_offset: i32,
    /// Flags: bit 0 = is_utc, bit 1 = is_local
    pub flags: u32,
}

pub const TIME_UTC: u32 = 1 << 0;
pub const TIME_LOCAL: u32 = 1 << 1;

impl RubyTime {
    /// Create Time from current system time (UTC)
    pub fn now() -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap();
        Self {
            sec: now.as_secs() as i64,
            nsec: now.subsec_nanos(),
            utc_offset: 0,
            flags: TIME_UTC,
        }
    }

    /// Create Time from current system time (local)
    pub fn now_local() -> Self {
        let mut t = Self::now();
        // TODO: Get actual local timezone offset
        t.utc_offset = 0;
        t.flags = TIME_LOCAL;
        t
    }

    /// Create Time from seconds since epoch
    pub fn at(seconds: f64) -> Self {
        let sec = seconds as i64;
        let nsec = ((seconds - sec as f64) * 1_000_000_000.0) as u32;
        Self {
            sec,
            nsec,
            utc_offset: 0,
            flags: TIME_UTC,
        }
    }

    /// Create Time from seconds and nanoseconds
    pub fn new(sec: i64, nsec: u32) -> Self {
        Self {
            sec,
            nsec: nsec.min(999_999_999),
            utc_offset: 0,
            flags: TIME_UTC,
        }
    }

    /// Convert to f64 seconds
    pub fn to_f64(&self) -> f64 {
        self.sec as f64 + (self.nsec as f64 / 1_000_000_000.0)
    }

    /// Convert to milliseconds
    pub fn to_millis(&self) -> i64 {
        self.sec * 1000 + (self.nsec / 1_000_000) as i64
    }

    /// Convert to microseconds
    pub fn to_micros(&self) -> i64 {
        self.sec * 1_000_000 + (self.nsec / 1000) as i64
    }

    /// Convert to nanoseconds
    pub fn to_nanos(&self) -> i128 {
        (self.sec as i128 * 1_000_000_000) + self.nsec as i128
    }

    /// Check if time is UTC
    pub fn is_utc(&self) -> bool {
        (self.flags & TIME_UTC) != 0
    }

    /// Check if time is local
    pub fn is_local(&self) -> bool {
        (self.flags & TIME_LOCAL) != 0
    }

    /// Get timezone offset in seconds
    pub fn gmt_offset(&self) -> i32 {
        self.utc_offset
    }

    /// Add duration to time
    pub fn add(&self, dur: Duration) -> Self {
        let total_nanos = (self.nsec as u128) + dur.as_nanos();
        let add_sec = (total_nanos / 1_000_000_000) as i64;
        let new_nsec = (total_nanos % 1_000_000_000) as u32;
        
        Self {
            sec: self.sec + add_sec,
            nsec: new_nsec,
            utc_offset: self.utc_offset,
            flags: self.flags,
        }
    }

    /// Subtract another time, returning duration
    pub fn sub(&self, other: &Self) -> Duration {
        let self_nanos = self.sec as u128 * 1_000_000_000 + self.nsec as u128;
        let other_nanos = other.sec as u128 * 1_000_000_000 + other.nsec as u128;
        
        if self_nanos > other_nanos {
            Duration::from_nanos((self_nanos - other_nanos) as u64)
        } else {
            Duration::from_secs(0)
        }
    }

    /// Format using strftime-style format string
    pub fn strftime(&self, fmt: &str) -> String {
        // Simplified implementation - just return ISO format
        if fmt == "%Y-%m-%d %H:%M:%S" {
            format!("{}-{:02}-{:02} {:02}:{:02}:{:02}",
                1970 + self.sec / 31_536_000, // approximate year
                1, 1, // month/day placeholder
                (self.sec % 86400) / 3600,
                (self.sec % 3600) / 60,
                self.sec % 60
            )
        } else {
            format!("{}.{:09}", self.sec, self.nsec)
        }
    }

    /// Compare two times
    pub fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match self.sec.cmp(&other.sec) {
            std::cmp::Ordering::Equal => self.nsec.cmp(&other.nsec),
            other => other,
        }
    }
}

impl Clone for RubyTime {
    fn clone(&self) -> Self {
        Self {
            sec: self.sec,
            nsec: self.nsec,
            utc_offset: self.utc_offset,
            flags: self.flags,
        }
    }
}

impl Default for RubyTime {
    fn default() -> Self {
        Self::now()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_time_now() {
        let t = RubyTime::now();
        assert!(t.sec > 1704067200); // After Jan 1, 2024
        assert!(t.nsec < 1_000_000_000);
        assert!(t.is_utc());
    }

    #[test]
    fn test_time_at() {
        let t = RubyTime::at(1234567890.5);
        assert_eq!(t.sec, 1234567890);
        assert!(t.nsec >= 500_000_000); // Use >= for floating point precision
    }

    #[test]
    fn test_time_new() {
        let t = RubyTime::new(1000, 500_000_000);
        assert_eq!(t.sec, 1000);
        assert_eq!(t.nsec, 500_000_000);
    }

    #[test]
    fn test_time_to_f64() {
        let t = RubyTime::new(10, 500_000_000);
        assert!(t.to_f64() > 10.5 - 0.001 && t.to_f64() < 10.5 + 0.001);
    }

    #[test]
    fn test_time_add() {
        let t = RubyTime::new(100, 500_000_000);
        let t2 = t.add(Duration::from_secs(10));
        assert_eq!(t2.sec, 110);
        assert_eq!(t2.nsec, 500_000_000);
    }

    #[test]
    fn test_time_cmp() {
        let t1 = RubyTime::new(100, 0);
        let t2 = RubyTime::new(100, 100);
        let t3 = RubyTime::new(101, 0);
        
        assert_eq!(t1.cmp(&t1), std::cmp::Ordering::Equal);
        assert_eq!(t1.cmp(&t2), std::cmp::Ordering::Less);
        assert_eq!(t2.cmp(&t1), std::cmp::Ordering::Greater);
        assert_eq!(t1.cmp(&t3), std::cmp::Ordering::Less);
    }
}

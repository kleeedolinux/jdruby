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
    /// Supports: %Y (year), %m (month 01-12), %d (day 01-31), %H (hour 00-23), %M (minute 00-59), %S (second 00-59),
    /// %N (nanoseconds), %z (timezone offset), %Z (timezone name), %A (weekday name), %B (month name)
    pub fn strftime(&self, fmt: &str) -> String {
        let secs_since_epoch = self.sec;
        
        // Calculate date components (simplified - doesn't handle leap years, month lengths correctly)
        // Using standard Unix epoch calculations
        let days_since_epoch = (secs_since_epoch / 86400) as i64;
        let seconds_of_day = (secs_since_epoch % 86400) as u64;
        
        let hour = (seconds_of_day / 3600) as u32;
        let minute = ((seconds_of_day % 3600) / 60) as u32;
        let second = (seconds_of_day % 60) as u32;
        
        // Zeller's congruence-based weekday calculation (0 = Sunday)
        // Using a simplified algorithm
        let weekday = ((days_since_epoch + 4) % 7) as usize; // Jan 1, 1970 was Thursday (4)
        const WEEKDAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
        const WEEKDAYS_ABBR: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
        
        // Approximate year/month/day calculation (simplified)
        let year = 1970 + (days_since_epoch / 365) as i32;
        let day_of_year = (days_since_epoch % 365) as u32;
        
        // Month calculation (simplified, not accounting for leap years)
        const MONTH_DAYS: [u32; 12] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
        let mut month = 1u32;
        for (i, &days) in MONTH_DAYS.iter().enumerate().skip(1) {
            if day_of_year < days {
                month = i as u32;
                break;
            }
            month = (i + 1) as u32;
        }
        let day = if month > 1 {
            day_of_year - MONTH_DAYS[month as usize - 1] + 1
        } else {
            day_of_year + 1
        };
        
        const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June",
                                     "July", "August", "September", "October", "November", "December"];
        const MONTHS_ABBR: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun",
                                          "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
        
        // Format parsing
        let mut result = String::with_capacity(fmt.len() * 2);
        let mut chars = fmt.chars();
        
        while let Some(ch) = chars.next() {
            if ch == '%' {
                match chars.next() {
                    Some('Y') => result.push_str(&format!("{:04}", year)),
                    Some('y') => result.push_str(&format!("{:02}", year % 100)),
                    Some('m') => result.push_str(&format!("{:02}", month)),
                    Some('d') => result.push_str(&format!("{:02}", day)),
                    Some('e') => result.push_str(&format!("{:2}", day)), // space-padded
                    Some('H') => result.push_str(&format!("{:02}", hour)),
                    Some('k') => result.push_str(&format!("{:2}", hour)), // space-padded
                    Some('M') => result.push_str(&format!("{:02}", minute)),
                    Some('S') => result.push_str(&format!("{:02}", second)),
                    Some('N') => result.push_str(&format!("{:09}", self.nsec)),
                    Some('L') => result.push_str(&format!("{:03}", self.nsec / 1_000_000)), // milliseconds
                    Some('z') => {
                        let offset = self.utc_offset;
                        let sign = if offset >= 0 { '+' } else { '-' };
                        let abs_offset = offset.abs();
                        let offset_hours = abs_offset / 3600;
                        let offset_mins = (abs_offset % 3600) / 60;
                        result.push_str(&format!("{}{:02}{:02}", sign, offset_hours, offset_mins));
                    }
                    Some('Z') => {
                        if self.is_utc() {
                            result.push_str("UTC");
                        } else {
                            result.push_str(&format!("UTC{:+03}", self.utc_offset / 3600));
                        }
                    }
                    Some('A') => result.push_str(WEEKDAYS[weekday]),
                    Some('a') => result.push_str(WEEKDAYS_ABBR[weekday]),
                    Some('B') => result.push_str(MONTHS[(month - 1) as usize]),
                    Some('b') | Some('h') => result.push_str(MONTHS_ABBR[(month - 1) as usize]),
                    Some('j') => result.push_str(&format!("{:03}", day_of_year + 1)), // day of year
                    Some('u') => result.push_str(&format!("{}", if weekday == 0 { 7 } else { weekday })), // weekday 1-7 (Monday=1)
                    Some('w') => result.push_str(&format!("{}", weekday)), // weekday 0-6 (Sunday=0)
                    Some('s') => result.push_str(&format!("{}", self.sec)), // seconds since epoch
                    Some('%') => result.push('%'),
                    Some(other) => {
                        result.push('%');
                        result.push(other);
                    }
                    None => result.push('%'),
                }
            } else {
                result.push(ch);
            }
        }
        
        result
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

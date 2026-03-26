//! # Ruby Encoding Implementation
//!
//! String encoding support.
//! Follows MRI's encoding.c structure.

use std::collections::HashMap;

/// Encoding index type
pub type EncodingIndex = u32;

/// Well-known encoding indices (matching MRI)
pub const ENCINDEX_ASCII_8BIT: EncodingIndex = 0;
pub const ENCINDEX_UTF_8: EncodingIndex = 1;
pub const ENCINDEX_US_ASCII: EncodingIndex = 2;
pub const ENCINDEX_BINARY: EncodingIndex = ENCINDEX_ASCII_8BIT;

/// Ruby Encoding
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RubyEncoding {
    pub index: EncodingIndex,
    pub flags: u32,
}

/// Encoding flags
pub const ENC_FLAG_VALID: u32 = 1 << 0;
pub const ENC_FLAG_INVALID: u32 = 1 << 1;
pub const ENC_FLAG_UNDEF: u32 = 1 << 2;

/// Global encoding table
pub struct EncodingTable {
    encodings: Vec<RubyEncoding>,
    name_to_index: HashMap<String, EncodingIndex>,
}

impl EncodingTable {
    pub fn new() -> Self {
        let mut table = Self {
            encodings: Vec::new(),
            name_to_index: HashMap::new(),
        };
        
        // Register built-in encodings
        table.register("ASCII-8BIT", ENCINDEX_ASCII_8BIT);
        table.register("UTF-8", ENCINDEX_UTF_8);
        table.register("US-ASCII", ENCINDEX_US_ASCII);
        table.register("BINARY", ENCINDEX_BINARY);
        
        table
    }
    
    fn register(&mut self, name: &str, index: EncodingIndex) {
        self.name_to_index.insert(name.to_string(), index);
        if index as usize >= self.encodings.len() {
            self.encodings.resize(index as usize + 1, RubyEncoding {
                index: 0,
                flags: 0,
            });
        }
        self.encodings[index as usize] = RubyEncoding {
            index,
            flags: ENC_FLAG_VALID,
        };
    }
    
    /// Find encoding by name
    pub fn find(&self, name: &str) -> Option<RubyEncoding> {
        self.name_to_index.get(name).map(|&idx| self.encodings[idx as usize])
    }
    
    /// Get encoding by index
    pub fn get(&self, index: EncodingIndex) -> Option<RubyEncoding> {
        self.encodings.get(index as usize).copied()
    }
    
    /// Get encoding name
    pub fn name(&self, enc: RubyEncoding) -> Option<&str> {
        for (name, &idx) in &self.name_to_index {
            if idx == enc.index {
                return Some(name);
            }
        }
        None
    }
}

impl Default for EncodingTable {
    fn default() -> Self {
        Self::new()
    }
}

impl RubyEncoding {
    /// UTF-8 encoding
    pub fn utf8() -> Self {
        Self {
            index: ENCINDEX_UTF_8,
            flags: ENC_FLAG_VALID,
        }
    }
    
    /// Binary/ASCII-8BIT encoding
    pub fn binary() -> Self {
        Self {
            index: ENCINDEX_ASCII_8BIT,
            flags: ENC_FLAG_VALID,
        }
    }
    
    /// US-ASCII encoding
    pub fn us_ascii() -> Self {
        Self {
            index: ENCINDEX_US_ASCII,
            flags: ENC_FLAG_VALID,
        }
    }
    
    /// Check if valid
    pub fn is_valid(&self) -> bool {
        (self.flags & ENC_FLAG_VALID) != 0
    }
    
    /// Check if UTF-8
    pub fn is_utf8(&self) -> bool {
        self.index == ENCINDEX_UTF_8
    }
    
    /// Check if binary
    pub fn is_binary(&self) -> bool {
        self.index == ENCINDEX_ASCII_8BIT || self.index == ENCINDEX_BINARY
    }
    
    /// Check if ASCII-compatible
    pub fn ascii_compatible(&self) -> bool {
        // UTF-8 and US-ASCII are ASCII-compatible
        self.index == ENCINDEX_UTF_8 || self.index == ENCINDEX_US_ASCII
    }
}

/// Check if bytes are valid UTF-8
pub fn is_valid_utf8(bytes: &[u8]) -> bool {
    std::str::from_utf8(bytes).is_ok()
}

/// Check if bytes are valid ASCII
pub fn is_valid_ascii(bytes: &[u8]) -> bool {
    bytes.iter().all(|&b| b < 128)
}

/// Get character length in UTF-8
pub fn utf8_char_len(bytes: &[u8]) -> usize {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.chars().count(),
        Err(_) => bytes.len(), // Fallback to byte count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encoding_utf8() {
        let enc = RubyEncoding::utf8();
        assert!(enc.is_utf8());
        assert!(enc.is_valid());
        assert!(enc.ascii_compatible());
    }

    #[test]
    fn test_encoding_binary() {
        let enc = RubyEncoding::binary();
        assert!(enc.is_binary());
        assert!(!enc.is_utf8());
    }

    #[test]
    fn test_encoding_table() {
        let table = EncodingTable::new();
        let utf8 = table.find("UTF-8").unwrap();
        assert!(utf8.is_utf8());
        
        let binary = table.find("BINARY").unwrap();
        assert!(binary.is_binary());
    }

    #[test]
    fn test_valid_utf8() {
        assert!(is_valid_utf8(b"hello"));
        assert!(is_valid_utf8("こんにちは".as_bytes()));
        assert!(!is_valid_utf8(&[0x80, 0x81, 0x82])); // Invalid sequence
    }

    #[test]
    fn test_valid_ascii() {
        assert!(is_valid_ascii(b"hello"));
        assert!(!is_valid_ascii(&[0x80, 0x81])); // Contains high bit
    }

    #[test]
    fn test_utf8_char_len() {
        assert_eq!(utf8_char_len("hello".as_bytes()), 5);
        assert_eq!(utf8_char_len("こんにちは".as_bytes()), 5); // 5 Japanese chars
    }
}

//! # Ruby Dir Implementation
//!
//! Directory operations.
//! Follows MRI's dir.c structure.

use std::fs::{self, ReadDir};
use std::path::Path;

/// Ruby Dir - directory iterator
#[repr(C)]
pub struct RubyDir {
    pub path: String,
    pub inner: Option<ReadDir>,
}

impl RubyDir {
    /// Open a directory
    pub fn open(path: &str) -> std::io::Result<Self> {
        let inner = fs::read_dir(path)?;
        Ok(Self {
            path: path.to_string(),
            inner: Some(inner),
        })
    }

    /// Read next entry
    pub fn read(&mut self) -> Option<std::io::Result<String>> {
        self.inner.as_mut()?.next().map(|entry| {
            entry.map(|e| e.file_name().to_string_lossy().to_string())
        })
    }

    /// Get all entries as vector
    pub fn entries(&mut self) -> std::io::Result<Vec<String>> {
        let mut result = Vec::new();
        if let Some(inner) = self.inner.as_mut() {
            for entry in inner {
                result.push(entry?.file_name().to_string_lossy().to_string());
            }
        }
        Ok(result)
    }

    /// Rewind to beginning
    pub fn rewind(&mut self) -> std::io::Result<()> {
        self.inner = Some(fs::read_dir(&self.path)?);
        Ok(())
    }

    /// Close directory
    pub fn close(&mut self) {
        self.inner = None;
    }

    /// Check if closed
    pub fn is_closed(&self) -> bool {
        self.inner.is_none()
    }

    // === Class methods ===

    /// Create directory
    pub fn mkdir(path: &str, mode: u32) -> std::io::Result<()> {
        let _ = mode; // Mode not used on all platforms
        fs::create_dir(path)
    }

    /// Create directory recursively
    pub fn mkdir_p(path: &str, mode: u32) -> std::io::Result<()> {
        let _ = mode;
        fs::create_dir_all(path)
    }

    /// Remove directory
    pub fn rmdir(path: &str) -> std::io::Result<()> {
        fs::remove_dir(path)
    }

    /// Remove directory recursively
    pub fn rmdir_r(path: &str) -> std::io::Result<()> {
        fs::remove_dir_all(path)
    }

    /// Check if directory exists
    pub fn exists(path: &str) -> bool {
        Path::new(path).is_dir()
    }

    /// Check if file/directory exists
    pub fn exist(path: &str) -> bool {
        Path::new(path).exists()
    }

    /// Get current working directory
    pub fn pwd() -> std::io::Result<String> {
        std::env::current_dir()
            .map(|p| p.to_string_lossy().to_string())
    }

    /// Change directory
    pub fn chdir(path: &str) -> std::io::Result<()> {
        std::env::set_current_dir(path)
    }

    /// Get home directory
    pub fn home() -> Option<String> {
        std::env::var("HOME").ok()
    }

    /// Glob pattern matching
    pub fn glob(pattern: &str) -> Vec<String> {
        let mut results = Vec::new();
        
        // Simple glob implementation (just handles * wildcard)
        if pattern.contains('*') {
            if let Ok(entries) = Self::glob_dir(".") {
                for entry in entries {
                    if Self::match_glob(&entry, pattern) {
                        results.push(entry);
                    }
                }
            }
        } else {
            // No wildcard - exact match
            if Path::new(pattern).exists() {
                results.push(pattern.to_string());
            }
        }
        
        results
    }

    fn glob_dir(path: &str) -> std::io::Result<Vec<String>> {
        let mut entries = Vec::new();
        for entry in fs::read_dir(path)? {
            entries.push(entry?.file_name().to_string_lossy().to_string());
        }
        Ok(entries)
    }

    fn match_glob(name: &str, pattern: &str) -> bool {
        // Very simple glob matching
        if pattern == "*" {
            return true;
        }
        if pattern.starts_with("*") && pattern.len() > 1 {
            let suffix = &pattern[1..];
            return name.ends_with(suffix);
        }
        if pattern.ends_with("*") && pattern.len() > 1 {
            let prefix = &pattern[..pattern.len()-1];
            return name.starts_with(prefix);
        }
        false
    }
}

impl Drop for RubyDir {
    fn drop(&mut self) {
        self.close();
    }
}

impl Default for RubyDir {
    fn default() -> Self {
        Self {
            path: String::new(),
            inner: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dir_pwd() {
        let pwd = RubyDir::pwd().unwrap();
        assert!(!pwd.is_empty());
    }

    #[test]
    fn test_dir_exists() {
        assert!(RubyDir::exists("/"));
        assert!(RubyDir::exists("/tmp") || RubyDir::exists("/var/tmp"));
        assert!(!RubyDir::exists("/nonexistent_directory_12345"));
    }

    #[test]
    fn test_dir_glob() {
        let results = RubyDir::glob("*");
        // Should find something in current directory
        assert!(!results.is_empty());
    }
}

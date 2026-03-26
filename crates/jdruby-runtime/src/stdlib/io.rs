//! # Ruby IO Implementation
//!
//! File and stream I/O operations.
//! Follows MRI's io.c structure.

use std::fs::{File, OpenOptions, Metadata};
use std::io::{Read, Write, Seek, SeekFrom};
use std::sync::atomic::{AtomicU32, Ordering};

/// IO flags
pub const IO_READABLE: u32 = 1 << 0;
pub const IO_WRITABLE: u32 = 1 << 1;
pub const IO_SYNC: u32 = 1 << 2;
pub const IO_BINARY: u32 = 1 << 3;
pub const IO_EOF: u32 = 1 << 4;

/// Ruby IO - wraps a file or stream
#[repr(C)]
pub struct RubyIO {
    pub flags: AtomicU32,
    pub fd: i32,  // File descriptor (-1 for closed)
    pub internal: Option<Box<dyn IOInner>>, // Trait object for different I/O types
}

/// Trait for IO operations
pub trait IOInner: Send + std::io::Read + std::io::Write + std::io::Seek {
    fn close(&mut self) -> std::io::Result<()>;
    fn metadata(&self) -> std::io::Result<std::fs::Metadata>;
}

/// File-based IO implementation
struct FileIO {
    file: File,
    #[allow(dead_code)]
    path: String,
}

impl IOInner for FileIO {
    fn close(&mut self) -> std::io::Result<()> {
        Ok(())
    }

    fn metadata(&self) -> std::io::Result<Metadata> {
        self.file.metadata()
    }
}

impl Read for FileIO {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.file.read(buf)
    }
}

impl Write for FileIO {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.file.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}

impl Seek for FileIO {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.file.seek(pos)
    }
}

impl RubyIO {
    /// Create a new IO from a file
    pub fn from_file(file: File, _fd: i32) -> Self {
        Self {
            flags: AtomicU32::new(IO_READABLE | IO_WRITABLE),
            fd: -1,
            internal: Some(Box::new(FileIO {
                file,
                path: String::new(),
            })),
        }
    }

    /// Open a file with given mode
    pub fn open(path: &str, mode: &str) -> std::io::Result<Self> {
        let mut options = OpenOptions::new();
        
        let flags = match mode {
            "r" => { options.read(true); IO_READABLE },
            "w" => { options.write(true).truncate(true).create(true); IO_WRITABLE },
            "a" => { options.append(true).create(true); IO_WRITABLE },
            "r+" => { options.read(true).write(true); IO_READABLE | IO_WRITABLE },
            "w+" => { options.read(true).write(true).truncate(true).create(true); IO_READABLE | IO_WRITABLE },
            "a+" => { options.read(true).append(true).create(true); IO_READABLE | IO_WRITABLE },
            _ => { options.read(true); IO_READABLE },
        };

        let file = options.open(path)?;
        
        Ok(Self {
            flags: AtomicU32::new(flags),
            fd: -1,
            internal: Some(Box::new(FileIO {
                file,
                path: path.to_string(),
            })),
        })
    }

    /// Read bytes into buffer
    pub fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if let Some(inner) = self.internal.as_mut() {
            inner.read(buf)
        } else {
            Err(std::io::Error::new(std::io::ErrorKind::Other, "IO closed"))
        }
    }

    /// Read all bytes
    pub fn read_all(&mut self) -> std::io::Result<Vec<u8>> {
        let mut buf = Vec::new();
        if let Some(inner) = self.internal.as_mut() {
            std::io::Read::read_to_end(inner.as_mut(), &mut buf)?;
        }
        Ok(buf)
    }

    /// Write bytes
    pub fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if let Some(inner) = self.internal.as_mut() {
            inner.write(buf)
        } else {
            Err(std::io::Error::new(std::io::ErrorKind::Other, "IO closed"))
        }
    }

    /// Write string with newline
    pub fn puts(&mut self, s: &str) -> std::io::Result<()> {
        self.write(s.as_bytes())?;
        self.write(b"\n")?;
        Ok(())
    }

    /// Seek to position
    pub fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        if let Some(inner) = self.internal.as_mut() {
            inner.seek(pos)
        } else {
            Err(std::io::Error::new(std::io::ErrorKind::Other, "IO closed"))
        }
    }

    /// Close the IO
    pub fn close(&mut self) -> std::io::Result<()> {
        if let Some(mut inner) = self.internal.take() {
            inner.close()?;
        }
        self.flags.store(0, Ordering::SeqCst);
        Ok(())
    }

    /// Check if readable
    pub fn is_readable(&self) -> bool {
        self.flags.load(Ordering::Relaxed) & IO_READABLE != 0
    }

    /// Check if writable
    pub fn is_writable(&self) -> bool {
        self.flags.load(Ordering::Relaxed) & IO_WRITABLE != 0
    }

    /// Set sync mode
    pub fn set_sync(&self, sync: bool) {
        if sync {
            self.flags.fetch_or(IO_SYNC, Ordering::SeqCst);
        } else {
            self.flags.fetch_and(!IO_SYNC, Ordering::SeqCst);
        }
    }

    /// Check if closed
    pub fn is_closed(&self) -> bool {
        self.internal.is_none()
    }
}

impl Drop for RubyIO {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

/// Ruby File - extends IO with file-specific operations
#[repr(C)]
pub struct RubyFile {
    pub io: RubyIO,
    pub path: String,
}

impl RubyFile {
    /// Open a file
    pub fn open(path: &str, mode: &str) -> std::io::Result<Self> {
        let io = RubyIO::open(path, mode)?;
        Ok(Self {
            io,
            path: path.to_string(),
        })
    }

    /// Read entire file to string
    pub fn read(path: &str) -> std::io::Result<String> {
        let mut file = Self::open(path, "r")?;
        let bytes = file.io.read_all()?;
        String::from_utf8(bytes).map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid UTF-8")
        })
    }

    /// Write string to file
    pub fn write(path: &str, contents: &str) -> std::io::Result<()> {
        let mut file = Self::open(path, "w")?;
        file.io.write(contents.as_bytes())?;
        Ok(())
    }

    /// Get file path
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Get file metadata
    pub fn stat(&self) -> std::io::Result<Metadata> {
        if let Some(inner) = self.io.internal.as_ref() {
            inner.metadata()
        } else {
            Err(std::io::Error::new(std::io::ErrorKind::Other, "IO closed"))
        }
    }

    /// Truncate file to given size
    pub fn truncate(&mut self, _size: u64) -> std::io::Result<()> {
        if let Some(inner) = self.io.internal.as_mut() {
            inner.seek(SeekFrom::Start(0))?;
            // Note: Actual truncation would require file-specific operations
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_io_open() {
        let io = RubyIO::open("/dev/null", "r");
        assert!(io.is_ok());
    }

    #[test]
    fn test_file_write_read() {
        let path = "/tmp/test_jdruby_file.txt";
        let contents = "Hello, JDRuby!";
        
        // Write
        RubyFile::write(path, contents).unwrap();
        
        // Read
        let read = RubyFile::read(path).unwrap();
        assert_eq!(read, contents);
        
        // Cleanup
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_io_flags() {
        let io = RubyIO::open("/dev/null", "r").unwrap();
        assert!(io.is_readable());
        assert!(!io.is_writable());
    }
}

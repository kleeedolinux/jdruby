/// A byte-offset range in source code.
///
/// Used to track the exact location of tokens, AST nodes, and diagnostics
/// back to the original source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SourceSpan {
    /// Byte offset of the start (inclusive).
    pub start: usize,
    /// Byte offset of the end (exclusive).
    pub end: usize,
}

impl SourceSpan {
    /// Create a new span from start (inclusive) to end (exclusive).
    pub fn new(start: usize, end: usize) -> Self {
        debug_assert!(start <= end, "SourceSpan start ({start}) > end ({end})");
        Self { start, end }
    }

    /// Create a zero-width span at a single position.
    pub fn at(pos: usize) -> Self {
        Self { start: pos, end: pos }
    }

    /// Merge two spans into one covering both.
    pub fn merge(self, other: Self) -> Self {
        Self {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }

    /// The length of this span in bytes.
    pub fn len(&self) -> usize {
        self.end - self.start
    }

    /// Whether this span is empty.
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
}

impl Default for SourceSpan {
    fn default() -> Self {
        Self { start: 0, end: 0 }
    }
}

impl std::fmt::Display for SourceSpan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}..{}", self.start, self.end)
    }
}

/// Represents a source file loaded into the compiler.
#[derive(Debug, Clone)]
pub struct SourceFile {
    /// The file path or name (e.g., "main.rb").
    pub name: String,
    /// The full source text content.
    pub content: String,
}

impl SourceFile {
    /// Create a new source file.
    pub fn new(name: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            content: content.into(),
        }
    }

    /// Get the line and column (1-indexed) for a byte offset.
    pub fn line_col(&self, offset: usize) -> (usize, usize) {
        let mut line = 1;
        let mut col = 1;
        for (i, ch) in self.content.char_indices() {
            if i >= offset {
                break;
            }
            if ch == '\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
        }
        (line, col)
    }

    /// Extract the text covered by a span.
    pub fn slice(&self, span: &SourceSpan) -> &str {
        &self.content[span.start..span.end]
    }
}

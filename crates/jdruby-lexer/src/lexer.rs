use jdruby_common::{Diagnostic, SourceSpan};

use crate::token::{Token, TokenKind};

/// Hand-written lexer for Ruby source code.
///
/// Scans the source character-by-character, producing a stream of [`Token`]s.
/// Handles Ruby's context-sensitive tokenization including string interpolation,
/// comments, heredocs, and number formats.
pub struct Lexer<'src> {
    /// The source text being tokenized.
    source: &'src str,
    /// The source as a byte slice for fast indexing.
    bytes: &'src [u8],
    /// Current byte position in the source.
    pos: usize,
    /// Collected diagnostics (errors/warnings).
    diagnostics: Vec<Diagnostic>,
}

impl<'src> Lexer<'src> {
    /// Create a new lexer for the given source text.
    pub fn new(source: &'src str) -> Self {
        Self {
            source,
            bytes: source.as_bytes(),
            pos: 0,
            diagnostics: Vec::new(),
        }
    }

    /// Tokenize the entire source and return all tokens + diagnostics.
    pub fn tokenize(&mut self) -> (Vec<Token>, Vec<Diagnostic>) {
        let mut tokens = Vec::new();
        loop {
            let tok = self.next_token();
            let is_eof = tok.kind == TokenKind::Eof;
            tokens.push(tok);
            if is_eof {
                break;
            }
        }
        (tokens, std::mem::take(&mut self.diagnostics))
    }

    /// Scan the next token from the source.
    pub fn next_token(&mut self) -> Token {
        self.skip_whitespace();

        if self.is_at_end() {
            return self.make_token(TokenKind::Eof, self.pos, self.pos);
        }

        let start = self.pos;
        let ch = self.advance();

        match ch {
            // ── Newlines ─────────────────────────────────────
            b'\n' => self.make_token(TokenKind::Newline, start, self.pos),

            // ── Comments ─────────────────────────────────────
            b'#' => self.lex_line_comment(start),

            // ── Strings ──────────────────────────────────────
            b'"' => self.lex_double_string(start),
            b'\'' => self.lex_single_string(start),

            // ── Numbers ──────────────────────────────────────
            b'0'..=b'9' => self.lex_number(start),

            // ── Symbols ──────────────────────────────────────
            b':' => {
                if self.peek() == b':' {
                    self.advance();
                    self.make_token(TokenKind::ColonColon, start, self.pos)
                } else if self.peek().is_ascii_alphabetic() || self.peek() == b'_' {
                    self.lex_symbol(start)
                } else if self.peek() == b'"' {
                    self.advance(); // skip the opening "
                    self.lex_quoted_symbol(start)
                } else {
                    self.make_token(TokenKind::Colon, start, self.pos)
                }
            }

            // ── Instance/Class variables ─────────────────────
            b'@' => {
                if self.peek() == b'@' {
                    self.advance();
                    self.lex_identifier_from(start, TokenKind::ClassVar)
                } else if self.peek().is_ascii_alphabetic() || self.peek() == b'_' {
                    self.lex_identifier_from(start, TokenKind::InstanceVar)
                } else {
                    self.make_token(TokenKind::At, start, self.pos)
                }
            }

            // ── Global variables ─────────────────────────────
            b'$' => self.lex_global_var(start),

            // ── Identifiers & Keywords ───────────────────────
            b'a'..=b'z' | b'_' => self.lex_identifier(start),
            b'A'..=b'Z' => self.lex_constant(start),

            // ── Operators & Delimiters ───────────────────────
            b'+' => {
                if self.peek() == b'=' {
                    self.advance();
                    self.make_token(TokenKind::PlusEqual, start, self.pos)
                } else {
                    self.make_token(TokenKind::Plus, start, self.pos)
                }
            }
            b'-' => {
                if self.peek() == b'=' {
                    self.advance();
                    self.make_token(TokenKind::MinusEqual, start, self.pos)
                } else if self.peek() == b'>' {
                    self.advance();
                    self.make_token(TokenKind::Arrow, start, self.pos)
                } else {
                    self.make_token(TokenKind::Minus, start, self.pos)
                }
            }
            b'*' => {
                if self.peek() == b'*' {
                    self.advance();
                    if self.peek() == b'=' {
                        self.advance();
                        self.make_token(TokenKind::DoubleStarEqual, start, self.pos)
                    } else {
                        self.make_token(TokenKind::DoubleStar, start, self.pos)
                    }
                } else if self.peek() == b'=' {
                    self.advance();
                    self.make_token(TokenKind::StarEqual, start, self.pos)
                } else {
                    self.make_token(TokenKind::Star, start, self.pos)
                }
            }
            b'/' => {
                if self.peek() == b'=' {
                    self.advance();
                    self.make_token(TokenKind::SlashEqual, start, self.pos)
                } else {
                    self.make_token(TokenKind::Slash, start, self.pos)
                }
            }
            b'%' => {
                if self.peek() == b'=' {
                    self.advance();
                    self.make_token(TokenKind::PercentEqual, start, self.pos)
                } else {
                    self.make_token(TokenKind::Percent, start, self.pos)
                }
            }
            b'=' => {
                if self.peek() == b'=' {
                    self.advance();
                    if self.peek() == b'=' {
                        self.advance();
                        self.make_token(TokenKind::TripleEqual, start, self.pos)
                    } else {
                        self.make_token(TokenKind::EqualEqual, start, self.pos)
                    }
                } else if self.peek() == b'~' {
                    self.advance();
                    self.make_token(TokenKind::Match, start, self.pos)
                } else if self.peek() == b'>' {
                    self.advance();
                    self.make_token(TokenKind::FatArrow, start, self.pos)
                } else if self.pos == 1 || (start > 0 && self.bytes[start - 1] == b'\n') {
                    // Check for =begin block comment
                    if self.remaining().starts_with("begin") {
                        self.lex_block_comment(start)
                    } else {
                        self.make_token(TokenKind::Equal, start, self.pos)
                    }
                } else {
                    self.make_token(TokenKind::Equal, start, self.pos)
                }
            }
            b'!' => {
                if self.peek() == b'=' {
                    self.advance();
                    self.make_token(TokenKind::BangEqual, start, self.pos)
                } else if self.peek() == b'~' {
                    self.advance();
                    self.make_token(TokenKind::NotMatch, start, self.pos)
                } else {
                    self.make_token(TokenKind::Bang, start, self.pos)
                }
            }
            b'<' => {
                if self.peek() == b'=' {
                    self.advance();
                    if self.peek() == b'>' {
                        self.advance();
                        self.make_token(TokenKind::Spaceship, start, self.pos)
                    } else {
                        self.make_token(TokenKind::LessEqual, start, self.pos)
                    }
                } else if self.peek() == b'<' {
                    self.advance();
                    if self.peek() == b'=' {
                        self.advance();
                        self.make_token(TokenKind::LessLessEqual, start, self.pos)
                    } else {
                        self.make_token(TokenKind::LessLess, start, self.pos)
                    }
                } else {
                    self.make_token(TokenKind::Less, start, self.pos)
                }
            }
            b'>' => {
                if self.peek() == b'=' {
                    self.advance();
                    self.make_token(TokenKind::GreaterEqual, start, self.pos)
                } else if self.peek() == b'>' {
                    self.advance();
                    if self.peek() == b'=' {
                        self.advance();
                        self.make_token(TokenKind::GreaterGreaterEqual, start, self.pos)
                    } else {
                        self.make_token(TokenKind::GreaterGreater, start, self.pos)
                    }
                } else {
                    self.make_token(TokenKind::Greater, start, self.pos)
                }
            }
            b'&' => {
                if self.peek() == b'&' {
                    self.advance();
                    if self.peek() == b'=' {
                        self.advance();
                        self.make_token(TokenKind::AmpAmpEqual, start, self.pos)
                    } else {
                        self.make_token(TokenKind::AmpAmp, start, self.pos)
                    }
                } else if self.peek() == b'=' {
                    self.advance();
                    self.make_token(TokenKind::AmpEqual, start, self.pos)
                } else {
                    self.make_token(TokenKind::Amp, start, self.pos)
                }
            }
            b'|' => {
                if self.peek() == b'|' {
                    self.advance();
                    if self.peek() == b'=' {
                        self.advance();
                        self.make_token(TokenKind::PipePipeEqual, start, self.pos)
                    } else {
                        self.make_token(TokenKind::PipePipe, start, self.pos)
                    }
                } else if self.peek() == b'=' {
                    self.advance();
                    self.make_token(TokenKind::PipeEqual, start, self.pos)
                } else {
                    self.make_token(TokenKind::Pipe, start, self.pos)
                }
            }
            b'^' => {
                if self.peek() == b'=' {
                    self.advance();
                    self.make_token(TokenKind::CaretEqual, start, self.pos)
                } else {
                    self.make_token(TokenKind::Caret, start, self.pos)
                }
            }
            b'~' => self.make_token(TokenKind::Tilde, start, self.pos),
            b'.' => {
                if self.peek() == b'.' {
                    self.advance();
                    if self.peek() == b'.' {
                        self.advance();
                        self.make_token(TokenKind::DotDotDot, start, self.pos)
                    } else {
                        self.make_token(TokenKind::DotDot, start, self.pos)
                    }
                } else {
                    self.make_token(TokenKind::Dot, start, self.pos)
                }
            }

            // ── Simple Delimiters ────────────────────────────
            b'(' => self.make_token(TokenKind::LParen, start, self.pos),
            b')' => self.make_token(TokenKind::RParen, start, self.pos),
            b'[' => self.make_token(TokenKind::LBracket, start, self.pos),
            b']' => self.make_token(TokenKind::RBracket, start, self.pos),
            b'{' => self.make_token(TokenKind::LBrace, start, self.pos),
            b'}' => self.make_token(TokenKind::RBrace, start, self.pos),
            b',' => self.make_token(TokenKind::Comma, start, self.pos),
            b';' => self.make_token(TokenKind::Semicolon, start, self.pos),
            b'?' => self.make_token(TokenKind::Question, start, self.pos),

            // ── Unknown ──────────────────────────────────────
            _ => {
                self.diagnostics.push(Diagnostic::error(
                    format!("unexpected character: '{}'", ch as char),
                    SourceSpan::new(start, self.pos),
                ));
                self.make_token(TokenKind::Error, start, self.pos)
            }
        }
    }

    // ═══════════════════════════════════════════════════════════
    //  Lexer Helpers
    // ═══════════════════════════════════════════════════════════

    /// Advance to the next byte and return the current one.
    fn advance(&mut self) -> u8 {
        let ch = self.bytes[self.pos];
        self.pos += 1;
        ch
    }

    /// Peek at the current byte without advancing.
    fn peek(&self) -> u8 {
        if self.is_at_end() { 0 } else { self.bytes[self.pos] }
    }

    /// Peek at the byte after the current one.
    fn peek_next(&self) -> u8 {
        if self.pos + 1 >= self.bytes.len() { 0 } else { self.bytes[self.pos + 1] }
    }

    /// Check if we've reached the end of the source.
    fn is_at_end(&self) -> bool {
        self.pos >= self.bytes.len()
    }

    /// Get the remaining source text from the current position.
    fn remaining(&self) -> &str {
        &self.source[self.pos..]
    }

    /// Skip whitespace (spaces, tabs, carriage returns) but NOT newlines.
    fn skip_whitespace(&mut self) {
        while !self.is_at_end() {
            match self.bytes[self.pos] {
                b' ' | b'\t' | b'\r' => {
                    self.pos += 1;
                }
                _ => break,
            }
        }
    }

    /// Create a token from a span of the source.
    fn make_token(&self, kind: TokenKind, start: usize, end: usize) -> Token {
        let lexeme = &self.source[start..end];
        Token::new(kind, lexeme, SourceSpan::new(start, end))
    }

    // ═══════════════════════════════════════════════════════════
    //  String Lexing
    // ═══════════════════════════════════════════════════════════

    /// Lex a double-quoted string, handling escape sequences.
    fn lex_double_string(&mut self, start: usize) -> Token {
        let mut has_interpolation = false;
        while !self.is_at_end() && self.peek() != b'"' {
            if self.peek() == b'\\' {
                self.advance(); // skip backslash
                if !self.is_at_end() {
                    self.advance(); // skip escaped char
                }
            } else if self.peek() == b'#' && self.peek_next() == b'{' {
                has_interpolation = true;
                // For now, we consume the interpolation as part of the string
                // A full implementation would produce InterpolationStart/End tokens
                self.advance(); // skip #
                self.advance(); // skip {
                let mut depth = 1;
                while !self.is_at_end() && depth > 0 {
                    match self.peek() {
                        b'{' => {
                            depth += 1;
                            self.advance();
                        }
                        b'}' => {
                            depth -= 1;
                            self.advance();
                        }
                        b'\\' => {
                            self.advance();
                            if !self.is_at_end() {
                                self.advance();
                            }
                        }
                        _ => {
                            self.advance();
                        }
                    }
                }
            } else if self.peek() == b'\n' {
                // Multi-line strings are allowed in double quotes
                self.advance();
            } else {
                self.advance();
            }
        }

        if self.is_at_end() {
            self.diagnostics.push(Diagnostic::error(
                "unterminated string literal",
                SourceSpan::new(start, self.pos),
            ));
            self.make_token(TokenKind::Error, start, self.pos)
        } else {
            self.advance(); // closing "
            let _ = has_interpolation; // will be used later for interpolation tokens
            self.make_token(TokenKind::StringDouble, start, self.pos)
        }
    }

    /// Lex a single-quoted string (no interpolation, minimal escapes).
    fn lex_single_string(&mut self, start: usize) -> Token {
        while !self.is_at_end() && self.peek() != b'\'' {
            if self.peek() == b'\\' {
                self.advance(); // skip backslash
                if !self.is_at_end() {
                    self.advance(); // skip escaped char (only \\ and \' are meaningful)
                }
            } else {
                self.advance();
            }
        }

        if self.is_at_end() {
            self.diagnostics.push(Diagnostic::error(
                "unterminated string literal",
                SourceSpan::new(start, self.pos),
            ));
            self.make_token(TokenKind::Error, start, self.pos)
        } else {
            self.advance(); // closing '
            self.make_token(TokenKind::StringSingle, start, self.pos)
        }
    }

    // ═══════════════════════════════════════════════════════════
    //  Number Lexing
    // ═══════════════════════════════════════════════════════════

    /// Lex a number literal (integer or float).
    fn lex_number(&mut self, start: usize) -> Token {
        // Check for prefix: 0x, 0b, 0o
        if self.source.as_bytes()[start] == b'0' && !self.is_at_end() {
            match self.peek() {
                b'x' | b'X' => return self.lex_hex_number(start),
                b'b' | b'B' => return self.lex_binary_number(start),
                b'o' | b'O' => return self.lex_octal_number(start),
                _ => {}
            }
        }

        // Decimal digits (with optional underscores)
        self.consume_decimal_digits();

        // Check for float: decimal point followed by digit
        if self.peek() == b'.' && self.peek_next().is_ascii_digit() {
            self.advance(); // consume '.'
            self.consume_decimal_digits();

            // Exponent
            if self.peek() == b'e' || self.peek() == b'E' {
                self.advance();
                if self.peek() == b'+' || self.peek() == b'-' {
                    self.advance();
                }
                self.consume_decimal_digits();
            }

            return self.make_token(TokenKind::Float, start, self.pos);
        }

        // Exponent on integer makes it a float
        if self.peek() == b'e' || self.peek() == b'E' {
            self.advance();
            if self.peek() == b'+' || self.peek() == b'-' {
                self.advance();
            }
            self.consume_decimal_digits();
            return self.make_token(TokenKind::Float, start, self.pos);
        }

        self.make_token(TokenKind::Integer, start, self.pos)
    }

    fn lex_hex_number(&mut self, start: usize) -> Token {
        self.advance(); // skip 'x'/'X'
        while !self.is_at_end() && (self.peek().is_ascii_hexdigit() || self.peek() == b'_') {
            self.advance();
        }
        self.make_token(TokenKind::Integer, start, self.pos)
    }

    fn lex_binary_number(&mut self, start: usize) -> Token {
        self.advance(); // skip 'b'/'B'
        while !self.is_at_end() && (self.peek() == b'0' || self.peek() == b'1' || self.peek() == b'_')
        {
            self.advance();
        }
        self.make_token(TokenKind::Integer, start, self.pos)
    }

    fn lex_octal_number(&mut self, start: usize) -> Token {
        self.advance(); // skip 'o'/'O'
        while !self.is_at_end() && ((self.peek() >= b'0' && self.peek() <= b'7') || self.peek() == b'_')
        {
            self.advance();
        }
        self.make_token(TokenKind::Integer, start, self.pos)
    }

    fn consume_decimal_digits(&mut self) {
        while !self.is_at_end() && (self.peek().is_ascii_digit() || self.peek() == b'_') {
            self.advance();
        }
    }

    // ═══════════════════════════════════════════════════════════
    //  Identifier / Keyword Lexing
    // ═══════════════════════════════════════════════════════════

    /// Lex an identifier starting with a lowercase letter or underscore.
    /// Resolves keywords automatically.
    fn lex_identifier(&mut self, start: usize) -> Token {
        while !self.is_at_end() && is_ident_continue(self.peek()) {
            self.advance();
        }
        // Ruby allows ? and ! at the end of method names
        if !self.is_at_end() && (self.peek() == b'?' || self.peek() == b'!') {
            self.advance();
        }

        let lexeme = &self.source[start..self.pos];
        let kind = TokenKind::keyword_from_str(lexeme).unwrap_or(TokenKind::Identifier);
        self.make_token(kind, start, self.pos)
    }

    /// Lex an identifier that was started from a prefix (e.g., @, @@).
    fn lex_identifier_from(&mut self, start: usize, kind: TokenKind) -> Token {
        while !self.is_at_end() && is_ident_continue(self.peek()) {
            self.advance();
        }
        self.make_token(kind, start, self.pos)
    }

    /// Lex a constant (starts with uppercase letter).
    fn lex_constant(&mut self, start: usize) -> Token {
        while !self.is_at_end() && is_ident_continue(self.peek()) {
            self.advance();
        }
        self.make_token(TokenKind::Constant, start, self.pos)
    }

    // ═══════════════════════════════════════════════════════════
    //  Symbol Lexing
    // ═══════════════════════════════════════════════════════════

    /// Lex a symbol like `:foo`.
    fn lex_symbol(&mut self, start: usize) -> Token {
        while !self.is_at_end() && is_ident_continue(self.peek()) {
            self.advance();
        }
        // Symbols can also end with ? or !
        if !self.is_at_end() && (self.peek() == b'?' || self.peek() == b'!') {
            self.advance();
        }
        self.make_token(TokenKind::Symbol, start, self.pos)
    }

    /// Lex a quoted symbol like `:"hello world"`.
    fn lex_quoted_symbol(&mut self, start: usize) -> Token {
        while !self.is_at_end() && self.peek() != b'"' {
            if self.peek() == b'\\' {
                self.advance();
                if !self.is_at_end() {
                    self.advance();
                }
            } else {
                self.advance();
            }
        }

        if self.is_at_end() {
            self.diagnostics.push(Diagnostic::error(
                "unterminated quoted symbol",
                SourceSpan::new(start, self.pos),
            ));
            self.make_token(TokenKind::Error, start, self.pos)
        } else {
            self.advance(); // closing "
            self.make_token(TokenKind::Symbol, start, self.pos)
        }
    }

    // ═══════════════════════════════════════════════════════════
    //  Global Variable Lexing
    // ═══════════════════════════════════════════════════════════

    /// Lex a global variable: `$foo`, `$0`, `$!`, `$LOAD_PATH`, etc.
    fn lex_global_var(&mut self, start: usize) -> Token {
        if self.is_at_end() {
            return self.make_token(TokenKind::GlobalVar, start, self.pos);
        }

        // Special single-char globals: $!, $@, $;, $,, $., $/, $\, $*, $<, $>, $$, $?, $~, $&, $`, $', $+, $0-$9
        match self.peek() {
            b'!' | b'@' | b';' | b',' | b'.' | b'/' | b'\\' | b'*' | b'<' | b'>' | b'$'
            | b'?' | b'~' | b'&' | b'`' | b'\'' | b'+' | b'0'..=b'9' | b'_' => {
                self.advance();
                // $_ can be followed by more identifier chars
                if self.bytes[self.pos - 1] == b'_' {
                    while !self.is_at_end() && is_ident_continue(self.peek()) {
                        self.advance();
                    }
                }
            }
            c if c.is_ascii_alphabetic() => {
                while !self.is_at_end() && is_ident_continue(self.peek()) {
                    self.advance();
                }
            }
            _ => {}
        }
        self.make_token(TokenKind::GlobalVar, start, self.pos)
    }

    // ═══════════════════════════════════════════════════════════
    //  Comment Lexing
    // ═══════════════════════════════════════════════════════════

    /// Lex a line comment: `# ...`
    fn lex_line_comment(&mut self, start: usize) -> Token {
        while !self.is_at_end() && self.peek() != b'\n' {
            self.advance();
        }
        self.make_token(TokenKind::Comment, start, self.pos)
    }

    /// Lex a block comment: `=begin ... =end`
    fn lex_block_comment(&mut self, start: usize) -> Token {
        // Skip past "begin"
        for _ in 0..5 {
            if !self.is_at_end() {
                self.advance();
            }
        }
        // Read until we find `=end` at the start of a line
        loop {
            if self.is_at_end() {
                self.diagnostics.push(Diagnostic::error(
                    "unterminated block comment (missing =end)",
                    SourceSpan::new(start, self.pos),
                ));
                break;
            }
            if self.peek() == b'\n' {
                self.advance();
                if self.remaining().starts_with("=end") {
                    // consume "=end"
                    for _ in 0..4 {
                        self.advance();
                    }
                    // consume rest of the line
                    while !self.is_at_end() && self.peek() != b'\n' {
                        self.advance();
                    }
                    break;
                }
            } else {
                self.advance();
            }
        }
        self.make_token(TokenKind::Comment, start, self.pos)
    }
}

/// Check if a byte is valid as a continuation character in an identifier.
fn is_ident_continue(ch: u8) -> bool {
    ch.is_ascii_alphanumeric() || ch == b'_'
}

// ═══════════════════════════════════════════════════════════════
//  Tests
// ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to tokenize and return only non-EOF token kinds.
    fn token_kinds(source: &str) -> Vec<TokenKind> {
        let mut lexer = Lexer::new(source);
        let (tokens, _) = lexer.tokenize();
        tokens.into_iter().filter(|t| t.kind != TokenKind::Eof).map(|t| t.kind).collect()
    }

    /// Helper to tokenize and return (kind, lexeme) pairs (no EOF).
    fn token_pairs(source: &str) -> Vec<(TokenKind, String)> {
        let mut lexer = Lexer::new(source);
        let (tokens, _) = lexer.tokenize();
        tokens
            .into_iter()
            .filter(|t| t.kind != TokenKind::Eof)
            .map(|t| (t.kind, t.lexeme.clone()))
            .collect()
    }

    #[test]
    fn test_empty_source() {
        let mut lexer = Lexer::new("");
        let (tokens, diags) = lexer.tokenize();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::Eof);
        assert!(diags.is_empty());
    }

    #[test]
    fn test_keywords() {
        let kinds = token_kinds("def end class module if else");
        assert_eq!(
            kinds,
            vec![
                TokenKind::KwDef,
                TokenKind::KwEnd,
                TokenKind::KwClass,
                TokenKind::KwModule,
                TokenKind::KwIf,
                TokenKind::KwElse,
            ]
        );
    }

    #[test]
    fn test_identifiers() {
        let pairs = token_pairs("foo bar_baz _private method? save!");
        assert_eq!(pairs[0], (TokenKind::Identifier, "foo".to_string()));
        assert_eq!(pairs[1], (TokenKind::Identifier, "bar_baz".to_string()));
        assert_eq!(pairs[2], (TokenKind::Identifier, "_private".to_string()));
        assert_eq!(pairs[3], (TokenKind::Identifier, "method?".to_string()));
        assert_eq!(pairs[4], (TokenKind::Identifier, "save!".to_string()));
    }

    #[test]
    fn test_constants() {
        let pairs = token_pairs("Foo CONSTANT MyClass");
        assert_eq!(pairs[0], (TokenKind::Constant, "Foo".to_string()));
        assert_eq!(pairs[1], (TokenKind::Constant, "CONSTANT".to_string()));
        assert_eq!(pairs[2], (TokenKind::Constant, "MyClass".to_string()));
    }

    #[test]
    fn test_integers() {
        let pairs = token_pairs("42 0xFF 0b1010 0o17 1_000_000");
        assert_eq!(pairs[0], (TokenKind::Integer, "42".to_string()));
        assert_eq!(pairs[1], (TokenKind::Integer, "0xFF".to_string()));
        assert_eq!(pairs[2], (TokenKind::Integer, "0b1010".to_string()));
        assert_eq!(pairs[3], (TokenKind::Integer, "0o17".to_string()));
        assert_eq!(pairs[4], (TokenKind::Integer, "1_000_000".to_string()));
    }

    #[test]
    fn test_floats() {
        let pairs = token_pairs("3.14 1.0e10 2.5E-3");
        assert_eq!(pairs[0], (TokenKind::Float, "3.14".to_string()));
        assert_eq!(pairs[1], (TokenKind::Float, "1.0e10".to_string()));
        assert_eq!(pairs[2], (TokenKind::Float, "2.5E-3".to_string()));
    }

    #[test]
    fn test_strings() {
        let pairs = token_pairs(r#""hello" 'world'"#);
        assert_eq!(pairs[0], (TokenKind::StringDouble, "\"hello\"".to_string()));
        assert_eq!(pairs[1], (TokenKind::StringSingle, "'world'".to_string()));
    }

    #[test]
    fn test_string_interpolation() {
        let pairs = token_pairs(r#""hello #{name}""#);
        assert_eq!(pairs[0].0, TokenKind::StringDouble);
        assert_eq!(pairs[0].1, "\"hello #{name}\"");
    }

    #[test]
    fn test_symbols() {
        let pairs = token_pairs(":foo :bar? :\"hello world\"");
        assert_eq!(pairs[0], (TokenKind::Symbol, ":foo".to_string()));
        assert_eq!(pairs[1], (TokenKind::Symbol, ":bar?".to_string()));
        assert_eq!(pairs[2], (TokenKind::Symbol, ":\"hello world\"".to_string()));
    }

    #[test]
    fn test_operators() {
        let kinds = token_kinds("+ - * / % ** == != < > <= >= <=>");
        assert_eq!(
            kinds,
            vec![
                TokenKind::Plus,
                TokenKind::Minus,
                TokenKind::Star,
                TokenKind::Slash,
                TokenKind::Percent,
                TokenKind::DoubleStar,
                TokenKind::EqualEqual,
                TokenKind::BangEqual,
                TokenKind::Less,
                TokenKind::Greater,
                TokenKind::LessEqual,
                TokenKind::GreaterEqual,
                TokenKind::Spaceship,
            ]
        );
    }

    #[test]
    fn test_assignment_operators() {
        let kinds = token_kinds("+= -= *= /= %= **= &&= ||=");
        assert_eq!(
            kinds,
            vec![
                TokenKind::PlusEqual,
                TokenKind::MinusEqual,
                TokenKind::StarEqual,
                TokenKind::SlashEqual,
                TokenKind::PercentEqual,
                TokenKind::DoubleStarEqual,
                TokenKind::AmpAmpEqual,
                TokenKind::PipePipeEqual,
            ]
        );
    }

    #[test]
    fn test_delimiters() {
        let kinds = token_kinds("( ) [ ] { } , . :: ; => ->");
        assert_eq!(
            kinds,
            vec![
                TokenKind::LParen,
                TokenKind::RParen,
                TokenKind::LBracket,
                TokenKind::RBracket,
                TokenKind::LBrace,
                TokenKind::RBrace,
                TokenKind::Comma,
                TokenKind::Dot,
                TokenKind::ColonColon,
                TokenKind::Semicolon,
                TokenKind::FatArrow,
                TokenKind::Arrow,
            ]
        );
    }

    #[test]
    fn test_instance_and_class_vars() {
        let pairs = token_pairs("@name @@count");
        assert_eq!(pairs[0], (TokenKind::InstanceVar, "@name".to_string()));
        assert_eq!(pairs[1], (TokenKind::ClassVar, "@@count".to_string()));
    }

    #[test]
    fn test_global_vars() {
        let pairs = token_pairs("$stdout $0 $LOAD_PATH");
        assert_eq!(pairs[0], (TokenKind::GlobalVar, "$stdout".to_string()));
        assert_eq!(pairs[1], (TokenKind::GlobalVar, "$0".to_string()));
        assert_eq!(pairs[2], (TokenKind::GlobalVar, "$LOAD_PATH".to_string()));
    }

    #[test]
    fn test_comments() {
        let pairs = token_pairs("# this is a comment\nfoo");
        assert_eq!(pairs[0].0, TokenKind::Comment);
        assert_eq!(pairs[1].0, TokenKind::Newline);
        assert_eq!(pairs[2], (TokenKind::Identifier, "foo".to_string()));
    }

    #[test]
    fn test_newlines() {
        let kinds = token_kinds("foo\nbar");
        assert_eq!(kinds, vec![TokenKind::Identifier, TokenKind::Newline, TokenKind::Identifier]);
    }

    #[test]
    fn test_ruby_method_definition() {
        let kinds = token_kinds("def greet(name)\n  puts \"Hello #{name}\"\nend");
        assert_eq!(
            kinds,
            vec![
                TokenKind::KwDef,
                TokenKind::Identifier, // greet
                TokenKind::LParen,
                TokenKind::Identifier, // name
                TokenKind::RParen,
                TokenKind::Newline,
                TokenKind::KwPuts,
                TokenKind::StringDouble, // "Hello #{name}"
                TokenKind::Newline,
                TokenKind::KwEnd,
            ]
        );
    }

    #[test]
    fn test_ruby_class_definition() {
        let kinds = token_kinds("class Dog < Animal\n  attr_reader :name\nend");
        assert_eq!(
            kinds,
            vec![
                TokenKind::KwClass,
                TokenKind::Constant,   // Dog
                TokenKind::Less,       // <
                TokenKind::Constant,   // Animal
                TokenKind::Newline,
                TokenKind::KwAttrReader,
                TokenKind::Symbol,     // :name
                TokenKind::Newline,
                TokenKind::KwEnd,
            ]
        );
    }

    #[test]
    fn test_range_operator() {
        let pairs = token_pairs("1..10 1...10");
        assert_eq!(pairs[0], (TokenKind::Integer, "1".to_string()));
        assert_eq!(pairs[1], (TokenKind::DotDot, "..".to_string()));
        assert_eq!(pairs[2], (TokenKind::Integer, "10".to_string()));
        assert_eq!(pairs[3], (TokenKind::Integer, "1".to_string()));
        assert_eq!(pairs[4], (TokenKind::DotDotDot, "...".to_string()));
        assert_eq!(pairs[5], (TokenKind::Integer, "10".to_string()));
    }

    #[test]
    fn test_logical_operators() {
        let kinds = token_kinds("&& || ! and or not");
        assert_eq!(
            kinds,
            vec![
                TokenKind::AmpAmp,
                TokenKind::PipePipe,
                TokenKind::Bang,
                TokenKind::KwAnd,
                TokenKind::KwOr,
                TokenKind::KwNot,
            ]
        );
    }

    #[test]
    fn test_unterminated_string() {
        let mut lexer = Lexer::new("\"hello");
        let (tokens, diags) = lexer.tokenize();
        assert_eq!(tokens[0].kind, TokenKind::Error);
        assert!(!diags.is_empty());
    }
}

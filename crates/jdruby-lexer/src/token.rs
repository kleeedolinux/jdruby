use jdruby_common::SourceSpan;

/// A single token produced by the lexer.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    /// The kind/type of this token.
    pub kind: TokenKind,
    /// The raw text of this token as it appeared in source.
    pub lexeme: String,
    /// The source location of this token.
    pub span: SourceSpan,
}

impl Token {
    /// Create a new token.
    pub fn new(kind: TokenKind, lexeme: impl Into<String>, span: SourceSpan) -> Self {
        Self { kind, lexeme: lexeme.into(), span }
    }
}

impl std::fmt::Display for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}({:?}) @ {}", self.kind, self.lexeme, self.span)
    }
}

/// All possible token types in Ruby.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenKind {
    // ── Literals ──────────────────────────────────────────────
    /// Integer literal: `42`, `0xFF`, `0b1010`, `0o17`, `1_000_000`
    Integer,
    /// Float literal: `3.14`, `1.0e10`, `2.5E-3`
    Float,
    /// Double-quoted string: `"hello"`
    StringDouble,
    /// Single-quoted string: `'hello'`
    StringSingle,
    /// Symbol: `:foo`, `:"complex symbol"`
    Symbol,
    /// Regex literal: `/pattern/flags`
    Regex,

    // ── Identifiers & Names ──────────────────────────────────
    /// Local variable or method name: `foo`, `bar_baz`, `method?`, `save!`
    Identifier,
    /// Constant or class/module name: `Foo`, `CONSTANT`, `MyClass`
    Constant,
    /// Instance variable: `@foo`
    InstanceVar,
    /// Class variable: `@@foo`
    ClassVar,
    /// Global variable: `$foo`, `$0`, `$LOAD_PATH`
    GlobalVar,

    // ── Keywords ─────────────────────────────────────────────
    KwDef,
    KwEnd,
    KwClass,
    KwModule,
    KwIf,
    KwElsif,
    KwElse,
    KwUnless,
    KwCase,
    KwWhen,
    KwIn,
    KwWhile,
    KwUntil,
    KwFor,
    KwDo,
    KwBegin,
    KwRescue,
    KwEnsure,
    KwRaise,
    KwReturn,
    KwYield,
    KwBlock,
    KwSelf_,
    KwSuper,
    KwNil,
    KwTrue,
    KwFalse,
    KwAnd,
    KwOr,
    KwNot,
    KwThen,
    KwRequire,
    KwRequireRelative,
    KwLoad,
    KwInclude,
    KwExtend,
    KwPrepend,
    KwAttrReader,
    KwAttrWriter,
    KwAttrAccessor,
    KwPublic,
    KwPrivate,
    KwProtected,
    KwLambda,
    KwProc,
    KwPuts,
    KwPrint,
    KwP,
    KwAlias,
    KwDefined,
    KwBreak,
    KwNext,
    KwRedo,
    KwRetry,
    KwFreeze,

    // ── Operators ────────────────────────────────────────────
    /// `+`
    Plus,
    /// `-`
    Minus,
    /// `*`
    Star,
    /// `/`
    Slash,
    /// `%`
    Percent,
    /// `**`
    DoubleStar,
    /// `==`
    EqualEqual,
    /// `!=`
    BangEqual,
    /// `<`
    Less,
    /// `>`
    Greater,
    /// `<=`
    LessEqual,
    /// `>=`
    GreaterEqual,
    /// `<=>`
    Spaceship,
    /// `===`
    TripleEqual,
    /// `=~`
    Match,
    /// `!~`
    NotMatch,
    /// `&&`
    AmpAmp,
    /// `||`
    PipePipe,
    /// `!`
    Bang,
    /// `&`
    Amp,
    /// `|`
    Pipe,
    /// `^`
    Caret,
    /// `~`
    Tilde,
    /// `<<`
    LessLess,
    /// `>>`
    GreaterGreater,
    /// `..`
    DotDot,
    /// `...`
    DotDotDot,

    // ── Assignment ───────────────────────────────────────────
    /// `=`
    Equal,
    /// `+=`
    PlusEqual,
    /// `-=`
    MinusEqual,
    /// `*=`
    StarEqual,
    /// `/=`
    SlashEqual,
    /// `%=`
    PercentEqual,
    /// `**=`
    DoubleStarEqual,
    /// `&&=`
    AmpAmpEqual,
    /// `||=`
    PipePipeEqual,
    /// `&=`
    AmpEqual,
    /// `|=`
    PipeEqual,
    /// `^=`
    CaretEqual,
    /// `<<=`
    LessLessEqual,
    /// `>>=`
    GreaterGreaterEqual,

    // ── Delimiters ───────────────────────────────────────────
    /// `(`
    LParen,
    /// `)`
    RParen,
    /// `[`
    LBracket,
    /// `]`
    RBracket,
    /// `{`
    LBrace,
    /// `}`
    RBrace,
    /// `,`
    Comma,
    /// `.`
    Dot,
    /// `::`
    ColonColon,
    /// `;`
    Semicolon,
    /// `=>`
    FatArrow,
    /// `->`
    Arrow,
    /// `?`
    Question,
    /// `:`
    Colon,
    /// `@` (standalone, e.g., in decorators)
    At,

    // ── Special ──────────────────────────────────────────────
    /// A newline that is syntactically significant in Ruby.
    Newline,
    /// `#{ ... }` interpolation start inside a string
    InterpolationStart,
    /// End of interpolation
    InterpolationEnd,
    /// A comment: `# ...` or `=begin ... =end`
    Comment,
    /// End of file
    Eof,
    /// An unrecognized or error token
    Error,
}

impl TokenKind {
    /// Look up a keyword by its string representation.
    /// Returns `None` if the string is not a keyword.
    pub fn keyword_from_str(s: &str) -> Option<TokenKind> {
        match s {
            "def" => Some(Self::KwDef),
            "end" => Some(Self::KwEnd),
            "class" => Some(Self::KwClass),
            "module" => Some(Self::KwModule),
            "if" => Some(Self::KwIf),
            "elsif" => Some(Self::KwElsif),
            "else" => Some(Self::KwElse),
            "unless" => Some(Self::KwUnless),
            "case" => Some(Self::KwCase),
            "when" => Some(Self::KwWhen),
            "in" => Some(Self::KwIn),
            "while" => Some(Self::KwWhile),
            "until" => Some(Self::KwUntil),
            "for" => Some(Self::KwFor),
            "do" => Some(Self::KwDo),
            "begin" => Some(Self::KwBegin),
            "rescue" => Some(Self::KwRescue),
            "ensure" => Some(Self::KwEnsure),
            "raise" => Some(Self::KwRaise),
            "return" => Some(Self::KwReturn),
            "yield" => Some(Self::KwYield),
            "self" => Some(Self::KwSelf_),
            "super" => Some(Self::KwSuper),
            "nil" => Some(Self::KwNil),
            "true" => Some(Self::KwTrue),
            "false" => Some(Self::KwFalse),
            "and" => Some(Self::KwAnd),
            "or" => Some(Self::KwOr),
            "not" => Some(Self::KwNot),
            "then" => Some(Self::KwThen),
            "require" => Some(Self::KwRequire),
            "require_relative" => Some(Self::KwRequireRelative),
            "load" => Some(Self::KwLoad),
            "include" => Some(Self::KwInclude),
            "extend" => Some(Self::KwExtend),
            "prepend" => Some(Self::KwPrepend),
            "attr_reader" => Some(Self::KwAttrReader),
            "attr_writer" => Some(Self::KwAttrWriter),
            "attr_accessor" => Some(Self::KwAttrAccessor),
            "public" => Some(Self::KwPublic),
            "private" => Some(Self::KwPrivate),
            "protected" => Some(Self::KwProtected),
            "lambda" => Some(Self::KwLambda),
            "proc" => Some(Self::KwProc),
            "puts" => Some(Self::KwPuts),
            "print" => Some(Self::KwPrint),
            "p" => Some(Self::KwP),
            "alias" => Some(Self::KwAlias),
            "defined?" => Some(Self::KwDefined),
            "break" => Some(Self::KwBreak),
            "next" => Some(Self::KwNext),
            "redo" => Some(Self::KwRedo),
            "retry" => Some(Self::KwRetry),
            "freeze" => Some(Self::KwFreeze),
            _ => None,
        }
    }

    /// Returns `true` if this token kind is a keyword.
    pub fn is_keyword(&self) -> bool {
        matches!(
            self,
            Self::KwDef
                | Self::KwEnd
                | Self::KwClass
                | Self::KwModule
                | Self::KwIf
                | Self::KwElsif
                | Self::KwElse
                | Self::KwUnless
                | Self::KwCase
                | Self::KwWhen
                | Self::KwIn
                | Self::KwWhile
                | Self::KwUntil
                | Self::KwFor
                | Self::KwDo
                | Self::KwBegin
                | Self::KwRescue
                | Self::KwEnsure
                | Self::KwRaise
                | Self::KwReturn
                | Self::KwYield
                | Self::KwBlock
                | Self::KwSelf_
                | Self::KwSuper
                | Self::KwNil
                | Self::KwTrue
                | Self::KwFalse
                | Self::KwAnd
                | Self::KwOr
                | Self::KwNot
                | Self::KwThen
                | Self::KwRequire
                | Self::KwRequireRelative
                | Self::KwLoad
                | Self::KwInclude
                | Self::KwExtend
                | Self::KwPrepend
                | Self::KwAttrReader
                | Self::KwAttrWriter
                | Self::KwAttrAccessor
                | Self::KwPublic
                | Self::KwPrivate
                | Self::KwProtected
                | Self::KwLambda
                | Self::KwProc
                | Self::KwPuts
                | Self::KwPrint
                | Self::KwP
                | Self::KwAlias
                | Self::KwDefined
                | Self::KwBreak
                | Self::KwNext
                | Self::KwRedo
                | Self::KwRetry
                | Self::KwFreeze
        )
    }

    /// Returns `true` if this token kind is a literal value.
    pub fn is_literal(&self) -> bool {
        matches!(
            self,
            Self::Integer
                | Self::Float
                | Self::StringDouble
                | Self::StringSingle
                | Self::Symbol
                | Self::Regex
        )
    }

    /// Returns `true` if this token kind is an operator.
    pub fn is_operator(&self) -> bool {
        matches!(
            self,
            Self::Plus
                | Self::Minus
                | Self::Star
                | Self::Slash
                | Self::Percent
                | Self::DoubleStar
                | Self::EqualEqual
                | Self::BangEqual
                | Self::Less
                | Self::Greater
                | Self::LessEqual
                | Self::GreaterEqual
                | Self::Spaceship
                | Self::TripleEqual
                | Self::Match
                | Self::NotMatch
                | Self::AmpAmp
                | Self::PipePipe
                | Self::Bang
                | Self::Amp
                | Self::Pipe
                | Self::Caret
                | Self::Tilde
                | Self::LessLess
                | Self::GreaterGreater
                | Self::DotDot
                | Self::DotDotDot
        )
    }
}

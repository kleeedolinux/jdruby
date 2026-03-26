use thiserror::Error;
use crate::Diagnostic;

/// Compilation stage where an error occurred.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompilationStage {
    /// Lexing phase.
    Lexer,
    /// Parsing phase (AST generation).
    Parser,
    /// High-level IR (HIR) generation and lowering.
    Hir,
    /// Mid-level IR (MIR) generation and optimization.
    Mir,
    /// LLVM IR code generation.
    Codegen,
    /// Binary building and linking.
    Build,
    /// JIT compilation.
    Jit,
    /// Runtime execution.
    Runtime,
}

impl std::fmt::Display for CompilationStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Lexer => write!(f, "lexer"),
            Self::Parser => write!(f, "parser"),
            Self::Hir => write!(f, "HIR"),
            Self::Mir => write!(f, "MIR"),
            Self::Codegen => write!(f, "codegen"),
            Self::Build => write!(f, "build"),
            Self::Jit => write!(f, "JIT"),
            Self::Runtime => write!(f, "runtime"),
        }
    }
}

/// Stage-specific error with context.
#[derive(Debug, Clone)]
pub struct StageError {
    /// The compilation stage where the error occurred.
    pub stage: CompilationStage,
    /// The error message.
    pub message: String,
    /// Optional source location.
    pub location: Option<String>,
    /// Additional context (e.g., IR snippet, AST node).
    pub context: Option<String>,
}

impl StageError {
    /// Create a new stage error.
    pub fn new(stage: CompilationStage, message: impl Into<String>) -> Self {
        Self {
            stage,
            message: message.into(),
            location: None,
            context: None,
        }
    }

    /// Add location information.
    pub fn with_location(mut self, location: impl Into<String>) -> Self {
        self.location = Some(location.into());
        self
    }

    /// Add context information (e.g., IR snippet).
    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = Some(context.into());
        self
    }
}

impl std::fmt::Display for StageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.stage, self.message)?;
        if let Some(loc) = &self.location {
            write!(f, " at {}", loc)?;
        }
        Ok(())
    }
}

impl std::error::Error for StageError {}

/// Unified error type for the JDRuby compiler.
#[derive(Debug, Error)]
pub enum JDRubyError {
    /// An I/O error (file not found, permission denied, etc.)
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// A lexer error (unexpected character, unterminated string, etc.)
    #[error("Lexer error at {offset}: {message}")]
    Lexer { message: String, offset: usize },

    /// A parser error (unexpected token, missing delimiter, etc.)
    #[error("Parse error: {message}")]
    Parse { message: String },

    /// A semantic error (undefined variable, type mismatch, etc.)
    #[error("Semantic error: {message}")]
    Semantic { message: String },

    /// A code generation error.
    #[error("Codegen error: {message}")]
    Codegen { message: String },

    /// A build/link error.
    #[error("Build error: {message}")]
    Build { message: String },

    /// A runtime error.
    #[error("Runtime error: {message}")]
    Runtime { message: String },

    /// Multiple errors collected during compilation.
    #[error("Compilation failed with {0} error(s)")]
    Multiple(usize),

    /// A stage-specific error with full context.
    #[error("{0}")]
    Stage(#[from] StageError),

    /// LLVM IR parsing/validation error.
    #[error("LLVM IR error in {function}: {message}\n{ir_snippet}")]
    LlvmIr {
        function: String,
        message: String,
        ir_snippet: String,
    },

    /// MIR validation error.
    #[error("MIR validation error in {function}: {message}")]
    MirValidation {
        function: String,
        message: String,
        instruction: Option<String>,
    },

    /// HIR lowering error.
    #[error("HIR lowering error: {message}")]
    HirLowering {
        message: String,
        node: Option<String>,
    },
}

impl JDRubyError {
    /// Create an LLVM IR error.
    pub fn llvm_ir(function: impl Into<String>, message: impl Into<String>, ir_snippet: impl Into<String>) -> Self {
        Self::LlvmIr {
            function: function.into(),
            message: message.into(),
            ir_snippet: ir_snippet.into(),
        }
    }

    /// Create a MIR validation error.
    pub fn mir_validation(function: impl Into<String>, message: impl Into<String>) -> Self {
        Self::MirValidation {
            function: function.into(),
            message: message.into(),
            instruction: None,
        }
    }

    /// Create a HIR lowering error.
    pub fn hir_lowering(message: impl Into<String>) -> Self {
        Self::HirLowering {
            message: message.into(),
            node: None,
        }
    }

    /// Create a stage error.
    pub fn stage(stage: CompilationStage, message: impl Into<String>) -> Self {
        Self::Stage(StageError::new(stage, message))
    }

    /// Get the compilation stage if this is a stage error.
    pub fn stage_kind(&self) -> Option<CompilationStage> {
        match self {
            Self::Stage(e) => Some(e.stage),
            Self::LlvmIr { .. } => Some(CompilationStage::Codegen),
            Self::MirValidation { .. } => Some(CompilationStage::Mir),
            Self::HirLowering { .. } => Some(CompilationStage::Hir),
            Self::Lexer { .. } => Some(CompilationStage::Lexer),
            Self::Parse { .. } => Some(CompilationStage::Parser),
            Self::Codegen { .. } => Some(CompilationStage::Codegen),
            Self::Build { .. } => Some(CompilationStage::Build),
            Self::Runtime { .. } => Some(CompilationStage::Runtime),
            _ => None,
        }
    }

    /// Returns true if this error has diagnostic information.
    pub fn has_diagnostics(&self) -> bool {
        matches!(self, Self::Stage(_) | Self::LlvmIr { .. } | Self::MirValidation { .. } | Self::HirLowering { .. })
    }
}

/// A type alias for results using `JDRubyError`.
pub type JDRubyResult<T> = Result<T, JDRubyError>;

/// Error reporter that collects and emits errors.
#[derive(Debug, Default)]
pub struct ErrorReporter {
    errors: Vec<JDRubyError>,
    diagnostics: Vec<Diagnostic>,
}

impl ErrorReporter {
    /// Create a new error reporter.
    pub fn new() -> Self {
        Self::default()
    }

    /// Report an error.
    pub fn report(&mut self, error: JDRubyError) {
        self.errors.push(error);
    }

    /// Report a diagnostic.
    pub fn report_diagnostic(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    /// Check if there are any errors.
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty() || self.diagnostics.iter().any(|d| d.is_error())
    }

    /// Get the number of errors.
    pub fn error_count(&self) -> usize {
        let diag_errors = self.diagnostics.iter().filter(|d| d.is_error()).count();
        self.errors.len() + diag_errors
    }

    /// Take all errors and clear the reporter.
    pub fn take_errors(&mut self) -> Vec<JDRubyError> {
        std::mem::take(&mut self.errors)
    }

    /// Take all diagnostics and clear the reporter.
    pub fn take_diagnostics(&mut self) -> Vec<Diagnostic> {
        std::mem::take(&mut self.diagnostics)
    }

    /// Emit all errors to the CLI using eprintln.
    pub fn emit_to_cli(&self) {
        use std::io::Write;

        let stderr = std::io::stderr();
        let mut handle = stderr.lock();

        // Emit structured errors
        for error in &self.errors {
            let _ = writeln!(handle, "\x1b[1;31merror\x1b[0m: {}", error);

            // Emit additional context for specific error types
            match error {
                JDRubyError::LlvmIr { function, ir_snippet, .. } => {
                    let _ = writeln!(handle, "  \x1b[1m-->\x1b[0m in function `{}`", function);
                    let _ = writeln!(handle, "   \x1b[1m|\x1b[0m");
                    for line in ir_snippet.lines().take(5) {
                        let _ = writeln!(handle, "   \x1b[1m|\x1b[0m {}", line);
                    }
                    if ir_snippet.lines().count() > 5 {
                        let _ = writeln!(handle, "   \x1b[1m|\x1b[0m ...");
                    }
                }
                JDRubyError::MirValidation { function, instruction, .. } => {
                    let _ = writeln!(handle, "  \x1b[1m-->\x1b[0m in MIR function `{}`", function);
                    if let Some(inst) = instruction {
                        let _ = writeln!(handle, "   \x1b[1m|\x1b[0m instruction: {}", inst);
                    }
                }
                JDRubyError::Stage(e) if e.context.is_some() => {
                    let _ = writeln!(handle, "   \x1b[1m|\x1b[0m");
                    if let Some(ctx) = &e.context {
                        for line in ctx.lines().take(3) {
                            let _ = writeln!(handle, "   \x1b[1m|\x1b[0m {}", line);
                        }
                    }
                }
                _ => {}
            }
            let _ = writeln!(handle);
        }

        // Emit diagnostics
        for diag in &self.diagnostics {
            let severity = match diag.severity {
                crate::DiagnosticSeverity::Error => "\x1b[1;31merror\x1b[0m",
                crate::DiagnosticSeverity::Warning => "\x1b[1;33mwarning\x1b[0m",
                crate::DiagnosticSeverity::Info => "\x1b[1;34minfo\x1b[0m",
                crate::DiagnosticSeverity::Hint => "\x1b[1;36mhint\x1b[0m",
            };
            let _ = writeln!(handle, "{}: {}", severity, diag.message);
        }

        if self.has_errors() {
            let _ = writeln!(handle, "\x1b[1;31merror\x1b[0m: could not compile due to {} previous error(s)", self.error_count());
        }
    }

    /// Convert all collected errors into a single JDRubyError.
    pub fn into_error(self) -> Option<JDRubyError> {
        let count = self.error_count();
        if count == 0 {
            None
        } else if count == 1 && self.errors.len() == 1 {
            self.errors.into_iter().next()
        } else if count == 1 && !self.diagnostics.is_empty() {
            self.diagnostics.into_iter().find(|d| d.is_error()).map(|d| JDRubyError::Codegen { message: d.message.clone() })
        } else {
            Some(JDRubyError::Multiple(count))
        }
    }
}

/// Helper macro to report stage errors.
#[macro_export]
macro_rules! stage_error {
    ($stage:expr, $msg:expr) => {
        $crate::JDRubyError::stage($stage, $msg)
    };
    ($stage:expr, $msg:expr, $loc:expr) => {
        $crate::JDRubyError::Stage(
            $crate::StageError::new($stage, $msg)
                .with_location($loc)
        )
    };
}

/// Helper macro to report LLVM IR errors.
#[macro_export]
macro_rules! llvm_ir_error {
    ($func:expr, $msg:expr, $ir:expr) => {
        $crate::JDRubyError::llvm_ir($func, $msg, $ir)
    };
}

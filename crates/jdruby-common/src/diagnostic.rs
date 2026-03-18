use crate::SourceSpan;

/// Severity level for compiler diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiagnosticSeverity {
    /// A fatal error — compilation cannot continue.
    Error,
    /// A warning — compilation proceeds but something is suspicious.
    Warning,
    /// An informational note — provides context for another diagnostic.
    Info,
    /// A hint — a suggestion for improvement.
    Hint,
}

impl std::fmt::Display for DiagnosticSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Error => write!(f, "error"),
            Self::Warning => write!(f, "warning"),
            Self::Info => write!(f, "info"),
            Self::Hint => write!(f, "hint"),
        }
    }
}

/// A compiler diagnostic (error, warning, info).
///
/// Diagnostics are collected during compilation and displayed to the user
/// with source location context using `ariadne`.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    /// The severity level.
    pub severity: DiagnosticSeverity,
    /// The primary message describing the issue.
    pub message: String,
    /// The source location where the issue was found.
    pub span: SourceSpan,
    /// Optional labels providing additional context.
    pub labels: Vec<DiagnosticLabel>,
    /// Optional help text suggesting a fix.
    pub help: Option<String>,
}

/// A label attached to a diagnostic, pointing to a specific span.
#[derive(Debug, Clone)]
pub struct DiagnosticLabel {
    /// The span this label points to.
    pub span: SourceSpan,
    /// The message for this label.
    pub message: String,
}

impl Diagnostic {
    /// Create a new error diagnostic.
    pub fn error(message: impl Into<String>, span: SourceSpan) -> Self {
        Self {
            severity: DiagnosticSeverity::Error,
            message: message.into(),
            span,
            labels: Vec::new(),
            help: None,
        }
    }

    /// Create a new warning diagnostic.
    pub fn warning(message: impl Into<String>, span: SourceSpan) -> Self {
        Self {
            severity: DiagnosticSeverity::Warning,
            message: message.into(),
            span,
            labels: Vec::new(),
            help: None,
        }
    }

    /// Add a label to this diagnostic.
    pub fn with_label(mut self, span: SourceSpan, message: impl Into<String>) -> Self {
        self.labels.push(DiagnosticLabel { span, message: message.into() });
        self
    }

    /// Add help text to this diagnostic.
    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }
}

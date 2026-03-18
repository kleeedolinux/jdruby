//! # JDRuby Builder
//!
//! Build orchestrator that coordinates the full compilation pipeline:
//!
//! ```text
//! Source → Lexer → Parser → Semantic → HIR → MIR → LLVM IR → Native Binary
//! ```
//!
//! Also handles:
//! - Multi-file compilation
//! - Dependency resolution (require/require_relative)
//! - Linking with the runtime library
//! - Invoking system linker (clang/gcc)

use jdruby_common::JDRubyError;
use std::path::{Path, PathBuf};

/// Configuration for the build pipeline.
#[derive(Debug, Clone)]
pub struct BuildConfig {
    /// Input source files.
    pub input_files: Vec<PathBuf>,
    /// Output binary path.
    pub output_path: PathBuf,
    /// Optimization level (0-3).
    pub opt_level: u8,
    /// Whether to emit debug info.
    pub debug_info: bool,
    /// Whether to use JIT compilation.
    pub jit: bool,
    /// Additional library paths for linking.
    pub lib_paths: Vec<PathBuf>,
    /// System compiler to use for linking (e.g., "clang", "gcc").
    pub linker: String,
}

impl Default for BuildConfig {
    fn default() -> Self {
        Self {
            input_files: Vec::new(),
            output_path: PathBuf::from("a.out"),
            opt_level: 2,
            debug_info: false,
            jit: false,
            lib_paths: Vec::new(),
            linker: "cc".to_string(),
        }
    }
}

/// The build pipeline orchestrator.
pub struct BuildPipeline {
    config: BuildConfig,
}

impl BuildPipeline {
    /// Create a new build pipeline with the given configuration.
    pub fn new(config: BuildConfig) -> Self {
        Self { config }
    }

    /// Run the full compilation pipeline.
    pub fn build(&self) -> Result<(), JDRubyError> {
        // TODO: Implement full pipeline:
        // 1. Read source files
        // 2. Lex each file
        // 3. Parse tokens into AST
        // 4. Run semantic analysis
        // 5. Lower AST to HIR
        // 6. Optimize HIR
        // 7. Lower HIR to MIR
        // 8. Optimize MIR
        // 9. Generate LLVM IR
        // 10. Run LLVM optimization passes
        // 11. Emit object file
        // 12. Link into final binary
        Err(JDRubyError::Build {
            message: "build pipeline not yet implemented".to_string(),
        })
    }

    /// Lex a single source file and return its tokens.
    pub fn lex_file(&self, path: &Path) -> Result<Vec<jdruby_lexer::Token>, JDRubyError> {
        let source = std::fs::read_to_string(path)?;
        let mut lexer = jdruby_lexer::Lexer::new(&source);
        let (tokens, diagnostics) = lexer.tokenize();

        if diagnostics.iter().any(|d| {
            d.severity == jdruby_common::DiagnosticSeverity::Error
        }) {
            return Err(JDRubyError::Multiple(
                diagnostics.iter().filter(|d| d.severity == jdruby_common::DiagnosticSeverity::Error).count(),
            ));
        }

        Ok(tokens)
    }
}

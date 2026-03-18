//! # JDRuby Builder — Build Pipeline Orchestrator
//!
//! Orchestrates the full compilation pipeline:
//! Source → Lex → Parse → Semantic → HIR → MIR → Codegen → Link

use std::path::PathBuf;
use jdruby_common::{Diagnostic, JDRubyError};

/// Build configuration.
#[derive(Debug, Clone)]
pub struct BuildConfig {
    pub input_files: Vec<PathBuf>,
    pub output_path: PathBuf,
    pub opt_level: u8,
    pub debug_info: bool,
    pub emit_hir: bool,
    pub emit_mir: bool,
    pub emit_llvm_ir: bool,
    pub verbose: bool,
}

impl Default for BuildConfig {
    fn default() -> Self {
        Self {
            input_files: Vec::new(),
            output_path: PathBuf::from("a.out"),
            opt_level: 2,
            debug_info: false,
            emit_hir: false,
            emit_mir: false,
            emit_llvm_ir: false,
            verbose: false,
        }
    }
}

/// Build pipeline result.
#[derive(Debug)]
pub struct BuildResult {
    pub diagnostics: Vec<Diagnostic>,
    pub llvm_ir: Option<String>,
    pub success: bool,
}

/// The full compilation pipeline.
pub struct BuildPipeline {
    config: BuildConfig,
}

impl BuildPipeline {
    pub fn new(config: BuildConfig) -> Self {
        Self { config }
    }

    /// Run the full build pipeline.
    pub fn build(&self) -> Result<(), JDRubyError> {
        let mut all_diagnostics: Vec<Diagnostic> = Vec::new();
        let mut has_errors = false;

        for input in &self.config.input_files {
            if self.config.verbose {
                eprintln!("\x1b[1;36mCompiling\x1b[0m {}", input.display());
            }

            // 1. Read source
            let source = std::fs::read_to_string(input).map_err(|e| {
                JDRubyError::Io(std::io::Error::new(e.kind(), format!("{}: {}", input.display(), e)))
            })?;

            // 2. Lex
            if self.config.verbose { eprintln!("  → Lexing..."); }
            let mut lexer = jdruby_lexer::Lexer::new(&source);
            let (tokens, lex_diags) = lexer.tokenize();
            all_diagnostics.extend(lex_diags.iter().cloned());
            if lex_diags.iter().any(|d| d.is_error()) {
                has_errors = true;
                continue;
            }

            // 3. Parse
            if self.config.verbose { eprintln!("  → Parsing..."); }
            let (program, parse_diags) = jdruby_parser::parse(tokens);
            all_diagnostics.extend(parse_diags.iter().cloned());
            if parse_diags.iter().any(|d| d.is_error()) {
                has_errors = true;
                continue;
            }

            // 4. Semantic Analysis
            if self.config.verbose { eprintln!("  → Semantic analysis..."); }
            let mut analyzer = jdruby_semantic::SemanticAnalyzer::new();
            let sem_diags = analyzer.analyze(&program);
            all_diagnostics.extend(sem_diags.iter().cloned());
            // Semantic warnings don't stop compilation

            // 5. AST → HIR
            if self.config.verbose { eprintln!("  → Lowering to HIR..."); }
            let mut hir_module = jdruby_hir::AstLowering::lower(&program);
            if self.config.emit_hir {
                eprintln!("\n── HIR ──\n{:#?}\n", hir_module);
            }

            // 6. HIR Optimization
            if self.config.verbose { eprintln!("  → Optimizing HIR..."); }
            jdruby_hir::HirOptimizer::optimize(&mut hir_module);

            // 7. HIR → MIR
            if self.config.verbose { eprintln!("  → Lowering to MIR..."); }
            let mut mir_module = jdruby_mir::HirLowering::lower(&hir_module);
            if self.config.emit_mir {
                eprintln!("\n── MIR ──\n{:#?}\n", mir_module);
            }

            // 8. MIR Optimization
            if self.config.verbose { eprintln!("  → Optimizing MIR..."); }
            jdruby_mir::MirOptimizer::optimize(&mut mir_module);

            // 9. Codegen → LLVM IR
            if self.config.verbose { eprintln!("  → Generating LLVM IR..."); }
            let codegen_config = jdruby_codegen::CodegenConfig {
                opt_level: match self.config.opt_level {
                    0 => jdruby_codegen::OptLevel::O0,
                    1 => jdruby_codegen::OptLevel::O1,
                    3 => jdruby_codegen::OptLevel::O3,
                    _ => jdruby_codegen::OptLevel::O2,
                },
                debug_info: self.config.debug_info,
                ..Default::default()
            };
            let mut codegen = jdruby_codegen::CodeGenerator::new(codegen_config);
            match codegen.generate(&mir_module) {
                Ok(ir) => {
                    if self.config.emit_llvm_ir {
                        println!("{}", ir);
                    }
                    if self.config.verbose {
                        eprintln!("  → LLVM IR generated ({} bytes)", ir.len());
                    }
                    // Write .ll file
                    let ll_path = self.config.output_path.with_extension("ll");
                    if let Err(e) = std::fs::write(&ll_path, &ir) {
                        eprintln!("\x1b[1;33mwarning\x1b[0m: could not write {}: {}", ll_path.display(), e);
                    }
                }
                Err(diags) => {
                    all_diagnostics.extend(diags);
                    has_errors = true;
                }
            }
        }

        // Print diagnostics
        if !all_diagnostics.is_empty() {
            for d in &all_diagnostics {
                let prefix = if d.is_error() { "\x1b[1;31merror" }
                             else { "\x1b[1;33mwarning" };
                eprintln!("{}\x1b[0m: {}", prefix, d.message);
            }
        }

        if has_errors {
            Err(JDRubyError::Build { message: format!(
                "compilation failed with {} error(s)",
                all_diagnostics.iter().filter(|d| d.is_error()).count()
            ) })
        } else {
            if self.config.verbose {
                eprintln!("\x1b[1;32m✓\x1b[0m Build complete: {}", self.config.output_path.display());
            }
            Ok(())
        }
    }
}

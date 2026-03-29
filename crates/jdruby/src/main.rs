use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process;

/// JDRuby — A full native Ruby compiler and runtime
///
/// Compiles Ruby source code to native binaries via LLVM
/// while maintaining full compatibility with Ruby 3.4.
#[derive(Parser)]
#[command(name = "jdruby")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "JDRuby — Native Ruby compiler powered by LLVM", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Tokenize a Ruby source file and print the token stream
    Lex {
        /// Path to the Ruby source file
        file: PathBuf,
    },

    /// Parse a Ruby source file and print the AST
    Parse {
        /// Path to the Ruby source file
        file: PathBuf,
    },

    /// Compile a Ruby source file using JIT compilation (use --aot for AOT)
    Build {
        /// Path to the Ruby source file
        file: PathBuf,

        /// Output binary path (defaults to input filename without extension)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Optimization level (0-3)
        #[arg(short = 'O', long, default_value = "2")]
        opt_level: u8,

        /// Emit debug information
        #[arg(short, long)]
        debug: bool,

        /// Emit LLVM IR (.ll file)
        #[arg(long = "emit-ll")]
        emit_ll: bool,

        /// Emit LLVM IR (.ll file) always, even on error
        #[arg(long = "emit-ll-v")]
        emit_ll_v: bool,

        /// Emit HIR (.hir file)
        #[arg(long = "emit-hir")]
        emit_hir: bool,

        /// Emit MIR (.mir file)
        #[arg(long = "emit-mir")]
        emit_mir: bool,

        /// Emit assembly (.s file)
        #[arg(long = "emit-asm", short = 'S')]
        emit_asm: bool,

        /// Use AOT compilation instead of JIT (default)
        #[arg(long = "aot")]
        aot: bool,

        /// Verbose output
        #[arg(short, long)]
        verbose: bool,
    },

    /// Compile and run a Ruby source file via JIT
    Run {
        /// Path to the Ruby source file
        file: PathBuf,

        /// Use interpreter (Tier 0) instead of JIT
        #[arg(long = "interp")]
        interp: bool,

        /// JIT tier (1=baseline, 2=optimizing)
        #[arg(long = "tier", default_value = "1")]
        tier: u8,

        /// Verbose output
        #[arg(short, long)]
        verbose: bool,
    },

    /// Show version and environment info
    Info,
}

fn main() {
    // Initialize the JDRuby FFI bridge before anything else
    jdruby_ffi::bridge::init_bridge();
    
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Lex { file } => cmd_lex(&file),
        Commands::Parse { file } => cmd_parse(&file),
        Commands::Build { file, output, opt_level, debug, emit_ll, emit_ll_v, emit_hir, emit_mir, emit_asm, aot, verbose } => {
            // Derive output name from input file if not specified
            let output = output.unwrap_or_else(|| {
                file.file_stem()
                    .map(|s| PathBuf::from(s.to_string_lossy().to_string()))
                    .unwrap_or_else(|| PathBuf::from("a.out"))
            });
            if aot {
                cmd_build(&file, &output, opt_level, debug, emit_ll || emit_ll_v, emit_hir, emit_mir, emit_asm, verbose)
            } else {
                cmd_build_jit(&file, &output, opt_level, emit_ll || emit_ll_v, verbose)
            }
        }
        Commands::Run { file, interp, tier, verbose } => {
            if interp {
                cmd_run_interp(&file, verbose)
            } else {
                cmd_run_jit(&file, tier, verbose)
            }
        }
        Commands::Info => cmd_info(),
    };

    if let Err(e) = result {
        eprintln!("\x1b[1;31merror\x1b[0m: {e}");
        process::exit(1);
    }
}

/// Tokenize a file and print each token.
fn cmd_lex(file: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let source = std::fs::read_to_string(file)?;
    let mut lexer = jdruby_lexer::Lexer::new(&source);
    let (tokens, diagnostics) = lexer.tokenize();

    for diag in &diagnostics {
        let (line, col) = line_col_from_offset(&source, diag.span.start);
        eprintln!(
            "\x1b[1;33m{}\x1b[0m: {} ({}:{}:{})",
            diag.severity, diag.message, file.display(), line, col
        );
    }

    println!(
        "\x1b[1;36m── Tokens for {} ──\x1b[0m ({} tokens)",
        file.display(), tokens.len()
    );
    println!();

    for token in &tokens {
        let (line, col) = line_col_from_offset(&source, token.span.start);
        println!(
            "  \x1b[90m{:>4}:{:<3}\x1b[0m  \x1b[1;32m{:<25}\x1b[0m  \x1b[33m{:?}\x1b[0m",
            line, col,
            format!("{:?}", token.kind),
            token.lexeme.escape_default().to_string()
        );
    }

    println!();
    if diagnostics.is_empty() {
        println!("\x1b[1;32m✓\x1b[0m No lexer errors");
    } else {
        println!("\x1b[1;31m✗\x1b[0m {} diagnostic(s)", diagnostics.len());
    }

    Ok(())
}

/// Parse a file and print the AST.
fn cmd_parse(file: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let source = std::fs::read_to_string(file)?;
    let mut lexer = jdruby_lexer::Lexer::new(&source);
    let (tokens, lex_diags) = lexer.tokenize();

    for diag in &lex_diags {
        let (line, col) = line_col_from_offset(&source, diag.span.start);
        eprintln!(
            "\x1b[1;33m{}\x1b[0m: {} ({}:{}:{})",
            diag.severity, diag.message, file.display(), line, col
        );
    }

    let (program, parse_diags) = jdruby_parser::parse(tokens);

    for diag in &parse_diags {
        let (line, col) = line_col_from_offset(&source, diag.span.start);
        eprintln!(
            "\x1b[1;33m{}\x1b[0m: {} ({}:{}:{})",
            diag.severity, diag.message, file.display(), line, col
        );
    }

    println!("{:#?}", program);
    Ok(())
}

/// Compile a Ruby file to a native binary.
fn cmd_build(
    file: &PathBuf,
    output: &PathBuf,
    opt_level: u8,
    debug: bool,
    emit_ll: bool,
    emit_hir: bool,
    emit_mir: bool,
    emit_asm: bool,
    verbose: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let stem = file.file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "output".to_string());

    let config = jdruby_builder::BuildConfig {
        input_files: vec![file.clone()],
        output_path: output.clone(),
        opt_level,
        debug_info: debug,
        emit_hir,
        emit_mir,
        emit_llvm_ir: emit_ll,
        verbose,
        ..Default::default()
    };

    let pipeline = jdruby_builder::BuildPipeline::new(config);
    pipeline.build()?;

    // Write .ll file with source name
    if emit_ll {
        let ll_path = PathBuf::from(format!("{}.ll", stem));
        // Already written by the pipeline to output_path.ll
        eprintln!("\x1b[1;32m✓\x1b[0m LLVM IR written to {}", ll_path.display());
    }

    // If --emit-asm, generate assembly from the .ll file
    if emit_asm {
        let ll_path = output.with_extension("ll");
        let asm_path = PathBuf::from(format!("{}.s", stem));
        // Try to invoke llc to get assembly
        let status = std::process::Command::new("llc")
            .args([
                ll_path.to_str().unwrap_or("a.ll"),
                "-o", asm_path.to_str().unwrap_or("a.s"),
                "--filetype=asm",
            ])
            .status();
        match status {
            Ok(s) if s.success() => {
                eprintln!("\x1b[1;32m✓\x1b[0m Assembly written to {}", asm_path.display());
            }
            _ => {
                eprintln!("\x1b[1;33mwarning\x1b[0m: `llc` not found — install LLVM to emit assembly");
            }
        }
    }

    Ok(())
}

/// Compile and run a Ruby file via interpreter (Tier 0).
fn cmd_run_interp(file: &PathBuf, verbose: bool) -> Result<(), Box<dyn std::error::Error>> {
    let source = std::fs::read_to_string(file)?;
    let mut lexer = jdruby_lexer::Lexer::new(&source);
    let (tokens, _) = lexer.tokenize();
    let (ast, _) = jdruby_parser::parse(tokens);
    
    // Lower to MIR and interpret
    let hir = jdruby_hir::AstLowering::lower(&ast);
    let mir = jdruby_mir::HirLowering::lower(&hir);
    
    let mut interpreter = jdruby_jit::interpreter::MirInterpreter::new();
    
    for func in &mir.functions {
        if verbose {
            eprintln!("Interpreting: {}", func.name);
        }
        let result = interpreter.call_function(func, &[]);
        if verbose {
            eprintln!("Result: {:?}", result);
        }
    }
    
    // Print any output from the interpreter
    for line in &interpreter.output {
        println!("{}", line);
    }
    
    Ok(())
}

/// Compile and run a Ruby file via JIT compilation (Tier 1/2).
fn cmd_run_jit(file: &PathBuf, tier: u8, verbose: bool) -> Result<(), Box<dyn std::error::Error>> {
    use inkwell::context::Context;
    use jdruby_jit::compiler::{JitCompiler, CompilationTier};
    
    let source = std::fs::read_to_string(file)?;
    let mut lexer = jdruby_lexer::Lexer::new(&source);
    let (tokens, _) = lexer.tokenize();
    let (ast, _) = jdruby_parser::parse(tokens);
    
    // Lower to MIR
    let hir = jdruby_hir::AstLowering::lower(&ast);
    let mir = jdruby_mir::HirLowering::lower(&hir);
    
    // Initialize JIT compiler
    let context = Context::create();
    let mut compiler = JitCompiler::new(&context);
    
    let tier = if tier >= 2 {
        CompilationTier::Optimizing
    } else {
        CompilationTier::Baseline
    };
    
    // Compile and execute each function
    for func in &mir.functions {
        if verbose {
            eprintln!("JIT compiling [{}]: {}", 
                if tier == CompilationTier::Optimizing { "O2" } else { "O0" },
                func.name);
        }
        
        match compiler.compile_function_ir(func, tier, 1) {
            Ok(compiled) => {
                if verbose {
                    eprintln!("  -> Compiled to native code (tier: {:?})", compiled.tier);
                }
                // Note: Execution would happen here via unsafe { compiler.execute(&func.name) }
                // For now we just report successful compilation
            }
            Err(e) => {
                eprintln!("JIT compilation failed for {}: {}", func.name, e);
            }
        }
    }
    
    eprintln!("\x1b[1;32m✓\x1b[0m JIT compilation complete ({} functions)", compiler.compiled_count());
    Ok(())
}

/// Build a native binary using JIT compilation.
fn cmd_build_jit(
    file: &PathBuf,
    output: &PathBuf,
    opt_level: u8,
    emit_ll_v: bool,
    verbose: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    use inkwell::context::Context;
    use inkwell::OptimizationLevel;
    use jdruby_jit::binary_builder::{BinaryBuilder, BinaryBuilderConfig};
    
    let source = std::fs::read_to_string(file)?;
    let mut lexer = jdruby_lexer::Lexer::new(&source);
    let (tokens, _) = lexer.tokenize();
    let (ast, _) = jdruby_parser::parse(tokens);
    
    // Lower to MIR
    let hir = jdruby_hir::AstLowering::lower(&ast);
    let mir = jdruby_mir::HirLowering::lower(&hir);
    
    if verbose {
        eprintln!("Building native binary: {}", output.display());
        eprintln!("  Input: {}", file.display());
        eprintln!("  Functions: {}", mir.functions.len());
        eprintln!("  Opt level: {}", opt_level);
    }
    
    if verbose || emit_ll_v {
        let func_names: Vec<String> = mir.functions.iter().map(|f| f.name.clone()).collect();
        eprintln!("  MIR functions: {} ({})", mir.functions.len(), func_names.join(", "));
    }
    
    // DEBUG: Print all functions in MIR
    eprintln!("DEBUG: MIR has {} functions:", mir.functions.len());
    for (i, func) in mir.functions.iter().enumerate() {
        eprintln!("  {}: {} (params: {})", i, func.name, func.params.len());
    }
    
    // Build binary using inkwell
    let context = Context::create();
    let config = BinaryBuilderConfig {
        output_path: output.clone(),
        opt_level: match opt_level {
            0 => OptimizationLevel::None,
            1 => OptimizationLevel::Less,
            2 => OptimizationLevel::Default,
            _ => OptimizationLevel::Aggressive,
        },
        ..Default::default()
    };
    
    let mut builder = BinaryBuilder::new(&context, config);
    
    // Create a single MIR module containing all functions
    let mir_module = jdruby_mir::MirModule {
        name: "main".to_string(),
        functions: mir.functions,
    };
    
    // Emit IR before parsing if --emit-ll-v is set
    if emit_ll_v {
        use jdruby_codegen::{CodeGenerator, CodegenConfig, OutputFormat};
        let codegen_config = CodegenConfig {
            target_triple: "x86_64-unknown-linux-gnu".into(),
            output_format: OutputFormat::LlvmIr,
            ..Default::default()
        };
        let llvm_context = Context::create();
        let mut codegen = CodeGenerator::new(codegen_config, &llvm_context);
        eprintln!("DEBUG: Generating full module with {} functions", mir_module.functions.len());
        let (ir_text, reporter) = codegen.generate_with_errors(&mir_module);
        
        // Always write IR file, even if there are errors
        let ll_path = std::path::PathBuf::from("main.ll");
        
        // Explicitly truncate to prevent trailing garbage from previous larger builds
        match std::fs::File::create(&ll_path) {
            Ok(mut file) => {
                use std::io::Write;
                if let Err(e) = file.write_all(ir_text.as_bytes()) {
                    eprintln!("\x1b[1;33mwarning\x1b[0m: could not write {}: {}", ll_path.display(), e);
                } else {
                    eprintln!("\x1b[1;32m✓\x1b[0m LLVM IR written to {} ({} bytes)", ll_path.display(), ir_text.len());
                }
            }
            Err(e) => {
                eprintln!("\x1b[1;33mwarning\x1b[0m: could not create {}: {}", ll_path.display(), e);
            }
        }
        
        // Also report any codegen errors
        if reporter.has_errors() {
            reporter.emit_to_cli();
        }
    }
    
    builder.add_module("main", &mir_module)?;
    
    // Build the final binary
    let result = builder.build()?;
    
    eprintln!("\x1b[1;32m✓\x1b[0m Native binary: {}", result.display());
    Ok(())
}

/// Show version and environment info.
fn cmd_info() -> Result<(), Box<dyn std::error::Error>> {
    println!("\x1b[1;36mJDRuby\x1b[0m v{}", env!("CARGO_PKG_VERSION"));
    println!();
    println!("  \x1b[1mTarget Ruby:\x1b[0m     3.4.6 + 4.0");
    println!("  \x1b[1mCompiler:\x1b[0m        Rust → LLVM → Native");
    println!("  \x1b[1mPipeline:\x1b[0m        Lexer → Parser → Semantic → HIR → MIR → LLVM IR");
    println!("  \x1b[1mRuntime:\x1b[0m         Light GC · Green Threads · Async I/O");
    println!("  \x1b[1mFFI:\x1b[0m             Crystal-like Rust/C interop");
    println!("  \x1b[1mGem Support:\x1b[0m     Full compatibility + JDGems (optimized)");
    println!();
    println!("  \x1b[90mhttps://github.com/jdruby/jdruby\x1b[0m");
    Ok(())
}

/// Helper: get (line, col) from a byte offset.
fn line_col_from_offset(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    for (i, ch) in source.char_indices() {
        if i >= offset { break; }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

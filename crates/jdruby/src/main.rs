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

    /// Compile a Ruby source file to a native binary
    Build {
        /// Path to the Ruby source file
        file: PathBuf,

        /// Output binary path
        #[arg(short, long, default_value = "a.out")]
        output: PathBuf,

        /// Optimization level (0-3)
        #[arg(short = 'O', long, default_value = "2")]
        opt_level: u8,

        /// Emit debug information
        #[arg(short, long)]
        debug: bool,
    },

    /// Compile and run a Ruby source file
    Run {
        /// Path to the Ruby source file
        file: PathBuf,
    },

    /// Show version and environment info
    Info,
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Lex { file } => cmd_lex(&file),
        Commands::Parse { file } => cmd_parse(&file),
        Commands::Build { file, output, opt_level, debug } => {
            cmd_build(&file, &output, opt_level, debug)
        }
        Commands::Run { file } => cmd_run(&file),
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

    // Print diagnostics
    for diag in &diagnostics {
        let (line, col) = line_col_from_offset(&source, diag.span.start);
        eprintln!(
            "\x1b[1;33m{}\x1b[0m: {} ({}:{}:{})",
            diag.severity,
            diag.message,
            file.display(),
            line,
            col
        );
    }

    // Print tokens
    println!(
        "\x1b[1;36m── Tokens for {} ──\x1b[0m ({} tokens)",
        file.display(),
        tokens.len()
    );
    println!();

    for token in &tokens {
        let (line, col) = line_col_from_offset(&source, token.span.start);
        println!(
            "  \x1b[90m{:>4}:{:<3}\x1b[0m  \x1b[1;32m{:<25}\x1b[0m  \x1b[33m{:?}\x1b[0m",
            line,
            col,
            format!("{:?}", token.kind),
            token.lexeme.escape_default().to_string()
        );
    }

    println!();
    if diagnostics.is_empty() {
        println!("\x1b[1;32m✓\x1b[0m No lexer errors");
    } else {
        println!(
            "\x1b[1;31m✗\x1b[0m {} diagnostic(s)",
            diagnostics.len()
        );
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
) -> Result<(), Box<dyn std::error::Error>> {
    let config = jdruby_builder::BuildConfig {
        input_files: vec![file.clone()],
        output_path: output.clone(),
        opt_level,
        debug_info: debug,
        ..Default::default()
    };

    let pipeline = jdruby_builder::BuildPipeline::new(config);
    pipeline.build().map_err(|e| e.into())
}

/// Compile and run a Ruby file.
fn cmd_run(file: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    eprintln!(
        "\x1b[1;33mwarning\x1b[0m: `run` command not yet implemented — showing tokens instead"
    );
    cmd_lex(file)
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

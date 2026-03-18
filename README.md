# JDRuby

**A full native Ruby compiler and runtime implemented in Rust.**

JDRuby compiles Ruby source code to native binaries via LLVM while maintaining full compatibility
with Ruby 3.4.6 and preparing for Ruby 4.0. It aims to deliver superior performance through
native compilation while keeping the Ruby ecosystem fully accessible.

## Architecture

```
Ruby Source → Lexer → Parser → Semantic Analysis → HIR → MIR → LLVM IR → Native Binary
```

### Compiler Pipeline

| Stage | Crate | Description |
|-------|-------|-------------|
| **Lexer** | `jdruby-lexer` | Hand-written tokenizer for Ruby's context-sensitive syntax |
| **Parser** | `jdruby-parser` | Recursive descent parser producing AST |
| **AST** | `jdruby-ast` | Complete Ruby 3.4 AST node definitions |
| **Semantic** | `jdruby-semantic` | Multi-pass type checking & symbol resolution |
| **HIR** | `jdruby-hir` | High-Level IR for constant folding, dead code elimination, inlining |
| **MIR** | `jdruby-mir` | Mid-Level IR with register-based instructions for LLVM translation |
| **Codegen** | `jdruby-codegen` | LLVM IR generation via `inkwell` |
| **Builder** | `jdruby-builder` | Compilation orchestrator & system linker integration |

### Runtime

| Feature | Description |
|---------|-------------|
| **Light GC** | Low-latency concurrent garbage collector inspired by Go |
| **Green Threads** | M:N cooperative threading with async I/O support |
| **FFI** | Crystal-like syntax for Rust/C interop |
| **Object Model** | Tagged-union values with heap-allocated objects |

## Building

```bash
# Build the compiler
cargo build --release

# Run tests
cargo test

# Tokenize a Ruby file
cargo run -- lex example.rb

# Show compiler info
cargo run -- info
```

## Usage

```bash
# Tokenize and inspect
jdruby lex script.rb

# Parse and show AST
jdruby parse script.rb

# Compile to native binary
jdruby build script.rb -o program

# Compile and run
jdruby run script.rb
```

## Project Structure

```
jdruby/
├── crates/
│   ├── jdruby/             # CLI binary
│   ├── jdruby-common/      # Shared types (spans, diagnostics, errors)
│   ├── jdruby-lexer/       # Ruby tokenizer
│   ├── jdruby-ast/         # AST node definitions
│   ├── jdruby-parser/      # Recursive descent parser
│   ├── jdruby-semantic/    # Semantic analysis
│   ├── jdruby-hir/         # High-Level IR
│   ├── jdruby-mir/         # Mid-Level IR
│   ├── jdruby-codegen/     # LLVM code generation
│   ├── jdruby-runtime/     # Runtime (GC, threads, objects)
│   └── jdruby-builder/     # Build pipeline orchestrator
├── Cargo.toml              # Workspace root
└── README.md
```

## Design Goals

- **Full Ruby compatibility** — syntax, semantics, and API
- **Native performance** — compiled binaries, no interpreter overhead
- **Modular pipeline** — clean separation of compiler phases
- **Modern runtime** — low-latency GC, green threads, async execution
- **Gem ecosystem** — full compatibility with standard gems + optimized JDGems
- **Optional typing** — gradual type annotations for performance hints

## Supported Ruby Versions

| Version | Status |
|---------|--------|
| Ruby 3.4.6 | Target (base) |
| Ruby 4.0 | Target (latest) |

## License

MIT

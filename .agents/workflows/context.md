---
description: Context about the project
---

--- START OF FILE GEMINI.MD ---
# JDRuby (Julia's Dream Ruby) - Context & State

## Project Overview
JDRuby is a state-of-the-art, full native Ruby compiler and runtime implemented in Rust. It strictly adheres to being a **TRUE COMPILER (AOT/Native)** and **not a traditional bytecode interpreter**. It compiles Ruby code directly down to highly optimized native machine code via an advanced Intermediate Representation (IR) and LLVM.

**Target**: Full compatibility with Ruby 3.4.6+, standard gems, and C-extensions, while introducing modern runtime improvements (Light GC, Green Threads, optional static typing).

---

## Architectural Pipeline
The compilation pipeline flows as follows:
1. **Lexer/Parser (`jdruby-parser`)**: Converts Ruby source to an AST. Hand-rolled recursive descent parser supporting complex Ruby syntax (blocks, metaprogramming constructs, postfix modifiers, etc.).
2. **Semantic Analysis (`jdruby-semantic`)**: Resolves symbols, variables, and prepares scopes.
3. **HIR Lowering (`jdruby-hir`)**: Converts AST to High-Level IR (preserves types and variable names).
4. **MIR Lowering (`jdruby-mir`)**: Converts HIR to Mid-Level IR (SSA-like, basic blocks, simplified instructions).
5. **LLVM Codegen (`jdruby-codegen`)**: Translates MIR to LLVM IR (`.ll`). Maps Ruby method calls, class definitions, and object instantiation to native equivalents.
6. **Builder/JIT (`jdruby-builder` / `jdruby-jit`)**: 
   - `build`: Uses `clang` to compile the LLVM IR alongside a C-runtime (`runtime.c`) to produce a final native binary.
   - `run`: Compiles to a temporary native binary, executes it, and cleans it up (pure execution path, no slow interpretation).

---

## Core Components & Crate Layout
*   `jdruby-common`: Shared types, error handling, `SourceSpan`, `Diagnostic`.
*   `jdruby-ast` / `jdruby-hir` / `jdruby-mir`: Tree and IR definitions.
*   `jdruby-runtime`: 
    *   **JDGC**: A concurrent, region-based, incremental, and compacting Garbage Collector modeled specifically for this architecture (Dijkstra Tri-color, Brooks read barriers, TLABs).
    *   **Memory Layout**: Native implementation of `RString`, `RArray`, `RBasic`, etc.
    *   **C Runtime (`runtime.c`)**: The C bridging code that the LLVM IR links against (provides `jdruby_puts`, `jdruby_main`, etc.).
*   `jdruby-ffi`: MRI `ruby.h` ABI compatibility layer. Bridges the compiled Rust memory model with traditional C-extensions using Matz's C-API (`rb_define_method`, `rb_funcall`, etc.).
*   `jdruby-builder`: Orchestrates the CLI. Supports flags like `--emit-ll`, `--emit-hir`, `--emit-mir`, `--emit-asm`.

---

## Recent Milestones & Decisions
1. **Dropped the AST/MIR Interpreter**: JDRuby is now 100% compiled. The `run` command compiles the Ruby script into a temporary native binary and executes it directly, rather than simulating it in Rust.
2. **OOP and Method Dispatch**: Fixed codegen to appropriately mangle class methods (e.g., `ClassName__method_name`) and ensure LLVM properly targets `@jdruby_main`.
3. **Parser Hardening**: Patched infinite loops related to block arguments (`&block`), postfix modifiers (`if`/`unless`), and method calls with attached `do...end` blocks.
4. **FFI Boundary Established**: Created the core layout for `VALUE` mapping to support native C extensions seamlessly.

---

## Current State & Next Steps
*   **Current State**: The compiler successfully parses complex Ruby code (like `a.rb` with modules, classes, and metaprogramming), lowers it through MIR, and generates LLVM IR. 
*   **Immediate Next Steps**:
    1. Expand `runtime.c` to fully support dynamic method dispatch (`rb_funcall` equivalent in C) so that complex OOP constructs and metaprogramming in the generated LLVM IR execute correctly.
    2. Flesh out the native standard library bindings in the FFI layer (e.g., fully linking `Scheduler.new` and `Logger#log` to their native memory representations).
    3. Implement IR-level Inline Caching (IC) for rapid dynamic method resolution.
    4. Refine memory bridging between Rust's `jdruby-runtime` allocator and C-extensions.

## Reminders for the LLM
*   **Do NOT write interpreter logic.** Everything must map to MIR and then LLVM IR.
*   Respect the strict AOT/JIT native binary generation pipeline.
*   When fixing Ruby execution bugs, focus on how `jdruby-codegen` generates the LLVM IR
--- END OF FILE GEMINI.MD ---
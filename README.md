# JDRuby: Julia's Dream Ruby

JDRuby is a high-performance, purely Ahead-of-Time (AOT) and Just-In-Time (JIT) native compiler for the Ruby programming language. Unlike orthodox Ruby runtimes (MRI, YARV, JRuby) that map Ruby syntax into a bytecode virtual machine loop, JDRuby compiles Ruby source directly down to standalone, optimized native assembly (ELF/Mach-O binaries) using an advanced, LLVM-backed compiler infrastructure written natively in Rust.

The architecture emphasizes strict Static Single Assignment (SSA) form generation, aggressive dynamic-to-static speculative unboxing, and 100% C-ABI memory layout synchronization with MRI, permitting legacy C-Extensions (native Gems) to execute seamlessly against compiled routines without an active bytecode interpreter.

## The Compilation Pipeline

JDRuby is highly compartmentalized into distinct Cargo crates, implementing a textbook modern compiler pass architecture:

### 1. Front-end (`jdruby-lexer` and `jdruby-parser`)
- **Lexer**: A handwritten, zero-allocation scanner resolving Ruby 3.4+ grammar complexities directly at the byte boundary. Hand-tuned to support nested string interpolations, robust heredoc resolution, block/keyword symbol disambiguation, and postfix control-flow.
- **Parser**: A recursive-descent algorithm constructing a strongly typed Abstract Syntax Tree (`jdruby-ast`). Retains deep positional heuristics (`SourceSpan`) for precise compiler diagnostics. 

### 2. Semantic Analysis (`jdruby-semantic`)
Performs pre-compilation symbol tracking, scoping constraint resolution, and the first pass of semantic validity checking. Tracks object/metaclass boundaries, module inclusions, local/instance variable lifetimes, and closures to guide HIR generation.

### 3. Lowering to High-Level IR (`jdruby-hir`)
Lowers the semantic AST into a High-Level Intermediate Representation (HIR), decoupling execution logic from Ruby syntactic sugar. 
- Transmutes iterative abstractions (`each`, `map`) and enumerator chains into core loop nodes prior to block inlining.
- Executes Constant Folding, dead-branch elimination, and resolves static variable assignments.

### 4. Medium-Level IR & Static Single Assignment (`jdruby-mir`)
The HIR is flattened into a register-based, strict Static Single Assignment (SSA) MIR.
- **SSA Construction**: Re-maps sequential variable assignments into unique, immutable registers. Unification of branching control flow (e.g., `if`/`else`) terminates in an explicit `Phi` node binding the disparate register edges into the outgoing fundamental block.
- **Dynamic Speculative Unboxing**: Resolves static math loops by conditionally emitting `UnboxValue` instructions prior to arithmetic operations, dropping dynamically typed checks if flow analysis verifies numeric purity.
- **Inline Caching (IC) Reservations**: Dispatches (`MethodCall`) automatically reserve contiguous address locations (`ic_slot`) mapping the invocation receiver payload (e.g., the Class ID) to the final resolved native function pointer. 

### 5. LLVM Backend & Native Binary Compilation (`jdruby-codegen` / `jdruby-builder`)
Traverses the generated MIR and translates it directly into LLVM IR (`.ll`). 
- Generates native data segmentation matching the C-API header declarations precisely. Functions output C-standard calling conventions (`extern "C"`), inherently resolving complex dispatch variations mapping directly onto LLVM instructions.
- The pipeline delegates compilation to `clang` which performs standard industrial optimizations (Vectorization, Loop Invariant Code Motion, Dead Store Elimination) and links identically structured `runtime.c` alongside the generated user code into the final executable.

---

## JDGC: Julia's Dream Garbage Collector (Detailed Technical Architecture)

The bedrock of JDRuby's execution speed is its fully custom, Concurrent Region-Based Evacuating Garbage Collector (`crates/jdruby-runtime/src/gc.rs`), engineered to eliminate Stop-the-World (STW) pauses, allowing tight LLVM loops to execute without periodic locking synchronization.

### The Object Header (`AtomicU64`)
Every heap-allocated structure in JDRuby is prefixed by a strictly contiguous, 64-bit atomic configuration word and a payload size indicator. Because JDRuby enforces `8-byte` alignment (minimum `OBJ_ALIGN`) for all allocations, the lowest 3 bits of any valid memory pointer are guaranteed mathematically to be `000`. 

JDGC usurps these 3 unused lower bits to pack the necessary concurrent phase state directly alongside the Forwarding Pointer representation without causing struct bloat:
```text
┌────────────────────────────────────────────────────────────────────┐
│ Bit 63 ─────────────────────── Bit 3 │ Bit 2  │ Bit 1 │ Bit 0      │
│        Forwarding Pointer (61 bits)  │ Pinned │    Color (2b)      │
└────────────────────────────────────────────────────────────────────┘
```
- **Bits `0-1` (Tri-Color Protocol):** Tracks algorithmic status dynamically. `00` (White) indicates un-marked; `01` (Gray) entails traversal is currently queued; `10` (Black) designates full scanning completion.
- **Bit `2` (Pinned Flag):** Legacy FFI safety boundary. See "Pinning" below.
- **Bits `3-63` (Forwarding Pointer):** Represents the primary target address. When freshly minted, all allocations point *back to themselves*. 

### Tri-Color Concurrent Mark with Dijkstra Insertion Barriers
Marking threads traverse object graphs (`RootSet` -> Stack -> Registers) without pausing the mutator (the compiled user code). To prevent a concurrent mutator from hiding a previously *White* object behind an already *Black* scanned object and subsequently removing the initial reference resulting in premature reaping, JDRuby mandates insertion/deletion memory barriers. 

Before the LLVM IR initiates an assignment (`inst::Store` to an object property array), the instruction stream dynamically embeds a Write Barrier sequence. The barrier examines the color of the target payload via a non-blocking `AtomicU64::load(Ordering::Acquire)`: if the source object dictates a transition violation, the Write Barrier invokes a micro-trampoline forcing the target pointer back to `GRAY` (`01`) and queues it into the GC processing sequence automatically before mutator resumption.

### Thread-Local Allocation Buffers (TLABs) & Region Pointers
Allocating an object like `String.new("foo")` requires zero global synchronization in JDRuby. The `GlobalScope` slices available system memory into chunked `2 MiB` blocks (Regions). Each active computational Green Thread fetches an empty chunk, assigning it as its designated exclusive TLAB.

Allocation drops directly to intrinsic LLVM instructions: JDRuby merely bumps an internal pointer up by `$payload_size` (aligned symmetrically). Since no other thread accesses this current region block simultaneously, the routine entirely sidesteps explicit generic standard-library `malloc()`. Once a `2 MiB` slab overflows exactly, the JDRuby application yields a singular synchronising request acquiring precisely one new distinct empty block via the global allocator matrix.

### Evacuation & The Brooks Read Barrier
To fight memory fragmentation, JDGC utilizes an evacuation scheme (Generational Compaction). The GC detects dense pockets of living objects inside old unmanaged regions (From-Space), computes sequential destination vectors inside fresh distinct areas (To-Space), and copies the raw payloads natively. 

During typical STW environments, pausing application threads allows seamless wholesale pointer rewriting toward the new destination addresses safely. Because JDGC insists on concurrent background operation, memory duplication inherently induces a split-brain paradox for active logic pointers continuously accessed by mutator routines running in parallel.

To safely reconcile parallel reading, JDRuby employs **Brooks Read Barriers**. All LLVM Codegen `getelementptr` instructions dereferencing pointer loads automatically weave an interstitial self-referential hop:
```c
// Native compiled pseudo-logic for accessing instance arrays dynamically:
ObjectHeader* ptr = base_obj_address;
ObjectHeader* actual_ptr = ptr->bits & !0b111; // Strip bits 0,1,2 leaving the fwd_ptr.
return actual_ptr->payload; 
```
If an object rests safely unshuttled, the `Forwarding Pointer` correctly refers precisely back unto itself natively, imposing marginal nano-second cache-line costs. If the GC relocates the active object, the runtime executes an `AtomicU64::compare_exchange_weak` (CAS) on the abandoned original header, rewriting the `Forwarding Pointer` explicitly unto the destination coordinate. Mutators concurrently traversing the outdated node instantly pivot across the explicit hop toward the actively verified exact duplicate automatically.

### FFI Object Pinning
Standard C-Extensions (`.so` objects utilizing `ruby.h`) inevitably assume absolute global control natively over raw pointers during API exchanges. Concurrency imposes catastrophic memory access violation potentials if JDGC decides to evacuate a previously passed raw structure arbitrarily. 

When establishing the `VALUE` transit natively through JDRuby's FFI shim framework (`jdruby_send` / `rb_funcall`), the internal translation layer mandates a bitmask inclusion asserting `Bit 2` (`0b100`) across the atomic `ObjectHeader`. This action registers the struct as `PINNED`. An actively pinned node absolutely rejects evacuation attempts issued by background processes regardless of fragmentation metrics, retaining absolute legacy memory integrity universally.

---

## Memory Model & C-API Compatibility

JDRuby establishes parity with Matz's Ruby runtime (MRI) memory models through identical memory offsets—without dragging over the execution logic.

### Tagged `VALUE` Representation
LLVM functions and the JDRuby core agree that all Ruby variables operate as a 64-bit generic unsigned integer (`VALUE`).
- **Fixnum**: Any VALUE where the lowest bit is `1` evaluates natively as an inline integer. The LLVM bit-shift and addition operation skips memory checks ENTIRELY during iteration.
- **Immediate Values**: `JD_QNIL`, `JD_QTRUE`, and `JD_QFALSE` operate precisely at defined hexadecimal addresses `0x08`, `0x14`, `0x00`. Fast equality evaluations occur directly via register comparisons without branching logic.

### Contiguous Class Models
Instead of referencing deep pointer charts, objects like `Array` are directly instantiated matching structured C-alignments mapping perfectly to hardware caches:
```c
typedef struct {
    uint32_t type_tag;  // Hardcoded to 0x07 = Array
    uint32_t flags;     // Tracks Frozen, Shared bytes
    int64_t  len;       // Current actively held VALUE elements
    VALUE*   ptr;       // Buffer pointing directly to contiguous chunk allocation
    int64_t  capa;      // Buffer capacity limit
} JdArray;
```
When LLVM analyzes `array.length`, the codegen outputs a fundamental `getelementptr` offset (typically adding static bytes), instantly reading the size without dynamic dispatch penalty.

### The Standard `ruby.h` Shim layer (`jdruby-ffi`)
Using pure Rust `#[no_mangle] extern "C"` exports, the `jdruby-ffi` crate emulates Matz's C-extension system perfectly. Calling `.require` on a legacy gem (`.so` or `.bundle` File) natively loads the C-functions alongside JDRuby. When the legacy code executes `rb_funcall` or `rb_define_method`, JDRuby intercepts the signature. The function points are ingested directly into the `MethodTable` resolving through our trampoline, tricking legacy extensions completely.

---

## Immediate Roadmap & Technical Objectives

1. **Fiber Integration & Green Thread Context Switching**  
   Implement un-lockable coroutines integrating directly with the LLVM asynchronous continuation model. JDRuby aims to support millions of concurrent IO-blocked fibers matching native performance.

2. **Advanced Type Guessing Algorithm Expansion**  
   Extend the speculative unboxing functionality inside `jdruby-mir` utilizing a multi-layered Profile-Guided Optimization (PGO) heuristic. Generating Tier 0 IR instructions, logging type outcomes, and JIT-upgrading "hot" loops exclusively.

3. **Complete `JD_METHOD_TABLE` Megamorphic Polymorphism Guarding**  
   Implement a tiered Inline Cache constraint. Call sites will resolve via mono-morphic paths natively. If the target receiver alters dynamically above a configured threshold (usually 4 varying Class IDs), control falls back from inline LLVM `call` instructions to an algorithmic table dispatch to prevent pipeline stalls.

4. **Integration of Variadic ABI Shims**  
   Currently, Rust struggles with mapping unknown `varargs(...)` safely out of standard `libffi`. We intend to implement custom assembly trampoline thunks capturing the `x86_64` system ABI registers (`%rdi`, `%rsi`, etc.) and marshalling standard argument arrays seamlessly into Ruby.

## Usage Commands

Build an executable:
```bash
jdruby build file.rb -o my_compiled_ruby_project
```

Print intermediate analysis layers:
```bash
jdruby build file.rb --emit-hir  ## Prints High-Level Execution Path
jdruby build file.rb --emit-mir  ## Prints Static Single Assignment register allocation block paths
jdruby build file.rb --emit-ll   ## Dumps pure LLVM IR representation mappings
```

; ModuleID = 'main'
source_filename = "main"
target triple = "x86_64-unknown-linux-gnu"

; ── String Constants ──
@.str.0 = private unnamed_addr constant [14 x i8] c"Hello, World!\00", align 1

; ── Runtime Declarations ──
;
; Value representation: all Ruby values are i64 (tagged pointers).
; Integers use tagged fixnum encoding (value << 1 | 1).
; Objects are heap pointers (always even, tag bit = 0).
;

; Value constructors
declare i64 @jdruby_int_new(i64)              ; create tagged integer
declare i64 @jdruby_float_new(double)          ; box a float
declare i64 @jdruby_str_new(i8*, i64)          ; create string from ptr+len
declare i64 @jdruby_sym_intern(i8*)            ; intern a symbol
declare i64 @jdruby_ary_new(i32, ...)          ; create array (argc, elems...)
declare i64 @jdruby_hash_new(i32, ...)         ; create hash (npairs, k, v...)
declare i64 @jdruby_bool(i1)                   ; box boolean

; Well-known constants
@JDRUBY_NIL   = external global i64              ; nil value
@JDRUBY_TRUE  = external global i64              ; true value
@JDRUBY_FALSE = external global i64              ; false value

; Method dispatch
declare i64 @jdruby_send(i64, i8*, i32, ...)   ; receiver, method_name, argc, args...
declare i64 @jdruby_call(i8*, i32, ...)        ; func_name, argc, args...
declare i64 @jdruby_yield(i32, ...)            ; argc, args...
declare i64 @jdruby_block_given()              ; check if block given

; I/O builtins
declare void @jdruby_puts(i64)                 ; puts(value)
declare void @jdruby_print(i64)                ; print(value)
declare i64  @jdruby_p(i64)                    ; p(value) → value
declare void @jdruby_raise(i8*, ...)           ; raise exception

; Arithmetic intrinsics (fast path for tagged integers)
declare i64 @jdruby_int_add(i64, i64)
declare i64 @jdruby_int_sub(i64, i64)
declare i64 @jdruby_int_mul(i64, i64)
declare i64 @jdruby_int_div(i64, i64)
declare i64 @jdruby_int_mod(i64, i64)
declare i64 @jdruby_int_pow(i64, i64)

; Comparison
declare i1  @jdruby_eq(i64, i64)
declare i1  @jdruby_lt(i64, i64)
declare i1  @jdruby_gt(i64, i64)
declare i1  @jdruby_le(i64, i64)
declare i1  @jdruby_ge(i64, i64)
declare i1  @jdruby_truthy(i64)                ; test Ruby truthiness

; Class/module support
declare i64 @jdruby_class_new(i8*, i64)       ; name, superclass
declare void @jdruby_def_method(i64, i8*, i8*) ; class, name, func_ptr
declare i64 @jdruby_const_get(i8*)             ; get constant by name
declare void @jdruby_const_set(i8*, i64)       ; set constant

define i64 @main() {
entry_0:
  %str_ptr_0 = getelementptr inbounds [14 x i8], [14 x i8]* @.str.0, i64 0, i64 0
  %r0 = call i64 @jdruby_str_new(i8* %str_ptr_0, i64 13)
  call void @jdruby_puts(i64 %r0)
  %r1 = load i64, i64* @JDRUBY_NIL, align 8
  ret i64 %r1
}


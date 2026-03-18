; ModuleID = 'main'
source_filename = "main"
target triple = "x86_64-unknown-linux-gnu"

; Runtime declarations
declare i64 @rb_int_new(i64)
declare double @rb_float_new(double)
declare i8* @rb_str_new(i8*, i64)
declare i8* @rb_sym_new(i8*)
declare i64 @rb_ary_new(...)
declare i64 @rb_hash_new(...)
declare i64 @rb_yield(...)
declare void @rb_puts(i64)
declare void @rb_print(i64)
declare void @rb_p(i64)
declare void @rb_raise(i8*, ...)
declare i64 @rb_funcall(i64, i64, i32, ...)

define i64 @main() {
entry_0:
  ; string literal: "Hello, World!s"
  %r0 = call i64 @rb_int_new(i64 0) ; TODO: string alloc
  %r1 = call i64 @puts(i64 %r0)
  ret i64 %r1
}


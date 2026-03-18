; ModuleID = 'main'
source_filename = "main"
target triple = "x86_64-unknown-linux-gnu"

; ── String Constants ──
@.str.0 = private unnamed_addr constant [6 x i8] c"email\00", align 1
@.str.1 = private unnamed_addr constant [17 x i8] c"create_task_type\00", align 1
@.str.2 = private unnamed_addr constant [7 x i8] c"backup\00", align 1
@.str.3 = private unnamed_addr constant [4 x i8] c"new\00", align 1
@.str.4 = private unnamed_addr constant [17 x i8] c"Clean temp files\00", align 1
@.str.5 = private unnamed_addr constant [9 x i8] c"add_task\00", align 1
@.str.6 = private unnamed_addr constant [19 x i8] c"Send welcome email\00", align 1
@.str.7 = private unnamed_addr constant [15 x i8] c"add_email_task\00", align 1
@.str.8 = private unnamed_addr constant [16 x i8] c"Database backup\00", align 1
@.str.9 = private unnamed_addr constant [16 x i8] c"add_backup_task\00", align 1
@.str.10 = private unnamed_addr constant [8 x i8] c"run_all\00", align 1
@.str.11 = private unnamed_addr constant [25 x i8] c"[#{Time.now}] #{message}\00", align 1
@.str.12 = private unnamed_addr constant [24 x i8] c"Starting task: #{@name}\00", align 1
@.str.13 = private unnamed_addr constant [20 x i8] c"Added task: #{name}\00", align 1
@.str.14 = private unnamed_addr constant [21 x i8] c"Running all tasks...\00", align 1
@.str.15 = private unnamed_addr constant [4 x i8] c"run\00", align 1
@.str.16 = private unnamed_addr constant [5 x i8] c"each\00", align 1
@.str.17 = private unnamed_addr constant [21 x i8] c"All tasks completed!\00", align 1
@.str.18 = private unnamed_addr constant [22 x i8] c"add_#{type_name}_task\00", align 1

; ── Global Variables ──
@Scheduler = internal global i64 0, align 8
@scheduler = internal global i64 0, align 8
@name = internal global i64 0, align 8
@block = internal global i64 0, align 8
@_at_action = internal global i64 0, align 8
@Task = internal global i64 0, align 8
@task = internal global i64 0, align 8
@_at_tasks = internal global i64 0, align 8

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
  %sym_ptr_0 = getelementptr inbounds [6 x i8], [6 x i8]* @.str.0, i64 0, i64 0
  %r0 = call i64 @jdruby_sym_intern(i8* %sym_ptr_0)
  %r2 = load i64, i64* @Scheduler, align 8
  %meth_ptr_1 = getelementptr inbounds [17 x i8], [17 x i8]* @.str.1, i64 0, i64 0
  %r1 = call i64 (i64, i8*, i32, ...) @jdruby_send(i64 %r2, i8* %meth_ptr_1, i32 1, i64 %r0)
  %sym_ptr_3 = getelementptr inbounds [7 x i8], [7 x i8]* @.str.2, i64 0, i64 0
  %r3 = call i64 @jdruby_sym_intern(i8* %sym_ptr_3)
  %r5 = load i64, i64* @Scheduler, align 8
  %meth_ptr_4 = getelementptr inbounds [17 x i8], [17 x i8]* @.str.1, i64 0, i64 0
  %r4 = call i64 (i64, i8*, i32, ...) @jdruby_send(i64 %r5, i8* %meth_ptr_4, i32 1, i64 %r3)
  %r7 = load i64, i64* @Scheduler, align 8
  %meth_ptr_6 = getelementptr inbounds [4 x i8], [4 x i8]* @.str.3, i64 0, i64 0
  %r6 = call i64 (i64, i8*, i32, ...) @jdruby_send(i64 %r7, i8* %meth_ptr_6, i32 0)
  store i64 %r6, i64* @scheduler, align 8
  %str_ptr_8 = getelementptr inbounds [17 x i8], [17 x i8]* @.str.4, i64 0, i64 0
  %r8 = call i64 @jdruby_str_new(i8* %str_ptr_8, i64 16)
  %r10 = load i64, i64* @scheduler, align 8
  %meth_ptr_9 = getelementptr inbounds [9 x i8], [9 x i8]* @.str.5, i64 0, i64 0
  %r9 = call i64 (i64, i8*, i32, ...) @jdruby_send(i64 %r10, i8* %meth_ptr_9, i32 1, i64 %r8)
  %str_ptr_11 = getelementptr inbounds [19 x i8], [19 x i8]* @.str.6, i64 0, i64 0
  %r11 = call i64 @jdruby_str_new(i8* %str_ptr_11, i64 18)
  %r13 = load i64, i64* @scheduler, align 8
  %meth_ptr_12 = getelementptr inbounds [15 x i8], [15 x i8]* @.str.7, i64 0, i64 0
  %r12 = call i64 (i64, i8*, i32, ...) @jdruby_send(i64 %r13, i8* %meth_ptr_12, i32 1, i64 %r11)
  %str_ptr_14 = getelementptr inbounds [16 x i8], [16 x i8]* @.str.8, i64 0, i64 0
  %r14 = call i64 @jdruby_str_new(i8* %str_ptr_14, i64 15)
  %r16 = load i64, i64* @scheduler, align 8
  %meth_ptr_15 = getelementptr inbounds [16 x i8], [16 x i8]* @.str.9, i64 0, i64 0
  %r15 = call i64 (i64, i8*, i32, ...) @jdruby_send(i64 %r16, i8* %meth_ptr_15, i32 1, i64 %r14)
  %r18 = load i64, i64* @scheduler, align 8
  %meth_ptr_17 = getelementptr inbounds [8 x i8], [8 x i8]* @.str.10, i64 0, i64 0
  %r17 = call i64 (i64, i8*, i32, ...) @jdruby_send(i64 %r18, i8* %meth_ptr_17, i32 0)
  ret i64 %r17
}

define i64 @Logger__log(i64 %r0) {
entry_0:
  %str_ptr_1 = getelementptr inbounds [25 x i8], [25 x i8]* @.str.11, i64 0, i64 0
  %r1 = call i64 @jdruby_str_new(i8* %str_ptr_1, i64 24)
  call void @jdruby_puts(i64 %r1)
  %r2 = load i64, i64* @JDRUBY_NIL, align 8
  ret i64 %r2
}

define i64 @Task__initialize(i64 %r0, i64 %r1) {
entry_0:
  store i64 %r0, i64* @name, align 8
  store i64 %r1, i64* @block, align 8
  %r2 = load i64, i64* @name, align 8
  %r3 = load i64, i64* @block, align 8
  ret i64 %r3
}

define i64 @Task__run() {
entry_0:
  %str_ptr_0 = getelementptr inbounds [24 x i8], [24 x i8]* @.str.12, i64 0, i64 0
  %r0 = call i64 @jdruby_str_new(i8* %str_ptr_0, i64 23)
  %r1 = call i64 @log(i64 %r0)
  %r2 = load i64, i64* @_at_action, align 8
  %br_cond_2 = call i1 @jdruby_truthy(i64 %r2)
  br i1 %br_cond_2, label %then_0, label %else_1
then_0:
  unreachable
else_1:
  unreachable
}

define i64 @Scheduler__initialize() {
entry_0:
  %r0 = call i64 @rb_ary_new()
  ret i64 %r0
}

define i64 @Scheduler__add_task(i64 %r0, i64 %r1) {
entry_0:
  store i64 %r0, i64* @name, align 8
  store i64 %r1, i64* @block, align 8
  %r2 = load i64, i64* @name, align 8
  %r3 = load i64, i64* @block, align 8
  %r5 = load i64, i64* @Task, align 8
  %meth_ptr_4 = getelementptr inbounds [4 x i8], [4 x i8]* @.str.3, i64 0, i64 0
  %r4 = call i64 (i64, i8*, i32, ...) @jdruby_send(i64 %r5, i8* %meth_ptr_4, i32 2, i64 %r2, i64 %r3)
  store i64 %r4, i64* @task, align 8
  %r6 = load i64, i64* @_at_tasks, align 8
  %r7 = load i64, i64* @task, align 8
  %r8 = shl i64 %r6, %r7
  %str_ptr_9 = getelementptr inbounds [20 x i8], [20 x i8]* @.str.13, i64 0, i64 0
  %r9 = call i64 @jdruby_str_new(i8* %str_ptr_9, i64 19)
  %r10 = call i64 @log(i64 %r9)
  ret i64 %r10
}

define i64 @Scheduler__run_all() {
entry_0:
  %str_ptr_0 = getelementptr inbounds [21 x i8], [21 x i8]* @.str.14, i64 0, i64 0
  %r0 = call i64 @jdruby_str_new(i8* %str_ptr_0, i64 20)
  %r1 = call i64 @log(i64 %r0)
  %sym_ptr_2 = getelementptr inbounds [4 x i8], [4 x i8]* @.str.15, i64 0, i64 0
  %r2 = call i64 @jdruby_sym_intern(i8* %sym_ptr_2)
  %r4 = load i64, i64* @_at_tasks, align 8
  %meth_ptr_3 = getelementptr inbounds [5 x i8], [5 x i8]* @.str.16, i64 0, i64 0
  %r3 = call i64 (i64, i8*, i32, ...) @jdruby_send(i64 %r4, i8* %meth_ptr_3, i32 1, i64 %r2)
  %str_ptr_5 = getelementptr inbounds [21 x i8], [21 x i8]* @.str.17, i64 0, i64 0
  %r5 = call i64 @jdruby_str_new(i8* %str_ptr_5, i64 20)
  %r6 = call i64 @log(i64 %r5)
  ret i64 %r6
}

define i64 @Scheduler__create_task_type(i64 %r0) {
entry_0:
  %str_ptr_1 = getelementptr inbounds [22 x i8], [22 x i8]* @.str.18, i64 0, i64 0
  %r1 = call i64 @jdruby_str_new(i8* %str_ptr_1, i64 21)
  %r2 = call i64 @define_method(i64 %r1)
  ret i64 %r2
}


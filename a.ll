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
  ; symbol: :email
  %r0 = call i64 @rb_int_new(i64 0) ; TODO: symbol intern
  %r2 = load i64, i64* @Scheduler, align 8
  ; method call: .create_task_type
  %r1 = call i64 @rb_funcall(i64 %r2, i64 %r0)
  ; symbol: :backup
  %r3 = call i64 @rb_int_new(i64 0) ; TODO: symbol intern
  %r5 = load i64, i64* @Scheduler, align 8
  ; method call: .create_task_type
  %r4 = call i64 @rb_funcall(i64 %r5, i64 %r3)
  %r7 = load i64, i64* @Scheduler, align 8
  ; method call: .new
  %r6 = call i64 @rb_funcall(i64 %r7)
  store i64 %r6, i64* @scheduler, align 8
  ; string literal: "Clean temp files"
  %r8 = call i64 @rb_int_new(i64 0) ; TODO: string alloc
  %r10 = load i64, i64* @scheduler, align 8
  ; method call: .add_task
  %r9 = call i64 @rb_funcall(i64 %r10, i64 %r8)
  ; string literal: "Send welcome email"
  %r11 = call i64 @rb_int_new(i64 0) ; TODO: string alloc
  %r13 = load i64, i64* @scheduler, align 8
  ; method call: .add_email_task
  %r12 = call i64 @rb_funcall(i64 %r13, i64 %r11)
  ; string literal: "Database backup"
  %r14 = call i64 @rb_int_new(i64 0) ; TODO: string alloc
  %r16 = load i64, i64* @scheduler, align 8
  ; method call: .add_backup_task
  %r15 = call i64 @rb_funcall(i64 %r16, i64 %r14)
  %r18 = load i64, i64* @scheduler, align 8
  ; method call: .run_all
  %r17 = call i64 @rb_funcall(i64 %r18)
  ret i64 %r17
}

define i64 @Logger__log(i64 %r0) {
entry_0:
  ; string literal: "[#{Time.now}] #{message}"
  %r1 = call i64 @rb_int_new(i64 0) ; TODO: string alloc
  %r2 = call i64 @puts(i64 %r1)
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
  ; string literal: "Starting task: #{@name}"
  %r0 = call i64 @rb_int_new(i64 0) ; TODO: string alloc
  %r1 = call i64 @log(i64 %r0)
  %r2 = load i64, i64* @@action, align 8
  %cmp_2 = icmp ne i64 %r2, 0
  br i1 %cmp_2, label %then_0, label %else_1
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
  ; method call: .new
  %r4 = call i64 @rb_funcall(i64 %r5, i64 %r2, i64 %r3)
  store i64 %r4, i64* @task, align 8
  %r6 = load i64, i64* @@tasks, align 8
  %r7 = load i64, i64* @task, align 8
  %r8 = shl i64 %r6, %r7
  ; string literal: "Added task: #{name}"
  %r9 = call i64 @rb_int_new(i64 0) ; TODO: string alloc
  %r10 = call i64 @log(i64 %r9)
  ret i64 %r10
}

define i64 @Scheduler__run_all() {
entry_0:
  ; string literal: "Running all tasks..."
  %r0 = call i64 @rb_int_new(i64 0) ; TODO: string alloc
  %r1 = call i64 @log(i64 %r0)
  ; symbol: :run
  %r2 = call i64 @rb_int_new(i64 0) ; TODO: symbol intern
  %r4 = load i64, i64* @@tasks, align 8
  ; method call: .each
  %r3 = call i64 @rb_funcall(i64 %r4, i64 %r2)
  ; string literal: "All tasks completed!"
  %r5 = call i64 @rb_int_new(i64 0) ; TODO: string alloc
  %r6 = call i64 @log(i64 %r5)
  ret i64 %r6
}

define i64 @Scheduler__create_task_type(i64 %r0) {
entry_0:
  ; string literal: "add_#{type_name}_task"
  %r1 = call i64 @rb_int_new(i64 0) ; TODO: string alloc
  %r2 = call i64 @define_method(i64 %r1)
  ret i64 %r2
}


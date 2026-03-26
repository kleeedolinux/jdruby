; ModuleID = 'main'
source_filename = "main"
target triple = "x86_64-unknown-linux-gnu"

@.str.0 = private unnamed_addr constant [7 x i8] c"Logger\00", align 1
@.str.1 = private unnamed_addr constant [4 x i8] c"log\00", align 1
@.str.2 = private unnamed_addr constant [11 x i8] c"Logger#log\00", align 1
@.str.3 = private unnamed_addr constant [5 x i8] c"Task\00", align 1
@.str.4 = private unnamed_addr constant [8 x i8] c"include\00", align 1
@.str.5 = private unnamed_addr constant [11 x i8] c"initialize\00", align 1
@.str.6 = private unnamed_addr constant [16 x i8] c"Task#initialize\00", align 1
@.str.7 = private unnamed_addr constant [4 x i8] c"run\00", align 1
@.str.8 = private unnamed_addr constant [9 x i8] c"Task#run\00", align 1
@.str.9 = private unnamed_addr constant [10 x i8] c"Scheduler\00", align 1
@.str.10 = private unnamed_addr constant [21 x i8] c"Scheduler#initialize\00", align 1
@.str.11 = private unnamed_addr constant [9 x i8] c"add_task\00", align 1
@.str.12 = private unnamed_addr constant [19 x i8] c"Scheduler#add_task\00", align 1
@.str.13 = private unnamed_addr constant [8 x i8] c"run_all\00", align 1
@.str.14 = private unnamed_addr constant [18 x i8] c"Scheduler#run_all\00", align 1
@.str.15 = private unnamed_addr constant [17 x i8] c"create_task_type\00", align 1
@.str.16 = private unnamed_addr constant [27 x i8] c"Scheduler#create_task_type\00", align 1
@.str.17 = private unnamed_addr constant [6 x i8] c"email\00", align 1
@.str.18 = private unnamed_addr constant [7 x i8] c"backup\00", align 1
@.str.19 = private unnamed_addr constant [4 x i8] c"new\00", align 1
@.str.20 = private unnamed_addr constant [17 x i8] c"Clean temp files\00", align 1
@.str.21 = private unnamed_addr constant [19 x i8] c"Send welcome email\00", align 1
@.str.22 = private unnamed_addr constant [15 x i8] c"add_email_task\00", align 1
@.str.23 = private unnamed_addr constant [16 x i8] c"Database backup\00", align 1
@.str.24 = private unnamed_addr constant [16 x i8] c"add_backup_task\00", align 1
@.str.25 = private unnamed_addr constant [5 x i8] c"to_s\00", align 1
@.str.26 = private unnamed_addr constant [3 x i8] c"] \00", align 1
@.str.27 = private unnamed_addr constant [4 x i8] c"now\00", align 1
@.str.28 = private unnamed_addr constant [2 x i8] c"[\00", align 1
@.str.29 = private unnamed_addr constant [2 x i8] c"+\00", align 1
@.str.30 = private unnamed_addr constant [6 x i8] c"@name\00", align 1
@.str.31 = private unnamed_addr constant [8 x i8] c"@action\00", align 1
@.str.32 = private unnamed_addr constant [16 x i8] c"Starting task: \00", align 1
@.str.33 = private unnamed_addr constant [5 x i8] c"call\00", align 1
@.str.34 = private unnamed_addr constant [16 x i8] c"Finished task: \00", align 1
@.str.35 = private unnamed_addr constant [7 x i8] c"@tasks\00", align 1
@.str.36 = private unnamed_addr constant [3 x i8] c"<<\00", align 1
@.str.37 = private unnamed_addr constant [13 x i8] c"Added task: \00", align 1
@.str.38 = private unnamed_addr constant [21 x i8] c"Running all tasks...\00", align 1
@.str.39 = private unnamed_addr constant [5 x i8] c"each\00", align 1
@.str.40 = private unnamed_addr constant [21 x i8] c"All tasks completed!\00", align 1
@.str.41 = private unnamed_addr constant [6 x i8] c"_task\00", align 1
@.str.42 = private unnamed_addr constant [5 x i8] c"add_\00", align 1
@.str.43 = private unnamed_addr constant [14 x i8] c"define_method\00", align 1

@Logger = internal global i64 0, align 8
@Task = internal global i64 0, align 8
@Scheduler = internal global i64 0, align 8
@Time = internal global i64 0, align 8

@JDRUBY_NIL = external global i64
@JDRUBY_TRUE = external global i64
@JDRUBY_FALSE = external global i64
@Qnil = external global i64
@Qtrue = external global i64
@Qfalse = external global i64

declare i64 @jdruby_int_new(i64)
declare i64 @jdruby_float_new(double)
declare i64 @jdruby_str_new(i8*, i64)
declare i64 @jdruby_sym_intern(i8*)
declare i64 @jdruby_ary_new(i32, ...)
declare i64 @jdruby_hash_new(i32, ...)
declare i64 @jdruby_bool(i1)
declare i64 @jdruby_send(i64, i8*, i32, ...)
declare i64 @jdruby_call(i8*, i32, ...)
declare i64 @jdruby_yield(i32, ...)
declare i1 @jdruby_block_given()
declare void @jdruby_puts(i64)
declare void @jdruby_print(i64)
declare i64 @jdruby_p(i64)
declare void @jdruby_raise(i8*, ...)
declare i64 @jdruby_int_add(i64, i64)
declare i64 @jdruby_int_sub(i64, i64)
declare i64 @jdruby_int_mul(i64, i64)
declare i64 @jdruby_int_div(i64, i64)
declare i64 @jdruby_int_mod(i64, i64)
declare i64 @jdruby_int_pow(i64, i64)
declare i1 @jdruby_eq(i64, i64)
declare i1 @jdruby_lt(i64, i64)
declare i1 @jdruby_gt(i64, i64)
declare i1 @jdruby_le(i64, i64)
declare i1 @jdruby_ge(i64, i64)
declare i1 @jdruby_truthy(i64)
declare i64 @jdruby_class_new(i8*, i64)
declare void @jdruby_def_method(i64, i8*, i8*)
declare i64 @jdruby_const_get(i8*)
declare void @jdruby_const_set(i8*, i64)
declare i64 @jdruby_ivar_get(i64, i8*)
declare void @jdruby_ivar_set(i64, i8*, i64)
declare i64 @rb_int_new(i64)
declare i64 @rb_str_new(i8*, i64)
declare i64 @rb_ary_new()
declare i64 @rb_hash_new()
declare i64 @rb_intern(i8*)
declare i64 @rb_funcallv(i64, i64, i32, i64*)
declare i64 @rb_define_class(i8*, i64)
declare void @rb_define_method(i64, i8*, i64, i32)
declare i64 @rb_iv_get(i64, i8*)
declare i64 @rb_iv_set(i64, i8*, i64)
declare i64 @rb_const_get(i64, i64)
declare void @rb_const_set(i64, i64, i64)
declare void @rb_gc_mark(i64)
define i64 @main() {
entry:
  %local_scheduler = alloca i64, align 8
  br label %entry_0

entry_0:
  %cls_name_0 = getelementptr inbounds [7 x i8], [7 x i8]* @.str.0, i64 0, i64 0
  %sc_val_0 = load i64, i64* @JDRUBY_NIL, align 8
  %r0 = call i64 @jdruby_class_new(i8* %cls_name_0, i64 %sc_val_0)
  store i64 %r0, i64* @Logger, align 8
  %def_meth_log_Logger__log = getelementptr inbounds [4 x i8], [4 x i8]* @.str.1, i64 0, i64 0
  %def_func_log_Logger__log = getelementptr inbounds [11 x i8], [11 x i8]* @.str.2, i64 0, i64 0
  call void @jdruby_def_method(i64 %r0, i8* %def_meth_log_Logger__log, i8* %def_func_log_Logger__log)
  %cls_name_1 = getelementptr inbounds [5 x i8], [5 x i8]* @.str.3, i64 0, i64 0
  %sc_val_1 = load i64, i64* @JDRUBY_NIL, align 8
  %r1 = call i64 @jdruby_class_new(i8* %cls_name_1, i64 %sc_val_1)
  store i64 %r1, i64* @Task, align 8
  %inc_mod_Logger = getelementptr inbounds [7 x i8], [7 x i8]* @.str.0, i64 0, i64 0
  %inc_mod_val_Logger = call i64 @jdruby_const_get(i8* %inc_mod_Logger)
  %inc_name_Logger = getelementptr inbounds [8 x i8], [8 x i8]* @.str.4, i64 0, i64 0
  call i64 (i64, i8*, i32, ...) @jdruby_send(i64 %r1, i8* %inc_name_Logger, i32 1, i64 %inc_mod_val_Logger)
  %def_meth_initialize_Task__initialize = getelementptr inbounds [11 x i8], [11 x i8]* @.str.5, i64 0, i64 0
  %def_func_initialize_Task__initialize = getelementptr inbounds [16 x i8], [16 x i8]* @.str.6, i64 0, i64 0
  call void @jdruby_def_method(i64 %r1, i8* %def_meth_initialize_Task__initialize, i8* %def_func_initialize_Task__initialize)
  %def_meth_run_Task__run = getelementptr inbounds [4 x i8], [4 x i8]* @.str.7, i64 0, i64 0
  %def_func_run_Task__run = getelementptr inbounds [9 x i8], [9 x i8]* @.str.8, i64 0, i64 0
  call void @jdruby_def_method(i64 %r1, i8* %def_meth_run_Task__run, i8* %def_func_run_Task__run)
  %cls_name_2 = getelementptr inbounds [10 x i8], [10 x i8]* @.str.9, i64 0, i64 0
  %sc_val_2 = load i64, i64* @JDRUBY_NIL, align 8
  %r2 = call i64 @jdruby_class_new(i8* %cls_name_2, i64 %sc_val_2)
  store i64 %r2, i64* @Scheduler, align 8
  %inc_mod_Logger = getelementptr inbounds [7 x i8], [7 x i8]* @.str.0, i64 0, i64 0
  %inc_mod_val_Logger = call i64 @jdruby_const_get(i8* %inc_mod_Logger)
  %inc_name_Logger = getelementptr inbounds [8 x i8], [8 x i8]* @.str.4, i64 0, i64 0
  call i64 (i64, i8*, i32, ...) @jdruby_send(i64 %r2, i8* %inc_name_Logger, i32 1, i64 %inc_mod_val_Logger)
  %def_meth_initialize_Scheduler__initialize = getelementptr inbounds [11 x i8], [11 x i8]* @.str.5, i64 0, i64 0
  %def_func_initialize_Scheduler__initialize = getelementptr inbounds [21 x i8], [21 x i8]* @.str.10, i64 0, i64 0
  call void @jdruby_def_method(i64 %r2, i8* %def_meth_initialize_Scheduler__initialize, i8* %def_func_initialize_Scheduler__initialize)
  %def_meth_add_task_Scheduler__add_task = getelementptr inbounds [9 x i8], [9 x i8]* @.str.11, i64 0, i64 0
  %def_func_add_task_Scheduler__add_task = getelementptr inbounds [19 x i8], [19 x i8]* @.str.12, i64 0, i64 0
  call void @jdruby_def_method(i64 %r2, i8* %def_meth_add_task_Scheduler__add_task, i8* %def_func_add_task_Scheduler__add_task)
  %def_meth_run_all_Scheduler__run_all = getelementptr inbounds [8 x i8], [8 x i8]* @.str.13, i64 0, i64 0
  %def_func_run_all_Scheduler__run_all = getelementptr inbounds [18 x i8], [18 x i8]* @.str.14, i64 0, i64 0
  call void @jdruby_def_method(i64 %r2, i8* %def_meth_run_all_Scheduler__run_all, i8* %def_func_run_all_Scheduler__run_all)
  %def_meth_create_task_type_Scheduler__create_task_type = getelementptr inbounds [17 x i8], [17 x i8]* @.str.15, i64 0, i64 0
  %def_func_create_task_type_Scheduler__create_task_type = getelementptr inbounds [27 x i8], [27 x i8]* @.str.16, i64 0, i64 0
  call void @jdruby_def_method(i64 %r2, i8* %def_meth_create_task_type_Scheduler__create_task_type, i8* %def_func_create_task_type_Scheduler__create_task_type)
  %sym_ptr_3 = getelementptr inbounds [6 x i8], [6 x i8]* @.str.17, i64 0, i64 0
  %r3 = call i64 @jdruby_sym_intern(i8* %sym_ptr_3)
  %const_ptr_5 = getelementptr inbounds [10 x i8], [10 x i8]* @.str.9, i64 0, i64 0
  %r5 = call i64 @jdruby_const_get(i8* %const_ptr_5)
  %meth_ptr_4 = getelementptr inbounds [17 x i8], [17 x i8]* @.str.15, i64 0, i64 0
  %r4 = call i64 (i64, i8*, i32, ...) @jdruby_send(i64 %r5, i8* %meth_ptr_4, i32 1, i64 %r3)
  %sym_ptr_6 = getelementptr inbounds [7 x i8], [7 x i8]* @.str.18, i64 0, i64 0
  %r6 = call i64 @jdruby_sym_intern(i8* %sym_ptr_6)
  %const_ptr_8 = getelementptr inbounds [10 x i8], [10 x i8]* @.str.9, i64 0, i64 0
  %r8 = call i64 @jdruby_const_get(i8* %const_ptr_8)
  %meth_ptr_7 = getelementptr inbounds [17 x i8], [17 x i8]* @.str.15, i64 0, i64 0
  %r7 = call i64 (i64, i8*, i32, ...) @jdruby_send(i64 %r8, i8* %meth_ptr_7, i32 1, i64 %r6)
  %const_ptr_10 = getelementptr inbounds [10 x i8], [10 x i8]* @.str.9, i64 0, i64 0
  %r10 = call i64 @jdruby_const_get(i8* %const_ptr_10)
  %meth_ptr_9 = getelementptr inbounds [4 x i8], [4 x i8]* @.str.19, i64 0, i64 0
  %r9 = call i64 (i64, i8*, i32, ...) @jdruby_send(i64 %r10, i8* %meth_ptr_9, i32 0)
  store i64 %r9, i64* %local_scheduler, align 8
  %str_ptr_11 = getelementptr inbounds [17 x i8], [17 x i8]* @.str.20, i64 0, i64 0
  %r11 = call i64 @jdruby_str_new(i8* %str_ptr_11, i64 16)
  %r13 = load i64, i64* %local_scheduler, align 8
  %meth_ptr_12 = getelementptr inbounds [9 x i8], [9 x i8]* @.str.11, i64 0, i64 0
  %r12 = call i64 (i64, i8*, i32, ...) @jdruby_send(i64 %r13, i8* %meth_ptr_12, i32 1, i64 %r11)
  %str_ptr_14 = getelementptr inbounds [19 x i8], [19 x i8]* @.str.21, i64 0, i64 0
  %r14 = call i64 @jdruby_str_new(i8* %str_ptr_14, i64 18)
  %r16 = load i64, i64* %local_scheduler, align 8
  %meth_ptr_15 = getelementptr inbounds [15 x i8], [15 x i8]* @.str.22, i64 0, i64 0
  %r15 = call i64 (i64, i8*, i32, ...) @jdruby_send(i64 %r16, i8* %meth_ptr_15, i32 1, i64 %r14)
  %str_ptr_17 = getelementptr inbounds [16 x i8], [16 x i8]* @.str.23, i64 0, i64 0
  %r17 = call i64 @jdruby_str_new(i8* %str_ptr_17, i64 15)
  %r19 = load i64, i64* %local_scheduler, align 8
  %meth_ptr_18 = getelementptr inbounds [16 x i8], [16 x i8]* @.str.24, i64 0, i64 0
  %r18 = call i64 (i64, i8*, i32, ...) @jdruby_send(i64 %r19, i8* %meth_ptr_18, i32 1, i64 %r17)
  %r21 = load i64, i64* %local_scheduler, align 8
  %meth_ptr_20 = getelementptr inbounds [8 x i8], [8 x i8]* @.str.13, i64 0, i64 0
  %r20 = call i64 (i64, i8*, i32, ...) @jdruby_send(i64 %r21, i8* %meth_ptr_20, i32 0)
  ret i64 %r20
}

define i64 @Logger__log(i64 %r0, i64 %r1) {
entry:
  %local_message = alloca i64, align 8
  %local_self = alloca i64, align 8
  store i64 %r0, i64* %local_self, align 8
  br label %entry_0

entry_0:
  store i64 %r1, i64* %local_message, align 8
  %r3 = load i64, i64* %local_message, align 8
  %meth_ptr_2 = getelementptr inbounds [5 x i8], [5 x i8]* @.str.25, i64 0, i64 0
  %r2 = call i64 (i64, i8*, i32, ...) @jdruby_send(i64 %r3, i8* %meth_ptr_2, i32 0)
  %str_ptr_5 = getelementptr inbounds [3 x i8], [3 x i8]* @.str.26, i64 0, i64 0
  %r5 = call i64 @jdruby_str_new(i8* %str_ptr_5, i64 2)
  %meth_ptr_8 = getelementptr inbounds [4 x i8], [4 x i8]* @.str.27, i64 0, i64 0
  %r8 = call i64 (i64, i8*, i32, ...) @jdruby_send(i64 %r9, i8* %meth_ptr_8, i32 0)
  %meth_ptr_7 = getelementptr inbounds [5 x i8], [5 x i8]* @.str.25, i64 0, i64 0
  %r7 = call i64 (i64, i8*, i32, ...) @jdruby_send(i64 %r8, i8* %meth_ptr_7, i32 0)
  %str_ptr_11 = getelementptr inbounds [2 x i8], [2 x i8]* @.str.28, i64 0, i64 0
  %r11 = call i64 @jdruby_str_new(i8* %str_ptr_11, i64 1)
  %meth_ptr_10 = getelementptr inbounds [2 x i8], [2 x i8]* @.str.29, i64 0, i64 0
  %r10 = call i64 (i64, i8*, i32, ...) @jdruby_send(i64 %r11, i8* %meth_ptr_10, i32 1, i64 %r7)
  %meth_ptr_6 = getelementptr inbounds [2 x i8], [2 x i8]* @.str.29, i64 0, i64 0
  %r6 = call i64 (i64, i8*, i32, ...) @jdruby_send(i64 %r10, i8* %meth_ptr_6, i32 1, i64 %r5)
  %meth_ptr_4 = getelementptr inbounds [2 x i8], [2 x i8]* @.str.29, i64 0, i64 0
  %r4 = call i64 (i64, i8*, i32, ...) @jdruby_send(i64 %r6, i8* %meth_ptr_4, i32 1, i64 %r2)
  call void @jdruby_puts(i64 %r4)
  %r12 = load i64, i64* @JDRUBY_NIL, align 8
  ret i64 %r12
}

define i64 @Task__initialize(i64 %r0, i64 %r1, i64 %r2) {
entry:
  %local_name = alloca i64, align 8
  %local_block = alloca i64, align 8
  %local_self = alloca i64, align 8
  store i64 %r0, i64* %local_self, align 8
  br label %entry_0

entry_0:
  store i64 %r1, i64* %local_name, align 8
  store i64 %r2, i64* %local_block, align 8
  %r3 = load i64, i64* %local_name, align 8
  %self_for_3 = load i64, i64* %local_self, align 8
  %ivar_str_3 = getelementptr inbounds [6 x i8], [6 x i8]* @.str.30, i64 0, i64 0
  call void @jdruby_ivar_set(i64 %self_for_3, i8* %ivar_str_3, i64 %r3)
  %r4 = load i64, i64* %local_block, align 8
  %self_for_4 = load i64, i64* %local_self, align 8
  %ivar_str_4 = getelementptr inbounds [8 x i8], [8 x i8]* @.str.31, i64 0, i64 0
  call void @jdruby_ivar_set(i64 %self_for_4, i8* %ivar_str_4, i64 %r4)
  ret i64 %r4
}

define i64 @Task__run(i64 %r0) {
entry:
  %local_self = alloca i64, align 8
  store i64 %r0, i64* %local_self, align 8
  br label %entry_0

entry_0:
  %self_for_2 = load i64, i64* %local_self, align 8
  %ivar_str_2 = getelementptr inbounds [6 x i8], [6 x i8]* @.str.30, i64 0, i64 0
  %r2 = call i64 @jdruby_ivar_get(i64 %self_for_2, i8* %ivar_str_2)
  %meth_ptr_1 = getelementptr inbounds [5 x i8], [5 x i8]* @.str.25, i64 0, i64 0
  %r1 = call i64 (i64, i8*, i32, ...) @jdruby_send(i64 %r2, i8* %meth_ptr_1, i32 0)
  %str_ptr_4 = getelementptr inbounds [16 x i8], [16 x i8]* @.str.32, i64 0, i64 0
  %r4 = call i64 @jdruby_str_new(i8* %str_ptr_4, i64 15)
  %meth_ptr_3 = getelementptr inbounds [2 x i8], [2 x i8]* @.str.29, i64 0, i64 0
  %r3 = call i64 (i64, i8*, i32, ...) @jdruby_send(i64 %r4, i8* %meth_ptr_3, i32 1, i64 %r1)
  %self_for_call_5 = load i64, i64* %local_self, align 8
  %r5 = call i64 (i64, i8*, i32, ...) @jdruby_send(i64 %self_for_call_5, i8* %meth_ptr_5, i32 1, i64 %r3)
  %self_for_6 = load i64, i64* %local_self, align 8
  %ivar_str_6 = getelementptr inbounds [8 x i8], [8 x i8]* @.str.31, i64 0, i64 0
  %r6 = call i64 @jdruby_ivar_get(i64 %self_for_6, i8* %ivar_str_6)
  %br_cond_6 = call i1 @jdruby_truthy(i64 %r6)
  br i1 %br_cond_6, label %then_0, label %else_1
then_0:
  %self_for_9 = load i64, i64* %local_self, align 8
  %ivar_str_9 = getelementptr inbounds [8 x i8], [8 x i8]* @.str.31, i64 0, i64 0
  %r9 = call i64 @jdruby_ivar_get(i64 %self_for_9, i8* %ivar_str_9)
  %meth_ptr_8 = getelementptr inbounds [5 x i8], [5 x i8]* @.str.33, i64 0, i64 0
  %r8 = call i64 (i64, i8*, i32, ...) @jdruby_send(i64 %r9, i8* %meth_ptr_8, i32 0)
  br label %merge_2
else_1:
  br label %merge_2
merge_2:
  %self_for_13 = load i64, i64* %local_self, align 8
  %ivar_str_13 = getelementptr inbounds [6 x i8], [6 x i8]* @.str.30, i64 0, i64 0
  %r13 = call i64 @jdruby_ivar_get(i64 %self_for_13, i8* %ivar_str_13)
  %meth_ptr_12 = getelementptr inbounds [5 x i8], [5 x i8]* @.str.25, i64 0, i64 0
  %r12 = call i64 (i64, i8*, i32, ...) @jdruby_send(i64 %r13, i8* %meth_ptr_12, i32 0)
  %str_ptr_15 = getelementptr inbounds [16 x i8], [16 x i8]* @.str.34, i64 0, i64 0
  %r15 = call i64 @jdruby_str_new(i8* %str_ptr_15, i64 15)
  %meth_ptr_14 = getelementptr inbounds [2 x i8], [2 x i8]* @.str.29, i64 0, i64 0
  %r14 = call i64 (i64, i8*, i32, ...) @jdruby_send(i64 %r15, i8* %meth_ptr_14, i32 1, i64 %r12)
  %self_for_call_16 = load i64, i64* %local_self, align 8
  %r16 = call i64 (i64, i8*, i32, ...) @jdruby_send(i64 %self_for_call_16, i8* %meth_ptr_16, i32 1, i64 %r14)
  ret i64 %r16
}

define i64 @Scheduler__initialize(i64 %r0) {
entry:
  %local_self = alloca i64, align 8
  store i64 %r0, i64* %local_self, align 8
  br label %entry_0

entry_0:
  %r1 = call i64 (i32, ...) @jdruby_ary_new(i32 0)
  %self_for_1 = load i64, i64* %local_self, align 8
  %ivar_str_1 = getelementptr inbounds [7 x i8], [7 x i8]* @.str.35, i64 0, i64 0
  call void @jdruby_ivar_set(i64 %self_for_1, i8* %ivar_str_1, i64 %r1)
  ret i64 %r1
}

define i64 @Scheduler__add_task(i64 %r0, i64 %r1, i64 %r2) {
entry:
  %local_self = alloca i64, align 8
  %local_task = alloca i64, align 8
  %local_name = alloca i64, align 8
  %local_block = alloca i64, align 8
  store i64 %r0, i64* %local_self, align 8
  br label %entry_0

entry_0:
  store i64 %r1, i64* %local_name, align 8
  store i64 %r2, i64* %local_block, align 8
  %r3 = load i64, i64* %local_name, align 8
  %r4 = load i64, i64* %local_block, align 8
  %const_ptr_6 = getelementptr inbounds [5 x i8], [5 x i8]* @.str.3, i64 0, i64 0
  %r6 = call i64 @jdruby_const_get(i8* %const_ptr_6)
  %meth_ptr_5 = getelementptr inbounds [4 x i8], [4 x i8]* @.str.19, i64 0, i64 0
  %r5 = call i64 (i64, i8*, i32, ...) @jdruby_send(i64 %r6, i8* %meth_ptr_5, i32 2, i64 %r3, i64 %r4)
  store i64 %r5, i64* %local_task, align 8
  %r7 = load i64, i64* %local_task, align 8
  %self_for_9 = load i64, i64* %local_self, align 8
  %ivar_str_9 = getelementptr inbounds [7 x i8], [7 x i8]* @.str.35, i64 0, i64 0
  %r9 = call i64 @jdruby_ivar_get(i64 %self_for_9, i8* %ivar_str_9)
  %meth_ptr_8 = getelementptr inbounds [3 x i8], [3 x i8]* @.str.36, i64 0, i64 0
  %r8 = call i64 (i64, i8*, i32, ...) @jdruby_send(i64 %r9, i8* %meth_ptr_8, i32 1, i64 %r7)
  %r11 = load i64, i64* %local_name, align 8
  %meth_ptr_10 = getelementptr inbounds [5 x i8], [5 x i8]* @.str.25, i64 0, i64 0
  %r10 = call i64 (i64, i8*, i32, ...) @jdruby_send(i64 %r11, i8* %meth_ptr_10, i32 0)
  %str_ptr_13 = getelementptr inbounds [13 x i8], [13 x i8]* @.str.37, i64 0, i64 0
  %r13 = call i64 @jdruby_str_new(i8* %str_ptr_13, i64 12)
  %meth_ptr_12 = getelementptr inbounds [2 x i8], [2 x i8]* @.str.29, i64 0, i64 0
  %r12 = call i64 (i64, i8*, i32, ...) @jdruby_send(i64 %r13, i8* %meth_ptr_12, i32 1, i64 %r10)
  %self_for_call_14 = load i64, i64* %local_self, align 8
  %r14 = call i64 (i64, i8*, i32, ...) @jdruby_send(i64 %self_for_call_14, i8* %meth_ptr_14, i32 1, i64 %r12)
  ret i64 %r14
}

define i64 @Scheduler__run_all(i64 %r0) {
entry:
  %local_self = alloca i64, align 8
  store i64 %r0, i64* %local_self, align 8
  br label %entry_0

entry_0:
  %str_ptr_1 = getelementptr inbounds [21 x i8], [21 x i8]* @.str.38, i64 0, i64 0
  %r1 = call i64 @jdruby_str_new(i8* %str_ptr_1, i64 20)
  %self_for_call_2 = load i64, i64* %local_self, align 8
  %r2 = call i64 (i64, i8*, i32, ...) @jdruby_send(i64 %self_for_call_2, i8* %meth_ptr_2, i32 1, i64 %r1)
  %sym_ptr_3 = getelementptr inbounds [4 x i8], [4 x i8]* @.str.7, i64 0, i64 0
  %r3 = call i64 @jdruby_sym_intern(i8* %sym_ptr_3)
  %self_for_5 = load i64, i64* %local_self, align 8
  %ivar_str_5 = getelementptr inbounds [7 x i8], [7 x i8]* @.str.35, i64 0, i64 0
  %r5 = call i64 @jdruby_ivar_get(i64 %self_for_5, i8* %ivar_str_5)
  %meth_ptr_4 = getelementptr inbounds [5 x i8], [5 x i8]* @.str.39, i64 0, i64 0
  %r4 = call i64 (i64, i8*, i32, ...) @jdruby_send(i64 %r5, i8* %meth_ptr_4, i32 1, i64 %r3)
  %str_ptr_6 = getelementptr inbounds [21 x i8], [21 x i8]* @.str.40, i64 0, i64 0
  %r6 = call i64 @jdruby_str_new(i8* %str_ptr_6, i64 20)
  %self_for_call_7 = load i64, i64* %local_self, align 8
  %r7 = call i64 (i64, i8*, i32, ...) @jdruby_send(i64 %self_for_call_7, i8* %meth_ptr_7, i32 1, i64 %r6)
  ret i64 %r7
}

define i64 @Scheduler__create_task_type(i64 %r0, i64 %r1) {
entry:
  %local_type_name = alloca i64, align 8
  %local_self = alloca i64, align 8
  store i64 %r0, i64* %local_self, align 8
  br label %entry_0

entry_0:
  store i64 %r1, i64* %local_type_name, align 8
  %str_ptr_2 = getelementptr inbounds [6 x i8], [6 x i8]* @.str.41, i64 0, i64 0
  %r2 = call i64 @jdruby_str_new(i8* %str_ptr_2, i64 5)
  %r5 = load i64, i64* %local_type_name, align 8
  %meth_ptr_4 = getelementptr inbounds [5 x i8], [5 x i8]* @.str.25, i64 0, i64 0
  %r4 = call i64 (i64, i8*, i32, ...) @jdruby_send(i64 %r5, i8* %meth_ptr_4, i32 0)
  %str_ptr_7 = getelementptr inbounds [5 x i8], [5 x i8]* @.str.42, i64 0, i64 0
  %r7 = call i64 @jdruby_str_new(i8* %str_ptr_7, i64 4)
  %meth_ptr_6 = getelementptr inbounds [2 x i8], [2 x i8]* @.str.29, i64 0, i64 0
  %r6 = call i64 (i64, i8*, i32, ...) @jdruby_send(i64 %r7, i8* %meth_ptr_6, i32 1, i64 %r4)
  %meth_ptr_3 = getelementptr inbounds [2 x i8], [2 x i8]* @.str.29, i64 0, i64 0
  %r3 = call i64 (i64, i8*, i32, ...) @jdruby_send(i64 %r6, i8* %meth_ptr_3, i32 1, i64 %r2)
  %self_for_call_8 = load i64, i64* %local_self, align 8
  %r8 = call i64 (i64, i8*, i32, ...) @jdruby_send(i64 %self_for_call_8, i8* %meth_ptr_8, i32 1, i64 %r3)
  ret i64 %r8
}


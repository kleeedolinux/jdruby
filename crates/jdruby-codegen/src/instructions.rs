//! MIR instruction emission to LLVM IR using Inkwell.

use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::values::{BasicMetadataValueEnum, BasicValueEnum, FunctionValue, PointerValue};
use inkwell::basic_block::BasicBlock;
use inkwell::AddressSpace;
use jdruby_common::Diagnostic;
use jdruby_mir::{MirFunction, MirBlock, MirInst, MirTerminator, MirConst, MirBinOp, MirUnOp};
use crate::context::CodegenContext;
use crate::runtime::get_runtime_fn_value;
use crate::utils::sanitize_name;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};

// Single global counter for all unique global names
static GLOBAL_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Register mapping for function-local values.
type RegMap<'ctx> = HashMap<u32, BasicValueEnum<'ctx>>;

/// Emit a function to LLVM IR using Inkwell.
pub fn emit_function<'ctx>(
    func: &MirFunction,
    ctx: &CodegenContext<'ctx>,
    llvm_ctx: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
) -> Result<FunctionValue<'ctx>, Vec<Diagnostic>> {
    let i64_type = llvm_ctx.i64_type();
    
    // Get or create function
    let fn_name = sanitize_name(&func.name);
    let function = if let Some(existing) = module.get_function(&fn_name) {
        // Function was pre-declared, use it
        existing
    } else {
        // Create function type: i64 @func_name(i64, i64, ...)
        let fn_type = i64_type.fn_type(&vec![i64_type.into(); func.params.len()], false);
        module.add_function(&fn_name, fn_type, None)
    };
    
    // Create all basic blocks first
    let mut blocks: HashMap<String, BasicBlock<'ctx>> = HashMap::new();
    for block in &func.blocks {
        let bb = llvm_ctx.append_basic_block(function, &block.label);
        blocks.insert(block.label.clone(), bb);
    }
    
    // Get the entry block (first block from MIR)
    let entry_bb = func.blocks.first()
        .and_then(|b| blocks.get(&b.label))
        .copied()
        .unwrap_or_else(|| llvm_ctx.append_basic_block(function, "entry"));
    
    // Position at entry block to emit allocas
    builder.position_at_end(entry_bb);
    
    // Add bridge initialization at the start of main function
    if func.name == "main" {
        let init_fn = module.get_function("jdruby_init_bridge");
        if let Some(init_fn) = init_fn {
            builder.build_call(init_fn, &[], "init").unwrap();
        }
    }
    
    // Collect all locals that need allocas (including captured vars)
    let locals = collect_locals(func);
    
    // Create allocas for locals in the entry block
    let mut local_allocs: HashMap<String, PointerValue<'ctx>> = HashMap::new();
    for local in &locals {
        let alloca = builder.build_alloca(i64_type, &format!("local_{}", sanitize_name(local))).unwrap();
        local_allocs.insert(local.clone(), alloca);
    }
    
    // Initialize local_self for main (top-level self is the main object)
    if func.name == "main" {
        if let Some(self_alloca) = local_allocs.get("self") {
            let main_self = module.get_global("JDRUBY_NIL");
            if let Some(global) = main_self {
                let self_val = builder.build_load(i64_type, global.as_pointer_value(), "main_self").unwrap();
                builder.build_store(*self_alloca, self_val).unwrap();
            }
        }
    }
    
    // Map params to registers (captured vars come first for block functions, then actual params)
    let mut reg_map: RegMap = HashMap::new();
    let captured_count = func.captured_vars.len();
    let is_block_function = func.name.starts_with("block_") || func.name.starts_with("block_in_") || func.name.starts_with("__sym_proc_");
    
    // For block functions: captured vars are passed as the first N parameters
    if is_block_function {
        for (i, var_name) in func.captured_vars.iter().enumerate() {
            let param = function.get_nth_param(i as u32).unwrap();
            // Store captured var into its local alloca
            if let Some(alloca) = local_allocs.get(var_name) {
                builder.build_store(*alloca, param).unwrap();
            }
            // Don't add to reg_map - captured vars are accessed via Load instructions
        }
        
        // Map actual function params (after captured vars)
        for (i, reg_id) in func.params.iter().enumerate() {
            let param_idx = (captured_count + i) as u32;
            let param = function.get_nth_param(param_idx).unwrap();
            reg_map.insert(*reg_id, param);
        }
    } else {
        // Regular function: just map params directly
        for (i, reg_id) in func.params.iter().enumerate() {
            let param = function.get_nth_param(i as u32).unwrap();
            reg_map.insert(*reg_id, param);
        }
    }
    
    // Store self if present AND if the first param is not register 0
    // (register 0 will be stored via MIR Store instruction)
    if !func.params.is_empty() && func.params[0] != 0 {
        let self_val = function.get_nth_param(0).unwrap();
        if let Some(self_alloca) = local_allocs.get("self") {
            builder.build_store(*self_alloca, self_val).unwrap();
        }
    }
    
    // If there's more than one block, branch from entry to the first code block
    // Note: if entry_bb IS the first code block, we don't need a branch
    if func.blocks.len() > 1 {
        if let Some(first_block) = func.blocks.first() {
            if let Some(&target) = blocks.get(&first_block.label) {
                if target != entry_bb {
                    builder.build_unconditional_branch(target).unwrap();
                }
            }
        }
    }
    
    // Emit each block
    for block in &func.blocks {
        let bb = *blocks.get(&block.label).unwrap();
        
        // For single-block functions, we're already positioned after allocas
        // For multi-block functions, position at each block
        if func.blocks.len() > 1 || bb != entry_bb {
            builder.position_at_end(bb);
        }
        
        emit_block(block, ctx, llvm_ctx, module, builder, &mut reg_map, &local_allocs, &blocks)?;
    }
    
    Ok(function)
}

fn collect_locals(func: &MirFunction) -> std::collections::HashSet<String> {
    let mut locals = std::collections::HashSet::new();
    for block in &func.blocks {
        for inst in &block.instructions {
            if let MirInst::Store(name, _) | MirInst::Load(_, name) = inst {
                if !name.starts_with(|c: char| c.is_ascii_uppercase())
                    && !name.starts_with('@')
                    && !name.starts_with('$')
                {
                    locals.insert(name.clone());
                }
            }
        }
    }
    
    // Add captured variables as locals so they get allocas
    for captured in &func.captured_vars {
        locals.insert(captured.clone());
    }
    
    let has_self = !func.params.is_empty();
    if has_self {
        locals.insert("self".to_string());
    }
    
    locals
}

fn emit_block<'ctx>(
    block: &MirBlock,
    ctx: &CodegenContext<'ctx>,
    llvm_ctx: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
    reg_map: &mut RegMap<'ctx>,
    local_allocs: &HashMap<String, PointerValue<'ctx>>,
    blocks: &HashMap<String, BasicBlock<'ctx>>,
) -> Result<(), Vec<Diagnostic>> {
    for inst in &block.instructions {
        emit_instruction(inst, ctx, llvm_ctx, module, builder, reg_map, local_allocs)?;
    }
    emit_terminator(&block.terminator, llvm_ctx, module, builder, reg_map, blocks)?;
    Ok(())
}

fn emit_instruction<'ctx>(
    inst: &MirInst,
    ctx: &CodegenContext<'ctx>,
    llvm_ctx: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
    reg_map: &mut RegMap<'ctx>,
    local_allocs: &HashMap<String, PointerValue<'ctx>>,
) -> Result<(), Vec<Diagnostic>> {
    let i64_type = llvm_ctx.i64_type();
    
    match inst {
        MirInst::LoadConst(reg, c) => {
            let val = emit_load_const(c, ctx, llvm_ctx, module, builder)?;
            reg_map.insert(*reg, val);
        }
        MirInst::Copy(dest, src) => {
            let src_val = reg_map.get(src).copied().unwrap_or(i64_type.const_int(0, false).into());
            reg_map.insert(*dest, src_val);
        }
        MirInst::BinOp(dest, op, left, right) => {
            let val = emit_bin_op(op, *left, *right, llvm_ctx, module, builder, reg_map)?;
            reg_map.insert(*dest, val);
        }
        MirInst::UnOp(dest, op, src) => {
            let val = emit_un_op(op, *src, llvm_ctx, module, builder, reg_map)?;
            reg_map.insert(*dest, val);
        }
        MirInst::Call(dest, name, args) => {
            let val = emit_call(name, args, ctx, llvm_ctx, module, builder, reg_map, local_allocs)?;
            reg_map.insert(*dest, val);
        }
        MirInst::MethodCall(dest, recv, method, args) => {
            let val = emit_method_call(*recv, method, args, ctx, llvm_ctx, module, builder, reg_map)?;
            reg_map.insert(*dest, val);
        }
        MirInst::Load(reg, name) => {
            let val = emit_load(name, *reg, ctx, llvm_ctx, module, builder, reg_map, local_allocs)?;
            reg_map.insert(*reg, val);
        }
        MirInst::Store(name, reg) => {
            emit_store(name, *reg, ctx, llvm_ctx, module, builder, reg_map, local_allocs)?;
        }
        MirInst::Alloc(reg, _name) => {
            let alloca = builder.build_alloca(i64_type, &format!("alloca_{}", reg)).unwrap();
            let ptr_val = builder.build_ptr_to_int(alloca, i64_type, &format!("r{}", reg)).unwrap();
            reg_map.insert(*reg, ptr_val.into());
        }
        MirInst::ClassNew(dest, name, superclass) => {
            let val = emit_class_new(name, superclass.as_deref(), ctx, llvm_ctx, module, builder)?;
            reg_map.insert(*dest, val);
        }
        MirInst::DefMethod(class_reg, method_name, func_name) => {
            emit_def_method(*class_reg, method_name, func_name, ctx, llvm_ctx, module, builder, reg_map)?;
        }
        MirInst::DefSingletonMethod(obj_reg, method_name, func_name) => {
            emit_def_singleton_method(*obj_reg, method_name, func_name, ctx, llvm_ctx, module, builder, reg_map)?;
        }
        MirInst::IncludeModule(class_reg, module_name) => {
            emit_include_module(*class_reg, module_name, ctx, llvm_ctx, module, builder, reg_map)?;
        }
        MirInst::Nop => {}

        // =========================================================================
        // CLASS/MODULE OPERATIONS
        // =========================================================================
        MirInst::ModuleNew(dest, name) => {
            let val = emit_module_new(name, ctx, llvm_ctx, module, builder)?;
            reg_map.insert(*dest, val);
        }
        MirInst::SingletonClassGet(dest, obj_reg) => {
            let val = emit_singleton_class_get(*obj_reg, ctx, llvm_ctx, module, builder, reg_map)?;
            reg_map.insert(*dest, val);
        }
        MirInst::PrependModule(class_reg, module_name) => {
            emit_prepend_module(*class_reg, module_name, ctx, llvm_ctx, module, builder, reg_map)?;
        }
        MirInst::ExtendModule(obj_reg, module_name) => {
            emit_extend_module(*obj_reg, module_name, ctx, llvm_ctx, module, builder, reg_map)?;
        }

        // =========================================================================
        // BLOCK/CLOSURE OPERATIONS
        // =========================================================================
        MirInst::BlockCreate { dest, func_symbol, captured_vars, is_lambda } => {
            let val = emit_block_create(func_symbol, captured_vars, *is_lambda, ctx, llvm_ctx, module, builder, reg_map)?;
            reg_map.insert(*dest, val);
        }
        MirInst::ProcCreate { dest, block_reg } => {
            let val = emit_proc_create(*block_reg, ctx, llvm_ctx, module, builder, reg_map)?;
            reg_map.insert(*dest, val);
        }
        MirInst::LambdaCreate { dest, block_reg } => {
            let val = emit_lambda_create(*block_reg, ctx, llvm_ctx, module, builder, reg_map)?;
            reg_map.insert(*dest, val);
        }
        MirInst::BlockYield { dest, block_reg, args } => {
            let val = emit_block_yield(*block_reg, args, ctx, llvm_ctx, module, builder, reg_map)?;
            reg_map.insert(*dest, val);
        }
        MirInst::BlockGiven { dest } => {
            let val = emit_block_given(ctx, llvm_ctx, module, builder)?;
            reg_map.insert(*dest, val);
        }
        MirInst::CurrentBlock { dest } => {
            let val = emit_current_block(ctx, llvm_ctx, module, builder)?;
            reg_map.insert(*dest, val);
        }

        // =========================================================================
        // DYNAMIC METHOD OPERATIONS
        // =========================================================================
        MirInst::DefineMethodDynamic { dest, class_reg, name_reg, method_func, visibility } => {
            let val = emit_define_method_dynamic(*class_reg, *name_reg, method_func, visibility, ctx, llvm_ctx, module, builder, reg_map)?;
            reg_map.insert(*dest, val);
        }
        MirInst::UndefMethod { dest, class_reg, name_reg } => {
            let val = emit_undef_method(*class_reg, *name_reg, ctx, llvm_ctx, module, builder, reg_map)?;
            reg_map.insert(*dest, val);
        }
        MirInst::RemoveMethod { dest, class_reg, name_reg } => {
            let val = emit_remove_method(*class_reg, *name_reg, ctx, llvm_ctx, module, builder, reg_map)?;
            reg_map.insert(*dest, val);
        }
        MirInst::AliasMethod { dest, class_reg, new_name_reg, old_name_reg } => {
            let val = emit_alias_method(*class_reg, *new_name_reg, *old_name_reg, ctx, llvm_ctx, module, builder, reg_map)?;
            reg_map.insert(*dest, val);
        }
        MirInst::SetVisibility { dest, class_reg, visibility, method_names } => {
            let val = emit_set_visibility(*class_reg, visibility, method_names, ctx, llvm_ctx, module, builder, reg_map)?;
            reg_map.insert(*dest, val);
        }

        // =========================================================================
        // DYNAMIC EVALUATION
        // =========================================================================
        MirInst::Eval { dest, code_reg, binding_reg, filename_reg, line_reg } => {
            let val = emit_eval(*code_reg, *binding_reg, *filename_reg, *line_reg, ctx, llvm_ctx, module, builder, reg_map)?;
            reg_map.insert(*dest, val);
        }
        MirInst::InstanceEval { dest, obj_reg, code_reg, binding_reg } => {
            let val = emit_instance_eval(*obj_reg, *code_reg, *binding_reg, ctx, llvm_ctx, module, builder, reg_map)?;
            reg_map.insert(*dest, val);
        }
        MirInst::ClassEval { dest, class_reg, code_reg, binding_reg } => {
            let val = emit_class_eval(*class_reg, *code_reg, *binding_reg, ctx, llvm_ctx, module, builder, reg_map)?;
            reg_map.insert(*dest, val);
        }
        MirInst::ModuleEval { dest, module_reg, code_reg, binding_reg } => {
            let val = emit_module_eval(*module_reg, *code_reg, *binding_reg, ctx, llvm_ctx, module, builder, reg_map)?;
            reg_map.insert(*dest, val);
        }
        MirInst::BindingGet { dest } => {
            let val = emit_binding_get(ctx, llvm_ctx, module, builder)?;
            reg_map.insert(*dest, val);
        }

        // =========================================================================
        // REFLECTION
        // =========================================================================
        MirInst::Send { dest, obj_reg, name_reg, args, block_reg } => {
            let val = emit_send(*obj_reg, *name_reg, args, *block_reg, ctx, llvm_ctx, module, builder, reg_map)?;
            reg_map.insert(*dest, val);
        }
        MirInst::PublicSend { dest, obj_reg, name_reg, args, block_reg } => {
            let val = emit_public_send(*obj_reg, *name_reg, args, *block_reg, ctx, llvm_ctx, module, builder, reg_map)?;
            reg_map.insert(*dest, val);
        }
        MirInst::RespondTo { dest, obj_reg, name_reg, include_private } => {
            let val = emit_respond_to(*obj_reg, *name_reg, *include_private, ctx, llvm_ctx, module, builder, reg_map)?;
            reg_map.insert(*dest, val);
        }
        MirInst::MethodGet { dest, obj_reg, name_reg } => {
            let val = emit_method_get(*obj_reg, *name_reg, ctx, llvm_ctx, module, builder, reg_map)?;
            reg_map.insert(*dest, val);
        }
        MirInst::InstanceMethodGet { dest, class_reg, name_reg } => {
            let val = emit_instance_method_get(*class_reg, *name_reg, ctx, llvm_ctx, module, builder, reg_map)?;
            reg_map.insert(*dest, val);
        }
        MirInst::MethodObjectCall { dest, method_reg, receiver_reg, args, block_reg } => {
            let val = emit_method_object_call(*method_reg, *receiver_reg, args, *block_reg, ctx, llvm_ctx, module, builder, reg_map)?;
            reg_map.insert(*dest, val);
        }
        MirInst::MethodBind { dest, method_reg, obj_reg } => {
            let val = emit_method_bind(*method_reg, *obj_reg, ctx, llvm_ctx, module, builder, reg_map)?;
            reg_map.insert(*dest, val);
        }

        // =========================================================================
        // DYNAMIC VARIABLE ACCESS
        // =========================================================================
        MirInst::IvarGetDynamic { dest, obj_reg, name_reg } => {
            let val = emit_ivar_get_dynamic(*obj_reg, *name_reg, ctx, llvm_ctx, module, builder, reg_map)?;
            reg_map.insert(*dest, val);
        }
        MirInst::IvarSetDynamic { obj_reg, name_reg, value_reg } => {
            emit_ivar_set_dynamic(*obj_reg, *name_reg, *value_reg, ctx, llvm_ctx, module, builder, reg_map)?;
        }
        MirInst::CvarGetDynamic { dest, class_reg, name_reg } => {
            let val = emit_cvar_get_dynamic(*class_reg, *name_reg, ctx, llvm_ctx, module, builder, reg_map)?;
            reg_map.insert(*dest, val);
        }
        MirInst::CvarSetDynamic { class_reg, name_reg, value_reg } => {
            emit_cvar_set_dynamic(*class_reg, *name_reg, *value_reg, ctx, llvm_ctx, module, builder, reg_map)?;
        }
        MirInst::ConstGetDynamic { dest, class_reg, name_reg, inherit } => {
            let val = emit_const_get_dynamic(*class_reg, *name_reg, *inherit, ctx, llvm_ctx, module, builder, reg_map)?;
            reg_map.insert(*dest, val);
        }
        MirInst::ConstSetDynamic { class_reg, name_reg, value_reg } => {
            emit_const_set_dynamic(*class_reg, *name_reg, *value_reg, ctx, llvm_ctx, module, builder, reg_map)?;
        }

        // =========================================================================
        // METHOD MISSING
        // =========================================================================
        MirInst::MethodMissing { dest, obj_reg, name_reg, args, block_reg } => {
            let val = emit_method_missing(*obj_reg, *name_reg, args, *block_reg, ctx, llvm_ctx, module, builder, reg_map)?;
            reg_map.insert(*dest, val);
        }
        MirInst::BlockInvoke { dest, block_reg, args, splat_arg, block_arg } => {
            let val = emit_block_invoke(*block_reg, args, *splat_arg, *block_arg, ctx, llvm_ctx, module, builder, reg_map)?;
            reg_map.insert(*dest, val);
        }
        MirInst::SendWithIC { dest, obj_reg, method_name, args, block_reg, cache_slot } => {
            let val = emit_send_with_ic(*obj_reg, method_name, args, *block_reg, *cache_slot, ctx, llvm_ctx, module, builder, reg_map)?;
            reg_map.insert(*dest, val);
        }
    }
    
    Ok(())
}

fn emit_load_const<'ctx>(
    c: &MirConst,
    _ctx: &CodegenContext<'ctx>,
    llvm_ctx: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
) -> Result<BasicValueEnum<'ctx>, Vec<Diagnostic>> {
    let i64_type = llvm_ctx.i64_type();
    
    match c {
        MirConst::Integer(v) => {
            let fn_val = get_runtime_fn_value(module, "jdruby_int_new").unwrap();
            let arg = i64_type.const_int(*v as u64, true);
            let val = builder.build_call(fn_val, &[arg.into()], "int_new").unwrap();
            Ok(val.try_as_basic_value().unwrap_basic())
        }
        MirConst::Float(v) => {
            let fn_val = get_runtime_fn_value(module, "jdruby_float_new").unwrap();
            let f64_type = llvm_ctx.f64_type();
            let arg = f64_type.const_float(*v);
            let val = builder.build_call(fn_val, &[arg.into()], "float_new").unwrap();
            Ok(val.try_as_basic_value().unwrap_basic())
        }
        MirConst::Bool(true) => {
            let global = module.get_global("JDRUBY_TRUE").unwrap();
            let loaded = builder.build_load(i64_type, global.as_pointer_value(), "load_true").unwrap();
            Ok(loaded)
        }
        MirConst::Bool(false) => {
            let global = module.get_global("JDRUBY_FALSE").unwrap();
            let loaded = builder.build_load(i64_type, global.as_pointer_value(), "load_false").unwrap();
            Ok(loaded)
        }
        MirConst::Nil => {
            let global = module.get_global("JDRUBY_NIL").unwrap();
            let loaded = builder.build_load(i64_type, global.as_pointer_value(), "load_nil").unwrap();
            Ok(loaded)
        }
        MirConst::String(s) => {
            // Get or create string constant with unique name
            let byte_len = s.len();
            let i8_type = llvm_ctx.i8_type();
            let array_type = i8_type.array_type((byte_len + 1) as u32);
            let counter = GLOBAL_COUNTER.fetch_add(1, Ordering::SeqCst);
            let global_name = format!("str_const_{}_{}", s.len(), counter);
            let global = module.add_global(array_type, None, &global_name);
            global.set_linkage(inkwell::module::Linkage::Private);
            global.set_unnamed_addr(true);
            global.set_alignment(1);
            
            // Create string bytes with null terminator
            let mut bytes: Vec<_> = s.bytes().map(|b| i8_type.const_int(b as u64, false)).collect();
            bytes.push(i8_type.const_int(0, false));
            let const_array = i8_type.const_array(&bytes);
            global.set_initializer(&const_array);
            
            // GEP to get pointer to first element
            let ptr = global.as_pointer_value();
            let zero = llvm_ctx.i64_type().const_int(0, false);
            let str_ptr = unsafe { builder.build_gep(array_type, ptr, &[zero, zero], "str_ptr").unwrap() };
            
            // Call jdruby_str_new
            let fn_val = get_runtime_fn_value(module, "jdruby_str_new").unwrap();
            let len_val = i64_type.const_int(byte_len as u64, false);
            let val = builder.build_call(fn_val, &[str_ptr.into(), len_val.into()], "str_new").unwrap();
            Ok(val.try_as_basic_value().unwrap_basic())
        }
        MirConst::Symbol(s) => {
            // Similar to string but for symbols with unique name
            let byte_len = s.len();
            let i8_type = llvm_ctx.i8_type();
            let array_type = i8_type.array_type((byte_len + 1) as u32);
            let counter = GLOBAL_COUNTER.fetch_add(1, Ordering::SeqCst);
            let global = module.add_global(array_type, None, &format!("sym_const_{}", counter));
            global.set_linkage(inkwell::module::Linkage::Private);
            global.set_unnamed_addr(true);
            
            let mut bytes: Vec<_> = s.bytes().map(|b| i8_type.const_int(b as u64, false)).collect();
            bytes.push(i8_type.const_int(0, false));
            let const_array = i8_type.const_array(&bytes);
            global.set_initializer(&const_array);
            
            let ptr = global.as_pointer_value();
            let zero = llvm_ctx.i64_type().const_int(0, false);
            let sym_ptr = unsafe { builder.build_gep(array_type, ptr, &[zero, zero], "sym_ptr").unwrap() };
            
            let fn_val = get_runtime_fn_value(module, "jdruby_sym_intern").unwrap();
            let val = builder.build_call(fn_val, &[sym_ptr.into()], "sym_intern").unwrap();
            Ok(val.try_as_basic_value().unwrap_basic())
        }
    }
}

fn emit_bin_op<'ctx>(
    op: &MirBinOp,
    left: u32,
    right: u32,
    llvm_ctx: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
    reg_map: &RegMap<'ctx>,
) -> Result<BasicValueEnum<'ctx>, Vec<Diagnostic>> {
    let i64_type = llvm_ctx.i64_type();
    let i1_type = llvm_ctx.bool_type();
    
    let left_val = reg_map.get(&left).copied().unwrap_or(i64_type.const_int(0, false).into());
    let right_val = reg_map.get(&right).copied().unwrap_or(i64_type.const_int(0, false).into());
    
    match op {
        MirBinOp::Add => {
            let fn_val = get_runtime_fn_value(module, "jdruby_int_add").unwrap();
            let val = builder.build_call(fn_val, &[left_val.into(), right_val.into()], "int_add").unwrap();
            Ok(val.try_as_basic_value().unwrap_basic())
        }
        MirBinOp::Sub => {
            let fn_val = get_runtime_fn_value(module, "jdruby_int_sub").unwrap();
            let val = builder.build_call(fn_val, &[left_val.into(), right_val.into()], "int_sub").unwrap();
            Ok(val.try_as_basic_value().unwrap_basic())
        }
        MirBinOp::Mul => {
            let fn_val = get_runtime_fn_value(module, "jdruby_int_mul").unwrap();
            let val = builder.build_call(fn_val, &[left_val.into(), right_val.into()], "int_mul").unwrap();
            Ok(val.try_as_basic_value().unwrap_basic())
        }
        MirBinOp::Div => {
            let fn_val = get_runtime_fn_value(module, "jdruby_int_div").unwrap();
            let val = builder.build_call(fn_val, &[left_val.into(), right_val.into()], "int_div").unwrap();
            Ok(val.try_as_basic_value().unwrap_basic())
        }
        MirBinOp::Mod => {
            let fn_val = get_runtime_fn_value(module, "jdruby_int_mod").unwrap();
            let val = builder.build_call(fn_val, &[left_val.into(), right_val.into()], "int_mod").unwrap();
            Ok(val.try_as_basic_value().unwrap_basic())
        }
        MirBinOp::Pow => {
            let fn_val = get_runtime_fn_value(module, "jdruby_int_pow").unwrap();
            let val = builder.build_call(fn_val, &[left_val.into(), right_val.into()], "int_pow").unwrap();
            Ok(val.try_as_basic_value().unwrap_basic())
        }
        MirBinOp::Eq => {
            let fn_val = get_runtime_fn_value(module, "jdruby_eq").unwrap();
            let cmp_val = builder.build_call(fn_val, &[left_val.into(), right_val.into()], "eq").unwrap()
                .try_as_basic_value().unwrap_basic().into_int_value();
            let bool_fn = get_runtime_fn_value(module, "jdruby_bool").unwrap();
            let val = builder.build_call(bool_fn, &[cmp_val.into()], "bool").unwrap();
            Ok(val.try_as_basic_value().unwrap_basic())
        }
        MirBinOp::NotEq => {
            let fn_val = get_runtime_fn_value(module, "jdruby_eq").unwrap();
            let cmp_val = builder.build_call(fn_val, &[left_val.into(), right_val.into()], "eq").unwrap()
                .try_as_basic_value().unwrap_basic().into_int_value();
            let inv_val = builder.build_xor(cmp_val, i1_type.const_int(1, false), "neq").unwrap();
            let bool_fn = get_runtime_fn_value(module, "jdruby_bool").unwrap();
            let val = builder.build_call(bool_fn, &[inv_val.into()], "bool").unwrap();
            Ok(val.try_as_basic_value().unwrap_basic())
        }
        MirBinOp::Lt => {
            let fn_val = get_runtime_fn_value(module, "jdruby_lt").unwrap();
            let cmp_val = builder.build_call(fn_val, &[left_val.into(), right_val.into()], "lt").unwrap()
                .try_as_basic_value().unwrap_basic().into_int_value();
            let bool_fn = get_runtime_fn_value(module, "jdruby_bool").unwrap();
            let val = builder.build_call(bool_fn, &[cmp_val.into()], "bool").unwrap();
            Ok(val.try_as_basic_value().unwrap_basic())
        }
        MirBinOp::Gt => {
            let fn_val = get_runtime_fn_value(module, "jdruby_gt").unwrap();
            let cmp_val = builder.build_call(fn_val, &[left_val.into(), right_val.into()], "gt").unwrap()
                .try_as_basic_value().unwrap_basic().into_int_value();
            let bool_fn = get_runtime_fn_value(module, "jdruby_bool").unwrap();
            let val = builder.build_call(bool_fn, &[cmp_val.into()], "bool").unwrap();
            Ok(val.try_as_basic_value().unwrap_basic())
        }
        MirBinOp::LtEq => {
            let fn_val = get_runtime_fn_value(module, "jdruby_le").unwrap();
            let cmp_val = builder.build_call(fn_val, &[left_val.into(), right_val.into()], "le").unwrap()
                .try_as_basic_value().unwrap_basic().into_int_value();
            let bool_fn = get_runtime_fn_value(module, "jdruby_bool").unwrap();
            let val = builder.build_call(bool_fn, &[cmp_val.into()], "bool").unwrap();
            Ok(val.try_as_basic_value().unwrap_basic())
        }
        MirBinOp::GtEq => {
            let fn_val = get_runtime_fn_value(module, "jdruby_ge").unwrap();
            let cmp_val = builder.build_call(fn_val, &[left_val.into(), right_val.into()], "ge").unwrap()
                .try_as_basic_value().unwrap_basic().into_int_value();
            let bool_fn = get_runtime_fn_value(module, "jdruby_bool").unwrap();
            let val = builder.build_call(bool_fn, &[cmp_val.into()], "bool").unwrap();
            Ok(val.try_as_basic_value().unwrap_basic())
        }
        MirBinOp::And => {
            let fn_val = get_runtime_fn_value(module, "jdruby_truthy").unwrap();
            let test_val = builder.build_call(fn_val, &[left_val.into()], "and_test").unwrap()
                .try_as_basic_value().unwrap_basic().into_int_value();
            let val = builder.build_select(test_val, right_val, left_val, "and").unwrap();
            Ok(val)
        }
        MirBinOp::Or => {
            let fn_val = get_runtime_fn_value(module, "jdruby_truthy").unwrap();
            let test_val = builder.build_call(fn_val, &[left_val.into()], "or_test").unwrap()
                .try_as_basic_value().unwrap_basic().into_int_value();
            let val = builder.build_select(test_val, left_val, right_val, "or").unwrap();
            Ok(val)
        }
        MirBinOp::BitAnd => {
            let val = builder.build_and(left_val.into_int_value(), right_val.into_int_value(), "bitand").unwrap();
            Ok(val.into())
        }
        MirBinOp::BitOr => {
            let val = builder.build_or(left_val.into_int_value(), right_val.into_int_value(), "bitor").unwrap();
            Ok(val.into())
        }
        MirBinOp::BitXor => {
            let val = builder.build_xor(left_val.into_int_value(), right_val.into_int_value(), "bitxor").unwrap();
            Ok(val.into())
        }
        MirBinOp::Shl => {
            let val = builder.build_left_shift(left_val.into_int_value(), right_val.into_int_value(), "shl").unwrap();
            Ok(val.into())
        }
        MirBinOp::Shr => {
            let val = builder.build_right_shift(left_val.into_int_value(), right_val.into_int_value(), false, "shr").unwrap();
            Ok(val.into())
        }
        MirBinOp::Cmp => {
            let lt_fn = get_runtime_fn_value(module, "jdruby_lt").unwrap();
            let gt_fn = get_runtime_fn_value(module, "jdruby_gt").unwrap();
            
            let lt_val = builder.build_call(lt_fn, &[left_val.into(), right_val.into()], "cmp_lt").unwrap()
                .try_as_basic_value().unwrap_basic().into_int_value();
            let gt_val = builder.build_call(gt_fn, &[left_val.into(), right_val.into()], "cmp_gt").unwrap()
                .try_as_basic_value().unwrap_basic().into_int_value();
            
            let neg_one = i64_type.const_int((-1i64) as u64, true);
            let zero = i64_type.const_int(0, false);
            let one = i64_type.const_int(1, false);
            
            let sel1 = builder.build_select(lt_val, neg_one, zero, "cmp_sel1").unwrap().into_int_value();
            let val = builder.build_select(gt_val, one, sel1, "cmp").unwrap();
            Ok(val)
        }
    }
}

fn emit_un_op<'ctx>(
    op: &MirUnOp,
    src: u32,
    llvm_ctx: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
    reg_map: &RegMap<'ctx>,
) -> Result<BasicValueEnum<'ctx>, Vec<Diagnostic>> {
    let i64_type = llvm_ctx.i64_type();
    let i1_type = llvm_ctx.bool_type();
    
    let src_val = reg_map.get(&src).copied().unwrap_or(i64_type.const_int(0, false).into());
    
    match op {
        MirUnOp::Neg => {
            let zero = i64_type.const_int(0, false);
            let val = builder.build_int_sub(zero, src_val.into_int_value(), "neg").unwrap();
            Ok(val.into())
        }
        MirUnOp::Not => {
            let fn_val = get_runtime_fn_value(module, "jdruby_truthy").unwrap();
            let test_val = builder.build_call(fn_val, &[src_val.into()], "not_test").unwrap()
                .try_as_basic_value().unwrap_basic().into_int_value();
            let inv_val = builder.build_xor(test_val, i1_type.const_int(1, false), "not_inv").unwrap();
            let bool_fn = get_runtime_fn_value(module, "jdruby_bool").unwrap();
            let val = builder.build_call(bool_fn, &[inv_val.into()], "not").unwrap();
            Ok(val.try_as_basic_value().unwrap_basic())
        }
        MirUnOp::BitNot => {
            let neg_one = i64_type.const_int((-1i64) as u64, true);
            let val = builder.build_xor(src_val.into_int_value(), neg_one, "bitnot").unwrap();
            Ok(val.into())
        }
    }
}

fn emit_call<'ctx>(
    name: &str,
    args: &[u32],
    _ctx: &CodegenContext<'ctx>,
    llvm_ctx: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
    reg_map: &RegMap<'ctx>,
    local_allocs: &HashMap<String, PointerValue<'ctx>>,
) -> Result<BasicValueEnum<'ctx>, Vec<Diagnostic>> {
    let i64_type = llvm_ctx.i64_type();
    
    match name {
        "puts" => {
            let fn_val = get_runtime_fn_value(module, "jdruby_puts").unwrap();
            for &arg in args {
                let arg_val = reg_map.get(&arg).copied().unwrap_or(i64_type.const_int(0, false).into());
                builder.build_call(fn_val, &[arg_val.into()], "").unwrap();
            }
            let global = module.get_global("JDRUBY_NIL").unwrap();
            let nil_val = builder.build_load(i64_type, global.as_pointer_value(), "nil").unwrap();
            Ok(nil_val)
        }
        "print" => {
            let fn_val = get_runtime_fn_value(module, "jdruby_print").unwrap();
            for &arg in args {
                let arg_val = reg_map.get(&arg).copied().unwrap_or(i64_type.const_int(0, false).into());
                builder.build_call(fn_val, &[arg_val.into()], "").unwrap();
            }
            let global = module.get_global("JDRUBY_NIL").unwrap();
            let nil_val = builder.build_load(i64_type, global.as_pointer_value(), "nil").unwrap();
            Ok(nil_val)
        }
        "p" => {
            if let Some(&first) = args.first() {
                let fn_val = get_runtime_fn_value(module, "jdruby_p").unwrap();
                let arg_val = reg_map.get(&first).copied().unwrap_or(i64_type.const_int(0, false).into());
                let val = builder.build_call(fn_val, &[arg_val.into()], "p").unwrap();
                Ok(val.try_as_basic_value().unwrap_basic())
            } else {
                let global = module.get_global("JDRUBY_NIL").unwrap();
                let nil_val = builder.build_load(i64_type, global.as_pointer_value(), "nil").unwrap();
                Ok(nil_val)
            }
        }
        "rb_ary_new" | "jdruby_ary_new" => {
            let fn_val = get_runtime_fn_value(module, "jdruby_ary_new").unwrap();
            let argc_val = llvm_ctx.i32_type().const_int(args.len() as u64, false);
            let arg_vals: Vec<BasicMetadataValueEnum<'ctx>> = args.iter().map(|&r| {
                reg_map.get(&r).copied().unwrap_or(i64_type.const_int(0, false).into()).into()
            }).collect();
            let mut all_args: Vec<_> = vec![argc_val.into()];
            all_args.extend(arg_vals);
            let val = builder.build_call(fn_val, all_args.as_slice(), "ary_new").unwrap();
            Ok(val.try_as_basic_value().unwrap_basic())
        }
        _ => {
            // Check if function exists in the module (for direct calls)
            let func_name = sanitize_name(name);
            if let Some(func_val) = module.get_function(&func_name) {
                // Function exists - call it directly
                let mut arg_vals: Vec<BasicMetadataValueEnum<'ctx>> = vec![];
                
                // For top-level functions without self, we may need to pass nil as first arg
                // Check if function expects a self parameter
                let param_count = func_val.get_params().len();
                let args_count = args.len();
                
                if param_count > 0 && args_count < param_count {
                    // Need to add nil for self parameter
                    let global = module.get_global("JDRUBY_NIL").unwrap();
                    let nil_val = builder.build_load(i64_type, global.as_pointer_value(), "nil_self").unwrap();
                    arg_vals.push(nil_val.into());
                }
                
                // Add remaining arguments
                for &arg in args {
                    let arg_val = reg_map.get(&arg).copied().unwrap_or(i64_type.const_int(0, false).into());
                    arg_vals.push(arg_val.into());
                }
                
                let val = builder.build_call(func_val, &arg_vals, &format!("call_{}", name)).unwrap();
                Ok(val.try_as_basic_value().unwrap_basic())
            } else {
                // Function doesn't exist - fall back to jdruby_send for method dispatch
                let fn_val = get_runtime_fn_value(module, "jdruby_send").unwrap();
                
                // Get self value - for top-level functions without self param, use nil
                // (register 0 will be stored via MIR Store instruction)
                let self_val = if let Some(self_alloca) = local_allocs.get("self") {
                    builder.build_load(i64_type, *self_alloca, "self").unwrap()
                } else {
                    // Top-level code without self - use nil as receiver
                    let global = module.get_global("JDRUBY_NIL").unwrap();
                    builder.build_load(i64_type, global.as_pointer_value(), "nil_self").unwrap()
                };
                
                // Create method name string with unique name
                let i8_type = llvm_ctx.i8_type();
                let i64_ptr_type = llvm_ctx.ptr_type(AddressSpace::default());
                let name_len = name.len();
                let array_type = i8_type.array_type((name_len + 1) as u32);
                let counter = GLOBAL_COUNTER.fetch_add(1, Ordering::SeqCst);
                let global = module.add_global(array_type, None, &format!("meth_call_{}", counter));
                global.set_linkage(inkwell::module::Linkage::Private);
                let mut bytes: Vec<_> = name.bytes().map(|b| i8_type.const_int(b as u64, false)).collect();
                bytes.push(i8_type.const_int(0, false));
                global.set_initializer(&i8_type.const_array(&bytes));
                
                let ptr = global.as_pointer_value();
                let zero = llvm_ctx.i64_type().const_int(0, false);
                let meth_ptr = unsafe { builder.build_gep(array_type, ptr, &[zero, zero], "meth_ptr").unwrap() };
                
                let argc_val = llvm_ctx.i32_type().const_int(args.len() as u64, false);
                
                // Build argv array on stack if there are arguments
                let argv_ptr = if args.is_empty() {
                    i64_ptr_type.const_null()
                } else {
                    let argv_alloca = builder.build_alloca(i64_type.array_type(args.len() as u32), "argv").unwrap();
                    for (i, &arg) in args.iter().enumerate() {
                        let arg_val = reg_map.get(&arg).copied().unwrap_or(i64_type.const_int(0, false).into());
                        let idx = i64_type.const_int(i as u64, false);
                        let elem_ptr = unsafe { builder.build_gep(i64_type.array_type(args.len() as u32), argv_alloca, &[zero, idx], &format!("argv_{}", i)).unwrap() };
                        builder.build_store(elem_ptr, arg_val).unwrap();
                    }
                    unsafe { builder.build_gep(i64_type.array_type(args.len() as u32), argv_alloca, &[zero, zero], "argv_ptr").unwrap() }
                };
                
                let val = builder.build_call(fn_val, &[self_val.into(), meth_ptr.into(), argc_val.into(), argv_ptr.into()], "send").unwrap();
                Ok(val.try_as_basic_value().unwrap_basic())
            }
        }
    }
}

fn emit_method_call<'ctx>(
    recv: u32,
    method: &str,
    args: &[u32],
    _ctx: &CodegenContext<'ctx>,
    llvm_ctx: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
    reg_map: &RegMap<'ctx>,
) -> Result<BasicValueEnum<'ctx>, Vec<Diagnostic>> {
    let i64_type = llvm_ctx.i64_type();
    let recv_val = reg_map.get(&recv).copied().unwrap_or(i64_type.const_int(0, false).into());
    
    // Handle string operations with dedicated runtime functions
    match method {
        "+" if args.len() == 1 => {
            // String concatenation
            let arg_val = reg_map.get(&args[0]).copied().unwrap_or(i64_type.const_int(0, false).into());
            let fn_val = get_runtime_fn_value(module, "jdruby_str_concat").unwrap();
            let val = builder.build_call(fn_val, &[recv_val.into(), arg_val.into()], "str_concat").unwrap();
            return Ok(val.try_as_basic_value().unwrap_basic());
        }
        "to_s" if args.is_empty() => {
            // Convert to string
            let fn_val = get_runtime_fn_value(module, "jdruby_to_s").unwrap();
            let val = builder.build_call(fn_val, &[recv_val.into()], "to_s").unwrap();
            return Ok(val.try_as_basic_value().unwrap_basic());
        }
        _ => {}
    }
    
    let i8_type = llvm_ctx.i8_type();
    let i64_ptr_type = llvm_ctx.ptr_type(AddressSpace::default());
    
    let fn_val = get_runtime_fn_value(module, "jdruby_send").unwrap();
    
    // Create method name string with unique name
    let name_len = method.len();
    let array_type = i8_type.array_type((name_len + 1) as u32);
    let counter = GLOBAL_COUNTER.fetch_add(1, Ordering::SeqCst);
    let global = module.add_global(array_type, None, &format!("meth_call_{}", counter));
    global.set_linkage(inkwell::module::Linkage::Private);
    let mut bytes: Vec<_> = method.bytes().map(|b| i8_type.const_int(b as u64, false)).collect();
    bytes.push(i8_type.const_int(0, false));
    global.set_initializer(&i8_type.const_array(&bytes));
    
    let ptr = global.as_pointer_value();
    let zero = llvm_ctx.i64_type().const_int(0, false);
    let meth_ptr = unsafe { builder.build_gep(array_type, ptr, &[zero, zero], "meth_ptr").unwrap() };
    
    let argc_val = llvm_ctx.i32_type().const_int(args.len() as u64, false);
    
    // Build argv array on stack if there are arguments
    let argv_ptr = if args.is_empty() {
        // Pass null pointer for empty args
        i64_ptr_type.const_null()
    } else {
        // Allocate array on stack
        let argv_alloca = builder.build_alloca(i64_type.array_type(args.len() as u32), "argv").unwrap();
        
        // Store each argument into the array
        for (i, &arg) in args.iter().enumerate() {
            let arg_val = reg_map.get(&arg).copied().unwrap_or(i64_type.const_int(0, false).into());
            let idx = i64_type.const_int(i as u64, false);
            let elem_ptr = unsafe { builder.build_gep(i64_type.array_type(args.len() as u32), argv_alloca, &[zero, idx], &format!("argv_{}", i)).unwrap() };
            builder.build_store(elem_ptr, arg_val).unwrap();
        }
        
        // Get pointer to first element
        let argv_ptr_val = unsafe { builder.build_gep(i64_type.array_type(args.len() as u32), argv_alloca, &[zero, zero], "argv_ptr").unwrap() };
        argv_ptr_val
    };
    
    let val = builder.build_call(fn_val, &[recv_val.into(), meth_ptr.into(), argc_val.into(), argv_ptr.into()], "send").unwrap();
    Ok(val.try_as_basic_value().unwrap_basic())
}

fn emit_load<'ctx>(
    name: &str,
    _reg: u32,
    _ctx: &CodegenContext<'ctx>,
    llvm_ctx: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
    _reg_map: &RegMap<'ctx>,
    local_allocs: &HashMap<String, PointerValue<'ctx>>,
) -> Result<BasicValueEnum<'ctx>, Vec<Diagnostic>> {
    let i64_type = llvm_ctx.i64_type();
    let i8_type = llvm_ctx.i8_type();
    
    if name.starts_with(|c: char| c.is_ascii_uppercase()) || name.starts_with('$') {
        // Global variable load - create/get the actual global and load from it
        let global_name = sanitize_name(name);
        let global = module.get_global(&global_name).unwrap_or_else(|| {
            let g = module.add_global(i64_type, None, &global_name);
            g.set_initializer(&i64_type.const_int(0, false));
            g
        });
        let val = builder.build_load(i64_type, global.as_pointer_value(), &format!("load_{}", global_name)).unwrap();
        Ok(val)
    } else if name.starts_with('@') {
        // Instance variable
        let self_alloca = local_allocs.get("self").copied().unwrap();
        let self_val = builder.build_load(i64_type, self_alloca, "self").unwrap();
        
        let name_len = name.len();
        let array_type = i8_type.array_type((name_len + 1) as u32);
        let counter = GLOBAL_COUNTER.fetch_add(1, Ordering::SeqCst);
        let global = module.add_global(array_type, None, &format!("ivar_name_{}", counter));
        global.set_linkage(inkwell::module::Linkage::Private);
        let mut bytes: Vec<_> = name.bytes().map(|b| i8_type.const_int(b as u64, false)).collect();
        bytes.push(i8_type.const_int(0, false));
        global.set_initializer(&i8_type.const_array(&bytes));
        
        let ptr = global.as_pointer_value();
        let zero = llvm_ctx.i64_type().const_int(0, false);
        let ivar_ptr = unsafe { builder.build_gep(array_type, ptr, &[zero, zero], "ivar_ptr").unwrap() };
        
        let fn_val = get_runtime_fn_value(module, "jdruby_ivar_get").unwrap();
        let val = builder.build_call(fn_val, &[self_val.into(), ivar_ptr.into()], "ivar_get").unwrap();
        Ok(val.try_as_basic_value().unwrap_basic())
    } else {
        // Local variable
        let alloca = local_allocs.get(name).copied().unwrap();
        let val = builder.build_load(i64_type, alloca, &format!("load_{}", sanitize_name(name))).unwrap();
        Ok(val)
    }
}

fn emit_store<'ctx>(
    name: &str,
    reg: u32,
    _ctx: &CodegenContext<'ctx>,
    llvm_ctx: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
    reg_map: &RegMap<'ctx>,
    local_allocs: &HashMap<String, PointerValue<'ctx>>,
) -> Result<(), Vec<Diagnostic>> {
    let i64_type = llvm_ctx.i64_type();
    let i8_type = llvm_ctx.i8_type();
    let reg_val = reg_map.get(&reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    
    if name.starts_with(|c: char| c.is_ascii_uppercase()) || name.starts_with('$') {
        // Global store
        let global_name = sanitize_name(name);
        let global = module.get_global(&global_name).unwrap_or_else(|| {
            let g = module.add_global(i64_type, None, &global_name);
            g.set_initializer(&i64_type.const_int(0, false));
            g
        });
        builder.build_store(global.as_pointer_value(), reg_val).unwrap();
    } else if name.starts_with('@') {
        // Instance variable store
        let self_alloca = local_allocs.get("self").copied().unwrap();
        let self_val = builder.build_load(i64_type, self_alloca, "self").unwrap();
        
        let name_len = name.len();
        let array_type = i8_type.array_type((name_len + 1) as u32);
        let counter = GLOBAL_COUNTER.fetch_add(1, Ordering::SeqCst);
        let global = module.add_global(array_type, None, &format!("ivar_name_{}", counter));
        global.set_linkage(inkwell::module::Linkage::Private);
        let mut bytes: Vec<_> = name.bytes().map(|b| i8_type.const_int(b as u64, false)).collect();
        bytes.push(i8_type.const_int(0, false));
        global.set_initializer(&i8_type.const_array(&bytes));
        
        let ptr = global.as_pointer_value();
        let zero = llvm_ctx.i64_type().const_int(0, false);
        let ivar_ptr = unsafe { builder.build_gep(array_type, ptr, &[zero, zero], "ivar_ptr").unwrap() };
        
        let fn_val = get_runtime_fn_value(module, "jdruby_ivar_set").unwrap();
        builder.build_call(fn_val, &[self_val.into(), ivar_ptr.into(), reg_val.into()], "").unwrap();
    } else {
        // Local store
        let alloca = local_allocs.get(name).copied().unwrap();
        builder.build_store(alloca, reg_val).unwrap();
    }
    
    Ok(())
}

fn emit_class_new<'ctx>(
    name: &str,
    superclass: Option<&str>,
    _ctx: &CodegenContext<'ctx>,
    llvm_ctx: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
) -> Result<BasicValueEnum<'ctx>, Vec<Diagnostic>> {
    let i64_type = llvm_ctx.i64_type();
    let i8_type = llvm_ctx.i8_type();
    
    // Class name with unique suffix
    let name_len = name.len();
    let array_type = i8_type.array_type((name_len + 1) as u32);
    let counter = GLOBAL_COUNTER.fetch_add(1, Ordering::SeqCst);
    let global = module.add_global(array_type, None, &format!("class_name_{}", counter));
    global.set_linkage(inkwell::module::Linkage::Private);
    let mut bytes: Vec<_> = name.bytes().map(|b| i8_type.const_int(b as u64, false)).collect();
    bytes.push(i8_type.const_int(0, false));
    global.set_initializer(&i8_type.const_array(&bytes));
    
    let ptr = global.as_pointer_value();
    let zero = llvm_ctx.i64_type().const_int(0, false);
    let name_ptr = unsafe { builder.build_gep(array_type, ptr, &[zero, zero], "name_ptr").unwrap() };
    
    // Superclass
    let sc_val = if let Some(sc) = superclass {
        let sc_len = sc.len();
        let sc_array_type = i8_type.array_type((sc_len + 1) as u32);
        let counter = GLOBAL_COUNTER.fetch_add(1, Ordering::SeqCst);
        let sc_global = module.add_global(sc_array_type, None, &format!("sc_name_{}", counter));
        sc_global.set_linkage(inkwell::module::Linkage::Private);
        let mut sc_bytes: Vec<_> = sc.bytes().map(|b| i8_type.const_int(b as u64, false)).collect();
        sc_bytes.push(i8_type.const_int(0, false));
        sc_global.set_initializer(&i8_type.const_array(&sc_bytes));
        
        let sc_ptr = sc_global.as_pointer_value();
        let sc_name_ptr = unsafe { builder.build_gep(sc_array_type, sc_ptr, &[zero, zero], "sc_name_ptr").unwrap() };
        
        let const_fn = get_runtime_fn_value(module, "jdruby_const_get").unwrap();
        let sc_val_call = builder.build_call(const_fn, &[sc_name_ptr.into()], "sc_get").unwrap();
        sc_val_call.try_as_basic_value().unwrap_basic()
    } else {
        let global = module.get_global("JDRUBY_NIL").unwrap();
        builder.build_load(i64_type, global.as_pointer_value(), "nil_sc").unwrap()
    };
    
    let fn_val = get_runtime_fn_value(module, "jdruby_class_new").unwrap();
    let val = builder.build_call(fn_val, &[name_ptr.into(), sc_val.into()], "class_new").unwrap();
    Ok(val.try_as_basic_value().unwrap_basic())
}

fn emit_def_method<'ctx>(
    class_reg: u32,
    method_name: &str,
    func_name: &str,
    _ctx: &CodegenContext<'ctx>,
    llvm_ctx: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
    reg_map: &RegMap<'ctx>,
) -> Result<(), Vec<Diagnostic>> {
    let i64_type = llvm_ctx.i64_type();
    let i8_type = llvm_ctx.i8_type();
    
    let class_val = reg_map.get(&class_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    
    // Method name with unique suffix
    let meth_len = method_name.len();
    let meth_array_type = i8_type.array_type((meth_len + 1) as u32);
    let counter = GLOBAL_COUNTER.fetch_add(1, Ordering::SeqCst);
    let meth_global = module.add_global(meth_array_type, None, &format!("def_meth_name_{}", counter));
    meth_global.set_linkage(inkwell::module::Linkage::Private);
    let mut meth_bytes: Vec<_> = method_name.bytes().map(|b| i8_type.const_int(b as u64, false)).collect();
    meth_bytes.push(i8_type.const_int(0, false));
    meth_global.set_initializer(&i8_type.const_array(&meth_bytes));
    
    let meth_ptr = meth_global.as_pointer_value();
    let zero = llvm_ctx.i64_type().const_int(0, false);
    let meth_name_ptr = unsafe { builder.build_gep(meth_array_type, meth_ptr, &[zero, zero], "meth_name_ptr").unwrap() };
    
    // Function name with unique suffix - sanitize the function name for LLVM
    let sanitized_func_name = sanitize_name(func_name);
    let func_len = sanitized_func_name.len();
    let func_array_type = i8_type.array_type((func_len + 1) as u32);
    let counter = GLOBAL_COUNTER.fetch_add(1, Ordering::SeqCst);
    let func_global = module.add_global(func_array_type, None, &format!("def_func_name_{}", counter));
    func_global.set_linkage(inkwell::module::Linkage::Private);
    let mut func_bytes: Vec<_> = sanitized_func_name.bytes().map(|b| i8_type.const_int(b as u64, false)).collect();
    func_bytes.push(i8_type.const_int(0, false));
    func_global.set_initializer(&i8_type.const_array(&func_bytes));
    
    let func_ptr = func_global.as_pointer_value();
    let func_name_ptr = unsafe { builder.build_gep(func_array_type, func_ptr, &[zero, zero], "func_name_ptr").unwrap() };
    
    let fn_val = get_runtime_fn_value(module, "jdruby_def_method").unwrap();
    builder.build_call(fn_val, &[class_val.into(), meth_name_ptr.into(), func_name_ptr.into()], "").unwrap();
    
    Ok(())
}

fn emit_def_singleton_method<'ctx>(
    obj_reg: u32,
    method_name: &str,
    func_name: &str,
    _ctx: &CodegenContext<'ctx>,
    llvm_ctx: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
    reg_map: &RegMap<'ctx>,
) -> Result<(), Vec<Diagnostic>> {
    let i64_type = llvm_ctx.i64_type();
    let i8_type = llvm_ctx.i8_type();
    
    let obj_val = reg_map.get(&obj_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    
    // Method name with unique suffix
    let meth_len = method_name.len();
    let meth_array_type = i8_type.array_type((meth_len + 1) as u32);
    let counter = GLOBAL_COUNTER.fetch_add(1, Ordering::SeqCst);
    let meth_global = module.add_global(meth_array_type, None, &format!("def_singleton_meth_name_{}", counter));
    meth_global.set_linkage(inkwell::module::Linkage::Private);
    let mut meth_bytes: Vec<_> = method_name.bytes().map(|b| i8_type.const_int(b as u64, false)).collect();
    meth_bytes.push(i8_type.const_int(0, false));
    meth_global.set_initializer(&i8_type.const_array(&meth_bytes));
    
    let meth_ptr = meth_global.as_pointer_value();
    let zero = llvm_ctx.i64_type().const_int(0, false);
    let meth_name_ptr = unsafe { builder.build_gep(meth_array_type, meth_ptr, &[zero, zero], "singleton_meth_name_ptr").unwrap() };
    
    // Function name with unique suffix - sanitize the function name for LLVM
    let sanitized_func_name = sanitize_name(func_name);
    let func_len = sanitized_func_name.len();
    let func_array_type = i8_type.array_type((func_len + 1) as u32);
    let counter = GLOBAL_COUNTER.fetch_add(1, Ordering::SeqCst);
    let func_global = module.add_global(func_array_type, None, &format!("def_singleton_func_name_{}", counter));
    func_global.set_linkage(inkwell::module::Linkage::Private);
    let mut func_bytes: Vec<_> = sanitized_func_name.bytes().map(|b| i8_type.const_int(b as u64, false)).collect();
    func_bytes.push(i8_type.const_int(0, false));
    func_global.set_initializer(&i8_type.const_array(&func_bytes));
    
    let func_ptr = func_global.as_pointer_value();
    let func_name_ptr = unsafe { builder.build_gep(func_array_type, func_ptr, &[zero, zero], "singleton_func_name_ptr").unwrap() };
    
    let fn_val = get_runtime_fn_value(module, "jdruby_def_singleton_method").unwrap();
    builder.build_call(fn_val, &[obj_val.into(), meth_name_ptr.into(), func_name_ptr.into()], "").unwrap();
    
    Ok(())
}

fn emit_include_module<'ctx>(
    class_reg: u32,
    module_name: &str,
    _ctx: &CodegenContext<'ctx>,
    llvm_ctx: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
    reg_map: &RegMap<'ctx>,
) -> Result<(), Vec<Diagnostic>> {
    let i64_type = llvm_ctx.i64_type();
    let i8_type = llvm_ctx.i8_type();
    
    let class_val = reg_map.get(&class_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    
    // Module name with unique suffix
    let mod_len = module_name.len();
    let mod_array_type = i8_type.array_type((mod_len + 1) as u32);
    let counter = GLOBAL_COUNTER.fetch_add(1, Ordering::SeqCst);
    let mod_global = module.add_global(mod_array_type, None, &format!("inc_mod_name_{}", counter));
    mod_global.set_linkage(inkwell::module::Linkage::Private);
    let mut mod_bytes: Vec<_> = module_name.bytes().map(|b| i8_type.const_int(b as u64, false)).collect();
    mod_bytes.push(i8_type.const_int(0, false));
    mod_global.set_initializer(&i8_type.const_array(&mod_bytes));
    
    let mod_ptr = mod_global.as_pointer_value();
    let zero = llvm_ctx.i64_type().const_int(0, false);
    let mod_name_ptr = unsafe { builder.build_gep(mod_array_type, mod_ptr, &[zero, zero], "mod_name_ptr").unwrap() };
    
    let const_fn = get_runtime_fn_value(module, "jdruby_const_get").unwrap();
    let mod_val = builder.build_call(const_fn, &[mod_name_ptr.into()], "mod_get").unwrap()
        .try_as_basic_value().unwrap_basic();
    
    // Include method name
    let incl_name = "include";
    let incl_len = incl_name.len();
    let incl_array_type = i8_type.array_type((incl_len + 1) as u32);
    let incl_global = module.add_global(incl_array_type, None, &format!("inc_name_{}", reg_map.len()));
    incl_global.set_linkage(inkwell::module::Linkage::Private);
    let mut incl_bytes: Vec<_> = incl_name.bytes().map(|b| i8_type.const_int(b as u64, false)).collect();
    incl_bytes.push(i8_type.const_int(0, false));
    incl_global.set_initializer(&i8_type.const_array(&incl_bytes));
    
    let incl_ptr = incl_global.as_pointer_value();
    let incl_name_ptr = unsafe { builder.build_gep(incl_array_type, incl_ptr, &[zero, zero], "incl_name_ptr").unwrap() };
    
    let fn_val = get_runtime_fn_value(module, "jdruby_send").unwrap();
    let argc_val = llvm_ctx.i32_type().const_int(1, false);
    
    // Build argv array with mod_val
    let argv_alloca = builder.build_alloca(i64_type.array_type(1), "argv").unwrap();
    let zero = llvm_ctx.i64_type().const_int(0, false);
    let elem_ptr = unsafe { builder.build_gep(i64_type.array_type(1), argv_alloca, &[zero, zero], "argv_0").unwrap() };
    builder.build_store(elem_ptr, mod_val).unwrap();
    let argv_ptr = unsafe { builder.build_gep(i64_type.array_type(1), argv_alloca, &[zero, zero], "argv_ptr").unwrap() };
    
    builder.build_call(fn_val, &[class_val.into(), incl_name_ptr.into(), argc_val.into(), argv_ptr.into()], "").unwrap();
    
    Ok(())
}

fn emit_terminator<'ctx>(
    term: &MirTerminator,
    llvm_ctx: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
    reg_map: &RegMap<'ctx>,
    blocks: &HashMap<String, BasicBlock<'ctx>>,
) -> Result<(), Vec<Diagnostic>> {
    let i64_type = llvm_ctx.i64_type();
    
    match term {
        MirTerminator::Return(Some(reg)) => {
            let reg_val = reg_map.get(reg).copied().unwrap_or(i64_type.const_int(0, false).into());
            builder.build_return(Some(&reg_val)).unwrap();
        }
        MirTerminator::Return(None) => {
            let global = module.get_global("JDRUBY_NIL").unwrap();
            let nil_val = builder.build_load(i64_type, global.as_pointer_value(), "ret_nil").unwrap();
            builder.build_return(Some(&nil_val)).unwrap();
        }
        MirTerminator::Branch(label) => {
            let target = *blocks.get(label).unwrap();
            builder.build_unconditional_branch(target).unwrap();
        }
        MirTerminator::CondBranch(reg, then_l, else_l) => {
            let reg_val = reg_map.get(reg).copied().unwrap_or(i64_type.const_int(0, false).into());
            let fn_val = get_runtime_fn_value(module, "jdruby_truthy").unwrap();
            let cond_val = builder.build_call(fn_val, &[reg_val.into()], "br_cond").unwrap()
                .try_as_basic_value().unwrap_basic().into_int_value();
            let then_bb = *blocks.get(then_l).unwrap();
            let else_bb = *blocks.get(else_l).unwrap();
            builder.build_conditional_branch(cond_val, then_bb, else_bb).unwrap();
        }
        MirTerminator::Unreachable => {
            builder.build_unreachable().unwrap();
        }
    }
    
    Ok(())
}

// =========================================================================
// CLASS/MODULE OPERATIONS
// =========================================================================

fn emit_module_new<'ctx>(
    name: &str,
    _ctx: &CodegenContext<'ctx>,
    llvm_ctx: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
) -> Result<BasicValueEnum<'ctx>, Vec<Diagnostic>> {
    let _i64_type = llvm_ctx.i64_type();
    let i8_type = llvm_ctx.i8_type();
    
    let name_len = name.len();
    let array_type = i8_type.array_type((name_len + 1) as u32);
    let counter = GLOBAL_COUNTER.fetch_add(1, Ordering::SeqCst);
    let global = module.add_global(array_type, None, &format!("mod_name_{}", counter));
    global.set_linkage(inkwell::module::Linkage::Private);
    let mut bytes: Vec<_> = name.bytes().map(|b| i8_type.const_int(b as u64, false)).collect();
    bytes.push(i8_type.const_int(0, false));
    global.set_initializer(&i8_type.const_array(&bytes));
    
    let ptr = global.as_pointer_value();
    let zero = llvm_ctx.i64_type().const_int(0, false);
    let name_ptr = unsafe { builder.build_gep(array_type, ptr, &[zero, zero], "mod_name_ptr").unwrap() };
    
    let fn_val = get_runtime_fn_value(module, "jdruby_module_new").unwrap();
    let val = builder.build_call(fn_val, &[name_ptr.into()], "module_new").unwrap();
    Ok(val.try_as_basic_value().unwrap_basic())
}

fn emit_singleton_class_get<'ctx>(
    obj_reg: u32,
    _ctx: &CodegenContext<'ctx>,
    llvm_ctx: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
    reg_map: &RegMap<'ctx>,
) -> Result<BasicValueEnum<'ctx>, Vec<Diagnostic>> {
    let i64_type = llvm_ctx.i64_type();
    let obj_val = reg_map.get(&obj_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    
    let fn_val = get_runtime_fn_value(module, "jdruby_singleton_class_get").unwrap();
    let val = builder.build_call(fn_val, &[obj_val.into()], "singleton_class_get").unwrap();
    Ok(val.try_as_basic_value().unwrap_basic())
}

fn emit_prepend_module<'ctx>(
    class_reg: u32,
    module_name: &str,
    _ctx: &CodegenContext<'ctx>,
    llvm_ctx: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
    reg_map: &RegMap<'ctx>,
) -> Result<(), Vec<Diagnostic>> {
    let i64_type = llvm_ctx.i64_type();
    let i8_type = llvm_ctx.i8_type();
    
    let class_val = reg_map.get(&class_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    
    let name_len = module_name.len();
    let array_type = i8_type.array_type((name_len + 1) as u32);
    let counter = GLOBAL_COUNTER.fetch_add(1, Ordering::SeqCst);
    let global = module.add_global(array_type, None, &format!("prepend_mod_name_{}", counter));
    global.set_linkage(inkwell::module::Linkage::Private);
    let mut bytes: Vec<_> = module_name.bytes().map(|b| i8_type.const_int(b as u64, false)).collect();
    bytes.push(i8_type.const_int(0, false));
    global.set_initializer(&i8_type.const_array(&bytes));
    
    let ptr = global.as_pointer_value();
    let zero = llvm_ctx.i64_type().const_int(0, false);
    let name_ptr = unsafe { builder.build_gep(array_type, ptr, &[zero, zero], "prepend_name_ptr").unwrap() };
    
    let fn_val = get_runtime_fn_value(module, "jdruby_prepend_module").unwrap();
    builder.build_call(fn_val, &[class_val.into(), name_ptr.into()], "").unwrap();
    Ok(())
}

fn emit_extend_module<'ctx>(
    obj_reg: u32,
    module_name: &str,
    _ctx: &CodegenContext<'ctx>,
    llvm_ctx: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
    reg_map: &RegMap<'ctx>,
) -> Result<(), Vec<Diagnostic>> {
    let i64_type = llvm_ctx.i64_type();
    let i8_type = llvm_ctx.i8_type();
    
    let obj_val = reg_map.get(&obj_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    
    let name_len = module_name.len();
    let array_type = i8_type.array_type((name_len + 1) as u32);
    let counter = GLOBAL_COUNTER.fetch_add(1, Ordering::SeqCst);
    let global = module.add_global(array_type, None, &format!("extend_mod_name_{}", counter));
    global.set_linkage(inkwell::module::Linkage::Private);
    let mut bytes: Vec<_> = module_name.bytes().map(|b| i8_type.const_int(b as u64, false)).collect();
    bytes.push(i8_type.const_int(0, false));
    global.set_initializer(&i8_type.const_array(&bytes));
    
    let ptr = global.as_pointer_value();
    let zero = llvm_ctx.i64_type().const_int(0, false);
    let name_ptr = unsafe { builder.build_gep(array_type, ptr, &[zero, zero], "extend_name_ptr").unwrap() };
    
    let fn_val = get_runtime_fn_value(module, "jdruby_extend_module").unwrap();
    builder.build_call(fn_val, &[obj_val.into(), name_ptr.into()], "").unwrap();
    Ok(())
}

// =========================================================================
// BLOCK/CLOSURE OPERATIONS
// =========================================================================

fn emit_block_create<'ctx>(
    func_symbol: &str,
    captured_vars: &[u32],
    _is_lambda: bool,
    _ctx: &CodegenContext<'ctx>,
    llvm_ctx: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
    reg_map: &RegMap<'ctx>,
) -> Result<BasicValueEnum<'ctx>, Vec<Diagnostic>> {
    let i64_type = llvm_ctx.i64_type();
    let i8_type = llvm_ctx.i8_type();
    let i64_ptr_type = llvm_ctx.ptr_type(AddressSpace::default());
    
    // Function name
    let func_len = func_symbol.len();
    let func_array_type = i8_type.array_type((func_len + 1) as u32);
    let counter = GLOBAL_COUNTER.fetch_add(1, Ordering::SeqCst);
    let func_global = module.add_global(func_array_type, None, &format!("block_func_{}", counter));
    func_global.set_linkage(inkwell::module::Linkage::Private);
    let mut func_bytes: Vec<_> = func_symbol.bytes().map(|b| i8_type.const_int(b as u64, false)).collect();
    func_bytes.push(i8_type.const_int(0, false));
    func_global.set_initializer(&i8_type.const_array(&func_bytes));
    
    let zero = llvm_ctx.i64_type().const_int(0, false);
    let func_ptr = func_global.as_pointer_value();
    let func_name_ptr = unsafe { builder.build_gep(func_array_type, func_ptr, &[zero, zero], "block_func_ptr").unwrap() };
    
    // Captured vars array
    let argc_val = llvm_ctx.i32_type().const_int(captured_vars.len() as u64, false);
    let argv_ptr = if captured_vars.is_empty() {
        i64_ptr_type.const_null()
    } else {
        let argv_alloca = builder.build_alloca(i64_type.array_type(captured_vars.len() as u32), "captured_argv").unwrap();
        for (i, &var) in captured_vars.iter().enumerate() {
            let var_val = reg_map.get(&var).copied().unwrap_or(i64_type.const_int(0, false).into());
            let idx = i64_type.const_int(i as u64, false);
            let elem_ptr = unsafe { builder.build_gep(i64_type.array_type(captured_vars.len() as u32), argv_alloca, &[zero, idx], &format!("cap_{}", i)).unwrap() };
            builder.build_store(elem_ptr, var_val).unwrap();
        }
        unsafe { builder.build_gep(i64_type.array_type(captured_vars.len() as u32), argv_alloca, &[zero, zero], "captured_argv_ptr").unwrap() }
    };
    
    let fn_val = get_runtime_fn_value(module, "jdruby_block_create").unwrap();
    let val = builder.build_call(fn_val, &[func_name_ptr.into(), argc_val.into(), argv_ptr.into()], "block_create").unwrap();
    Ok(val.try_as_basic_value().unwrap_basic())
}

fn emit_proc_create<'ctx>(
    block_reg: u32,
    _ctx: &CodegenContext<'ctx>,
    llvm_ctx: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
    reg_map: &RegMap<'ctx>,
) -> Result<BasicValueEnum<'ctx>, Vec<Diagnostic>> {
    let i64_type = llvm_ctx.i64_type();
    let block_val = reg_map.get(&block_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    
    let fn_val = get_runtime_fn_value(module, "jdruby_proc_create").unwrap();
    let val = builder.build_call(fn_val, &[block_val.into()], "proc_create").unwrap();
    Ok(val.try_as_basic_value().unwrap_basic())
}

fn emit_lambda_create<'ctx>(
    block_reg: u32,
    _ctx: &CodegenContext<'ctx>,
    llvm_ctx: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
    reg_map: &RegMap<'ctx>,
) -> Result<BasicValueEnum<'ctx>, Vec<Diagnostic>> {
    let i64_type = llvm_ctx.i64_type();
    let block_val = reg_map.get(&block_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    
    let fn_val = get_runtime_fn_value(module, "jdruby_lambda_create").unwrap();
    let val = builder.build_call(fn_val, &[block_val.into()], "lambda_create").unwrap();
    Ok(val.try_as_basic_value().unwrap_basic())
}

fn emit_block_yield<'ctx>(
    block_reg: u32,
    args: &[u32],
    _ctx: &CodegenContext<'ctx>,
    llvm_ctx: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
    reg_map: &RegMap<'ctx>,
) -> Result<BasicValueEnum<'ctx>, Vec<Diagnostic>> {
    let i64_type = llvm_ctx.i64_type();
    let i64_ptr_type = llvm_ctx.ptr_type(AddressSpace::default());
    let block_val = reg_map.get(&block_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    
    let argc_val = llvm_ctx.i32_type().const_int(args.len() as u64, false);
    let argv_ptr = if args.is_empty() {
        i64_ptr_type.const_null()
    } else {
        let argv_alloca = builder.build_alloca(i64_type.array_type(args.len() as u32), "yield_argv").unwrap();
        let zero = llvm_ctx.i64_type().const_int(0, false);
        for (i, &arg) in args.iter().enumerate() {
            let arg_val = reg_map.get(&arg).copied().unwrap_or(i64_type.const_int(0, false).into());
            let idx = i64_type.const_int(i as u64, false);
            let elem_ptr = unsafe { builder.build_gep(i64_type.array_type(args.len() as u32), argv_alloca, &[zero, idx], &format!("yield_arg_{}", i)).unwrap() };
            builder.build_store(elem_ptr, arg_val).unwrap();
        }
        unsafe { builder.build_gep(i64_type.array_type(args.len() as u32), argv_alloca, &[zero, zero], "yield_argv_ptr").unwrap() }
    };
    
    let fn_val = get_runtime_fn_value(module, "jdruby_block_yield").unwrap();
    let val = builder.build_call(fn_val, &[block_val.into(), argc_val.into(), argv_ptr.into()], "block_yield").unwrap();
    Ok(val.try_as_basic_value().unwrap_basic())
}

fn emit_block_given<'ctx>(
    _ctx: &CodegenContext<'ctx>,
    _llvm_ctx: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
) -> Result<BasicValueEnum<'ctx>, Vec<Diagnostic>> {
    let fn_val = get_runtime_fn_value(module, "jdruby_block_given").unwrap();
    let val = builder.build_call(fn_val, &[], "block_given").unwrap();
    Ok(val.try_as_basic_value().unwrap_basic())
}

fn emit_current_block<'ctx>(
    _ctx: &CodegenContext<'ctx>,
    _llvm_ctx: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
) -> Result<BasicValueEnum<'ctx>, Vec<Diagnostic>> {
    let fn_val = get_runtime_fn_value(module, "jdruby_current_block").unwrap();
    let val = builder.build_call(fn_val, &[], "current_block").unwrap();
    Ok(val.try_as_basic_value().unwrap_basic())
}

// =========================================================================
// DYNAMIC METHOD OPERATIONS
// =========================================================================

fn emit_define_method_dynamic<'ctx>(
    class_reg: u32,
    name_reg: u32,
    method_func: &str,
    visibility: &jdruby_mir::MirVisibility,
    _ctx: &CodegenContext<'ctx>,
    llvm_ctx: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
    reg_map: &RegMap<'ctx>,
) -> Result<BasicValueEnum<'ctx>, Vec<Diagnostic>> {
    let i64_type = llvm_ctx.i64_type();
    let i8_type = llvm_ctx.i8_type();
    
    let class_val = reg_map.get(&class_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    let name_val = reg_map.get(&name_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    
    let func_len = method_func.len();
    let func_array_type = i8_type.array_type((func_len + 1) as u32);
    let counter = GLOBAL_COUNTER.fetch_add(1, Ordering::SeqCst);
    let func_global = module.add_global(func_array_type, None, &format!("dyn_method_func_{}", counter));
    func_global.set_linkage(inkwell::module::Linkage::Private);
    let mut func_bytes: Vec<_> = method_func.bytes().map(|b| i8_type.const_int(b as u64, false)).collect();
    func_bytes.push(i8_type.const_int(0, false));
    func_global.set_initializer(&i8_type.const_array(&func_bytes));
    
    let zero = llvm_ctx.i64_type().const_int(0, false);
    let func_ptr = func_global.as_pointer_value();
    let func_name_ptr = unsafe { builder.build_gep(func_array_type, func_ptr, &[zero, zero], "dyn_method_func_ptr").unwrap() };
    
    let vis_val = llvm_ctx.i32_type().const_int(*visibility as u32 as u64, false);
    
    let fn_val = get_runtime_fn_value(module, "jdruby_define_method_dynamic").unwrap();
    let val = builder.build_call(fn_val, &[class_val.into(), name_val.into(), func_name_ptr.into(), vis_val.into()], "define_method_dyn").unwrap();
    Ok(val.try_as_basic_value().unwrap_basic())
}

fn emit_undef_method<'ctx>(
    class_reg: u32,
    name_reg: u32,
    _ctx: &CodegenContext<'ctx>,
    llvm_ctx: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
    reg_map: &RegMap<'ctx>,
) -> Result<BasicValueEnum<'ctx>, Vec<Diagnostic>> {
    let i64_type = llvm_ctx.i64_type();
    let class_val = reg_map.get(&class_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    let name_val = reg_map.get(&name_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    
    let fn_val = get_runtime_fn_value(module, "jdruby_undef_method").unwrap();
    let val = builder.build_call(fn_val, &[class_val.into(), name_val.into()], "undef_method").unwrap();
    Ok(val.try_as_basic_value().unwrap_basic())
}

fn emit_remove_method<'ctx>(
    class_reg: u32,
    name_reg: u32,
    _ctx: &CodegenContext<'ctx>,
    llvm_ctx: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
    reg_map: &RegMap<'ctx>,
) -> Result<BasicValueEnum<'ctx>, Vec<Diagnostic>> {
    let i64_type = llvm_ctx.i64_type();
    let class_val = reg_map.get(&class_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    let name_val = reg_map.get(&name_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    
    let fn_val = get_runtime_fn_value(module, "jdruby_remove_method").unwrap();
    let val = builder.build_call(fn_val, &[class_val.into(), name_val.into()], "remove_method").unwrap();
    Ok(val.try_as_basic_value().unwrap_basic())
}

fn emit_alias_method<'ctx>(
    class_reg: u32,
    new_name_reg: u32,
    old_name_reg: u32,
    _ctx: &CodegenContext<'ctx>,
    llvm_ctx: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
    reg_map: &RegMap<'ctx>,
) -> Result<BasicValueEnum<'ctx>, Vec<Diagnostic>> {
    let i64_type = llvm_ctx.i64_type();
    let class_val = reg_map.get(&class_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    let new_name_val = reg_map.get(&new_name_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    let old_name_val = reg_map.get(&old_name_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    
    let fn_val = get_runtime_fn_value(module, "jdruby_alias_method").unwrap();
    let val = builder.build_call(fn_val, &[class_val.into(), new_name_val.into(), old_name_val.into()], "alias_method").unwrap();
    Ok(val.try_as_basic_value().unwrap_basic())
}

fn emit_set_visibility<'ctx>(
    class_reg: u32,
    visibility: &jdruby_mir::MirVisibility,
    method_names: &[u32],
    _ctx: &CodegenContext<'ctx>,
    llvm_ctx: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
    reg_map: &RegMap<'ctx>,
) -> Result<BasicValueEnum<'ctx>, Vec<Diagnostic>> {
    let i64_type = llvm_ctx.i64_type();
    let i64_ptr_type = llvm_ctx.ptr_type(AddressSpace::default());
    let class_val = reg_map.get(&class_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    
    let vis_val = llvm_ctx.i32_type().const_int(*visibility as u32 as u64, false);
    let argc_val = llvm_ctx.i32_type().const_int(method_names.len() as u64, false);
    let argv_ptr = if method_names.is_empty() {
        i64_ptr_type.const_null()
    } else {
        let argv_alloca = builder.build_alloca(i64_type.array_type(method_names.len() as u32), "vis_argv").unwrap();
        let zero = llvm_ctx.i64_type().const_int(0, false);
        for (i, &name) in method_names.iter().enumerate() {
            let name_val = reg_map.get(&name).copied().unwrap_or(i64_type.const_int(0, false).into());
            let idx = i64_type.const_int(i as u64, false);
            let elem_ptr = unsafe { builder.build_gep(i64_type.array_type(method_names.len() as u32), argv_alloca, &[zero, idx], &format!("vis_name_{}", i)).unwrap() };
            builder.build_store(elem_ptr, name_val).unwrap();
        }
        unsafe { builder.build_gep(i64_type.array_type(method_names.len() as u32), argv_alloca, &[zero, zero], "vis_argv_ptr").unwrap() }
    };
    
    let fn_val = get_runtime_fn_value(module, "jdruby_set_visibility").unwrap();
    let val = builder.build_call(fn_val, &[class_val.into(), vis_val.into(), argc_val.into(), argv_ptr.into()], "set_visibility").unwrap();
    Ok(val.try_as_basic_value().unwrap_basic())
}

// =========================================================================
// DYNAMIC EVALUATION
// =========================================================================

fn emit_eval<'ctx>(
    code_reg: u32,
    binding_reg: Option<u32>,
    filename_reg: Option<u32>,
    line_reg: Option<u32>,
    _ctx: &CodegenContext<'ctx>,
    llvm_ctx: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
    reg_map: &RegMap<'ctx>,
) -> Result<BasicValueEnum<'ctx>, Vec<Diagnostic>> {
    let i64_type = llvm_ctx.i64_type();
    let code_val = reg_map.get(&code_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    let binding_val = binding_reg.and_then(|r| reg_map.get(&r).copied()).unwrap_or(i64_type.const_int(0, false).into());
    let filename_val = filename_reg.and_then(|r| reg_map.get(&r).copied()).unwrap_or(i64_type.const_int(0, false).into());
    let line_val = line_reg.and_then(|r| reg_map.get(&r).copied()).unwrap_or(i64_type.const_int(0, false).into());
    
    let fn_val = get_runtime_fn_value(module, "jdruby_eval").unwrap();
    let val = builder.build_call(fn_val, &[code_val.into(), binding_val.into(), filename_val.into(), line_val.into()], "eval").unwrap();
    Ok(val.try_as_basic_value().unwrap_basic())
}

fn emit_instance_eval<'ctx>(
    obj_reg: u32,
    code_reg: u32,
    binding_reg: Option<u32>,
    _ctx: &CodegenContext<'ctx>,
    llvm_ctx: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
    reg_map: &RegMap<'ctx>,
) -> Result<BasicValueEnum<'ctx>, Vec<Diagnostic>> {
    let i64_type = llvm_ctx.i64_type();
    let obj_val = reg_map.get(&obj_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    let code_val = reg_map.get(&code_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    let binding_val = binding_reg.and_then(|r| reg_map.get(&r).copied()).unwrap_or(i64_type.const_int(0, false).into());
    
    let fn_val = get_runtime_fn_value(module, "jdruby_instance_eval").unwrap();
    let val = builder.build_call(fn_val, &[obj_val.into(), code_val.into(), binding_val.into()], "instance_eval").unwrap();
    Ok(val.try_as_basic_value().unwrap_basic())
}

fn emit_class_eval<'ctx>(
    class_reg: u32,
    code_reg: u32,
    binding_reg: Option<u32>,
    _ctx: &CodegenContext<'ctx>,
    llvm_ctx: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
    reg_map: &RegMap<'ctx>,
) -> Result<BasicValueEnum<'ctx>, Vec<Diagnostic>> {
    let i64_type = llvm_ctx.i64_type();
    let class_val = reg_map.get(&class_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    let code_val = reg_map.get(&code_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    let binding_val = binding_reg.and_then(|r| reg_map.get(&r).copied()).unwrap_or(i64_type.const_int(0, false).into());
    
    let fn_val = get_runtime_fn_value(module, "jdruby_class_eval").unwrap();
    let val = builder.build_call(fn_val, &[class_val.into(), code_val.into(), binding_val.into()], "class_eval").unwrap();
    Ok(val.try_as_basic_value().unwrap_basic())
}

fn emit_module_eval<'ctx>(
    module_reg: u32,
    code_reg: u32,
    binding_reg: Option<u32>,
    _ctx: &CodegenContext<'ctx>,
    llvm_ctx: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
    reg_map: &RegMap<'ctx>,
) -> Result<BasicValueEnum<'ctx>, Vec<Diagnostic>> {
    let i64_type = llvm_ctx.i64_type();
    let mod_val = reg_map.get(&module_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    let code_val = reg_map.get(&code_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    let binding_val = binding_reg.and_then(|r| reg_map.get(&r).copied()).unwrap_or(i64_type.const_int(0, false).into());
    
    let fn_val = get_runtime_fn_value(module, "jdruby_module_eval").unwrap();
    let val = builder.build_call(fn_val, &[mod_val.into(), code_val.into(), binding_val.into()], "module_eval").unwrap();
    Ok(val.try_as_basic_value().unwrap_basic())
}

fn emit_binding_get<'ctx>(
    _ctx: &CodegenContext<'ctx>,
    _llvm_ctx: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
) -> Result<BasicValueEnum<'ctx>, Vec<Diagnostic>> {
    let fn_val = get_runtime_fn_value(module, "jdruby_binding_get").unwrap();
    let val = builder.build_call(fn_val, &[], "binding_get").unwrap();
    Ok(val.try_as_basic_value().unwrap_basic())
}

// =========================================================================
// REFLECTION
// =========================================================================

fn emit_send<'ctx>(
    obj_reg: u32,
    name_reg: u32,
    args: &[u32],
    block_reg: Option<u32>,
    _ctx: &CodegenContext<'ctx>,
    llvm_ctx: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
    reg_map: &RegMap<'ctx>,
) -> Result<BasicValueEnum<'ctx>, Vec<Diagnostic>> {
    let i64_type = llvm_ctx.i64_type();
    let i64_ptr_type = llvm_ctx.ptr_type(AddressSpace::default());
    let obj_val = reg_map.get(&obj_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    let name_val = reg_map.get(&name_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    let block_val = block_reg.and_then(|r| reg_map.get(&r).copied()).unwrap_or(i64_type.const_int(0, false).into());
    
    let argc_val = llvm_ctx.i32_type().const_int(args.len() as u64, false);
    let argv_ptr = if args.is_empty() {
        i64_ptr_type.const_null()
    } else {
        let argv_alloca = builder.build_alloca(i64_type.array_type(args.len() as u32), "send_argv").unwrap();
        let zero = llvm_ctx.i64_type().const_int(0, false);
        for (i, &arg) in args.iter().enumerate() {
            let arg_val = reg_map.get(&arg).copied().unwrap_or(i64_type.const_int(0, false).into());
            let idx = i64_type.const_int(i as u64, false);
            let elem_ptr = unsafe { builder.build_gep(i64_type.array_type(args.len() as u32), argv_alloca, &[zero, idx], &format!("send_arg_{}", i)).unwrap() };
            builder.build_store(elem_ptr, arg_val).unwrap();
        }
        unsafe { builder.build_gep(i64_type.array_type(args.len() as u32), argv_alloca, &[zero, zero], "send_argv_ptr").unwrap() }
    };
    
    let fn_val = get_runtime_fn_value(module, "jdruby_send_dynamic").unwrap();
    let val = builder.build_call(fn_val, &[obj_val.into(), name_val.into(), argc_val.into(), argv_ptr.into(), block_val.into()], "send").unwrap();
    Ok(val.try_as_basic_value().unwrap_basic())
}

fn emit_public_send<'ctx>(
    obj_reg: u32,
    name_reg: u32,
    args: &[u32],
    block_reg: Option<u32>,
    _ctx: &CodegenContext<'ctx>,
    llvm_ctx: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
    reg_map: &RegMap<'ctx>,
) -> Result<BasicValueEnum<'ctx>, Vec<Diagnostic>> {
    let i64_type = llvm_ctx.i64_type();
    let i64_ptr_type = llvm_ctx.ptr_type(AddressSpace::default());
    let obj_val = reg_map.get(&obj_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    let name_val = reg_map.get(&name_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    let block_val = block_reg.and_then(|r| reg_map.get(&r).copied()).unwrap_or(i64_type.const_int(0, false).into());
    
    let argc_val = llvm_ctx.i32_type().const_int(args.len() as u64, false);
    let argv_ptr = if args.is_empty() {
        i64_ptr_type.const_null()
    } else {
        let argv_alloca = builder.build_alloca(i64_type.array_type(args.len() as u32), "public_send_argv").unwrap();
        let zero = llvm_ctx.i64_type().const_int(0, false);
        for (i, &arg) in args.iter().enumerate() {
            let arg_val = reg_map.get(&arg).copied().unwrap_or(i64_type.const_int(0, false).into());
            let idx = i64_type.const_int(i as u64, false);
            let elem_ptr = unsafe { builder.build_gep(i64_type.array_type(args.len() as u32), argv_alloca, &[zero, idx], &format!("psend_arg_{}", i)).unwrap() };
            builder.build_store(elem_ptr, arg_val).unwrap();
        }
        unsafe { builder.build_gep(i64_type.array_type(args.len() as u32), argv_alloca, &[zero, zero], "public_send_argv_ptr").unwrap() }
    };
    
    let fn_val = get_runtime_fn_value(module, "jdruby_public_send").unwrap();
    let val = builder.build_call(fn_val, &[obj_val.into(), name_val.into(), argc_val.into(), argv_ptr.into(), block_val.into()], "public_send").unwrap();
    Ok(val.try_as_basic_value().unwrap_basic())
}

fn emit_respond_to<'ctx>(
    obj_reg: u32,
    name_reg: u32,
    include_private: bool,
    _ctx: &CodegenContext<'ctx>,
    llvm_ctx: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
    reg_map: &RegMap<'ctx>,
) -> Result<BasicValueEnum<'ctx>, Vec<Diagnostic>> {
    let i64_type = llvm_ctx.i64_type();
    let obj_val = reg_map.get(&obj_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    let name_val = reg_map.get(&name_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    let priv_val = llvm_ctx.bool_type().const_int(if include_private { 1 } else { 0 }, false);
    
    let fn_val = get_runtime_fn_value(module, "jdruby_respond_to").unwrap();
    let val = builder.build_call(fn_val, &[obj_val.into(), name_val.into(), priv_val.into()], "respond_to").unwrap();
    Ok(val.try_as_basic_value().unwrap_basic())
}

fn emit_method_get<'ctx>(
    obj_reg: u32,
    name_reg: u32,
    _ctx: &CodegenContext<'ctx>,
    llvm_ctx: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
    reg_map: &RegMap<'ctx>,
) -> Result<BasicValueEnum<'ctx>, Vec<Diagnostic>> {
    let i64_type = llvm_ctx.i64_type();
    let obj_val = reg_map.get(&obj_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    let name_val = reg_map.get(&name_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    
    let fn_val = get_runtime_fn_value(module, "jdruby_method_get").unwrap();
    let val = builder.build_call(fn_val, &[obj_val.into(), name_val.into()], "method_get").unwrap();
    Ok(val.try_as_basic_value().unwrap_basic())
}

fn emit_instance_method_get<'ctx>(
    class_reg: u32,
    name_reg: u32,
    _ctx: &CodegenContext<'ctx>,
    llvm_ctx: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
    reg_map: &RegMap<'ctx>,
) -> Result<BasicValueEnum<'ctx>, Vec<Diagnostic>> {
    let i64_type = llvm_ctx.i64_type();
    let class_val = reg_map.get(&class_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    let name_val = reg_map.get(&name_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    
    let fn_val = get_runtime_fn_value(module, "jdruby_instance_method_get").unwrap();
    let val = builder.build_call(fn_val, &[class_val.into(), name_val.into()], "instance_method_get").unwrap();
    Ok(val.try_as_basic_value().unwrap_basic())
}

fn emit_method_object_call<'ctx>(
    method_reg: u32,
    receiver_reg: Option<u32>,
    args: &[u32],
    block_reg: Option<u32>,
    _ctx: &CodegenContext<'ctx>,
    llvm_ctx: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
    reg_map: &RegMap<'ctx>,
) -> Result<BasicValueEnum<'ctx>, Vec<Diagnostic>> {
    let i64_type = llvm_ctx.i64_type();
    let i64_ptr_type = llvm_ctx.ptr_type(AddressSpace::default());
    let method_val = reg_map.get(&method_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    let receiver_val = receiver_reg.and_then(|r| reg_map.get(&r).copied()).unwrap_or(i64_type.const_int(0, false).into());
    let block_val = block_reg.and_then(|r| reg_map.get(&r).copied()).unwrap_or(i64_type.const_int(0, false).into());
    
    let argc_val = llvm_ctx.i32_type().const_int(args.len() as u64, false);
    let argv_ptr = if args.is_empty() {
        i64_ptr_type.const_null()
    } else {
        let argv_alloca = builder.build_alloca(i64_type.array_type(args.len() as u32), "mcall_argv").unwrap();
        let zero = llvm_ctx.i64_type().const_int(0, false);
        for (i, &arg) in args.iter().enumerate() {
            let arg_val = reg_map.get(&arg).copied().unwrap_or(i64_type.const_int(0, false).into());
            let idx = i64_type.const_int(i as u64, false);
            let elem_ptr = unsafe { builder.build_gep(i64_type.array_type(args.len() as u32), argv_alloca, &[zero, idx], &format!("mcall_arg_{}", i)).unwrap() };
            builder.build_store(elem_ptr, arg_val).unwrap();
        }
        unsafe { builder.build_gep(i64_type.array_type(args.len() as u32), argv_alloca, &[zero, zero], "mcall_argv_ptr").unwrap() }
    };
    
    let fn_val = get_runtime_fn_value(module, "jdruby_method_object_call").unwrap();
    let val = builder.build_call(fn_val, &[method_val.into(), receiver_val.into(), argc_val.into(), argv_ptr.into(), block_val.into()], "method_object_call").unwrap();
    Ok(val.try_as_basic_value().unwrap_basic())
}

fn emit_method_bind<'ctx>(
    method_reg: u32,
    obj_reg: u32,
    _ctx: &CodegenContext<'ctx>,
    llvm_ctx: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
    reg_map: &RegMap<'ctx>,
) -> Result<BasicValueEnum<'ctx>, Vec<Diagnostic>> {
    let i64_type = llvm_ctx.i64_type();
    let method_val = reg_map.get(&method_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    let obj_val = reg_map.get(&obj_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    
    let fn_val = get_runtime_fn_value(module, "jdruby_method_bind").unwrap();
    let val = builder.build_call(fn_val, &[method_val.into(), obj_val.into()], "method_bind").unwrap();
    Ok(val.try_as_basic_value().unwrap_basic())
}

// =========================================================================
// DYNAMIC VARIABLE ACCESS
// =========================================================================

fn emit_ivar_get_dynamic<'ctx>(
    obj_reg: u32,
    name_reg: u32,
    _ctx: &CodegenContext<'ctx>,
    llvm_ctx: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
    reg_map: &RegMap<'ctx>,
) -> Result<BasicValueEnum<'ctx>, Vec<Diagnostic>> {
    let i64_type = llvm_ctx.i64_type();
    let obj_val = reg_map.get(&obj_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    let name_val = reg_map.get(&name_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    
    let fn_val = get_runtime_fn_value(module, "jdruby_ivar_get_dynamic").unwrap();
    let val = builder.build_call(fn_val, &[obj_val.into(), name_val.into()], "ivar_get_dyn").unwrap();
    Ok(val.try_as_basic_value().unwrap_basic())
}

fn emit_ivar_set_dynamic<'ctx>(
    obj_reg: u32,
    name_reg: u32,
    value_reg: u32,
    _ctx: &CodegenContext<'ctx>,
    llvm_ctx: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
    reg_map: &RegMap<'ctx>,
) -> Result<(), Vec<Diagnostic>> {
    let i64_type = llvm_ctx.i64_type();
    let obj_val = reg_map.get(&obj_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    let name_val = reg_map.get(&name_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    let value_val = reg_map.get(&value_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    
    let fn_val = get_runtime_fn_value(module, "jdruby_ivar_set_dynamic").unwrap();
    builder.build_call(fn_val, &[obj_val.into(), name_val.into(), value_val.into()], "").unwrap();
    Ok(())
}

fn emit_cvar_get_dynamic<'ctx>(
    class_reg: u32,
    name_reg: u32,
    _ctx: &CodegenContext<'ctx>,
    llvm_ctx: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
    reg_map: &RegMap<'ctx>,
) -> Result<BasicValueEnum<'ctx>, Vec<Diagnostic>> {
    let i64_type = llvm_ctx.i64_type();
    let class_val = reg_map.get(&class_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    let name_val = reg_map.get(&name_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    
    let fn_val = get_runtime_fn_value(module, "jdruby_cvar_get_dynamic").unwrap();
    let val = builder.build_call(fn_val, &[class_val.into(), name_val.into()], "cvar_get_dyn").unwrap();
    Ok(val.try_as_basic_value().unwrap_basic())
}

fn emit_cvar_set_dynamic<'ctx>(
    class_reg: u32,
    name_reg: u32,
    value_reg: u32,
    _ctx: &CodegenContext<'ctx>,
    llvm_ctx: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
    reg_map: &RegMap<'ctx>,
) -> Result<(), Vec<Diagnostic>> {
    let i64_type = llvm_ctx.i64_type();
    let class_val = reg_map.get(&class_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    let name_val = reg_map.get(&name_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    let value_val = reg_map.get(&value_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    
    let fn_val = get_runtime_fn_value(module, "jdruby_cvar_set_dynamic").unwrap();
    builder.build_call(fn_val, &[class_val.into(), name_val.into(), value_val.into()], "").unwrap();
    Ok(())
}

fn emit_const_get_dynamic<'ctx>(
    class_reg: u32,
    name_reg: u32,
    inherit: bool,
    _ctx: &CodegenContext<'ctx>,
    llvm_ctx: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
    reg_map: &RegMap<'ctx>,
) -> Result<BasicValueEnum<'ctx>, Vec<Diagnostic>> {
    let i64_type = llvm_ctx.i64_type();
    let class_val = reg_map.get(&class_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    let name_val = reg_map.get(&name_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    let inherit_val = llvm_ctx.bool_type().const_int(if inherit { 1 } else { 0 }, false);
    
    let fn_val = get_runtime_fn_value(module, "jdruby_const_get_dynamic").unwrap();
    let val = builder.build_call(fn_val, &[class_val.into(), name_val.into(), inherit_val.into()], "const_get_dyn").unwrap();
    Ok(val.try_as_basic_value().unwrap_basic())
}

fn emit_const_set_dynamic<'ctx>(
    class_reg: u32,
    name_reg: u32,
    value_reg: u32,
    _ctx: &CodegenContext<'ctx>,
    llvm_ctx: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
    reg_map: &RegMap<'ctx>,
) -> Result<(), Vec<Diagnostic>> {
    let i64_type = llvm_ctx.i64_type();
    let class_val = reg_map.get(&class_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    let name_val = reg_map.get(&name_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    let value_val = reg_map.get(&value_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    
    let fn_val = get_runtime_fn_value(module, "jdruby_const_set_dynamic").unwrap();
    builder.build_call(fn_val, &[class_val.into(), name_val.into(), value_val.into()], "").unwrap();
    Ok(())
}

// =========================================================================
// METHOD MISSING
// =========================================================================

fn emit_method_missing<'ctx>(
    obj_reg: u32,
    name_reg: u32,
    args: &[u32],
    block_reg: Option<u32>,
    _ctx: &CodegenContext<'ctx>,
    llvm_ctx: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
    reg_map: &RegMap<'ctx>,
) -> Result<BasicValueEnum<'ctx>, Vec<Diagnostic>> {
    let i64_type = llvm_ctx.i64_type();
    let i64_ptr_type = llvm_ctx.ptr_type(AddressSpace::default());
    let obj_val = reg_map.get(&obj_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    let name_val = reg_map.get(&name_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    let block_val = block_reg.and_then(|r| reg_map.get(&r).copied()).unwrap_or(i64_type.const_int(0, false).into());
    
    let argc_val = llvm_ctx.i32_type().const_int(args.len() as u64, false);
    let argv_ptr = if args.is_empty() {
        i64_ptr_type.const_null()
    } else {
        let argv_alloca = builder.build_alloca(i64_type.array_type(args.len() as u32), "missing_argv").unwrap();
        let zero = llvm_ctx.i64_type().const_int(0, false);
        for (i, &arg) in args.iter().enumerate() {
            let arg_val = reg_map.get(&arg).copied().unwrap_or(i64_type.const_int(0, false).into());
            let idx = i64_type.const_int(i as u64, false);
            let elem_ptr = unsafe { builder.build_gep(i64_type.array_type(args.len() as u32), argv_alloca, &[zero, idx], &format!("missing_arg_{}", i)).unwrap() };
            builder.build_store(elem_ptr, arg_val).unwrap();
        }
        unsafe { builder.build_gep(i64_type.array_type(args.len() as u32), argv_alloca, &[zero, zero], "missing_argv_ptr").unwrap() }
    };
    
    let fn_val = get_runtime_fn_value(module, "jdruby_method_missing").unwrap();
    let val = builder.build_call(fn_val, &[obj_val.into(), name_val.into(), argc_val.into(), argv_ptr.into(), block_val.into()], "method_missing").unwrap();
    Ok(val.try_as_basic_value().unwrap_basic())
}

fn emit_block_invoke<'ctx>(
    block_reg: u32,
    args: &[u32],
    splat_arg: Option<u32>,
    block_arg: Option<u32>,
    _ctx: &CodegenContext<'ctx>,
    llvm_ctx: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
    reg_map: &RegMap<'ctx>,
) -> Result<BasicValueEnum<'ctx>, Vec<Diagnostic>> {
    let i64_type = llvm_ctx.i64_type();
    let i64_ptr_type = llvm_ctx.ptr_type(AddressSpace::default());
    let block_val = reg_map.get(&block_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    let splat_val = splat_arg.and_then(|r| reg_map.get(&r).copied()).unwrap_or(i64_type.const_int(0, false).into());
    let block_arg_val = block_arg.and_then(|r| reg_map.get(&r).copied()).unwrap_or(i64_type.const_int(0, false).into());
    
    let argc_val = llvm_ctx.i32_type().const_int(args.len() as u64, false);
    let argv_ptr = if args.is_empty() {
        i64_ptr_type.const_null()
    } else {
        let argv_alloca = builder.build_alloca(i64_type.array_type(args.len() as u32), "invoke_argv").unwrap();
        let zero = llvm_ctx.i64_type().const_int(0, false);
        for (i, &arg) in args.iter().enumerate() {
            let arg_val = reg_map.get(&arg).copied().unwrap_or(i64_type.const_int(0, false).into());
            let idx = i64_type.const_int(i as u64, false);
            let elem_ptr = unsafe { builder.build_gep(i64_type.array_type(args.len() as u32), argv_alloca, &[zero, idx], &format!("invoke_arg_{}", i)).unwrap() };
            builder.build_store(elem_ptr, arg_val).unwrap();
        }
        unsafe { builder.build_gep(i64_type.array_type(args.len() as u32), argv_alloca, &[zero, zero], "invoke_argv_ptr").unwrap() }
    };
    
    let fn_val = get_runtime_fn_value(module, "jdruby_block_invoke").unwrap();
    let val = builder.build_call(fn_val, &[block_val.into(), argc_val.into(), argv_ptr.into(), splat_val.into(), block_arg_val.into()], "block_invoke").unwrap();
    Ok(val.try_as_basic_value().unwrap_basic())
}

fn emit_send_with_ic<'ctx>(
    obj_reg: u32,
    method_name: &str,
    args: &[u32],
    block_reg: Option<u32>,
    cache_slot: u32,
    _ctx: &CodegenContext<'ctx>,
    llvm_ctx: &'ctx Context,
    module: &Module<'ctx>,
    builder: &Builder<'ctx>,
    reg_map: &RegMap<'ctx>,
) -> Result<BasicValueEnum<'ctx>, Vec<Diagnostic>> {
    let i64_type = llvm_ctx.i64_type();
    let i64_ptr_type = llvm_ctx.ptr_type(AddressSpace::default());
    let i8_type = llvm_ctx.i8_type();
    let obj_val = reg_map.get(&obj_reg).copied().unwrap_or(i64_type.const_int(0, false).into());
    let block_val = block_reg.and_then(|r| reg_map.get(&r).copied()).unwrap_or(i64_type.const_int(0, false).into());
    
    // Create method name string
    let method_len = method_name.len();
    let method_array_type = i8_type.array_type((method_len + 1) as u32);
    let counter = GLOBAL_COUNTER.fetch_add(1, Ordering::SeqCst);
    let method_global = module.add_global(method_array_type, None, &format!("method_name_{}", counter));
    method_global.set_linkage(inkwell::module::Linkage::Private);
    let mut method_bytes: Vec<_> = method_name.bytes().map(|b| i8_type.const_int(b as u64, false)).collect();
    method_bytes.push(i8_type.const_int(0, false));
    method_global.set_initializer(&i8_type.const_array(&method_bytes));
    
    let zero = llvm_ctx.i64_type().const_int(0, false);
    let method_ptr = method_global.as_pointer_value();
    let method_name_ptr = unsafe { builder.build_gep(method_array_type, method_ptr, &[zero, zero], "method_name_ptr").unwrap() };
    
    let cache_val = llvm_ctx.i32_type().const_int(cache_slot as u64, false);
    
    let argc_val = llvm_ctx.i32_type().const_int(args.len() as u64, false);
    let argv_ptr = if args.is_empty() {
        i64_ptr_type.const_null()
    } else {
        let argv_alloca = builder.build_alloca(i64_type.array_type(args.len() as u32), "send_ic_argv").unwrap();
        let zero = llvm_ctx.i64_type().const_int(0, false);
        for (i, &arg) in args.iter().enumerate() {
            let arg_val = reg_map.get(&arg).copied().unwrap_or(i64_type.const_int(0, false).into());
            let idx = i64_type.const_int(i as u64, false);
            let elem_ptr = unsafe { builder.build_gep(i64_type.array_type(args.len() as u32), argv_alloca, &[zero, idx], &format!("send_ic_arg_{}", i)).unwrap() };
            builder.build_store(elem_ptr, arg_val).unwrap();
        }
        unsafe { builder.build_gep(i64_type.array_type(args.len() as u32), argv_alloca, &[zero, zero], "send_ic_argv_ptr").unwrap() }
    };
    
    let fn_val = get_runtime_fn_value(module, "jdruby_send_with_ic").unwrap();
    let val = builder.build_call(fn_val, &[obj_val.into(), method_name_ptr.into(), argc_val.into(), argv_ptr.into(), block_val.into(), cache_val.into()], "send_with_ic").unwrap();
    Ok(val.try_as_basic_value().unwrap_basic())
}

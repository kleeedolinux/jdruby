//! MIR instruction emission to LLVM IR using Inkwell.

use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::values::{BasicMetadataValueEnum, BasicValueEnum, FunctionValue, PointerValue};
use inkwell::basic_block::BasicBlock;
use jdruby_common::Diagnostic;
use jdruby_mir::{MirFunction, MirBlock, MirInst, MirTerminator, MirConst, MirBinOp, MirUnOp};
use crate::context::CodegenContext;
use crate::runtime::get_runtime_fn_value;
use crate::utils::sanitize_name;
use std::collections::HashMap;

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
    
    // Create function type: i64 @func_name(i64, i64, ...)
    let fn_type = i64_type.fn_type(&vec![i64_type.into(); func.params.len()], false);
    let fn_name = sanitize_name(&func.name);
    let function = module.add_function(&fn_name, fn_type, None);
    
    // Create entry basic block
    let entry = llvm_ctx.append_basic_block(function, "entry");
    builder.position_at_end(entry);
    
    // Map params to registers
    let mut reg_map: RegMap = HashMap::new();
    for (i, reg_id) in func.params.iter().enumerate() {
        let param = function.get_nth_param(i as u32).unwrap();
        reg_map.insert(*reg_id, param);
    }
    
    // Collect all locals that need allocas
    let locals = collect_locals(func);
    
    // Create allocas for locals
    let mut local_allocs: HashMap<String, PointerValue<'ctx>> = HashMap::new();
    for local in &locals {
        let alloca = builder.build_alloca(i64_type, &format!("local_{}", sanitize_name(local))).unwrap();
        local_allocs.insert(local.clone(), alloca);
    }
    
    // Store self if present
    if !func.params.is_empty() {
        let self_val = function.get_nth_param(0).unwrap();
        if let Some(self_alloca) = local_allocs.get("self") {
            builder.build_store(*self_alloca, self_val).unwrap();
        }
    }
    
    // Create all basic blocks first
    let mut blocks: HashMap<String, BasicBlock<'ctx>> = HashMap::new();
    for block in &func.blocks {
        let bb = llvm_ctx.append_basic_block(function, &block.label);
        blocks.insert(block.label.clone(), bb);
    }
    
    // Branch to first block
    if let Some(first_block) = func.blocks.first() {
        let target = blocks.get(&first_block.label).unwrap();
        builder.build_unconditional_branch(*target).unwrap();
    }
    
    // Emit each block
    for block in &func.blocks {
        let bb = *blocks.get(&block.label).unwrap();
        builder.position_at_end(bb);
        
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
    let i8_type = llvm_ctx.i8_type();
    
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
            let val = emit_load(name, ctx, llvm_ctx, module, builder, reg_map, local_allocs)?;
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
        MirInst::IncludeModule(class_reg, module_name) => {
            emit_include_module(*class_reg, module_name, ctx, llvm_ctx, module, builder, reg_map)?;
        }
        MirInst::Nop => {}
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
            // Get or create string constant
            let byte_len = s.len();
            let i8_type = llvm_ctx.i8_type();
            let array_type = i8_type.array_type((byte_len + 1) as u32);
            let global = module.add_global(array_type, None, "str_const");
            global.set_linkage(inkwell::module::Linkage::Private);
            global.set_unnamed_addr(true);
            
            // Create string bytes with null terminator
            let mut bytes: Vec<_> = s.bytes().map(|b| i8_type.const_int(b as u64, false)).collect();
            bytes.push(i8_type.const_int(0, false));
            let const_array = i8_type.const_array(&bytes);
            global.set_initializer(&const_array);
            
            // GEP to get pointer to first element
            let ptr = global.as_pointer_value();
            let zero = llvm_ctx.i64_type().const_int(0, false);
            let str_ptr = unsafe { builder.build_gep(i8_type, ptr, &[zero, zero], "str_ptr").unwrap() };
            
            // Call jdruby_str_new
            let fn_val = get_runtime_fn_value(module, "jdruby_str_new").unwrap();
            let len_val = i64_type.const_int(byte_len as u64, false);
            let val = builder.build_call(fn_val, &[str_ptr.into(), len_val.into()], "str_new").unwrap();
            Ok(val.try_as_basic_value().unwrap_basic())
        }
        MirConst::Symbol(s) => {
            // Similar to string but for symbols
            let byte_len = s.len();
            let i8_type = llvm_ctx.i8_type();
            let array_type = i8_type.array_type((byte_len + 1) as u32);
            let global = module.add_global(array_type, None, "sym_const");
            global.set_linkage(inkwell::module::Linkage::Private);
            global.set_unnamed_addr(true);
            
            let mut bytes: Vec<_> = s.bytes().map(|b| i8_type.const_int(b as u64, false)).collect();
            bytes.push(i8_type.const_int(0, false));
            let const_array = i8_type.const_array(&bytes);
            global.set_initializer(&const_array);
            
            let ptr = global.as_pointer_value();
            let zero = llvm_ctx.i64_type().const_int(0, false);
            let sym_ptr = unsafe { builder.build_gep(i8_type, ptr, &[zero, zero], "sym_ptr").unwrap() };
            
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
            // Default: call jdruby_send with self
            let fn_val = get_runtime_fn_value(module, "jdruby_send").unwrap();
            let self_alloca = local_allocs.get("self").copied().unwrap();
            let self_val = builder.build_load(i64_type, self_alloca, "self").unwrap();
            
            // Create method name string
            let i8_type = llvm_ctx.i8_type();
            let name_len = name.len();
            let array_type = i8_type.array_type((name_len + 1) as u32);
            let global = module.add_global(array_type, None, "meth_name");
            global.set_linkage(inkwell::module::Linkage::Private);
            let mut bytes: Vec<_> = name.bytes().map(|b| i8_type.const_int(b as u64, false)).collect();
            bytes.push(i8_type.const_int(0, false));
            global.set_initializer(&i8_type.const_array(&bytes));
            
            let ptr = global.as_pointer_value();
            let zero = llvm_ctx.i64_type().const_int(0, false);
            let meth_ptr = unsafe { builder.build_gep(i8_type, ptr, &[zero, zero], "meth_ptr").unwrap() };
            
            let argc_val = llvm_ctx.i32_type().const_int(args.len() as u64, false);
            let mut all_args: Vec<_> = vec![self_val.into(), meth_ptr.into(), argc_val.into()];
            for &arg in args {
                let arg_val = reg_map.get(&arg).copied().unwrap_or(i64_type.const_int(0, false).into());
                all_args.push(arg_val.into());
            }
            
            let val = builder.build_call(fn_val, all_args.as_slice(), "send").unwrap();
            Ok(val.try_as_basic_value().unwrap_basic())
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
    let i8_type = llvm_ctx.i8_type();
    
    let fn_val = get_runtime_fn_value(module, "jdruby_send").unwrap();
    let recv_val = reg_map.get(&recv).copied().unwrap_or(i64_type.const_int(0, false).into());
    
    // Create method name string
    let name_len = method.len();
    let array_type = i8_type.array_type((name_len + 1) as u32);
    let global = module.add_global(array_type, None, "meth_name");
    global.set_linkage(inkwell::module::Linkage::Private);
    let mut bytes: Vec<_> = method.bytes().map(|b| i8_type.const_int(b as u64, false)).collect();
    bytes.push(i8_type.const_int(0, false));
    global.set_initializer(&i8_type.const_array(&bytes));
    
    let ptr = global.as_pointer_value();
    let zero = llvm_ctx.i64_type().const_int(0, false);
    let meth_ptr = unsafe { builder.build_gep(i8_type, ptr, &[zero, zero], "meth_ptr").unwrap() };
    
    let argc_val = llvm_ctx.i32_type().const_int(args.len() as u64, false);
    let mut all_args: Vec<_> = vec![recv_val.into(), meth_ptr.into(), argc_val.into()];
    for &arg in args {
        let arg_val = reg_map.get(&arg).copied().unwrap_or(i64_type.const_int(0, false).into());
        all_args.push(arg_val.into());
    }
    
    let val = builder.build_call(fn_val, all_args.as_slice(), "send").unwrap();
    Ok(val.try_as_basic_value().unwrap_basic())
}

fn emit_load<'ctx>(
    name: &str,
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
        // Constant/global lookup
        let name_len = name.len();
        let array_type = i8_type.array_type((name_len + 1) as u32);
        let global = module.add_global(array_type, None, "const_name");
        global.set_linkage(inkwell::module::Linkage::Private);
        let mut bytes: Vec<_> = name.bytes().map(|b| i8_type.const_int(b as u64, false)).collect();
        bytes.push(i8_type.const_int(0, false));
        global.set_initializer(&i8_type.const_array(&bytes));
        
        let ptr = global.as_pointer_value();
        let zero = llvm_ctx.i64_type().const_int(0, false);
        let const_ptr = unsafe { builder.build_gep(i8_type, ptr, &[zero, zero], "const_ptr").unwrap() };
        
        let fn_val = get_runtime_fn_value(module, "jdruby_const_get").unwrap();
        let val = builder.build_call(fn_val, &[const_ptr.into()], "const_get").unwrap();
        Ok(val.try_as_basic_value().unwrap_basic())
    } else if name.starts_with('@') {
        // Instance variable
        let self_alloca = local_allocs.get("self").copied().unwrap();
        let self_val = builder.build_load(i64_type, self_alloca, "self").unwrap();
        
        let name_len = name.len();
        let array_type = i8_type.array_type((name_len + 1) as u32);
        let global = module.add_global(array_type, None, "ivar_name");
        global.set_linkage(inkwell::module::Linkage::Private);
        let mut bytes: Vec<_> = name.bytes().map(|b| i8_type.const_int(b as u64, false)).collect();
        bytes.push(i8_type.const_int(0, false));
        global.set_initializer(&i8_type.const_array(&bytes));
        
        let ptr = global.as_pointer_value();
        let zero = llvm_ctx.i64_type().const_int(0, false);
        let ivar_ptr = unsafe { builder.build_gep(i8_type, ptr, &[zero, zero], "ivar_ptr").unwrap() };
        
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
        let global = module.add_global(array_type, None, "ivar_name");
        global.set_linkage(inkwell::module::Linkage::Private);
        let mut bytes: Vec<_> = name.bytes().map(|b| i8_type.const_int(b as u64, false)).collect();
        bytes.push(i8_type.const_int(0, false));
        global.set_initializer(&i8_type.const_array(&bytes));
        
        let ptr = global.as_pointer_value();
        let zero = llvm_ctx.i64_type().const_int(0, false);
        let ivar_ptr = unsafe { builder.build_gep(i8_type, ptr, &[zero, zero], "ivar_ptr").unwrap() };
        
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
    
    // Class name
    let name_len = name.len();
    let array_type = i8_type.array_type((name_len + 1) as u32);
    let global = module.add_global(array_type, None, "class_name");
    global.set_linkage(inkwell::module::Linkage::Private);
    let mut bytes: Vec<_> = name.bytes().map(|b| i8_type.const_int(b as u64, false)).collect();
    bytes.push(i8_type.const_int(0, false));
    global.set_initializer(&i8_type.const_array(&bytes));
    
    let ptr = global.as_pointer_value();
    let zero = llvm_ctx.i64_type().const_int(0, false);
    let name_ptr = unsafe { builder.build_gep(i8_type, ptr, &[zero, zero], "name_ptr").unwrap() };
    
    // Superclass
    let sc_val = if let Some(sc) = superclass {
        let sc_len = sc.len();
        let sc_array_type = i8_type.array_type((sc_len + 1) as u32);
        let sc_global = module.add_global(sc_array_type, None, "sc_name");
        sc_global.set_linkage(inkwell::module::Linkage::Private);
        let mut sc_bytes: Vec<_> = sc.bytes().map(|b| i8_type.const_int(b as u64, false)).collect();
        sc_bytes.push(i8_type.const_int(0, false));
        sc_global.set_initializer(&i8_type.const_array(&sc_bytes));
        
        let sc_ptr = sc_global.as_pointer_value();
        let sc_name_ptr = unsafe { builder.build_gep(i8_type, sc_ptr, &[zero, zero], "sc_name_ptr").unwrap() };
        
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
    
    // Method name
    let meth_len = method_name.len();
    let meth_array_type = i8_type.array_type((meth_len + 1) as u32);
    let meth_global = module.add_global(meth_array_type, None, "def_meth_name");
    meth_global.set_linkage(inkwell::module::Linkage::Private);
    let mut meth_bytes: Vec<_> = method_name.bytes().map(|b| i8_type.const_int(b as u64, false)).collect();
    meth_bytes.push(i8_type.const_int(0, false));
    meth_global.set_initializer(&i8_type.const_array(&meth_bytes));
    
    let meth_ptr = meth_global.as_pointer_value();
    let zero = llvm_ctx.i64_type().const_int(0, false);
    let meth_name_ptr = unsafe { builder.build_gep(i8_type, meth_ptr, &[zero, zero], "meth_name_ptr").unwrap() };
    
    // Function name
    let func_len = func_name.len();
    let func_array_type = i8_type.array_type((func_len + 1) as u32);
    let func_global = module.add_global(func_array_type, None, "def_func_name");
    func_global.set_linkage(inkwell::module::Linkage::Private);
    let mut func_bytes: Vec<_> = func_name.bytes().map(|b| i8_type.const_int(b as u64, false)).collect();
    func_bytes.push(i8_type.const_int(0, false));
    func_global.set_initializer(&i8_type.const_array(&func_bytes));
    
    let func_ptr = func_global.as_pointer_value();
    let func_name_ptr = unsafe { builder.build_gep(i8_type, func_ptr, &[zero, zero], "func_name_ptr").unwrap() };
    
    let fn_val = get_runtime_fn_value(module, "jdruby_def_method").unwrap();
    builder.build_call(fn_val, &[class_val.into(), meth_name_ptr.into(), func_name_ptr.into()], "").unwrap();
    
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
    
    // Module name
    let mod_len = module_name.len();
    let mod_array_type = i8_type.array_type((mod_len + 1) as u32);
    let mod_global = module.add_global(mod_array_type, None, "inc_mod_name");
    mod_global.set_linkage(inkwell::module::Linkage::Private);
    let mut mod_bytes: Vec<_> = module_name.bytes().map(|b| i8_type.const_int(b as u64, false)).collect();
    mod_bytes.push(i8_type.const_int(0, false));
    mod_global.set_initializer(&i8_type.const_array(&mod_bytes));
    
    let mod_ptr = mod_global.as_pointer_value();
    let zero = llvm_ctx.i64_type().const_int(0, false);
    let mod_name_ptr = unsafe { builder.build_gep(i8_type, mod_ptr, &[zero, zero], "mod_name_ptr").unwrap() };
    
    let const_fn = get_runtime_fn_value(module, "jdruby_const_get").unwrap();
    let mod_val = builder.build_call(const_fn, &[mod_name_ptr.into()], "mod_get").unwrap()
        .try_as_basic_value().unwrap_basic();
    
    // Include method name
    let incl_name = "include";
    let incl_len = incl_name.len();
    let incl_array_type = i8_type.array_type((incl_len + 1) as u32);
    let incl_global = module.add_global(incl_array_type, None, "inc_name");
    incl_global.set_linkage(inkwell::module::Linkage::Private);
    let mut incl_bytes: Vec<_> = incl_name.bytes().map(|b| i8_type.const_int(b as u64, false)).collect();
    incl_bytes.push(i8_type.const_int(0, false));
    incl_global.set_initializer(&i8_type.const_array(&incl_bytes));
    
    let incl_ptr = incl_global.as_pointer_value();
    let incl_name_ptr = unsafe { builder.build_gep(i8_type, incl_ptr, &[zero, zero], "incl_name_ptr").unwrap() };
    
    let fn_val = get_runtime_fn_value(module, "jdruby_send").unwrap();
    let argc_val = llvm_ctx.i32_type().const_int(1, false);
    builder.build_call(fn_val, &[class_val.into(), incl_name_ptr.into(), argc_val.into(), mod_val.into()], "").unwrap();
    
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

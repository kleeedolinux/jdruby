//! MIR instruction emission to LLVM IR.

use jdruby_common::Diagnostic;
use jdruby_mir::{MirFunction, MirBlock, MirInst, MirTerminator, MirConst, MirBinOp, MirUnOp};
use crate::context::CodegenContext;
use crate::utils::sanitize_name;

/// Emit a function to LLVM IR.
pub fn emit_function(
    func: &MirFunction,
    ctx: &CodegenContext,
    out: &mut String,
) -> Result<(), Vec<Diagnostic>> {
    let params: Vec<String> = func
        .params
        .iter()
        .map(|r| format!("i64 %r{}", r))
        .collect();

    out.push_str(&format!(
        "define i64 @{}({}) {{\n",
        sanitize_name(&func.name),
        params.join(", ")
    ));

    emit_allocas(func, out);

    for block in &func.blocks {
        emit_block(block, ctx, out);
    }

    out.push_str("}\n\n");
    Ok(())
}

fn emit_allocas(func: &MirFunction, out: &mut String) {
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

    out.push_str("entry:\n");
    for local in &locals {
        out.push_str(&format!(
            "  %local_{} = alloca i64, align 8\n",
            sanitize_name(local)
        ));
    }

    if has_self {
        out.push_str(&format!(
            "  store i64 %r{}, i64* %local_self, align 8\n",
            func.params[0]
        ));
    }

    if let Some(first) = func.blocks.first() {
        out.push_str(&format!("  br label %{}\n\n", first.label));
    } else {
        out.push_str("  ret i64 0\n}\n\n");
    }
}

fn emit_block(block: &MirBlock, ctx: &CodegenContext, out: &mut String) {
    out.push_str(&format!("{}:\n", block.label));
    for inst in &block.instructions {
        emit_instruction(inst, ctx, out);
    }
    emit_terminator(&block.terminator, out);
}

fn emit_instruction(inst: &MirInst, ctx: &CodegenContext, out: &mut String) {
    match inst {
        MirInst::LoadConst(reg, c) => emit_load_const(*reg, c, ctx, out),
        MirInst::Copy(dest, src) => {
            out.push_str(&format!("  %r{} = add i64 %r{}, 0\n", dest, src));
        }
        MirInst::BinOp(dest, op, left, right) => emit_bin_op(*dest, op, *left, *right, out),
        MirInst::UnOp(dest, op, src) => emit_un_op(*dest, op, *src, out),
        MirInst::Call(dest, name, args) => emit_call(*dest, name, args, ctx, out),
        MirInst::MethodCall(dest, recv, method, args) => {
            emit_method_call(*dest, *recv, method, args, ctx, out)
        }
        MirInst::Load(reg, name) => emit_load(*reg, name, ctx, out),
        MirInst::Store(name, reg) => emit_store(name, *reg, ctx, out),
        MirInst::Alloc(reg, name) => {
            out.push_str(&format!(
                "  %alloca_{} = alloca i64, align 8\n",
                reg
            ));
            out.push_str(&format!(
                "  %r{} = ptrtoint i64* %alloca_{} to i64\n",
                reg, reg
            ));
        }
        MirInst::ClassNew(dest, name, superclass) => {
            emit_class_new(*dest, name, superclass.as_deref(), ctx, out)
        }
        MirInst::DefMethod(class_reg, method_name, func_name) => {
            emit_def_method(*class_reg, method_name, func_name, ctx, out)
        }
        MirInst::IncludeModule(class_reg, module_name) => {
            emit_include_module(*class_reg, module_name, ctx, out)
        }
        MirInst::Nop => {}
    }
}

fn emit_load_const(reg: u32, c: &MirConst, ctx: &CodegenContext, out: &mut String) {
    match c {
        MirConst::Integer(v) => {
            out.push_str(&format!(
                "  %r{} = call i64 @jdruby_int_new(i64 {})\n",
                reg, v
            ));
        }
        MirConst::Float(v) => {
            out.push_str(&format!(
                "  %r{} = call i64 @jdruby_float_new(double {:.15e})\n",
                reg, v
            ));
        }
        MirConst::Bool(true) => {
            out.push_str(&format!(
                "  %r{} = load i64, i64* @JDRUBY_TRUE, align 8\n",
                reg
            ));
        }
        MirConst::Bool(false) => {
            out.push_str(&format!(
                "  %r{} = load i64, i64* @JDRUBY_FALSE, align 8\n",
                reg
            ));
        }
        MirConst::Nil => {
            out.push_str(&format!(
                "  %r{} = load i64, i64* @JDRUBY_NIL, align 8\n",
                reg
            ));
        }
        MirConst::String(s) => {
            if let Some(const_name) = ctx.get_string_constant(s) {
                let byte_len = s.len();
                out.push_str(&format!(
                    "  %str_ptr_{} = getelementptr inbounds [{} x i8], [{} x i8]* @{}, i64 0, i64 0\n",
                    reg, byte_len + 1, byte_len + 1, const_name
                ));
                out.push_str(&format!(
                    "  %r{} = call i64 @jdruby_str_new(i8* %str_ptr_{}, i64 {})\n",
                    reg, reg, byte_len
                ));
            }
        }
        MirConst::Symbol(s) => {
            if let Some(const_name) = ctx.get_string_constant(s) {
                out.push_str(&format!(
                    "  %sym_ptr_{} = getelementptr inbounds [{} x i8], [{} x i8]* @{}, i64 0, i64 0\n",
                    reg, s.len() + 1, s.len() + 1, const_name
                ));
                out.push_str(&format!(
                    "  %r{} = call i64 @jdruby_sym_intern(i8* %sym_ptr_{})\n",
                    reg, reg
                ));
            }
        }
    }
}

fn emit_bin_op(dest: u32, op: &MirBinOp, left: u32, right: u32, out: &mut String) {
    match op {
        MirBinOp::Add => out.push_str(&format!(
            "  %r{} = call i64 @jdruby_int_add(i64 %r{}, i64 %r{})\n",
            dest, left, right
        )),
        MirBinOp::Sub => out.push_str(&format!(
            "  %r{} = call i64 @jdruby_int_sub(i64 %r{}, i64 %r{})\n",
            dest, left, right
        )),
        MirBinOp::Mul => out.push_str(&format!(
            "  %r{} = call i64 @jdruby_int_mul(i64 %r{}, i64 %r{})\n",
            dest, left, right
        )),
        MirBinOp::Div => out.push_str(&format!(
            "  %r{} = call i64 @jdruby_int_div(i64 %r{}, i64 %r{})\n",
            dest, left, right
        )),
        MirBinOp::Mod => out.push_str(&format!(
            "  %r{} = call i64 @jdruby_int_mod(i64 %r{}, i64 %r{})\n",
            dest, left, right
        )),
        MirBinOp::Pow => out.push_str(&format!(
            "  %r{} = call i64 @jdruby_int_pow(i64 %r{}, i64 %r{})\n",
            dest, left, right
        )),
        MirBinOp::Eq => {
            out.push_str(&format!(
                "  %eq_{} = call i1 @jdruby_eq(i64 %r{}, i64 %r{})\n",
                dest, left, right
            ));
            out.push_str(&format!(
                "  %r{} = call i64 @jdruby_bool(i1 %eq_{})\n",
                dest, dest
            ));
        }
        MirBinOp::NotEq => {
            out.push_str(&format!(
                "  %neq_{} = call i1 @jdruby_eq(i64 %r{}, i64 %r{})\n",
                dest, left, right
            ));
            out.push_str(&format!(
                "  %neq_inv_{} = xor i1 %neq_{}, true\n",
                dest, dest
            ));
            out.push_str(&format!(
                "  %r{} = call i64 @jdruby_bool(i1 %neq_inv_{})\n",
                dest, dest
            ));
        }
        MirBinOp::Lt => {
            out.push_str(&format!(
                "  %lt_{} = call i1 @jdruby_lt(i64 %r{}, i64 %r{})\n",
                dest, left, right
            ));
            out.push_str(&format!(
                "  %r{} = call i64 @jdruby_bool(i1 %lt_{})\n",
                dest, dest
            ));
        }
        MirBinOp::Gt => {
            out.push_str(&format!(
                "  %gt_{} = call i1 @jdruby_gt(i64 %r{}, i64 %r{})\n",
                dest, left, right
            ));
            out.push_str(&format!(
                "  %r{} = call i64 @jdruby_bool(i1 %gt_{})\n",
                dest, dest
            ));
        }
        MirBinOp::LtEq => {
            out.push_str(&format!(
                "  %le_{} = call i1 @jdruby_le(i64 %r{}, i64 %r{})\n",
                dest, left, right
            ));
            out.push_str(&format!(
                "  %r{} = call i64 @jdruby_bool(i1 %le_{})\n",
                dest, dest
            ));
        }
        MirBinOp::GtEq => {
            out.push_str(&format!(
                "  %ge_{} = call i1 @jdruby_ge(i64 %r{}, i64 %r{})\n",
                dest, left, right
            ));
            out.push_str(&format!(
                "  %r{} = call i64 @jdruby_bool(i1 %ge_{})\n",
                dest, dest
            ));
        }
        MirBinOp::And => {
            out.push_str(&format!(
                "  %and_test_{} = call i1 @jdruby_truthy(i64 %r{})\n",
                dest, left
            ));
            out.push_str(&format!(
                "  %r{} = select i1 %and_test_{}, i64 %r{}, i64 %r{}\n",
                dest, dest, right, left
            ));
        }
        MirBinOp::Or => {
            out.push_str(&format!(
                "  %or_test_{} = call i1 @jdruby_truthy(i64 %r{})\n",
                dest, left
            ));
            out.push_str(&format!(
                "  %r{} = select i1 %or_test_{}, i64 %r{}, i64 %r{}\n",
                dest, dest, left, right
            ));
        }
        MirBinOp::BitAnd => {
            out.push_str(&format!(
                "  %r{} = and i64 %r{}, %r{}\n",
                dest, left, right
            ));
        }
        MirBinOp::BitOr => {
            out.push_str(&format!("  %r{} = or i64 %r{}, %r{}\n", dest, left, right));
        }
        MirBinOp::BitXor => {
            out.push_str(&format!(
                "  %r{} = xor i64 %r{}, %r{}\n",
                dest, left, right
            ));
        }
        MirBinOp::Shl => {
            out.push_str(&format!(
                "  %r{} = shl i64 %r{}, %r{}\n",
                dest, left, right
            ));
        }
        MirBinOp::Shr => {
            out.push_str(&format!(
                "  %r{} = ashr i64 %r{}, %r{}\n",
                dest, left, right
            ));
        }
        MirBinOp::Cmp => {
            out.push_str(&format!(
                "  %cmp_lt_{} = call i1 @jdruby_lt(i64 %r{}, i64 %r{})\n",
                dest, left, right
            ));
            out.push_str(&format!(
                "  %cmp_gt_{} = call i1 @jdruby_gt(i64 %r{}, i64 %r{})\n",
                dest, left, right
            ));
            out.push_str(&format!(
                "  %cmp_sel1_{} = select i1 %cmp_lt_{}, i64 -1, i64 0\n",
                dest, dest
            ));
            out.push_str(&format!(
                "  %r{} = select i1 %cmp_gt_{}, i64 1, i64 %cmp_sel1_{}\n",
                dest, dest, dest
            ));
        }
    }
}

fn emit_un_op(dest: u32, op: &MirUnOp, src: u32, out: &mut String) {
    match op {
        MirUnOp::Neg => {
            out.push_str(&format!("  %r{} = sub i64 0, %r{}\n", dest, src));
        }
        MirUnOp::Not => {
            out.push_str(&format!(
                "  %not_{} = call i1 @jdruby_truthy(i64 %r{})\n",
                dest, src
            ));
            out.push_str(&format!(
                "  %not_inv_{} = xor i1 %not_{}, true\n",
                dest, dest
            ));
            out.push_str(&format!(
                "  %r{} = call i64 @jdruby_bool(i1 %not_inv_{})\n",
                dest, dest
            ));
        }
        MirUnOp::BitNot => {
            out.push_str(&format!("  %r{} = xor i64 %r{}, -1\n", dest, src));
        }
    }
}

fn emit_call(dest: u32, name: &str, args: &[u32], ctx: &CodegenContext, out: &mut String) {
    match name {
        "puts" => {
            for &arg in args {
                out.push_str(&format!(
                    "  call void @jdruby_puts(i64 %r{})\n",
                    arg
                ));
            }
            out.push_str(&format!(
                "  %r{} = load i64, i64* @JDRUBY_NIL, align 8\n",
                dest
            ));
        }
        "print" => {
            for &arg in args {
                out.push_str(&format!(
                    "  call void @jdruby_print(i64 %r{})\n",
                    arg
                ));
            }
            out.push_str(&format!(
                "  %r{} = load i64, i64* @JDRUBY_NIL, align 8\n",
                dest
            ));
        }
        "p" => {
            if let Some(&first) = args.first() {
                out.push_str(&format!(
                    "  %r{} = call i64 @jdruby_p(i64 %r{})\n",
                    dest, first
                ));
            } else {
                out.push_str(&format!(
                    "  %r{} = load i64, i64* @JDRUBY_NIL, align 8\n",
                    dest
                ));
            }
        }
        "rb_ary_new" | "jdruby_ary_new" => {
            let arg_list: Vec<String> = args.iter().map(|r| format!("i64 %r{}", r)).collect();
            let argc = args.len() as i32;
            out.push_str(&format!(
                "  %r{} = call i64 (i32, ...) @jdruby_ary_new(i32 {}{})\n",
                dest,
                argc,
                if argc > 0 {
                    format!(", {}", arg_list.join(", "))
                } else {
                    "".to_string()
                }
            ));
        }
        _ => {
            let method_str = ctx.get_string_constant(name);
            if method_str.is_some() {
                out.push_str(&format!(
                    "  %self_for_call_{} = load i64, i64* %local_self, align 8\n",
                    dest
                ));
            }
            let argc = args.len() as i32;
            let mut arg_str = format!("i64 %self_for_call_{}, i8* %meth_ptr_{}, i32 {}", dest, dest, argc);
            for arg in args {
                arg_str.push_str(&format!(", i64 %r{}", arg));
            }
            out.push_str(&format!(
                "  %r{} = call i64 (i64, i8*, i32, ...) @jdruby_send({})\n",
                dest, arg_str
            ));
        }
    }
}

fn emit_method_call(
    dest: u32,
    recv: u32,
    method: &str,
    args: &[u32],
    ctx: &CodegenContext,
    out: &mut String,
) {
    if let Some(method_str) = ctx.get_string_constant(method) {
        let mlen = method.len() + 1;
        out.push_str(&format!(
            "  %meth_ptr_{} = getelementptr inbounds [{} x i8], [{} x i8]* @{}, i64 0, i64 0\n",
            dest, mlen, mlen, method_str
        ));
    }

    let argc = args.len() as i32;
    let mut arg_str = format!("i64 %r{}, i8* %meth_ptr_{}, i32 {}", recv, dest, argc);
    for arg in args {
        arg_str.push_str(&format!(", i64 %r{}", arg));
    }
    out.push_str(&format!(
        "  %r{} = call i64 (i64, i8*, i32, ...) @jdruby_send({})\n",
        dest, arg_str
    ));
}

fn emit_load(reg: u32, name: &str, ctx: &CodegenContext, out: &mut String) {
    if name.starts_with(|c: char| c.is_ascii_uppercase()) || name.starts_with('$') {
        if let Some(const_name) = ctx.get_string_constant(name) {
            let name_len = name.len() + 1;
            out.push_str(&format!(
                "  %const_ptr_{} = getelementptr inbounds [{} x i8], [{} x i8]* @{}, i64 0, i64 0\n",
                reg, name_len, name_len, const_name
            ));
            out.push_str(&format!(
                "  %r{} = call i64 @jdruby_const_get(i8* %const_ptr_{})\n",
                reg, reg
            ));
        }
    } else if name.starts_with('@') {
        if let Some(ivar_str) = ctx.get_string_constant(name) {
            let ilen = name.len() + 1;
            out.push_str(&format!(
                "  %self_for_{} = load i64, i64* %local_self, align 8\n",
                reg
            ));
            out.push_str(&format!(
                "  %ivar_str_{} = getelementptr inbounds [{} x i8], [{} x i8]* @{}, i64 0, i64 0\n",
                reg, ilen, ilen, ivar_str
            ));
            out.push_str(&format!(
                "  %r{} = call i64 @jdruby_ivar_get(i64 %self_for_{}, i8* %ivar_str_{})\n",
                reg, reg, reg
            ));
        }
    } else {
        out.push_str(&format!(
            "  %r{} = load i64, i64* %local_{}, align 8\n",
            reg,
            sanitize_name(name)
        ));
    }
}

fn emit_store(name: &str, reg: u32, ctx: &CodegenContext, out: &mut String) {
    if name.starts_with(|c: char| c.is_ascii_uppercase()) || name.starts_with('$') {
        out.push_str(&format!(
            "  store i64 %r{}, i64* @{}, align 8\n",
            reg,
            sanitize_name(name)
        ));
    } else if name.starts_with('@') {
        if let Some(ivar_str) = ctx.get_string_constant(name) {
            let ilen = name.len() + 1;
            out.push_str(&format!(
                "  %self_for_{} = load i64, i64* %local_self, align 8\n",
                reg
            ));
            out.push_str(&format!(
                "  %ivar_str_{} = getelementptr inbounds [{} x i8], [{} x i8]* @{}, i64 0, i64 0\n",
                reg, ilen, ilen, ivar_str
            ));
            out.push_str(&format!(
                "  call void @jdruby_ivar_set(i64 %self_for_{}, i8* %ivar_str_{}, i64 %r{})\n",
                reg, reg, reg
            ));
        }
    } else {
        out.push_str(&format!(
            "  store i64 %r{}, i64* %local_{}, align 8\n",
            reg,
            sanitize_name(name)
        ));
    }
}

fn emit_class_new(
    dest: u32,
    name: &str,
    superclass: Option<&str>,
    ctx: &CodegenContext,
    out: &mut String,
) {
    if let Some(name_const) = ctx.get_string_constant(name) {
        let name_len = name.len() + 1;
        out.push_str(&format!(
            "  %cls_name_{} = getelementptr inbounds [{} x i8], [{} x i8]* @{}, i64 0, i64 0\n",
            dest, name_len, name_len, name_const
        ));

        if let Some(sc) = superclass {
            if let Some(sc_const) = ctx.get_string_constant(sc) {
                let sc_len = sc.len() + 1;
                out.push_str(&format!(
                    "  %sc_name_{} = getelementptr inbounds [{} x i8], [{} x i8]* @{}, i64 0, i64 0\n",
                    dest, sc_len, sc_len, sc_const
                ));
                out.push_str(&format!(
                    "  %sc_val_{} = call i64 @jdruby_const_get(i8* %sc_name_{})\n",
                    dest, dest
                ));
            }
        } else {
            out.push_str(&format!(
                "  %sc_val_{} = load i64, i64* @JDRUBY_NIL, align 8\n",
                dest
            ));
        }

        out.push_str(&format!(
            "  %r{} = call i64 @jdruby_class_new(i8* %cls_name_{}, i64 %sc_val_{})\n",
            dest, dest, dest
        ));
    }
}

fn emit_def_method(
    class_reg: u32,
    method_name: &str,
    func_name: &str,
    ctx: &CodegenContext,
    out: &mut String,
) {
    if let (Some(meth_const), Some(func_const)) = (
        ctx.get_string_constant(method_name),
        ctx.get_string_constant(func_name),
    ) {
        let meth_len = method_name.len() + 1;
        let func_len = func_name.len() + 1;
        let uid = format!("{}_{}", sanitize_name(method_name), sanitize_name(func_name));

        out.push_str(&format!(
            "  %def_meth_{} = getelementptr inbounds [{} x i8], [{} x i8]* @{}, i64 0, i64 0\n",
            uid, meth_len, meth_len, meth_const
        ));
        out.push_str(&format!(
            "  %def_func_{} = getelementptr inbounds [{} x i8], [{} x i8]* @{}, i64 0, i64 0\n",
            uid, func_len, func_len, func_const
        ));
        out.push_str(&format!(
            "  call void @jdruby_def_method(i64 %r{}, i8* %def_meth_{}, i8* %def_func_{})\n",
            class_reg, uid, uid
        ));
    }
}

fn emit_include_module(
    class_reg: u32,
    module_name: &str,
    ctx: &CodegenContext,
    out: &mut String,
) {
    if let (Some(mod_const), Some(incl_const)) = (
        ctx.get_string_constant(module_name),
        ctx.get_string_constant("include"),
    ) {
        let mod_len = module_name.len() + 1;
        let incl_len = "include".len() + 1;
        let uid = sanitize_name(module_name);

        out.push_str(&format!(
            "  %inc_mod_{} = getelementptr inbounds [{} x i8], [{} x i8]* @{}, i64 0, i64 0\n",
            uid, mod_len, mod_len, mod_const
        ));
        out.push_str(&format!(
            "  %inc_mod_val_{} = call i64 @jdruby_const_get(i8* %inc_mod_{})\n",
            uid, uid
        ));
        out.push_str(&format!(
            "  %inc_name_{} = getelementptr inbounds [{} x i8], [{} x i8]* @{}, i64 0, i64 0\n",
            uid, incl_len, incl_len, incl_const
        ));
        out.push_str(&format!(
            "  call i64 (i64, i8*, i32, ...) @jdruby_send(i64 %r{}, i8* %inc_name_{}, i32 1, i64 %inc_mod_val_{})\n",
            class_reg, uid, uid
        ));
    }
}

fn emit_terminator(term: &MirTerminator, out: &mut String) {
    match term {
        MirTerminator::Return(Some(reg)) => {
            out.push_str(&format!("  ret i64 %r{}\n", reg));
        }
        MirTerminator::Return(None) => {
            out.push_str("  %ret_nil = load i64, i64* @JDRUBY_NIL, align 8\n");
            out.push_str("  ret i64 %ret_nil\n");
        }
        MirTerminator::Branch(label) => {
            out.push_str(&format!("  br label %{}\n", label));
        }
        MirTerminator::CondBranch(reg, then_l, else_l) => {
            out.push_str(&format!(
                "  %br_cond_{} = call i1 @jdruby_truthy(i64 %r{})\n",
                reg, reg
            ));
            out.push_str(&format!(
                "  br i1 %br_cond_{}, label %{}, label %{}\n",
                reg, then_l, else_l
            ));
        }
        MirTerminator::Unreachable => {
            out.push_str("  unreachable\n");
        }
    }
}

use jdruby_hir::{HirModule, HirNode, HirOp, HirUnaryOp, HirLiteralValue};
use crate::nodes::*;

/// Lowers HIR to MIR (register-based flat IR).
pub struct HirLowering {
    next_reg: RegId,
    next_block: u32,
    current_blocks: Vec<MirBlock>,
    current_insts: Vec<MirInst>,
    /// Pending label for the next block (set by start_block)
    pending_label: Option<BlockLabel>,
}

impl HirLowering {
    pub fn new() -> Self {
        Self {
            next_reg: 0,
            next_block: 0,
            current_blocks: Vec::new(),
            current_insts: Vec::new(),
            pending_label: None,
        }
    }

    pub fn lower(module: &HirModule) -> MirModule {
        let mut lowering = Self::new();
        let mut functions = Vec::new();

        // Collect ALL top-level code into a `main` function,
        // including class/module definitions (they emit registration instructions)
        let main_body: Vec<&HirNode> = module.nodes.iter().collect();

        if !main_body.is_empty() {
            let func = lowering.lower_function("main", &[], &main_body);
            functions.push(func);
        }

        // Lower named functions (standalone defs outside classes)
        for node in &module.nodes {
            if let HirNode::FuncDef(def) = node {
                let body_refs: Vec<&HirNode> = def.body.iter().collect();
                let func = lowering.lower_function(&def.name, &def.params, &body_refs);
                functions.push(func);
            }
            if let HirNode::ClassDef(cls) = node {
                for cn in &cls.body {
                    if let HirNode::FuncDef(def) = cn {
                        let qualified = format!("{}#{}", cls.name, def.name);
                        // Add implicit `self` parameter for instance methods
                        let mut params = vec!["self".to_string()];
                        params.extend(def.params.iter().cloned());
                        let body_refs: Vec<&HirNode> = def.body.iter().collect();
                        let func = lowering.lower_function(&qualified, &params, &body_refs);
                        functions.push(func);
                    }
                }
            }
        }

        MirModule { name: module.name.clone(), functions }
    }

    fn lower_function(&mut self, name: &str, params: &[String], body: &[&HirNode]) -> MirFunction {
        self.next_reg = 0;
        self.next_block = 0;
        self.current_blocks = Vec::new();
        self.current_insts = Vec::new();
        self.pending_label = None;

        // Allocate registers for parameters
        let param_regs: Vec<RegId> = params.iter().map(|p| {
            let reg = self.alloc_reg();
            self.emit(MirInst::Store(p.clone(), reg));
            reg
        }).collect();

        // Lower body
        let mut last_reg = None;
        for node in body {
            last_reg = Some(self.lower_node(node));
        }

        // Finalize current block
        let terminator = if let Some(r) = last_reg {
            MirTerminator::Return(Some(r))
        } else {
            MirTerminator::Return(None)
        };
        self.finish_block(terminator);

        MirFunction {
            name: name.to_string(),
            params: param_regs,
            blocks: std::mem::take(&mut self.current_blocks),
            next_reg: self.next_reg,
            span: jdruby_common::SourceSpan::default(),
        }
    }

    fn lower_node(&mut self, node: &HirNode) -> RegId {
        match node {
            HirNode::Literal(lit) => {
                let reg = self.alloc_reg();
                let c = match &lit.value {
                    HirLiteralValue::Integer(v) => MirConst::Integer(*v),
                    HirLiteralValue::Float(v) => MirConst::Float(*v),
                    HirLiteralValue::String(v) => MirConst::String(v.clone()),
                    HirLiteralValue::Symbol(v) => MirConst::Symbol(v.clone()),
                    HirLiteralValue::Bool(v) => MirConst::Bool(*v),
                    HirLiteralValue::Nil => MirConst::Nil,
                    HirLiteralValue::Array(elems) => {
                        let elem_regs: Vec<RegId> = elems.iter().map(|e| self.lower_node(e)).collect();
                        self.emit(MirInst::Call(reg, "rb_ary_new".into(), elem_regs));
                        return reg;
                    }
                    HirLiteralValue::Hash(entries) => {
                        let entry_regs: Vec<RegId> = entries.iter().flat_map(|(k, v)| {
                            vec![self.lower_node(k), self.lower_node(v)]
                        }).collect();
                        self.emit(MirInst::Call(reg, "rb_hash_new".into(), entry_regs));
                        return reg;
                    }
                };
                self.emit(MirInst::LoadConst(reg, c));
                reg
            }
            HirNode::VarRef(var) => {
                let reg = self.alloc_reg();
                self.emit(MirInst::Load(reg, var.name.clone()));
                reg
            }
            HirNode::BinOp(op) => {
                let left = self.lower_node(&op.left);
                let right = self.lower_node(&op.right);
                let reg = self.alloc_reg();
                let mir_op = Self::convert_binop(&op.op);
                self.emit(MirInst::BinOp(reg, mir_op, left, right));
                reg
            }
            HirNode::UnOp(op) => {
                let operand = self.lower_node(&op.operand);
                let reg = self.alloc_reg();
                let mir_op = match op.op {
                    HirUnaryOp::Neg => MirUnOp::Neg,
                    HirUnaryOp::Not => MirUnOp::Not,
                    HirUnaryOp::BitNot => MirUnOp::BitNot,
                };
                self.emit(MirInst::UnOp(reg, mir_op, operand));
                reg
            }
            HirNode::Call(call) => {
                let arg_regs: Vec<RegId> = call.args.iter().map(|a| self.lower_node(a)).collect();
                let reg = self.alloc_reg();
                if let Some(recv) = &call.receiver {
                    let recv_reg = self.lower_node(recv);
                    self.emit(MirInst::MethodCall(reg, recv_reg, call.method.clone(), arg_regs));
                } else {
                    self.emit(MirInst::Call(reg, call.method.clone(), arg_regs));
                }
                reg
            }
            HirNode::Assign(assign) => {
                let val = self.lower_node(&assign.value);
                self.emit(MirInst::Store(assign.target.name.clone(), val));
                val
            }
            HirNode::Branch(branch) => {
                let cond = self.lower_node(&branch.condition);
                let then_label = self.make_label("then");
                let else_label = self.make_label("else");
                let merge_label = self.make_label("merge");

                self.finish_block(MirTerminator::CondBranch(cond, then_label.clone(), else_label.clone()));

                // Then block
                self.start_block(then_label);
                let mut then_result = self.alloc_reg();
                self.emit(MirInst::LoadConst(then_result, MirConst::Nil));
                for n in &branch.then_body { then_result = self.lower_node(n); }
                self.finish_block(MirTerminator::Branch(merge_label.clone()));

                // Else block
                self.start_block(else_label);
                let else_result = self.alloc_reg();
                self.emit(MirInst::LoadConst(else_result, MirConst::Nil));
                for n in &branch.else_body { _ = self.lower_node(n); }
                self.finish_block(MirTerminator::Branch(merge_label.clone()));

                // Merge block
                self.start_block(merge_label);
                let result = self.alloc_reg();
                self.emit(MirInst::Copy(result, then_result));
                result
            }
            HirNode::Loop(lp) => {
                let cond_label = self.make_label("loop_cond");
                let body_label = self.make_label("loop_body");
                let exit_label = self.make_label("loop_exit");

                self.finish_block(MirTerminator::Branch(cond_label.clone()));

                // Condition block
                self.start_block(cond_label.clone());
                let cond = self.lower_node(&lp.condition);
                self.finish_block(MirTerminator::CondBranch(cond, body_label.clone(), exit_label.clone()));

                // Body block
                self.start_block(body_label);
                for n in &lp.body { self.lower_node(n); }
                self.finish_block(MirTerminator::Branch(cond_label));

                // Exit block
                self.start_block(exit_label);
                let result = self.alloc_reg();
                self.emit(MirInst::LoadConst(result, MirConst::Nil));
                result
            }
            HirNode::Return(ret) => {
                let val = ret.value.as_ref().map(|v| self.lower_node(v));
                self.finish_block(MirTerminator::Return(val));
                let after_label = self.make_label("after_return");
                self.start_block(after_label);
                let reg = self.alloc_reg();
                self.emit(MirInst::LoadConst(reg, MirConst::Nil));
                reg
            }
            HirNode::FuncDef(_) => {
                // Standalone functions are handled at module level; emit Nil in main
                let reg = self.alloc_reg();
                self.emit(MirInst::LoadConst(reg, MirConst::Nil));
                reg
            }
            HirNode::ClassDef(cls) => {
                // Emit class registration instructions
                let class_reg = self.alloc_reg();
                self.emit(MirInst::ClassNew(class_reg, cls.name.clone(), cls.superclass.clone()));
                // Store class as a constant
                self.emit(MirInst::Store(cls.name.clone(), class_reg));

                // Register each method or include in the class body
                for node in &cls.body {
                    match node {
                        HirNode::FuncDef(def) => {
                            let qualified = format!("{}#{}", cls.name, def.name);
                            self.emit(MirInst::DefMethod(class_reg, def.name.clone(), qualified));
                        }
                        HirNode::Call(call) => {
                            if call.method == "include" {
                                if let Some(HirNode::VarRef(v)) = call.args.first() {
                                    self.emit(MirInst::IncludeModule(class_reg, v.name.clone()));
                                }
                            }
                        }
                        _ => {}
                    }
                }

                class_reg
            }
            HirNode::Seq(nodes) => {
                let mut last = self.alloc_reg();
                self.emit(MirInst::LoadConst(last, MirConst::Nil));
                for n in nodes { last = self.lower_node(n); }
                last
            }
            HirNode::Yield(args) => {
                let arg_regs: Vec<RegId> = args.iter().map(|a| self.lower_node(a)).collect();
                let reg = self.alloc_reg();
                self.emit(MirInst::Call(reg, "rb_yield".into(), arg_regs));
                reg
            }
            HirNode::Break | HirNode::Next => {
                let reg = self.alloc_reg();
                self.emit(MirInst::LoadConst(reg, MirConst::Nil));
                reg
            }
            HirNode::Nop => {
                let reg = self.alloc_reg();
                self.emit(MirInst::LoadConst(reg, MirConst::Nil));
                reg
            }

            // =====================================================================
            // METAPROGRAMMING NODE LOWERING
            // =====================================================================

            // Blocks and Closures
            HirNode::BlockDef(block_def) => {
                let captured_regs: Vec<RegId> = block_def.captured_vars.iter()
                    .map(|name| {
                        let reg = self.alloc_reg();
                        self.emit(MirInst::Load(reg, name.clone()));
                        reg
                    })
                    .collect();
                let reg = self.alloc_reg();
                let func_symbol = format!("block_{}_{}", self.current_blocks.len(), reg);
                self.emit(MirInst::BlockCreate {
                    dest: reg,
                    func_symbol,
                    captured_vars: captured_regs,
                    is_lambda: block_def.is_lambda,
                });
                reg
            }
            HirNode::ProcDef(proc_def) => {
                let captured_regs: Vec<RegId> = proc_def.captured_vars.iter()
                    .map(|name| {
                        let reg = self.alloc_reg();
                        self.emit(MirInst::Load(reg, name.clone()));
                        reg
                    })
                    .collect();
                let block_reg = self.alloc_reg();
                let func_symbol = format!("proc_{}_{}", self.current_blocks.len(), block_reg);
                self.emit(MirInst::BlockCreate {
                    dest: block_reg,
                    func_symbol,
                    captured_vars: captured_regs,
                    is_lambda: false,
                });
                let reg = self.alloc_reg();
                self.emit(MirInst::ProcCreate { dest: reg, block_reg });
                reg
            }
            HirNode::LambdaDef(lambda_def) => {
                let captured_regs: Vec<RegId> = lambda_def.captured_vars.iter()
                    .map(|name| {
                        let reg = self.alloc_reg();
                        self.emit(MirInst::Load(reg, name.clone()));
                        reg
                    })
                    .collect();
                let block_reg = self.alloc_reg();
                let func_symbol = format!("lambda_{}_{}", self.current_blocks.len(), block_reg);
                self.emit(MirInst::BlockCreate {
                    dest: block_reg,
                    func_symbol,
                    captured_vars: captured_regs,
                    is_lambda: true,
                });
                let reg = self.alloc_reg();
                self.emit(MirInst::LambdaCreate { dest: reg, block_reg });
                reg
            }

            // Module/Class Metaprogramming
            HirNode::ModuleDef(mod_def) => {
                let reg = self.alloc_reg();
                self.emit(MirInst::ModuleNew(reg, mod_def.name.clone()));
                self.emit(MirInst::Store(mod_def.name.clone(), reg));
                reg
            }
            HirNode::SingletonClass(singleton) => {
                let obj_reg = self.lower_node(&singleton.receiver);
                let reg = self.alloc_reg();
                self.emit(MirInst::SingletonClassGet(reg, obj_reg));
                for node in &singleton.body {
                    self.lower_node(node);
                }
                reg
            }

            // Dynamic Method Operations
            HirNode::DefineMethod(def) => {
                let class_reg = def.target_class.as_ref()
                    .map(|t| self.lower_node(t))
                    .unwrap_or_else(|| {
                        let r = self.alloc_reg();
                        self.emit(MirInst::LoadConst(r, MirConst::Nil));
                        r
                    });
                let name_reg = self.lower_node(&def.name);
                let reg = self.alloc_reg();
                let method_func = format!("method_{}_{}", self.current_blocks.len(), reg);
                let visibility = def.visibility.map(|v| self.convert_visibility(v))
                    .unwrap_or(MirVisibility::Public);
                self.emit(MirInst::DefineMethodDynamic {
                    dest: reg,
                    class_reg,
                    name_reg,
                    method_func,
                    visibility,
                });
                reg
            }
            HirNode::UndefMethod(undef) => {
                let class_reg = undef.target_class.as_ref()
                    .map(|t| self.lower_node(t))
                    .unwrap_or_else(|| {
                        let r = self.alloc_reg();
                        self.emit(MirInst::LoadConst(r, MirConst::Nil));
                        r
                    });
                let name_reg = self.lower_node(&undef.name);
                let reg = self.alloc_reg();
                self.emit(MirInst::UndefMethod { dest: reg, class_reg, name_reg });
                reg
            }
            HirNode::AliasMethod(alias) => {
                let class_reg = alias.target_class.as_ref()
                    .map(|t| self.lower_node(t))
                    .unwrap_or_else(|| {
                        let r = self.alloc_reg();
                        self.emit(MirInst::LoadConst(r, MirConst::Nil));
                        r
                    });
                let new_name_reg = self.lower_node(&alias.new_name);
                let old_name_reg = self.lower_node(&alias.old_name);
                let reg = self.alloc_reg();
                self.emit(MirInst::AliasMethod { dest: reg, class_reg, new_name_reg, old_name_reg });
                reg
            }
            HirNode::RemoveMethod(rem) => {
                let class_reg = rem.target_class.as_ref()
                    .map(|t| self.lower_node(t))
                    .unwrap_or_else(|| {
                        let r = self.alloc_reg();
                        self.emit(MirInst::LoadConst(r, MirConst::Nil));
                        r
                    });
                let name_reg = self.lower_node(&rem.name);
                let reg = self.alloc_reg();
                self.emit(MirInst::RemoveMethod { dest: reg, class_reg, name_reg });
                reg
            }
            HirNode::VisibilitySet(vis) => {
                let class_reg = vis.target_class.as_ref()
                    .map(|t| self.lower_node(t))
                    .unwrap_or_else(|| {
                        let r = self.alloc_reg();
                        self.emit(MirInst::LoadConst(r, MirConst::Nil));
                        r
                    });
                let method_name_regs: Vec<RegId> = vis.method_names.iter()
                    .map(|n| self.lower_node(n))
                    .collect();
                let reg = self.alloc_reg();
                let visibility = self.convert_visibility(vis.visibility);
                self.emit(MirInst::SetVisibility { dest: reg, class_reg, visibility, method_names: method_name_regs });
                reg
            }

            // Dynamic Evaluation
            HirNode::InstanceEval(eval) => {
                let obj_reg = eval.receiver.as_ref()
                    .map(|r| self.lower_node(r))
                    .unwrap_or_else(|| {
                        let r = self.alloc_reg();
                        self.emit(MirInst::Load(r, "self".to_string()));
                        r
                    });
                let code_reg = match &eval.source {
                    jdruby_hir::HirEvalSource::String(n) => self.lower_node(n),
                    jdruby_hir::HirEvalSource::Block(_) => {
                        let r = self.alloc_reg();
                        self.emit(MirInst::LoadConst(r, MirConst::Nil));
                        r
                    }
                };
                let binding_reg = eval.binding.as_ref().map(|b| self.lower_node(b));
                let reg = self.alloc_reg();
                self.emit(MirInst::InstanceEval { dest: reg, obj_reg, code_reg, binding_reg });
                reg
            }
            HirNode::ClassEval(eval) => {
                let class_reg = eval.receiver.as_ref()
                    .map(|r| self.lower_node(r))
                    .unwrap_or_else(|| {
                        let r = self.alloc_reg();
                        self.emit(MirInst::LoadConst(r, MirConst::Nil));
                        r
                    });
                let code_reg = match &eval.source {
                    jdruby_hir::HirEvalSource::String(n) => self.lower_node(n),
                    jdruby_hir::HirEvalSource::Block(_) => {
                        let r = self.alloc_reg();
                        self.emit(MirInst::LoadConst(r, MirConst::Nil));
                        r
                    }
                };
                let binding_reg = eval.binding.as_ref().map(|b| self.lower_node(b));
                let reg = self.alloc_reg();
                self.emit(MirInst::ClassEval { dest: reg, class_reg, code_reg, binding_reg });
                reg
            }
            HirNode::ModuleEval(eval) => {
                let module_reg = eval.receiver.as_ref()
                    .map(|r| self.lower_node(r))
                    .unwrap_or_else(|| {
                        let r = self.alloc_reg();
                        self.emit(MirInst::LoadConst(r, MirConst::Nil));
                        r
                    });
                let code_reg = match &eval.source {
                    jdruby_hir::HirEvalSource::String(n) => self.lower_node(n),
                    jdruby_hir::HirEvalSource::Block(_) => {
                        let r = self.alloc_reg();
                        self.emit(MirInst::LoadConst(r, MirConst::Nil));
                        r
                    }
                };
                let binding_reg = eval.binding.as_ref().map(|b| self.lower_node(b));
                let reg = self.alloc_reg();
                self.emit(MirInst::ModuleEval { dest: reg, module_reg, code_reg, binding_reg });
                reg
            }
            HirNode::Eval(eval) => {
                let code_reg = match &eval.source {
                    jdruby_hir::HirEvalSource::String(n) => self.lower_node(n),
                    jdruby_hir::HirEvalSource::Block(_) => {
                        let r = self.alloc_reg();
                        self.emit(MirInst::LoadConst(r, MirConst::Nil));
                        r
                    }
                };
                let binding_reg = eval.binding.as_ref().map(|b| self.lower_node(b));
                let filename_reg = eval.filename.as_ref().map(|f| {
                    let r = self.alloc_reg();
                    self.emit(MirInst::LoadConst(r, MirConst::String(f.clone())));
                    r
                });
                let line_reg = eval.line.map(|l| {
                    let r = self.alloc_reg();
                    self.emit(MirInst::LoadConst(r, MirConst::Integer(l as i64)));
                    r
                });
                let reg = self.alloc_reg();
                self.emit(MirInst::Eval { dest: reg, code_reg, binding_reg, filename_reg, line_reg });
                reg
            }
            HirNode::BindingGet(_) => {
                let reg = self.alloc_reg();
                self.emit(MirInst::BindingGet { dest: reg });
                reg
            }

            // Reflection
            HirNode::Send(send) => {
                let obj_reg = self.lower_node(&send.receiver);
                let name_reg = self.lower_node(&send.method_name);
                let arg_regs: Vec<RegId> = send.args.iter().map(|a| self.lower_node(a)).collect();
                let block_reg = send.block.as_ref().map(|_| {
                    let r = self.alloc_reg();
                    self.emit(MirInst::LoadConst(r, MirConst::Nil));
                    r
                });
                let reg = self.alloc_reg();
                self.emit(MirInst::Send { dest: reg, obj_reg, name_reg, args: arg_regs, block_reg });
                reg
            }
            HirNode::PublicSend(send) => {
                let obj_reg = self.lower_node(&send.receiver);
                let name_reg = self.lower_node(&send.method_name);
                let arg_regs: Vec<RegId> = send.args.iter().map(|a| self.lower_node(a)).collect();
                let block_reg = send.block.as_ref().map(|_| {
                    let r = self.alloc_reg();
                    self.emit(MirInst::LoadConst(r, MirConst::Nil));
                    r
                });
                let reg = self.alloc_reg();
                self.emit(MirInst::PublicSend { dest: reg, obj_reg, name_reg, args: arg_regs, block_reg });
                reg
            }
            HirNode::InternalSend(send) => {
                let obj_reg = self.lower_node(&send.receiver);
                let name_reg = self.lower_node(&send.method_name);
                let arg_regs: Vec<RegId> = send.args.iter().map(|a| self.lower_node(a)).collect();
                let block_reg = send.block.as_ref().map(|_| {
                    let r = self.alloc_reg();
                    self.emit(MirInst::LoadConst(r, MirConst::Nil));
                    r
                });
                let reg = self.alloc_reg();
                self.emit(MirInst::Send { dest: reg, obj_reg, name_reg, args: arg_regs, block_reg });
                reg
            }
            HirNode::RespondTo(resp) => {
                let obj_reg = self.lower_node(&resp.receiver);
                let name_reg = self.lower_node(&resp.method_name);
                let reg = self.alloc_reg();
                self.emit(MirInst::RespondTo { dest: reg, obj_reg, name_reg, include_private: resp.include_private });
                reg
            }
            HirNode::MethodObj(meth) => {
                let obj_reg = self.lower_node(&meth.receiver);
                let name_reg = self.lower_node(&meth.method_name);
                let reg = self.alloc_reg();
                self.emit(MirInst::MethodGet { dest: reg, obj_reg, name_reg });
                reg
            }
            HirNode::InstanceMethod(meth) => {
                let class_reg = self.lower_node(&meth.target_class);
                let name_reg = self.lower_node(&meth.method_name);
                let reg = self.alloc_reg();
                self.emit(MirInst::InstanceMethodGet { dest: reg, class_reg, name_reg });
                reg
            }
            HirNode::MethodCall(call) => {
                let method_reg = self.lower_node(&call.method_obj);
                let receiver_reg = call.receiver.as_ref().map(|r| self.lower_node(r));
                let arg_regs: Vec<RegId> = call.args.iter().map(|a| self.lower_node(a)).collect();
                let block_reg = call.block.as_ref().map(|_| {
                    let r = self.alloc_reg();
                    self.emit(MirInst::LoadConst(r, MirConst::Nil));
                    r
                });
                let reg = self.alloc_reg();
                self.emit(MirInst::MethodObjectCall { dest: reg, method_reg, receiver_reg, args: arg_regs, block_reg });
                reg
            }
            HirNode::MethodBind(bind) => {
                let method_reg = self.lower_node(&bind.method_obj);
                let obj_reg = self.lower_node(&bind.receiver);
                let reg = self.alloc_reg();
                self.emit(MirInst::MethodBind { dest: reg, method_reg, obj_reg });
                reg
            }

            // Dynamic Variable Access
            HirNode::IvarGetDynamic(ivar) => {
                let obj_reg = self.lower_node(&ivar.target);
                let name_reg = self.lower_node(&ivar.name);
                let reg = self.alloc_reg();
                self.emit(MirInst::IvarGetDynamic { dest: reg, obj_reg, name_reg });
                reg
            }
            HirNode::IvarSetDynamic(ivar) => {
                let obj_reg = self.lower_node(&ivar.target);
                let name_reg = self.lower_node(&ivar.name);
                let value_reg = ivar.value.as_ref().map(|v| self.lower_node(v)).unwrap_or_else(|| {
                    let r = self.alloc_reg();
                    self.emit(MirInst::LoadConst(r, MirConst::Nil));
                    r
                });
                self.emit(MirInst::IvarSetDynamic { obj_reg, name_reg, value_reg });
                value_reg
            }
            HirNode::CvarGetDynamic(cvar) => {
                let class_reg = self.lower_node(&cvar.target);
                let name_reg = self.lower_node(&cvar.name);
                let reg = self.alloc_reg();
                self.emit(MirInst::CvarGetDynamic { dest: reg, class_reg, name_reg });
                reg
            }
            HirNode::CvarSetDynamic(cvar) => {
                let class_reg = self.lower_node(&cvar.target);
                let name_reg = self.lower_node(&cvar.name);
                let value_reg = cvar.value.as_ref().map(|v| self.lower_node(v)).unwrap_or_else(|| {
                    let r = self.alloc_reg();
                    self.emit(MirInst::LoadConst(r, MirConst::Nil));
                    r
                });
                self.emit(MirInst::CvarSetDynamic { class_reg, name_reg, value_reg });
                value_reg
            }
            HirNode::ConstGetDynamic(cst) => {
                let class_reg = self.lower_node(&cst.target_class);
                let name_reg = self.lower_node(&cst.name);
                let reg = self.alloc_reg();
                self.emit(MirInst::ConstGetDynamic { dest: reg, class_reg, name_reg, inherit: cst.inherit });
                reg
            }
            HirNode::ConstSetDynamic(cst) => {
                let class_reg = self.lower_node(&cst.target_class);
                let name_reg = self.lower_node(&cst.name);
                let value_reg = cst.value.as_ref().map(|v| self.lower_node(v)).unwrap_or_else(|| {
                    let r = self.alloc_reg();
                    self.emit(MirInst::LoadConst(r, MirConst::Nil));
                    r
                });
                self.emit(MirInst::ConstSetDynamic { class_reg, name_reg, value_reg });
                value_reg
            }

            // Include/Extend/Prepend
            HirNode::Include(inc) => {
                let class_reg = inc.target_class.as_ref()
                    .map(|t| self.lower_node(t))
                    .unwrap_or_else(|| {
                        let r = self.alloc_reg();
                        self.emit(MirInst::LoadConst(r, MirConst::Nil));
                        r
                    });
                let module_reg = self.lower_node(&inc.module);
                let reg = self.alloc_reg();
                self.emit(MirInst::IncludeModule(class_reg, format!("module_{}", module_reg)));
                reg
            }
            HirNode::Extend(ext) => {
                let obj_reg = ext.target_class.as_ref()
                    .map(|t| self.lower_node(t))
                    .unwrap_or_else(|| {
                        let r = self.alloc_reg();
                        self.emit(MirInst::Load(r, "self".to_string()));
                        r
                    });
                let module_reg = self.lower_node(&ext.module);
                let reg = self.alloc_reg();
                self.emit(MirInst::ExtendModule(obj_reg, format!("module_{}", module_reg)));
                reg
            }
            HirNode::Prepend(pre) => {
                let class_reg = pre.target_class.as_ref()
                    .map(|t| self.lower_node(t))
                    .unwrap_or_else(|| {
                        let r = self.alloc_reg();
                        self.emit(MirInst::LoadConst(r, MirConst::Nil));
                        r
                    });
                let module_reg = self.lower_node(&pre.module);
                let reg = self.alloc_reg();
                self.emit(MirInst::PrependModule(class_reg, format!("module_{}", module_reg)));
                reg
            }

            // Method Missing
            HirNode::MethodMissing(mm) => {
                let obj_reg = self.lower_node(&mm.receiver);
                let name_reg = self.alloc_reg();
                self.emit(MirInst::LoadConst(name_reg, MirConst::Symbol(mm.method_name.clone())));
                let arg_regs: Vec<RegId> = mm.args.iter().map(|a| self.lower_node(a)).collect();
                let block_reg = mm.block.as_ref().map(|_| {
                    let r = self.alloc_reg();
                    self.emit(MirInst::LoadConst(r, MirConst::Nil));
                    r
                });
                let reg = self.alloc_reg();
                self.emit(MirInst::MethodMissing { dest: reg, obj_reg, name_reg, args: arg_regs, block_reg });
                reg
            }
        }
    }

    fn convert_visibility(&self, vis: jdruby_hir::Visibility) -> MirVisibility {
        match vis {
            jdruby_hir::Visibility::Public => MirVisibility::Public,
            jdruby_hir::Visibility::Protected => MirVisibility::Protected,
            jdruby_hir::Visibility::Private => MirVisibility::Private,
            jdruby_hir::Visibility::ModuleFunction => MirVisibility::ModuleFunction,
        }
    }

    fn alloc_reg(&mut self) -> RegId {
        let r = self.next_reg;
        self.next_reg += 1;
        r
    }

    fn make_label(&mut self, prefix: &str) -> BlockLabel {
        let l = format!("{}_{}", prefix, self.next_block);
        self.next_block += 1;
        l
    }

    fn emit(&mut self, inst: MirInst) {
        self.current_insts.push(inst);
    }

    fn finish_block(&mut self, terminator: MirTerminator) {
        let label = if let Some(lbl) = self.pending_label.take() {
            lbl
        } else if self.current_blocks.is_empty() {
            format!("entry_{}", self.current_blocks.len())
        } else {
            format!("bb_{}", self.current_blocks.len())
        };
        self.current_blocks.push(MirBlock {
            label,
            instructions: std::mem::take(&mut self.current_insts),
            terminator,
        });
    }

    fn start_block(&mut self, label: BlockLabel) {
        // Set the pending label — the next finish_block will use it
        self.pending_label = Some(label);
    }

    fn convert_binop(op: &HirOp) -> MirBinOp {
        match op {
            HirOp::Add => MirBinOp::Add,
            HirOp::Sub => MirBinOp::Sub,
            HirOp::Mul => MirBinOp::Mul,
            HirOp::Div => MirBinOp::Div,
            HirOp::Mod => MirBinOp::Mod,
            HirOp::Pow => MirBinOp::Pow,
            HirOp::Eq => MirBinOp::Eq,
            HirOp::NotEq => MirBinOp::NotEq,
            HirOp::Lt => MirBinOp::Lt,
            HirOp::Gt => MirBinOp::Gt,
            HirOp::LtEq => MirBinOp::LtEq,
            HirOp::GtEq => MirBinOp::GtEq,
            HirOp::Cmp => MirBinOp::Cmp,
            HirOp::And => MirBinOp::And,
            HirOp::Or => MirBinOp::Or,
            HirOp::BitAnd => MirBinOp::BitAnd,
            HirOp::BitOr => MirBinOp::BitOr,
            HirOp::BitXor => MirBinOp::BitXor,
            HirOp::Shl => MirBinOp::Shl,
            HirOp::Shr => MirBinOp::Shr,
        }
    }
}

impl Default for HirLowering {
    fn default() -> Self { Self::new() }
}

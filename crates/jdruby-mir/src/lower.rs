use jdruby_hir::{HirModule, HirNode, HirOp, HirUnaryOp, HirLiteralValue};
use crate::nodes::*;
use crate::inline_cache::InlineCacheTable;
use std::sync::Arc;

/// Lowers HIR to MIR (register-based flat IR).
pub struct HirLowering {
    next_reg: RegId,
    next_block: u32,
    current_blocks: Vec<MirBlock>,
    current_insts: Vec<MirInst>,
    /// Pending label for the next block (set by start_block)
    pending_label: Option<BlockLabel>,
    /// Block functions collected during lowering that need to be emitted
    block_functions: Vec<MirFunction>,
    /// Current implicit block register (for passing blocks through method calls)
    current_block: Option<RegId>,
    /// Inline cache table for method dispatch optimization (shared ownership)
    ic_table: Arc<InlineCacheTable>,
}

impl HirLowering {
    pub fn new() -> Self {
        Self {
            next_reg: 0,
            next_block: 0,
            current_blocks: Vec::new(),
            current_insts: Vec::new(),
            pending_label: None,
            block_functions: Vec::new(),
            current_block: None,
            ic_table: Arc::new(InlineCacheTable::new(256)), // 256 IC slots per module
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
                        // For class methods (def self.xxx), use . separator and still add self param
                        // For instance methods, use # separator and add self param (the instance)
                        let qualified = if def.is_class_method {
                            format!("{}.{}", cls.name, def.name)
                        } else {
                            format!("{}#{}", cls.name, def.name)
                        };
                        // Add implicit `self` parameter for both instance and class methods
                        // For class methods, self is the class object; for instance methods, self is the instance
                        let mut params = vec!["self".to_string()];
                        params.extend(def.params.iter().cloned());
                        let body_refs: Vec<&HirNode> = def.body.iter().collect();
                        let func = lowering.lower_function(&qualified, &params, &body_refs);
                        functions.push(func);
                    }
                }
            }
        }

        // Add all block functions that were collected during lowering
        functions.extend(std::mem::take(&mut lowering.block_functions));

        MirModule { name: module.name.clone(), functions }
    }

    fn lower_function(&mut self, name: &str, params: &[String], body: &[&HirNode]) -> MirFunction {
        self.next_reg = 0;
        self.next_block = 0;
        self.current_blocks = Vec::new();
        self.current_insts = Vec::new();
        self.pending_label = None;

        // Check if method body contains Yield - if so, add block parameter
        let has_yield = body.iter().any(|node| contains_yield(node));
        
        // Also check if method calls define_method - if so, it needs a block param
        // because the dynamically defined method might yield
        let calls_define_method = body.iter().any(|node| contains_define_method(node));
        
        // Allocate registers for parameters
        let mut param_regs: Vec<RegId> = params.iter().map(|p| {
            let reg = self.alloc_reg();
            self.emit(MirInst::Store(p.clone(), reg));
            reg
        }).collect();
        
        // If method yields OR calls define_method, get the implicit block via CurrentBlock
        // and store it as local variable "block" for yield to access
        let has_yield = body.iter().any(|node| contains_yield(node));
        let calls_define_method = body.iter().any(|node| contains_define_method(node));
        if has_yield || calls_define_method {
            let block_reg = self.alloc_reg();
            self.emit(MirInst::CurrentBlock { dest: block_reg });
            self.emit(MirInst::Store("block".to_string(), block_reg));
        }

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
            captured_vars: vec![],
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
                        // First lower all element expressions
                        let elem_regs: Vec<RegId> = elems.iter().map(|e| self.lower_node(e)).collect();
                        // Create empty array first
                        let array_reg = self.alloc_reg();
                        self.emit(MirInst::Call(array_reg, "jdruby_ary_new_empty".into(), vec![]));
                        // Then push each element
                        for elem_reg in elem_regs {
                            self.emit(MirInst::Call(array_reg, "jdruby_ary_push".into(), vec![array_reg, elem_reg]));
                        }
                        return array_reg;
                    }
                    HirLiteralValue::Hash(entries) => {
                        // First lower all key-value pairs
                        let entry_regs: Vec<RegId> = entries.iter().flat_map(|(k, v)| {
                            vec![self.lower_node(k), self.lower_node(v)]
                        }).collect();
                        // Create empty hash first
                        let hash_reg = self.alloc_reg();
                        self.emit(MirInst::Call(hash_reg, "jdruby_hash_new_empty".into(), vec![]));
                        // Then set each key-value pair
                        let mut i = 0;
                        while i < entry_regs.len() {
                            let key_reg = entry_regs[i];
                            let val_reg = entry_regs[i + 1];
                            self.emit(MirInst::Call(hash_reg, "jdruby_hash_set".into(), vec![hash_reg, key_reg, val_reg]));
                            i += 2;
                        }
                        return hash_reg;
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
                // Process arguments, but detect block pass (symbol literals that represent &:sym)
                // ONLY convert symbol to block if explicitly marked as a block in HIR (call.block is Some)
                // or if the HIR node itself indicates it's a block pass
                let mut processed_args: Vec<RegId> = Vec::new();
                let mut block_reg: Option<RegId> = None;
                
                // First check if there's an explicit block in the call
                let has_explicit_block = call.block.is_some();
                
                for arg in &call.args {
                    // Check if this argument is a block pass - only if:
                    // 1. There's an explicit block in HIR, OR
                    // 2. The argument node itself is marked as a block pass (not just a symbol literal)
                    // In Ruby, `&:run` syntax should be parsed into call.block, not call.args
                    // If a symbol literal ends up in args without explicit block marking, 
                    // it should be treated as a positional argument
                    let is_block_pass_symbol = matches!(
                        arg, 
                        HirNode::Literal(lit) if matches!(&lit.value, HirLiteralValue::Symbol(_))
                    );
                    
                    // Only treat symbol as block pass if there's explicit block context
                    // or if the call's block field contains this symbol
                    let should_convert_to_block = is_block_pass_symbol 
                        && block_reg.is_none() 
                        && has_explicit_block
                        && is_symbol_block_arg(arg, call.block.as_ref());
                    
                    if should_convert_to_block {
                        // Convert symbol to proc and use as block
                        if let HirNode::Literal(lit) = arg {
                            if let HirLiteralValue::Symbol(sym_name) = &lit.value {
                                let sym_reg = self.alloc_reg();
                                self.emit(MirInst::LoadConst(sym_reg, MirConst::Symbol(sym_name.clone())));
                                let proc_reg = self.alloc_reg();
                                self.emit(MirInst::SymbolToProc { dest: proc_reg, symbol_reg: sym_reg });
                                block_reg = Some(proc_reg);
                            } else {
                                processed_args.push(self.lower_node(arg));
                            }
                        } else {
                            processed_args.push(self.lower_node(arg));
                        }
                    } else {
                        processed_args.push(self.lower_node(arg));
                    }
                }
                
                // If we didn't find a block pass in args, check the explicit block field
                if block_reg.is_none() {
                    if let Some(ref block) = call.block {
                        // Check if this is &:sym syntax (block body is a single symbol literal)
                        let is_symbol_proc = block.body.len() == 1 &&
                            matches!(&block.body[0], HirNode::Literal(lit) if matches!(&lit.value, HirLiteralValue::Symbol(_)));
                        
                        if is_symbol_proc {
                            // Handle &:sym syntax - convert symbol to proc
                            if let HirNode::Literal(lit) = &block.body[0] {
                                if let HirLiteralValue::Symbol(sym_name) = &lit.value {
                                    let sym_reg = self.alloc_reg();
                                    self.emit(MirInst::LoadConst(sym_reg, MirConst::Symbol(sym_name.clone())));
                                    let proc_reg = self.alloc_reg();
                                    self.emit(MirInst::SymbolToProc { dest: proc_reg, symbol_reg: sym_reg });
                                    block_reg = Some(proc_reg);
                                }
                            } else {
                                unreachable!()
                            }
                        } else if !block.body.is_empty() {
                            // Create block function from the body (handles both single and multi-expression blocks)
                            let func_symbol = format!("block_in_{}_{}", call.method, self.next_reg);
                            let body_cloned: Vec<HirNode> = block.body.iter().cloned().collect();
                            
                            // Check if block captures self and add to captured_vars if needed
                            // But don't add if self is already the first captured var (common case)
                            let mut effective_captured_vars = block.captured_vars.clone();
                            if block.captures_self && effective_captured_vars.first().map_or(true, |s| s != "self") {
                                effective_captured_vars.insert(0, "self".to_string());
                            }
                            
                            let block_func = self.lower_block_function(&func_symbol, &block.params, &body_cloned, &effective_captured_vars);
                            self.block_functions.push(block_func);
                            
                            // Load captured variables from the HIR block
                            let captured_regs: Vec<RegId> = effective_captured_vars.iter()
                                .map(|name| {
                                    let reg = self.alloc_reg();
                                    self.emit(MirInst::Load(reg, name.clone()));
                                    reg
                                })
                                .collect();
                            
                            // Create block object
                            let reg = self.alloc_reg();
                            self.emit(MirInst::BlockCreate {
                                dest: reg,
                                func_symbol,
                                captured_vars: captured_regs,
                                is_lambda: false,
                            });
                            block_reg = Some(reg);
                        } else {
                            // Empty block body - use implicit block if available
                            block_reg = self.current_block;
                        }
                    } else {
                        // No explicit block - use implicit block if available
                        block_reg = self.current_block;
                    }
                }
                
                let reg = self.alloc_reg();
                if let Some(recv) = &call.receiver {
                    let recv_reg = self.lower_node(recv);
                    
                    // If block is present, use MethodCall with block support
                    if let Some(block) = block_reg {
                        self.emit(MirInst::MethodCall(reg, recv_reg, call.method.clone(), processed_args, Some(block)));
                    } else {
                        self.emit(MirInst::MethodCall(reg, recv_reg, call.method.clone(), processed_args, None));
                    }
                } else {
                    // No receiver - this is a function call (not a method call)
                    // Use Send if block is present, otherwise use Call
                    if let Some(block) = block_reg {
                        // For function calls with blocks, we need to use Send
                        // The "receiver" is the current self/context
                        let recv_reg = self.alloc_reg();
                        self.emit(MirInst::Load(recv_reg, "self".to_string()));
                        let method_reg = self.alloc_reg();
                        self.emit(MirInst::LoadConst(method_reg, MirConst::Symbol(call.method.clone())));
                        self.emit(MirInst::Send { dest: reg, obj_reg: recv_reg, name_reg: method_reg, args: processed_args, block_reg: Some(block) });
                    } else {
                        self.emit(MirInst::Call(reg, call.method.clone(), processed_args));
                    }
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
                            // For class methods (def self.xxx), use . separator
                            // For instance methods, use # separator
                            let qualified = if def.is_class_method {
                                format!("{}.{}", cls.name, def.name)
                            } else {
                                format!("{}#{}", cls.name, def.name)
                            };
                            if def.is_class_method {
                                // For class methods (def self.xxx), define on the singleton class
                                let singleton_reg = self.alloc_reg();
                                self.emit(MirInst::SingletonClassGet(singleton_reg, class_reg));
                                self.emit(MirInst::DefMethod(singleton_reg, def.name.clone(), qualified));
                            } else {
                                // For instance methods, define on the class itself
                                self.emit(MirInst::DefMethod(class_reg, def.name.clone(), qualified));
                            }
                        }
                        HirNode::Call(call) => {
                            match call.method.as_str() {
                                "include" => {
                                    // Lower the module expression (could be VarRef, method call, etc.)
                                    if let Some(first_arg) = call.args.first() {
                                        let module_reg = self.lower_node(first_arg);
                                        self.emit(MirInst::IncludeModule(class_reg, module_reg));
                                    }
                                }
                                "prepend" => {
                                    // Lower the module expression
                                    if let Some(first_arg) = call.args.first() {
                                        let module_reg = self.lower_node(first_arg);
                                        self.emit(MirInst::PrependModule(class_reg, module_reg));
                                    }
                                }
                                "extend" => {
                                    // Lower the module expression
                                    if let Some(first_arg) = call.args.first() {
                                        let module_reg = self.lower_node(first_arg);
                                        self.emit(MirInst::ExtendModule(class_reg, module_reg));
                                    }
                                }
                                _ => {}
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
                // Use the current block register if available, otherwise load from implicit block
                let block_reg = self.current_block.unwrap_or_else(|| {
                    let r = self.alloc_reg();
                    self.emit(MirInst::CurrentBlock { dest: r });
                    r
                });
                self.emit(MirInst::BlockInvoke {
                    dest: reg,
                    block_reg,
                    args: arg_regs,
                    splat_arg: None,
                    block_arg: None,
                });
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
            HirNode::ExceptionBegin(exc) => {
                // Lower exception handling block
                // For now, lower the body and ignore rescue/ensure (stub)
                let mut last_reg = self.alloc_reg();
                self.emit(MirInst::LoadConst(last_reg, MirConst::Nil));
                for node in &exc.body {
                    last_reg = self.lower_node(node);
                }
                last_reg
            }

            // =====================================================================
            // METAPROGRAMMING NODE LOWERING
            // =====================================================================

            // Blocks and Closures
            HirNode::BlockDef(block_def) => {
                // Create the block function from the body
                let func_symbol = format!("block_{}_{}", self.current_blocks.len(), self.next_reg);
                
                // Check if block captures self and add to captured_vars if needed
                // But don't add if self is already the first captured var (common case)
                let mut effective_captured_vars = block_def.captured_vars.clone();
                if block_def.captures_self && effective_captured_vars.first().map_or(true, |s| s != "self") {
                    effective_captured_vars.insert(0, "self".to_string());
                }
                
                let block_func = self.lower_block_function(&func_symbol, &block_def.params, &block_def.body, &effective_captured_vars);
                self.block_functions.push(block_func);

                // Load captured variables
                let captured_regs: Vec<RegId> = effective_captured_vars.iter()
                    .map(|name| {
                        let reg = self.alloc_reg();
                        self.emit(MirInst::Load(reg, name.clone()));
                        reg
                    })
                    .collect();

                // Create block referencing the function
                let reg = self.alloc_reg();
                self.emit(MirInst::BlockCreate {
                    dest: reg,
                    func_symbol,
                    captured_vars: captured_regs,
                    is_lambda: block_def.is_lambda,
                });
                
                // Set as current block for implicit propagation
                self.current_block = Some(reg);
                
                reg
            }
            HirNode::ProcDef(proc_def) => {
                // Create the proc function from the body
                let func_symbol = format!("proc_{}_{}", self.current_blocks.len(), self.next_reg);
                
                // Check if block captures self and add to captured_vars if needed
                // But don't add if self is already the first captured var (common case)
                let mut effective_captured_vars = proc_def.captured_vars.clone();
                if proc_def.captures_self && effective_captured_vars.first().map_or(true, |s| s != "self") {
                    effective_captured_vars.insert(0, "self".to_string());
                }
                
                let proc_func = self.lower_block_function(&func_symbol, &proc_def.params, &proc_def.body, &effective_captured_vars);
                self.block_functions.push(proc_func);

                // Load captured variables
                let captured_regs: Vec<RegId> = effective_captured_vars.iter()
                    .map(|name| {
                        let reg = self.alloc_reg();
                        self.emit(MirInst::Load(reg, name.clone()));
                        reg
                    })
                    .collect();

                // Create block
                let block_reg = self.alloc_reg();
                self.emit(MirInst::BlockCreate {
                    dest: block_reg,
                    func_symbol,
                    captured_vars: captured_regs,
                    is_lambda: false,
                });

                // Wrap in Proc
                let reg = self.alloc_reg();
                self.emit(MirInst::ProcCreate { dest: reg, block_reg });
                reg
            }
            HirNode::LambdaDef(lambda_def) => {
                // Create the lambda function from the body
                let func_symbol = format!("lambda_{}_{}", self.current_blocks.len(), self.next_reg);
                
                // Check if block captures self and add to captured_vars if needed
                // But don't add if self is already the first captured var (common case)
                let mut effective_captured_vars = lambda_def.captured_vars.clone();
                if lambda_def.captures_self && effective_captured_vars.first().map_or(true, |s| s != "self") {
                    effective_captured_vars.insert(0, "self".to_string());
                }
                
                let lambda_func = self.lower_block_function(&func_symbol, &lambda_def.params, &lambda_def.body, &effective_captured_vars);
                self.block_functions.push(lambda_func);

                // Load captured variables
                let captured_regs: Vec<RegId> = effective_captured_vars.iter()
                    .map(|name| {
                        let reg = self.alloc_reg();
                        self.emit(MirInst::Load(reg, name.clone()));
                        reg
                    })
                    .collect();

                // Create block
                let block_reg = self.alloc_reg();
                self.emit(MirInst::BlockCreate {
                    dest: block_reg,
                    func_symbol,
                    captured_vars: captured_regs,
                    is_lambda: true,
                });

                // Wrap in Lambda
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
                let visibility = def.visibility.map(|v| self.convert_visibility(v))
                    .unwrap_or(MirVisibility::Public);
                
                // Properly handle the body for define_method
                // The body (HirBlockDef) becomes the method body
                let func_symbol = format!("method_body_{}_{}", self.current_blocks.len(), self.next_reg);
                let body_cloned: Vec<HirNode> = def.body.body.iter().cloned().collect();
                
                // Check if block captures self
                // But don't add if self is already the first captured var (common case)
                let mut effective_captured_vars = def.body.captured_vars.clone();
                if def.body.captures_self && effective_captured_vars.first().map_or(true, |s| s != "self") {
                    effective_captured_vars.insert(0, "self".to_string());
                }
                
                // Create the block function that will serve as method body
                let block_func = self.lower_block_function(&func_symbol, &def.body.params, &body_cloned, &effective_captured_vars);
                self.block_functions.push(block_func);
                
                // Load captured variables
                let captured_regs: Vec<RegId> = effective_captured_vars.iter()
                    .map(|name| {
                        let reg = self.alloc_reg();
                        self.emit(MirInst::Load(reg, name.clone()));
                        reg
                    })
                    .collect();
                
                // Create block object
                let block_reg = self.alloc_reg();
                self.emit(MirInst::BlockCreate {
                    dest: block_reg,
                    func_symbol: func_symbol.clone(),
                    captured_vars: captured_regs,
                    is_lambda: false,
                });
                
                // Also define the method with the block's function symbol
                // This associates the method name with the block's implementation
                self.emit(MirInst::DefMethod(class_reg, func_symbol.clone(), func_symbol));
                
                // Emit DefineMethodDynamic for any additional runtime handling
                self.emit(MirInst::DefineMethodDynamic {
                    dest: reg,
                    class_reg,
                    name_reg,
                    method_func: "__defined_method__".to_string(),
                    visibility,
                    block_reg: Some(block_reg),
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
                let name_node = &send.method_name;
                
                // Process all arguments normally - symbols are regular args, not blocks
                let arg_regs: Vec<RegId> = send.args.iter()
                    .map(|arg| self.lower_node(arg))
                    .collect();
                
                // Use block from send.block if present
                let block_reg = send.block.as_ref().map(|block| {
                    // Create block function from the body
                    let func_symbol = format!("block_in_send_{}_{}", self.current_blocks.len(), self.next_reg);
                    let body_cloned: Vec<HirNode> = block.body.iter().cloned().collect();
                    
                    // Check if block captures self and add to captured_vars if needed
                    // But don't add if self is already the first captured var (common case)
                    let mut effective_captured_vars = block.captured_vars.clone();
                    if block.captures_self && effective_captured_vars.first().map_or(true, |s| s != "self") {
                        effective_captured_vars.insert(0, "self".to_string());
                    }
                    
                    let block_func = self.lower_block_function(&func_symbol, &block.params, &body_cloned, &effective_captured_vars);
                    self.block_functions.push(block_func);
                    
                    // Load captured variables from the HIR block
                    let captured_regs: Vec<RegId> = effective_captured_vars.iter()
                        .map(|name| {
                            let reg = self.alloc_reg();
                            self.emit(MirInst::Load(reg, name.clone()));
                            reg
                        })
                        .collect();
                    
                    // Create block object
                    let reg = self.alloc_reg();
                    self.emit(MirInst::BlockCreate {
                        dest: reg,
                        func_symbol,
                        captured_vars: captured_regs,
                        is_lambda: false,
                    });
                    reg
                });
                
                let reg = self.alloc_reg();
                
                // Check if method name is a compile-time constant (symbol literal)
                // If so, use SendWithIC for optimized dispatch
                if let HirNode::Literal(lit) = name_node {
                    if let HirLiteralValue::Symbol(method_name) = &lit.value {
                        // Static method name - use inline cache
                        let cache_slot = self.ic_table.alloc_slot();
                        self.emit(MirInst::SendWithIC { 
                            dest: reg, 
                            obj_reg, 
                            method_name: method_name.clone(), 
                            args: arg_regs, 
                            block_reg,
                            cache_slot,
                        });
                        return reg;
                    }
                }
                
                // Dynamic method name - use generic Send
                let name_reg = self.lower_node(name_node);
                self.emit(MirInst::Send { dest: reg, obj_reg, name_reg, args: arg_regs, block_reg });
                reg
            }
            HirNode::PublicSend(send) => {
                let obj_reg = self.lower_node(&send.receiver);
                let name_reg = self.lower_node(&send.method_name);
                
                // Process all arguments normally - symbols are regular args, not blocks
                let arg_regs: Vec<RegId> = send.args.iter()
                    .map(|arg| self.lower_node(arg))
                    .collect();
                
                // Use block from send.block if present
                let block_reg = send.block.as_ref().map(|block| {
                    // Create block function from the body
                    let func_symbol = format!("block_in_public_send_{}_{}", self.current_blocks.len(), self.next_reg);
                    let body_cloned: Vec<HirNode> = block.body.iter().cloned().collect();
                    
                    // Check if block captures self and add to captured_vars if needed
                    // But don't add if self is already the first captured var (common case)
                    let mut effective_captured_vars = block.captured_vars.clone();
                    if block.captures_self && effective_captured_vars.first().map_or(true, |s| s != "self") {
                        effective_captured_vars.insert(0, "self".to_string());
                    }
                    
                    let block_func = self.lower_block_function(&func_symbol, &block.params, &body_cloned, &effective_captured_vars);
                    self.block_functions.push(block_func);
                    
                    // Load captured variables from the HIR block
                    let captured_regs: Vec<RegId> = effective_captured_vars.iter()
                        .map(|name| {
                            let reg = self.alloc_reg();
                            self.emit(MirInst::Load(reg, name.clone()));
                            reg
                        })
                        .collect();
                    
                    // Create block object
                    let reg = self.alloc_reg();
                    self.emit(MirInst::BlockCreate {
                        dest: reg,
                        func_symbol,
                        captured_vars: captured_regs,
                        is_lambda: false,
                    });
                    reg
                });
                
                let reg = self.alloc_reg();
                self.emit(MirInst::PublicSend { dest: reg, obj_reg, name_reg, args: arg_regs, block_reg });
                reg
            }
            HirNode::InternalSend(send) => {
                let obj_reg = self.lower_node(&send.receiver);
                let name_reg = self.lower_node(&send.method_name);
                
                // Process all arguments normally - symbols are regular args, not blocks
                let arg_regs: Vec<RegId> = send.args.iter()
                    .map(|arg| self.lower_node(arg))
                    .collect();
                
                // Use block from send.block if present
                let block_reg = send.block.as_ref().map(|block| {
                    // Create block function from the body
                    let func_symbol = format!("block_in_internal_send_{}_{}", self.current_blocks.len(), self.next_reg);
                    let body_cloned: Vec<HirNode> = block.body.iter().cloned().collect();
                    
                    // Check if block captures self and add to captured_vars if needed
                    // But don't add if self is already the first captured var (common case)
                    let mut effective_captured_vars = block.captured_vars.clone();
                    if block.captures_self && effective_captured_vars.first().map_or(true, |s| s != "self") {
                        effective_captured_vars.insert(0, "self".to_string());
                    }
                    
                    let block_func = self.lower_block_function(&func_symbol, &block.params, &body_cloned, &effective_captured_vars);
                    self.block_functions.push(block_func);
                    
                    // Load captured variables from the HIR block
                    let captured_regs: Vec<RegId> = effective_captured_vars.iter()
                        .map(|name| {
                            let reg = self.alloc_reg();
                            self.emit(MirInst::Load(reg, name.clone()));
                            reg
                        })
                        .collect();
                    
                    // Create block object
                    let reg = self.alloc_reg();
                    self.emit(MirInst::BlockCreate {
                        dest: reg,
                        func_symbol,
                        captured_vars: captured_regs,
                        is_lambda: false,
                    });
                    reg
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
                self.emit(MirInst::IncludeModule(class_reg, module_reg));
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
                self.emit(MirInst::ExtendModule(obj_reg, module_reg));
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
                self.emit(MirInst::PrependModule(class_reg, module_reg));
                reg
            }

            // Method Missing
            HirNode::MethodMissing(mm) => {
                let obj_reg = self.lower_node(&mm.receiver);
                let name_reg = self.alloc_reg();
                self.emit(MirInst::LoadConst(name_reg, MirConst::Symbol(mm.method_name.clone())));
                let arg_regs: Vec<RegId> = mm.args.iter().map(|a| self.lower_node(a)).collect();
                // Properly lower the block if present
                let block_reg = mm.block.as_ref().map(|block| {
                    let func_symbol = format!("block_in_mm_{}_{}", mm.method_name, self.next_reg);
                    let body_cloned: Vec<HirNode> = block.body.iter().cloned().collect();
                    
                    // Check if block captures self and add to captured_vars if needed
                    // But don't add if self is already the first captured var (common case)
                    let mut effective_captured_vars = block.captured_vars.clone();
                    if block.captures_self && effective_captured_vars.first().map_or(true, |s| s != "self") {
                        effective_captured_vars.insert(0, "self".to_string());
                    }
                    
                    let block_func = self.lower_block_function(&func_symbol, &block.params, &body_cloned, &effective_captured_vars);
                    self.block_functions.push(block_func);
                    
                    // Load captured variables
                    let captured_regs: Vec<RegId> = effective_captured_vars.iter()
                        .map(|name| {
                            let reg = self.alloc_reg();
                            self.emit(MirInst::Load(reg, name.clone()));
                            reg
                        })
                        .collect();
                    
                    // Create block object
                    let reg = self.alloc_reg();
                    self.emit(MirInst::BlockCreate {
                        dest: reg,
                        func_symbol,
                        captured_vars: captured_regs,
                        is_lambda: false,
                    });
                    reg
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

    /// Lower a block body into a separate MIR function
    fn lower_block_function(&mut self, name: &str, params: &[jdruby_hir::HirBlockParam], body: &[HirNode], captured_vars: &[String]) -> MirFunction {
        // Save current state
        let saved_next_reg = self.next_reg;
        let saved_next_block = self.next_block;
        let saved_blocks = std::mem::take(&mut self.current_blocks);
        let saved_insts = std::mem::take(&mut self.current_insts);
        let saved_pending = self.pending_label.take();

        // Reset for new function
        self.next_reg = 0;
        self.next_block = 0;

        // Allocate registers for captured variables FIRST - these come as initial parameters
        // from the block creation/call site
        let capture_regs: Vec<RegId> = captured_vars.iter().map(|name| {
            let reg = self.alloc_reg();
            // Store the capture value to the local variable name so body can reference it
            self.emit(MirInst::Store(name.clone(), reg));
            reg
        }).collect();
        
        // Allocate registers for block parameters (these come from yield call AFTER captures)
        let param_regs: Vec<RegId> = params.iter().map(|p| {
            let reg = self.alloc_reg();
            self.emit(MirInst::Store(p.name.clone(), reg));
            reg
        }).collect();

        // Lower block body
        let mut last_reg = None;
        for node in body {
            last_reg = Some(self.lower_node(node));
        }

        // Finalize block
        let terminator = if let Some(r) = last_reg {
            MirTerminator::Return(Some(r))
        } else {
            MirTerminator::Return(None)
        };
        self.finish_block(terminator);

        // Create the function - params include captures FIRST, then yield params
        let mut all_params = capture_regs;
        all_params.extend(param_regs);
        
        let func = MirFunction {
            name: name.to_string(),
            params: all_params,
            blocks: std::mem::take(&mut self.current_blocks),
            next_reg: self.next_reg,
            span: jdruby_common::SourceSpan::default(),
            captured_vars: captured_vars.to_vec(),
        };

        // Restore state
        self.next_reg = saved_next_reg;
        self.next_block = saved_next_block;
        self.current_blocks = saved_blocks;
        self.current_insts = saved_insts;
        self.pending_label = saved_pending;

        func
    }
}

/// Check if a HIR node contains a Yield expression (recursively)
fn contains_yield(node: &HirNode) -> bool {
    match node {
        HirNode::Yield(_) => true,
        HirNode::BinOp(op) => contains_yield(&op.left) || contains_yield(&op.right),
        HirNode::UnOp(op) => contains_yield(&op.operand),
        HirNode::Call(call) => {
            call.receiver.as_ref().map_or(false, contains_yield) ||
            call.args.iter().any(contains_yield) ||
            call.block.as_ref().map_or(false, |b| b.body.iter().any(contains_yield))
        }
        HirNode::Assign(assign) => contains_yield(&assign.value),
        HirNode::Branch(branch) => {
            contains_yield(&branch.condition) ||
            branch.then_body.iter().any(contains_yield) ||
            branch.else_body.iter().any(contains_yield)
        }
        HirNode::Loop(lp) => {
            contains_yield(&lp.condition) ||
            lp.body.iter().any(contains_yield)
        }
        HirNode::Return(ret) => ret.value.as_ref().map_or(false, contains_yield),
        HirNode::Seq(nodes) => nodes.iter().any(contains_yield),
        _ => false,
    }
}

/// Check if a HIR node contains a call to define_method (recursively)
fn contains_define_method(node: &HirNode) -> bool {
    match node {
        HirNode::Call(call) => {
            if call.method == "define_method" {
                return true;
            }
            call.receiver.as_ref().map_or(false, contains_define_method) ||
            call.args.iter().any(contains_define_method) ||
            call.block.as_ref().map_or(false, |b| b.body.iter().any(contains_define_method))
        }
        HirNode::BinOp(op) => contains_define_method(&op.left) || contains_define_method(&op.right),
        HirNode::UnOp(op) => contains_define_method(&op.operand),
        HirNode::Assign(assign) => contains_define_method(&assign.value),
        HirNode::Branch(branch) => {
            contains_define_method(&branch.condition) ||
            branch.then_body.iter().any(contains_define_method) ||
            branch.else_body.iter().any(contains_define_method)
        }
        HirNode::Loop(lp) => {
            contains_define_method(&lp.condition) ||
            lp.body.iter().any(contains_define_method)
        }
        HirNode::Return(ret) => ret.value.as_ref().map_or(false, contains_define_method),
        HirNode::Seq(nodes) => nodes.iter().any(contains_define_method),
        _ => false,
    }
}

/// Check if a symbol argument matches the block's symbol content
/// This is used to detect &:sym syntax where the symbol in args should become the block
fn is_symbol_block_arg(arg: &jdruby_hir::HirNode, block: Option<&jdruby_hir::HirBlock>) -> bool {
    if let Some(block) = block {
        // Check if block body is a single symbol literal that matches the arg
        if block.body.len() == 1 {
            if let jdruby_hir::HirNode::Literal(arg_lit) = arg {
                if let jdruby_hir::HirLiteralValue::Symbol(arg_sym) = &arg_lit.value {
                    if let jdruby_hir::HirNode::Literal(block_lit) = &block.body[0] {
                        if let jdruby_hir::HirLiteralValue::Symbol(block_sym) = &block_lit.value {
                            return arg_sym == block_sym;
                        }
                    }
                }
            }
        }
    }
    false
}

impl Default for HirLowering {
    fn default() -> Self { Self::new() }
}

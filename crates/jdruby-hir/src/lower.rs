use jdruby_ast::*;
use jdruby_common::SourceSpan;
use crate::nodes::*;

/// Factory for lowering blocks, procs, and lambdas from AST to HIR
pub struct BlockLoweringFactory;

impl BlockLoweringFactory {
    /// Lower a block definition from AST BlockCall or lambda/proc expressions
    pub fn lower_block_def(
        &self,
        params: &[Param],
        body: &[Stmt],
        is_lambda: bool,
        span: SourceSpan,
    ) -> HirBlockDef {
        let hir_params = params.iter().map(|p| self.lower_param(p)).collect();
        let (captured_vars, captures_self) = self.analyze_captures(body, params);

        HirBlockDef {
            params: hir_params,
            body: body.iter().filter_map(AstLowering::lower_stmt).collect(),
            is_lambda,
            captures_self,
            captured_vars,
            span,
        }
    }

    fn lower_param(&self, param: &Param) -> HirBlockParam {
        HirBlockParam {
            name: param.name.clone(),
            default_value: param.default.as_ref().map(|e| AstLowering::lower_expr(e)),
            splat: matches!(param.kind, ParamKind::Rest),
            block: matches!(param.kind, ParamKind::Block),
            span: param.span,
        }
    }

    /// Analyze variable captures using the enhanced capture analyzer
    fn analyze_captures(&self, body: &[Stmt], params: &[Param]) -> (Vec<String>, bool) {
        let mut analyzer = CaptureAnalyzer::new();
        // Add block parameters as local vars so they're not captured
        for param in params {
            analyzer.local_vars.insert(param.name.clone());
        }
        analyzer.analyze(body)
    }
}

/// Analyzer for finding captured variables from outer scope
pub struct CaptureAnalyzer {
    captured_vars: Vec<String>,
    local_vars: std::collections::HashSet<String>,
    captured_self: bool,
}

impl CaptureAnalyzer {
    pub fn new() -> Self {
        Self {
            captured_vars: Vec::new(),
            local_vars: std::collections::HashSet::new(),
            captured_self: false,
        }
    }

    pub fn analyze(mut self, body: &[Stmt]) -> (Vec<String>, bool) {
        self.analyze_stmts(body);
        (self.captured_vars, self.captured_self)
    }

    pub fn analyze_stmts(&mut self, stmts: &[Stmt]) {
        for stmt in stmts {
            self.analyze_stmt(stmt);
        }
    }

    fn analyze_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Expr(es) => self.analyze_expr(&es.expr),
            Stmt::Assignment(a) => {
                self.analyze_expr(&a.value);
                if let AssignTarget::LocalVar(name) = &a.target {
                    self.local_vars.insert(name.clone());
                }
            }
            Stmt::CompoundAssignment(ca) => {
                self.analyze_expr(&ca.value);
                if let AssignTarget::LocalVar(name) = &ca.target {
                    self.local_vars.insert(name.clone());
                }
            }
            Stmt::MethodDef(def) => {
                // Method definitions introduce their own scope
                let mut inner = Self::new();
                for param in &def.params {
                    inner.local_vars.insert(param.name.clone());
                }
                inner.analyze_stmts(&def.body);
                // Methods don't capture outer scope by default, but body might reference outer vars
                for (var, _) in inner.captured_vars.iter().zip(0..) {
                    if !self.local_vars.contains(var) && !self.captured_vars.contains(var) {
                        self.captured_vars.push(var.clone());
                    }
                }
            }
            Stmt::If(s) => {
                self.analyze_expr(&s.condition);
                self.analyze_stmts(&s.then_body);
                for elsif in &s.elsif_clauses {
                    self.analyze_expr(&elsif.condition);
                    self.analyze_stmts(&elsif.body);
                }
                if let Some(else_body) = &s.else_body {
                    self.analyze_stmts(else_body);
                }
            }
            Stmt::Unless(s) => {
                self.analyze_expr(&s.condition);
                self.analyze_stmts(&s.body);
                if let Some(else_body) = &s.else_body {
                    self.analyze_stmts(else_body);
                }
            }
            Stmt::While(s) => {
                self.analyze_expr(&s.condition);
                self.analyze_stmts(&s.body);
            }
            Stmt::Until(s) => {
                self.analyze_expr(&s.condition);
                self.analyze_stmts(&s.body);
            }
            Stmt::For(s) => {
                self.local_vars.insert(s.var.clone());
                self.analyze_expr(&s.iterable);
                self.analyze_stmts(&s.body);
            }
            Stmt::Case(s) => {
                if let Some(subject) = &s.subject {
                    self.analyze_expr(subject);
                }
                for clause in &s.when_clauses {
                    for pattern in &clause.patterns {
                        self.analyze_expr(pattern);
                    }
                    self.analyze_stmts(&clause.body);
                }
                if let Some(else_body) = &s.else_body {
                    self.analyze_stmts(else_body);
                }
            }
            Stmt::Return(s) => {
                if let Some(val) = &s.value {
                    self.analyze_expr(val);
                }
            }
            Stmt::Yield(s) => {
                for arg in &s.args {
                    self.analyze_expr(arg);
                }
                // yield implies block usage, captured self might be needed
                self.captured_self = true;
            }
            Stmt::Break(s) => {
                if let Some(val) = &s.value {
                    self.analyze_expr(val);
                }
            }
            Stmt::Next(s) => {
                if let Some(val) = &s.value {
                    self.analyze_expr(val);
                }
            }
            Stmt::BeginRescue(s) => {
                self.analyze_stmts(&s.body);
                for rescue in &s.rescue_clauses {
                    if let Some(var) = &rescue.var {
                        self.local_vars.insert(var.clone());
                    }
                    self.analyze_stmts(&rescue.body);
                }
                if let Some(else_body) = &s.else_body {
                    self.analyze_stmts(else_body);
                }
                if let Some(ensure_body) = &s.ensure_body {
                    self.analyze_stmts(ensure_body);
                }
            }
            _ => {}
        }
    }

    fn analyze_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::LocalVar(v) => {
                if !self.local_vars.contains(&v.name) && !self.captured_vars.contains(&v.name) {
                    self.captured_vars.push(v.name.clone());
                }
            }
            Expr::SelfExpr(_) => {
                self.captured_self = true;
            }
            Expr::InstanceVar(_) | Expr::ClassVar(_) | Expr::GlobalVar(_) | Expr::ConstRef(_) => {
                // These don't require local capture, but self might be needed for ivar access
            }
            Expr::BinaryOp(op) => {
                self.analyze_expr(&op.left);
                self.analyze_expr(&op.right);
            }
            Expr::UnaryOp(op) => {
                self.analyze_expr(&op.operand);
            }
            Expr::MethodCall(c) => {
                if let Some(recv) = &c.receiver {
                    self.analyze_expr(recv);
                } else {
                    // No receiver means implicit self
                    self.captured_self = true;
                }
                for arg in &c.args {
                    self.analyze_expr(arg);
                }
            }
            Expr::BlockCall(bc) => {
                if let Some(recv) = &bc.call.receiver {
                    self.analyze_expr(recv);
                }
                for arg in &bc.call.args {
                    self.analyze_expr(arg);
                }
                // Analyze block body with new scope
                let mut inner = Self::new();
                for param in &bc.params {
                    inner.local_vars.insert(param.name.clone());
                }
                inner.analyze_stmts(&bc.body);
                // Merge captures from block
                for var in inner.captured_vars {
                    if !self.local_vars.contains(&var) && !self.captured_vars.contains(&var) {
                        self.captured_vars.push(var);
                    }
                }
                if inner.captured_self {
                    self.captured_self = true;
                }
            }
            Expr::Lambda(l) => {
                let mut inner = Self::new();
                for param in &l.params {
                    inner.local_vars.insert(param.name.clone());
                }
                inner.analyze_stmts(&l.body);
                for var in inner.captured_vars {
                    if !self.local_vars.contains(&var) && !self.captured_vars.contains(&var) {
                        self.captured_vars.push(var);
                    }
                }
                if inner.captured_self {
                    self.captured_self = true;
                }
            }
            Expr::Proc(l) => {
                let mut inner = Self::new();
                for param in &l.params {
                    inner.local_vars.insert(param.name.clone());
                }
                inner.analyze_stmts(&l.body);
                for var in inner.captured_vars {
                    if !self.local_vars.contains(&var) && !self.captured_vars.contains(&var) {
                        self.captured_vars.push(var);
                    }
                }
                if inner.captured_self {
                    self.captured_self = true;
                }
            }
            Expr::ArrayLit(a) => {
                for elem in &a.elements {
                    self.analyze_expr(elem);
                }
            }
            Expr::HashLit(h) => {
                for (k, v) in &h.entries {
                    self.analyze_expr(k);
                    self.analyze_expr(v);
                }
            }
            Expr::InterpolatedString(s) => {
                for part in &s.parts {
                    if let StringPart::Interpolation(expr) = part {
                        self.analyze_expr(expr);
                    }
                }
            }
            Expr::Ternary(t) => {
                self.analyze_expr(&t.condition);
                self.analyze_expr(&t.then_expr);
                self.analyze_expr(&t.else_expr);
            }
            Expr::RangeLit(r) => {
                self.analyze_expr(&r.start);
                self.analyze_expr(&r.end);
            }
            Expr::Defined(d) => {
                self.analyze_expr(&d.expr);
            }
            Expr::PatternMatch(pm) => {
                self.analyze_expr(&pm.subject);
                self.analyze_expr(&pm.pattern);
            }
            Expr::YieldExpr(y) => {
                for arg in &y.args {
                    self.analyze_expr(arg);
                }
                self.captured_self = true;
            }
            // Literals don't capture anything
            Expr::IntegerLit(_) | Expr::FloatLit(_) | Expr::StringLit(_) |
            Expr::SymbolLit(_) | Expr::BoolLit(_) | Expr::NilLit(_) |
            Expr::RegexLit(_) | Expr::SuperCall(_) => {}
        }
    }
}

/// Factory for lowering module and class definitions
pub struct ModuleLoweringFactory;

impl ModuleLoweringFactory {
    /// Lower a module definition
    pub fn lower_module_def(&self, def: &ModuleDef) -> HirModuleDef {
        let body: Vec<HirNode> = def.body.iter().filter_map(AstLowering::lower_stmt).collect();
        let nesting_path = vec![def.name.clone()];

        HirModuleDef {
            name: def.name.clone(),
            body,
            nesting_path,
            span: def.span,
        }
    }

    /// Lower a class definition (already exists but using factory pattern)
    #[allow(dead_code)]
    pub fn lower_class_def(&self, def: &ClassDef) -> HirClassDef {
        let body: Vec<HirNode> = def.body.iter().filter_map(AstLowering::lower_stmt).collect();
        let superclass = def.superclass.as_ref().and_then(|e| {
            if let Expr::ConstRef(c) = e.as_ref() {
                Some(c.path.join("::"))
            } else {
                None
            }
        });

        HirClassDef {
            name: def.name.clone(),
            superclass,
            body,
            span: def.span,
        }
    }
}

/// Lowers an AST Program into HIR nodes.
pub struct AstLowering;

impl AstLowering {
    pub fn lower(program: &Program) -> HirModule {
        let mut nodes = Vec::new();
        for stmt in &program.body {
            if let Some(n) = Self::lower_stmt(stmt) {
                nodes.push(n);
            }
        }
        HirModule { name: "main".into(), nodes }
    }

    fn lower_stmt(stmt: &Stmt) -> Option<HirNode> {
        match stmt {
            Stmt::Expr(es) => Some(Self::lower_expr(&es.expr)),
            Stmt::Assignment(a) => {
                let target = Self::lower_assign_target(&a.target, a.span);
                let value = Self::lower_expr(&a.value);
                Some(HirNode::Assign(Box::new(HirAssign { target, value, span: a.span })))
            }
            Stmt::CompoundAssignment(ca) => {
                let target = Self::lower_assign_target(&ca.target, ca.span);
                let current = HirNode::VarRef(target.clone());
                let value = Self::lower_expr(&ca.value);
                let op = Self::lower_binop(&ca.op);
                let combined = HirNode::BinOp(Box::new(HirBinOp {
                    left: current, op, right: value, span: ca.span,
                }));
                Some(HirNode::Assign(Box::new(HirAssign { target, value: combined, span: ca.span })))
            }
            Stmt::MethodDef(def) => {
                let params: Vec<String> = def.params.iter().map(|p| p.name.clone()).collect();
                let body: Vec<HirNode> = def.body.iter().filter_map(Self::lower_stmt).collect();
                Some(HirNode::FuncDef(Box::new(HirFuncDef {
                    name: def.name.clone(), params, body,
                    is_class_method: def.is_class_method, span: def.span,
                })))
            }
            Stmt::ClassDef(def) => {
                let body: Vec<HirNode> = def.body.iter().filter_map(Self::lower_stmt).collect();
                let superclass = def.superclass.as_ref().and_then(|e| {
                    if let Expr::ConstRef(c) = e.as_ref() {
                        Some(c.path.join("::"))
                    } else { None }
                });
                Some(HirNode::ClassDef(Box::new(HirClassDef {
                    name: def.name.clone(), superclass, body, span: def.span,
                })))
            }
            Stmt::ModuleDef(def) => {
                let factory = ModuleLoweringFactory;
                Some(HirNode::ModuleDef(Box::new(factory.lower_module_def(def))))
            }
            Stmt::If(s) => {
                let cond = Self::lower_expr(&s.condition);
                let then_body: Vec<HirNode> = s.then_body.iter().filter_map(Self::lower_stmt).collect();
                let mut else_body: Vec<HirNode> = Vec::new();
                for clause in &s.elsif_clauses {
                    let inner_cond = Self::lower_expr(&clause.condition);
                    let inner_body: Vec<HirNode> = clause.body.iter().filter_map(Self::lower_stmt).collect();
                    else_body.push(HirNode::Branch(Box::new(HirBranch {
                        condition: inner_cond, then_body: inner_body,
                        else_body: vec![], span: clause.span,
                    })));
                }
                if let Some(eb) = &s.else_body {
                    let stmts: Vec<HirNode> = eb.iter().filter_map(Self::lower_stmt).collect();
                    if else_body.is_empty() {
                        else_body = stmts;
                    } else {
                        // Nest the else into the last elsif
                        if let Some(HirNode::Branch(ref mut last)) = else_body.last_mut() {
                            last.else_body = stmts;
                        }
                    }
                }
                Some(HirNode::Branch(Box::new(HirBranch {
                    condition: cond, then_body, else_body, span: s.span,
                })))
            }
            Stmt::Unless(s) => {
                let cond = HirNode::UnOp(Box::new(HirUnOp {
                    op: HirUnaryOp::Not, operand: Self::lower_expr(&s.condition), span: s.span,
                }));
                let then_body: Vec<HirNode> = s.body.iter().filter_map(Self::lower_stmt).collect();
                let else_body = s.else_body.as_ref().map(|eb| {
                    eb.iter().filter_map(Self::lower_stmt).collect()
                }).unwrap_or_default();
                Some(HirNode::Branch(Box::new(HirBranch {
                    condition: cond, then_body, else_body, span: s.span,
                })))
            }
            Stmt::While(s) => {
                let cond = Self::lower_expr(&s.condition);
                let body: Vec<HirNode> = s.body.iter().filter_map(Self::lower_stmt).collect();
                Some(HirNode::Loop(Box::new(HirLoop {
                    condition: cond, body, is_while: true, span: s.span,
                })))
            }
            Stmt::Until(s) => {
                let cond = HirNode::UnOp(Box::new(HirUnOp {
                    op: HirUnaryOp::Not, operand: Self::lower_expr(&s.condition), span: s.span,
                }));
                let body: Vec<HirNode> = s.body.iter().filter_map(Self::lower_stmt).collect();
                Some(HirNode::Loop(Box::new(HirLoop {
                    condition: cond, body, is_while: true, span: s.span,
                })))
            }
            Stmt::For(s) => {
                // Desugar `for x in iter` → `iter.each { |x| body }`
                let iter = Self::lower_expr(&s.iterable);
                let body: Vec<HirNode> = s.body.iter().filter_map(Self::lower_stmt).collect();
                Some(HirNode::Call(Box::new(HirCall {
                    receiver: Some(iter), method: "each".into(), args: vec![],
                    block: Some(HirBlock { 
                        params: vec![HirBlockParam { name: s.var.clone(), default_value: None, splat: false, block: false, span: s.span }], 
                        body,
                        captured_vars: vec![],
                        captures_self: false,
                    }),
                    span: s.span,
                })))
            }
            Stmt::Return(s) => {
                let value = s.value.as_ref().map(|v| Self::lower_expr(v));
                Some(HirNode::Return(Box::new(HirReturn { value, span: s.span })))
            }
            Stmt::Yield(s) => {
                let args: Vec<HirNode> = s.args.iter().map(Self::lower_expr).collect();
                Some(HirNode::Yield(args))
            }
            Stmt::Break(_) => Some(HirNode::Break),
            Stmt::Next(_) => Some(HirNode::Next),
            Stmt::Case(s) => Self::lower_case(s),
            Stmt::BeginRescue(s) => Self::lower_begin_rescue(s),
            Stmt::Alias(_) | Stmt::Require(_) | Stmt::AttrDecl(_) => {
                Some(HirNode::Nop)
            }
            Stmt::MixinStmt(m) => {
                let method_name = match m.kind {
                    jdruby_ast::MixinKind::Include => "include",
                    jdruby_ast::MixinKind::Extend => "extend",
                    jdruby_ast::MixinKind::Prepend => "prepend",
                };
                Some(HirNode::Call(Box::new(HirCall {
                    receiver: None,
                    method: method_name.into(),
                    args: vec![Self::lower_expr(&m.module)],
                    block: None,
                    span: m.span,
                })))
            }
        }
    }

    fn lower_case(s: &CaseStmt) -> Option<HirNode> {
        // Desugar case/when into nested if/elsif
        let subject = s.subject.as_ref().map(|e| Self::lower_expr(e));
        let mut result: Option<HirNode> = s.else_body.as_ref().map(|eb| {
            HirNode::Seq(eb.iter().filter_map(Self::lower_stmt).collect())
        });
        for clause in s.when_clauses.iter().rev() {
            let cond = if clause.patterns.len() == 1 {
                if let Some(ref subj) = subject {
                    HirNode::BinOp(Box::new(HirBinOp {
                        left: subj.clone(), op: HirOp::Eq,
                        right: Self::lower_expr(&clause.patterns[0]),
                        span: clause.span,
                    }))
                } else {
                    Self::lower_expr(&clause.patterns[0])
                }
            } else {
                // Multiple patterns: combine with OR
                let mut combined = Self::lower_expr(&clause.patterns[0]);
                for pat in &clause.patterns[1..] {
                    combined = HirNode::BinOp(Box::new(HirBinOp {
                        left: combined, op: HirOp::Or,
                        right: Self::lower_expr(pat), span: clause.span,
                    }));
                }
                combined
            };
            let then_body: Vec<HirNode> = clause.body.iter().filter_map(Self::lower_stmt).collect();
            let else_body = result.map(|r| vec![r]).unwrap_or_default();
            result = Some(HirNode::Branch(Box::new(HirBranch {
                condition: cond, then_body, else_body, span: clause.span,
            })));
        }
        result.or(Some(HirNode::Nop))
    }

    fn lower_begin_rescue(s: &BeginRescueStmt) -> Option<HirNode> {
        let body: Vec<HirNode> = s.body.iter().filter_map(Self::lower_stmt).collect();
        
        let rescue_clauses: Vec<HirRescueClause> = s.rescue_clauses.iter().map(|rc| {
            HirRescueClause {
                exceptions: rc.exceptions.iter().map(Self::lower_expr).collect(),
                var: rc.var.clone(),
                body: rc.body.iter().filter_map(Self::lower_stmt).collect(),
                span: rc.span,
            }
        }).collect();
        
        let else_body = s.else_body.as_ref().map(|eb| {
            eb.iter().filter_map(Self::lower_stmt).collect()
        });
        
        let ensure_body = s.ensure_body.as_ref().map(|eb| {
            eb.iter().filter_map(Self::lower_stmt).collect()
        });
        
        Some(HirNode::ExceptionBegin(Box::new(HirExceptionBegin {
            body,
            rescue_clauses,
            else_body,
            ensure_body,
            span: s.span,
        })))
    }

    fn lower_expr(expr: &Expr) -> HirNode {
        match expr {
            Expr::IntegerLit(n) => HirNode::Literal(HirLiteral {
                value: HirLiteralValue::Integer(n.value), span: n.span,
            }),
            Expr::FloatLit(n) => HirNode::Literal(HirLiteral {
                value: HirLiteralValue::Float(n.value), span: n.span,
            }),
            Expr::StringLit(n) => HirNode::Literal(HirLiteral {
                value: HirLiteralValue::String(n.value.clone()), span: n.span,
            }),
            Expr::SymbolLit(n) => HirNode::Literal(HirLiteral {
                value: HirLiteralValue::Symbol(n.name.clone()), span: n.span,
            }),
            Expr::BoolLit(n) => HirNode::Literal(HirLiteral {
                value: HirLiteralValue::Bool(n.value), span: n.span,
            }),
            Expr::NilLit(n) => HirNode::Literal(HirLiteral {
                value: HirLiteralValue::Nil, span: n.span,
            }),
            Expr::ArrayLit(a) => HirNode::Literal(HirLiteral {
                value: HirLiteralValue::Array(a.elements.iter().map(Self::lower_expr).collect()),
                span: a.span,
            }),
            Expr::HashLit(h) => HirNode::Literal(HirLiteral {
                value: HirLiteralValue::Hash(h.entries.iter().map(|(k, v)| {
                    (Self::lower_expr(k), Self::lower_expr(v))
                }).collect()),
                span: h.span,
            }),
            Expr::LocalVar(v) => HirNode::VarRef(HirVarRef {
                name: v.name.clone(), scope: VarScope::Local, span: v.span,
            }),
            Expr::InstanceVar(v) => HirNode::VarRef(HirVarRef {
                name: v.name.clone(), scope: VarScope::Instance, span: v.span,
            }),
            Expr::ClassVar(v) => HirNode::VarRef(HirVarRef {
                name: v.name.clone(), scope: VarScope::Class, span: v.span,
            }),
            Expr::GlobalVar(v) => HirNode::VarRef(HirVarRef {
                name: v.name.clone(), scope: VarScope::Global, span: v.span,
            }),
            Expr::ConstRef(c) => HirNode::VarRef(HirVarRef {
                name: c.path.join("::"), scope: VarScope::Local, span: c.span,
            }),
            Expr::SelfExpr(s) => HirNode::VarRef(HirVarRef {
                name: "self".into(), scope: VarScope::Local, span: s.span,
            }),
            Expr::BinaryOp(op) => {
                // Ruby `<<` is a method call, not a bitshift
                if op.op == BinOperator::Shl {
                    return HirNode::Call(Box::new(HirCall {
                        receiver: Some(Self::lower_expr(&op.left)),
                        method: "<<".into(),
                        args: vec![Self::lower_expr(&op.right)],
                        block: None,
                        span: op.span,
                    }));
                }
                HirNode::BinOp(Box::new(HirBinOp {
                    left: Self::lower_expr(&op.left),
                    op: Self::lower_binop(&op.op),
                    right: Self::lower_expr(&op.right),
                    span: op.span,
                }))
            }
            Expr::UnaryOp(op) => HirNode::UnOp(Box::new(HirUnOp {
                op: match op.op {
                    UnOperator::Neg | UnOperator::Pos => HirUnaryOp::Neg,
                    UnOperator::Not => HirUnaryOp::Not,
                    UnOperator::BitNot => HirUnaryOp::BitNot,
                },
                operand: Self::lower_expr(&op.operand),
                span: op.span,
            })),
            Expr::MethodCall(call) => {
                // Handle block_arg for &:sym syntax
                let block = call.block_arg.as_ref().map(|block_expr| {
                    // Check if it's a symbol literal (e.g., &:run)
                    if let Expr::SymbolLit(sym) = block_expr.as_ref() {
                        // Create a block that will be converted via SymbolToProc in MIR
                        HirBlock {
                            params: vec![],
                            body: vec![HirNode::Literal(HirLiteral {
                                value: HirLiteralValue::Symbol(sym.name.clone()),
                                span: sym.span,
                            })],
                            captured_vars: vec![],
                            captures_self: false,
                        }
                    } else {
                        // Other block expressions - not yet supported
                        HirBlock {
                            params: vec![],
                            body: vec![],
                            captured_vars: vec![],
                            captures_self: false,
                        }
                    }
                });
                HirNode::Call(Box::new(HirCall {
                    receiver: call.receiver.as_ref().map(|r| Self::lower_expr(r)),
                    method: call.method.clone(),
                    args: call.args.iter().map(Self::lower_expr).collect(),
                    block,
                    span: call.span,
                }))
            }
            Expr::BlockCall(bc) => {
                // Special handling for define_method - it defines a method dynamically
                if bc.call.method == "define_method" {
                    // The block body becomes the method body
                    let factory = BlockLoweringFactory;
                    let block_def = factory.lower_block_def(&bc.params, &bc.body, false, bc.span);
                    
                    // Create a DefineMethod HIR node (or use Call with special marker)
                    // For now, we'll create a Call but mark it specially
                    HirNode::Call(Box::new(HirCall {
                        receiver: bc.call.receiver.as_ref().map(|r| Self::lower_expr(r)),
                        method: "define_method".into(),
                        args: bc.call.args.iter().map(Self::lower_expr).collect(),
                        block: Some(HirBlock {
                            params: block_def.params,
                            body: block_def.body,
                            captured_vars: block_def.captured_vars,
                            captures_self: block_def.captures_self,
                        }),
                        span: bc.span,
                    }))
                } else {
                    let factory = BlockLoweringFactory;
                    let block_def = factory.lower_block_def(&bc.params, &bc.body, false, bc.span);
                    HirNode::Call(Box::new(HirCall {
                        receiver: bc.call.receiver.as_ref().map(|r| Self::lower_expr(r)),
                        method: bc.call.method.clone(),
                        args: bc.call.args.iter().map(Self::lower_expr).collect(),
                        block: Some(HirBlock {
                            params: block_def.params,
                            body: block_def.body,
                            captured_vars: block_def.captured_vars,
                            captures_self: block_def.captures_self,
                        }),
                        span: bc.span,
                    }))
                }
            }
            Expr::SuperCall(s) => HirNode::Call(Box::new(HirCall {
                receiver: None, method: "super".into(),
                args: s.args.iter().map(Self::lower_expr).collect(),
                block: None, span: s.span,
            })),
            Expr::YieldExpr(y) => HirNode::Yield(y.args.iter().map(Self::lower_expr).collect()),
            Expr::Lambda(l) => {
                let factory = BlockLoweringFactory;
                let block_def = factory.lower_block_def(&l.params, &l.body, true, l.span);
                HirNode::LambdaDef(Box::new(HirLambdaDef {
                    params: block_def.params,
                    body: block_def.body,
                    captures_self: block_def.captures_self,
                    captured_vars: block_def.captured_vars,
                    span: l.span,
                }))
            }
            Expr::Proc(p) => {
                let factory = BlockLoweringFactory;
                let block_def = factory.lower_block_def(&p.params, &p.body, false, p.span);
                HirNode::ProcDef(Box::new(HirProcDef {
                    params: block_def.params,
                    body: block_def.body,
                    captures_self: block_def.captures_self,
                    captured_vars: block_def.captured_vars,
                    span: p.span,
                }))
            },
            Expr::RangeLit(r) => HirNode::Call(Box::new(HirCall {
                receiver: None, method: "Range.new".into(),
                args: vec![Self::lower_expr(&r.start), Self::lower_expr(&r.end),
                    HirNode::Literal(HirLiteral {
                        value: HirLiteralValue::Bool(r.exclusive),
                        span: r.span,
                    })],
                block: None, span: r.span,
            })),
            Expr::Ternary(t) => HirNode::Branch(Box::new(HirBranch {
                condition: Self::lower_expr(&t.condition),
                then_body: vec![Self::lower_expr(&t.then_expr)],
                else_body: vec![Self::lower_expr(&t.else_expr)],
                span: t.span,
            })),
            Expr::Defined(d) => HirNode::Call(Box::new(HirCall {
                receiver: None, method: "defined?".into(),
                args: vec![Self::lower_expr(&d.expr)],
                block: None, span: d.span,
            })),
            Expr::InterpolatedString(s) => Self::lower_interpolated_string(s),
            Expr::RegexLit(r) => HirNode::Literal(HirLiteral {
                value: HirLiteralValue::String(r.pattern.clone()), span: r.span,
            }),
            Expr::PatternMatch(pm) => HirNode::Call(Box::new(HirCall {
                receiver: Some(Self::lower_expr(&pm.subject)),
                method: "===".into(),
                args: vec![Self::lower_expr(&pm.pattern)],
                block: None, span: pm.span,
            })),
        }
    }

    fn lower_binop(op: &BinOperator) -> HirOp {
        match op {
            BinOperator::Add => HirOp::Add,
            BinOperator::Sub => HirOp::Sub,
            BinOperator::Mul => HirOp::Mul,
            BinOperator::Div => HirOp::Div,
            BinOperator::Mod => HirOp::Mod,
            BinOperator::Pow => HirOp::Pow,
            BinOperator::Eq | BinOperator::CaseEq => HirOp::Eq,
            BinOperator::NotEq => HirOp::NotEq,
            BinOperator::Lt => HirOp::Lt,
            BinOperator::Gt => HirOp::Gt,
            BinOperator::LtEq => HirOp::LtEq,
            BinOperator::GtEq => HirOp::GtEq,
            BinOperator::Spaceship => HirOp::Cmp,
            BinOperator::Match | BinOperator::NotMatch => HirOp::Eq,
            BinOperator::And => HirOp::And,
            BinOperator::Or => HirOp::Or,
            BinOperator::BitAnd => HirOp::BitAnd,
            BinOperator::BitOr => HirOp::BitOr,
            BinOperator::BitXor => HirOp::BitXor,
            BinOperator::Shl => HirOp::Shl, // only reached for non-Ruby-<< contexts
            BinOperator::Shr => HirOp::Shr,
            BinOperator::Range | BinOperator::RangeExcl => HirOp::Add, // handled separately
        }
    }

    fn lower_assign_target(target: &AssignTarget, span: SourceSpan) -> HirVarRef {
        match target {
            AssignTarget::LocalVar(n) => HirVarRef { name: n.clone(), scope: VarScope::Local, span },
            AssignTarget::InstanceVar(n) => HirVarRef { name: n.clone(), scope: VarScope::Instance, span },
            AssignTarget::ClassVar(n) => HirVarRef { name: n.clone(), scope: VarScope::Class, span },
            AssignTarget::GlobalVar(n) => HirVarRef { name: n.clone(), scope: VarScope::Global, span },
            AssignTarget::Constant(n) => HirVarRef { name: n.clone(), scope: VarScope::Local, span },
            AssignTarget::Index(_, _) => HirVarRef { name: "<index>".into(), scope: VarScope::Local, span },
            AssignTarget::Attribute(_, attr) => HirVarRef { name: attr.clone(), scope: VarScope::Instance, span },
        }
    }

    /// Lower interpolated string: `"hello #{expr}"` → concat chain
    fn lower_interpolated_string(s: &InterpolatedString) -> HirNode {
        let mut parts: Vec<HirNode> = Vec::new();
        for part in &s.parts {
            match part {
                StringPart::Literal(text) => {
                    parts.push(HirNode::Literal(HirLiteral {
                        value: HirLiteralValue::String(text.clone()),
                        span: s.span,
                    }));
                }
                StringPart::Interpolation(expr) => {
                    // Wrap in to_s call for non-string expressions
                    let lowered = Self::lower_expr(expr);
                    parts.push(HirNode::Call(Box::new(HirCall {
                        receiver: Some(lowered),
                        method: "to_s".into(),
                        args: vec![],
                        block: None,
                        span: s.span,
                    })));
                }
            }
        }

        if parts.is_empty() {
            return HirNode::Literal(HirLiteral {
                value: HirLiteralValue::String(String::new()),
                span: s.span,
            });
        }

        let mut result = parts.remove(0);
        for part in parts {
            result = HirNode::Call(Box::new(HirCall {
                receiver: Some(result),
                method: "+".into(),
                args: vec![part],
                block: None,
                span: s.span,
            }));
        }
        result
    }
}

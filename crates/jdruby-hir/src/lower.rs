use jdruby_ast::*;
use jdruby_common::SourceSpan;
use crate::nodes::*;

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
                let body: Vec<HirNode> = def.body.iter().filter_map(Self::lower_stmt).collect();
                Some(HirNode::ClassDef(Box::new(HirClassDef {
                    name: def.name.clone(), superclass: None, body, span: def.span,
                })))
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
                    block: Some(HirBlock { params: vec![s.var.clone()], body }),
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
            Stmt::BeginRescue(_) => Some(HirNode::Nop), // simplified for now
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
            Expr::MethodCall(call) => HirNode::Call(Box::new(HirCall {
                receiver: call.receiver.as_ref().map(|r| Self::lower_expr(r)),
                method: call.method.clone(),
                args: call.args.iter().map(Self::lower_expr).collect(),
                block: None, span: call.span,
            })),
            Expr::BlockCall(bc) => HirNode::Call(Box::new(HirCall {
                receiver: bc.call.receiver.as_ref().map(|r| Self::lower_expr(r)),
                method: bc.call.method.clone(),
                args: bc.call.args.iter().map(Self::lower_expr).collect(),
                block: Some(HirBlock {
                    params: bc.params.iter().map(|p| p.name.clone()).collect(),
                    body: bc.body.iter().filter_map(Self::lower_stmt).collect(),
                }),
                span: bc.span,
            })),
            Expr::SuperCall(s) => HirNode::Call(Box::new(HirCall {
                receiver: None, method: "super".into(),
                args: s.args.iter().map(Self::lower_expr).collect(),
                block: None, span: s.span,
            })),
            Expr::YieldExpr(y) => HirNode::Yield(y.args.iter().map(Self::lower_expr).collect()),
            Expr::Lambda(l) => HirNode::FuncDef(Box::new(HirFuncDef {
                name: "<lambda>".into(),
                params: l.params.iter().map(|p| p.name.clone()).collect(),
                body: l.body.iter().filter_map(Self::lower_stmt).collect(),
                is_class_method: false, span: l.span,
            })),
            Expr::Proc(p) => HirNode::FuncDef(Box::new(HirFuncDef {
                name: "<proc>".into(),
                params: p.params.iter().map(|pp| pp.name.clone()).collect(),
                body: p.body.iter().filter_map(Self::lower_stmt).collect(),
                is_class_method: false, span: p.span,
            })),
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
            result = HirNode::BinOp(Box::new(HirBinOp {
                left: result,
                op: HirOp::Add, // String concatenation
                right: part,
                span: s.span,
            }));
        }
        result
    }
}

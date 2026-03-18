use jdruby_ast::*;
use jdruby_common::{Diagnostic, SourceSpan};
use crate::scope::*;
use crate::types::*;

/// The semantic analyzer — performs multi-pass analysis on a Ruby AST.
pub struct SemanticAnalyzer {
    scopes: ScopeStack,
    diagnostics: Vec<Diagnostic>,
    /// Class hierarchy: class_name → superclass_name
    class_hierarchy: std::collections::HashMap<String, Option<String>>,
    /// Module definitions: module_name → list of method names
    module_methods: std::collections::HashMap<String, Vec<String>>,
}

impl SemanticAnalyzer {
    pub fn new() -> Self {
        let mut sa = Self {
            scopes: ScopeStack::new(),
            diagnostics: Vec::new(),
            class_hierarchy: std::collections::HashMap::new(),
            module_methods: std::collections::HashMap::new(),
        };
        sa.define_builtins();
        sa
    }

    /// Run all analysis passes on the program.
    pub fn analyze(&mut self, program: &Program) -> Vec<Diagnostic> {
        // Pass 1: Collect symbols (classes, modules, top-level methods)
        self.pass_collect(program);
        // Pass 2: Resolve types and analyze bodies
        self.pass_analyze(program);
        std::mem::take(&mut self.diagnostics)
    }

    /// Define built-in Ruby methods and constants.
    fn define_builtins(&mut self) {
        let builtins = [
            ("puts", RubyType::Nil),
            ("print", RubyType::Nil),
            ("p", RubyType::Any),
            ("gets", RubyType::Optional(Box::new(RubyType::String))),
            ("raise", RubyType::Void),
            ("require", RubyType::Bool),
            ("require_relative", RubyType::Bool),
            ("rand", RubyType::Any),
            ("sleep", RubyType::Integer),
            ("exit", RubyType::Void),
            ("abort", RubyType::Void),
            ("at_exit", RubyType::Proc),
            ("freeze", RubyType::SelfType),
            ("frozen?", RubyType::Bool),
            ("nil?", RubyType::Bool),
            ("is_a?", RubyType::Bool),
            ("kind_of?", RubyType::Bool),
            ("respond_to?", RubyType::Bool),
            ("send", RubyType::Any),
            ("class", RubyType::Class("Class".into())),
            ("to_s", RubyType::String),
            ("to_i", RubyType::Integer),
            ("to_f", RubyType::Float),
            ("to_a", RubyType::Array(Box::new(RubyType::Any))),
            ("inspect", RubyType::String),
            ("hash", RubyType::Integer),
            ("dup", RubyType::SelfType),
            ("clone", RubyType::SelfType),
            ("tap", RubyType::SelfType),
            ("then", RubyType::Any),
            ("yield_self", RubyType::Any),
            ("object_id", RubyType::Integer),
            ("equal?", RubyType::Bool),
            ("eql?", RubyType::Bool),
            ("instance_of?", RubyType::Bool),
            ("method", RubyType::Proc),
            ("methods", RubyType::Array(Box::new(RubyType::Symbol))),
            ("instance_variables", RubyType::Array(Box::new(RubyType::Symbol))),
            ("instance_variable_get", RubyType::Any),
            ("instance_variable_set", RubyType::Any),
        ];
        for (name, ret_type) in builtins {
            self.scopes.define(Symbol {
                name: name.to_string(),
                kind: SymbolKind::Method(MethodSignature {
                    name: name.to_string(),
                    params: vec![],
                    return_type: ret_type,
                    visibility: Visibility::Public,
                    is_class_method: false,
                }),
                ty: RubyType::Proc,
                initialized: true,
                ref_count: 0,
                visibility: Visibility::Public,
            });
        }
        // Built-in constants
        for name in ["ARGV", "STDIN", "STDOUT", "STDERR", "ENV", "RUBY_VERSION",
            "RUBY_PLATFORM", "RUBY_ENGINE", "TRUE", "FALSE", "NIL",
            "__FILE__", "__LINE__", "__dir__", "__method__"]
        {
            self.scopes.define(Symbol {
                name: name.to_string(),
                kind: SymbolKind::Constant,
                ty: RubyType::Any,
                initialized: true, ref_count: 0,
                visibility: Visibility::Public,
            });
        }
        // Built-in classes
        for name in ["Object", "BasicObject", "Kernel", "Integer", "Float",
            "String", "Symbol", "Array", "Hash", "Range", "Regexp", "Proc",
            "NilClass", "TrueClass", "FalseClass", "IO", "File", "Dir",
            "Exception", "StandardError", "RuntimeError", "TypeError",
            "ArgumentError", "NameError", "NoMethodError", "ZeroDivisionError",
            "Comparable", "Enumerable", "Enumerator", "Struct", "Class", "Module",
            "Numeric", "Math", "Thread", "Mutex", "Fiber", "Process", "Signal",
            "Time", "Random", "Set", "GC", "Marshal", "Encoding"]
        {
            self.class_hierarchy.insert(name.to_string(), Some("Object".to_string()));
            self.scopes.define(Symbol {
                name: name.to_string(),
                kind: SymbolKind::Class { superclass: Some("Object".to_string()) },
                ty: RubyType::Class(name.to_string()),
                initialized: true, ref_count: 0,
                visibility: Visibility::Public,
            });
        }
    }

    // ═══════════════════════════════════════════════════════
    //  Pass 1: Collect symbols
    // ═══════════════════════════════════════════════════════

    fn pass_collect(&mut self, program: &Program) {
        for stmt in &program.body {
            self.collect_stmt(stmt);
        }
    }

    fn collect_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::MethodDef(def) => {
                let sig = MethodSignature {
                    name: def.name.clone(),
                    params: def.params.iter().map(|p| ParamSig {
                        name: p.name.clone(),
                        ty: RubyType::Any,
                        has_default: p.default.is_some(),
                        is_rest: p.kind == ParamKind::Rest,
                        is_keyword: p.kind == ParamKind::Keyword,
                        is_block: p.kind == ParamKind::Block,
                    }).collect(),
                    return_type: RubyType::Any,
                    visibility: self.scopes.current().current_visibility,
                    is_class_method: def.is_class_method,
                };
                self.scopes.define(Symbol {
                    name: def.name.clone(),
                    kind: SymbolKind::Method(sig),
                    ty: RubyType::Proc,
                    initialized: true, ref_count: 0,
                    visibility: self.scopes.current().current_visibility,
                });
            }
            Stmt::ClassDef(def) => {
                let super_name = match &def.superclass {
                    Some(expr) => self.expr_to_name(expr),
                    None => Some("Object".to_string()),
                };
                self.class_hierarchy.insert(def.name.clone(), super_name.clone());
                self.scopes.define(Symbol {
                    name: def.name.clone(),
                    kind: SymbolKind::Class { superclass: super_name },
                    ty: RubyType::Class(def.name.clone()),
                    initialized: true, ref_count: 0,
                    visibility: Visibility::Public,
                });
                // Collect methods inside the class
                self.scopes.push(ScopeKind::Class, Some(def.name.clone()));
                for s in &def.body { self.collect_stmt(s); }
                self.scopes.pop();
            }
            Stmt::ModuleDef(def) => {
                self.scopes.define(Symbol {
                    name: def.name.clone(),
                    kind: SymbolKind::Module,
                    ty: RubyType::Module(def.name.clone()),
                    initialized: true, ref_count: 0,
                    visibility: Visibility::Public,
                });
                self.scopes.push(ScopeKind::Module, Some(def.name.clone()));
                for s in &def.body { self.collect_stmt(s); }
                self.scopes.pop();
            }
            _ => {}
        }
    }

    // ═══════════════════════════════════════════════════════
    //  Pass 2: Analyze bodies
    // ═══════════════════════════════════════════════════════

    fn pass_analyze(&mut self, program: &Program) {
        for stmt in &program.body {
            self.analyze_stmt(stmt);
        }
    }

    fn analyze_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Expr(es) => { self.analyze_expr(&es.expr); }
            Stmt::Assignment(a) => self.analyze_assignment(a),
            Stmt::CompoundAssignment(ca) => self.analyze_compound_assignment(ca),
            Stmt::MethodDef(def) => self.analyze_method_def(def),
            Stmt::ClassDef(def) => self.analyze_class_def(def),
            Stmt::ModuleDef(def) => self.analyze_module_def(def),
            Stmt::If(s) => self.analyze_if(s),
            Stmt::Unless(s) => self.analyze_unless(s),
            Stmt::While(s) => self.analyze_while(s),
            Stmt::Until(s) => self.analyze_until(s),
            Stmt::For(s) => self.analyze_for(s),
            Stmt::Case(s) => self.analyze_case(s),
            Stmt::BeginRescue(s) => self.analyze_begin_rescue(s),
            Stmt::Return(s) => {
                if !self.scopes.in_method() {
                    self.warn("return used outside of method", s.span);
                }
                if let Some(v) = &s.value { self.analyze_expr(v); }
            }
            Stmt::Yield(s) => {
                if !self.scopes.in_method() {
                    self.diag("yield used outside of method", s.span);
                }
                for a in &s.args { self.analyze_expr(a); }
            }
            Stmt::Break(s) => {
                if let Some(v) = &s.value { self.analyze_expr(v); }
            }
            Stmt::Next(s) => {
                if let Some(v) = &s.value { self.analyze_expr(v); }
            }
            Stmt::Require(_) | Stmt::Alias(_) | Stmt::AttrDecl(_) | Stmt::MixinStmt(_) => {}
        }
    }

    fn analyze_assignment(&mut self, a: &AssignmentStmt) {
        let val_type = self.analyze_expr(&a.value);
        match &a.target {
            AssignTarget::LocalVar(name) => {
                if let Some(sym) = self.scopes.lookup_mut(name) {
                    sym.ty = val_type;
                    sym.initialized = true;
                } else {
                    self.scopes.define(Symbol {
                        name: name.clone(), kind: SymbolKind::LocalVar,
                        ty: val_type, initialized: true, ref_count: 0,
                        visibility: Visibility::Public,
                    });
                }
            }
            AssignTarget::InstanceVar(name) => {
                if !self.scopes.in_class() && !self.scopes.in_method() {
                    self.warn(&format!("instance variable {} used outside class/method", name), a.span);
                }
                self.scopes.define(Symbol {
                    name: name.clone(), kind: SymbolKind::InstanceVar,
                    ty: val_type, initialized: true, ref_count: 0,
                    visibility: Visibility::Private,
                });
            }
            AssignTarget::ClassVar(name) => {
                if !self.scopes.in_class() {
                    self.warn(&format!("class variable {} used outside class", name), a.span);
                }
                self.scopes.define(Symbol {
                    name: name.clone(), kind: SymbolKind::ClassVar,
                    ty: val_type, initialized: true, ref_count: 0,
                    visibility: Visibility::Private,
                });
            }
            AssignTarget::GlobalVar(name) => {
                self.scopes.define(Symbol {
                    name: name.clone(), kind: SymbolKind::GlobalVar,
                    ty: val_type, initialized: true, ref_count: 0,
                    visibility: Visibility::Public,
                });
            }
            AssignTarget::Constant(name) => {
                if self.scopes.lookup(name).is_some() {
                    self.warn(&format!("already initialized constant {}", name), a.span);
                }
                self.scopes.define(Symbol {
                    name: name.clone(), kind: SymbolKind::Constant,
                    ty: val_type, initialized: true, ref_count: 0,
                    visibility: Visibility::Public,
                });
            }
            AssignTarget::Index(recv, idx) => {
                self.analyze_expr(recv);
                self.analyze_expr(idx);
            }
            AssignTarget::Attribute(recv, _) => { self.analyze_expr(recv); }
        }
    }

    fn analyze_compound_assignment(&mut self, ca: &CompoundAssignmentStmt) {
        self.analyze_expr(&ca.value);
        // Ensure the target is defined
        match &ca.target {
            AssignTarget::LocalVar(name) => {
                if self.scopes.lookup(name).is_none() {
                    self.diag(&format!("undefined local variable '{}'", name), ca.span);
                }
            }
            _ => {}
        }
    }

    fn analyze_method_def(&mut self, def: &MethodDef) {
        self.scopes.push(ScopeKind::Method, Some(def.name.clone()));
        // Define parameters
        for param in &def.params {
            let ty = if let Some(default) = &param.default {
                self.analyze_expr(default)
            } else {
                RubyType::Any
            };
            self.scopes.define(Symbol {
                name: param.name.clone(), kind: SymbolKind::Param,
                ty, initialized: true, ref_count: 0,
                visibility: Visibility::Public,
            });
        }
        for stmt in &def.body { self.analyze_stmt(stmt); }
        self.scopes.pop();
    }

    fn analyze_class_def(&mut self, def: &ClassDef) {
        if let Some(sup) = &def.superclass {
            let name = self.expr_to_name(sup);
            if let Some(n) = &name {
                if self.scopes.lookup(n).is_none() && !self.class_hierarchy.contains_key(n) {
                    self.diag(&format!("undefined superclass '{}'", n), def.span);
                }
            }
        }
        self.scopes.push(ScopeKind::Class, Some(def.name.clone()));
        for stmt in &def.body { self.analyze_stmt(stmt); }
        self.scopes.pop();
    }

    fn analyze_module_def(&mut self, def: &ModuleDef) {
        self.scopes.push(ScopeKind::Module, Some(def.name.clone()));
        for stmt in &def.body { self.analyze_stmt(stmt); }
        self.scopes.pop();
    }

    fn analyze_if(&mut self, s: &IfStmt) {
        self.analyze_expr(&s.condition);
        for stmt in &s.then_body { self.analyze_stmt(stmt); }
        for clause in &s.elsif_clauses {
            self.analyze_expr(&clause.condition);
            for stmt in &clause.body { self.analyze_stmt(stmt); }
        }
        if let Some(else_body) = &s.else_body {
            for stmt in else_body { self.analyze_stmt(stmt); }
        }
    }

    fn analyze_unless(&mut self, s: &UnlessStmt) {
        self.analyze_expr(&s.condition);
        for stmt in &s.body { self.analyze_stmt(stmt); }
        if let Some(else_body) = &s.else_body {
            for stmt in else_body { self.analyze_stmt(stmt); }
        }
    }

    fn analyze_while(&mut self, s: &WhileStmt) {
        self.analyze_expr(&s.condition);
        for stmt in &s.body { self.analyze_stmt(stmt); }
    }

    fn analyze_until(&mut self, s: &UntilStmt) {
        self.analyze_expr(&s.condition);
        for stmt in &s.body { self.analyze_stmt(stmt); }
    }

    fn analyze_for(&mut self, s: &ForStmt) {
        self.analyze_expr(&s.iterable);
        self.scopes.define(Symbol {
            name: s.var.clone(), kind: SymbolKind::LocalVar,
            ty: RubyType::Any, initialized: true, ref_count: 0,
            visibility: Visibility::Public,
        });
        for stmt in &s.body { self.analyze_stmt(stmt); }
    }

    fn analyze_case(&mut self, s: &CaseStmt) {
        if let Some(subj) = &s.subject { self.analyze_expr(subj); }
        for clause in &s.when_clauses {
            for pat in &clause.patterns { self.analyze_expr(pat); }
            for stmt in &clause.body { self.analyze_stmt(stmt); }
        }
        if let Some(else_body) = &s.else_body {
            for stmt in else_body { self.analyze_stmt(stmt); }
        }
    }

    fn analyze_begin_rescue(&mut self, s: &BeginRescueStmt) {
        for stmt in &s.body { self.analyze_stmt(stmt); }
        for clause in &s.rescue_clauses {
            self.scopes.push(ScopeKind::Rescue, None);
            for ex in &clause.exceptions { self.analyze_expr(ex); }
            if let Some(var) = &clause.var {
                self.scopes.define(Symbol {
                    name: var.clone(), kind: SymbolKind::LocalVar,
                    ty: RubyType::Instance("Exception".into()),
                    initialized: true, ref_count: 0,
                    visibility: Visibility::Public,
                });
            }
            for stmt in &clause.body { self.analyze_stmt(stmt); }
            self.scopes.pop();
        }
        if let Some(else_body) = &s.else_body {
            for stmt in else_body { self.analyze_stmt(stmt); }
        }
        if let Some(ensure_body) = &s.ensure_body {
            for stmt in ensure_body { self.analyze_stmt(stmt); }
        }
    }

    // ═══════════════════════════════════════════════════════
    //  Expression analysis — returns inferred type
    // ═══════════════════════════════════════════════════════

    fn analyze_expr(&mut self, expr: &Expr) -> RubyType {
        match expr {
            Expr::IntegerLit(_) => RubyType::Integer,
            Expr::FloatLit(_) => RubyType::Float,
            Expr::StringLit(_) | Expr::InterpolatedString(_) => RubyType::String,
            Expr::SymbolLit(_) => RubyType::Symbol,
            Expr::BoolLit(_) => RubyType::Bool,
            Expr::NilLit(_) => RubyType::Nil,
            Expr::SelfExpr(_) => RubyType::SelfType,
            Expr::RegexLit(_) => RubyType::Regexp,

            Expr::ArrayLit(a) => {
                let mut elem_type = RubyType::Unknown;
                for el in &a.elements {
                    let t = self.analyze_expr(el);
                    elem_type = if elem_type == RubyType::Unknown { t }
                    else if elem_type == t { elem_type }
                    else { RubyType::Any };
                }
                RubyType::Array(Box::new(if elem_type == RubyType::Unknown {
                    RubyType::Any
                } else { elem_type }))
            }
            Expr::HashLit(h) => {
                for (k, v) in &h.entries {
                    self.analyze_expr(k);
                    self.analyze_expr(v);
                }
                RubyType::Hash(Box::new(RubyType::Any), Box::new(RubyType::Any))
            }
            Expr::RangeLit(r) => {
                self.analyze_expr(&r.start);
                self.analyze_expr(&r.end);
                RubyType::Range
            }

            Expr::LocalVar(v) => {
                if let Some(sym) = self.scopes.lookup(&v.name) {
                    let ty = sym.ty.clone();
                    // Increment ref count
                    if let Some(s) = self.scopes.lookup_mut(&v.name) {
                        s.ref_count += 1;
                    }
                    ty
                } else {
                    // Could be a method call without parens
                    RubyType::Any
                }
            }
            Expr::InstanceVar(v) => {
                if let Some(sym) = self.scopes.lookup(&v.name) {
                    sym.ty.clone()
                } else {
                    RubyType::Any
                }
            }
            Expr::ClassVar(v) => {
                if let Some(sym) = self.scopes.lookup(&v.name) {
                    sym.ty.clone()
                } else {
                    RubyType::Any
                }
            }
            Expr::GlobalVar(_) => RubyType::Any,
            Expr::ConstRef(c) => {
                let name = c.path.join("::");
                if let Some(sym) = self.scopes.lookup(&name) {
                    sym.ty.clone()
                } else if let Some(sym) = self.scopes.lookup(c.path.last().unwrap_or(&name)) {
                    sym.ty.clone()
                } else {
                    RubyType::Any
                }
            }

            Expr::BinaryOp(op) => {
                let lt = self.analyze_expr(&op.left);
                let rt = self.analyze_expr(&op.right);
                self.infer_binary_type(&lt, &op.op, &rt)
            }
            Expr::UnaryOp(op) => {
                let t = self.analyze_expr(&op.operand);
                match op.op {
                    UnOperator::Neg | UnOperator::Pos => t,
                    UnOperator::Not => RubyType::Bool,
                    UnOperator::BitNot => RubyType::Integer,
                }
            }

            Expr::MethodCall(call) => {
                if let Some(recv) = &call.receiver {
                    self.analyze_expr(recv);
                }
                for arg in &call.args { self.analyze_expr(arg); }
                for (_, v) in &call.kwargs { self.analyze_expr(v); }
                // Look up return type from symbol table
                if let Some(sym) = self.scopes.lookup(&call.method) {
                    if let SymbolKind::Method(sig) = &sym.kind {
                        sig.return_type.clone()
                    } else { RubyType::Any }
                } else {
                    RubyType::Any
                }
            }
            Expr::BlockCall(bc) => {
                self.analyze_expr(&Expr::MethodCall(*bc.call.clone()));
                self.scopes.push(ScopeKind::Block, None);
                for p in &bc.params {
                    self.scopes.define(Symbol {
                        name: p.name.clone(), kind: SymbolKind::BlockParam,
                        ty: RubyType::Any, initialized: true, ref_count: 0,
                        visibility: Visibility::Public,
                    });
                }
                for stmt in &bc.body { self.analyze_stmt(stmt); }
                self.scopes.pop();
                RubyType::Any
            }
            Expr::SuperCall(s) => {
                for a in &s.args { self.analyze_expr(a); }
                RubyType::Any
            }
            Expr::YieldExpr(y) => {
                for a in &y.args { self.analyze_expr(a); }
                RubyType::Any
            }
            Expr::Lambda(l) => {
                self.scopes.push(ScopeKind::Lambda, None);
                for p in &l.params {
                    self.scopes.define(Symbol {
                        name: p.name.clone(), kind: SymbolKind::Param,
                        ty: RubyType::Any, initialized: true, ref_count: 0,
                        visibility: Visibility::Public,
                    });
                }
                for stmt in &l.body { self.analyze_stmt(stmt); }
                self.scopes.pop();
                RubyType::Proc
            }
            Expr::Proc(p) => {
                self.scopes.push(ScopeKind::Lambda, None);
                for param in &p.params {
                    self.scopes.define(Symbol {
                        name: param.name.clone(), kind: SymbolKind::Param,
                        ty: RubyType::Any, initialized: true, ref_count: 0,
                        visibility: Visibility::Public,
                    });
                }
                for stmt in &p.body { self.analyze_stmt(stmt); }
                self.scopes.pop();
                RubyType::Proc
            }
            Expr::Ternary(t) => {
                self.analyze_expr(&t.condition);
                let then_t = self.analyze_expr(&t.then_expr);
                let else_t = self.analyze_expr(&t.else_expr);
                if then_t == else_t { then_t } else { then_t.union(else_t) }
            }
            Expr::Defined(_) => RubyType::Optional(Box::new(RubyType::String)),
            Expr::PatternMatch(pm) => {
                self.analyze_expr(&pm.subject);
                self.analyze_expr(&pm.pattern);
                RubyType::Bool
            }
        }
    }

    /// Infer the result type of a binary operation.
    fn infer_binary_type(&self, left: &RubyType, op: &BinOperator, right: &RubyType) -> RubyType {
        match op {
            // Comparison operators always return Bool
            BinOperator::Eq | BinOperator::NotEq | BinOperator::Lt
            | BinOperator::Gt | BinOperator::LtEq | BinOperator::GtEq
            | BinOperator::CaseEq | BinOperator::Match | BinOperator::NotMatch => RubyType::Bool,

            // Spaceship returns Integer (-1, 0, 1) or nil
            BinOperator::Spaceship => RubyType::Optional(Box::new(RubyType::Integer)),

            // Logical operators
            BinOperator::And | BinOperator::Or => {
                if left == right { left.clone() } else { RubyType::Any }
            }

            // Arithmetic
            BinOperator::Add | BinOperator::Sub | BinOperator::Mul | BinOperator::Pow => {
                match (left, right) {
                    (RubyType::Integer, RubyType::Integer) => RubyType::Integer,
                    (RubyType::Float, _) | (_, RubyType::Float) => RubyType::Float,
                    (RubyType::String, RubyType::String) if *op == BinOperator::Add => RubyType::String,
                    (RubyType::Array(_), RubyType::Array(_)) if *op == BinOperator::Add => left.clone(),
                    _ => RubyType::Any,
                }
            }
            BinOperator::Div => {
                match (left, right) {
                    (RubyType::Integer, RubyType::Integer) => RubyType::Integer,
                    _ => RubyType::Float,
                }
            }
            BinOperator::Mod => RubyType::Integer,

            // Bitwise
            BinOperator::BitAnd | BinOperator::BitOr | BinOperator::BitXor
            | BinOperator::Shl | BinOperator::Shr => RubyType::Integer,

            // Range
            BinOperator::Range | BinOperator::RangeExcl => RubyType::Range,
        }
    }

    /// Try to extract a simple name from an expression (e.g. constant reference).
    fn expr_to_name(&self, expr: &Expr) -> Option<String> {
        match expr {
            Expr::ConstRef(c) => Some(c.path.join("::")),
            Expr::LocalVar(v) => Some(v.name.clone()),
            _ => None,
        }
    }

    fn diag(&mut self, msg: &str, span: SourceSpan) {
        self.diagnostics.push(Diagnostic::error(msg.to_string(), span));
    }

    fn warn(&mut self, msg: &str, span: SourceSpan) {
        self.diagnostics.push(Diagnostic::warning(msg.to_string(), span));
    }
}

impl Default for SemanticAnalyzer {
    fn default() -> Self { Self::new() }
}

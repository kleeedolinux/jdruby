use jdruby_ast::*;
use jdruby_common::{Diagnostic, SourceSpan};
use jdruby_lexer::{Token, TokenKind};

/// Recursive descent parser for Ruby.
pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    diagnostics: Vec<Diagnostic>,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0, diagnostics: Vec::new() }
    }

    pub fn parse(&mut self) -> (Program, Vec<Diagnostic>) {
        let start = self.current_span();
        let mut body = Vec::new();
        self.skip_newlines();
        while !self.is_at_end() {
            match self.parse_stmt() {
                Some(stmt) => body.push(stmt),
                None => { self.advance(); }
            }
            self.skip_newlines();
        }
        let end = self.current_span();
        let program = Program { body, span: start.merge(end) };
        (program, std::mem::take(&mut self.diagnostics))
    }

    // ── Token helpers ──────────────────────────────────────

    fn current(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or(self.tokens.last().unwrap())
    }

    fn current_kind(&self) -> TokenKind {
        self.current().kind
    }

    fn current_span(&self) -> SourceSpan {
        self.current().span
    }

    fn advance(&mut self) -> &Token {
        let tok = &self.tokens[self.pos.min(self.tokens.len() - 1)];
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
        tok
    }

    fn peek_kind(&self) -> TokenKind {
        self.current_kind()
    }

    fn check(&self, kind: TokenKind) -> bool {
        self.current_kind() == kind
    }

    fn eat(&mut self, kind: TokenKind) -> bool {
        if self.check(kind) { self.advance(); true } else { false }
    }

    fn expect(&mut self, kind: TokenKind) -> Option<Token> {
        if self.check(kind) {
            Some(self.advance().clone())
        } else {
            self.error(format!("expected {:?}, found {:?}", kind, self.current_kind()));
            None
        }
    }

    fn is_at_end(&self) -> bool {
        self.pos >= self.tokens.len() || self.current_kind() == TokenKind::Eof
    }

    fn skip_newlines(&mut self) {
        while self.check(TokenKind::Newline) || self.check(TokenKind::Comment) {
            self.advance();
        }
    }

    fn skip_terminators(&mut self) {
        while self.check(TokenKind::Newline) || self.check(TokenKind::Semicolon)
            || self.check(TokenKind::Comment)
        {
            self.advance();
        }
    }

    fn error(&mut self, msg: String) {
        self.diagnostics.push(Diagnostic::error(msg, self.current_span()));
    }

    fn lexeme(&self) -> String {
        self.current().lexeme.clone()
    }

    // ── Statement parsing ──────────────────────────────────

    fn parse_stmt(&mut self) -> Option<Stmt> {
        self.skip_newlines();
        if self.is_at_end() { return None; }

        let stmt = match self.current_kind() {
            TokenKind::KwDef => self.parse_method_def(),
            TokenKind::KwClass => self.parse_class_def(),
            TokenKind::KwModule => self.parse_module_def(),
            TokenKind::KwIf => self.parse_if_stmt(),
            TokenKind::KwUnless => self.parse_unless_stmt(),
            TokenKind::KwWhile => self.parse_while_stmt(),
            TokenKind::KwUntil => self.parse_until_stmt(),
            TokenKind::KwFor => self.parse_for_stmt(),
            TokenKind::KwCase => self.parse_case_stmt(),
            TokenKind::KwBegin => self.parse_begin_rescue(),
            TokenKind::KwReturn => self.parse_return_stmt(),
            TokenKind::KwBreak => self.parse_break_stmt(),
            TokenKind::KwNext => self.parse_next_stmt(),
            TokenKind::KwYield => self.parse_yield_stmt(),
            TokenKind::KwAlias => self.parse_alias_stmt(),
            TokenKind::KwRequire | TokenKind::KwRequireRelative => self.parse_require_stmt(),
            TokenKind::KwAttrReader | TokenKind::KwAttrWriter
            | TokenKind::KwAttrAccessor => self.parse_attr_decl(),
            TokenKind::KwInclude | TokenKind::KwExtend
            | TokenKind::KwPrepend => self.parse_mixin_stmt(),
            _ => self.parse_expr_or_assignment_stmt(),
        };
        self.skip_terminators();
        stmt
    }

    fn parse_expr_or_assignment_stmt(&mut self) -> Option<Stmt> {
        let start = self.current_span();
        let expr = self.parse_expr()?;

        // Check for assignment
        if self.check(TokenKind::Equal) {
            self.advance();
            let value = self.parse_expr()?;
            let target = self.expr_to_assign_target(expr)?;
            let span = start.merge(self.prev_span());
            return Some(Stmt::Assignment(AssignmentStmt { target, value, span }));
        }

        // Check for compound assignment
        if let Some(op) = self.compound_assign_op() {
            self.advance();
            let value = self.parse_expr()?;
            let target = self.expr_to_assign_target(expr)?;
            let span = start.merge(self.prev_span());
            return Some(Stmt::CompoundAssignment(CompoundAssignmentStmt {
                target, op, value, span,
            }));
        }

        let span = start.merge(self.prev_span());
        Some(Stmt::Expr(ExprStmt { expr, span }))
    }

    fn compound_assign_op(&self) -> Option<BinOperator> {
        match self.current_kind() {
            TokenKind::PlusEqual => Some(BinOperator::Add),
            TokenKind::MinusEqual => Some(BinOperator::Sub),
            TokenKind::StarEqual => Some(BinOperator::Mul),
            TokenKind::SlashEqual => Some(BinOperator::Div),
            TokenKind::PercentEqual => Some(BinOperator::Mod),
            TokenKind::DoubleStarEqual => Some(BinOperator::Pow),
            TokenKind::AmpAmpEqual => Some(BinOperator::And),
            TokenKind::PipePipeEqual => Some(BinOperator::Or),
            TokenKind::AmpEqual => Some(BinOperator::BitAnd),
            TokenKind::PipeEqual => Some(BinOperator::BitOr),
            TokenKind::CaretEqual => Some(BinOperator::BitXor),
            TokenKind::LessLessEqual => Some(BinOperator::Shl),
            TokenKind::GreaterGreaterEqual => Some(BinOperator::Shr),
            _ => None,
        }
    }

    fn expr_to_assign_target(&mut self, expr: Expr) -> Option<AssignTarget> {
        match expr {
            Expr::LocalVar(v) => Some(AssignTarget::LocalVar(v.name)),
            Expr::InstanceVar(v) => Some(AssignTarget::InstanceVar(v.name)),
            Expr::ClassVar(v) => Some(AssignTarget::ClassVar(v.name)),
            Expr::GlobalVar(v) => Some(AssignTarget::GlobalVar(v.name)),
            Expr::ConstRef(c) => Some(AssignTarget::Constant(c.path.join("::"))),
            _ => {
                self.error("invalid assignment target".into());
                None
            }
        }
    }

    fn prev_span(&self) -> SourceSpan {
        if self.pos > 0 {
            self.tokens[self.pos - 1].span
        } else {
            SourceSpan::default()
        }
    }

    // ── Expression parsing (precedence climbing) ───────────

    fn parse_expr(&mut self) -> Option<Expr> {
        self.parse_ternary()
    }

    fn parse_ternary(&mut self) -> Option<Expr> {
        let start = self.current_span();
        let cond = self.parse_or()?;
        if self.eat(TokenKind::Question) {
            let then_expr = self.parse_expr()?;
            self.expect(TokenKind::Colon)?;
            let else_expr = self.parse_expr()?;
            let span = start.merge(self.prev_span());
            return Some(Expr::Ternary(TernaryExpr {
                condition: Box::new(cond),
                then_expr: Box::new(then_expr),
                else_expr: Box::new(else_expr),
                span,
            }));
        }
        Some(cond)
    }

    fn parse_or(&mut self) -> Option<Expr> {
        let start = self.current_span();
        let mut left = self.parse_and()?;
        while self.check(TokenKind::PipePipe) || self.check(TokenKind::KwOr) {
            self.advance();
            let right = self.parse_and()?;
            let span = start.merge(self.prev_span());
            left = Expr::BinaryOp(BinaryOp {
                left: Box::new(left), op: BinOperator::Or,
                right: Box::new(right), span,
            });
        }
        Some(left)
    }

    fn parse_and(&mut self) -> Option<Expr> {
        let start = self.current_span();
        let mut left = self.parse_not()?;
        while self.check(TokenKind::AmpAmp) || self.check(TokenKind::KwAnd) {
            self.advance();
            let right = self.parse_not()?;
            let span = start.merge(self.prev_span());
            left = Expr::BinaryOp(BinaryOp {
                left: Box::new(left), op: BinOperator::And,
                right: Box::new(right), span,
            });
        }
        Some(left)
    }

    fn parse_not(&mut self) -> Option<Expr> {
        if self.check(TokenKind::Bang) || self.check(TokenKind::KwNot) {
            let start = self.current_span();
            self.advance();
            let operand = self.parse_not()?;
            let span = start.merge(self.prev_span());
            return Some(Expr::UnaryOp(UnaryOp {
                op: UnOperator::Not, operand: Box::new(operand), span,
            }));
        }
        self.parse_comparison()
    }

    fn parse_comparison(&mut self) -> Option<Expr> {
        let start = self.current_span();
        let mut left = self.parse_range()?;
        loop {
            let op = match self.current_kind() {
                TokenKind::EqualEqual => BinOperator::Eq,
                TokenKind::BangEqual => BinOperator::NotEq,
                TokenKind::Less => BinOperator::Lt,
                TokenKind::Greater => BinOperator::Gt,
                TokenKind::LessEqual => BinOperator::LtEq,
                TokenKind::GreaterEqual => BinOperator::GtEq,
                TokenKind::Spaceship => BinOperator::Spaceship,
                TokenKind::TripleEqual => BinOperator::CaseEq,
                TokenKind::Match => BinOperator::Match,
                TokenKind::NotMatch => BinOperator::NotMatch,
                _ => break,
            };
            self.advance();
            let right = self.parse_range()?;
            let span = start.merge(self.prev_span());
            left = Expr::BinaryOp(BinaryOp {
                left: Box::new(left), op, right: Box::new(right), span,
            });
        }
        Some(left)
    }

    fn parse_range(&mut self) -> Option<Expr> {
        let start = self.current_span();
        let left = self.parse_bitor()?;
        if self.check(TokenKind::DotDot) || self.check(TokenKind::DotDotDot) {
            let exclusive = self.current_kind() == TokenKind::DotDotDot;
            self.advance();
            let right = self.parse_bitor()?;
            let span = start.merge(self.prev_span());
            return Some(Expr::RangeLit(RangeLit {
                start: Box::new(left), end: Box::new(right), exclusive, span,
            }));
        }
        Some(left)
    }

    fn parse_bitor(&mut self) -> Option<Expr> {
        let start = self.current_span();
        let mut left = self.parse_bitxor()?;
        while self.check(TokenKind::Pipe) {
            self.advance();
            let right = self.parse_bitxor()?;
            let span = start.merge(self.prev_span());
            left = Expr::BinaryOp(BinaryOp {
                left: Box::new(left), op: BinOperator::BitOr,
                right: Box::new(right), span,
            });
        }
        Some(left)
    }

    fn parse_bitxor(&mut self) -> Option<Expr> {
        let start = self.current_span();
        let mut left = self.parse_bitand()?;
        while self.check(TokenKind::Caret) {
            self.advance();
            let right = self.parse_bitand()?;
            let span = start.merge(self.prev_span());
            left = Expr::BinaryOp(BinaryOp {
                left: Box::new(left), op: BinOperator::BitXor,
                right: Box::new(right), span,
            });
        }
        Some(left)
    }

    fn parse_bitand(&mut self) -> Option<Expr> {
        let start = self.current_span();
        let mut left = self.parse_shift()?;
        while self.check(TokenKind::Amp) {
            self.advance();
            let right = self.parse_shift()?;
            let span = start.merge(self.prev_span());
            left = Expr::BinaryOp(BinaryOp {
                left: Box::new(left), op: BinOperator::BitAnd,
                right: Box::new(right), span,
            });
        }
        Some(left)
    }

    fn parse_shift(&mut self) -> Option<Expr> {
        let start = self.current_span();
        let mut left = self.parse_additive()?;
        while self.check(TokenKind::LessLess) || self.check(TokenKind::GreaterGreater) {
            let op = if self.current_kind() == TokenKind::LessLess {
                BinOperator::Shl
            } else {
                BinOperator::Shr
            };
            self.advance();
            let right = self.parse_additive()?;
            let span = start.merge(self.prev_span());
            left = Expr::BinaryOp(BinaryOp {
                left: Box::new(left), op, right: Box::new(right), span,
            });
        }
        Some(left)
    }

    fn parse_additive(&mut self) -> Option<Expr> {
        let start = self.current_span();
        let mut left = self.parse_multiplicative()?;
        while self.check(TokenKind::Plus) || self.check(TokenKind::Minus) {
            let op = if self.current_kind() == TokenKind::Plus {
                BinOperator::Add
            } else {
                BinOperator::Sub
            };
            self.advance();
            let right = self.parse_multiplicative()?;
            let span = start.merge(self.prev_span());
            left = Expr::BinaryOp(BinaryOp {
                left: Box::new(left), op, right: Box::new(right), span,
            });
        }
        Some(left)
    }

    fn parse_multiplicative(&mut self) -> Option<Expr> {
        let start = self.current_span();
        let mut left = self.parse_power()?;
        while self.check(TokenKind::Star) || self.check(TokenKind::Slash)
            || self.check(TokenKind::Percent)
        {
            let op = match self.current_kind() {
                TokenKind::Star => BinOperator::Mul,
                TokenKind::Slash => BinOperator::Div,
                _ => BinOperator::Mod,
            };
            self.advance();
            let right = self.parse_power()?;
            let span = start.merge(self.prev_span());
            left = Expr::BinaryOp(BinaryOp {
                left: Box::new(left), op, right: Box::new(right), span,
            });
        }
        Some(left)
    }

    fn parse_power(&mut self) -> Option<Expr> {
        let start = self.current_span();
        let base = self.parse_unary()?;
        if self.check(TokenKind::DoubleStar) {
            self.advance();
            let exp = self.parse_power()?; // right-associative
            let span = start.merge(self.prev_span());
            return Some(Expr::BinaryOp(BinaryOp {
                left: Box::new(base), op: BinOperator::Pow,
                right: Box::new(exp), span,
            }));
        }
        Some(base)
    }

    fn parse_unary(&mut self) -> Option<Expr> {
        let start = self.current_span();
        match self.current_kind() {
            TokenKind::Minus => {
                self.advance();
                let operand = self.parse_unary()?;
                let span = start.merge(self.prev_span());
                Some(Expr::UnaryOp(UnaryOp {
                    op: UnOperator::Neg, operand: Box::new(operand), span,
                }))
            }
            TokenKind::Plus => {
                self.advance();
                let operand = self.parse_unary()?;
                let span = start.merge(self.prev_span());
                Some(Expr::UnaryOp(UnaryOp {
                    op: UnOperator::Pos, operand: Box::new(operand), span,
                }))
            }
            TokenKind::Tilde => {
                self.advance();
                let operand = self.parse_unary()?;
                let span = start.merge(self.prev_span());
                Some(Expr::UnaryOp(UnaryOp {
                    op: UnOperator::BitNot, operand: Box::new(operand), span,
                }))
            }
            _ => self.parse_postfix(),
        }
    }

    fn parse_postfix(&mut self) -> Option<Expr> {
        let mut expr = self.parse_primary()?;
        loop {
            if self.check(TokenKind::Dot) {
                self.advance();
                expr = self.parse_method_call_on(expr)?;
            } else if self.check(TokenKind::ColonColon) {
                self.advance();
                let start = self.prev_span();
                let name = self.expect_ident_or_const()?;
                let span = start.merge(self.prev_span());
                if let Expr::ConstRef(mut cref) = expr {
                    cref.path.push(name);
                    cref.span = span;
                    expr = Expr::ConstRef(cref);
                } else {
                    expr = Expr::MethodCall(MethodCall {
                        receiver: Some(Box::new(expr)),
                        method: name, args: vec![], kwargs: vec![],
                        block_arg: None, span,
                    });
                }
            } else if self.check(TokenKind::LBracket) {
                let start = self.current_span();
                self.advance();
                let index = self.parse_expr()?;
                self.expect(TokenKind::RBracket)?;
                let span = start.merge(self.prev_span());
                expr = Expr::MethodCall(MethodCall {
                    receiver: Some(Box::new(expr)),
                    method: "[]".into(), args: vec![index], kwargs: vec![],
                    block_arg: None, span,
                });
            } else {
                break;
            }
        }
        Some(expr)
    }

    fn parse_method_call_on(&mut self, receiver: Expr) -> Option<Expr> {
        let start = receiver.span();
        let method = self.expect_ident_or_const()?;
        let (args, kwargs) = if self.check(TokenKind::LParen) {
            self.advance();
            let r = self.parse_arg_list(TokenKind::RParen)?;
            self.expect(TokenKind::RParen)?;
            r
        } else if self.is_arg_start() {
            self.parse_bare_arg_list()?
        } else {
            (vec![], vec![])
        };
        let span = start.merge(self.prev_span());
        Some(Expr::MethodCall(MethodCall {
            receiver: Some(Box::new(receiver)),
            method, args, kwargs, block_arg: None, span,
        }))
    }

    fn is_arg_start(&self) -> bool {
        matches!(
            self.current_kind(),
            TokenKind::Integer | TokenKind::Float | TokenKind::StringDouble
            | TokenKind::StringSingle | TokenKind::Symbol | TokenKind::KwTrue
            | TokenKind::KwFalse | TokenKind::KwNil | TokenKind::Identifier
            | TokenKind::Constant | TokenKind::InstanceVar | TokenKind::ClassVar
            | TokenKind::GlobalVar | TokenKind::LBracket | TokenKind::LBrace
            | TokenKind::Colon
        )
    }

    // ── Primary expressions ────────────────────────────────

    fn parse_primary(&mut self) -> Option<Expr> {
        let span = self.current_span();
        match self.current_kind() {
            TokenKind::Integer => {
                let lex = self.lexeme();
                self.advance();
                let val = parse_int(&lex);
                Some(Expr::IntegerLit(IntegerLit { value: val, span }))
            }
            TokenKind::Float => {
                let lex = self.lexeme();
                self.advance();
                let val: f64 = lex.replace('_', "").parse().unwrap_or(0.0);
                Some(Expr::FloatLit(FloatLit { value: val, span }))
            }
            TokenKind::StringDouble | TokenKind::StringSingle => {
                let lex = self.lexeme();
                self.advance();
                let val = unescape_string(&lex);
                Some(Expr::StringLit(StringLit { value: val, span }))
            }
            TokenKind::Symbol => {
                let lex = self.lexeme();
                self.advance();
                let name = lex.trim_start_matches(':').trim_matches('"').to_string();
                Some(Expr::SymbolLit(SymbolLit { name, span }))
            }
            TokenKind::KwTrue => { self.advance(); Some(Expr::BoolLit(BoolLit { value: true, span })) }
            TokenKind::KwFalse => { self.advance(); Some(Expr::BoolLit(BoolLit { value: false, span })) }
            TokenKind::KwNil => { self.advance(); Some(Expr::NilLit(NilLit { span })) }
            TokenKind::KwSelf_ => { self.advance(); Some(Expr::SelfExpr(SelfExpr { span })) }
            TokenKind::Identifier => self.parse_identifier_expr(),
            TokenKind::Constant => self.parse_constant_expr(),
            TokenKind::InstanceVar => {
                let name = self.lexeme();
                self.advance();
                Some(Expr::InstanceVar(InstanceVarExpr { name, span }))
            }
            TokenKind::ClassVar => {
                let name = self.lexeme();
                self.advance();
                Some(Expr::ClassVar(ClassVarExpr { name, span }))
            }
            TokenKind::GlobalVar => {
                let name = self.lexeme();
                self.advance();
                Some(Expr::GlobalVar(GlobalVarExpr { name, span }))
            }
            TokenKind::LParen => {
                self.advance();
                let expr = self.parse_expr()?;
                self.expect(TokenKind::RParen)?;
                Some(expr)
            }
            TokenKind::LBracket => self.parse_array_lit(),
            TokenKind::LBrace => self.parse_hash_lit(),
            TokenKind::Arrow => self.parse_lambda(),
            TokenKind::KwLambda => { self.advance(); self.parse_lambda_body() }
            TokenKind::KwProc => self.parse_proc(),
            TokenKind::KwDefined => self.parse_defined(),
            TokenKind::KwSuper => self.parse_super(),
            TokenKind::KwPuts | TokenKind::KwPrint | TokenKind::KwP
            | TokenKind::KwRaise => self.parse_builtin_call(),
            _ => {
                self.error(format!("unexpected token: {:?}", self.current_kind()));
                None
            }
        }
    }

    fn parse_identifier_expr(&mut self) -> Option<Expr> {
        let start = self.current_span();
        let name = self.lexeme();
        self.advance();
        // Method call with parens
        if self.check(TokenKind::LParen) {
            self.advance();
            let (args, kwargs) = self.parse_arg_list(TokenKind::RParen)?;
            self.expect(TokenKind::RParen)?;
            let span = start.merge(self.prev_span());
            return Some(Expr::MethodCall(MethodCall {
                receiver: None, method: name, args, kwargs,
                block_arg: None, span,
            }));
        }
        // Block call
        if self.check(TokenKind::LBrace) || self.check(TokenKind::KwDo) {
            let (args, kwargs) = if self.is_arg_start() {
                self.parse_bare_arg_list()?
            } else {
                (vec![], vec![])
            };
            if self.check(TokenKind::LBrace) || self.check(TokenKind::KwDo) {
                let call = MethodCall {
                    receiver: None, method: name, args, kwargs,
                    block_arg: None, span: start,
                };
                return self.parse_block_call(call);
            }
        }
        Some(Expr::LocalVar(LocalVar { name, span: start }))
    }

    fn parse_constant_expr(&mut self) -> Option<Expr> {
        let start = self.current_span();
        let name = self.lexeme();
        self.advance();
        Some(Expr::ConstRef(ConstRef { path: vec![name], span: start }))
    }

    fn parse_builtin_call(&mut self) -> Option<Expr> {
        let start = self.current_span();
        let method = self.lexeme();
        self.advance();
        let (args, kwargs) = if self.check(TokenKind::LParen) {
            self.advance();
            let r = self.parse_arg_list(TokenKind::RParen)?;
            self.expect(TokenKind::RParen)?;
            r
        } else if !self.check(TokenKind::Newline) && !self.check(TokenKind::Eof)
            && !self.check(TokenKind::Semicolon) && self.is_arg_start()
        {
            self.parse_bare_arg_list()?
        } else {
            (vec![], vec![])
        };
        let span = start.merge(self.prev_span());
        Some(Expr::MethodCall(MethodCall {
            receiver: None, method, args, kwargs, block_arg: None, span,
        }))
    }

    fn parse_super(&mut self) -> Option<Expr> {
        let start = self.current_span();
        self.advance();
        let args = if self.check(TokenKind::LParen) {
            self.advance();
            let (a, _) = self.parse_arg_list(TokenKind::RParen)?;
            self.expect(TokenKind::RParen)?;
            a
        } else {
            vec![]
        };
        let span = start.merge(self.prev_span());
        Some(Expr::SuperCall(SuperCallExpr { args, span }))
    }

    fn parse_defined(&mut self) -> Option<Expr> {
        let start = self.current_span();
        self.advance();
        let has_paren = self.eat(TokenKind::LParen);
        let expr = self.parse_expr()?;
        if has_paren { self.expect(TokenKind::RParen)?; }
        let span = start.merge(self.prev_span());
        Some(Expr::Defined(DefinedExpr { expr: Box::new(expr), span }))
    }

    fn parse_array_lit(&mut self) -> Option<Expr> {
        let start = self.current_span();
        self.advance(); // [
        let mut elements = Vec::new();
        self.skip_newlines();
        while !self.check(TokenKind::RBracket) && !self.is_at_end() {
            elements.push(self.parse_expr()?);
            self.skip_newlines();
            if !self.eat(TokenKind::Comma) { break; }
            self.skip_newlines();
        }
        self.expect(TokenKind::RBracket)?;
        let span = start.merge(self.prev_span());
        Some(Expr::ArrayLit(ArrayLit { elements, span }))
    }

    fn parse_hash_lit(&mut self) -> Option<Expr> {
        let start = self.current_span();
        self.advance(); // {
        let mut entries = Vec::new();
        self.skip_newlines();
        while !self.check(TokenKind::RBrace) && !self.is_at_end() {
            let key = self.parse_expr()?;
            if self.eat(TokenKind::FatArrow) || self.eat(TokenKind::Colon) {
                let value = self.parse_expr()?;
                entries.push((key, value));
            }
            self.skip_newlines();
            if !self.eat(TokenKind::Comma) { break; }
            self.skip_newlines();
        }
        self.expect(TokenKind::RBrace)?;
        let span = start.merge(self.prev_span());
        Some(Expr::HashLit(HashLit { entries, span }))
    }

    fn parse_lambda(&mut self) -> Option<Expr> {
        let start = self.current_span();
        self.advance(); // ->
        self.parse_lambda_body_from(start)
    }

    fn parse_lambda_body(&mut self) -> Option<Expr> {
        self.parse_lambda_body_from(self.prev_span())
    }

    fn parse_lambda_body_from(&mut self, start: SourceSpan) -> Option<Expr> {
        let params = if self.check(TokenKind::LParen) {
            self.advance();
            let p = self.parse_param_list()?;
            self.expect(TokenKind::RParen)?;
            p
        } else {
            vec![]
        };
        let body = if self.check(TokenKind::LBrace) {
            self.advance();
            let b = self.parse_body_until_rbrace()?;
            self.expect(TokenKind::RBrace)?;
            b
        } else {
            self.expect(TokenKind::KwDo)?;
            let b = self.parse_body_until_end()?;
            self.expect(TokenKind::KwEnd)?;
            b
        };
        let span = start.merge(self.prev_span());
        Some(Expr::Lambda(LambdaExpr { params, body, span }))
    }

    fn parse_proc(&mut self) -> Option<Expr> {
        let start = self.current_span();
        self.advance(); // proc
        let body = if self.check(TokenKind::LBrace) {
            self.advance();
            let params = if self.eat(TokenKind::Pipe) {
                let p = self.parse_block_params()?;
                self.expect(TokenKind::Pipe)?;
                p
            } else { vec![] };
            let b = self.parse_body_until_rbrace()?;
            self.expect(TokenKind::RBrace)?;
            return Some(Expr::Proc(ProcExpr {
                params, body: b, span: start.merge(self.prev_span()),
            }));
        } else {
            self.expect(TokenKind::KwDo)?;
            let params = if self.eat(TokenKind::Pipe) {
                let p = self.parse_block_params()?;
                self.expect(TokenKind::Pipe)?;
                p
            } else { vec![] };
            let b = self.parse_body_until_end()?;
            self.expect(TokenKind::KwEnd)?;
            (params, b)
        };
        let span = start.merge(self.prev_span());
        Some(Expr::Proc(ProcExpr { params: body.0, body: body.1, span }))
    }

    fn parse_block_call(&mut self, call: MethodCall) -> Option<Expr> {
        let start = call.span;
        let is_brace = self.check(TokenKind::LBrace);
        self.advance(); // { or do
        let params = if self.eat(TokenKind::Pipe) {
            let p = self.parse_block_params()?;
            self.expect(TokenKind::Pipe)?;
            p
        } else { vec![] };
        let body = if is_brace {
            let b = self.parse_body_until_rbrace()?;
            self.expect(TokenKind::RBrace)?;
            b
        } else {
            let b = self.parse_body_until_end()?;
            self.expect(TokenKind::KwEnd)?;
            b
        };
        let span = start.merge(self.prev_span());
        Some(Expr::BlockCall(BlockCall {
            call: Box::new(call), params, body, span,
        }))
    }

    // ── Argument & parameter lists ─────────────────────────

    fn parse_arg_list(&mut self, end: TokenKind) -> Option<(Vec<Expr>, Vec<(String, Expr)>)> {
        let mut args = Vec::new();
        let mut kwargs = Vec::new();
        self.skip_newlines();
        while !self.check(end) && !self.is_at_end() {
            // Check for keyword arg: `key: value`
            if self.current_kind() == TokenKind::Identifier && self.peek_at(1) == TokenKind::Colon {
                let key = self.lexeme();
                self.advance(); // ident
                self.advance(); // :
                let val = self.parse_expr()?;
                kwargs.push((key, val));
            } else {
                args.push(self.parse_expr()?);
            }
            self.skip_newlines();
            if !self.eat(TokenKind::Comma) { break; }
            self.skip_newlines();
        }
        Some((args, kwargs))
    }

    fn parse_bare_arg_list(&mut self) -> Option<(Vec<Expr>, Vec<(String, Expr)>)> {
        let mut args = Vec::new();
        let mut kwargs = Vec::new();
        loop {
            if self.current_kind() == TokenKind::Identifier && self.peek_at(1) == TokenKind::Colon {
                let key = self.lexeme();
                self.advance();
                self.advance();
                let val = self.parse_expr()?;
                kwargs.push((key, val));
            } else {
                args.push(self.parse_expr()?);
            }
            if !self.eat(TokenKind::Comma) { break; }
        }
        Some((args, kwargs))
    }

    fn parse_param_list(&mut self) -> Option<Vec<Param>> {
        let mut params = Vec::new();
        while !self.check(TokenKind::RParen) && !self.is_at_end() {
            params.push(self.parse_param()?);
            if !self.eat(TokenKind::Comma) { break; }
        }
        Some(params)
    }

    fn parse_param(&mut self) -> Option<Param> {
        let start = self.current_span();
        // Check for special param kinds
        if self.eat(TokenKind::Star) {
            let name = self.expect_ident()?;
            return Some(Param {
                name, default: None, kind: ParamKind::Rest,
                span: start.merge(self.prev_span()),
            });
        }
        if self.eat(TokenKind::DoubleStar) {
            let name = self.expect_ident()?;
            return Some(Param {
                name, default: None, kind: ParamKind::KeywordRest,
                span: start.merge(self.prev_span()),
            });
        }
        if self.eat(TokenKind::Amp) {
            let name = self.expect_ident()?;
            return Some(Param {
                name, default: None, kind: ParamKind::Block,
                span: start.merge(self.prev_span()),
            });
        }
        let name = self.expect_ident()?;
        // Keyword param: `name:`
        if self.eat(TokenKind::Colon) {
            let default = if !self.check(TokenKind::Comma) && !self.check(TokenKind::RParen) {
                Some(self.parse_expr()?)
            } else { None };
            return Some(Param {
                name, default, kind: ParamKind::Keyword,
                span: start.merge(self.prev_span()),
            });
        }
        // Optional param with default
        if self.eat(TokenKind::Equal) {
            let default = self.parse_expr()?;
            return Some(Param {
                name, default: Some(default), kind: ParamKind::Optional,
                span: start.merge(self.prev_span()),
            });
        }
        Some(Param {
            name, default: None, kind: ParamKind::Required,
            span: start.merge(self.prev_span()),
        })
    }

    fn parse_block_params(&mut self) -> Option<Vec<Param>> {
        let mut params = Vec::new();
        while !self.check(TokenKind::Pipe) && !self.is_at_end() {
            let span = self.current_span();
            let name = self.expect_ident()?;
            params.push(Param {
                name, default: None, kind: ParamKind::Required, span,
            });
            if !self.eat(TokenKind::Comma) { break; }
        }
        Some(params)
    }

    fn peek_at(&self, offset: usize) -> TokenKind {
        self.tokens.get(self.pos + offset).map(|t| t.kind).unwrap_or(TokenKind::Eof)
    }

    fn expect_ident(&mut self) -> Option<String> {
        if self.check(TokenKind::Identifier) {
            let s = self.lexeme();
            self.advance();
            Some(s)
        } else {
            self.error(format!("expected identifier, found {:?}", self.current_kind()));
            None
        }
    }

    fn expect_ident_or_const(&mut self) -> Option<String> {
        if self.check(TokenKind::Identifier) || self.check(TokenKind::Constant) {
            let s = self.lexeme();
            self.advance();
            Some(s)
        } else {
            self.error(format!("expected identifier or constant, found {:?}", self.current_kind()));
            None
        }
    }

    // ── Body parsing helpers ───────────────────────────────

    fn parse_body_until_end(&mut self) -> Option<Vec<Stmt>> {
        let mut body = Vec::new();
        self.skip_newlines();
        while !self.check(TokenKind::KwEnd) && !self.check(TokenKind::KwRescue)
            && !self.check(TokenKind::KwEnsure) && !self.check(TokenKind::KwElse)
            && !self.check(TokenKind::KwElsif) && !self.is_at_end()
        {
            if let Some(s) = self.parse_stmt() { body.push(s); }
            self.skip_newlines();
        }
        Some(body)
    }

    fn parse_body_until_rbrace(&mut self) -> Option<Vec<Stmt>> {
        let mut body = Vec::new();
        self.skip_newlines();
        while !self.check(TokenKind::RBrace) && !self.is_at_end() {
            if let Some(s) = self.parse_stmt() { body.push(s); }
            self.skip_newlines();
        }
        Some(body)
    }

    fn parse_body_until_when(&mut self) -> Option<Vec<Stmt>> {
        let mut body = Vec::new();
        self.skip_newlines();
        while !self.check(TokenKind::KwWhen) && !self.check(TokenKind::KwElse)
            && !self.check(TokenKind::KwEnd) && !self.is_at_end()
        {
            if let Some(s) = self.parse_stmt() { body.push(s); }
            self.skip_newlines();
        }
        Some(body)
    }

    // ── Definitions ────────────────────────────────────────

    fn parse_method_def(&mut self) -> Option<Stmt> {
        let start = self.current_span();
        self.advance(); // def
        let mut is_class_method = false;
        let name;
        // Check for `def self.method`
        if self.check(TokenKind::KwSelf_) {
            let next = self.peek_at(1);
            if next == TokenKind::Dot {
                self.advance(); // self
                self.advance(); // .
                is_class_method = true;
            }
        }
        name = self.expect_ident_or_const()?;
        let params = if self.check(TokenKind::LParen) {
            self.advance();
            let p = self.parse_param_list()?;
            self.expect(TokenKind::RParen)?;
            p
        } else {
            vec![]
        };
        self.skip_terminators();
        // Check for endless method: `def foo = expr`
        if self.eat(TokenKind::Equal) {
            let expr = self.parse_expr()?;
            let span = start.merge(self.prev_span());
            return Some(Stmt::MethodDef(MethodDef {
                name, params,
                body: vec![Stmt::Expr(ExprStmt { expr: expr.clone(), span: span })],
                is_class_method, span,
            }));
        }
        let body = self.parse_body_until_end()?;
        self.expect(TokenKind::KwEnd)?;
        let span = start.merge(self.prev_span());
        Some(Stmt::MethodDef(MethodDef { name, params, body, is_class_method, span }))
    }

    fn parse_class_def(&mut self) -> Option<Stmt> {
        let start = self.current_span();
        self.advance(); // class
        let name = self.expect_ident_or_const()?;
        let superclass = if self.eat(TokenKind::Less) {
            Some(Box::new(self.parse_expr()?))
        } else {
            None
        };
        self.skip_terminators();
        let body = self.parse_body_until_end()?;
        self.expect(TokenKind::KwEnd)?;
        let span = start.merge(self.prev_span());
        Some(Stmt::ClassDef(ClassDef { name, superclass, body, span }))
    }

    fn parse_module_def(&mut self) -> Option<Stmt> {
        let start = self.current_span();
        self.advance(); // module
        let name = self.expect_ident_or_const()?;
        self.skip_terminators();
        let body = self.parse_body_until_end()?;
        self.expect(TokenKind::KwEnd)?;
        let span = start.merge(self.prev_span());
        Some(Stmt::ModuleDef(ModuleDef { name, body, span }))
    }

    // ── Control flow ───────────────────────────────────────

    fn parse_if_stmt(&mut self) -> Option<Stmt> {
        let start = self.current_span();
        self.advance(); // if
        let condition = self.parse_expr()?;
        self.eat(TokenKind::KwThen);
        self.skip_terminators();
        let then_body = self.parse_body_until_end()?;
        let mut elsif_clauses = Vec::new();
        while self.check(TokenKind::KwElsif) {
            let es = self.current_span();
            self.advance();
            let cond = self.parse_expr()?;
            self.eat(TokenKind::KwThen);
            self.skip_terminators();
            let body = self.parse_body_until_end()?;
            elsif_clauses.push(ElsifClause {
                condition: cond, body, span: es.merge(self.prev_span()),
            });
        }
        let else_body = if self.eat(TokenKind::KwElse) {
            self.skip_terminators();
            Some(self.parse_body_until_end()?)
        } else {
            None
        };
        self.expect(TokenKind::KwEnd)?;
        let span = start.merge(self.prev_span());
        Some(Stmt::If(IfStmt { condition, then_body, elsif_clauses, else_body, span }))
    }

    fn parse_unless_stmt(&mut self) -> Option<Stmt> {
        let start = self.current_span();
        self.advance(); // unless
        let condition = self.parse_expr()?;
        self.eat(TokenKind::KwThen);
        self.skip_terminators();
        let body = self.parse_body_until_end()?;
        let else_body = if self.eat(TokenKind::KwElse) {
            self.skip_terminators();
            Some(self.parse_body_until_end()?)
        } else {
            None
        };
        self.expect(TokenKind::KwEnd)?;
        let span = start.merge(self.prev_span());
        Some(Stmt::Unless(UnlessStmt { condition, body, else_body, span }))
    }

    fn parse_while_stmt(&mut self) -> Option<Stmt> {
        let start = self.current_span();
        self.advance(); // while
        let condition = self.parse_expr()?;
        self.eat(TokenKind::KwDo);
        self.skip_terminators();
        let body = self.parse_body_until_end()?;
        self.expect(TokenKind::KwEnd)?;
        let span = start.merge(self.prev_span());
        Some(Stmt::While(WhileStmt { condition, body, span }))
    }

    fn parse_until_stmt(&mut self) -> Option<Stmt> {
        let start = self.current_span();
        self.advance(); // until
        let condition = self.parse_expr()?;
        self.eat(TokenKind::KwDo);
        self.skip_terminators();
        let body = self.parse_body_until_end()?;
        self.expect(TokenKind::KwEnd)?;
        let span = start.merge(self.prev_span());
        Some(Stmt::Until(UntilStmt { condition, body, span }))
    }

    fn parse_for_stmt(&mut self) -> Option<Stmt> {
        let start = self.current_span();
        self.advance(); // for
        let var = self.expect_ident()?;
        self.expect(TokenKind::KwIn)?;
        let iterable = self.parse_expr()?;
        self.eat(TokenKind::KwDo);
        self.skip_terminators();
        let body = self.parse_body_until_end()?;
        self.expect(TokenKind::KwEnd)?;
        let span = start.merge(self.prev_span());
        Some(Stmt::For(ForStmt { var, iterable, body, span }))
    }

    fn parse_case_stmt(&mut self) -> Option<Stmt> {
        let start = self.current_span();
        self.advance(); // case
        let subject = if !self.check(TokenKind::Newline) && !self.check(TokenKind::Semicolon) {
            Some(self.parse_expr()?)
        } else {
            None
        };
        self.skip_terminators();
        let mut when_clauses = Vec::new();
        while self.check(TokenKind::KwWhen) {
            let ws = self.current_span();
            self.advance();
            let mut patterns = vec![self.parse_expr()?];
            while self.eat(TokenKind::Comma) {
                patterns.push(self.parse_expr()?);
            }
            self.eat(TokenKind::KwThen);
            self.skip_terminators();
            let body = self.parse_body_until_when()?;
            when_clauses.push(WhenClause {
                patterns, body, span: ws.merge(self.prev_span()),
            });
        }
        let else_body = if self.eat(TokenKind::KwElse) {
            self.skip_terminators();
            Some(self.parse_body_until_end()?)
        } else {
            None
        };
        self.expect(TokenKind::KwEnd)?;
        let span = start.merge(self.prev_span());
        Some(Stmt::Case(CaseStmt { subject, when_clauses, else_body, span }))
    }

    fn parse_begin_rescue(&mut self) -> Option<Stmt> {
        let start = self.current_span();
        self.advance(); // begin
        self.skip_terminators();
        let body = self.parse_body_until_end()?;
        let mut rescue_clauses = Vec::new();
        while self.check(TokenKind::KwRescue) {
            let rs = self.current_span();
            self.advance();
            let mut exceptions = Vec::new();
            let mut var = None;
            if !self.check(TokenKind::Newline) && !self.check(TokenKind::Semicolon)
                && !self.check(TokenKind::FatArrow)
            {
                exceptions.push(self.parse_expr()?);
                while self.eat(TokenKind::Comma) {
                    exceptions.push(self.parse_expr()?);
                }
            }
            if self.eat(TokenKind::FatArrow) {
                var = Some(self.expect_ident()?);
            }
            self.skip_terminators();
            let rbody = self.parse_body_until_end()?;
            rescue_clauses.push(RescueClause {
                exceptions, var, body: rbody,
                span: rs.merge(self.prev_span()),
            });
        }
        let else_body = if self.eat(TokenKind::KwElse) {
            self.skip_terminators();
            Some(self.parse_body_until_end()?)
        } else {
            None
        };
        let ensure_body = if self.eat(TokenKind::KwEnsure) {
            self.skip_terminators();
            Some(self.parse_body_until_end()?)
        } else {
            None
        };
        self.expect(TokenKind::KwEnd)?;
        let span = start.merge(self.prev_span());
        Some(Stmt::BeginRescue(BeginRescueStmt {
            body, rescue_clauses, else_body, ensure_body, span,
        }))
    }

    // ── Jump statements ────────────────────────────────────

    fn parse_return_stmt(&mut self) -> Option<Stmt> {
        let start = self.current_span();
        self.advance(); // return
        let value = if !self.check(TokenKind::Newline) && !self.check(TokenKind::Semicolon)
            && !self.check(TokenKind::Eof) && !self.check(TokenKind::KwEnd)
        {
            Some(self.parse_expr()?)
        } else {
            None
        };
        let span = start.merge(self.prev_span());
        Some(Stmt::Return(ReturnStmt { value, span }))
    }

    fn parse_break_stmt(&mut self) -> Option<Stmt> {
        let start = self.current_span();
        self.advance();
        let value = if self.is_arg_start() { Some(self.parse_expr()?) } else { None };
        let span = start.merge(self.prev_span());
        Some(Stmt::Break(BreakStmt { value, span }))
    }

    fn parse_next_stmt(&mut self) -> Option<Stmt> {
        let start = self.current_span();
        self.advance();
        let value = if self.is_arg_start() { Some(self.parse_expr()?) } else { None };
        let span = start.merge(self.prev_span());
        Some(Stmt::Next(NextStmt { value, span }))
    }

    fn parse_yield_stmt(&mut self) -> Option<Stmt> {
        let start = self.current_span();
        self.advance();
        let args = if self.check(TokenKind::LParen) {
            self.advance();
            let (a, _) = self.parse_arg_list(TokenKind::RParen)?;
            self.expect(TokenKind::RParen)?;
            a
        } else if self.is_arg_start() {
            let (a, _) = self.parse_bare_arg_list()?;
            a
        } else {
            vec![]
        };
        let span = start.merge(self.prev_span());
        Some(Stmt::Yield(YieldStmt { args, span }))
    }

    // ── Misc statements ────────────────────────────────────

    fn parse_alias_stmt(&mut self) -> Option<Stmt> {
        let start = self.current_span();
        self.advance(); // alias
        let new_name = self.expect_ident_or_const()?;
        let old_name = self.expect_ident_or_const()?;
        let span = start.merge(self.prev_span());
        Some(Stmt::Alias(AliasStmt { new_name, old_name, span }))
    }

    fn parse_require_stmt(&mut self) -> Option<Stmt> {
        let start = self.current_span();
        let is_relative = self.current_kind() == TokenKind::KwRequireRelative;
        self.advance();
        let path = if self.check(TokenKind::StringDouble) || self.check(TokenKind::StringSingle) {
            let lex = self.lexeme();
            self.advance();
            unescape_string(&lex)
        } else {
            self.error("expected string after require".into());
            return None;
        };
        let span = start.merge(self.prev_span());
        Some(Stmt::Require(RequireStmt { path, is_relative, span }))
    }

    fn parse_attr_decl(&mut self) -> Option<Stmt> {
        let start = self.current_span();
        let kind = match self.current_kind() {
            TokenKind::KwAttrReader => AttrKind::Reader,
            TokenKind::KwAttrWriter => AttrKind::Writer,
            _ => AttrKind::Accessor,
        };
        self.advance();
        let mut names = Vec::new();
        loop {
            if self.check(TokenKind::Symbol) {
                let lex = self.lexeme();
                names.push(lex.trim_start_matches(':').trim_matches('"').to_string());
                self.advance();
            } else if self.check(TokenKind::Identifier) {
                names.push(self.lexeme());
                self.advance();
            } else {
                break;
            }
            if !self.eat(TokenKind::Comma) { break; }
        }
        let span = start.merge(self.prev_span());
        Some(Stmt::AttrDecl(AttrDeclStmt { kind, names, span }))
    }

    fn parse_mixin_stmt(&mut self) -> Option<Stmt> {
        let start = self.current_span();
        let kind = match self.current_kind() {
            TokenKind::KwInclude => MixinKind::Include,
            TokenKind::KwExtend => MixinKind::Extend,
            _ => MixinKind::Prepend,
        };
        self.advance();
        let module = self.parse_expr()?;
        let span = start.merge(self.prev_span());
        Some(Stmt::MixinStmt(MixinStmt { kind, module, span }))
    }
}

// ── Span trait for Expr ────────────────────────────────────

trait ExprSpan {
    fn span(&self) -> SourceSpan;
}

impl ExprSpan for Expr {
    fn span(&self) -> SourceSpan {
        match self {
            Expr::IntegerLit(n) => n.span,
            Expr::FloatLit(n) => n.span,
            Expr::StringLit(n) => n.span,
            Expr::InterpolatedString(n) => n.span,
            Expr::SymbolLit(n) => n.span,
            Expr::BoolLit(n) => n.span,
            Expr::NilLit(n) => n.span,
            Expr::ArrayLit(n) => n.span,
            Expr::HashLit(n) => n.span,
            Expr::RangeLit(n) => n.span,
            Expr::RegexLit(n) => n.span,
            Expr::LocalVar(n) => n.span,
            Expr::InstanceVar(n) => n.span,
            Expr::ClassVar(n) => n.span,
            Expr::GlobalVar(n) => n.span,
            Expr::ConstRef(n) => n.span,
            Expr::SelfExpr(n) => n.span,
            Expr::BinaryOp(n) => n.span,
            Expr::UnaryOp(n) => n.span,
            Expr::MethodCall(n) => n.span,
            Expr::BlockCall(n) => n.span,
            Expr::SuperCall(n) => n.span,
            Expr::YieldExpr(n) => n.span,
            Expr::Lambda(n) => n.span,
            Expr::Proc(n) => n.span,
            Expr::PatternMatch(n) => n.span,
            Expr::Ternary(n) => n.span,
            Expr::Defined(n) => n.span,
        }
    }
}

// ── Helpers ────────────────────────────────────────────────

fn parse_int(s: &str) -> i64 {
    let s = s.replace('_', "");
    if s.starts_with("0x") || s.starts_with("0X") {
        i64::from_str_radix(&s[2..], 16).unwrap_or(0)
    } else if s.starts_with("0b") || s.starts_with("0B") {
        i64::from_str_radix(&s[2..], 2).unwrap_or(0)
    } else if s.starts_with("0o") || s.starts_with("0O") {
        i64::from_str_radix(&s[2..], 8).unwrap_or(0)
    } else {
        s.parse().unwrap_or(0)
    }
}

fn unescape_string(s: &str) -> String {
    // Remove opening/closing quotes
    let inner = if (s.starts_with('"') && s.ends_with('"'))
        || (s.starts_with('\'') && s.ends_with('\''))
    {
        &s[1..s.len() - 1]
    } else {
        s
    };
    let mut result = String::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => result.push('\n'),
                Some('t') => result.push('\t'),
                Some('r') => result.push('\r'),
                Some('\\') => result.push('\\'),
                Some('"') => result.push('"'),
                Some('\'') => result.push('\''),
                Some('0') => result.push('\0'),
                Some(other) => { result.push('\\'); result.push(other); }
                None => result.push('\\'),
            }
        } else {
            result.push(c);
        }
    }
    result
}

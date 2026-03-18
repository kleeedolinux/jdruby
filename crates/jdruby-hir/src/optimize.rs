use crate::nodes::*;

/// HIR-level optimizations.
pub struct HirOptimizer;

impl HirOptimizer {
    /// Run all HIR optimizations on a module.
    pub fn optimize(module: &mut HirModule) {
        for node in &mut module.nodes {
            Self::optimize_node(node);
        }
    }

    fn optimize_node(node: &mut HirNode) {
        // Recurse first (bottom-up)
        match node {
            HirNode::BinOp(op) => {
                Self::optimize_node(&mut op.left);
                Self::optimize_node(&mut op.right);
            }
            HirNode::UnOp(op) => Self::optimize_node(&mut op.operand),
            HirNode::Call(call) => {
                if let Some(r) = &mut call.receiver { Self::optimize_node(r); }
                for a in &mut call.args { Self::optimize_node(a); }
            }
            HirNode::Assign(a) => Self::optimize_node(&mut a.value),
            HirNode::Branch(b) => {
                Self::optimize_node(&mut b.condition);
                for n in &mut b.then_body { Self::optimize_node(n); }
                for n in &mut b.else_body { Self::optimize_node(n); }
            }
            HirNode::Loop(l) => {
                Self::optimize_node(&mut l.condition);
                for n in &mut l.body { Self::optimize_node(n); }
            }
            HirNode::FuncDef(f) => {
                for n in &mut f.body { Self::optimize_node(n); }
            }
            HirNode::ClassDef(c) => {
                for n in &mut c.body { Self::optimize_node(n); }
            }
            HirNode::Seq(nodes) => {
                for n in nodes { Self::optimize_node(n); }
            }
            _ => {}
        }

        // Apply optimizations
        Self::constant_fold(node);
        Self::dead_branch_elimination(node);
    }

    /// Constant folding: evaluate constant binary ops at compile time.
    fn constant_fold(node: &mut HirNode) {
        if let HirNode::BinOp(op) = node {
            if let (HirNode::Literal(left), HirNode::Literal(right)) = (&op.left, &op.right) {
                if let Some(result) = Self::fold_binary(&left.value, &op.op, &right.value) {
                    *node = HirNode::Literal(HirLiteral { value: result, span: op.span });
                }
            }
        }
        // Double negation elimination: !!x → x (truthiness)
        if let HirNode::UnOp(outer) = node {
            if outer.op == HirUnaryOp::Not {
                if let HirNode::UnOp(inner) = &outer.operand {
                    if inner.op == HirUnaryOp::Not {
                        *node = inner.operand.clone();
                    }
                }
            }
        }
    }

    fn fold_binary(left: &HirLiteralValue, op: &HirOp, right: &HirLiteralValue) -> Option<HirLiteralValue> {
        match (left, op, right) {
            // Integer arithmetic
            (HirLiteralValue::Integer(a), HirOp::Add, HirLiteralValue::Integer(b)) => Some(HirLiteralValue::Integer(a + b)),
            (HirLiteralValue::Integer(a), HirOp::Sub, HirLiteralValue::Integer(b)) => Some(HirLiteralValue::Integer(a - b)),
            (HirLiteralValue::Integer(a), HirOp::Mul, HirLiteralValue::Integer(b)) => Some(HirLiteralValue::Integer(a * b)),
            (HirLiteralValue::Integer(a), HirOp::Div, HirLiteralValue::Integer(b)) if *b != 0 => Some(HirLiteralValue::Integer(a / b)),
            (HirLiteralValue::Integer(a), HirOp::Mod, HirLiteralValue::Integer(b)) if *b != 0 => Some(HirLiteralValue::Integer(a % b)),
            // Integer comparison
            (HirLiteralValue::Integer(a), HirOp::Eq, HirLiteralValue::Integer(b)) => Some(HirLiteralValue::Bool(a == b)),
            (HirLiteralValue::Integer(a), HirOp::NotEq, HirLiteralValue::Integer(b)) => Some(HirLiteralValue::Bool(a != b)),
            (HirLiteralValue::Integer(a), HirOp::Lt, HirLiteralValue::Integer(b)) => Some(HirLiteralValue::Bool(a < b)),
            (HirLiteralValue::Integer(a), HirOp::Gt, HirLiteralValue::Integer(b)) => Some(HirLiteralValue::Bool(a > b)),
            // Float arithmetic
            (HirLiteralValue::Float(a), HirOp::Add, HirLiteralValue::Float(b)) => Some(HirLiteralValue::Float(a + b)),
            (HirLiteralValue::Float(a), HirOp::Sub, HirLiteralValue::Float(b)) => Some(HirLiteralValue::Float(a - b)),
            (HirLiteralValue::Float(a), HirOp::Mul, HirLiteralValue::Float(b)) => Some(HirLiteralValue::Float(a * b)),
            // String concatenation
            (HirLiteralValue::String(a), HirOp::Add, HirLiteralValue::String(b)) => {
                Some(HirLiteralValue::String(format!("{}{}", a, b)))
            }
            // Boolean logic
            (HirLiteralValue::Bool(a), HirOp::And, HirLiteralValue::Bool(b)) => Some(HirLiteralValue::Bool(*a && *b)),
            (HirLiteralValue::Bool(a), HirOp::Or, HirLiteralValue::Bool(b)) => Some(HirLiteralValue::Bool(*a || *b)),
            _ => None,
        }
    }

    /// Dead branch elimination: remove branches with constant conditions.
    fn dead_branch_elimination(node: &mut HirNode) {
        if let HirNode::Branch(branch) = node {
            if let HirNode::Literal(lit) = &branch.condition {
                match &lit.value {
                    HirLiteralValue::Bool(true) | HirLiteralValue::Integer(_)
                    | HirLiteralValue::String(_) | HirLiteralValue::Float(_)
                    | HirLiteralValue::Symbol(_) => {
                        *node = HirNode::Seq(branch.then_body.clone());
                    }
                    HirLiteralValue::Bool(false) | HirLiteralValue::Nil => {
                        *node = HirNode::Seq(branch.else_body.clone());
                    }
                    _ => {}
                }
            }
        }
    }
}

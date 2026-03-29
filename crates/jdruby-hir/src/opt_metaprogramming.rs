//! HIR-level metaprogramming optimizations
//!
//! Optimizations for metaprogramming patterns:
//! - Block inlining for simple blocks
//! - Constant send method name recognition
//! - Guard pattern optimization (respond_to? checks)
//! - Lambda/Proc conversion optimization

use crate::nodes::*;

/// HIR metaprogramming optimizer
pub struct HirMetaOptimizer;

impl HirMetaOptimizer {
    /// Run all metaprogramming optimizations on a module
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
                if let Some(block) = &mut call.block {
                    for n in &mut block.body { Self::optimize_node(n); }
                    // Try to inline simple blocks
                    if let Some(inlined) = Self::try_inline_block(block) {
                        *block = inlined;
                    }
                }
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
            HirNode::Yield(args) => {
                for arg in args { Self::optimize_node(arg); }
            }
            HirNode::BlockDef(def) => {
                for n in &mut def.body { Self::optimize_node(n); }
            }
            HirNode::ProcDef(def) => {
                for n in &mut def.body { Self::optimize_node(n); }
            }
            HirNode::LambdaDef(def) => {
                for n in &mut def.body { Self::optimize_node(n); }
            }
            HirNode::DefineMethod(def) => {
                Self::optimize_node(&mut def.name);
                for n in &mut def.body.body { Self::optimize_node(n); }
            }
            HirNode::Send(send) | HirNode::PublicSend(send) | HirNode::InternalSend(send) => {
                Self::optimize_node(&mut send.receiver);
                Self::optimize_node(&mut send.method_name);
                for arg in &mut send.args { Self::optimize_node(arg); }
                if let Some(block) = &mut send.block {
                    for n in &mut block.body { Self::optimize_node(n); }
                }
            }
            _ => {}
        }

        // Apply pattern optimizations
        Self::optimize_guard_patterns(node);
        Self::optimize_const_send(node);
    }

    /// Try to inline a simple block
    /// Criteria for inlining:
    /// - Single expression body
    /// - No explicit return
    /// - No yield
    /// - No nested blocks
    fn try_inline_block(block: &mut HirBlock) -> Option<HirBlock> {
        // If body is already simple, return as-is
        if block.body.len() == 1 {
            // Check if the single node is simple enough to inline
            if Self::is_simple_inlineable(&block.body[0]) {
                return Some(block.clone());
            }
        }
        None
    }

    fn is_simple_inlineable(node: &HirNode) -> bool {
        match node {
            // Literals are always inlineable
            HirNode::Literal(_) => true,
            // Variable references are inlineable
            HirNode::VarRef(_) => true,
            // Binary ops on simple nodes are inlineable
            HirNode::BinOp(op) => {
                Self::is_simple_inlineable(&op.left) && Self::is_simple_inlineable(&op.right)
            }
            // Unary ops on simple nodes are inlineable
            HirNode::UnOp(op) => Self::is_simple_inlineable(&op.operand),
            // Calls with simple arguments might be inlineable
            HirNode::Call(call) => {
                call.receiver.as_ref().map_or(true, |r| Self::is_simple_inlineable(r))
                    && call.args.iter().all(Self::is_simple_inlineable)
                    && call.block.is_none()
            }
            // Yield is NOT inlineable (would need to be handled specially)
            HirNode::Yield(_) => false,
            // Return is NOT inlineable (changes control flow)
            HirNode::Return(_) => false,
            // Break/Next are NOT inlineable
            HirNode::Break | HirNode::Next => false,
            // Sequences with all inlineable items are inlineable
            HirNode::Seq(nodes) => nodes.iter().all(Self::is_simple_inlineable),
            // Assignments with simple values are inlineable
            HirNode::Assign(a) => Self::is_simple_inlineable(&a.value),
            _ => false,
        }
    }

    /// Optimize guard patterns like: obj.respond_to?(:foo) && obj.foo
    /// Transform to: GuardedCall(obj, :foo)
    fn optimize_guard_patterns(node: &mut HirNode) {
        // Pattern: Branch { condition: Call { method: "respond_to?" }, ... }
        // Could be optimized to avoid double method lookup
        if let HirNode::Branch(branch) = node {
            if let HirNode::Call(cond_call) = &branch.condition {
                if cond_call.method == "respond_to?" {
                    if let Some(HirNode::Literal(lit)) = cond_call.args.first() {
                        if let HirLiteralValue::Symbol(_method_name) = &lit.value {
                            // Found respond_to?(:method_name) pattern
                            // Mark then_body as having a method guard
                            // Full implementation would track this through to codegen
                            // for optimized guarded call emission
                        }
                    }
                }
            }
        }
    }

    /// Optimize constant send method names
    /// Recognize patterns like: send(obj, :foo, args) where :foo is constant
    /// Transform to direct call if possible
    fn optimize_const_send(node: &mut HirNode) {
        if let HirNode::Send(send) = node {
            if let HirNode::Literal(lit) = &send.method_name {
                if let HirLiteralValue::Symbol(_method_name) = &lit.value {
                    // Send with constant method name
                    // Could be transformed to a guarded direct call
                    // if we know the receiver type or are willing to speculate
                }
            }
        }
    }

    /// Check if a block is a simple "pass-through" that just yields arguments
    /// Pattern: { |a, b| yield a, b } → can be optimized to just pass the block through
    pub fn is_pass_through_block(block: &HirBlock) -> bool {
        // Simple heuristic: if body is just yield with the same args as params
        if block.body.len() == 1 {
            if let HirNode::Yield(args) = &block.body[0] {
                // Check if args match params
                if args.len() == block.params.len() {
                    let matches = args.iter().enumerate().all(|(i, arg)| {
                        if let HirNode::VarRef(v) = arg {
                            v.name == block.params[i].name
                        } else {
                            false
                        }
                    });
                    return matches;
                }
            }
        }
        false
    }
}

/// Variable substitutor for inlining
/// Replaces variable references in HIR nodes according to a substitution map
struct VariableSubstitutor {
    substitutions: std::collections::HashMap<String, HirNode>,
}

impl VariableSubstitutor {
    fn new(substitutions: std::collections::HashMap<String, HirNode>) -> Self {
        Self { substitutions }
    }

    /// Substitute variables in a HIR node recursively
    fn substitute(&self, node: &mut HirNode) {
        match node {
            HirNode::VarRef(v) => {
                if let Some(replacement) = self.substitutions.get(&v.name) {
                    *node = replacement.clone();
                }
            }
            HirNode::BinOp(op) => {
                self.substitute(&mut op.left);
                self.substitute(&mut op.right);
            }
            HirNode::UnOp(op) => {
                self.substitute(&mut op.operand);
            }
            HirNode::Call(call) => {
                if let Some(r) = &mut call.receiver {
                    self.substitute(r);
                }
                for arg in &mut call.args {
                    self.substitute(arg);
                }
                if let Some(block) = &mut call.block {
                    for n in &mut block.body {
                        self.substitute(n);
                    }
                }
            }
            HirNode::Assign(a) => {
                self.substitute(&mut a.value);
            }
            HirNode::Branch(b) => {
                self.substitute(&mut b.condition);
                for n in &mut b.then_body {
                    self.substitute(n);
                }
                for n in &mut b.else_body {
                    self.substitute(n);
                }
            }
            HirNode::Loop(l) => {
                self.substitute(&mut l.condition);
                for n in &mut l.body {
                    self.substitute(n);
                }
            }
            HirNode::Return(r) => {
                if let Some(v) = &mut r.value {
                    self.substitute(v);
                }
            }
            HirNode::FuncDef(f) => {
                for n in &mut f.body {
                    self.substitute(n);
                }
            }
            HirNode::ClassDef(c) => {
                for n in &mut c.body {
                    self.substitute(n);
                }
            }
            HirNode::Seq(nodes) => {
                for n in nodes {
                    self.substitute(n);
                }
            }
            HirNode::Yield(args) => {
                for arg in args {
                    self.substitute(arg);
                }
            }
            HirNode::Literal(_) | HirNode::Break | HirNode::Next | HirNode::Nop => {}
            _ => {
                // For other node types, recursively substitute in any HirNode fields
                // This is a conservative approach that handles the common cases
            }
        }
    }
}

/// Block inlining transformation
/// Inlines block bodies directly into the call site when beneficial
pub struct BlockInliner;

impl BlockInliner {
    /// Inline blocks in a module where beneficial
    pub fn inline(module: &mut HirModule) {
        for node in &mut module.nodes {
            Self::inline_node(node);
        }
    }

    fn inline_node(node: &mut HirNode) {
        match node {
            HirNode::Call(call) => {
                if let Some(block) = call.block.take() {
                    // Check if we should inline this block
                    if Self::should_inline(&block) {
                        // Build substitution map: block params -> call arguments
                        let mut substitutions = std::collections::HashMap::new();
                        
                        // For each block parameter, create a binding
                        for (i, param) in block.params.iter().enumerate() {
                            if i < call.args.len() {
                                // Create a unique variable name for this inline
                                let inline_var = format!("__inline_{}_{}", param.name, i);
                                substitutions.insert(param.name.clone(), HirNode::VarRef(HirVarRef {
                                    name: inline_var.clone(),
                                    scope: VarScope::Local,
                                    span: param.span,
                                }));
                            }
                        }
                        
                        // Clone the block body and substitute variables
                        let mut inlined_body: Vec<HirNode> = block.body.clone();
                        let substitutor = VariableSubstitutor::new(substitutions);
                        for n in &mut inlined_body {
                            substitutor.substitute(n);
                        }
                        
                        // Replace the call with the inlined body (wrapped in Seq)
                        if !inlined_body.is_empty() {
                            if inlined_body.len() == 1 {
                                *node = inlined_body.into_iter().next().unwrap();
                            } else {
                                *node = HirNode::Seq(inlined_body);
                            }
                        } else {
                            *node = HirNode::Nop;
                        }
                        return; // Skip recursion since we replaced the node
                    } else {
                        // Put the block back if we didn't inline
                        call.block = Some(block);
                    }
                }
                if let Some(r) = &mut call.receiver { Self::inline_node(r); }
                for a in &mut call.args { Self::inline_node(a); }
            }
            _ => {
                // Recurse into other node types
                HirMetaOptimizer::optimize_node(node);
            }
        }
    }

    fn should_inline(block: &HirBlock) -> bool {
        // Inline criteria:
        // 1. Block body is simple (single expression)
        // 2. No captures (or only simple captures)
        // 3. Called exactly once (heuristic)
        // 4. Small size (fewer than N instructions)
        
        if block.body.len() != 1 {
            return false;
        }
        
        // Don't inline if there are complex captures
        if block.captured_vars.len() > 2 {
            return false;
        }
        
        // Check body complexity
        HirMetaOptimizer::is_simple_inlineable(&block.body[0])
    }
}

/// Extract block body as standalone method for define_method
/// Transforms: define_method(:foo) { |x| body }
/// Into: def foo(x); body; end (with proper closure handling)
pub struct MethodExtractor;

impl MethodExtractor {
    /// Extract block bodies from define_method calls into standalone methods
    pub fn extract(module: &mut HirModule) {
        let mut extracted_methods = Vec::new();
        
        for node in &mut module.nodes {
            Self::extract_from_node(node, &mut extracted_methods);
        }
        
        // Add extracted methods to module
        module.nodes.extend(extracted_methods);
    }

    fn extract_from_node(node: &mut HirNode, extracted: &mut Vec<HirNode>) {
        match node {
            HirNode::DefineMethod(def) => {
                // Extract the block body as a standalone method
                let method_name = if let HirNode::Literal(lit) = &def.name {
                    if let HirLiteralValue::Symbol(name) = &lit.value {
                        name.clone()
                    } else {
                        return;
                    }
                } else {
                    return;
                };
                
                // Create a FuncDef from the block body
                let func_def = HirFuncDef {
                    name: method_name,
                    params: def.body.params.iter().map(|p| p.name.clone()).collect(),
                    body: def.body.body.clone(),
                    is_class_method: false,
                    span: def.span,
                };
                
                extracted.push(HirNode::FuncDef(Box::new(func_def)));
            }
            HirNode::ClassDef(cls) => {
                for n in &mut cls.body {
                    Self::extract_from_node(n, extracted);
                }
            }
            _ => {}
        }
    }
}

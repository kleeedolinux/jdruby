//! # Ruby Proc and Block Implementation
//!
//! Procedure objects and code blocks.
//! Follows MRI's proc.c structure.

use std::collections::HashMap;

/// Ruby Proc - procedure object
#[repr(C)]
pub struct RubyProc {
    /// Local variable bindings captured from scope
    pub binding: Option<RubyBinding>,
    /// Arity (number of arguments)
    pub arity: i32,
    /// Is a lambda (strict argument checking)
    pub is_lambda: bool,
    /// Block body (could be bytecode pointer or native function)
    pub body: ProcBody,
}

#[repr(C)]
pub union ProcBody {
    /// Bytecode pointer
    pub iseq: *mut u8,
    /// Native function pointer
    pub native: *mut u8,
}

/// Ruby Binding - captures local variable scope
#[repr(C)]
pub struct RubyBinding {
    /// Local variables (name -> value)
    pub locals: HashMap<String, u64>,
    /// Self object
    pub self_obj: u64,
    /// Reference to class where binding was created
    pub klass: u64,
}

/// Block representation
#[repr(C)]
pub enum Block {
    /// No block given
    None,
    /// Proc block
    Proc(Box<RubyProc>),
    /// Symbol reference (e.g., &:to_s)
    Symbol(u64),
    /// Native function block
    Native(fn(&[u64]) -> u64),
}

impl RubyProc {
    /// Create a new Proc
    pub fn new(body: ProcBody) -> Self {
        Self {
            binding: None,
            arity: -1, // Variable args by default
            is_lambda: false,
            body,
        }
    }

    /// Create a new Lambda
    pub fn new_lambda(body: ProcBody, arity: i32) -> Self {
        Self {
            binding: None,
            arity,
            is_lambda: true,
            body,
        }
    }

    /// Set binding
    pub fn set_binding(&mut self, binding: RubyBinding) {
        self.binding = Some(binding);
    }

    /// Get arity
    pub fn arity(&self) -> i32 {
        self.arity
    }

    /// Check if lambda
    pub fn is_lambda(&self) -> bool {
        self.is_lambda
    }

    /// Convert proc to lambda
    pub fn to_lambda(mut self) -> Self {
        self.is_lambda = true;
        self
    }

    /// Curry the proc (partial application)
    pub fn curry(&self, _args: &[u64]) -> Self {
        // Simplified - just clone
        Self {
            binding: self.binding.clone(),
            arity: self.arity.saturating_sub(_args.len() as i32),
            is_lambda: self.is_lambda,
            body: unsafe { ProcBody { iseq: self.body.iseq } },
        }
    }

    /// Call the proc with arguments
    pub fn call(&self, args: &[u64]) -> u64 {
        if self.is_lambda && self.arity >= 0 {
            // Lambda checks argument count
            if args.len() != self.arity as usize {
                panic!("wrong number of arguments");
            }
        }
        
        // Placeholder - would execute block body
        // In real implementation, this would interpret bytecode
        // or call native function
        0 // Return nil placeholder
    }
}

impl Clone for RubyBinding {
    fn clone(&self) -> Self {
        Self {
            locals: self.locals.clone(),
            self_obj: self.self_obj,
            klass: self.klass,
        }
    }
}

impl RubyBinding {
    /// Create new binding
    pub fn new(self_obj: u64, klass: u64) -> Self {
        Self {
            locals: HashMap::new(),
            self_obj,
            klass,
        }
    }

    /// Get local variable
    pub fn local_get(&self, name: &str) -> Option<u64> {
        self.locals.get(name).copied()
    }

    /// Set local variable
    pub fn local_set(&mut self, name: &str, value: u64) {
        self.locals.insert(name.to_string(), value);
    }

    /// Get self
    pub fn self_obj(&self) -> u64 {
        self.self_obj
    }
}

impl Block {
    /// Check if block is given
    pub fn is_given(&self) -> bool {
        !matches!(self, Block::None)
    }

    /// Yield to block with arguments
    pub fn yield_block(&self, args: &[u64]) -> u64 {
        match self {
            Block::None => panic!("no block given"),
            Block::Proc(proc) => proc.call(args),
            Block::Symbol(_sym) => {
                // Would call method on object
                0
            }
            Block::Native(f) => f(args),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proc_new() {
        let body = ProcBody { iseq: std::ptr::null_mut() };
        let proc = RubyProc::new(body);
        assert_eq!(proc.arity(), -1);
        assert!(!proc.is_lambda());
    }

    #[test]
    fn test_lambda_new() {
        let body = ProcBody { iseq: std::ptr::null_mut() };
        let lambda = RubyProc::new_lambda(body, 2);
        assert_eq!(lambda.arity(), 2);
        assert!(lambda.is_lambda());
    }

    #[test]
    fn test_binding() {
        let mut binding = RubyBinding::new(1, 2);
        binding.local_set("x", 42);
        binding.local_set("y", 100);
        
        assert_eq!(binding.local_get("x"), Some(42));
        assert_eq!(binding.local_get("y"), Some(100));
        assert_eq!(binding.local_get("z"), None);
    }

    #[test]
    fn test_block_none() {
        let block = Block::None;
        assert!(!block.is_given());
    }

    #[test]
    fn test_block_given() {
        let body = ProcBody { iseq: std::ptr::null_mut() };
        let proc = RubyProc::new(body);
        let block = Block::Proc(Box::new(proc));
        assert!(block.is_given());
    }
}

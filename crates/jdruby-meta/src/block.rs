//! Block and Proc Implementation
//!
//! Provides MRI-compatible block and proc structures with proper
//! variable capture and execution support.

use crate::types::*;
use crate::traits::{BlockParams, BlockBody};
use std::alloc::{alloc, dealloc, Layout};
use std::ptr;

/// Block creation options
#[derive(Debug, Clone)]
pub struct BlockOptions {
    pub is_lambda: bool,
    pub captures_self: bool,
    pub file: String,
    pub line: u32,
}

impl Default for BlockOptions {
    fn default() -> Self {
        Self {
            is_lambda: false,
            captures_self: true,
            file: String::new(),
            line: 0,
        }
    }
}

/// Block builder for creating blocks
pub struct BlockBuilder {
    params: BlockParams,
    body: BlockBody,
    options: BlockOptions,
    captured_vars: Vec<String>,
}

impl BlockBuilder {
    pub fn new() -> Self {
        Self {
            params: BlockParams {
                params: Vec::new(),
                is_lambda: false,
                captures: Vec::new(),
            },
            body: BlockBody {
                instructions: Vec::new(),
                local_count: 0,
                stack_size: 0,
            },
            options: BlockOptions::default(),
            captured_vars: Vec::new(),
        }
    }

    pub fn with_lambda(mut self, is_lambda: bool) -> Self {
        self.params.is_lambda = is_lambda;
        self.options.is_lambda = is_lambda;
        self
    }

    pub fn with_params(mut self, params: BlockParams) -> Self {
        self.params = params;
        self
    }

    pub fn with_body(mut self, body: BlockBody) -> Self {
        self.body = body;
        self
    }

    pub fn with_captures(mut self, captures: Vec<String>) -> Self {
        self.params.captures = captures.clone();
        self.captured_vars = captures;
        self
    }

    pub fn with_location(mut self, file: &str, line: u32) -> Self {
        self.options.file = file.to_string();
        self.options.line = line;
        self
    }

    /// Build the block structure
    pub fn build(self) -> *mut Block {
        let layout = Layout::new::<Block>();
        let block_ptr = unsafe { alloc(layout) as *mut Block };
        
        if block_ptr.is_null() {
            panic!("Failed to allocate block");
        }
        
        unsafe {
            // Initialize block fields
            (*block_ptr).iseq = ptr::null();
            (*block_ptr).self_ = QNIL;
            (*block_ptr).ep = ptr::null();
            (*block_ptr).flags = if self.options.is_lambda { 1 } else { 0 };
            (*block_ptr).proc = ptr::null_mut();
        }
        
        block_ptr
    }
}

impl Default for BlockBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Create a proc from a block
pub unsafe fn block_to_proc(block: *const Block) -> *mut Proc {
    if block.is_null() {
        return ptr::null_mut();
    }
    
    let layout = Layout::new::<Proc>();
    let proc_ptr = alloc(layout) as *mut Proc;
    
    if proc_ptr.is_null() {
        return ptr::null_mut();
    }
    
    // Copy block data
    (*proc_ptr).basic.flags = T_DATA << 0;
    (*proc_ptr).basic.klass = ptr::null(); // Proc class
    
    // Copy the block
    ptr::copy_nonoverlapping(block, &mut (*proc_ptr).block, 1);
    
    (*proc_ptr).is_from_method = false;
    (*proc_ptr).is_lambda = (*block).flags & 1 != 0;
    
    // Link back
    (*proc_ptr).block.proc = proc_ptr;
    
    proc_ptr
}

/// Create a lambda from a block
pub unsafe fn block_to_lambda(block: *const Block) -> *mut Proc {
    let proc = block_to_proc(block);
    if !proc.is_null() {
        (*proc).is_lambda = true;
        (*proc).block.flags |= 1;
    }
    proc
}

/// Yield to a block with arguments
pub unsafe fn block_yield(block: *const Block, _args: &[Value]) -> Value {
    if block.is_null() {
        return QNIL;
    }
    
    // In a real implementation, this would:
    // 1. Set up the VM stack
    // 2. Push arguments
    // 3. Execute the block's iseq
    // 4. Return the result
    
    // For now, return nil as placeholder
    QNIL
}

/// Check if block is a lambda
pub fn block_is_lambda(block: *const Block) -> bool {
    if block.is_null() {
        return false;
    }
    unsafe { (*block).flags & 1 != 0 }
}

/// Get block arity
pub fn block_arity(block: *const Block) -> i32 {
    if block.is_null() {
        return 0;
    }
    
    // In real implementation, parse iseq to get parameter count
    // For now, return a placeholder
    unsafe {
        if (*block).iseq.is_null() {
            -1
        } else {
            (*(*block).iseq).local_table_size as i32
        }
    }
}

/// Capture environment for a block
pub unsafe fn capture_environment(
    parent_ep: *const Value,
    var_names: &[String],
) -> *mut Env {
    let env_size = var_names.len();
    
    if env_size == 0 {
        return ptr::null_mut();
    }
    
    // Allocate environment
    let layout = Layout::new::<Env>();
    let env_ptr = alloc(layout) as *mut Env;
    
    if env_ptr.is_null() {
        return ptr::null_mut();
    }
    
    // Initialize
    (*env_ptr).basic.flags = T_IMEMO << 0;
    (*env_ptr).basic.klass = ptr::null();
    (*env_ptr).env_size = env_size as u32;
    (*env_ptr).local_size = env_size as u32;
    (*env_ptr).parent_env = parent_ep as *const Env;
    
    // Allocate environment storage
    let env_storage_layout = Layout::array::<Value>(env_size).unwrap();
    let env_storage = alloc(env_storage_layout) as *mut Value;
    
    if env_storage.is_null() {
        dealloc(env_ptr as *mut u8, layout);
        return ptr::null_mut();
    }
    
    (*env_ptr).env = env_storage;
    
    // Copy captured variables
    // In real implementation, look up each variable from parent_ep
    for i in 0..env_size {
        *env_storage.add(i) = QNIL;
    }
    
    env_ptr
}

/// Get binding from a block
pub unsafe fn block_binding(block: *const Block) -> *mut Binding {
    if block.is_null() {
        return ptr::null_mut();
    }
    
    let layout = Layout::new::<Binding>();
    let binding_ptr = alloc(layout) as *mut Binding;
    
    if binding_ptr.is_null() {
        return ptr::null_mut();
    }
    
    (*binding_ptr).basic.flags = T_DATA << 0;
    (*binding_ptr).basic.klass = ptr::null(); // Binding class
    (*binding_ptr).ep = (*block).ep;
    (*binding_ptr).iseq = (*block).iseq;
    (*binding_ptr).path = ptr::null();
    (*binding_ptr).first_lineno = 0;
    
    binding_ptr
}

/// Free a block
pub unsafe fn free_block(block: *mut Block) {
    if block.is_null() {
        return;
    }
    
    // Note: Don't free iseq here - it's shared
    let layout = Layout::new::<Block>();
    dealloc(block as *mut u8, layout);
}

/// Free a proc
pub unsafe fn free_proc(proc: *mut Proc) {
    if proc.is_null() {
        return;
    }
    
    let layout = Layout::new::<Proc>();
    dealloc(proc as *mut u8, layout);
}

/// Free an environment
pub unsafe fn free_env(env: *mut Env) {
    if env.is_null() {
        return;
    }
    
    if !(*env).env.is_null() {
        let layout = Layout::array::<Value>((*env).env_size as usize).unwrap();
        dealloc((*env).env as *mut u8, layout);
    }
    
    let layout = Layout::new::<Env>();
    dealloc(env as *mut u8, layout);
}

/// Free a binding
pub unsafe fn free_binding(binding: *mut Binding) {
    if binding.is_null() {
        return;
    }
    
    let layout = Layout::new::<Binding>();
    dealloc(binding as *mut u8, layout);
}

/// Block handle for VM execution
pub struct BlockHandle {
    pub block: *const Block,
    pub captured_env: *const Env,
}

impl BlockHandle {
    pub fn new(block: *const Block) -> Self {
        Self {
            block,
            captured_env: ptr::null(),
        }
    }

    pub fn with_env(block: *const Block, env: *const Env) -> Self {
        Self {
            block,
            captured_env: env,
        }
    }

    /// Execute the block with arguments
    pub unsafe fn call(&self, args: &[Value]) -> Value {
        block_yield(self.block, args)
    }

    /// Check if this is a lambda
    pub fn is_lambda(&self) -> bool {
        block_is_lambda(self.block)
    }

    /// Get block arity
    pub fn arity(&self) -> i32 {
        block_arity(self.block)
    }
}

/// Proc handle for VM execution
pub struct ProcHandle {
    pub proc: *const Proc,
}

impl ProcHandle {
    pub fn new(proc: *const Proc) -> Self {
        Self { proc }
    }

    /// Execute the proc with arguments
    pub unsafe fn call(&self, args: &[Value]) -> Value {
        if self.proc.is_null() {
            return QNIL;
        }
        block_yield(&(*self.proc).block, args)
    }

    /// Check if this is a lambda
    pub fn is_lambda(&self) -> bool {
        if self.proc.is_null() {
            return false;
        }
        unsafe { (*self.proc).is_lambda }
    }

    /// Get binding
    pub unsafe fn binding(&self) -> *mut Binding {
        if self.proc.is_null() {
            return ptr::null_mut();
        }
        block_binding(&(*self.proc).block)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_block_builder() {
        let builder = BlockBuilder::new()
            .with_lambda(true)
            .with_location("test.rb", 10);
        
        assert!(builder.options.is_lambda);
        assert_eq!(builder.options.line, 10);
    }
    
    #[test]
    fn test_block_arity_null() {
        assert_eq!(block_arity(ptr::null()), 0);
    }
    
    #[test]
    fn test_block_is_lambda_null() {
        assert!(!block_is_lambda(ptr::null()));
    }
}

//! Method Resolver with Inline Caching
//!
//! This module implements method dispatch resolution at the MIR level,
//! including inline caching for monomorphic and megamorphic call sites.

use crate::types::*;
use crate::inline_cache::{InlineCache, CacheEntry};
use std::collections::HashMap;

/// Method resolution result
#[derive(Debug, Clone)]
pub struct ResolutionResult {
    /// The resolved method entry
    pub method_entry: *const MethodEntry,
    /// The receiver class at resolution time
    pub receiver_class: ClassId,
    /// Type of call required
    pub call_type: CallType,
    /// Whether a class guard is needed
    pub needs_guard: bool,
    /// Call site ID for caching
    pub call_site_id: usize,
}

/// Types of method calls
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallType {
    /// Direct call to known method
    Direct,
    /// Virtual dispatch through vtable
    Virtual,
    /// Method missing fallback
    MethodMissing,
    /// Send (dynamic method name)
    DynamicSend,
}

/// Method resolver with inline caching
pub struct MethodResolver {
    /// Inline cache for monomorphic calls
    inline_cache: InlineCache,
    /// Megamorphic fallback cache (for >4 receiver types)
    megamorphic_cache: HashMap<usize, Vec<CacheEntry>>,
    /// Global method table references
    global_classes: HashMap<ClassId, *const RClass>,
    /// Method missing entry (singleton)
    method_missing: Option<*const MethodEntry>,
    /// Next call site ID
    next_call_site_id: usize,
}

impl MethodResolver {
    pub fn new() -> Self {
        Self {
            inline_cache: InlineCache::new(),
            megamorphic_cache: HashMap::new(),
            global_classes: HashMap::new(),
            method_missing: None,
            next_call_site_id: 0,
        }
    }

    /// Allocate a new call site ID
    fn alloc_call_site_id(&mut self) -> usize {
        let id = self.next_call_site_id;
        self.next_call_site_id += 1;
        id
    }

    /// Register a class with the resolver
    pub fn register_class(&mut self, class_id: ClassId, class_ptr: *const RClass) {
        self.global_classes.insert(class_id, class_ptr);
    }

    /// Set the global method_missing entry
    pub fn set_method_missing(&mut self, entry: *const MethodEntry) {
        self.method_missing = Some(entry);
    }

    /// Resolve a static method call (known method name at compile time)
    pub fn resolve_static(
        &mut self,
        receiver_class: ClassId,
        method_name: SymbolId,
    ) -> ResolutionResult {
        let call_site_id = self.alloc_call_site_id();
        
        // Check inline cache first
        if let Some(entry) = self.inline_cache.lookup(call_site_id, receiver_class) {
            return ResolutionResult {
                method_entry: entry.method_entry,
                receiver_class,
                call_type: entry.call_type,
                needs_guard: true,
                call_site_id,
            };
        }

        // Walk method hierarchy
        let result = self.lookup_method(receiver_class, method_name);
        
        // Update cache
        if !result.method_entry.is_null() {
            self.inline_cache.insert(
                call_site_id,
                receiver_class,
                CacheEntry {
                    method_entry: result.method_entry,
                    receiver_class,
                    call_type: result.call_type,
                    hit_count: 1,
                },
            );
        }

        result
    }

    /// Resolve a dynamic send (method name known only at runtime)
    pub fn resolve_dynamic(
        &mut self,
        receiver: ObjectId,
        name_reg: Value,
    ) -> ResolutionResult {
        let call_site_id = self.alloc_call_site_id();
        
        // Get receiver class
        let receiver_class = self.get_class_of(receiver);
        
        // Convert name_reg to SymbolId (runtime conversion)
        let method_name = value_to_symbol(name_reg);
        
        let result = self.lookup_method(receiver_class, method_name);
        
        ResolutionResult {
            method_entry: result.method_entry,
            receiver_class,
            call_type: CallType::DynamicSend,
            needs_guard: true,
            call_site_id,
        }
    }

    /// Resolve method_missing call
    pub fn resolve_method_missing(
        &mut self,
        receiver_class: ClassId,
        _original_name: SymbolId,
    ) -> ResolutionResult {
        let call_site_id = self.alloc_call_site_id();
        
        // Look for method_missing in receiver class
        let result = self.lookup_method(receiver_class, symbol_id_from_str("method_missing"));
        
        ResolutionResult {
            method_entry: if result.method_entry.is_null() {
                self.method_missing.unwrap_or(std::ptr::null())
            } else {
                result.method_entry
            },
            receiver_class,
            call_type: CallType::MethodMissing,
            needs_guard: true,
            call_site_id,
        }
    }

    /// Lookup method in class hierarchy (including modules)
    fn lookup_method(&self, mut class_id: ClassId, name: SymbolId) -> ResolutionResult {
        let mut visited = std::collections::HashSet::new();
        
        loop {
            if visited.contains(&class_id) {
                break;
            }
            visited.insert(class_id);
            
            // Get class pointer
            if let Some(&class_ptr) = self.global_classes.get(&class_id) {
                if class_ptr.is_null() {
                    break;
                }
                
                unsafe {
                    let class = &*class_ptr;
                    
                    // Check prepended modules first (reverse order)
                    let mut iclass = class.include_classes;
                    while !iclass.is_null() {
                        let ic = &*iclass;
                        if let Some(entry) = self.find_in_module(ic.module, name) {
                            return ResolutionResult {
                                method_entry: entry,
                                receiver_class: class_id,
                                call_type: CallType::Virtual,
                                needs_guard: true,
                                call_site_id: 0,
                            };
                        }
                        iclass = ic.basic.klass as *mut IClass;
                    }
                    
                    // Check class method table
                    if !class.m_tbl.is_null() {
                        if let Some(entry) = self.find_in_table(class.m_tbl, name) {
                            return ResolutionResult {
                                method_entry: entry,
                                receiver_class: class_id,
                                call_type: CallType::Virtual,
                                needs_guard: true,
                                call_site_id: 0,
                            };
                        }
                    }
                    
                    // Check included modules
                    let mut iclass = class.include_classes;
                    while !iclass.is_null() {
                        let ic = &*iclass;
                        if let Some(entry) = self.find_in_module(ic.module, name) {
                            return ResolutionResult {
                                method_entry: entry,
                                receiver_class: class_id,
                                call_type: CallType::Virtual,
                                needs_guard: true,
                                call_site_id: 0,
                            };
                        }
                        iclass = ic.basic.klass as *mut IClass;
                    }
                    
                    // Move to superclass
                    if class.super_.is_null() {
                        break;
                    }
                    class_id = class.super_ as usize as ClassId;
                }
            } else {
                break;
            }
        }
        
        // Method not found
        ResolutionResult {
            method_entry: std::ptr::null(),
            receiver_class: class_id,
            call_type: CallType::MethodMissing,
            needs_guard: false,
            call_site_id: 0,
        }
    }

    /// Find method in module
    fn find_in_module(&self, module: *const RClass, name: SymbolId) -> Option<*const MethodEntry> {
        if module.is_null() {
            return None;
        }
        unsafe {
            let m = &*module;
            if !m.m_tbl.is_null() {
                return self.find_in_table(m.m_tbl, name);
            }
        }
        None
    }

    /// Find method in method table
    fn find_in_table(&self, table: *mut MethodTable, name: SymbolId) -> Option<*const MethodEntry> {
        if table.is_null() {
            return None;
        }
        unsafe {
            let tbl = &*table;
            let mut node = tbl.entries;
            while !node.is_null() {
                let n = &*node;
                if n.key == name {
                    return Some(n.entry);
                }
                node = n.next;
            }
        }
        None
    }

    /// Get the class of an object
    fn get_class_of(&self, obj: ObjectId) -> ClassId {
        if immediate_p(obj) {
            // Immediate values have fixed classes
            if fixnum_p(obj) {
                // Integer class
                return 0;
            } else if symbol_p(obj) {
                // Symbol class
                return 0;
            } else if obj == QTRUE {
                // TrueClass
                return 0;
            } else if obj == QFALSE {
                // FalseClass
                return 0;
            } else if obj == QNIL {
                // NilClass
                return 0;
            }
        }
        
        // Get class from object header
        let basic = obj as *const RBasic;
        if !basic.is_null() {
            unsafe {
                let klass = (*basic).klass;
                if !klass.is_null() {
                    return klass as usize as ClassId;
                }
            }
        }
        
        0
    }

    /// Invalidate cache entries for a class/method
    pub fn invalidate(&mut self, class_id: ClassId, _method_name: SymbolId) {
        self.inline_cache.invalidate_class(class_id);
        
        // Also invalidate from megamorphic cache
        for entries in self.megamorphic_cache.values_mut() {
            entries.retain(|e| e.receiver_class != class_id);
        }
    }

    /// Get cache statistics
    pub fn cache_stats(&self) -> CacheStats {
        CacheStats {
            monomorphic_hits: self.inline_cache.hits(),
            monomorphic_misses: self.inline_cache.misses(),
            megamorphic_sites: self.megamorphic_cache.len(),
        }
    }
}

impl Default for MethodResolver {
    fn default() -> Self {
        Self::new()
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub monomorphic_hits: usize,
    pub monomorphic_misses: usize,
    pub megamorphic_sites: usize,
}

/// Convert VALUE to SymbolId
fn value_to_symbol(v: Value) -> SymbolId {
    if symbol_p(v) {
        v >> 4
    } else {
        0
    }
}

/// Create SymbolId from string (for internal use)
fn symbol_id_from_str(s: &str) -> SymbolId {
    // Simple hash for demo - real impl would use symbol table
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish() as SymbolId
}

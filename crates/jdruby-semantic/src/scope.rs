use std::collections::HashMap;
use crate::types::{RubyType, MethodSignature, Visibility};

/// The kind of scope we're currently in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeKind {
    /// Top-level / main scope.
    TopLevel,
    /// Inside a class body.
    Class,
    /// Inside a module body.
    Module,
    /// Inside a method body.
    Method,
    /// Inside a block (do...end or { }).
    Block,
    /// Inside a lambda/proc.
    Lambda,
    /// Inside a rescue clause.
    Rescue,
}

/// A symbol entry in the symbol table.
#[derive(Debug, Clone)]
pub struct Symbol {
    /// The symbol name.
    pub name: String,
    /// What kind of symbol this is.
    pub kind: SymbolKind,
    /// Inferred or declared type.
    pub ty: RubyType,
    /// Whether this symbol has been assigned a value.
    pub initialized: bool,
    /// Number of times this symbol is referenced.
    pub ref_count: u32,
    /// Visibility (for methods).
    pub visibility: Visibility,
}

/// What kind of symbol an entry represents.
#[derive(Debug, Clone, PartialEq)]
pub enum SymbolKind {
    /// A local variable.
    LocalVar,
    /// An instance variable (@).
    InstanceVar,
    /// A class variable (@@).
    ClassVar,
    /// A global variable ($).
    GlobalVar,
    /// A constant.
    Constant,
    /// A method definition.
    Method(MethodSignature),
    /// A class definition.
    Class { superclass: Option<String> },
    /// A module definition.
    Module,
    /// A parameter.
    Param,
    /// A block parameter.
    BlockParam,
}

/// A lexical scope containing symbol definitions.
#[derive(Debug, Clone)]
pub struct Scope {
    /// What kind of scope this is.
    pub kind: ScopeKind,
    /// The name of this scope (class name, method name, etc.)
    pub name: Option<String>,
    /// Symbols defined in this scope.
    pub symbols: HashMap<String, Symbol>,
    /// Current method visibility (public/private/protected).
    pub current_visibility: Visibility,
}

impl Scope {
    pub fn new(kind: ScopeKind, name: Option<String>) -> Self {
        Self {
            kind, name,
            symbols: HashMap::new(),
            current_visibility: Visibility::Public,
        }
    }

    /// Define a symbol in this scope.
    pub fn define(&mut self, sym: Symbol) {
        self.symbols.insert(sym.name.clone(), sym);
    }

    /// Look up a symbol in this scope only.
    pub fn lookup(&self, name: &str) -> Option<&Symbol> {
        self.symbols.get(name)
    }

    /// Look up a symbol mutably.
    pub fn lookup_mut(&mut self, name: &str) -> Option<&mut Symbol> {
        self.symbols.get_mut(name)
    }

    /// Check if a symbol exists in this scope.
    pub fn has(&self, name: &str) -> bool {
        self.symbols.contains_key(name)
    }
}

/// A scope stack for nested lexical scoping.
#[derive(Debug)]
pub struct ScopeStack {
    scopes: Vec<Scope>,
}

impl ScopeStack {
    pub fn new() -> Self {
        let mut stack = Self { scopes: Vec::new() };
        stack.push(ScopeKind::TopLevel, None);
        stack
    }

    /// Push a new scope onto the stack.
    pub fn push(&mut self, kind: ScopeKind, name: Option<String>) {
        self.scopes.push(Scope::new(kind, name));
    }

    /// Pop the current scope.
    pub fn pop(&mut self) -> Option<Scope> {
        if self.scopes.len() > 1 { self.scopes.pop() } else { None }
    }

    /// Get the current (topmost) scope.
    pub fn current(&self) -> &Scope {
        self.scopes.last().unwrap()
    }

    /// Get the current scope mutably.
    pub fn current_mut(&mut self) -> &mut Scope {
        self.scopes.last_mut().unwrap()
    }

    /// Define a symbol in the current scope.
    pub fn define(&mut self, sym: Symbol) {
        self.current_mut().define(sym);
    }

    /// Look up a symbol, walking up the scope chain.
    pub fn lookup(&self, name: &str) -> Option<&Symbol> {
        for scope in self.scopes.iter().rev() {
            if let Some(sym) = scope.lookup(name) {
                return Some(sym);
            }
        }
        None
    }

    /// Look up a symbol mutably in the nearest scope that has it.
    pub fn lookup_mut(&mut self, name: &str) -> Option<&mut Symbol> {
        for scope in self.scopes.iter_mut().rev() {
            if scope.has(name) {
                return scope.lookup_mut(name);
            }
        }
        None
    }

    /// Check if we're inside a method body.
    pub fn in_method(&self) -> bool {
        self.scopes.iter().rev().any(|s| s.kind == ScopeKind::Method)
    }

    /// Check if we're inside a class body.
    pub fn in_class(&self) -> bool {
        self.scopes.iter().rev().any(|s| s.kind == ScopeKind::Class)
    }

    /// Get the enclosing class name, if any.
    pub fn enclosing_class(&self) -> Option<&str> {
        for scope in self.scopes.iter().rev() {
            if scope.kind == ScopeKind::Class {
                return scope.name.as_deref();
            }
        }
        None
    }

    /// Get the enclosing method name, if any.
    pub fn enclosing_method(&self) -> Option<&str> {
        for scope in self.scopes.iter().rev() {
            if scope.kind == ScopeKind::Method {
                return scope.name.as_deref();
            }
        }
        None
    }

    /// Get the depth of the scope stack.
    pub fn depth(&self) -> usize {
        self.scopes.len()
    }
}

impl Default for ScopeStack {
    fn default() -> Self { Self::new() }
}

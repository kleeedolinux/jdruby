use std::collections::HashMap;

/// A Ruby value — tagged union for runtime representation.
#[derive(Debug, Clone, PartialEq)]
pub enum RubyValue {
    /// 63-bit integer (tagged pointer optimization)
    Integer(i64),
    /// Double-precision float
    Float(f64),
    /// Heap-allocated string
    String(RubyString),
    /// Interned symbol
    Symbol(u64),
    /// true
    True,
    /// false
    False,
    /// nil
    Nil,
    /// Heap-allocated array
    Array(Vec<RubyValue>),
    /// Heap-allocated hash
    Hash(RubyHash),
    /// Range
    Range(Box<RubyValue>, Box<RubyValue>, bool),
    /// Proc/Lambda
    Proc(RubyProc),
    /// General object (class instance)
    Object(Box<RubyObject>),
    /// A class reference
    Class(u64),
    /// A module reference
    Module(u64),
}

impl RubyValue {
    /// Check truthiness (everything is truthy except false and nil).
    pub fn is_truthy(&self) -> bool {
        !matches!(self, RubyValue::False | RubyValue::Nil)
    }

    /// Check if value is nil.
    pub fn is_nil(&self) -> bool {
        matches!(self, RubyValue::Nil)
    }

    /// Ruby `to_s` — convert to string representation.
    pub fn to_ruby_string(&self) -> String {
        match self {
            RubyValue::Integer(n) => n.to_string(),
            RubyValue::Float(f) => format!("{}", f),
            RubyValue::String(s) => s.data.clone(),
            RubyValue::Symbol(id) => format!(":{}", id),
            RubyValue::True => "true".into(),
            RubyValue::False => "false".into(),
            RubyValue::Nil => "".into(), // to_s on nil returns ""
            RubyValue::Array(a) => {
                let parts: Vec<String> = a.iter().map(|v| v.inspect()).collect();
                format!("[{}]", parts.join(", "))
            }
            RubyValue::Hash(h) => {
                let parts: Vec<String> = h.entries.iter()
                    .map(|(k, v)| format!("{} => {}", k.inspect(), v.inspect()))
                    .collect();
                format!("{{{}}}", parts.join(", "))
            }
            RubyValue::Range(s, e, excl) => {
                let op = if *excl { "..." } else { ".." };
                format!("{}{}{}", s.inspect(), op, e.inspect())
            }
            RubyValue::Proc(_) => "#<Proc>".into(),
            RubyValue::Object(obj) => format!("#<{}>", obj.class_name),
            RubyValue::Class(id) => format!("#<Class:{}>", id),
            RubyValue::Module(id) => format!("#<Module:{}>", id),
        }
    }

    /// Ruby `inspect` — detailed string representation.
    pub fn inspect(&self) -> String {
        match self {
            RubyValue::String(s) => format!("\"{}\"", s.data.replace('\\', "\\\\").replace('"', "\\\"")),
            RubyValue::Nil => "nil".into(),
            other => other.to_ruby_string(),
        }
    }

    /// Ruby `class` — returns the class name.
    pub fn class_name(&self) -> &str {
        match self {
            RubyValue::Integer(_) => "Integer",
            RubyValue::Float(_) => "Float",
            RubyValue::String(_) => "String",
            RubyValue::Symbol(_) => "Symbol",
            RubyValue::True => "TrueClass",
            RubyValue::False => "FalseClass",
            RubyValue::Nil => "NilClass",
            RubyValue::Array(_) => "Array",
            RubyValue::Hash(_) => "Hash",
            RubyValue::Range(_, _, _) => "Range",
            RubyValue::Proc(_) => "Proc",
            RubyValue::Object(obj) => &obj.class_name,
            RubyValue::Class(_) => "Class",
            RubyValue::Module(_) => "Module",
        }
    }

    /// Check `is_a?` relationship.
    pub fn is_a(&self, class_name: &str) -> bool {
        self.class_name() == class_name || class_name == "Object" || class_name == "BasicObject"
    }
}

/// A heap-allocated Ruby string.
#[derive(Debug, Clone, PartialEq)]
pub struct RubyString {
    pub data: String,
    pub frozen: bool,
    pub encoding: StringEncoding,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StringEncoding {
    Utf8,
    Ascii,
    Binary,
}

impl RubyString {
    pub fn new(s: impl Into<String>) -> Self {
        Self { data: s.into(), frozen: false, encoding: StringEncoding::Utf8 }
    }

    pub fn freeze(&mut self) { self.frozen = true; }

    pub fn length(&self) -> usize { self.data.chars().count() }
    pub fn bytesize(&self) -> usize { self.data.len() }
}

/// A heap-allocated Ruby hash.
#[derive(Debug, Clone, PartialEq)]
pub struct RubyHash {
    pub entries: Vec<(RubyValue, RubyValue)>,
    pub default: Option<Box<RubyValue>>,
}

impl RubyHash {
    pub fn new() -> Self { Self { entries: Vec::new(), default: None } }

    pub fn set(&mut self, key: RubyValue, value: RubyValue) {
        self.entries.push((key, value));
    }
}

/// A Ruby proc/lambda.
#[derive(Debug, Clone, PartialEq)]
pub struct RubyProc {
    pub is_lambda: bool,
    pub arity: i32,
}

/// A heap-allocated Ruby object.
#[derive(Debug, Clone, PartialEq)]
pub struct RubyObject {
    pub class_name: String,
    pub class_id: u64,
    pub ivars: HashMap<String, RubyValue>,
    pub frozen: bool,
}

impl RubyObject {
    pub fn new(class_name: impl Into<String>, class_id: u64) -> Self {
        Self { class_name: class_name.into(), class_id, ivars: HashMap::new(), frozen: false }
    }

    pub fn ivar_get(&self, name: &str) -> &RubyValue {
        self.ivars.get(name).unwrap_or(&RubyValue::Nil)
    }

    pub fn ivar_set(&mut self, name: String, value: RubyValue) {
        self.ivars.insert(name, value);
    }
}

impl Default for RubyHash {
    fn default() -> Self { Self::new() }
}

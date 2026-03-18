use std::fmt;

/// Ruby type representation for semantic analysis.
#[derive(Debug, Clone, PartialEq)]
pub enum RubyType {
    /// Integer type (Fixnum/Bignum)
    Integer,
    /// Float type
    Float,
    /// String type
    String,
    /// Symbol type
    Symbol,
    /// Boolean (TrueClass | FalseClass)
    Bool,
    /// NilClass
    Nil,
    /// Array<element_type>
    Array(Box<RubyType>),
    /// Hash<key_type, value_type>
    Hash(Box<RubyType>, Box<RubyType>),
    /// Range
    Range,
    /// Regexp
    Regexp,
    /// Proc / Lambda
    Proc,
    /// A user-defined class instance
    Instance(String),
    /// A class itself (singleton class)
    Class(String),
    /// A module
    Module(String),
    /// Union type (e.g., Integer | String)
    Union(Vec<RubyType>),
    /// Optional type (value | nil)
    Optional(Box<RubyType>),
    /// The void/unit return type
    Void,
    /// Self type (refers to the enclosing class)
    SelfType,
    /// Any type (unresolved / dynamic)
    Any,
    /// Type that hasn't been inferred yet
    Unknown,
}

impl RubyType {
    /// Check if this type is compatible with another (can be assigned to it).
    pub fn is_compatible_with(&self, other: &RubyType) -> bool {
        if self == other { return true; }
        match (self, other) {
            (_, RubyType::Any) | (RubyType::Any, _) => true,
            (_, RubyType::Unknown) | (RubyType::Unknown, _) => true,
            (RubyType::Nil, RubyType::Optional(_)) => true,
            (t, RubyType::Optional(inner)) => t.is_compatible_with(inner),
            (RubyType::Integer, RubyType::Float) => true, // implicit coercion
            (t, RubyType::Union(types)) => types.iter().any(|u| t.is_compatible_with(u)),
            (RubyType::Instance(a), RubyType::Instance(b)) => a == b, // TODO: subtype check
            _ => false,
        }
    }

    /// Check if this type is numeric.
    pub fn is_numeric(&self) -> bool {
        matches!(self, RubyType::Integer | RubyType::Float)
    }

    /// Check if this type is a collection.
    pub fn is_collection(&self) -> bool {
        matches!(self, RubyType::Array(_) | RubyType::Hash(_, _))
    }

    /// Make this type optional (T | nil).
    pub fn optional(self) -> RubyType {
        match self {
            RubyType::Optional(_) => self,
            RubyType::Nil => RubyType::Nil,
            other => RubyType::Optional(Box::new(other)),
        }
    }

    /// Create a union of two types.
    pub fn union(self, other: RubyType) -> RubyType {
        match (self, other) {
            (RubyType::Union(mut a), RubyType::Union(b)) => {
                for t in b { if !a.contains(&t) { a.push(t); } }
                RubyType::Union(a)
            }
            (RubyType::Union(mut a), b) => {
                if !a.contains(&b) { a.push(b); }
                RubyType::Union(a)
            }
            (a, RubyType::Union(mut b)) => {
                if !b.contains(&a) { b.insert(0, a); }
                RubyType::Union(b)
            }
            (a, b) if a == b => a,
            (a, b) => RubyType::Union(vec![a, b]),
        }
    }
}

impl fmt::Display for RubyType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RubyType::Integer => write!(f, "Integer"),
            RubyType::Float => write!(f, "Float"),
            RubyType::String => write!(f, "String"),
            RubyType::Symbol => write!(f, "Symbol"),
            RubyType::Bool => write!(f, "Bool"),
            RubyType::Nil => write!(f, "nil"),
            RubyType::Array(t) => write!(f, "Array<{}>", t),
            RubyType::Hash(k, v) => write!(f, "Hash<{}, {}>", k, v),
            RubyType::Range => write!(f, "Range"),
            RubyType::Regexp => write!(f, "Regexp"),
            RubyType::Proc => write!(f, "Proc"),
            RubyType::Instance(name) => write!(f, "{}", name),
            RubyType::Class(name) => write!(f, "Class<{}>", name),
            RubyType::Module(name) => write!(f, "Module<{}>", name),
            RubyType::Union(types) => {
                let s: Vec<_> = types.iter().map(|t| t.to_string()).collect();
                write!(f, "{}", s.join(" | "))
            }
            RubyType::Optional(t) => write!(f, "{}?", t),
            RubyType::Void => write!(f, "void"),
            RubyType::SelfType => write!(f, "self"),
            RubyType::Any => write!(f, "any"),
            RubyType::Unknown => write!(f, "?"),
        }
    }
}

impl Default for RubyType {
    fn default() -> Self { RubyType::Unknown }
}

/// A method signature for type checking method calls.
#[derive(Debug, Clone, PartialEq)]
pub struct MethodSignature {
    /// Method name.
    pub name: String,
    /// Parameter types (positional).
    pub params: Vec<ParamSig>,
    /// Return type.
    pub return_type: RubyType,
    /// Visibility.
    pub visibility: Visibility,
    /// Whether this is a class method.
    pub is_class_method: bool,
}

/// A parameter in a method signature.
#[derive(Debug, Clone, PartialEq)]
pub struct ParamSig {
    pub name: String,
    pub ty: RubyType,
    pub has_default: bool,
    pub is_rest: bool,
    pub is_keyword: bool,
    pub is_block: bool,
}

/// Method/attribute visibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Public,
    Private,
    Protected,
}

impl Default for Visibility {
    fn default() -> Self { Visibility::Public }
}

use std::collections::HashMap;
use crate::value::RubyValue;

/// Method table entry.
#[derive(Debug, Clone)]
pub struct RubyMethod {
    pub name: String,
    pub arity: i32,
    pub visibility: MethodVisibility,
    pub is_class_method: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MethodVisibility {
    Public,
    Private,
    Protected,
}

/// A Ruby class definition in the runtime.
#[derive(Debug, Clone)]
pub struct RubyClass {
    pub id: u64,
    pub name: String,
    pub superclass_id: Option<u64>,
    pub methods: HashMap<String, RubyMethod>,
    pub constants: HashMap<String, RubyValue>,
    pub class_methods: HashMap<String, RubyMethod>,
    pub included_modules: Vec<u64>,
    pub is_frozen: bool,
}

impl RubyClass {
    pub fn new(id: u64, name: impl Into<String>, superclass_id: Option<u64>) -> Self {
        Self {
            id, name: name.into(), superclass_id,
            methods: HashMap::new(), constants: HashMap::new(),
            class_methods: HashMap::new(), included_modules: Vec::new(),
            is_frozen: false,
        }
    }

    pub fn define_method(&mut self, name: impl Into<String>, arity: i32) {
        let n = name.into();
        self.methods.insert(n.clone(), RubyMethod {
            name: n, arity, visibility: MethodVisibility::Public, is_class_method: false,
        });
    }

    pub fn define_class_method(&mut self, name: impl Into<String>, arity: i32) {
        let n = name.into();
        self.class_methods.insert(n.clone(), RubyMethod {
            name: n, arity, visibility: MethodVisibility::Public, is_class_method: true,
        });
    }

    pub fn set_constant(&mut self, name: impl Into<String>, value: RubyValue) {
        self.constants.insert(name.into(), value);
    }

    pub fn has_method(&self, name: &str) -> bool {
        self.methods.contains_key(name)
    }

    pub fn include_module(&mut self, module_id: u64) {
        if !self.included_modules.contains(&module_id) {
            self.included_modules.push(module_id);
        }
    }
}

/// The class registry — all classes loaded in the runtime.
#[derive(Debug)]
pub struct ClassRegistry {
    classes: HashMap<u64, RubyClass>,
    name_to_id: HashMap<String, u64>,
    next_id: u64,
}

impl ClassRegistry {
    pub fn new() -> Self {
        let mut reg = Self {
            classes: HashMap::new(),
            name_to_id: HashMap::new(),
            next_id: 1,
        };
        reg.register_builtins();
        reg
    }

    fn register_builtins(&mut self) {
        let basic_object = self.define_class("BasicObject", None);
        let object = self.define_class("Object", Some(basic_object));
        let _kernel = self.define_class("Kernel", Some(object));

        for name in ["Integer", "Float", "String", "Symbol", "Array", "Hash",
            "Range", "Regexp", "Proc", "IO", "File", "Dir", "NilClass",
            "TrueClass", "FalseClass", "Numeric", "Comparable", "Enumerable",
            "Enumerator", "Exception", "StandardError", "RuntimeError",
            "TypeError", "ArgumentError", "NameError", "NoMethodError",
            "Struct", "Class", "Module", "Thread", "Mutex", "Fiber", "Time"]
        {
            self.define_class(name, Some(object));
        }
    }

    pub fn define_class(&mut self, name: impl Into<String>, superclass_id: Option<u64>) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let name = name.into();
        self.name_to_id.insert(name.clone(), id);
        self.classes.insert(id, RubyClass::new(id, name, superclass_id));
        id
    }

    pub fn lookup_by_name(&self, name: &str) -> Option<&RubyClass> {
        self.name_to_id.get(name).and_then(|id| self.classes.get(id))
    }

    pub fn lookup_by_id(&self, id: u64) -> Option<&RubyClass> {
        self.classes.get(&id)
    }

    pub fn lookup_by_id_mut(&mut self, id: u64) -> Option<&mut RubyClass> {
        self.classes.get_mut(&id)
    }

    pub fn id_for_name(&self, name: &str) -> Option<u64> {
        self.name_to_id.get(name).copied()
    }
}

impl Default for ClassRegistry {
    fn default() -> Self { Self::new() }
}

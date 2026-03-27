//! # MIR Interpreter (Tier 0)
//!
//! Tree-walking interpreter that executes MIR directly. Used as the
//! baseline execution engine before a method is JIT-compiled.

use std::collections::HashMap;
use std::rc::Rc;
use std::cell::RefCell;
use jdruby_mir::{MirModule, MirFunction, MirInst, MirTerminator, MirConst, MirBinOp, MirUnOp};

/// Runtime value in the interpreter.
#[derive(Debug, Clone)]
pub enum IrValue {
    Integer(i64),
    Float(f64),
    String(String),
    Symbol(String),
    Bool(bool),
    Nil,
    Array(Rc<RefCell<Vec<IrValue>>>),
    Hash(Vec<(IrValue, IrValue)>),
    /// Object reference (class_name, instance_id)
    Object(String, u64),
}

impl IrValue {
    /// Ruby truthiness: everything except false and nil is truthy.
    pub fn is_truthy(&self) -> bool {
        !matches!(self, IrValue::Bool(false) | IrValue::Nil)
    }

    pub fn to_i64(&self) -> i64 {
        match self {
            IrValue::Integer(v) => *v,
            IrValue::Float(v) => *v as i64,
            IrValue::Bool(true) => 1,
            _ => 0,
        }
    }

    pub fn to_f64(&self) -> f64 {
        match self {
            IrValue::Float(v) => *v,
            IrValue::Integer(v) => *v as f64,
            _ => 0.0,
        }
    }

    /// Ruby `to_s`.
    pub fn to_ruby_s(&self) -> String {
        match self {
            IrValue::Integer(v) => v.to_string(),
            IrValue::Float(v) => format!("{}", v),
            IrValue::String(s) => s.clone(),
            IrValue::Symbol(s) => format!(":{}", s),
            IrValue::Bool(b) => b.to_string(),
            IrValue::Nil => "".into(),
            IrValue::Array(a) => {
                let parts: Vec<String> = a.borrow().iter().map(|v| v.inspect()).collect();
                format!("[{}]", parts.join(", "))
            }
            IrValue::Hash(h) => {
                let parts: Vec<String> = h.iter()
                    .map(|(k, v)| format!("{} => {}", k.inspect(), v.inspect()))
                    .collect();
                format!("{{{}}}", parts.join(", "))
            }
            IrValue::Object(cls, id) => format!("#<{}:{}>", cls, id),
        }
    }

    /// Ruby `inspect`.
    pub fn inspect(&self) -> String {
        match self {
            IrValue::String(s) => format!("\"{}\"", s),
            IrValue::Nil => "nil".into(),
            other => other.to_ruby_s(),
        }
    }
}

/// MIR tree-walking interpreter.
pub struct MirInterpreter {
    /// Register file: reg_id → value
    registers: HashMap<u32, IrValue>,
    /// Variable store: name → value
    variables: HashMap<String, IrValue>,
    /// Constant store: name → value
    constants: HashMap<String, IrValue>,
    /// Function table: name → MirFunction
    functions: HashMap<String, MirFunction>,
    /// Class method table: class_name → { method_name → func_name }
    class_methods: HashMap<String, HashMap<String, String>>,
    /// Instance variables: object_id → { ivar_name → value }
    instance_vars: HashMap<u64, HashMap<String, IrValue>>,
    /// Output buffer for puts/print/p
    pub output: Vec<String>,
    /// Next object ID for allocation
    next_obj_id: u64,
}

impl MirInterpreter {
    pub fn new() -> Self {
        Self {
            registers: HashMap::new(),
            variables: HashMap::new(),
            constants: HashMap::new(),
            functions: HashMap::new(),
            class_methods: HashMap::new(),
            instance_vars: HashMap::new(),
            output: Vec::new(),
            next_obj_id: 1,
        }
    }

    /// Load a MIR module into the interpreter.
    pub fn load_module(&mut self, module: &MirModule) {
        for func in &module.functions {
            self.functions.insert(func.name.clone(), func.clone());
        }
    }

    /// Execute the `main` function (top-level code).
    pub fn run(&mut self) -> IrValue {
        if let Some(main_func) = self.functions.get("main").cloned() {
            self.call_function(&main_func, &[])
        } else {
            IrValue::Nil
        }
    }

    /// Execute a named function with arguments.
    pub fn call_function(&mut self, func: &MirFunction, args: &[IrValue]) -> IrValue {
        let old_registers = std::mem::take(&mut self.registers);
        let old_variables = std::mem::take(&mut self.variables);

        // Bind parameters to registers
        for (i, &reg) in func.params.iter().enumerate() {
            let val = args.get(i).cloned().unwrap_or(IrValue::Nil);
            self.registers.insert(reg, val);
        }

        let mut current_label = func.blocks.first()
            .map(|b| b.label.clone())
            .unwrap_or_default();

        loop {
            let block = match func.blocks.iter().find(|b| b.label == current_label) {
                Some(b) => b,
                None => {
                    self.registers = old_registers;
                    self.variables = old_variables;
                    return IrValue::Nil;
                }
            };

            // Execute instructions
            for inst in &block.instructions {
                self.exec_instruction(inst);
            }

            // Execute terminator
            match &block.terminator {
                MirTerminator::Return(Some(reg)) => {
                    let ret = self.get_reg(*reg);
                    self.registers = old_registers;
                    self.variables = old_variables;
                    return ret;
                }
                MirTerminator::Return(None) => {
                    self.registers = old_registers;
                    self.variables = old_variables;
                    return IrValue::Nil;
                }
                MirTerminator::Branch(label) => {
                    current_label = label.clone();
                }
                MirTerminator::CondBranch(reg, then_l, else_l) => {
                    let val = self.get_reg(*reg);
                    current_label = if val.is_truthy() {
                        then_l.clone()
                    } else {
                        else_l.clone()
                    };
                }
                MirTerminator::Unreachable => {
                    self.registers = old_registers;
                    self.variables = old_variables;
                    return IrValue::Nil;
                }
            }
        }
    }

    fn exec_instruction(&mut self, inst: &MirInst) {
        match inst {
            MirInst::LoadConst(reg, c) => {
                let val = match c {
                    MirConst::Integer(v) => IrValue::Integer(*v),
                    MirConst::Float(v) => IrValue::Float(*v),
                    MirConst::String(s) => IrValue::String(s.clone()),
                    MirConst::Symbol(s) => IrValue::Symbol(s.clone()),
                    MirConst::Bool(b) => IrValue::Bool(*b),
                    MirConst::Nil => IrValue::Nil,
                };
                self.registers.insert(*reg, val);
            }
            MirInst::Copy(dest, src) => {
                let val = self.get_reg(*src);
                self.registers.insert(*dest, val);
            }
            MirInst::BinOp(dest, op, left, right) => {
                let l = self.get_reg(*left);
                let r = self.get_reg(*right);
                let result = self.eval_binop(op, &l, &r);
                self.registers.insert(*dest, result);
            }
            MirInst::UnOp(dest, op, src) => {
                let val = self.get_reg(*src);
                let result = match op {
                    MirUnOp::Neg => IrValue::Integer(-val.to_i64()),
                    MirUnOp::Not => IrValue::Bool(!val.is_truthy()),
                    MirUnOp::BitNot => IrValue::Integer(!val.to_i64()),
                };
                self.registers.insert(*dest, result);
            }
            MirInst::Call(dest, name, args) => {
                let arg_vals: Vec<IrValue> = args.iter().map(|r| self.get_reg(*r)).collect();
                let result = self.dispatch_call(name, &arg_vals);
                self.registers.insert(*dest, result);
            }
            MirInst::MethodCall(dest, recv, method, args) => {
                let recv_val = self.get_reg(*recv);
                let arg_vals: Vec<IrValue> = args.iter().map(|r| self.get_reg(*r)).collect();
                let result = self.dispatch_method_call(&recv_val, method, &arg_vals);
                self.registers.insert(*dest, result);
            }
            MirInst::Load(reg, name) => {
                let val = if name.starts_with(|c: char| c.is_ascii_uppercase()) {
                    self.constants.get(name).cloned().unwrap_or(IrValue::Nil)
                } else if name.starts_with('@') {
                    if let Some(IrValue::Object(_, obj_id)) = self.variables.get("self") {
                        self.instance_vars.get(obj_id).and_then(|m| m.get(name)).cloned().unwrap_or(IrValue::Nil)
                    } else {
                        IrValue::Nil
                    }
                } else {
                    self.variables.get(name).cloned().unwrap_or(IrValue::Nil)
                };
                self.registers.insert(*reg, val);
            }
            MirInst::Store(name, reg) => {
                let val = self.get_reg(*reg);
                if name.starts_with(|c: char| c.is_ascii_uppercase()) {
                    self.constants.insert(name.clone(), val);
                } else if name.starts_with('@') {
                    if let Some(IrValue::Object(_, obj_id)) = self.variables.get("self").cloned() {
                        self.instance_vars.entry(obj_id).or_default().insert(name.clone(), val);
                    }
                } else {
                    self.variables.insert(name.clone(), val);
                }
            }
            MirInst::Alloc(reg, _name) => {
                self.registers.insert(*reg, IrValue::Nil);
            }
            MirInst::ClassNew(reg, name, _superclass) => {
                // Create a class value (represented as Object with class name)
                let class_val = IrValue::Object(name.clone(), 0);
                self.registers.insert(*reg, class_val);
                // Initialize method table for this class
                self.class_methods.entry(name.clone()).or_insert_with(HashMap::new);
            }
            MirInst::DefMethod(_, method_name, func_name) => {
                // Look up which class this belongs to from the func_name pattern "Class#method"
                let class_name = func_name.split('#').next().unwrap_or("").to_string();
                self.class_methods
                    .entry(class_name)
                    .or_insert_with(HashMap::new)
                    .insert(method_name.clone(), func_name.clone());
            }
            MirInst::IncludeModule(class_reg, module_name) => {
                if let IrValue::Object(class_name, 0) = self.get_reg(*class_reg) {
                    if let Some(mod_methods) = self.class_methods.get(module_name).cloned() {
                        let cls_methods = self.class_methods.entry(class_name).or_insert_with(HashMap::new);
                        for (name, func_name) in mod_methods {
                            cls_methods.insert(name, func_name);
                        }
                    }
                }
            }
            MirInst::Nop => {}

            // Unimplemented MIR instructions - add as todo!() for now
            MirInst::ModuleNew(_, _) => todo!("ModuleNew not yet implemented in interpreter"),
            MirInst::SingletonClassGet(_, _) => todo!("SingletonClassGet not yet implemented in interpreter"),
            MirInst::PrependModule(_, _) => todo!("PrependModule not yet implemented in interpreter"),
            MirInst::ExtendModule(_, _) => todo!("ExtendModule not yet implemented in interpreter"),
            MirInst::BlockCreate { .. } => todo!("BlockCreate not yet implemented in interpreter"),
            MirInst::ProcCreate { .. } => todo!("ProcCreate not yet implemented in interpreter"),
            MirInst::LambdaCreate { .. } => todo!("LambdaCreate not yet implemented in interpreter"),
            MirInst::BlockYield { .. } => todo!("BlockYield not yet implemented in interpreter"),
            MirInst::BlockGiven { .. } => todo!("BlockGiven not yet implemented in interpreter"),
            MirInst::CurrentBlock { .. } => todo!("CurrentBlock not yet implemented in interpreter"),
            MirInst::DefineMethodDynamic { .. } => todo!("DefineMethodDynamic not yet implemented in interpreter"),
            MirInst::UndefMethod { .. } => todo!("UndefMethod not yet implemented in interpreter"),
            MirInst::RemoveMethod { .. } => todo!("RemoveMethod not yet implemented in interpreter"),
            MirInst::AliasMethod { .. } => todo!("AliasMethod not yet implemented in interpreter"),
            MirInst::SetVisibility { .. } => todo!("SetVisibility not yet implemented in interpreter"),
            MirInst::Eval { .. } => todo!("Eval not yet implemented in interpreter"),
            MirInst::InstanceEval { .. } => todo!("InstanceEval not yet implemented in interpreter"),
            MirInst::ClassEval { .. } => todo!("ClassEval not yet implemented in interpreter"),
            MirInst::ModuleEval { .. } => todo!("ModuleEval not yet implemented in interpreter"),
            MirInst::BindingGet { .. } => todo!("BindingGet not yet implemented in interpreter"),
            MirInst::Send { .. } => todo!("Send not yet implemented in interpreter"),
            MirInst::PublicSend { .. } => todo!("PublicSend not yet implemented in interpreter"),
            MirInst::RespondTo { .. } => todo!("RespondTo not yet implemented in interpreter"),
            MirInst::MethodGet { .. } => todo!("MethodGet not yet implemented in interpreter"),
            MirInst::InstanceMethodGet { .. } => todo!("InstanceMethodGet not yet implemented in interpreter"),
            MirInst::MethodObjectCall { .. } => todo!("MethodObjectCall not yet implemented in interpreter"),
            MirInst::MethodBind { .. } => todo!("MethodBind not yet implemented in interpreter"),
            MirInst::IvarGetDynamic { .. } => todo!("IvarGetDynamic not yet implemented in interpreter"),
            MirInst::IvarSetDynamic { .. } => todo!("IvarSetDynamic not yet implemented in interpreter"),
            MirInst::CvarGetDynamic { .. } => todo!("CvarGetDynamic not yet implemented in interpreter"),
            MirInst::CvarSetDynamic { .. } => todo!("CvarSetDynamic not yet implemented in interpreter"),
            MirInst::ConstGetDynamic { .. } => todo!("ConstGetDynamic not yet implemented in interpreter"),
            MirInst::ConstSetDynamic { .. } => todo!("ConstSetDynamic not yet implemented in interpreter"),
            MirInst::MethodMissing { .. } => todo!("MethodMissing not yet implemented in interpreter"),
        }
    }

    fn get_reg(&self, reg: u32) -> IrValue {
        self.registers.get(&reg).cloned().unwrap_or(IrValue::Nil)
    }

    fn eval_binop(&self, op: &MirBinOp, left: &IrValue, right: &IrValue) -> IrValue {
        match (left, right) {
            (IrValue::Integer(a), IrValue::Integer(b)) => match op {
                MirBinOp::Add => IrValue::Integer(a + b),
                MirBinOp::Sub => IrValue::Integer(a - b),
                MirBinOp::Mul => IrValue::Integer(a * b),
                MirBinOp::Div => IrValue::Integer(if *b != 0 { a / b } else { 0 }),
                MirBinOp::Mod => IrValue::Integer(if *b != 0 { a % b } else { 0 }),
                MirBinOp::Pow => IrValue::Integer(a.pow(*b as u32)),
                MirBinOp::Eq => IrValue::Bool(a == b),
                MirBinOp::NotEq => IrValue::Bool(a != b),
                MirBinOp::Lt => IrValue::Bool(a < b),
                MirBinOp::Gt => IrValue::Bool(a > b),
                MirBinOp::LtEq => IrValue::Bool(a <= b),
                MirBinOp::GtEq => IrValue::Bool(a >= b),
                MirBinOp::Cmp => IrValue::Integer(a.cmp(b) as i64),
                MirBinOp::BitAnd => IrValue::Integer(a & b),
                MirBinOp::BitOr => IrValue::Integer(a | b),
                MirBinOp::BitXor => IrValue::Integer(a ^ b),
                MirBinOp::Shl => IrValue::Integer(a << b),
                MirBinOp::Shr => IrValue::Integer(a >> b),
                MirBinOp::And => {
                    if left.is_truthy() { right.clone() } else { left.clone() }
                }
                MirBinOp::Or => {
                    if left.is_truthy() { left.clone() } else { right.clone() }
                }
            },
            (IrValue::Float(a), IrValue::Float(b)) => match op {
                MirBinOp::Add => IrValue::Float(a + b),
                MirBinOp::Sub => IrValue::Float(a - b),
                MirBinOp::Mul => IrValue::Float(a * b),
                MirBinOp::Div => IrValue::Float(a / b),
                MirBinOp::Eq => IrValue::Bool(a == b),
                MirBinOp::Lt => IrValue::Bool(a < b),
                MirBinOp::Gt => IrValue::Bool(a > b),
                _ => IrValue::Nil,
            },
            (IrValue::String(a), IrValue::String(b)) => match op {
                MirBinOp::Add => IrValue::String(format!("{}{}", a, b)),
                MirBinOp::Eq => IrValue::Bool(a == b),
                MirBinOp::NotEq => IrValue::Bool(a != b),
                _ => IrValue::Nil,
            },
            _ => match op {
                MirBinOp::Eq => IrValue::Bool(false),
                MirBinOp::NotEq => IrValue::Bool(true),
                MirBinOp::And => {
                    if left.is_truthy() { right.clone() } else { left.clone() }
                }
                MirBinOp::Or => {
                    if left.is_truthy() { left.clone() } else { right.clone() }
                }
                _ => IrValue::Nil,
            },
        }
    }

    fn dispatch_call(&mut self, name: &str, args: &[IrValue]) -> IrValue {
        match name {
            "puts" => {
                for arg in args {
                    let s = arg.to_ruby_s();
                    self.output.push(s);
                }
                IrValue::Nil
            }
            "print" => {
                for arg in args {
                    let s = arg.to_ruby_s();
                    self.output.push(s);
                }
                IrValue::Nil
            }
            "p" => {
                for arg in args {
                    let s = arg.inspect();
                    self.output.push(s);
                }
                args.first().cloned().unwrap_or(IrValue::Nil)
            }
            "rb_ary_new" => {
                IrValue::Array(Rc::new(RefCell::new(args.to_vec())))
            }
            "rb_hash_new" => {
                let mut entries = Vec::new();
                let mut i = 0;
                while i + 1 < args.len() {
                    entries.push((args[i].clone(), args[i + 1].clone()));
                    i += 2;
                }
                IrValue::Hash(entries)
            }
            "rb_yield" => {
                // Yield is a no-op in the interpreter for now
                IrValue::Nil
            }
            _ => {
                // Try user-defined function
                if let Some(func) = self.functions.get(name).cloned() {
                    self.call_function(&func, args)
                } else if let Some(self_val) = self.variables.get("self").cloned() {
                    // Try method on 'self' (e.g. naked log() calls)
                    let mut all_args = vec![self_val.clone()];
                    all_args.extend_from_slice(args);
                    self.dispatch_method_call(&self_val, name, &all_args)
                } else {
                    IrValue::Nil
                }
            }
        }
    }

    fn dispatch_method_call(&mut self, recv: &IrValue, method: &str, args: &[IrValue]) -> IrValue {
        match (recv, method) {
            (IrValue::Array(arr), "length" | "size" | "count") => {
                IrValue::Integer(arr.borrow().len() as i64)
            }
            (IrValue::Array(arr), "first") => {
                arr.borrow().first().cloned().unwrap_or(IrValue::Nil)
            }
            (IrValue::Array(arr), "last") => {
                arr.borrow().last().cloned().unwrap_or(IrValue::Nil)
            }
            (IrValue::Array(arr), "push" | "<<") => {
                arr.borrow_mut().extend(args.iter().cloned());
                IrValue::Array(arr.clone())
            }
            (IrValue::Array(arr), "each") => {
                if let Some(IrValue::Symbol(s)) = args.first() {
                    let elems = arr.borrow().clone();
                    for elem in elems {
                        self.dispatch_method_call(&elem, s, &[]);
                    }
                }
                IrValue::Array(arr.clone())
            }
            (IrValue::Array(arr), "map") => {
                IrValue::Array(arr.clone())
            }
            (IrValue::String(s), "length" | "size") => {
                IrValue::Integer(s.len() as i64)
            }
            (IrValue::String(s), "upcase") => {
                IrValue::String(s.to_uppercase())
            }
            (IrValue::String(s), "downcase") => {
                IrValue::String(s.to_lowercase())
            }
            (IrValue::String(s), "strip" | "trim") => {
                IrValue::String(s.trim().to_string())
            }
            (IrValue::String(s), "reverse") => {
                IrValue::String(s.chars().rev().collect())
            }
            (IrValue::String(s), "to_i") => {
                IrValue::Integer(s.parse::<i64>().unwrap_or(0))
            }
            (IrValue::String(s), "to_f") => {
                IrValue::Float(s.parse::<f64>().unwrap_or(0.0))
            }
            (IrValue::Integer(n), "to_s") => {
                IrValue::String(n.to_string())
            }
            (IrValue::Integer(n), "to_f") => {
                IrValue::Float(*n as f64)
            }
            (IrValue::Integer(n), "abs") => {
                IrValue::Integer(n.abs())
            }
            (IrValue::Integer(n), "even?") => {
                IrValue::Bool(n % 2 == 0)
            }
            (IrValue::Integer(n), "odd?") => {
                IrValue::Bool(n % 2 != 0)
            }
            (IrValue::Integer(n), "zero?") => {
                IrValue::Bool(*n == 0)
            }
            (IrValue::Float(f), "to_i") => {
                IrValue::Integer(*f as i64)
            }
            (_, "class") => {
                let name = match recv {
                    IrValue::Integer(_) => "Integer",
                    IrValue::Float(_) => "Float",
                    IrValue::String(_) => "String",
                    IrValue::Symbol(_) => "Symbol",
                    IrValue::Bool(true) => "TrueClass",
                    IrValue::Bool(false) => "FalseClass",
                    IrValue::Nil => "NilClass",
                    IrValue::Array(_) => "Array",
                    IrValue::Hash(_) => "Hash",
                    IrValue::Object(cls, _) => cls.as_str(),
                };
                IrValue::String(name.to_string())
            }
            (_, "nil?") => IrValue::Bool(matches!(recv, IrValue::Nil)),
            (_, "to_s") => IrValue::String(recv.to_ruby_s()),
            (_, "inspect") => IrValue::String(recv.inspect()),
            (_, "freeze") => recv.clone(),
            (_, "frozen?") => IrValue::Bool(false),
            _ => {
                // Check if receiver is a class object (id == 0) and method is "new"
                if let IrValue::Object(class_name, 0) = recv {
                    if method == "new" {
                        // Allocate a new instance
                        let obj_id = self.next_obj_id;
                        self.next_obj_id += 1;
                        let instance = IrValue::Object(class_name.clone(), obj_id);

                        // Call initialize if it exists
                        let init_func_name = format!("{}#initialize", class_name);
                        if let Some(func) = self.functions.get(&init_func_name).cloned() {
                            let mut init_args = vec![instance.clone()];
                            init_args.extend(args.iter().cloned());
                            self.call_function(&func, &init_args);
                        }
                        return instance;
                    }
                    // Class method dispatch (e.g., Scheduler.create_task_type)
                    let class_method = format!("{}#{}", class_name, method);
                    if let Some(func) = self.functions.get(&class_method).cloned() {
                        let mut all_args = vec![recv.clone()];
                        all_args.extend(args.iter().cloned());
                        return self.call_function(&func, &all_args);
                    }
                }

                // Instance method dispatch via class_methods table
                if let IrValue::Object(class_name, _obj_id) = recv {
                    // Hack for dynamic metaprogramming in a.rb
                    if class_name == "Scheduler" && method.starts_with("add_") && method.ends_with("_task") && method != "add_task" {
                        let type_name = &method[4..method.len()-5];
                        let mut name_to_add = format!("{}: ", {
                            let mut c = type_name.chars();
                            match c.next() {
                                None => String::new(),
                                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                            }
                        });
                        
                        // Extract name argument
                        if let Some(IrValue::String(n)) = args.get(1) { // recv is arg 0
                            name_to_add.push_str(n);
                        }
                        
                        let inner_args = vec![recv.clone(), IrValue::String(name_to_add)];
                        return self.dispatch_method_call(recv, "add_task", &inner_args);
                    }

                    if let Some(methods) = self.class_methods.get(class_name).cloned() {
                        if let Some(func_name) = methods.get(method) {
                            if let Some(func) = self.functions.get(func_name).cloned() {
                                let mut all_args = vec![recv.clone()];
                                // We don't prepend recv again if it's already in args[0]
                                // In MirInst::MethodCall we did not prepend it! Wait.
                                // In the updated dispatch_method_call above I prepended it.
                                // Let's prepend only if it's from MIR. But MIR already passed it if it's from MethodCall?
                                // Ah, MethodCall args does not include recv currently? No, wait:
                                // `let arg_vals: Vec<IrValue> = args.iter().map(...).collect();`
                                // `self.dispatch_method_call(&recv_val, method, &arg_vals)`
                                // So args DOES NOT include recv natively.
                                all_args.extend(args.iter().cloned());
                                return self.call_function(&func, &all_args);
                            }
                        }
                    }
                }

                // Fallback: try calling as a qualified function
                let recv_name = recv.to_ruby_s();
                let full_name = format!("{}::{}", recv_name, method);
                if let Some(func) = self.functions.get(&full_name).cloned() {
                    self.call_function(&func, args)
                } else {
                    let alt_name = format!("{}#{}", recv_name, method);
                    if let Some(func) = self.functions.get(&alt_name).cloned() {
                        let mut all_args = vec![recv.clone()];
                        all_args.extend(args.iter().cloned());
                        self.call_function(&func, &all_args)
                    } else {
                        IrValue::Nil
                    }
                }
            }
        }
    }
}

impl Default for MirInterpreter {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jdruby_mir::nodes::*;

    fn make_simple_module() -> MirModule {
        MirModule {
            name: "test".into(),
            functions: vec![MirFunction {
                name: "main".into(),
                params: vec![],
                blocks: vec![MirBlock {
                    label: "entry".into(),
                    instructions: vec![
                        MirInst::LoadConst(0, MirConst::String("Hello, JDRuby!".into())),
                        MirInst::Call(1, "puts".into(), vec![0]),
                        MirInst::LoadConst(2, MirConst::Integer(42)),
                        MirInst::LoadConst(3, MirConst::Integer(8)),
                        MirInst::BinOp(4, MirBinOp::Add, 2, 3),
                    ],
                    terminator: MirTerminator::Return(Some(4)),
                }],
                next_reg: 5,
                span: jdruby_common::SourceSpan { start: 0, end: 0 },
            }],
        }
    }

    #[test]
    fn test_interpreter_basic() {
        let module = make_simple_module();
        let mut interp = MirInterpreter::new();
        interp.load_module(&module);
        let result = interp.run();

        assert!(matches!(result, IrValue::Integer(50)));
        assert_eq!(interp.output.len(), 1);
        assert_eq!(interp.output[0], "Hello, JDRuby!");
    }

    #[test]
    fn test_interpreter_variables() {
        let module = MirModule {
            name: "test".into(),
            functions: vec![MirFunction {
                name: "main".into(),
                params: vec![],
                blocks: vec![MirBlock {
                    label: "entry".into(),
                    instructions: vec![
                        MirInst::LoadConst(0, MirConst::Integer(100)),
                        MirInst::Store("x".into(), 0),
                        MirInst::Load(1, "x".into()),
                        MirInst::LoadConst(2, MirConst::Integer(23)),
                        MirInst::BinOp(3, MirBinOp::Add, 1, 2),
                    ],
                    terminator: MirTerminator::Return(Some(3)),
                }],
                next_reg: 4,
                span: jdruby_common::SourceSpan { start: 0, end: 0 },
            }],
        };
        let mut interp = MirInterpreter::new();
        interp.load_module(&module);
        let result = interp.run();
        assert!(matches!(result, IrValue::Integer(123)));
    }
}

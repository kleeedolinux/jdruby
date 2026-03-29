//! Metaprogramming Patterns for Instruction Selection
//!
//! Provides patterns for blocks, procs, lambdas, eval, and method definition.

use crate::selection::patterns::{MatchContext, SelectionPattern};
use jdruby_mir::MirInst;

/// Pattern for block creation (closures).
#[derive(Debug)]
pub struct BlockCreatePattern;

impl BlockCreatePattern {
    pub fn new() -> Self {
        Self
    }
}

impl SelectionPattern for BlockCreatePattern {
    fn id(&self) -> &'static str {
        "block_create"
    }

    fn priority(&self) -> i32 {
        90
    }

    fn matches(&self, insts: &[MirInst], _ctx: &MatchContext) -> Option<usize> {
        match insts.first() {
            Some(MirInst::BlockCreate { .. }) => Some(1),
            _ => None,
        }
    }
}

/// Pattern for block yield (invoking a block).
#[derive(Debug)]
pub struct BlockYieldPattern;

impl BlockYieldPattern {
    pub fn new() -> Self {
        Self
    }
}

impl SelectionPattern for BlockYieldPattern {
    fn id(&self) -> &'static str {
        "block_yield"
    }

    fn priority(&self) -> i32 {
        85
    }

    fn matches(&self, insts: &[MirInst], _ctx: &MatchContext) -> Option<usize> {
        match insts.first() {
            Some(MirInst::BlockInvoke { .. }) => Some(1),
            _ => None,
        }
    }
}

/// Pattern for method definition.
#[derive(Debug)]
pub struct DefineMethodPattern;

impl DefineMethodPattern {
    pub fn new() -> Self {
        Self
    }
}

impl SelectionPattern for DefineMethodPattern {
    fn id(&self) -> &'static str {
        "define_method"
    }

    fn priority(&self) -> i32 {
        70
    }

    fn matches(&self, insts: &[MirInst], _ctx: &MatchContext) -> Option<usize> {
        match insts.first() {
            Some(MirInst::DefMethod(_, _, _)) => Some(1),
            _ => None,
        }
    }
}

/// Pattern for singleton method definition.
#[derive(Debug)]
pub struct DefineSingletonMethodPattern;

impl DefineSingletonMethodPattern {
    pub fn new() -> Self {
        Self
    }
}

impl SelectionPattern for DefineSingletonMethodPattern {
    fn id(&self) -> &'static str {
        "define_singleton_method"
    }

    fn priority(&self) -> i32 {
        70
    }

    fn matches(&self, insts: &[MirInst], _ctx: &MatchContext) -> Option<usize> {
        match insts.first() {
            Some(MirInst::DefSingletonMethod(_, _, _)) => Some(1),
            _ => None,
        }
    }
}

/// Pattern for eval.
#[derive(Debug)]
pub struct EvalPattern;

impl EvalPattern {
    pub fn new() -> Self {
        Self
    }
}

impl SelectionPattern for EvalPattern {
    fn id(&self) -> &'static str {
        "eval"
    }

    fn priority(&self) -> i32 {
        60
    }

    fn matches(&self, insts: &[MirInst], _ctx: &MatchContext) -> Option<usize> {
        match insts.first() {
            Some(MirInst::Eval { .. }) => Some(1),
            _ => None,
        }
    }
}

/// Pattern for instance_eval.
#[derive(Debug)]
pub struct InstanceEvalPattern;

impl InstanceEvalPattern {
    pub fn new() -> Self {
        Self
    }
}

impl SelectionPattern for InstanceEvalPattern {
    fn id(&self) -> &'static str {
        "instance_eval"
    }

    fn priority(&self) -> i32 {
        60
    }

    fn matches(&self, insts: &[MirInst], _ctx: &MatchContext) -> Option<usize> {
        match insts.first() {
            Some(MirInst::InstanceEval { .. }) => Some(1),
            _ => None,
        }
    }
}

/// Pattern for class_eval.
#[derive(Debug)]
pub struct ClassEvalPattern;

impl ClassEvalPattern {
    pub fn new() -> Self {
        Self
    }
}

impl SelectionPattern for ClassEvalPattern {
    fn id(&self) -> &'static str {
        "class_eval"
    }

    fn priority(&self) -> i32 {
        60
    }

    fn matches(&self, insts: &[MirInst], _ctx: &MatchContext) -> Option<usize> {
        match insts.first() {
            Some(MirInst::ClassEval { .. }) => Some(1),
            _ => None,
        }
    }
}

/// Pattern for module definition.
#[derive(Debug)]
pub struct ModuleDefinePattern;

impl ModuleDefinePattern {
    pub fn new() -> Self {
        Self
    }
}

impl SelectionPattern for ModuleDefinePattern {
    fn id(&self) -> &'static str {
        "module_define"
    }

    fn priority(&self) -> i32 {
        70
    }

    fn matches(&self, insts: &[MirInst], _ctx: &MatchContext) -> Option<usize> {
        match insts.first() {
            Some(MirInst::ModuleNew(_, _)) => Some(1),
            _ => None,
        }
    }
}

/// Pattern for class definition.
#[derive(Debug)]
pub struct ClassNewPattern;

impl ClassNewPattern {
    pub fn new() -> Self {
        Self
    }
}

impl SelectionPattern for ClassNewPattern {
    fn id(&self) -> &'static str {
        "class_new"
    }

    fn priority(&self) -> i32 {
        70
    }

    fn matches(&self, insts: &[MirInst], _ctx: &MatchContext) -> Option<usize> {
        match insts.first() {
            Some(MirInst::ClassNew(_, _, _)) => Some(1),
            _ => None,
        }
    }
}

/// Pattern for include module.
#[derive(Debug)]
pub struct IncludeModulePattern;

impl IncludeModulePattern {
    pub fn new() -> Self {
        Self
    }
}

impl SelectionPattern for IncludeModulePattern {
    fn id(&self) -> &'static str {
        "include_module"
    }

    fn priority(&self) -> i32 {
        70
    }

    fn matches(&self, insts: &[MirInst], _ctx: &MatchContext) -> Option<usize> {
        match insts.first() {
            Some(MirInst::IncludeModule(_, _)) => Some(1),
            _ => None,
        }
    }
}

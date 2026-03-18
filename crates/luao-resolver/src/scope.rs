use std::collections::HashMap;

use crate::symbol::SymbolId;
use crate::types::LuaoType;

#[derive(Debug, Clone)]
pub struct Scope {
    pub parent: Option<usize>,
    pub symbols: HashMap<String, (SymbolId, LuaoType)>,
    pub kind: ScopeKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ScopeKind {
    Global,
    Function,
    Block,
    Class(String),
    Method,
}

impl Scope {
    pub fn new(kind: ScopeKind, parent: Option<usize>) -> Self {
        Self {
            parent,
            symbols: HashMap::new(),
            kind,
        }
    }
}

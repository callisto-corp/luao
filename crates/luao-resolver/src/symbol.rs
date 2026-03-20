use std::collections::HashMap;

use luao_parser::AccessModifier;

use crate::scope::Scope;
use crate::types::LuaoType;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SymbolId(pub usize);

#[derive(Debug, Clone)]
pub struct ClassSymbol {
    pub id: SymbolId,
    pub name: String,
    pub parent: Option<String>,
    pub interfaces: Vec<String>,
    pub fields: Vec<FieldSymbol>,
    pub methods: Vec<MethodSymbol>,
    pub is_abstract: bool,
    pub is_sealed: bool,
    pub is_extern: bool,
    pub type_params: Vec<String>,
    pub source_file: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FieldSymbol {
    pub name: String,
    pub type_info: LuaoType,
    pub access: AccessModifier,
    pub is_static: bool,
    pub is_readonly: bool,
    pub is_extern: bool,
}

#[derive(Debug, Clone)]
pub struct MethodSymbol {
    pub name: String,
    pub params: Vec<(String, LuaoType)>,
    pub return_type: LuaoType,
    pub access: AccessModifier,
    pub is_static: bool,
    pub is_abstract: bool,
    pub is_override: bool,
    pub is_extern: bool,
    pub is_async: bool,
    pub is_generator: bool,
}

#[derive(Debug, Clone)]
pub struct InterfaceSymbol {
    pub id: SymbolId,
    pub name: String,
    pub extends: Vec<String>,
    pub fields: Vec<FieldSymbol>,
    pub methods: Vec<MethodSymbol>,
    pub type_params: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct EnumSymbol {
    pub id: SymbolId,
    pub name: String,
    pub variants: Vec<EnumVariantSymbol>,
}

#[derive(Debug, Clone)]
pub struct EnumVariantSymbol {
    pub name: String,
    pub value: Option<i64>,
    pub is_extern: bool,
}

#[derive(Debug, Clone)]
pub struct SymbolTable {
    pub classes: HashMap<String, ClassSymbol>,
    pub interfaces: HashMap<String, InterfaceSymbol>,
    pub enums: HashMap<String, EnumSymbol>,
    pub scopes: Vec<Scope>,
    next_id: usize,
}

impl SymbolTable {
    pub fn new() -> Self {
        Self {
            classes: HashMap::new(),
            interfaces: HashMap::new(),
            enums: HashMap::new(),
            scopes: Vec::new(),
            next_id: 0,
        }
    }

    pub fn next_symbol_id(&mut self) -> SymbolId {
        let id = SymbolId(self.next_id);
        self.next_id += 1;
        id
    }

    pub fn register_class(&mut self, class: ClassSymbol) {
        self.classes.insert(class.name.clone(), class);
    }

    pub fn register_interface(&mut self, interface: InterfaceSymbol) {
        self.interfaces.insert(interface.name.clone(), interface);
    }

    pub fn register_enum(&mut self, enum_sym: EnumSymbol) {
        self.enums.insert(enum_sym.name.clone(), enum_sym);
    }

    pub fn lookup_class(&self, name: &str) -> Option<&ClassSymbol> {
        self.classes.get(name)
    }

    pub fn lookup_interface(&self, name: &str) -> Option<&InterfaceSymbol> {
        self.interfaces.get(name)
    }

    pub fn lookup_enum(&self, name: &str) -> Option<&EnumSymbol> {
        self.enums.get(name)
    }
}

impl Default for SymbolTable {
    fn default() -> Self {
        Self::new()
    }
}

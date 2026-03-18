use luao_parser::{
    AccessModifier, ClassMember, EnumDecl, Expression, InterfaceDecl, SourceFile, Statement,
    TypeAnnotation, TypeKind,
};

use crate::scope::{Scope, ScopeKind};
use crate::symbol::{
    ClassSymbol, EnumSymbol, EnumVariantSymbol, FieldSymbol, InterfaceSymbol, MethodSymbol, SymbolTable,
};
use crate::types::LuaoType;

pub struct Resolver {
    symbol_table: SymbolTable,
    source_file: Option<String>,
}

impl Resolver {
    pub fn new() -> Self {
        Self {
            symbol_table: SymbolTable::new(),
            source_file: None,
        }
    }

    pub fn resolve(&mut self, file: &SourceFile) -> SymbolTable {
        let global_scope = Scope::new(ScopeKind::Global, None);
        self.symbol_table.scopes.push(global_scope);

        for stmt in &file.statements {
            self.resolve_statement(stmt);
        }

        std::mem::replace(&mut self.symbol_table, SymbolTable::new())
    }

    fn resolve_statement(&mut self, stmt: &Statement) {
        match stmt {
            Statement::ClassDecl(class_decl) => self.resolve_class(class_decl),
            Statement::InterfaceDecl(iface_decl) => self.resolve_interface(iface_decl),
            Statement::EnumDecl(enum_decl) => self.resolve_enum(enum_decl),
            Statement::ExportDecl(inner, _) => self.resolve_statement(inner),
            _ => {}
        }
    }

    fn resolve_class(&mut self, decl: &luao_parser::ClassDecl) {
        let id = self.symbol_table.next_symbol_id();
        let name = decl.name.name.to_string();
        let parent = decl.parent.as_ref().map(|p| p.name.name.to_string());
        let interfaces = decl
            .interfaces
            .iter()
            .map(|i| i.name.name.to_string())
            .collect();
        let type_params = decl
            .type_params
            .iter()
            .map(|tp| tp.name.name.to_string())
            .collect();

        let mut fields = Vec::new();
        let mut methods = Vec::new();

        for member in &decl.members {
            match member {
                ClassMember::Field(f) => {
                    let type_info = f
                        .type_annotation
                        .as_ref()
                        .map(|ta| self.resolve_type(ta))
                        .unwrap_or(LuaoType::Unknown);
                    fields.push(FieldSymbol {
                        name: f.name.name.to_string(),
                        type_info,
                        access: f.access,
                        is_static: f.is_static,
                        is_readonly: f.is_readonly,
                        is_extern: f.is_extern,
                    });
                }
                ClassMember::Method(m) => {
                    let params = m
                        .params
                        .iter()
                        .map(|p| {
                            let ty = p
                                .type_annotation
                                .as_ref()
                                .map(|ta| self.resolve_type(ta))
                                .unwrap_or(LuaoType::Unknown);
                            (p.name.name.to_string(), ty)
                        })
                        .collect();
                    let return_type = m
                        .return_type
                        .as_ref()
                        .map(|ta| self.resolve_type(ta))
                        .unwrap_or(LuaoType::Void);
                    methods.push(MethodSymbol {
                        name: m.name.name.to_string(),
                        params,
                        return_type,
                        access: m.access,
                        is_static: m.is_static,
                        is_abstract: m.is_abstract,
                        is_override: m.is_override,
                        is_extern: m.is_extern,
                    });
                }
                ClassMember::Constructor(c) => {
                    let params = c
                        .params
                        .iter()
                        .map(|p| {
                            let ty = p
                                .type_annotation
                                .as_ref()
                                .map(|ta| self.resolve_type(ta))
                                .unwrap_or(LuaoType::Unknown);
                            (p.name.name.to_string(), ty)
                        })
                        .collect();
                    methods.push(MethodSymbol {
                        name: "constructor".to_string(),
                        params,
                        return_type: LuaoType::Void,
                        access: c.access,
                        is_static: false,
                        is_abstract: false,
                        is_override: false,
                        is_extern: false,
                    });
                }
                ClassMember::Property(_) => {}
            }
        }

        let class_sym = ClassSymbol {
            id,
            name: name.clone(),
            parent,
            interfaces,
            fields,
            methods,
            is_abstract: decl.is_abstract,
            is_sealed: decl.is_sealed,
            type_params,
            source_file: self.source_file.clone(),
        };

        self.symbol_table.register_class(class_sym);
    }

    fn resolve_interface(&mut self, decl: &InterfaceDecl) {
        let id = self.symbol_table.next_symbol_id();
        let name = decl.name.name.to_string();
        let extends = decl
            .extends
            .iter()
            .map(|e| e.name.name.to_string())
            .collect();
        let type_params = decl
            .type_params
            .iter()
            .map(|tp| tp.name.name.to_string())
            .collect();

        let methods = decl
            .methods
            .iter()
            .map(|m| {
                let params = m
                    .params
                    .iter()
                    .map(|p| {
                        let ty = p
                            .type_annotation
                            .as_ref()
                            .map(|ta| self.resolve_type(ta))
                            .unwrap_or(LuaoType::Unknown);
                        (p.name.name.to_string(), ty)
                    })
                    .collect();
                let return_type = m
                    .return_type
                    .as_ref()
                    .map(|ta| self.resolve_type(ta))
                    .unwrap_or(LuaoType::Void);
                MethodSymbol {
                    name: m.name.name.to_string(),
                    params,
                    return_type,
                    access: AccessModifier::Public,
                    is_static: false,
                    is_abstract: true,
                    is_override: false,
                    is_extern: m.is_extern,
                }
            })
            .collect();

        let iface_sym = InterfaceSymbol {
            id,
            name: name.clone(),
            extends,
            methods,
            type_params,
        };

        self.symbol_table.register_interface(iface_sym);
    }

    fn resolve_enum(&mut self, decl: &EnumDecl) {
        let id = self.symbol_table.next_symbol_id();
        let name = decl.name.name.to_string();

        let variants = decl
            .variants
            .iter()
            .map(|v| {
                let value = v.value.as_ref().and_then(|expr| {
                    if let Expression::Number(n, _) = expr {
                        n.parse::<i64>().ok()
                    } else {
                        None
                    }
                });
                EnumVariantSymbol {
                    name: v.name.name.to_string(),
                    value,
                    is_extern: v.is_extern,
                }
            })
            .collect();

        let enum_sym = EnumSymbol {
            id,
            name: name.clone(),
            variants,
        };

        self.symbol_table.register_enum(enum_sym);
    }

    fn resolve_type(&self, annotation: &TypeAnnotation) -> LuaoType {
        match &annotation.kind {
            TypeKind::Nil => LuaoType::Nil,
            TypeKind::Any => LuaoType::Any,
            TypeKind::Named(ident, type_args) => {
                let name = ident.name.as_str();
                match name {
                    "number" => LuaoType::Number,
                    "string" => LuaoType::String,
                    "boolean" => LuaoType::Boolean,
                    "void" => LuaoType::Void,
                    "table" if type_args.len() == 2 => {
                        let key = self.resolve_type(&type_args[0]);
                        let val = self.resolve_type(&type_args[1]);
                        LuaoType::Table(Box::new(key), Box::new(val))
                    }
                    _ => {
                        if let Some(cls) = self.symbol_table.lookup_class(name) {
                            LuaoType::Class(cls.id)
                        } else if let Some(iface) = self.symbol_table.lookup_interface(name) {
                            LuaoType::Interface(iface.id)
                        } else if let Some(en) = self.symbol_table.lookup_enum(name) {
                            LuaoType::Enum(en.id)
                        } else {
                            LuaoType::TypeParam(name.to_string())
                        }
                    }
                }
            }
            TypeKind::Function(params, ret) => {
                let param_types = params.iter().map(|p| self.resolve_type(p)).collect();
                let ret_type = self.resolve_type(ret);
                LuaoType::Function(param_types, Box::new(ret_type))
            }
            TypeKind::Array(inner) => {
                let inner_type = self.resolve_type(inner);
                LuaoType::Array(Box::new(inner_type))
            }
            TypeKind::Table(key, val) => {
                let key_type = self.resolve_type(key);
                let val_type = self.resolve_type(val);
                LuaoType::Table(Box::new(key_type), Box::new(val_type))
            }
            TypeKind::Union(types) => {
                let union_types = types.iter().map(|t| self.resolve_type(t)).collect();
                LuaoType::Union(union_types)
            }
            TypeKind::Optional(inner) => {
                let inner_type = self.resolve_type(inner);
                LuaoType::Optional(Box::new(inner_type))
            }
            TypeKind::Tuple(types) => {
                // Tuples resolve as a table of positional types; for now treat as Unknown
                let _ = types;
                LuaoType::Unknown
            }
        }
    }
}

impl Default for Resolver {
    fn default() -> Self {
        Self::new()
    }
}

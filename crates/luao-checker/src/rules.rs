use std::collections::HashMap;

use luao_parser::{
    AccessModifier, BinOp, Block, ClassMember, CompoundOp, Expression,
    SourceFile, Statement, TypeAnnotation, TypeKind, UnOp,
};
use luao_resolver::SymbolTable;
use luao_resolver::types::LuaoType;

use crate::diagnostic::Diagnostic;

// =============================================================================
// Type compatibility engine
// =============================================================================

/// Resolve a TypeAnnotation from AST into the resolver's LuaoType using the symbol table.
fn resolve_ast_type(ta: &TypeAnnotation, symbols: &SymbolTable) -> LuaoType {
    resolve_type_kind(&ta.kind, symbols)
}

fn resolve_type_kind(kind: &TypeKind, symbols: &SymbolTable) -> LuaoType {
    match kind {
        TypeKind::Named(id, type_args) => {
            let name = id.name.as_str();
            match name {
                "number" => LuaoType::Number,
                "string" => LuaoType::String,
                "boolean" => LuaoType::Boolean,
                "nil" => LuaoType::Nil,
                "any" => LuaoType::Any,
                "void" => LuaoType::Void,
                "table" if type_args.len() == 2 => {
                    let k = resolve_ast_type(&type_args[0], symbols);
                    let v = resolve_ast_type(&type_args[1], symbols);
                    LuaoType::Table(Box::new(k), Box::new(v))
                }
                "Table" if type_args.len() == 2 => {
                    let k = resolve_ast_type(&type_args[0], symbols);
                    let v = resolve_ast_type(&type_args[1], symbols);
                    LuaoType::Table(Box::new(k), Box::new(v))
                }
                _ => {
                    if let Some(cls) = symbols.lookup_class(name) {
                        LuaoType::Class(cls.id)
                    } else if let Some(iface) = symbols.lookup_interface(name) {
                        LuaoType::Interface(iface.id)
                    } else if let Some(en) = symbols.lookup_enum(name) {
                        LuaoType::Enum(en.id)
                    } else {
                        LuaoType::TypeParam(name.to_string())
                    }
                }
            }
        }
        TypeKind::Function(params, ret) => {
            let param_types: Vec<_> = params.iter().map(|p| resolve_ast_type(p, symbols)).collect();
            let ret_type = resolve_ast_type(ret, symbols);
            LuaoType::Function(param_types, Box::new(ret_type))
        }
        TypeKind::Array(inner) => {
            LuaoType::Array(Box::new(resolve_ast_type(inner, symbols)))
        }
        TypeKind::Table(k, v) => {
            LuaoType::Table(
                Box::new(resolve_ast_type(k, symbols)),
                Box::new(resolve_ast_type(v, symbols)),
            )
        }
        TypeKind::Union(parts) => {
            let types: Vec<_> = parts.iter().map(|p| resolve_ast_type(p, symbols)).collect();
            LuaoType::Union(types)
        }
        TypeKind::Optional(inner) => {
            LuaoType::Optional(Box::new(resolve_ast_type(inner, symbols)))
        }
        TypeKind::Nil => LuaoType::Nil,
        TypeKind::Any => LuaoType::Any,
        TypeKind::Tuple(_) => LuaoType::Unknown,
    }
}

/// Check if `source` type is assignable to `target` type.
/// Returns true if the assignment is valid.
fn is_assignable(source: &LuaoType, target: &LuaoType, symbols: &SymbolTable) -> bool {
    // Any is always compatible in both directions
    if matches!(source, LuaoType::Any) || matches!(target, LuaoType::Any) {
        return true;
    }
    // Unknown means we couldn't infer — allow it
    if matches!(source, LuaoType::Unknown) || matches!(target, LuaoType::Unknown) {
        return true;
    }
    // TypeParam — generic, allow it
    if matches!(source, LuaoType::TypeParam(_)) || matches!(target, LuaoType::TypeParam(_)) {
        return true;
    }
    // Exact match
    if source == target {
        return true;
    }
    // Nil is assignable to optional types
    if matches!(source, LuaoType::Nil) {
        return matches!(target, LuaoType::Optional(_) | LuaoType::Nil);
    }
    // T is assignable to T?
    if let LuaoType::Optional(inner) = target {
        if matches!(source, LuaoType::Nil) {
            return true;
        }
        return is_assignable(source, inner, symbols);
    }
    // Source is optional — unwrapped T? to T is ok if T matches
    if let LuaoType::Optional(inner) = source {
        return is_assignable(inner, target, symbols);
    }
    // Source is union — all members must be assignable to target
    if let LuaoType::Union(members) = source {
        return members.iter().all(|m| is_assignable(m, target, symbols));
    }
    // Target is union — source must be assignable to at least one member
    if let LuaoType::Union(members) = target {
        return members.iter().any(|m| is_assignable(source, m, symbols));
    }
    // Class to class — check inheritance chain
    if let (LuaoType::Class(src_id), LuaoType::Class(tgt_id)) = (source, target) {
        if src_id == tgt_id {
            return true;
        }
        // Walk inheritance chain of source to see if target is an ancestor
        return is_subclass_of_id(src_id, tgt_id, symbols);
    }
    // Class assignable to interface if it implements it
    if let (LuaoType::Class(src_id), LuaoType::Interface(tgt_id)) = (source, target) {
        return class_implements_interface_id(src_id, tgt_id, symbols);
    }
    // Array compatibility
    if let (LuaoType::Array(src_inner), LuaoType::Array(tgt_inner)) = (source, target) {
        return is_assignable(src_inner, tgt_inner, symbols);
    }
    // Table compatibility
    if let (LuaoType::Table(sk, sv), LuaoType::Table(tk, tv)) = (source, target) {
        return is_assignable(sk, tk, symbols) && is_assignable(sv, tv, symbols);
    }
    // Function compatibility
    if let (LuaoType::Function(sp, sr), LuaoType::Function(tp, tr)) = (source, target) {
        if sp.len() != tp.len() {
            return false;
        }
        // Contravariant params, covariant return
        for (s, t) in sp.iter().zip(tp.iter()) {
            if !is_assignable(t, s, symbols) {
                return false;
            }
        }
        return is_assignable(sr, tr, symbols);
    }
    // Enum is assignable to number (since enum values are numbers)
    if matches!(source, LuaoType::Enum(_)) && matches!(target, LuaoType::Number) {
        return true;
    }

    false
}

fn is_subclass_of_id(
    src_id: &luao_resolver::symbol::SymbolId,
    tgt_id: &luao_resolver::symbol::SymbolId,
    symbols: &SymbolTable,
) -> bool {
    // Find class by id
    let src_cls = symbols.classes.values().find(|c| c.id == *src_id);
    if let Some(cls) = src_cls {
        if let Some(ref parent_name) = cls.parent {
            if let Some(parent) = symbols.lookup_class(parent_name) {
                if parent.id == *tgt_id {
                    return true;
                }
                return is_subclass_of_id(&parent.id, tgt_id, symbols);
            }
        }
    }
    false
}

fn class_implements_interface_id(
    cls_id: &luao_resolver::symbol::SymbolId,
    iface_id: &luao_resolver::symbol::SymbolId,
    symbols: &SymbolTable,
) -> bool {
    let cls = symbols.classes.values().find(|c| c.id == *cls_id);
    if let Some(cls) = cls {
        for iface_name in &cls.interfaces {
            if let Some(iface) = symbols.lookup_interface(iface_name) {
                if iface.id == *iface_id {
                    return true;
                }
            }
        }
        // Check parent class too
        if let Some(ref parent_name) = cls.parent {
            if let Some(parent) = symbols.lookup_class(parent_name) {
                return class_implements_interface_id(&parent.id, iface_id, symbols);
            }
        }
    }
    false
}

fn type_name(ty: &LuaoType, symbols: &SymbolTable) -> String {
    match ty {
        LuaoType::Number => "number".to_string(),
        LuaoType::String => "string".to_string(),
        LuaoType::Boolean => "boolean".to_string(),
        LuaoType::Nil => "nil".to_string(),
        LuaoType::Any => "any".to_string(),
        LuaoType::Void => "void".to_string(),
        LuaoType::Table(k, v) => format!("Table<{}, {}>", type_name(k, symbols), type_name(v, symbols)),
        LuaoType::Array(inner) => format!("{}[]", type_name(inner, symbols)),
        LuaoType::Function(params, ret) => {
            let ps: Vec<_> = params.iter().map(|p| type_name(p, symbols)).collect();
            format!("({}) -> {}", ps.join(", "), type_name(ret, symbols))
        }
        LuaoType::Class(id) => {
            symbols.classes.values()
                .find(|c| c.id == *id)
                .map(|c| c.name.clone())
                .unwrap_or_else(|| format!("class#{}", id.0))
        }
        LuaoType::Interface(id) => {
            symbols.interfaces.values()
                .find(|i| i.id == *id)
                .map(|i| i.name.clone())
                .unwrap_or_else(|| format!("interface#{}", id.0))
        }
        LuaoType::Enum(id) => {
            symbols.enums.values()
                .find(|e| e.id == *id)
                .map(|e| e.name.clone())
                .unwrap_or_else(|| format!("enum#{}", id.0))
        }
        LuaoType::Union(parts) => {
            let ps: Vec<_> = parts.iter().map(|p| type_name(p, symbols)).collect();
            ps.join(" | ")
        }
        LuaoType::Optional(inner) => format!("{}?", type_name(inner, symbols)),
        LuaoType::TypeParam(name) => name.clone(),
        LuaoType::Unknown => "unknown".to_string(),
    }
}

// =============================================================================
// Type inference for expressions
// =============================================================================

/// Context for type-checking within a scope.
struct TypeEnv<'a> {
    symbols: &'a SymbolTable,
    /// local variable name → resolved type
    locals: HashMap<String, LuaoType>,
    /// Current class name (if inside a class body)
    current_class: Option<String>,
    /// Whether we are in a constructor
    in_constructor: bool,
    /// Whether we are in a static method
    in_static: bool,
    /// Expected return type of current function/method
    return_type: Option<LuaoType>,
}

impl<'a> TypeEnv<'a> {
    fn new(symbols: &'a SymbolTable) -> Self {
        Self {
            symbols,
            locals: HashMap::new(),
            current_class: None,
            in_constructor: false,
            in_static: false,
            return_type: None,
        }
    }

    fn child(&self) -> Self {
        Self {
            symbols: self.symbols,
            locals: self.locals.clone(),
            current_class: self.current_class.clone(),
            in_constructor: self.in_constructor,
            in_static: self.in_static,
            return_type: self.return_type.clone(),
        }
    }

    /// Infer the type of an expression.
    fn infer_expr(&self, expr: &Expression) -> LuaoType {
        match expr {
            Expression::Nil(_) => LuaoType::Nil,
            Expression::True(_) | Expression::False(_) => LuaoType::Boolean,
            Expression::Number(_, _) => LuaoType::Number,
            Expression::String(_, _) => LuaoType::String,
            Expression::Identifier(id) => {
                let name = id.name.as_str();
                if name == "self" {
                    if let Some(ref cls_name) = self.current_class {
                        if let Some(cls) = self.symbols.lookup_class(cls_name) {
                            return LuaoType::Class(cls.id);
                        }
                    }
                    return LuaoType::Unknown;
                }
                if let Some(ty) = self.locals.get(name) {
                    return ty.clone();
                }
                // Check if it's a class/enum name
                if let Some(cls) = self.symbols.lookup_class(name) {
                    return LuaoType::Class(cls.id);
                }
                if let Some(en) = self.symbols.lookup_enum(name) {
                    return LuaoType::Enum(en.id);
                }
                LuaoType::Unknown
            }
            Expression::NewExpr(ne) => {
                let name = ne.class_name.name.name.as_str();
                if let Some(cls) = self.symbols.lookup_class(name) {
                    LuaoType::Class(cls.id)
                } else {
                    LuaoType::Unknown
                }
            }
            Expression::FunctionCall(fc) => {
                // Try to resolve return type from callee
                if let Expression::FieldAccess(fa) = &fc.callee {
                    let obj_name = match &fa.object {
                        Expression::Identifier(id) => Some(id.name.as_str()),
                        _ => None,
                    };
                    if let Some(name) = obj_name {
                        if let Some(cls) = self.symbols.lookup_class(name) {
                            let method_name = fa.field.name.as_str();
                            if method_name == "new" {
                                return LuaoType::Class(cls.id);
                            }
                            if let Some(m) = cls.methods.iter().find(|m| m.name == method_name) {
                                return m.return_type.clone();
                            }
                        }
                    }
                }
                LuaoType::Unknown
            }
            Expression::MethodCall(mc) => {
                let obj_ty = self.infer_expr(&mc.object);
                let method_name = mc.method.name.as_str();
                if let LuaoType::Class(id) = &obj_ty {
                    if let Some(cls) = self.symbols.classes.values().find(|c| c.id == *id) {
                        // Search this class and parents
                        if let Some(m) = find_method_in_hierarchy(cls, method_name, self.symbols) {
                            return m.return_type.clone();
                        }
                    }
                }
                LuaoType::Unknown
            }
            Expression::FieldAccess(fa) => {
                let obj_ty = self.infer_expr(&fa.object);
                let field_name = fa.field.name.as_str();
                if let LuaoType::Class(id) = &obj_ty {
                    if let Some(cls) = self.symbols.classes.values().find(|c| c.id == *id) {
                        if let Some(f) = find_field_in_hierarchy(cls, field_name, self.symbols) {
                            return f.type_info.clone();
                        }
                    }
                }
                if let LuaoType::Enum(id) = &obj_ty {
                    if let Some(en) = self.symbols.enums.values().find(|e| e.id == *id) {
                        if field_name == "_values" {
                            return LuaoType::Table(Box::new(LuaoType::Number), Box::new(LuaoType::String));
                        }
                        if en.variants.iter().any(|v| v.name == field_name) {
                            return LuaoType::Number;
                        }
                    }
                }
                LuaoType::Unknown
            }
            Expression::BinaryOp(bo) => {
                match bo.op {
                    BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div |
                    BinOp::IntDiv | BinOp::Mod | BinOp::Pow => LuaoType::Number,
                    BinOp::Concat => LuaoType::String,
                    BinOp::Eq | BinOp::NotEq | BinOp::Lt | BinOp::Le |
                    BinOp::Gt | BinOp::Ge => LuaoType::Boolean,
                    BinOp::And | BinOp::Or => {
                        // and/or return one of their operands
                        let left = self.infer_expr(&bo.left);
                        let right = self.infer_expr(&bo.right);
                        if left == right { left } else { LuaoType::Unknown }
                    }
                    BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor |
                    BinOp::ShiftLeft | BinOp::ShiftRight => LuaoType::Number,
                }
            }
            Expression::UnaryOp(uo) => {
                match uo.op {
                    UnOp::Neg | UnOp::Len | UnOp::BitNot => LuaoType::Number,
                    UnOp::Not => LuaoType::Boolean,
                }
            }
            Expression::CastExpr(ce) => {
                resolve_ast_type(&ce.target_type, self.symbols)
            }
            Expression::Instanceof(_) => LuaoType::Boolean,
            Expression::FunctionExpr(fe) => {
                let param_types: Vec<_> = fe.params.iter().map(|p| {
                    p.type_annotation.as_ref()
                        .map(|ta| resolve_ast_type(ta, self.symbols))
                        .unwrap_or(LuaoType::Unknown)
                }).collect();
                let ret = fe.return_type.as_ref()
                    .map(|ta| resolve_ast_type(ta, self.symbols))
                    .unwrap_or(LuaoType::Unknown);
                LuaoType::Function(param_types, Box::new(ret))
            }
            Expression::TableConstructor(_) => LuaoType::Unknown,
            Expression::IndexAccess(_) => LuaoType::Unknown,
            Expression::IfExpression(ie) => {
                self.infer_expr(&ie.then_expr)
            }
            Expression::Vararg(_) => LuaoType::Unknown,
            Expression::SuperAccess(_) => LuaoType::Unknown,
            Expression::YieldExpr(_) => LuaoType::Unknown,
            Expression::AwaitExpr(ae) => self.infer_expr(&ae.expr),
        }
    }
}

fn find_method_in_hierarchy<'a>(
    cls: &'a luao_resolver::symbol::ClassSymbol,
    name: &str,
    symbols: &'a SymbolTable,
) -> Option<&'a luao_resolver::symbol::MethodSymbol> {
    if let Some(m) = cls.methods.iter().find(|m| m.name == name) {
        return Some(m);
    }
    if let Some(ref parent_name) = cls.parent {
        if let Some(parent) = symbols.lookup_class(parent_name) {
            return find_method_in_hierarchy(parent, name, symbols);
        }
    }
    None
}

fn find_field_in_hierarchy<'a>(
    cls: &'a luao_resolver::symbol::ClassSymbol,
    name: &str,
    symbols: &'a SymbolTable,
) -> Option<&'a luao_resolver::symbol::FieldSymbol> {
    if let Some(f) = cls.fields.iter().find(|f| f.name == name) {
        return Some(f);
    }
    if let Some(ref parent_name) = cls.parent {
        if let Some(parent) = symbols.lookup_class(parent_name) {
            return find_field_in_hierarchy(parent, name, symbols);
        }
    }
    None
}

/// Collect all abstract methods from the entire inheritance chain.
fn collect_abstract_methods<'a>(
    cls: &'a luao_resolver::symbol::ClassSymbol,
    symbols: &'a SymbolTable,
) -> Vec<(&'a str, &'a str)> {
    // Returns (method_name, declaring_class_name) for abstract methods not yet implemented
    let mut result = Vec::new();
    if let Some(ref parent_name) = cls.parent {
        if let Some(parent) = symbols.lookup_class(parent_name) {
            // Recursively get abstract methods from parent chain
            let parent_abstracts = collect_abstract_methods(parent, symbols);
            for (method_name, decl_class) in parent_abstracts {
                // Check if this class implements it
                if !cls.methods.iter().any(|m| m.name == method_name) {
                    result.push((method_name, decl_class));
                }
            }
            // Add abstract methods from direct parent
            for m in &parent.methods {
                if m.is_abstract && !cls.methods.iter().any(|cm| cm.name == m.name) {
                    result.push((&m.name, &parent.name));
                }
            }
        }
    }
    result
}

/// Collect all interface methods (including from parent interfaces).
fn collect_interface_methods<'a>(
    iface: &'a luao_resolver::symbol::InterfaceSymbol,
    symbols: &'a SymbolTable,
) -> Vec<&'a luao_resolver::symbol::MethodSymbol> {
    let mut methods: Vec<&luao_resolver::symbol::MethodSymbol> = iface.methods.iter().collect();
    for parent_name in &iface.extends {
        if let Some(parent) = symbols.lookup_interface(parent_name) {
            methods.extend(collect_interface_methods(parent, symbols));
        }
    }
    methods
}

/// Collect all interface fields (including from parent interfaces).
fn collect_interface_fields<'a>(
    iface: &'a luao_resolver::symbol::InterfaceSymbol,
    symbols: &'a SymbolTable,
) -> Vec<&'a luao_resolver::symbol::FieldSymbol> {
    let mut fields: Vec<&luao_resolver::symbol::FieldSymbol> = iface.fields.iter().collect();
    for parent_name in &iface.extends {
        if let Some(parent) = symbols.lookup_interface(parent_name) {
            fields.extend(collect_interface_fields(parent, symbols));
        }
    }
    fields
}

// =============================================================================
// E001: Cannot instantiate abstract class
// =============================================================================

pub fn check_abstract_instantiation(
    file: &SourceFile,
    symbols: &SymbolTable,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    walk_statements(&file.statements, &mut |stmt| {
        walk_exprs_in_stmt(stmt, &mut |expr| {
            if let Expression::NewExpr(new_expr) = expr {
                let class_name = new_expr.class_name.name.name.as_str();
                if let Some(cls) = symbols.lookup_class(class_name) {
                    if cls.is_abstract {
                        diagnostics.push(Diagnostic::error(
                            format!("cannot instantiate abstract class '{}'", class_name),
                            new_expr.span,
                            "E001",
                        ));
                    }
                }
            }
        });
    });
    diagnostics
}

// =============================================================================
// E002: Non-abstract class must implement all inherited abstract methods
// (transitive — walks full inheritance chain)
// =============================================================================

pub fn check_abstract_methods(
    _file: &SourceFile,
    symbols: &SymbolTable,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for cls in symbols.classes.values() {
        if cls.is_abstract {
            continue;
        }
        let unimplemented = collect_abstract_methods(cls, symbols);
        for (method_name, decl_class) in unimplemented {
            diagnostics.push(Diagnostic::error(
                format!(
                    "class '{}' must implement abstract method '{}' from '{}'",
                    cls.name, method_name, decl_class
                ),
                luao_lexer::Span::empty(),
                "E002",
            ));
        }
    }
    diagnostics
}

// =============================================================================
// E003: Class with abstract methods must be declared abstract
// =============================================================================

pub fn check_abstract_class_has_abstract_methods(
    file: &SourceFile,
    _symbols: &SymbolTable,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for stmt in &file.statements {
        let decl = match stmt {
            Statement::ClassDecl(d) => d,
            Statement::ExportDecl(inner, _) => {
                if let Statement::ClassDecl(d) = inner.as_ref() { d } else { continue; }
            }
            _ => continue,
        };
        if decl.is_abstract {
            continue;
        }
        let has_abstract_method = decl.members.iter().any(|m| {
            if let ClassMember::Method(method) = m {
                method.is_abstract
            } else {
                false
            }
        });
        if has_abstract_method {
            diagnostics.push(Diagnostic::error(
                format!(
                    "class '{}' has abstract methods but is not declared 'abstract'",
                    decl.name.name
                ),
                decl.name.span,
                "E003",
            ));
        }
    }
    diagnostics
}

// =============================================================================
// E004: Cannot extend sealed class from a different file
// =============================================================================

pub fn check_sealed_inheritance(
    _file: &SourceFile,
    symbols: &SymbolTable,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for cls in symbols.classes.values() {
        if let Some(parent_name) = &cls.parent {
            if let Some(parent) = symbols.lookup_class(parent_name) {
                if parent.is_sealed {
                    let same_file = match (&cls.source_file, &parent.source_file) {
                        (Some(a), Some(b)) => a == b,
                        _ => false,
                    };
                    if !same_file {
                        diagnostics.push(Diagnostic::error(
                            format!(
                                "cannot extend sealed class '{}' from a different file",
                                parent_name
                            ),
                            luao_lexer::Span::empty(),
                            "E004",
                        ));
                    }
                }
            }
        }
    }
    diagnostics
}

// =============================================================================
// E006: super used outside of a class with a parent
// =============================================================================

pub fn check_super_usage(
    file: &SourceFile,
    symbols: &SymbolTable,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for stmt in &file.statements {
        if let Statement::ClassDecl(decl) = stmt {
            let class_name = decl.name.name.as_str();
            let has_parent = symbols
                .lookup_class(class_name)
                .and_then(|c| c.parent.as_ref())
                .is_some();
            for member in &decl.members {
                match member {
                    ClassMember::Method(m) => {
                        if let Some(body) = &m.body {
                            check_super_in_block(body, has_parent, class_name, &mut diagnostics);
                        }
                    }
                    ClassMember::Constructor(c) => {
                        check_super_in_block(&c.body, has_parent, class_name, &mut diagnostics);
                    }
                    _ => {}
                }
            }
        }
    }
    diagnostics
}

fn check_super_in_block(
    block: &Block,
    has_parent: bool,
    class_name: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    walk_statements(&block.statements, &mut |stmt| {
        walk_exprs_in_stmt(stmt, &mut |expr| {
            if let Expression::SuperAccess(sa) = expr {
                if !has_parent {
                    diagnostics.push(Diagnostic::error(
                        format!(
                            "cannot use 'super' in class '{}' which has no parent class",
                            class_name
                        ),
                        sa.span,
                        "E006",
                    ));
                }
            }
        });
    });
}

// =============================================================================
// E007: override specified but no parent method exists
// =============================================================================

pub fn check_override_validity(
    _file: &SourceFile,
    symbols: &SymbolTable,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for cls in symbols.classes.values() {
        for method in &cls.methods {
            if method.is_override {
                let has_parent_method = has_method_in_ancestor(cls, &method.name, symbols);
                if !has_parent_method {
                    diagnostics.push(Diagnostic::error(
                        format!(
                            "method '{}' in class '{}' is marked override but no parent method '{}' exists",
                            method.name, cls.name, method.name
                        ),
                        luao_lexer::Span::empty(),
                        "E007",
                    ));
                }
            }
        }
    }
    diagnostics
}

fn has_method_in_ancestor(
    cls: &luao_resolver::symbol::ClassSymbol,
    method_name: &str,
    symbols: &SymbolTable,
) -> bool {
    if let Some(ref parent_name) = cls.parent {
        if let Some(parent) = symbols.lookup_class(parent_name) {
            if parent.methods.iter().any(|m| m.name == method_name) {
                return true;
            }
            return has_method_in_ancestor(parent, method_name, symbols);
        }
    }
    false
}

// =============================================================================
// E009: Cannot access private member from outside declaring class
// E010: Cannot access protected member from unrelated class
// =============================================================================

pub fn check_access_modifiers(
    file: &SourceFile,
    symbols: &SymbolTable,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut var_types: HashMap<String, String> = HashMap::new();
    for stmt in &file.statements {
        check_access_in_statement(stmt, None, symbols, &mut var_types, &mut diagnostics);
    }
    diagnostics
}

fn check_access_in_statement(
    stmt: &Statement,
    current_class: Option<&str>,
    symbols: &SymbolTable,
    var_types: &mut HashMap<String, String>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match stmt {
        Statement::ClassDecl(decl) => {
            let class_name = decl.name.name.as_str();
            for member in &decl.members {
                match member {
                    ClassMember::Method(m) => {
                        if let Some(body) = &m.body {
                            let mut child_vars = var_types.clone();
                            check_access_in_block(body, Some(class_name), symbols, &mut child_vars, diagnostics);
                        }
                    }
                    ClassMember::Constructor(c) => {
                        let mut child_vars = var_types.clone();
                        check_access_in_block(&c.body, Some(class_name), symbols, &mut child_vars, diagnostics);
                    }
                    ClassMember::Property(p) => {
                        if let Some(ref getter) = p.getter {
                            let mut child_vars = var_types.clone();
                            check_access_in_block(getter, Some(class_name), symbols, &mut child_vars, diagnostics);
                        }
                        if let Some((_, ref setter)) = p.setter {
                            let mut child_vars = var_types.clone();
                            check_access_in_block(setter, Some(class_name), symbols, &mut child_vars, diagnostics);
                        }
                    }
                    _ => {}
                }
            }
        }
        Statement::FunctionDecl(f) => {
            let mut child_vars = var_types.clone();
            check_access_in_block(&f.body, current_class, symbols, &mut child_vars, diagnostics);
        }
        Statement::ExpressionStatement(expr) => {
            check_access_in_expr(expr, current_class, symbols, var_types, diagnostics);
        }
        Statement::LocalAssignment(la) => {
            for val in &la.values {
                check_access_in_expr(val, current_class, symbols, var_types, diagnostics);
            }
            // Track variable types from type annotations and new expressions
            for (i, name) in la.names.iter().enumerate() {
                // From type annotation
                if let Some(Some(ta)) = la.type_annotations.get(i) {
                    if let TypeKind::Named(ref id, _) = ta.kind {
                        let type_name = id.name.to_string();
                        if symbols.classes.contains_key(type_name.as_str()) {
                            var_types.insert(name.name.to_string(), type_name);
                        }
                    }
                }
                // From new expression
                if let Some(Expression::NewExpr(ne)) = la.values.get(i) {
                    var_types.insert(
                        name.name.to_string(),
                        ne.class_name.name.name.to_string(),
                    );
                }
            }
        }
        Statement::Assignment(a) => {
            for val in &a.values {
                check_access_in_expr(val, current_class, symbols, var_types, diagnostics);
            }
            for target in &a.targets {
                check_access_in_expr(target, current_class, symbols, var_types, diagnostics);
            }
        }
        Statement::IfStatement(i) => {
            check_access_in_expr(&i.condition, current_class, symbols, var_types, diagnostics);
            check_access_in_block(&i.then_block, current_class, symbols, var_types, diagnostics);
            for (cond, block) in &i.elseif_clauses {
                check_access_in_expr(cond, current_class, symbols, var_types, diagnostics);
                check_access_in_block(block, current_class, symbols, var_types, diagnostics);
            }
            if let Some(block) = &i.else_block {
                check_access_in_block(block, current_class, symbols, var_types, diagnostics);
            }
        }
        Statement::WhileStatement(w) => {
            check_access_in_expr(&w.condition, current_class, symbols, var_types, diagnostics);
            check_access_in_block(&w.body, current_class, symbols, var_types, diagnostics);
        }
        Statement::RepeatStatement(r) => {
            check_access_in_block(&r.body, current_class, symbols, var_types, diagnostics);
            check_access_in_expr(&r.condition, current_class, symbols, var_types, diagnostics);
        }
        Statement::ReturnStatement(r) => {
            for val in &r.values {
                check_access_in_expr(val, current_class, symbols, var_types, diagnostics);
            }
        }
        _ => {}
    }
}

fn check_access_in_block(
    block: &Block,
    current_class: Option<&str>,
    symbols: &SymbolTable,
    var_types: &mut HashMap<String, String>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for stmt in &block.statements {
        check_access_in_statement(stmt, current_class, symbols, var_types, diagnostics);
    }
}

fn check_access_in_expr(
    expr: &Expression,
    current_class: Option<&str>,
    symbols: &SymbolTable,
    var_types: &HashMap<String, String>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match expr {
        Expression::FieldAccess(fa) => {
            let field_name = fa.field.name.as_str();
            let target_class_name = resolve_access_target_owned(&fa.object, current_class, symbols, var_types);

            if let Some(ref target) = target_class_name {
                if let Some(cls) = symbols.lookup_class(target) {
                    // Check field access
                    if let Some(field) = find_field_in_hierarchy(cls, field_name, symbols) {
                        check_member_access(
                            field.access, field_name, &cls.name, "field",
                            current_class, fa.span, symbols, diagnostics,
                        );
                    }
                    // Check method access (ClassName.method)
                    if let Some(method) = find_method_in_hierarchy(cls, field_name, symbols) {
                        check_member_access(
                            method.access, field_name, &cls.name, "method",
                            current_class, fa.span, symbols, diagnostics,
                        );
                    }
                }
            }
            check_access_in_expr(&fa.object, current_class, symbols, var_types, diagnostics);
        }
        Expression::MethodCall(mc) => {
            let method_name = mc.method.name.as_str();
            let target_class_name = resolve_access_target_owned(&mc.object, current_class, symbols, var_types);
            if let Some(ref target) = target_class_name {
                if let Some(cls) = symbols.lookup_class(target) {
                    if let Some(method) = find_method_in_hierarchy(cls, method_name, symbols) {
                        check_member_access(
                            method.access, method_name, &cls.name, "method",
                            current_class, mc.span, symbols, diagnostics,
                        );
                    }
                }
            }
            check_access_in_expr(&mc.object, current_class, symbols, var_types, diagnostics);
            for arg in &mc.args {
                check_access_in_expr(arg, current_class, symbols, var_types, diagnostics);
            }
        }
        Expression::FunctionCall(fc) => {
            check_access_in_expr(&fc.callee, current_class, symbols, var_types, diagnostics);
            for arg in &fc.args {
                check_access_in_expr(arg, current_class, symbols, var_types, diagnostics);
            }
        }
        Expression::BinaryOp(bo) => {
            check_access_in_expr(&bo.left, current_class, symbols, var_types, diagnostics);
            check_access_in_expr(&bo.right, current_class, symbols, var_types, diagnostics);
        }
        Expression::UnaryOp(uo) => {
            check_access_in_expr(&uo.operand, current_class, symbols, var_types, diagnostics);
        }
        _ => {}
    }
}

fn resolve_access_target_owned(
    expr: &Expression,
    current_class: Option<&str>,
    symbols: &SymbolTable,
    var_types: &HashMap<String, String>,
) -> Option<String> {
    match expr {
        Expression::Identifier(id) if id.name.as_str() == "self" => {
            current_class.map(|s| s.to_string())
        }
        Expression::Identifier(id) => {
            let name = id.name.as_str();
            // Check if it's a known class name (static access)
            if symbols.classes.contains_key(name) {
                return Some(name.to_string());
            }
            // Check if variable has a known class type
            if let Some(class_name) = var_types.get(name) {
                return Some(class_name.clone());
            }
            None
        }
        _ => None,
    }
}

fn check_member_access(
    access: AccessModifier,
    member_name: &str,
    declaring_class: &str,
    kind: &str,
    current_class: Option<&str>,
    span: luao_lexer::Span,
    symbols: &SymbolTable,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match access {
        AccessModifier::Private => {
            if current_class != Some(declaring_class) {
                diagnostics.push(Diagnostic::error(
                    format!(
                        "cannot access private {} '{}' of class '{}'",
                        kind, member_name, declaring_class
                    ),
                    span,
                    "E009",
                ));
            }
        }
        AccessModifier::Protected => {
            let is_self = current_class == Some(declaring_class);
            let is_subclass = current_class.map_or(false, |cur| {
                is_subclass_by_name(cur, declaring_class, symbols)
            });
            if !is_self && !is_subclass {
                diagnostics.push(Diagnostic::error(
                    format!(
                        "cannot access protected {} '{}' of class '{}' from unrelated class",
                        kind, member_name, declaring_class
                    ),
                    span,
                    "E010",
                ));
            }
        }
        AccessModifier::Public => {}
    }
}

fn is_subclass_by_name(child: &str, ancestor: &str, symbols: &SymbolTable) -> bool {
    if let Some(cls) = symbols.lookup_class(child) {
        if let Some(ref parent_name) = cls.parent {
            if parent_name == ancestor {
                return true;
            }
            return is_subclass_by_name(parent_name, ancestor, symbols);
        }
    }
    false
}

// =============================================================================
// E011: Assignment to readonly field outside constructor
// =============================================================================

pub fn check_readonly_assignments(
    file: &SourceFile,
    symbols: &SymbolTable,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for stmt in &file.statements {
        if let Statement::ClassDecl(decl) = stmt {
            let class_name = decl.name.name.as_str();
            for member in &decl.members {
                match member {
                    ClassMember::Method(m) => {
                        if let Some(body) = &m.body {
                            check_readonly_in_block(body, class_name, symbols, false, &mut diagnostics);
                        }
                    }
                    ClassMember::Constructor(c) => {
                        // In constructor, readonly assignments to own fields ARE allowed
                        check_readonly_in_block(&c.body, class_name, symbols, true, &mut diagnostics);
                    }
                    ClassMember::Property(p) => {
                        if let Some(ref getter) = p.getter {
                            check_readonly_in_block(getter, class_name, symbols, false, &mut diagnostics);
                        }
                        if let Some((_, ref setter)) = p.setter {
                            check_readonly_in_block(setter, class_name, symbols, false, &mut diagnostics);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    diagnostics
}

fn check_readonly_in_block(
    block: &Block,
    class_name: &str,
    symbols: &SymbolTable,
    in_constructor: bool,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for stmt in &block.statements {
        match stmt {
            Statement::Assignment(a) => {
                for target in &a.targets {
                    check_readonly_target(target, class_name, symbols, in_constructor, diagnostics);
                }
            }
            Statement::CompoundAssignment(ca) => {
                check_readonly_target(&ca.target, class_name, symbols, in_constructor, diagnostics);
            }
            Statement::IfStatement(i) => {
                check_readonly_in_block(&i.then_block, class_name, symbols, in_constructor, diagnostics);
                for (_, block) in &i.elseif_clauses {
                    check_readonly_in_block(block, class_name, symbols, in_constructor, diagnostics);
                }
                if let Some(block) = &i.else_block {
                    check_readonly_in_block(block, class_name, symbols, in_constructor, diagnostics);
                }
            }
            Statement::WhileStatement(w) => {
                check_readonly_in_block(&w.body, class_name, symbols, in_constructor, diagnostics);
            }
            Statement::RepeatStatement(r) => {
                check_readonly_in_block(&r.body, class_name, symbols, in_constructor, diagnostics);
            }
            Statement::ForNumeric(f) => {
                check_readonly_in_block(&f.body, class_name, symbols, in_constructor, diagnostics);
            }
            Statement::ForGeneric(f) => {
                check_readonly_in_block(&f.body, class_name, symbols, in_constructor, diagnostics);
            }
            Statement::DoBlock(b) => {
                check_readonly_in_block(b, class_name, symbols, in_constructor, diagnostics);
            }
            _ => {}
        }
    }
}

fn check_readonly_target(
    target: &Expression,
    class_name: &str,
    symbols: &SymbolTable,
    in_constructor: bool,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if let Expression::FieldAccess(fa) = target {
        // Only check self.field assignments
        if let Expression::Identifier(id) = &fa.object {
            if id.name.as_str() == "self" {
                let field_name = fa.field.name.as_str();
                if let Some(cls) = symbols.lookup_class(class_name) {
                    if let Some(field) = cls.fields.iter().find(|f| f.name == field_name) {
                        if field.is_readonly && !in_constructor {
                            diagnostics.push(Diagnostic::error(
                                format!(
                                    "cannot assign to readonly field '{}' outside constructor",
                                    field_name
                                ),
                                fa.span,
                                "E011",
                            ));
                        }
                    }
                }
            }
        }
    }
}

// =============================================================================
// E012: Class does not implement interface method/field
// (checks method signatures and fields, including parent interfaces)
// =============================================================================

pub fn check_interface_conformance(
    _file: &SourceFile,
    symbols: &SymbolTable,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for cls in symbols.classes.values() {
        for iface_name in &cls.interfaces {
            if let Some(iface) = symbols.lookup_interface(iface_name) {
                // Check methods (including inherited interface methods)
                let required_methods = collect_interface_methods(iface, symbols);
                for iface_method in &required_methods {
                    let class_method = find_method_in_hierarchy(cls, &iface_method.name, symbols);
                    match class_method {
                        None => {
                            diagnostics.push(Diagnostic::error(
                                format!(
                                    "class '{}' must implement method '{}' from interface '{}'",
                                    cls.name, iface_method.name, iface_name
                                ),
                                luao_lexer::Span::empty(),
                                "E012",
                            ));
                        }
                        Some(impl_method) => {
                            // Check parameter count
                            if impl_method.params.len() != iface_method.params.len() {
                                diagnostics.push(Diagnostic::error(
                                    format!(
                                        "method '{}' in class '{}' has {} parameters but interface '{}' requires {}",
                                        iface_method.name, cls.name,
                                        impl_method.params.len(), iface_name,
                                        iface_method.params.len()
                                    ),
                                    luao_lexer::Span::empty(),
                                    "E012",
                                ));
                            } else {
                                // Check parameter types
                                for (i, ((_, impl_ty), (_, iface_ty))) in impl_method.params.iter()
                                    .zip(iface_method.params.iter()).enumerate()
                                {
                                    if !matches!(impl_ty, LuaoType::Unknown) &&
                                       !matches!(iface_ty, LuaoType::Unknown) &&
                                       !is_assignable(iface_ty, impl_ty, symbols) {
                                        diagnostics.push(Diagnostic::error(
                                            format!(
                                                "parameter {} of method '{}' in class '{}' has type '{}' but interface '{}' expects '{}'",
                                                i + 1, iface_method.name, cls.name,
                                                type_name(impl_ty, symbols),
                                                iface_name,
                                                type_name(iface_ty, symbols)
                                            ),
                                            luao_lexer::Span::empty(),
                                            "E012",
                                        ));
                                    }
                                }
                            }
                            // Check return type
                            if !matches!(impl_method.return_type, LuaoType::Unknown) &&
                               !matches!(iface_method.return_type, LuaoType::Unknown) &&
                               !is_assignable(&impl_method.return_type, &iface_method.return_type, symbols)
                            {
                                diagnostics.push(Diagnostic::error(
                                    format!(
                                        "method '{}' in class '{}' returns '{}' but interface '{}' expects '{}'",
                                        iface_method.name, cls.name,
                                        type_name(&impl_method.return_type, symbols),
                                        iface_name,
                                        type_name(&iface_method.return_type, symbols)
                                    ),
                                    luao_lexer::Span::empty(),
                                    "E012",
                                ));
                            }
                        }
                    }
                }
                // Check fields (including inherited interface fields)
                let required_fields = collect_interface_fields(iface, symbols);
                for iface_field in &required_fields {
                    let class_field = find_field_in_hierarchy(cls, &iface_field.name, symbols);
                    match class_field {
                        None => {
                            diagnostics.push(Diagnostic::error(
                                format!(
                                    "class '{}' must have field '{}' from interface '{}'",
                                    cls.name, iface_field.name, iface_name
                                ),
                                luao_lexer::Span::empty(),
                                "E012",
                            ));
                        }
                        Some(impl_field) => {
                            if !matches!(impl_field.type_info, LuaoType::Unknown) &&
                               !matches!(iface_field.type_info, LuaoType::Unknown) &&
                               !is_assignable(&impl_field.type_info, &iface_field.type_info, symbols)
                            {
                                diagnostics.push(Diagnostic::error(
                                    format!(
                                        "field '{}' in class '{}' has type '{}' but interface '{}' expects '{}'",
                                        iface_field.name, cls.name,
                                        type_name(&impl_field.type_info, symbols),
                                        iface_name,
                                        type_name(&iface_field.type_info, symbols)
                                    ),
                                    luao_lexer::Span::empty(),
                                    "E012",
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
    diagnostics
}

// =============================================================================
// E013: Duplicate enum entry
// =============================================================================

pub fn check_duplicate_enum_entries(
    file: &SourceFile,
    _symbols: &SymbolTable,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for stmt in &file.statements {
        let decl = match stmt {
            Statement::EnumDecl(d) => d,
            Statement::ExportDecl(inner, _) => {
                if let Statement::EnumDecl(d) = inner.as_ref() { d } else { continue; }
            }
            _ => continue,
        };
        let mut seen: HashMap<&str, luao_lexer::Span> = HashMap::new();
        for variant in &decl.variants {
            let name = variant.name.name.as_str();
            if let Some(prev_span) = seen.get(name) {
                diagnostics.push(Diagnostic::error(
                    format!(
                        "duplicate enum entry '{}' in enum '{}'",
                        name, decl.name.name
                    ),
                    variant.name.span,
                    "E013",
                ));
                let _ = prev_span; // acknowledge it exists
            } else {
                seen.insert(name, variant.name.span);
            }
        }
    }
    diagnostics
}

// =============================================================================
// E014: Mixed auto-increment and string values in enum
// =============================================================================

pub fn check_enum_mixed_values(
    file: &SourceFile,
    _symbols: &SymbolTable,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for stmt in &file.statements {
        let decl = match stmt {
            Statement::EnumDecl(d) => d,
            Statement::ExportDecl(inner, _) => {
                if let Statement::EnumDecl(d) = inner.as_ref() { d } else { continue; }
            }
            _ => continue,
        };
        if decl.variants.is_empty() {
            continue;
        }
        let mut has_string = false;
        let mut has_number_or_auto = false;
        for variant in &decl.variants {
            match &variant.value {
                Some(Expression::String(_, _)) => has_string = true,
                Some(Expression::Number(_, _)) => has_number_or_auto = true,
                None => has_number_or_auto = true, // auto-increment
                _ => {}
            }
        }
        if has_string && has_number_or_auto {
            diagnostics.push(Diagnostic::error(
                format!(
                    "enum '{}' mixes string values with numeric/auto-increment values",
                    decl.name.name
                ),
                decl.name.span,
                "E014",
            ));
        }
        // If all string, check that all have explicit values
        if has_string && !has_number_or_auto {
            for variant in &decl.variants {
                if variant.value.is_none() {
                    diagnostics.push(Diagnostic::error(
                        format!(
                            "enum entry '{}' in string enum '{}' must have an explicit value",
                            variant.name.name, decl.name.name
                        ),
                        variant.name.span,
                        "E014",
                    ));
                }
            }
        }
    }
    diagnostics
}

// =============================================================================
// E015: Static method cannot reference self
// =============================================================================

pub fn check_static_self_usage(
    file: &SourceFile,
    _symbols: &SymbolTable,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for stmt in &file.statements {
        let decl = match stmt {
            Statement::ClassDecl(d) => d,
            Statement::ExportDecl(inner, _) => {
                if let Statement::ClassDecl(d) = inner.as_ref() { d } else { continue; }
            }
            _ => continue,
        };
        for member in &decl.members {
            if let ClassMember::Method(m) = member {
                if m.is_static {
                    if let Some(ref body) = m.body {
                        check_self_in_block(body, &decl.name.name, &m.name.name, &mut diagnostics);
                    }
                }
            }
        }
    }
    diagnostics
}

fn check_self_in_block(
    block: &Block,
    class_name: &smol_str::SmolStr,
    method_name: &smol_str::SmolStr,
    diagnostics: &mut Vec<Diagnostic>,
) {
    walk_statements(&block.statements, &mut |stmt| {
        walk_exprs_in_stmt(stmt, &mut |expr| {
            if let Expression::Identifier(id) = expr {
                if id.name.as_str() == "self" {
                    diagnostics.push(Diagnostic::error(
                        format!(
                            "static method '{}' in class '{}' cannot reference 'self'",
                            method_name, class_name
                        ),
                        id.span,
                        "E015",
                    ));
                }
            }
        });
    });
}

// =============================================================================
// E016: Constructor must not return a value
// =============================================================================

pub fn check_constructor_return(
    file: &SourceFile,
    _symbols: &SymbolTable,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for stmt in &file.statements {
        let decl = match stmt {
            Statement::ClassDecl(d) => d,
            Statement::ExportDecl(inner, _) => {
                if let Statement::ClassDecl(d) = inner.as_ref() { d } else { continue; }
            }
            _ => continue,
        };
        for member in &decl.members {
            if let ClassMember::Constructor(ctor) = member {
                check_return_in_block(&ctor.body, &decl.name.name, &mut diagnostics);
            }
        }
    }
    diagnostics
}

fn check_return_in_block(
    block: &Block,
    class_name: &smol_str::SmolStr,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for stmt in &block.statements {
        match stmt {
            Statement::ReturnStatement(r) => {
                if !r.values.is_empty() {
                    diagnostics.push(Diagnostic::error(
                        format!(
                            "constructor in class '{}' must not explicitly return a value",
                            class_name
                        ),
                        r.span,
                        "E016",
                    ));
                }
            }
            Statement::IfStatement(i) => {
                check_return_in_block(&i.then_block, class_name, diagnostics);
                for (_, block) in &i.elseif_clauses {
                    check_return_in_block(block, class_name, diagnostics);
                }
                if let Some(block) = &i.else_block {
                    check_return_in_block(block, class_name, diagnostics);
                }
            }
            Statement::WhileStatement(w) => {
                check_return_in_block(&w.body, class_name, diagnostics);
            }
            Statement::RepeatStatement(r) => {
                check_return_in_block(&r.body, class_name, diagnostics);
            }
            Statement::DoBlock(b) => {
                check_return_in_block(b, class_name, diagnostics);
            }
            _ => {}
        }
    }
}

// =============================================================================
// E017: Duplicate class member
// =============================================================================

pub fn check_duplicate_members(
    file: &SourceFile,
    _symbols: &SymbolTable,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for stmt in &file.statements {
        let decl = match stmt {
            Statement::ClassDecl(d) => d,
            Statement::ExportDecl(inner, _) => {
                if let Statement::ClassDecl(d) = inner.as_ref() { d } else { continue; }
            }
            _ => continue,
        };
        let mut seen: HashMap<String, (&str, luao_lexer::Span)> = HashMap::new();
        for member in &decl.members {
            let (name, kind, span) = match member {
                ClassMember::Field(f) => (f.name.name.to_string(), "field", f.name.span),
                ClassMember::Method(m) => (m.name.name.to_string(), "method", m.name.span),
                ClassMember::Property(p) => (p.name.name.to_string(), "property", p.name.span),
                ClassMember::Constructor(_) => continue, // handled by E020
            };
            if let Some((prev_kind, _prev_span)) = seen.get(&name) {
                diagnostics.push(Diagnostic::error(
                    format!(
                        "duplicate {} '{}' in class '{}' (previously declared as {})",
                        kind, name, decl.name.name, prev_kind
                    ),
                    span,
                    "E017",
                ));
            } else {
                seen.insert(name, (kind, span));
            }
        }
    }
    diagnostics
}

// =============================================================================
// E018: Type mismatch
// (full type checking: assignments, function args, returns, operators)
// =============================================================================

pub fn check_type_mismatches(
    file: &SourceFile,
    symbols: &SymbolTable,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut env = TypeEnv::new(symbols);
    for stmt in &file.statements {
        check_types_in_statement(stmt, &mut env, &mut diagnostics);
    }
    diagnostics
}

fn check_types_in_statement(
    stmt: &Statement,
    env: &mut TypeEnv,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match stmt {
        Statement::LocalAssignment(la) => {
            for (i, name) in la.names.iter().enumerate() {
                let declared_type = la.type_annotations.get(i)
                    .and_then(|t| t.as_ref())
                    .map(|ta| resolve_ast_type(ta, env.symbols));

                let value_type = la.values.get(i).map(|v| env.infer_expr(v));

                if let (Some(decl_ty), Some(val_ty)) = (&declared_type, &value_type) {
                    if !matches!(decl_ty, LuaoType::Unknown) &&
                       !matches!(val_ty, LuaoType::Unknown) &&
                       !is_assignable(val_ty, decl_ty, env.symbols)
                    {
                        diagnostics.push(Diagnostic::error(
                            format!(
                                "type mismatch: cannot assign '{}' to variable '{}' of type '{}'",
                                type_name(val_ty, env.symbols),
                                name.name,
                                type_name(decl_ty, env.symbols)
                            ),
                            name.span,
                            "E018",
                        ));
                    }
                }

                // Register variable type for future lookups
                if let Some(ty) = declared_type {
                    env.locals.insert(name.name.to_string(), ty);
                } else if let Some(ty) = value_type {
                    env.locals.insert(name.name.to_string(), ty);
                }
            }
            // Check expressions in values
            for val in &la.values {
                check_types_in_expr(val, env, diagnostics);
            }
        }
        Statement::Assignment(a) => {
            for (target, value) in a.targets.iter().zip(a.values.iter()) {
                let target_ty = env.infer_expr(target);
                let value_ty = env.infer_expr(value);
                if !matches!(target_ty, LuaoType::Unknown) &&
                   !matches!(value_ty, LuaoType::Unknown) &&
                   !is_assignable(&value_ty, &target_ty, env.symbols)
                {
                    diagnostics.push(Diagnostic::error(
                        format!(
                            "type mismatch: cannot assign '{}' to '{}'",
                            type_name(&value_ty, env.symbols),
                            type_name(&target_ty, env.symbols)
                        ),
                        target.span(),
                        "E018",
                    ));
                }
            }
            for val in &a.values {
                check_types_in_expr(val, env, diagnostics);
            }
        }
        Statement::CompoundAssignment(ca) => {
            let target_ty = env.infer_expr(&ca.target);
            let value_ty = env.infer_expr(&ca.value);
            let expected = match ca.op {
                CompoundOp::Add | CompoundOp::Sub | CompoundOp::Mul |
                CompoundOp::Div | CompoundOp::Mod | CompoundOp::Pow => LuaoType::Number,
                CompoundOp::Concat => LuaoType::String,
            };
            if !matches!(value_ty, LuaoType::Unknown) && !is_assignable(&value_ty, &expected, env.symbols) {
                diagnostics.push(Diagnostic::error(
                    format!(
                        "type mismatch: operator requires '{}' but got '{}'",
                        type_name(&expected, env.symbols),
                        type_name(&value_ty, env.symbols)
                    ),
                    ca.span,
                    "E018",
                ));
            }
            let _ = target_ty; // target type is also checked implicitly
        }
        Statement::ReturnStatement(r) => {
            if let Some(ref expected_ret) = env.return_type {
                if !r.values.is_empty() {
                    let actual_ty = env.infer_expr(&r.values[0]);
                    if !matches!(actual_ty, LuaoType::Unknown) &&
                       !matches!(expected_ret, LuaoType::Unknown | LuaoType::Void) &&
                       !is_assignable(&actual_ty, expected_ret, env.symbols)
                    {
                        diagnostics.push(Diagnostic::error(
                            format!(
                                "type mismatch: function returns '{}' but expected '{}'",
                                type_name(&actual_ty, env.symbols),
                                type_name(expected_ret, env.symbols)
                            ),
                            r.span,
                            "E018",
                        ));
                    }
                } else if !matches!(expected_ret, LuaoType::Void | LuaoType::Unknown) {
                    // Returning nothing when a type is expected (only warn, not error — could be early return)
                }
            }
            for val in &r.values {
                check_types_in_expr(val, env, diagnostics);
            }
        }
        Statement::ClassDecl(decl) => {
            let class_name = decl.name.name.to_string();
            for member in &decl.members {
                match member {
                    ClassMember::Constructor(ctor) => {
                        let mut child = env.child();
                        child.current_class = Some(class_name.clone());
                        child.in_constructor = true;
                        child.return_type = Some(LuaoType::Void);
                        for param in &ctor.params {
                            if let Some(ref ta) = param.type_annotation {
                                child.locals.insert(
                                    param.name.name.to_string(),
                                    resolve_ast_type(ta, env.symbols),
                                );
                            }
                        }
                        check_types_in_block(&ctor.body, &mut child, diagnostics);
                    }
                    ClassMember::Method(m) => {
                        if let Some(ref body) = m.body {
                            let mut child = env.child();
                            child.current_class = Some(class_name.clone());
                            child.in_static = m.is_static;
                            child.return_type = m.return_type.as_ref()
                                .map(|ta| resolve_ast_type(ta, env.symbols));
                            for param in &m.params {
                                if let Some(ref ta) = param.type_annotation {
                                    child.locals.insert(
                                        param.name.name.to_string(),
                                        resolve_ast_type(ta, env.symbols),
                                    );
                                }
                            }
                            check_types_in_block(body, &mut child, diagnostics);
                        }
                    }
                    ClassMember::Field(f) => {
                        if let (Some(ta), Some(val)) = (&f.type_annotation, &f.default_value) {
                            let declared = resolve_ast_type(ta, env.symbols);
                            let actual = env.infer_expr(val);
                            if !matches!(actual, LuaoType::Unknown) &&
                               !matches!(declared, LuaoType::Unknown) &&
                               !is_assignable(&actual, &declared, env.symbols)
                            {
                                diagnostics.push(Diagnostic::error(
                                    format!(
                                        "type mismatch: field '{}' declared as '{}' but default value is '{}'",
                                        f.name.name,
                                        type_name(&declared, env.symbols),
                                        type_name(&actual, env.symbols)
                                    ),
                                    f.name.span,
                                    "E018",
                                ));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        Statement::FunctionDecl(f) => {
            let mut child = env.child();
            child.return_type = f.return_type.as_ref()
                .map(|ta| resolve_ast_type(ta, env.symbols));
            for param in &f.params {
                if let Some(ref ta) = param.type_annotation {
                    child.locals.insert(
                        param.name.name.to_string(),
                        resolve_ast_type(ta, env.symbols),
                    );
                }
            }
            check_types_in_block(&f.body, &mut child, diagnostics);
        }
        Statement::IfStatement(i) => {
            check_types_in_expr(&i.condition, env, diagnostics);
            check_types_in_block(&i.then_block, env, diagnostics);
            for (cond, block) in &i.elseif_clauses {
                check_types_in_expr(cond, env, diagnostics);
                check_types_in_block(block, env, diagnostics);
            }
            if let Some(block) = &i.else_block {
                check_types_in_block(block, env, diagnostics);
            }
        }
        Statement::WhileStatement(w) => {
            check_types_in_expr(&w.condition, env, diagnostics);
            check_types_in_block(&w.body, env, diagnostics);
        }
        Statement::RepeatStatement(r) => {
            check_types_in_block(&r.body, env, diagnostics);
            check_types_in_expr(&r.condition, env, diagnostics);
        }
        Statement::ForNumeric(f) => {
            let mut child = env.child();
            child.locals.insert(f.name.name.to_string(), LuaoType::Number);
            check_types_in_block(&f.body, &mut child, diagnostics);
        }
        Statement::ForGeneric(f) => {
            check_types_in_block(&f.body, env, diagnostics);
        }
        Statement::DoBlock(b) => {
            check_types_in_block(b, env, diagnostics);
        }
        Statement::ExpressionStatement(expr) => {
            check_types_in_expr(expr, env, diagnostics);
        }
        _ => {}
    }
}

fn check_types_in_block(
    block: &Block,
    env: &mut TypeEnv,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for stmt in &block.statements {
        check_types_in_statement(stmt, env, diagnostics);
    }
}

fn check_types_in_expr(
    expr: &Expression,
    env: &TypeEnv,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match expr {
        Expression::BinaryOp(bo) => {
            let left_ty = env.infer_expr(&bo.left);
            let right_ty = env.infer_expr(&bo.right);

            match bo.op {
                BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div |
                BinOp::IntDiv | BinOp::Mod | BinOp::Pow => {
                    // Arithmetic: both operands must be number (or have operator overloads)
                    let left_has_overload = has_metamethod(&left_ty, &bo.op, env.symbols);
                    if !left_has_overload {
                        if !matches!(left_ty, LuaoType::Unknown | LuaoType::Any | LuaoType::Number) {
                            diagnostics.push(Diagnostic::error(
                                format!(
                                    "arithmetic operator on non-number type '{}'",
                                    type_name(&left_ty, env.symbols)
                                ),
                                bo.span,
                                "E018",
                            ));
                        }
                        if !matches!(right_ty, LuaoType::Unknown | LuaoType::Any | LuaoType::Number) {
                            diagnostics.push(Diagnostic::error(
                                format!(
                                    "arithmetic operator on non-number type '{}'",
                                    type_name(&right_ty, env.symbols)
                                ),
                                bo.span,
                                "E018",
                            ));
                        }
                    }
                }
                BinOp::Concat => {
                    // Concat: operands must be string or number
                    for (ty, side) in [(&left_ty, "left"), (&right_ty, "right")] {
                        if !matches!(ty, LuaoType::Unknown | LuaoType::Any |
                                       LuaoType::String | LuaoType::Number) {
                            diagnostics.push(Diagnostic::error(
                                format!(
                                    "cannot concatenate {} operand of type '{}'",
                                    side, type_name(ty, env.symbols)
                                ),
                                bo.span,
                                "E018",
                            ));
                        }
                    }
                }
                BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor |
                BinOp::ShiftLeft | BinOp::ShiftRight => {
                    // Bitwise: both operands must be number (integers)
                    if !matches!(left_ty, LuaoType::Unknown | LuaoType::Any | LuaoType::Number) {
                        diagnostics.push(Diagnostic::error(
                            format!(
                                "bitwise operator on non-number type '{}'",
                                type_name(&left_ty, env.symbols)
                            ),
                            bo.span,
                            "E018",
                        ));
                    }
                    if !matches!(right_ty, LuaoType::Unknown | LuaoType::Any | LuaoType::Number) {
                        diagnostics.push(Diagnostic::error(
                            format!(
                                "bitwise operator on non-number type '{}'",
                                type_name(&right_ty, env.symbols)
                            ),
                            bo.span,
                            "E018",
                        ));
                    }
                }
                _ => {} // comparison/logical — any type is fine
            }
            check_types_in_expr(&bo.left, env, diagnostics);
            check_types_in_expr(&bo.right, env, diagnostics);
        }
        Expression::UnaryOp(uo) => {
            let operand_ty = env.infer_expr(&uo.operand);
            match uo.op {
                UnOp::Neg => {
                    if !matches!(operand_ty, LuaoType::Unknown | LuaoType::Any | LuaoType::Number) {
                        // Check for __unm metamethod
                        if !has_unary_metamethod(&operand_ty, "__unm", env.symbols) {
                            diagnostics.push(Diagnostic::error(
                                format!(
                                    "unary minus on non-number type '{}'",
                                    type_name(&operand_ty, env.symbols)
                                ),
                                uo.span,
                                "E018",
                            ));
                        }
                    }
                }
                UnOp::Len => {
                    if !matches!(operand_ty, LuaoType::Unknown | LuaoType::Any |
                                           LuaoType::String | LuaoType::Table(_, _) |
                                           LuaoType::Array(_)) {
                        if !has_unary_metamethod(&operand_ty, "__len", env.symbols) {
                            diagnostics.push(Diagnostic::error(
                                format!(
                                    "length operator on type '{}' (expected string, table, or array)",
                                    type_name(&operand_ty, env.symbols)
                                ),
                                uo.span,
                                "E018",
                            ));
                        }
                    }
                }
                UnOp::BitNot => {
                    if !matches!(operand_ty, LuaoType::Unknown | LuaoType::Any | LuaoType::Number) {
                        diagnostics.push(Diagnostic::error(
                            format!(
                                "bitwise not on non-number type '{}'",
                                type_name(&operand_ty, env.symbols)
                            ),
                            uo.span,
                            "E018",
                        ));
                    }
                }
                UnOp::Not => {} // `not` works on any type
            }
            check_types_in_expr(&uo.operand, env, diagnostics);
        }
        Expression::MethodCall(mc) => {
            // Check argument types against method signature
            let obj_ty = env.infer_expr(&mc.object);
            if let LuaoType::Class(id) = &obj_ty {
                if let Some(cls) = env.symbols.classes.values().find(|c| c.id == *id) {
                    if let Some(method) = find_method_in_hierarchy(cls, mc.method.name.as_str(), env.symbols) {
                        // Check argument count
                        let expected_params = method.params.len();
                        let actual_args = mc.args.len();
                        let has_varargs = method.params.last().map_or(false, |(_, ty)| matches!(ty, LuaoType::Unknown));
                        if !has_varargs && actual_args != expected_params {
                            diagnostics.push(Diagnostic::error(
                                format!(
                                    "method '{}' expects {} argument(s) but got {}",
                                    mc.method.name, expected_params, actual_args
                                ),
                                mc.span,
                                "E018",
                            ));
                        }
                        // Check argument types
                        for (i, (arg, (_, param_ty))) in mc.args.iter().zip(method.params.iter()).enumerate() {
                            let arg_ty = env.infer_expr(arg);
                            if !matches!(arg_ty, LuaoType::Unknown) &&
                               !matches!(param_ty, LuaoType::Unknown) &&
                               !is_assignable(&arg_ty, param_ty, env.symbols)
                            {
                                diagnostics.push(Diagnostic::error(
                                    format!(
                                        "argument {} of method '{}' has type '{}' but expected '{}'",
                                        i + 1, mc.method.name,
                                        type_name(&arg_ty, env.symbols),
                                        type_name(param_ty, env.symbols)
                                    ),
                                    arg.span(),
                                    "E018",
                                ));
                            }
                        }
                    }
                }
            }
            check_types_in_expr(&mc.object, env, diagnostics);
            for arg in &mc.args {
                check_types_in_expr(arg, env, diagnostics);
            }
        }
        Expression::FunctionCall(fc) => {
            check_types_in_expr(&fc.callee, env, diagnostics);
            for arg in &fc.args {
                check_types_in_expr(arg, env, diagnostics);
            }
        }
        Expression::NewExpr(ne) => {
            // Check constructor arguments
            let class_name = ne.class_name.name.name.as_str();
            if let Some(cls) = env.symbols.lookup_class(class_name) {
                if let Some(ctor) = cls.methods.iter().find(|m| m.name == "constructor") {
                    let expected_params = ctor.params.len();
                    let actual_args = ne.args.len();
                    if actual_args != expected_params {
                        diagnostics.push(Diagnostic::error(
                            format!(
                                "constructor of '{}' expects {} argument(s) but got {}",
                                class_name, expected_params, actual_args
                            ),
                            ne.span,
                            "E018",
                        ));
                    }
                    for (i, (arg, (_, param_ty))) in ne.args.iter().zip(ctor.params.iter()).enumerate() {
                        let arg_ty = env.infer_expr(arg);
                        if !matches!(arg_ty, LuaoType::Unknown) &&
                           !matches!(param_ty, LuaoType::Unknown) &&
                           !is_assignable(&arg_ty, param_ty, env.symbols)
                        {
                            diagnostics.push(Diagnostic::error(
                                format!(
                                    "argument {} of '{}' constructor has type '{}' but expected '{}'",
                                    i + 1, class_name,
                                    type_name(&arg_ty, env.symbols),
                                    type_name(param_ty, env.symbols)
                                ),
                                arg.span(),
                                "E018",
                            ));
                        }
                    }
                }
            }
            for arg in &ne.args {
                check_types_in_expr(arg, env, diagnostics);
            }
        }
        _ => {}
    }
}

fn has_metamethod(ty: &LuaoType, op: &BinOp, symbols: &SymbolTable) -> bool {
    let metamethod_name = match op {
        BinOp::Add => "__add",
        BinOp::Sub => "__sub",
        BinOp::Mul => "__mul",
        BinOp::Div => "__div",
        BinOp::IntDiv => "__idiv",
        BinOp::Mod => "__mod",
        BinOp::Pow => "__pow",
        BinOp::Concat => "__concat",
        BinOp::Eq => "__eq",
        BinOp::Lt => "__lt",
        BinOp::Le => "__le",
        _ => return false,
    };
    has_unary_metamethod(ty, metamethod_name, symbols)
}

fn has_unary_metamethod(ty: &LuaoType, name: &str, symbols: &SymbolTable) -> bool {
    if let LuaoType::Class(id) = ty {
        if let Some(cls) = symbols.classes.values().find(|c| c.id == *id) {
            return find_method_in_hierarchy(cls, name, symbols).is_some();
        }
    }
    false
}

// =============================================================================
// E019: instanceof right-hand side must be a class name
// =============================================================================

pub fn check_instanceof_usage(
    file: &SourceFile,
    symbols: &SymbolTable,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    walk_statements(&file.statements, &mut |stmt| {
        walk_exprs_in_stmt(stmt, &mut |expr| {
            if let Expression::Instanceof(ie) = expr {
                let class_name = ie.class_name.name.as_str();
                if symbols.lookup_class(class_name).is_none() {
                    diagnostics.push(Diagnostic::error(
                        format!(
                            "instanceof right-hand side '{}' is not a class name",
                            class_name
                        ),
                        ie.class_name.span,
                        "E019",
                    ));
                }
            }
        });
    });
    diagnostics
}

// =============================================================================
// E020: Cannot declare more than one constructor per class
// =============================================================================

pub fn check_multiple_constructors(
    file: &SourceFile,
    _symbols: &SymbolTable,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for stmt in &file.statements {
        let decl = match stmt {
            Statement::ClassDecl(d) => d,
            Statement::ExportDecl(inner, _) => {
                if let Statement::ClassDecl(d) = inner.as_ref() { d } else { continue; }
            }
            _ => continue,
        };
        let mut ctor_count = 0;
        for member in &decl.members {
            if let ClassMember::Constructor(ctor) = member {
                ctor_count += 1;
                if ctor_count > 1 {
                    diagnostics.push(Diagnostic::error(
                        format!(
                            "class '{}' cannot have more than one constructor",
                            decl.name.name
                        ),
                        ctor.span,
                        "E020",
                    ));
                }
            }
        }
    }
    diagnostics
}

// =============================================================================
// Union type member access check (E013 in spec refers to this)
// =============================================================================

pub fn check_union_member_access(
    file: &SourceFile,
    _symbols: &SymbolTable,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut env = UnionEnv::new();
    for stmt in &file.statements {
        check_union_in_statement(stmt, &mut env, &mut diagnostics);
    }
    diagnostics
}

struct UnionEnv {
    union_vars: HashMap<String, bool>,
}

impl UnionEnv {
    fn new() -> Self {
        Self {
            union_vars: HashMap::new(),
        }
    }

    fn register(&mut self, name: &str, ty: Option<&TypeAnnotation>) {
        let is_union = ty.map_or(false, |t| matches!(t.kind, TypeKind::Union(_)));
        self.union_vars.insert(name.to_string(), is_union);
    }

    fn is_union(&self, name: &str) -> bool {
        self.union_vars.get(name).copied().unwrap_or(false)
    }
}

fn check_union_in_statement(
    stmt: &Statement,
    env: &mut UnionEnv,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match stmt {
        Statement::LocalAssignment(la) => {
            for (i, name) in la.names.iter().enumerate() {
                let ty = la.type_annotations.get(i).and_then(|t| t.as_ref());
                env.register(&name.name, ty);
            }
            for val in &la.values {
                check_union_in_expr(val, env, diagnostics);
            }
        }
        Statement::ClassDecl(decl) => {
            for member in &decl.members {
                match member {
                    ClassMember::Method(m) => {
                        let mut inner_env = UnionEnv::new();
                        inner_env.union_vars = env.union_vars.clone();
                        for p in &m.params {
                            inner_env.register(&p.name.name, p.type_annotation.as_ref());
                        }
                        if let Some(body) = &m.body {
                            check_union_in_block(body, &mut inner_env, diagnostics);
                        }
                    }
                    ClassMember::Constructor(c) => {
                        let mut inner_env = UnionEnv::new();
                        inner_env.union_vars = env.union_vars.clone();
                        for p in &c.params {
                            inner_env.register(&p.name.name, p.type_annotation.as_ref());
                        }
                        check_union_in_block(&c.body, &mut inner_env, diagnostics);
                    }
                    _ => {}
                }
            }
        }
        Statement::FunctionDecl(f) => {
            let mut inner_env = UnionEnv::new();
            inner_env.union_vars = env.union_vars.clone();
            for p in &f.params {
                inner_env.register(&p.name.name, p.type_annotation.as_ref());
            }
            check_union_in_block(&f.body, &mut inner_env, diagnostics);
        }
        Statement::ExpressionStatement(expr) => {
            check_union_in_expr(expr, env, diagnostics);
        }
        Statement::Assignment(a) => {
            for val in &a.values {
                check_union_in_expr(val, env, diagnostics);
            }
            for target in &a.targets {
                check_union_in_expr(target, env, diagnostics);
            }
        }
        Statement::ReturnStatement(r) => {
            for val in &r.values {
                check_union_in_expr(val, env, diagnostics);
            }
        }
        Statement::IfStatement(i) => {
            check_union_in_expr(&i.condition, env, diagnostics);
            check_union_in_block(&i.then_block, env, diagnostics);
            for (cond, block) in &i.elseif_clauses {
                check_union_in_expr(cond, env, diagnostics);
                check_union_in_block(block, env, diagnostics);
            }
            if let Some(block) = &i.else_block {
                check_union_in_block(block, env, diagnostics);
            }
        }
        Statement::WhileStatement(w) => {
            check_union_in_expr(&w.condition, env, diagnostics);
            check_union_in_block(&w.body, env, diagnostics);
        }
        Statement::RepeatStatement(r) => {
            check_union_in_block(&r.body, env, diagnostics);
            check_union_in_expr(&r.condition, env, diagnostics);
        }
        Statement::ForNumeric(f) => {
            check_union_in_block(&f.body, env, diagnostics);
        }
        Statement::ForGeneric(f) => {
            check_union_in_block(&f.body, env, diagnostics);
        }
        Statement::DoBlock(b) => {
            check_union_in_block(b, env, diagnostics);
        }
        _ => {}
    }
}

fn check_union_in_block(
    block: &Block,
    env: &mut UnionEnv,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for stmt in &block.statements {
        check_union_in_statement(stmt, env, diagnostics);
    }
}

fn check_union_in_expr(
    expr: &Expression,
    env: &UnionEnv,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match expr {
        Expression::FieldAccess(fa) => {
            if is_union_object(&fa.object, env) {
                diagnostics.push(Diagnostic::error(
                    format!(
                        "cannot access member '{}' on a union type; use 'as' to cast to a specific type first",
                        fa.field.name
                    ),
                    fa.span,
                    "E018",
                ));
            }
            check_union_in_expr(&fa.object, env, diagnostics);
        }
        Expression::MethodCall(mc) => {
            if is_union_object(&mc.object, env) {
                diagnostics.push(Diagnostic::error(
                    format!(
                        "cannot call method '{}' on a union type; use 'as' to cast to a specific type first",
                        mc.method.name
                    ),
                    mc.span,
                    "E018",
                ));
            }
            check_union_in_expr(&mc.object, env, diagnostics);
            for arg in &mc.args {
                check_union_in_expr(arg, env, diagnostics);
            }
        }
        Expression::FunctionCall(fc) => {
            check_union_in_expr(&fc.callee, env, diagnostics);
            for arg in &fc.args {
                check_union_in_expr(arg, env, diagnostics);
            }
        }
        Expression::BinaryOp(bo) => {
            check_union_in_expr(&bo.left, env, diagnostics);
            check_union_in_expr(&bo.right, env, diagnostics);
        }
        Expression::UnaryOp(uo) => {
            check_union_in_expr(&uo.operand, env, diagnostics);
        }
        Expression::CastExpr(_) => {
            // Cast narrows the type — don't check inner for union access
        }
        Expression::IndexAccess(ia) => {
            check_union_in_expr(&ia.object, env, diagnostics);
            check_union_in_expr(&ia.index, env, diagnostics);
        }
        Expression::TableConstructor(tc) => {
            for field in &tc.fields {
                match field {
                    luao_parser::TableField::NamedField(_, val, _) => {
                        check_union_in_expr(val, env, diagnostics);
                    }
                    luao_parser::TableField::IndexField(key, val, _) => {
                        check_union_in_expr(key, env, diagnostics);
                        check_union_in_expr(val, env, diagnostics);
                    }
                    luao_parser::TableField::ValueField(val, _) => {
                        check_union_in_expr(val, env, diagnostics);
                    }
                }
            }
        }
        _ => {}
    }
}

fn is_union_object(expr: &Expression, env: &UnionEnv) -> bool {
    match expr {
        Expression::Identifier(id) => env.is_union(&id.name),
        Expression::CastExpr(_) => false,
        _ => false,
    }
}

// =============================================================================
// Import shadowing check
// =============================================================================

pub fn check_import_shadowing(
    file: &SourceFile,
    _symbols: &SymbolTable,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    let mut imported_names: HashMap<String, luao_lexer::Span> = HashMap::new();
    for stmt in &file.statements {
        if let Statement::ImportDecl(import) = stmt {
            for name in &import.names {
                let local_name = name
                    .alias
                    .as_ref()
                    .map(|a| a.name.to_string())
                    .unwrap_or_else(|| name.name.name.to_string());
                imported_names.insert(local_name, import.span);
            }
        }
    }

    if imported_names.is_empty() {
        return diagnostics;
    }

    for stmt in &file.statements {
        let declared_names: Vec<(String, luao_lexer::Span)> = match stmt {
            Statement::LocalAssignment(la) => {
                la.names.iter().map(|n| (n.name.to_string(), n.span)).collect()
            }
            Statement::FunctionDecl(fd) => {
                if fd.is_local {
                    if let Some(part) = fd.name.parts.first() {
                        vec![(part.name.to_string(), part.span)]
                    } else {
                        vec![]
                    }
                } else {
                    vec![]
                }
            }
            Statement::ClassDecl(cd) => {
                vec![(cd.name.name.to_string(), cd.name.span)]
            }
            Statement::EnumDecl(ed) => {
                vec![(ed.name.name.to_string(), ed.name.span)]
            }
            Statement::ExportDecl(inner, _) => {
                match inner.as_ref() {
                    Statement::LocalAssignment(la) => {
                        la.names.iter().map(|n| (n.name.to_string(), n.span)).collect()
                    }
                    Statement::FunctionDecl(fd) => {
                        if let Some(part) = fd.name.parts.first() {
                            vec![(part.name.to_string(), part.span)]
                        } else {
                            vec![]
                        }
                    }
                    Statement::ClassDecl(cd) => {
                        vec![(cd.name.name.to_string(), cd.name.span)]
                    }
                    Statement::EnumDecl(ed) => {
                        vec![(ed.name.name.to_string(), ed.name.span)]
                    }
                    _ => vec![],
                }
            }
            _ => vec![],
        };

        for (name, span) in declared_names {
            if imported_names.contains_key(&name) {
                diagnostics.push(Diagnostic::error(
                    format!(
                        "cannot declare '{}' — it shadows an imported name; use 'as' to alias the import",
                        name
                    ),
                    span,
                    "E014",
                ));
            }
        }
    }

    diagnostics
}

// =============================================================================
// Override signature validation
// (when overriding, parameter count and types should match parent)
// =============================================================================

pub fn check_override_signatures(
    _file: &SourceFile,
    symbols: &SymbolTable,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for cls in symbols.classes.values() {
        if cls.parent.is_none() {
            continue;
        }
        for method in &cls.methods {
            if !method.is_override {
                continue;
            }
            if let Some(ref parent_name) = cls.parent {
                if let Some(parent) = symbols.lookup_class(parent_name) {
                    if let Some(parent_method) = find_method_in_hierarchy(parent, &method.name, symbols) {
                        // Check parameter count
                        if method.params.len() != parent_method.params.len() {
                            diagnostics.push(Diagnostic::error(
                                format!(
                                    "override method '{}' in class '{}' has {} parameters but parent has {}",
                                    method.name, cls.name,
                                    method.params.len(), parent_method.params.len()
                                ),
                                luao_lexer::Span::empty(),
                                "E007",
                            ));
                        }
                        // Check return type compatibility
                        if !matches!(method.return_type, LuaoType::Unknown) &&
                           !matches!(parent_method.return_type, LuaoType::Unknown) &&
                           !is_assignable(&method.return_type, &parent_method.return_type, symbols)
                        {
                            diagnostics.push(Diagnostic::error(
                                format!(
                                    "override method '{}' in class '{}' returns '{}' but parent returns '{}'",
                                    method.name, cls.name,
                                    type_name(&method.return_type, symbols),
                                    type_name(&parent_method.return_type, symbols)
                                ),
                                luao_lexer::Span::empty(),
                                "E007",
                            ));
                        }
                    }
                }
            }
        }
    }
    diagnostics
}

// =============================================================================
// Helpers: walking statements and expressions
// =============================================================================

fn walk_statements(stmts: &[Statement], visitor: &mut dyn FnMut(&Statement)) {
    for stmt in stmts {
        visitor(stmt);
        match stmt {
            Statement::IfStatement(i) => {
                walk_statements(&i.then_block.statements, visitor);
                for (_, block) in &i.elseif_clauses {
                    walk_statements(&block.statements, visitor);
                }
                if let Some(block) = &i.else_block {
                    walk_statements(&block.statements, visitor);
                }
            }
            Statement::WhileStatement(w) => {
                walk_statements(&w.body.statements, visitor);
            }
            Statement::RepeatStatement(r) => {
                walk_statements(&r.body.statements, visitor);
            }
            Statement::ForNumeric(f) => {
                walk_statements(&f.body.statements, visitor);
            }
            Statement::ForGeneric(f) => {
                walk_statements(&f.body.statements, visitor);
            }
            Statement::DoBlock(b) => {
                walk_statements(&b.statements, visitor);
            }
            Statement::FunctionDecl(f) => {
                walk_statements(&f.body.statements, visitor);
            }
            Statement::ClassDecl(decl) => {
                for member in &decl.members {
                    match member {
                        ClassMember::Method(m) => {
                            if let Some(body) = &m.body {
                                walk_statements(&body.statements, visitor);
                            }
                        }
                        ClassMember::Constructor(c) => {
                            walk_statements(&c.body.statements, visitor);
                        }
                        ClassMember::Property(p) => {
                            if let Some(ref getter) = p.getter {
                                walk_statements(&getter.statements, visitor);
                            }
                            if let Some((_, ref setter)) = p.setter {
                                walk_statements(&setter.statements, visitor);
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
}

fn walk_exprs_in_stmt(stmt: &Statement, visitor: &mut dyn FnMut(&Expression)) {
    match stmt {
        Statement::ExpressionStatement(expr) => walk_expr(expr, visitor),
        Statement::LocalAssignment(la) => {
            for val in &la.values {
                walk_expr(val, visitor);
            }
        }
        Statement::Assignment(a) => {
            for target in &a.targets {
                walk_expr(target, visitor);
            }
            for val in &a.values {
                walk_expr(val, visitor);
            }
        }
        Statement::CompoundAssignment(ca) => {
            walk_expr(&ca.target, visitor);
            walk_expr(&ca.value, visitor);
        }
        Statement::ReturnStatement(r) => {
            for val in &r.values {
                walk_expr(val, visitor);
            }
        }
        Statement::IfStatement(i) => {
            walk_expr(&i.condition, visitor);
        }
        Statement::WhileStatement(w) => {
            walk_expr(&w.condition, visitor);
        }
        Statement::RepeatStatement(r) => {
            walk_expr(&r.condition, visitor);
        }
        Statement::ForNumeric(f) => {
            walk_expr(&f.start, visitor);
            walk_expr(&f.stop, visitor);
            if let Some(ref step) = f.step {
                walk_expr(step, visitor);
            }
        }
        Statement::ForGeneric(f) => {
            for iter in &f.iterators {
                walk_expr(iter, visitor);
            }
        }
        _ => {}
    }
}

fn walk_expr(expr: &Expression, visitor: &mut dyn FnMut(&Expression)) {
    visitor(expr);
    match expr {
        Expression::BinaryOp(bo) => {
            walk_expr(&bo.left, visitor);
            walk_expr(&bo.right, visitor);
        }
        Expression::UnaryOp(uo) => {
            walk_expr(&uo.operand, visitor);
        }
        Expression::FunctionCall(fc) => {
            walk_expr(&fc.callee, visitor);
            for arg in &fc.args {
                walk_expr(arg, visitor);
            }
        }
        Expression::MethodCall(mc) => {
            walk_expr(&mc.object, visitor);
            for arg in &mc.args {
                walk_expr(arg, visitor);
            }
        }
        Expression::FieldAccess(fa) => {
            walk_expr(&fa.object, visitor);
        }
        Expression::IndexAccess(ia) => {
            walk_expr(&ia.object, visitor);
            walk_expr(&ia.index, visitor);
        }
        Expression::TableConstructor(tc) => {
            for field in &tc.fields {
                match field {
                    luao_parser::TableField::NamedField(_, val, _) => walk_expr(val, visitor),
                    luao_parser::TableField::IndexField(k, v, _) => {
                        walk_expr(k, visitor);
                        walk_expr(v, visitor);
                    }
                    luao_parser::TableField::ValueField(val, _) => walk_expr(val, visitor),
                }
            }
        }
        Expression::NewExpr(ne) => {
            for arg in &ne.args {
                walk_expr(arg, visitor);
            }
        }
        Expression::CastExpr(ce) => {
            walk_expr(&ce.expr, visitor);
        }
        Expression::Instanceof(ie) => {
            walk_expr(&ie.object, visitor);
        }
        Expression::IfExpression(ie) => {
            walk_expr(&ie.condition, visitor);
            walk_expr(&ie.then_expr, visitor);
            for (cond, expr) in &ie.elseif_clauses {
                walk_expr(cond, visitor);
                walk_expr(expr, visitor);
            }
            walk_expr(&ie.else_expr, visitor);
        }
        Expression::FunctionExpr(fe) => {
            // Don't walk into function bodies here — handled separately
            let _ = fe;
        }
        Expression::YieldExpr(ye) => {
            if let Some(ref val) = ye.value {
                walk_expr(val, visitor);
            }
        }
        Expression::AwaitExpr(ae) => {
            walk_expr(&ae.expr, visitor);
        }
        _ => {}
    }
}

// =============================================================================
// E021: yield outside generator function
// =============================================================================

pub fn check_yield_outside_generator(
    file: &SourceFile,
    _symbols: &SymbolTable,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    // Check top-level statements (not inside any function)
    check_yield_in_block(&file.statements, false, &mut diagnostics);
    diagnostics
}

fn check_yield_in_block(stmts: &[Statement], in_generator: bool, diagnostics: &mut Vec<Diagnostic>) {
    for stmt in stmts {
        check_yield_in_stmt(stmt, in_generator, diagnostics);
    }
}

fn check_yield_in_stmt(stmt: &Statement, in_generator: bool, diagnostics: &mut Vec<Diagnostic>) {
    match stmt {
        Statement::FunctionDecl(fd) => {
            // New function scope — check body with its own is_generator
            check_yield_in_block(&fd.body.statements, fd.is_generator, diagnostics);
        }
        Statement::ClassDecl(cd) => {
            for member in &cd.members {
                match member {
                    ClassMember::Method(m) => {
                        if let Some(ref body) = m.body {
                            check_yield_in_block(&body.statements, m.is_generator, diagnostics);
                        }
                    }
                    ClassMember::Constructor(c) => {
                        check_yield_in_block(&c.body.statements, false, diagnostics);
                    }
                    ClassMember::Property(p) => {
                        if let Some(ref getter) = p.getter {
                            check_yield_in_block(&getter.statements, false, diagnostics);
                        }
                        if let Some((_, ref setter)) = p.setter {
                            check_yield_in_block(&setter.statements, false, diagnostics);
                        }
                    }
                    _ => {}
                }
            }
        }
        Statement::ExportDecl(inner, _) => {
            check_yield_in_stmt(inner, in_generator, diagnostics);
        }
        _ => {
            // Check expressions in this statement for yield
            check_yield_in_stmt_exprs(stmt, in_generator, diagnostics);
            // Recurse into sub-blocks (if, while, for, etc.)
            match stmt {
                Statement::IfStatement(i) => {
                    check_yield_in_block(&i.then_block.statements, in_generator, diagnostics);
                    for (_, block) in &i.elseif_clauses {
                        check_yield_in_block(&block.statements, in_generator, diagnostics);
                    }
                    if let Some(ref block) = i.else_block {
                        check_yield_in_block(&block.statements, in_generator, diagnostics);
                    }
                }
                Statement::WhileStatement(w) => check_yield_in_block(&w.body.statements, in_generator, diagnostics),
                Statement::RepeatStatement(r) => check_yield_in_block(&r.body.statements, in_generator, diagnostics),
                Statement::ForNumeric(f) => check_yield_in_block(&f.body.statements, in_generator, diagnostics),
                Statement::ForGeneric(f) => check_yield_in_block(&f.body.statements, in_generator, diagnostics),
                Statement::DoBlock(b) => check_yield_in_block(&b.statements, in_generator, diagnostics),
                _ => {}
            }
        }
    }
}

fn check_yield_in_stmt_exprs(stmt: &Statement, in_generator: bool, diagnostics: &mut Vec<Diagnostic>) {
    walk_exprs_in_stmt(stmt, &mut |expr| {
        check_yield_in_expr(expr, in_generator, diagnostics);
    });
}

fn check_yield_in_expr(expr: &Expression, in_generator: bool, diagnostics: &mut Vec<Diagnostic>) {
    if let Expression::YieldExpr(ye) = expr {
        if !in_generator {
            diagnostics.push(Diagnostic::error(
                "'yield' can only be used inside a generator function".to_string(),
                ye.span,
                "E021",
            ));
        }
    }
    // Don't recurse into nested function expressions — they have their own scope
}

// =============================================================================
// E022: await outside async function
// =============================================================================

pub fn check_await_outside_async(
    file: &SourceFile,
    _symbols: &SymbolTable,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    check_await_in_block(&file.statements, false, &mut diagnostics);
    diagnostics
}

fn check_await_in_block(stmts: &[Statement], in_async: bool, diagnostics: &mut Vec<Diagnostic>) {
    for stmt in stmts {
        check_await_in_stmt(stmt, in_async, diagnostics);
    }
}

fn check_await_in_stmt(stmt: &Statement, in_async: bool, diagnostics: &mut Vec<Diagnostic>) {
    match stmt {
        Statement::FunctionDecl(fd) => {
            check_await_in_block(&fd.body.statements, fd.is_async, diagnostics);
        }
        Statement::ClassDecl(cd) => {
            for member in &cd.members {
                match member {
                    ClassMember::Method(m) => {
                        if let Some(ref body) = m.body {
                            check_await_in_block(&body.statements, m.is_async, diagnostics);
                        }
                    }
                    ClassMember::Constructor(c) => {
                        check_await_in_block(&c.body.statements, false, diagnostics);
                    }
                    ClassMember::Property(p) => {
                        if let Some(ref getter) = p.getter {
                            check_await_in_block(&getter.statements, false, diagnostics);
                        }
                        if let Some((_, ref setter)) = p.setter {
                            check_await_in_block(&setter.statements, false, diagnostics);
                        }
                    }
                    _ => {}
                }
            }
        }
        Statement::ExportDecl(inner, _) => {
            check_await_in_stmt(inner, in_async, diagnostics);
        }
        _ => {
            check_await_in_stmt_exprs(stmt, in_async, diagnostics);
            match stmt {
                Statement::IfStatement(i) => {
                    check_await_in_block(&i.then_block.statements, in_async, diagnostics);
                    for (_, block) in &i.elseif_clauses {
                        check_await_in_block(&block.statements, in_async, diagnostics);
                    }
                    if let Some(ref block) = i.else_block {
                        check_await_in_block(&block.statements, in_async, diagnostics);
                    }
                }
                Statement::WhileStatement(w) => check_await_in_block(&w.body.statements, in_async, diagnostics),
                Statement::RepeatStatement(r) => check_await_in_block(&r.body.statements, in_async, diagnostics),
                Statement::ForNumeric(f) => check_await_in_block(&f.body.statements, in_async, diagnostics),
                Statement::ForGeneric(f) => check_await_in_block(&f.body.statements, in_async, diagnostics),
                Statement::DoBlock(b) => check_await_in_block(&b.statements, in_async, diagnostics),
                _ => {}
            }
        }
    }
}

fn check_await_in_stmt_exprs(stmt: &Statement, in_async: bool, diagnostics: &mut Vec<Diagnostic>) {
    walk_exprs_in_stmt(stmt, &mut |expr| {
        check_await_in_expr(expr, in_async, diagnostics);
    });
}

fn check_await_in_expr(expr: &Expression, in_async: bool, diagnostics: &mut Vec<Diagnostic>) {
    if let Expression::AwaitExpr(ae) = expr {
        if !in_async {
            diagnostics.push(Diagnostic::error(
                "'await' can only be used inside an async function".to_string(),
                ae.span,
                "E022",
            ));
        }
    }
}

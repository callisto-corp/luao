use std::collections::HashMap;

use luao_parser::{
    AccessModifier, Block, ClassMember, Expression, SourceFile, Statement, TypeAnnotation, TypeKind,
};
use luao_resolver::SymbolTable;

use crate::diagnostic::Diagnostic;

pub fn check_abstract_instantiation(
    file: &SourceFile,
    symbols: &SymbolTable,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for stmt in &file.statements {
        collect_abstract_instantiation(stmt, symbols, &mut diagnostics);
    }
    diagnostics
}

fn collect_abstract_instantiation(
    stmt: &Statement,
    symbols: &SymbolTable,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match stmt {
        Statement::ClassDecl(decl) => {
            for member in &decl.members {
                if let ClassMember::Method(m) = member {
                    if let Some(body) = &m.body {
                        check_exprs_in_block_for_abstract_new(body, symbols, diagnostics);
                    }
                }
                if let ClassMember::Constructor(c) = member {
                    check_exprs_in_block_for_abstract_new(&c.body, symbols, diagnostics);
                }
            }
        }
        Statement::FunctionDecl(f) => {
            check_exprs_in_block_for_abstract_new(&f.body, symbols, diagnostics);
        }
        Statement::IfStatement(i) => {
            check_exprs_in_block_for_abstract_new(&i.then_block, symbols, diagnostics);
            for (_, block) in &i.elseif_clauses {
                check_exprs_in_block_for_abstract_new(block, symbols, diagnostics);
            }
            if let Some(block) = &i.else_block {
                check_exprs_in_block_for_abstract_new(block, symbols, diagnostics);
            }
        }
        Statement::WhileStatement(w) => {
            check_exprs_in_block_for_abstract_new(&w.body, symbols, diagnostics);
        }
        Statement::DoBlock(b) => {
            check_exprs_in_block_for_abstract_new(b, symbols, diagnostics);
        }
        _ => {}
    }
}

fn check_exprs_in_block_for_abstract_new(
    block: &Block,
    symbols: &SymbolTable,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for stmt in &block.statements {
        check_expr_stmt_for_abstract_new(stmt, symbols, diagnostics);
    }
}

fn check_expr_stmt_for_abstract_new(
    stmt: &Statement,
    symbols: &SymbolTable,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match stmt {
        Statement::ExpressionStatement(expr) => {
            check_expr_for_abstract_new(expr, symbols, diagnostics);
        }
        Statement::LocalAssignment(la) => {
            for val in &la.values {
                check_expr_for_abstract_new(val, symbols, diagnostics);
            }
        }
        Statement::Assignment(a) => {
            for val in &a.values {
                check_expr_for_abstract_new(val, symbols, diagnostics);
            }
        }
        Statement::ReturnStatement(r) => {
            for val in &r.values {
                check_expr_for_abstract_new(val, symbols, diagnostics);
            }
        }
        _ => {
            collect_abstract_instantiation(stmt, symbols, diagnostics);
        }
    }
}

fn check_expr_for_abstract_new(
    expr: &Expression,
    symbols: &SymbolTable,
    diagnostics: &mut Vec<Diagnostic>,
) {
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
}

pub fn check_abstract_methods(
    _file: &SourceFile,
    symbols: &SymbolTable,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for cls in symbols.classes.values() {
        if cls.is_abstract {
            continue;
        }
        if let Some(parent_name) = &cls.parent {
            if let Some(parent) = symbols.lookup_class(parent_name) {
                for parent_method in &parent.methods {
                    if parent_method.is_abstract {
                        let implemented = cls.methods.iter().any(|m| m.name == parent_method.name);
                        if !implemented {
                            diagnostics.push(Diagnostic::error(
                                format!(
                                    "class '{}' must implement abstract method '{}' from '{}'",
                                    cls.name, parent_method.name, parent_name
                                ),
                                luao_lexer::Span::empty(),
                                "E002",
                            ));
                        }
                    }
                }
            }
        }
    }
    diagnostics
}

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
                                "class '{}' cannot extend sealed class '{}'",
                                cls.name, parent_name
                            ),
                            luao_lexer::Span::empty(),
                            "E003",
                        ));
                    }
                }
            }
        }
    }
    diagnostics
}

pub fn check_interface_conformance(
    _file: &SourceFile,
    symbols: &SymbolTable,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for cls in symbols.classes.values() {
        for iface_name in &cls.interfaces {
            if let Some(iface) = symbols.lookup_interface(iface_name) {
                for iface_method in &iface.methods {
                    let implemented = cls.methods.iter().any(|m| m.name == iface_method.name);
                    if !implemented {
                        diagnostics.push(Diagnostic::error(
                            format!(
                                "class '{}' must implement method '{}' from interface '{}'",
                                cls.name, iface_method.name, iface_name
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

pub fn check_readonly_assignments(
    file: &SourceFile,
    symbols: &SymbolTable,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for stmt in &file.statements {
        if let Statement::ClassDecl(decl) = stmt {
            let class_name = decl.name.name.as_str();
            for member in &decl.members {
                if let ClassMember::Method(m) = member {
                    if let Some(body) = &m.body {
                        check_readonly_in_block(body, class_name, symbols, false, &mut diagnostics);
                    }
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
    _in_constructor: bool,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for stmt in &block.statements {
        if let Statement::Assignment(a) = stmt {
            for target in &a.targets {
                if let Expression::FieldAccess(fa) = target {
                    let field_name = fa.field.name.as_str();
                    if let Some(cls) = symbols.lookup_class(class_name) {
                        if let Some(field) = cls.fields.iter().find(|f| f.name == field_name) {
                            if field.is_readonly {
                                diagnostics.push(Diagnostic::error(
                                    format!(
                                        "cannot assign to readonly field '{}' outside constructor",
                                        field_name
                                    ),
                                    fa.span,
                                    "E005",
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
}

pub fn check_access_modifiers(
    file: &SourceFile,
    symbols: &SymbolTable,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for stmt in &file.statements {
        check_access_in_statement(stmt, None, symbols, &mut diagnostics);
    }
    diagnostics
}

fn check_access_in_statement(
    stmt: &Statement,
    current_class: Option<&str>,
    symbols: &SymbolTable,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match stmt {
        Statement::ClassDecl(decl) => {
            let class_name = decl.name.name.as_str();
            for member in &decl.members {
                match member {
                    ClassMember::Method(m) => {
                        if let Some(body) = &m.body {
                            check_access_in_block(body, Some(class_name), symbols, diagnostics);
                        }
                    }
                    ClassMember::Constructor(c) => {
                        check_access_in_block(&c.body, Some(class_name), symbols, diagnostics);
                    }
                    ClassMember::Property(p) => {
                        if let Some(ref getter) = p.getter {
                            check_access_in_block(getter, Some(class_name), symbols, diagnostics);
                        }
                        if let Some((_, ref setter)) = p.setter {
                            check_access_in_block(setter, Some(class_name), symbols, diagnostics);
                        }
                    }
                    _ => {}
                }
            }
        }
        Statement::FunctionDecl(f) => {
            check_access_in_block(&f.body, current_class, symbols, diagnostics);
        }
        Statement::ExpressionStatement(expr) => {
            check_access_in_expr(expr, current_class, symbols, diagnostics);
        }
        Statement::LocalAssignment(la) => {
            for val in &la.values {
                check_access_in_expr(val, current_class, symbols, diagnostics);
            }
        }
        Statement::Assignment(a) => {
            for val in &a.values {
                check_access_in_expr(val, current_class, symbols, diagnostics);
            }
            for target in &a.targets {
                check_access_in_expr(target, current_class, symbols, diagnostics);
            }
        }
        _ => {}
    }
}

fn check_access_in_block(
    block: &Block,
    current_class: Option<&str>,
    symbols: &SymbolTable,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for stmt in &block.statements {
        check_access_in_statement(stmt, current_class, symbols, diagnostics);
    }
}

fn check_access_in_expr(
    expr: &Expression,
    current_class: Option<&str>,
    symbols: &SymbolTable,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if let Expression::FieldAccess(fa) = expr {
        let field_name = fa.field.name.as_str();

        // Determine which class the field access is on
        let target_class = match &fa.object {
            Expression::Identifier(id) if id.name.as_str() == "self" => current_class,
            Expression::Identifier(id) => {
                // ClassName.field — check if it's a known class
                let name = id.name.as_str();
                if symbols.classes.contains_key(name) {
                    Some(name)
                } else {
                    None // Unknown object, can't check
                }
            }
            _ => None,
        };

        if let Some(target) = target_class {
            if let Some(cls) = symbols.lookup_class(target) {
                if let Some(field) = cls.fields.iter().find(|f| f.name == field_name) {
                    match field.access {
                        AccessModifier::Private => {
                            if current_class != Some(cls.name.as_str()) {
                                diagnostics.push(Diagnostic::error(
                                    format!(
                                        "cannot access private field '{}' of class '{}'",
                                        field_name, cls.name
                                    ),
                                    fa.span,
                                    "E006",
                                ));
                            }
                        }
                        AccessModifier::Protected => {
                            let is_self = current_class == Some(cls.name.as_str());
                            let is_subclass = current_class.map_or(false, |cur| {
                                symbols
                                    .lookup_class(cur)
                                    .and_then(|c| c.parent.as_deref())
                                    .map_or(false, |p| p == cls.name)
                            });
                            if !is_self && !is_subclass {
                                diagnostics.push(Diagnostic::error(
                                    format!(
                                        "cannot access protected field '{}' of class '{}'",
                                        field_name, cls.name
                                    ),
                                    fa.span,
                                    "E006",
                                ));
                            }
                        }
                        AccessModifier::Public => {}
                    }
                }
            }
        }
    }
}

pub fn check_override_validity(
    _file: &SourceFile,
    symbols: &SymbolTable,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for cls in symbols.classes.values() {
        for method in &cls.methods {
            if method.is_override {
                let has_parent_method = cls
                    .parent
                    .as_ref()
                    .and_then(|pn| symbols.lookup_class(pn))
                    .map_or(false, |parent| {
                        parent.methods.iter().any(|m| m.name == method.name)
                    });
                if !has_parent_method {
                    diagnostics.push(Diagnostic::error(
                        format!(
                            "method '{}' in class '{}' is marked override but does not override a parent method",
                            method.name, cls.name
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
    for stmt in &block.statements {
        check_super_in_statement(stmt, has_parent, class_name, diagnostics);
    }
}

fn check_super_in_statement(
    stmt: &Statement,
    has_parent: bool,
    class_name: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match stmt {
        Statement::ExpressionStatement(expr) => {
            check_super_in_expr(expr, has_parent, class_name, diagnostics);
        }
        Statement::LocalAssignment(la) => {
            for val in &la.values {
                check_super_in_expr(val, has_parent, class_name, diagnostics);
            }
        }
        Statement::Assignment(a) => {
            for val in &a.values {
                check_super_in_expr(val, has_parent, class_name, diagnostics);
            }
        }
        Statement::ReturnStatement(r) => {
            for val in &r.values {
                check_super_in_expr(val, has_parent, class_name, diagnostics);
            }
        }
        Statement::IfStatement(i) => {
            check_super_in_block(&i.then_block, has_parent, class_name, diagnostics);
            for (_, block) in &i.elseif_clauses {
                check_super_in_block(block, has_parent, class_name, diagnostics);
            }
            if let Some(block) = &i.else_block {
                check_super_in_block(block, has_parent, class_name, diagnostics);
            }
        }
        Statement::WhileStatement(w) => {
            check_super_in_block(&w.body, has_parent, class_name, diagnostics);
        }
        Statement::DoBlock(b) => {
            check_super_in_block(b, has_parent, class_name, diagnostics);
        }
        _ => {}
    }
}

fn check_super_in_expr(
    expr: &Expression,
    has_parent: bool,
    class_name: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match expr {
        Expression::SuperAccess(sa) => {
            if !has_parent {
                diagnostics.push(Diagnostic::error(
                    format!(
                        "cannot use 'super' in class '{}' which has no parent class",
                        class_name
                    ),
                    sa.span,
                    "E008",
                ));
            }
        }
        Expression::MethodCall(mc) => {
            check_super_in_expr(&mc.object, has_parent, class_name, diagnostics);
            for arg in &mc.args {
                check_super_in_expr(arg, has_parent, class_name, diagnostics);
            }
        }
        Expression::FunctionCall(fc) => {
            check_super_in_expr(&fc.callee, has_parent, class_name, diagnostics);
            for arg in &fc.args {
                check_super_in_expr(arg, has_parent, class_name, diagnostics);
            }
        }
        Expression::BinaryOp(bo) => {
            check_super_in_expr(&bo.left, has_parent, class_name, diagnostics);
            check_super_in_expr(&bo.right, has_parent, class_name, diagnostics);
        }
        Expression::UnaryOp(uo) => {
            check_super_in_expr(&uo.operand, has_parent, class_name, diagnostics);
        }
        Expression::FieldAccess(fa) => {
            check_super_in_expr(&fa.object, has_parent, class_name, diagnostics);
        }
        _ => {}
    }
}

// --- E014: Import name shadowing ---

pub fn check_import_shadowing(
    file: &SourceFile,
    _symbols: &SymbolTable,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    // Collect all imported names (including aliases)
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

    // Check top-level declarations for shadowing
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
                // Check inside export too
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

// --- E013: Union type member access ---

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

/// Tracks which variable names have union type annotations.
struct UnionEnv {
    /// variable name -> true if its declared type is a union
    union_vars: HashMap<String, bool>,
}

impl UnionEnv {
    fn new() -> Self {
        Self {
            union_vars: HashMap::new(),
        }
    }

    fn register(&mut self, name: &str, ty: Option<&TypeAnnotation>) {
        let is_union = ty.map_or(false, |t| is_union_type(&t.kind));
        self.union_vars.insert(name.to_string(), is_union);
    }

    fn is_union(&self, name: &str) -> bool {
        self.union_vars.get(name).copied().unwrap_or(false)
    }
}

fn is_union_type(kind: &TypeKind) -> bool {
    matches!(kind, TypeKind::Union(_))
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
                        // Copy outer scope
                        inner_env.union_vars = env.union_vars.clone();
                        // Register params
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
            // Check if the object is a bare identifier with a union type
            if is_union_object(&fa.object, env) {
                diagnostics.push(Diagnostic::error(
                    format!(
                        "cannot access member '{}' on a union type; use 'as' to cast to a specific type first",
                        fa.field.name
                    ),
                    fa.span,
                    "E013",
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
                    "E013",
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
            // The inner expression is being cast — don't check it for union access
            // (that's the whole point of casting). But recurse into nested exprs.
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

/// Returns true if the expression is a variable reference with a union type.
/// CastExpr is explicitly NOT union — that's how you narrow.
fn is_union_object(expr: &Expression, env: &UnionEnv) -> bool {
    match expr {
        Expression::Identifier(id) => env.is_union(&id.name),
        // A cast narrows the type — not a union anymore
        Expression::CastExpr(_) => false,
        _ => false,
    }
}

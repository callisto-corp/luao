use luao_parser::{
    AccessModifier, Block, ClassMember, Expression, SourceFile, Statement,
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
        for cls in symbols.classes.values() {
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
                break;
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

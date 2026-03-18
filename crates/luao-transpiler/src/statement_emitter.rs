use luao_parser::Statement;

use crate::class_emitter;
use crate::emitter::Emitter;
use crate::enum_emitter;
use crate::expression_emitter::emit_expression;

pub fn emit_statement(emitter: &mut Emitter, stmt: &Statement) {
    match stmt {
        Statement::LocalAssignment(la) => {
            let names: Vec<_> = la.names.iter().map(|n| emitter.rename_decl(&n.name)).collect();

            // Track variable types — first explicit type wins, then infer from RHS
            for (i, name_id) in la.names.iter().enumerate() {
                let var_name = emitter.rename_decl(&name_id.name);
                // 1. Explicit type annotation: local x: Foo
                if let Some(Some(ta)) = la.type_annotations.get(i) {
                    if let Some(tn) = resolve_type_name(emitter, ta) {
                        emitter.local_var_types.insert(var_name, tn);
                        continue;
                    }
                }
                // 2. Infer from RHS: cast, new, or method return type
                if let Some(val) = la.values.get(i) {
                    if let Some(inferred) = infer_type_from_expr(emitter, val) {
                        emitter.local_var_types.insert(var_name, inferred);
                    }
                }
            }

            // If all names are exported (forward-declared), skip `local`
            let all_exported = names.iter().all(|n| emitter.is_exported(n));
            let prefix = if all_exported { "" } else { "local " };
            if la.values.is_empty() {
                if !all_exported {
                    emitter.writeln(&format!("{}{}", prefix, names.join(", ")));
                }
            } else {
                // Set table_target_type for table constructors assigned to typed vars
                let mut values = Vec::new();
                for (idx, v) in la.values.iter().enumerate() {
                    // If this value is a table constructor and the var has a known type, set context
                    let var_name = la.names.get(idx).map(|n| emitter.rename_decl(&n.name));
                    let saved = emitter.table_target_type.take();
                    if let Some(ref vn) = var_name {
                        if let Some(type_name) = emitter.local_var_types.get(vn).cloned() {
                            if matches!(v, luao_parser::Expression::TableConstructor(_)) {
                                emitter.table_target_type = Some(type_name);
                            }
                        }
                    }
                    values.push(emit_expression(emitter, v));
                    emitter.table_target_type = saved;
                }
                emitter.writeln(&format!(
                    "{}{} = {}",
                    prefix,
                    names.join(", "),
                    values.join(", ")
                ));
            }
        }
        Statement::Assignment(assign) => {
            // Check for property setter: self.prop = val → self:__set_prop(val)
            if assign.targets.len() == 1 && assign.values.len() == 1 {
                if let luao_parser::Expression::FieldAccess(fa) = &assign.targets[0] {
                    if let luao_parser::Expression::Identifier(id) = &fa.object {
                        if id.name.as_str() == "self" {
                            if let Some(class_name) = emitter.current_class.clone() {
                                let prop_key = (class_name, fa.field.name.to_string());
                                if let Some(setter_method) = emitter.property_setters.get(&prop_key).cloned() {
                                    let val = emit_expression(emitter, &assign.values[0]);
                                    emitter.writeln(&format!("self:{}({})", setter_method, val));
                                    return;
                                }
                            }
                        }
                    }
                }
            }
            let targets: Vec<_> = assign
                .targets
                .iter()
                .map(|t| emit_expression(emitter, t))
                .collect();
            let values: Vec<_> = assign
                .values
                .iter()
                .map(|v| emit_expression(emitter, v))
                .collect();
            emitter.writeln(&format!("{} = {}", targets.join(", "), values.join(", ")));

            // Track types from assignment: x = new Foo()
            for (i, target) in assign.targets.iter().enumerate() {
                if let luao_parser::Expression::Identifier(id) = target {
                    if let Some(val) = assign.values.get(i) {
                        if let Some(inferred) = infer_type_from_expr(emitter, val) {
                            let var_name = emitter.rename(&id.name);
                            emitter.local_var_types.insert(var_name, inferred);
                        }
                    }
                }
            }
        }
        Statement::CompoundAssignment(ca) => {
            let target = emit_expression(emitter, &ca.target);
            let value = emit_expression(emitter, &ca.value);
            let op = match ca.op {
                luao_parser::CompoundOp::Add => "+",
                luao_parser::CompoundOp::Sub => "-",
                luao_parser::CompoundOp::Mul => "*",
                luao_parser::CompoundOp::Div => "/",
                luao_parser::CompoundOp::Mod => "%",
                luao_parser::CompoundOp::Pow => "^",
                luao_parser::CompoundOp::Concat => "..",
            };
            emitter.writeln(&format!("{} = {} {} {}", target, target, op, value));
        }
        Statement::FunctionDecl(fd) => {
            let name = emit_function_name(emitter, fd);
            let params = emitter.emit_params(&fd.params);
            // Track parameter types
            track_param_types(emitter, &fd.params);
            if fd.is_local && !emitter.is_exported(&name) {
                emitter.writeln(&format!("local function {}({})", name, params));
            } else {
                emitter.writeln(&format!("function {}({})", name, params));
            }
            emitter.emit_block(&fd.body);
            emitter.writeln("end");
        }
        Statement::IfStatement(if_stmt) => {
            let cond = emit_expression(emitter, &if_stmt.condition);
            emitter.writeln(&format!("if {} then", cond));
            emitter.emit_block(&if_stmt.then_block);
            for (elseif_cond, elseif_block) in &if_stmt.elseif_clauses {
                let c = emit_expression(emitter, elseif_cond);
                emitter.writeln(&format!("elseif {} then", c));
                emitter.emit_block(elseif_block);
            }
            if let Some(else_block) = &if_stmt.else_block {
                emitter.writeln("else");
                emitter.emit_block(else_block);
            }
            emitter.writeln("end");
        }
        Statement::WhileStatement(ws) => {
            let cond = emit_expression(emitter, &ws.condition);
            emitter.writeln(&format!("while {} do", cond));
            emitter.emit_block(&ws.body);
            emitter.writeln("end");
        }
        Statement::RepeatStatement(rs) => {
            emitter.writeln("repeat");
            emitter.emit_block(&rs.body);
            let cond = emit_expression(emitter, &rs.condition);
            emitter.writeln(&format!("until {}", cond));
        }
        Statement::ForNumeric(f) => {
            let start = emit_expression(emitter, &f.start);
            let stop = emit_expression(emitter, &f.stop);
            if let Some(step) = &f.step {
                let step_str = emit_expression(emitter, step);
                emitter.writeln(&format!(
                    "for {} = {}, {}, {} do",
                    f.name.name, start, stop, step_str
                ));
            } else {
                emitter.writeln(&format!("for {} = {}, {} do", f.name.name, start, stop));
            }
            emitter.emit_block(&f.body);
            emitter.writeln("end");
        }
        Statement::ForGeneric(f) => {
            let names: Vec<_> = f.names.iter().map(|n| n.name.to_string()).collect();
            let iters: Vec<_> = f
                .iterators
                .iter()
                .map(|i| emit_expression(emitter, i))
                .collect();
            emitter.writeln(&format!(
                "for {} in {} do",
                names.join(", "),
                iters.join(", ")
            ));
            emitter.emit_block(&f.body);
            emitter.writeln("end");
        }
        Statement::DoBlock(block) => {
            emitter.writeln("do");
            emitter.emit_block(block);
            emitter.writeln("end");
        }
        Statement::ReturnStatement(ret) => {
            if ret.values.is_empty() {
                emitter.writeln("return");
            } else {
                let values: Vec<_> = ret
                    .values
                    .iter()
                    .map(|v| emit_expression(emitter, v))
                    .collect();
                emitter.writeln(&format!("return {}", values.join(", ")));
            }
        }
        Statement::Break(_) => {
            emitter.writeln("break");
        }
        Statement::Continue(_) => {
            emitter.writeln("continue");
        }
        Statement::TypeAlias(_) => {
            // Type aliases are erased at compile time
        }
        Statement::ImportDecl(_) => {
            // Imports are handled by the bundler; in non-bundled builds, they're erased
        }
        Statement::ExportDecl(inner, _) => {
            // In non-bundled builds, just emit the inner statement
            emitter.emit_statement(inner);
        }
        Statement::ExpressionStatement(expr) => {
            let e = emit_expression(emitter, expr);
            emitter.writeln(&e);
        }
        Statement::ClassDecl(class) => {
            class_emitter::emit_class(emitter, class);
        }
        Statement::InterfaceDecl(_) => {}
        Statement::EnumDecl(enum_decl) => {
            enum_emitter::emit_enum(emitter, enum_decl);
        }
    }
}

/// Track parameter types from type annotations into the emitter's local_var_types map.
fn track_param_types(emitter: &mut Emitter, params: &[luao_parser::Parameter]) {
    for param in params {
        if param.is_vararg { continue; }
        if let Some(ref ta) = param.type_annotation {
            if let Some(tn) = resolve_type_name(emitter, ta) {
                emitter.local_var_types.insert(param.name.name.to_string(), tn);
            }
        }
    }
}

fn emit_function_name(emitter: &Emitter, fd: &luao_parser::FunctionDecl) -> String {
    let parts: Vec<String> = fd
        .name
        .parts
        .iter()
        .enumerate()
        .map(|(i, p)| {
            if i == 0 {
                emitter.rename(&p.name)
            } else {
                p.name.to_string()
            }
        })
        .collect();
    let mut name = parts.join(".");
    if let Some(method) = &fd.name.method {
        name.push(':');
        name.push_str(&method.name);
    }
    name
}

/// Resolve a type annotation to a class/interface name if it refers to a known type.
fn resolve_type_name(emitter: &Emitter, ta: &luao_parser::TypeAnnotation) -> Option<String> {
    if let luao_parser::TypeKind::Named(ref type_name, _) = ta.kind {
        let tn = type_name.name.to_string();
        if emitter.is_type(&tn) {
            return Some(tn);
        }
    }
    None
}

/// Infer the class type from an expression. Checks in priority order:
/// 1. Cast: `expr as Foo`
/// 2. New: `new Foo()`
/// 3. Method call return type: `obj:method()` where method returns a known class
fn infer_type_from_expr(emitter: &Emitter, expr: &luao_parser::Expression) -> Option<String> {
    match expr {
        // `thing as Foo` — cast tells us the type
        luao_parser::Expression::CastExpr(cast) => {
            if let luao_parser::TypeKind::Named(ref type_name, _) = cast.target_type.kind {
                let tn = type_name.name.to_string();
                if emitter.is_class(&tn) {
                    return Some(tn);
                }
            }
            None
        }
        // `new Foo()` — constructor tells us the type
        luao_parser::Expression::NewExpr(ne) => {
            Some(ne.class_name.name.name.to_string())
        }
        // `obj:method()` — check method return type in symbol table
        luao_parser::Expression::MethodCall(mc) => {
            // Resolve the object's type
            let obj_type = match &mc.object {
                luao_parser::Expression::Identifier(id) => {
                    let name = id.name.as_str();
                    if name == "self" {
                        emitter.current_class.clone()
                    } else {
                        emitter.local_var_types.get(name).cloned()
                    }
                }
                _ => None,
            };
            if let Some(class_name) = obj_type {
                if let Some(class) = emitter.symbol_table.classes.get(&class_name) {
                    let method_name = mc.method.name.as_str();
                    for method in &class.methods {
                        if method.name == method_name {
                            if let luao_resolver::LuaoType::Class(class_id) = &method.return_type {
                                for (cname, csym) in &emitter.symbol_table.classes {
                                    if csym.id == *class_id {
                                        return Some(cname.clone());
                                    }
                                }
                            }
                        }
                    }
                }
            }
            None
        }
        // `Foo.staticMethod()` — check static method return type
        luao_parser::Expression::FunctionCall(call) => {
            if let luao_parser::Expression::FieldAccess(fa) = &call.callee {
                if let luao_parser::Expression::Identifier(id) = &fa.object {
                    let class_name = id.name.to_string();
                    if let Some(class) = emitter.symbol_table.classes.get(&class_name) {
                        let method_name = fa.field.name.as_str();
                        for method in &class.methods {
                            if method.name == method_name {
                                if let luao_resolver::LuaoType::Class(class_id) = &method.return_type {
                                    for (cname, csym) in &emitter.symbol_table.classes {
                                        if csym.id == *class_id {
                                            return Some(cname.clone());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            None
        }
        _ => None,
    }
}

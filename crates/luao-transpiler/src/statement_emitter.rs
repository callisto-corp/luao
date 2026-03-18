use luao_parser::Statement;

use crate::class_emitter;
use crate::emitter::Emitter;
use crate::enum_emitter;
use crate::expression_emitter::emit_expression;

pub fn emit_statement(emitter: &mut Emitter, stmt: &Statement) {
    match stmt {
        Statement::LocalAssignment(la) => {
            let names: Vec<_> = la.names.iter().map(|n| emitter.rename_decl(&n.name)).collect();
            // If all names are exported (forward-declared), skip `local`
            let all_exported = names.iter().all(|n| emitter.is_exported(n));
            let prefix = if all_exported { "" } else { "local " };
            if la.values.is_empty() {
                if !all_exported {
                    emitter.writeln(&format!("{}{}", prefix, names.join(", ")));
                }
            } else {
                let values: Vec<_> = la
                    .values
                    .iter()
                    .map(|v| emit_expression(emitter, v))
                    .collect();
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
        }
        Statement::FunctionDecl(fd) => {
            let name = emit_function_name(emitter, fd);
            let params = emitter.emit_params(&fd.params);
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

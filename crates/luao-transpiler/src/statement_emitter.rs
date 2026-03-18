use luao_parser::Statement;

use crate::class_emitter;
use crate::emitter::Emitter;
use crate::enum_emitter;
use crate::expression_emitter::emit_expression;

pub fn emit_statement(emitter: &mut Emitter, stmt: &Statement) {
    match stmt {
        Statement::LocalAssignment(la) => {
            let names: Vec<_> = la.names.iter().map(|n| n.name.to_string()).collect();
            if la.values.is_empty() {
                emitter.writeln(&format!("local {}", names.join(", ")));
            } else {
                let values: Vec<_> = la
                    .values
                    .iter()
                    .map(|v| emit_expression(emitter, v))
                    .collect();
                emitter.writeln(&format!(
                    "local {} = {}",
                    names.join(", "),
                    values.join(", ")
                ));
            }
        }
        Statement::Assignment(assign) => {
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
            let name = emit_function_name(fd);
            let params = emitter.emit_params(&fd.params);
            if fd.is_local {
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

fn emit_function_name(fd: &luao_parser::FunctionDecl) -> String {
    let mut name = fd
        .name
        .parts
        .iter()
        .map(|p| p.name.to_string())
        .collect::<Vec<_>>()
        .join(".");
    if let Some(method) = &fd.name.method {
        name.push(':');
        name.push_str(&method.name);
    }
    name
}

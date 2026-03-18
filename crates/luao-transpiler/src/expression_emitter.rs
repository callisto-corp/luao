use luao_parser::{BinOp, Expression, TableField, UnOp};

use crate::emitter::Emitter;

pub fn emit_expression(emitter: &mut Emitter, expr: &Expression) -> String {
    match expr {
        Expression::Nil(_) => "nil".to_string(),
        Expression::True(_) => "true".to_string(),
        Expression::False(_) => "false".to_string(),
        Expression::Number(n, _) => n.to_string(),
        Expression::String(s, _) => s.to_string(),
        Expression::Vararg(_) => "...".to_string(),
        Expression::Identifier(id) => id.name.to_string(),
        Expression::BinaryOp(bin) => {
            let left = emit_expression(emitter, &bin.left);
            let right = emit_expression(emitter, &bin.right);
            let op = binop_to_lua(&bin.op);
            format!("({} {} {})", left, op, right)
        }
        Expression::UnaryOp(un) => {
            let operand = emit_expression(emitter, &un.operand);
            let op = unop_to_lua(&un.op);
            match un.op {
                UnOp::Not => format!("({} {})", op, operand),
                _ => format!("({}{})", op, operand),
            }
        }
        Expression::FunctionCall(call) => {
            if let Expression::SuperAccess(_) = &call.callee {
                let callee = emit_expression(emitter, &call.callee);
                let args = emit_args(emitter, &call.args);
                if args.is_empty() {
                    format!("{}(self)", callee)
                } else {
                    format!("{}(self, {})", callee, args)
                }
            } else {
                let callee = emit_expression(emitter, &call.callee);
                let args = emit_args(emitter, &call.args);
                format!("{}({})", callee, args)
            }
        }
        Expression::MethodCall(mc) => {
            let object = emit_expression(emitter, &mc.object);
            let args = emit_args(emitter, &mc.args);
            format!("{}:{}({})", object, mc.method.name, args)
        }
        Expression::FieldAccess(fa) => {
            let object = emit_expression(emitter, &fa.object);
            format!("{}.{}", object, fa.field.name)
        }
        Expression::IndexAccess(ia) => {
            let object = emit_expression(emitter, &ia.object);
            let index = emit_expression(emitter, &ia.index);
            format!("{}[{}]", object, index)
        }
        Expression::FunctionExpr(fe) => {
            let params = emitter.emit_params(&fe.params);
            let mut result = format!("function({})\n", params);
            let saved_output = std::mem::take(&mut emitter.output);
            emitter.emit_block(&fe.body);
            let body = std::mem::replace(&mut emitter.output, saved_output);
            result.push_str(&body);
            emitter.write_indent();
            result.push_str("end");
            result
        }
        Expression::TableConstructor(tc) => {
            if tc.fields.is_empty() {
                return "{}".to_string();
            }
            let mut parts = Vec::new();
            for field in &tc.fields {
                match field {
                    TableField::NamedField(name, value, _) => {
                        let val = emit_expression(emitter, value);
                        parts.push(format!("{} = {}", name.name, val));
                    }
                    TableField::IndexField(key, value, _) => {
                        let k = emit_expression(emitter, key);
                        let v = emit_expression(emitter, value);
                        parts.push(format!("[{}] = {}", k, v));
                    }
                    TableField::ValueField(value, _) => {
                        parts.push(emit_expression(emitter, value));
                    }
                }
            }
            format!("{{ {} }}", parts.join(", "))
        }
        Expression::Instanceof(inst) => {
            emitter.needs_instanceof = true;
            let obj = emit_expression(emitter, &inst.object);
            format!("__luao_instanceof({}, {})", obj, inst.class_name.name)
        }
        Expression::SuperAccess(sa) => {
            let parent = emitter
                .current_class_parent
                .clone()
                .unwrap_or_else(|| "super".to_string());
            format!("{}.{}", parent, sa.method.name)
        }
        Expression::NewExpr(ne) => {
            let class_name = ne.class_name.name.name.to_string();
            let args = emit_args(emitter, &ne.args);
            format!("{}._new({})", class_name, args)
        }
    }
}

fn emit_args(emitter: &mut Emitter, args: &[Expression]) -> String {
    args.iter()
        .map(|a| emit_expression(emitter, a))
        .collect::<Vec<_>>()
        .join(", ")
}

fn binop_to_lua(op: &BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::IntDiv => "//",
        BinOp::Mod => "%",
        BinOp::Pow => "^",
        BinOp::Concat => "..",
        BinOp::Eq => "==",
        BinOp::NotEq => "~=",
        BinOp::Lt => "<",
        BinOp::Le => "<=",
        BinOp::Gt => ">",
        BinOp::Ge => ">=",
        BinOp::And => "and",
        BinOp::Or => "or",
        BinOp::BitAnd => "&",
        BinOp::BitOr => "|",
        BinOp::BitXor => "~",
        BinOp::ShiftLeft => "<<",
        BinOp::ShiftRight => ">>",
    }
}

fn unop_to_lua(op: &UnOp) -> &'static str {
    match op {
        UnOp::Neg => "-",
        UnOp::Not => "not",
        UnOp::Len => "#",
        UnOp::BitNot => "~",
    }
}

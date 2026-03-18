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
        Expression::Identifier(id) => emitter.rename(&id.name),
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
            let method_name = maybe_mangle_access(emitter, &mc.object, &mc.method.name);
            let args = emit_args(emitter, &mc.args);
            format!("{}:{}({})", object, method_name, args)
        }
        Expression::FieldAccess(fa) => {
            // Check if this is a property getter access (self.prop where prop has a getter)
            if let Expression::Identifier(id) = &fa.object {
                if id.name.as_str() == "self" {
                    if let Some(class_name) = emitter.current_class.clone() {
                        let prop_key = (class_name, fa.field.name.to_string());
                        if let Some(getter_method) = emitter.property_getters.get(&prop_key).cloned() {
                            let self_name = emitter.rename("self");
                            return format!("{}:{}()", self_name, getter_method);
                        }
                    }
                }
            }
            let object = emit_expression(emitter, &fa.object);
            let field_name = maybe_mangle_access(emitter, &fa.object, &fa.field.name);
            format!("{}.{}", object, field_name)
        }
        Expression::IndexAccess(ia) => {
            let object = emit_expression(emitter, &ia.object);
            let index = emit_expression(emitter, &ia.index);
            format!("{}[{}]", object, index)
        }
        Expression::FunctionExpr(fe) => {
            let params = emitter.emit_params(&fe.params);
            // Track function expression parameter types
            for param in &fe.params {
                if param.is_vararg { continue; }
                if let Some(ref ta) = param.type_annotation {
                    if let luao_parser::TypeKind::Named(ref type_name, _) = ta.kind {
                        let tn = type_name.name.to_string();
                        if emitter.is_class(&tn) {
                            emitter.local_var_types.insert(param.name.name.to_string(), tn);
                        }
                    }
                }
            }
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
                        let field_name = if let Some(target_type) = emitter.table_target_type.clone() {
                            if is_extern_member(&emitter.symbol_table, &target_type, &name.name) {
                                name.name.to_string()
                            } else {
                                emitter.mangle_member(&target_type, &name.name)
                            }
                        } else {
                            name.name.to_string()
                        };
                        parts.push(format!("{} = {}", field_name, val));
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
            let class_name = emitter.rename(&inst.class_name.name);
            format!("__luao_instanceof({}, {})", obj, class_name)
        }
        Expression::SuperAccess(sa) => {
            let parent = emitter
                .current_class_parent
                .clone()
                .unwrap_or_else(|| "super".to_string());
            let method_name = sa.method.name.to_string();
            // Mangle parent class method access (respecting extern)
            if emitter.mangler.is_some()
                && !is_extern_member(&emitter.symbol_table, &parent, &method_name)
            {
                let mangled = emitter.mangle_member(&parent, &method_name);
                format!("{}.{}", parent, mangled)
            } else {
                format!("{}.{}", parent, method_name)
            }
        }
        Expression::NewExpr(ne) => {
            let original_name = ne.class_name.name.name.to_string();
            let class_name = emitter.rename(&original_name);
            let is_extern_class = emitter.symbol_table.classes.get(&original_name)
                .map(|c| c.is_extern)
                .unwrap_or(false);
            let new_name = if is_extern_class { "new".to_string() } else { emitter.mangle_shared("new") };
            let args = emit_args(emitter, &ne.args);
            format!("{}.{}({})", class_name, new_name, args)
        }
        Expression::CastExpr(cast) => {
            // Cast is erased at compile time — just emit the inner expression
            emit_expression(emitter, &cast.expr)
        }
        Expression::IfExpression(ie) => {
            // Luau if-expression → Lua ternary idiom using `and`/`or` for simple cases,
            // or an IIFE for complex cases with elseif.
            // For safety, always use IIFE: (function() if cond then return expr [elseif...] else return expr end end)()
            let mut result = String::from("(function()");
            let cond = emit_expression(emitter, &ie.condition);
            let then_val = emit_expression(emitter, &ie.then_expr);
            result.push_str(&format!(" if {} then return {} ", cond, then_val));
            for (eif_cond, eif_val) in &ie.elseif_clauses {
                let c = emit_expression(emitter, eif_cond);
                let v = emit_expression(emitter, eif_val);
                result.push_str(&format!("elseif {} then return {} ", c, v));
            }
            let else_val = emit_expression(emitter, &ie.else_expr);
            result.push_str(&format!("else return {} end end)()", else_val));
            result
        }
    }
}

/// Attempt to mangle a member name based on the object expression.
/// Mangles when the object is `self` (current class), a known class name, or a known enum name.
/// Respects the `extern` modifier — extern members are never mangled.
fn maybe_mangle_access(emitter: &mut Emitter, object: &Expression, member_name: &str) -> String {
    if emitter.mangler.is_none() {
        return member_name.to_string();
    }

    if let Expression::Identifier(id) = object {
        let name = id.name.as_str();

        // self.field or self:method() → mangle using current class
        if name == "self" {
            if let Some(class_name) = emitter.current_class.clone() {
                if is_extern_member(&emitter.symbol_table, &class_name, member_name) {
                    return member_name.to_string();
                }
                return emitter.mangle_member(&class_name, member_name);
            }
        }

        // ClassName.staticMember → mangle using that class
        if emitter.is_class(name) {
            if is_extern_member(&emitter.symbol_table, name, member_name) {
                return member_name.to_string();
            }
            let type_name = name.to_string();
            return emitter.mangle_member(&type_name, member_name);
        }

        // EnumName.Variant → mangle using that enum
        if emitter.is_enum(name) {
            if is_extern_member(&emitter.symbol_table, name, member_name) {
                return member_name.to_string();
            }
            let type_name = name.to_string();
            return emitter.mangle_member(&type_name, member_name);
        }

        // Local variable with known type → mangle using that type
        if let Some(class_name) = emitter.local_var_types.get(name).cloned() {
            if is_extern_member(&emitter.symbol_table, &class_name, member_name) {
                return member_name.to_string();
            }
            return emitter.mangle_member(&class_name, member_name);
        }
    }

    // self.field:method() or var.field:method() — resolve field type from symbol table
    if let Expression::FieldAccess(fa) = object {
        if let Some(owner_type) = resolve_expression_type(emitter, &fa.object) {
            // Look up the field type to find which class to mangle against
            let field_class = {
                let field_name = fa.field.name.as_str();
                let mut result = None;
                if let Some(class) = emitter.symbol_table.classes.get(&owner_type) {
                    for field in &class.fields {
                        if field.name == field_name {
                            if let luao_resolver::LuaoType::Class(class_id) = &field.type_info {
                                for (cname, csym) in &emitter.symbol_table.classes {
                                    if csym.id == *class_id {
                                        result = Some(cname.clone());
                                        break;
                                    }
                                }
                            }
                            break;
                        }
                    }
                }
                result
            };
            if let Some(cname) = field_class {
                if is_extern_member(&emitter.symbol_table, &cname, member_name) {
                    return member_name.to_string();
                }
                return emitter.mangle_member(&cname, member_name);
            }
        }
    }

    member_name.to_string()
}

/// Check if a member of a class or interface is marked as `extern` in the symbol table.
fn is_extern_member(symbol_table: &luao_resolver::SymbolTable, type_name: &str, member_name: &str) -> bool {
    if let Some(class) = symbol_table.classes.get(type_name) {
        for field in &class.fields {
            if field.name == member_name {
                return field.is_extern;
            }
        }
        for method in &class.methods {
            if method.name == member_name {
                return method.is_extern;
            }
        }
    }
    if let Some(iface) = symbol_table.interfaces.get(type_name) {
        for field in &iface.fields {
            if field.name == member_name {
                return field.is_extern;
            }
        }
        for method in &iface.methods {
            if method.name == member_name {
                return method.is_extern;
            }
        }
    }
    if let Some(enum_sym) = symbol_table.enums.get(type_name) {
        for variant in &enum_sym.variants {
            if variant.name == member_name {
                return variant.is_extern;
            }
        }
    }
    false
}

/// Try to resolve the class type name of an expression.
fn resolve_expression_type(emitter: &Emitter, expr: &Expression) -> Option<String> {
    match expr {
        Expression::Identifier(id) => {
            let name = id.name.as_str();
            if name == "self" {
                return emitter.current_class.clone();
            }
            emitter.local_var_types.get(name).cloned()
        }
        // self.field or var.field — resolve field type from owner's class
        Expression::FieldAccess(fa) => {
            let owner_type = resolve_expression_type(emitter, &fa.object)?;
            let class = emitter.symbol_table.classes.get(&owner_type)?;
            let field_name = fa.field.name.as_str();
            for field in &class.fields {
                if field.name == field_name {
                    if let luao_resolver::LuaoType::Class(class_id) = &field.type_info {
                        for (cname, csym) in &emitter.symbol_table.classes {
                            if csym.id == *class_id {
                                return Some(cname.clone());
                            }
                        }
                    }
                }
            }
            None
        }
        // new Foo() — type is Foo
        Expression::NewExpr(ne) => {
            Some(ne.class_name.name.name.to_string())
        }
        _ => None,
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

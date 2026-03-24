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
        Expression::Identifier(id) => {
            if id.name.as_str() == "Promise" {
                emitter.needs_async = true;
            }
            emitter.rename(&id.name)
        }
        Expression::BinaryOp(bin) => {
            let left = emit_bin_child(emitter, &bin.left, &bin.op, true);
            let right = emit_bin_child(emitter, &bin.right, &bin.op, false);
            let op = binop_to_lua(&bin.op);
            format!("{} {} {}", left, op, right)
        }
        Expression::UnaryOp(un) => {
            let operand = emit_expression(emitter, &un.operand);
            let op = unop_to_lua(&un.op);
            // Only wrap if the operand is a binary op (needs parens for clarity)
            let needs_parens = matches!(&un.operand, Expression::BinaryOp(_));
            if needs_parens {
                match un.op {
                    UnOp::Not => format!("{} ({})", op, operand),
                    _ => format!("{}({})", op, operand),
                }
            } else {
                match un.op {
                    UnOp::Not => format!("{} {}", op, operand),
                    _ => format!("{}{}", op, operand),
                }
            }
        }
        Expression::Grouped(inner, _) => {
            let inner_str = emit_expression(emitter, inner);
            format!("({})", inner_str)
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
            let saved_var_types = emitter.local_var_types.clone();
            // Track function expression parameter types
            for param in &fe.params {
                if param.is_vararg { continue; }
                if let Some(ref ta) = param.type_annotation {
                    if let luao_parser::TypeKind::Named(ref type_name, _) = ta.kind {
                        let tn = type_name.name.to_string();
                        if emitter.is_type(&tn) {
                            emitter.local_var_types.insert(param.name.name.to_string(), tn);
                        }
                    }
                }
            }

            if fe.is_async {
                emitter.needs_async = true;
            }

            if fe.is_generator || fe.is_async {
                let wrapper = if fe.is_async {
                    "__luao_async"
                } else {
                    "coroutine.wrap"
                };
                let saved_async_ctx = emitter.in_async_context;
                let saved_in_switch = emitter.in_switch_case;
                let saved_switch_ret = emitter.in_switch_return_mode;
                emitter.in_async_context = fe.is_async;
                emitter.in_switch_case = false;
                emitter.in_switch_return_mode = false;
                let mut result = format!("function({})\n", params);
                let saved_output = std::mem::take(&mut emitter.output);
                emitter.indent();
                emitter.writeln(&format!("return {}(function()", wrapper));
                emitter.emit_block(&fe.body);
                emitter.writeln("end)");
                emitter.dedent();
                let body = std::mem::replace(&mut emitter.output, saved_output);
                result.push_str(&body);
                emitter.write_indent();
                result.push_str("end");
                emitter.in_async_context = saved_async_ctx;
                emitter.in_switch_case = saved_in_switch;
                emitter.in_switch_return_mode = saved_switch_ret;
                emitter.local_var_types = saved_var_types;
                result
            } else {
                let saved_async_ctx = emitter.in_async_context;
                let saved_in_switch = emitter.in_switch_case;
                let saved_switch_ret = emitter.in_switch_return_mode;
                emitter.in_async_context = false;
                emitter.in_switch_case = false;
                emitter.in_switch_return_mode = false;
                let mut result = format!("function({})\n", params);
                let saved_output = std::mem::take(&mut emitter.output);
                emitter.emit_block(&fe.body);
                let body = std::mem::replace(&mut emitter.output, saved_output);
                result.push_str(&body);
                emitter.write_indent();
                result.push_str("end");
                emitter.in_async_context = saved_async_ctx;
                emitter.in_switch_case = saved_in_switch;
                emitter.in_switch_return_mode = saved_switch_ret;
                emitter.local_var_types = saved_var_types;
                result
            }
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
        Expression::YieldExpr(ye) => {
            if let Some(ref val) = ye.value {
                let v = emit_expression(emitter, val);
                format!("coroutine.yield({})", v)
            } else {
                "coroutine.yield()".to_string()
            }
        }
        Expression::AwaitExpr(ae) => {
            emitter.needs_async = true;
            let expr = emit_expression(emitter, &ae.expr);
            if emitter.in_async_context {
                format!("__luao_yield({})", expr)
            } else {
                format!("({}):expect()", expr)
            }
        }
        Expression::TupleLiteral(tl) => {
            emitter.needs_tuple = true;
            if tl.elements.is_empty() {
                "__luao_tuple({})".to_string()
            } else {
                let elems: Vec<_> = tl.elements.iter()
                    .map(|e| emit_expression(emitter, e))
                    .collect();
                format!("__luao_tuple({{ {} }})", elems.join(", "))
            }
        }
        Expression::ArrayLiteral(al) => {
            emitter.needs_array = true;
            if al.elements.is_empty() {
                "__luao_Array()".to_string()
            } else {
                let elems: Vec<_> = al.elements.iter()
                    .map(|e| emit_expression(emitter, e))
                    .collect();
                format!("__luao_Array({{ {} }})", elems.join(", "))
            }
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
            let field_name = fa.field.name.as_str();
            if let Some(field_type) = resolve_field_type(emitter, &owner_type, field_name) {
                if is_extern_member(&emitter.symbol_table, &field_type, member_name) {
                    return member_name.to_string();
                }
                return emitter.mangle_member(&field_type, member_name);
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
        // self.field or var.field — resolve field type from owner's class/interface
        Expression::FieldAccess(fa) => {
            let owner_type = resolve_expression_type(emitter, &fa.object)?;
            resolve_field_type(emitter, &owner_type, fa.field.name.as_str())
        }
        // new Foo() — type is Foo
        Expression::NewExpr(ne) => {
            Some(ne.class_name.name.name.to_string())
        }
        _ => None,
    }
}

/// Given an owner type and a field name, resolve the field's type from the symbol table.
/// Works for both classes and interfaces.
fn resolve_field_type(emitter: &Emitter, owner_type: &str, field_name: &str) -> Option<String> {
    if let Some(fields) = emitter.lookup_type_fields(owner_type) {
        for field in fields {
            if field.name == field_name {
                return resolve_luao_type(emitter, &field.type_info);
            }
        }
    }
    None
}

/// Resolve a LuaoType to a type name string, checking both classes and interfaces.
fn resolve_luao_type(emitter: &Emitter, ty: &luao_resolver::LuaoType) -> Option<String> {
    match ty {
        luao_resolver::LuaoType::Class(id) => {
            for (name, sym) in &emitter.symbol_table.classes {
                if sym.id == *id { return Some(name.clone()); }
            }
            None
        }
        luao_resolver::LuaoType::Interface(id) => {
            for (name, sym) in &emitter.symbol_table.interfaces {
                if sym.id == *id { return Some(name.clone()); }
            }
            None
        }
        luao_resolver::LuaoType::TypeParam(name) => {
            if emitter.symbol_table.classes.contains_key(name)
                || emitter.symbol_table.interfaces.contains_key(name)
            {
                Some(name.clone())
            } else {
                None
            }
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

/// Lua operator precedence (higher = binds tighter).
fn binop_precedence(op: &BinOp) -> u8 {
    match op {
        BinOp::Or => 1,
        BinOp::And => 2,
        BinOp::Lt | BinOp::Gt | BinOp::Le | BinOp::Ge | BinOp::NotEq | BinOp::Eq => 3,
        BinOp::BitOr => 4,
        BinOp::BitXor => 5,
        BinOp::BitAnd => 6,
        BinOp::ShiftLeft | BinOp::ShiftRight => 7,
        BinOp::Concat => 8,
        BinOp::Add | BinOp::Sub => 9,
        BinOp::Mul | BinOp::Div | BinOp::IntDiv | BinOp::Mod => 10,
        BinOp::Pow => 12,
    }
}

fn is_right_associative(op: &BinOp) -> bool {
    matches!(op, BinOp::Concat | BinOp::Pow)
}

/// Emit a child of a binary op, adding parens only when needed for precedence.
fn emit_bin_child(emitter: &mut Emitter, child: &Expression, parent_op: &BinOp, is_left: bool) -> String {
    let child_str = emit_expression(emitter, child);
    if let Expression::BinaryOp(child_bin) = child {
        let parent_prec = binop_precedence(parent_op);
        let child_prec = binop_precedence(&child_bin.op);
        // Need parens if child has lower precedence
        // or equal precedence on the wrong side of an associative op
        let needs_parens = child_prec < parent_prec
            || (child_prec == parent_prec && is_left && is_right_associative(parent_op))
            || (child_prec == parent_prec && !is_left && !is_right_associative(parent_op));
        if needs_parens {
            return format!("({})", child_str);
        }
    }
    child_str
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

use luao_parser::{AccessModifier, ClassDecl, ClassMember, Expression};

use crate::emitter::Emitter;
use crate::expression_emitter::emit_expression;


pub fn emit_class(emitter: &mut Emitter, class: &ClassDecl) {
    let class_name = emitter.rename_decl(&class.name.name);
    let parent_name = class
        .parent
        .as_ref()
        .map(|p| emitter.rename(&p.name.name));

    emitter.current_class = Some(class_name.clone());
    emitter.current_class_parent = parent_name.clone();

    let local_prefix = if emitter.is_exported(&class_name) { "" } else { "local " };
    if let Some(ref parent) = parent_name {
        emitter.writeln(&format!(
            "{}{} = setmetatable({{}}, {{ __index = {} }})",
            local_prefix, class_name, parent
        ));
    } else {
        emitter.writeln(&format!("{}{} = {{}}", local_prefix, class_name));
    }
    emitter.writeln(&format!("{}.__index = {}", class_name, class_name));
    emitter.newline();

    emit_properties(emitter, class, &class_name);

    let has_constructor = class.members.iter().any(|m| matches!(m, ClassMember::Constructor(_)));

    for member in &class.members {
        match member {
            ClassMember::Field(field) => {
                if field.is_static {
                    let field_name = member_output_name(emitter, &class_name, &field.name.name, field.access, field.is_extern || class.is_extern);
                    if let Some(ref val) = field.default_value {
                        let v = emit_expression(emitter, val);
                        emitter.writeln(&format!("{}.{} = {}", class_name, field_name, v));
                    } else {
                        emitter.writeln(&format!("{}.{} = nil", class_name, field_name));
                    }
                }
            }
            ClassMember::Constructor(ctor) => {
                emit_constructor(emitter, class, ctor, &class_name, &parent_name);
            }
            ClassMember::Method(method) => {
                emit_method(emitter, method, &class_name, &parent_name, class.is_extern);
            }
            ClassMember::Property(_) => {}
        }
    }

    // Generate default constructor if none was declared
    if !has_constructor {
        emit_default_constructor(emitter, class, &class_name, &parent_name);
    }

    emitter.current_class = None;
    emitter.current_class_parent = None;
    emitter.newline();
}

fn emit_constructor(
    emitter: &mut Emitter,
    class: &ClassDecl,
    ctor: &luao_parser::ConstructorDecl,
    class_name: &str,
    parent_name: &Option<String>,
) {
    let params = emitter.emit_params(&ctor.params);
    let new_name = if class.is_extern { "new".to_string() } else { emitter.mangle_shared("new") };

    emitter.writeln(&format!("function {}.{}({})", class_name, new_name, params));
    emitter.indent();

    if class.is_abstract {
        emitter.writeln(&format!(
            "error(\"Cannot instantiate abstract class {}\")",
            class_name
        ));
    }

    let has_super_call = parent_name.is_some()
        && ctor.body.statements.iter().any(|s| is_super_new_call(s));

    if !has_super_call {
        if let Some(parent) = parent_name {
            let parent_new = if is_parent_extern(emitter, parent_name) {
                "new".to_string()
            } else {
                emitter.mangle_shared("new")
            };
            emitter.writeln(&format!("local self = {}.{}()", parent, parent_new));
            emitter.writeln(&format!("setmetatable(self, {})", class_name));
        } else {
            emitter.writeln(&format!("local self = setmetatable({{}}, {})", class_name));
        }
    }

    emit_default_fields(emitter, class);

    let saved_var_types = emitter.local_var_types.clone();

    // Track constructor parameter types
    for param in &ctor.params {
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

    for stmt in &ctor.body.statements {
        emit_constructor_statement(emitter, stmt, parent_name, class_name);
    }

    emitter.local_var_types = saved_var_types;
    emitter.writeln("return self");
    emitter.dedent();
    emitter.writeln("end");
    emitter.newline();
}

fn emit_default_constructor(
    emitter: &mut Emitter,
    class: &ClassDecl,
    class_name: &str,
    parent_name: &Option<String>,
) {
    let new_name = if class.is_extern { "new".to_string() } else { emitter.mangle_shared("new") };
    emitter.writeln(&format!("function {}.{}()", class_name, new_name));
    emitter.indent();

    if class.is_abstract {
        emitter.writeln(&format!(
            "error(\"Cannot instantiate abstract class {}\")",
            class_name
        ));
    }

    if let Some(parent) = parent_name {
        let parent_new = if is_parent_extern(emitter, parent_name) {
            "new".to_string()
        } else {
            emitter.mangle_shared("new")
        };
        emitter.writeln(&format!("local self = {}.{}()", parent, parent_new));
        emitter.writeln(&format!("setmetatable(self, {})", class_name));
    } else {
        emitter.writeln(&format!("local self = setmetatable({{}}, {})", class_name));
    }

    emit_default_fields(emitter, class);

    emitter.writeln("return self");
    emitter.dedent();
    emitter.writeln("end");
    emitter.newline();
}

fn emit_default_fields(emitter: &mut Emitter, class: &ClassDecl) {
    let class_name = emitter.rename_decl(&class.name.name);
    for member in &class.members {
        if let ClassMember::Field(field) = member {
            if !field.is_static {
                if let Some(ref val) = field.default_value {
                    let field_name = member_output_name(emitter, &class_name, &field.name.name, field.access, field.is_extern || class.is_extern);
                    let v = emit_expression(emitter, val);
                    emitter.writeln(&format!("self.{} = {}", field_name, v));
                }
            }
        }
    }
}

/// Check if the parent class (by name) is marked as extern in the symbol table.
fn is_parent_extern(emitter: &Emitter, parent_name: &Option<String>) -> bool {
    if let Some(parent) = parent_name {
        if let Some(cls) = emitter.symbol_table.classes.get(parent) {
            return cls.is_extern;
        }
    }
    false
}

fn is_super_new_call(stmt: &luao_parser::Statement) -> bool {
    if let luao_parser::Statement::ExpressionStatement(expr) = stmt {
        if let Expression::FunctionCall(call) = expr {
            if let Expression::SuperAccess(sa) = &call.callee {
                return sa.method.name.as_str() == "new";
            }
        }
    }
    false
}

fn emit_constructor_statement(
    emitter: &mut Emitter,
    stmt: &luao_parser::Statement,
    parent_name: &Option<String>,
    class_name: &str,
) {
    if let luao_parser::Statement::ExpressionStatement(expr) = stmt {
        if let Expression::FunctionCall(call) = expr {
            if let Expression::SuperAccess(sa) = &call.callee {
                if sa.method.name.as_str() == "new" {
                    if let Some(parent) = parent_name {
                        let args = call
                            .args
                            .iter()
                            .map(|a| emit_expression(emitter, a))
                            .collect::<Vec<_>>()
                            .join(", ");
                        let parent_new = if is_parent_extern(emitter, parent_name) {
                            "new".to_string()
                        } else {
                            emitter.mangle_shared("new")
                        };
                        emitter.writeln(&format!("local self = {}.{}({})", parent, parent_new, args));
                        emitter.writeln(&format!("setmetatable(self, {})", class_name));
                        return;
                    }
                }
            }
        }
    }
    emitter.emit_statement(stmt);
}

fn emit_method(
    emitter: &mut Emitter,
    method: &luao_parser::MethodDecl,
    class_name: &str,
    parent_name: &Option<String>,
    class_is_extern: bool,
) {
    let original_name = method.name.name.to_string();
    let method_name = member_output_name(emitter, class_name, &original_name, method.access, method.is_extern || class_is_extern);
    let params = emitter.emit_params(&method.params);

    let is_operator = original_name.starts_with("__");
    let use_no_self = emitter.no_self && !method.is_static && !is_operator;

    let params = if use_no_self {
        if params.is_empty() {
            "self".to_string()
        } else {
            format!("self, {}", params)
        }
    } else if is_operator && !method.is_static {
        if params.is_empty() {
            "self".to_string()
        } else {
            format!("self, {}", params)
        }
    } else {
        params
    };

    if method.is_abstract && method.body.is_none() {
        // Abstract methods always use `.` since they just error
        emitter.writeln(&format!(
            "function {}.{}({})",
            class_name, method_name, params
        ));
        emitter.indent();
        emitter.writeln(&format!(
            "error(\"Abstract method '{}' must be implemented\")",
            original_name
        ));
        emitter.dedent();
        emitter.writeln("end");
        emitter.newline();
        return;
    }

    if method.is_static || is_operator || use_no_self {
        emitter.writeln(&format!(
            "function {}.{}({})",
            class_name, method_name, params
        ));
    } else {
        emitter.writeln(&format!(
            "function {}:{}({})",
            class_name, method_name, params
        ));
    }

    if let Some(ref body) = method.body {
        let saved_parent = emitter.current_class_parent.clone();
        let saved_var_types = emitter.local_var_types.clone();
        emitter.current_class_parent = parent_name.clone();
        // Track method parameter types
        for param in &method.params {
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
        emitter.emit_block(body);
        emitter.current_class_parent = saved_parent;
        emitter.local_var_types = saved_var_types;
    }

    emitter.writeln("end");
    emitter.newline();
}

/// Emit properties as compile-time methods. Non-extern properties become __get_/set_ methods.
/// Extern properties keep the runtime __index/__newindex interceptor approach for external compatibility.
fn emit_properties(emitter: &mut Emitter, class: &ClassDecl, class_name: &str) {
    let mut extern_getters: Vec<String> = Vec::new();
    let mut extern_setters: Vec<String> = Vec::new();

    // Collect existing method names to avoid collisions
    let method_names: std::collections::HashSet<String> = class
        .members
        .iter()
        .filter_map(|m| {
            if let ClassMember::Method(method) = m {
                Some(method.name.name.to_string())
            } else {
                None
            }
        })
        .collect();

    for member in &class.members {
        if let ClassMember::Property(prop) = member {
            let prop_name = prop.name.name.to_string();

            // All properties get methods AND runtime interceptors.
            // self.prop inside the class → compile-time method call (optimization).
            // obj.prop from external code → runtime __index/__newindex interceptor.
            if prop.getter.is_some() {
                extern_getters.push(prop_name.clone());
            }
            if prop.setter.is_some() {
                extern_setters.push(prop_name.clone());
            }

            if let Some(ref getter_body) = prop.getter {
                let method_name = unique_getter_name(&prop_name, &method_names);
                emitter.property_getters.insert(
                    (class_name.to_string(), prop_name.clone()),
                    method_name.clone(),
                );
                emitter.writeln(&format!("function {}:{}()", class_name, method_name));
                emitter.emit_block(getter_body);
                emitter.writeln("end");
                emitter.newline();
            }
            if let Some((ref param, ref setter_body)) = prop.setter {
                let method_name = unique_setter_name(&prop_name, &method_names);
                emitter.property_setters.insert(
                    (class_name.to_string(), prop_name.clone()),
                    method_name.clone(),
                );
                emitter.writeln(&format!(
                    "function {}:{}({})",
                    class_name, method_name, param.name
                ));
                emitter.emit_block(setter_body);
                emitter.writeln("end");
                emitter.newline();
            }
        }
    }

    // Emit runtime interceptors ONLY for extern properties
    if !extern_getters.is_empty() || !extern_setters.is_empty() {
        let original_index = format!("{}.__original_index", class_name);
        emitter.writeln(&format!("{} = {}.__index", original_index, class_name));
        emitter.writeln(&format!("{}.__index = function(t, k)", class_name));
        emitter.indent();

        for member in &class.members {
            if let ClassMember::Property(prop) = member {
                if prop.getter.is_some() {
                    let prop_name = output_field_name(&prop.name.name, prop.access);
                    let getter_method = emitter
                        .property_getters
                        .get(&(class_name.to_string(), prop.name.name.to_string()))
                        .cloned()
                        .unwrap_or_else(|| format!("__get_{}", prop_name));
                    emitter.writeln(&format!(
                        "if k == \"{}\" then return t:{}() end",
                        prop_name, getter_method
                    ));
                }
            }
        }

        emitter.writeln(&format!(
            "if type({}) == \"table\" then return {}[k] end",
            original_index, original_index
        ));
        emitter.writeln(&format!(
            "if type({}) == \"function\" then return {}(t, k) end",
            original_index, original_index
        ));
        emitter.dedent();
        emitter.writeln("end");
        emitter.newline();

        if !extern_setters.is_empty() {
            emitter.writeln(&format!("{}.__newindex = function(t, k, v)", class_name));
            emitter.indent();

            for member in &class.members {
                if let ClassMember::Property(prop) = member {
                    if prop.setter.is_some() {
                        let prop_name = output_field_name(&prop.name.name, prop.access);
                        let setter_method = emitter
                            .property_setters
                            .get(&(class_name.to_string(), prop.name.name.to_string()))
                            .cloned()
                            .unwrap_or_else(|| format!("__set_{}", prop_name));
                        emitter.writeln(&format!(
                            "if k == \"{}\" then t:{}(v) return end",
                            prop_name, setter_method
                        ));
                    }
                }
            }

            emitter.writeln("rawset(t, k, v)");
            emitter.dedent();
            emitter.writeln("end");
            emitter.newline();
        }
    }
}

fn unique_getter_name(prop_name: &str, existing: &std::collections::HashSet<String>) -> String {
    let candidate = format!("__get_{}", prop_name);
    if !existing.contains(&candidate) {
        return candidate;
    }
    let mut i = 2;
    loop {
        let candidate = format!("__get_{}_{}", prop_name, i);
        if !existing.contains(&candidate) {
            return candidate;
        }
        i += 1;
    }
}

fn unique_setter_name(prop_name: &str, existing: &std::collections::HashSet<String>) -> String {
    let candidate = format!("__set_{}", prop_name);
    if !existing.contains(&candidate) {
        return candidate;
    }
    let mut i = 2;
    loop {
        let candidate = format!("__set_{}_{}", prop_name, i);
        if !existing.contains(&candidate) {
            return candidate;
        }
        i += 1;
    }
}

/// Get the output name for a class member, applying mangling or the `_` prefix for private/protected.
/// If `is_extern` is true, mangling is skipped for this member.
fn member_output_name(emitter: &mut Emitter, class_name: &str, name: &str, access: AccessModifier, is_extern: bool) -> String {
    if emitter.mangler.is_some() && !is_extern {
        emitter.mangle_member(class_name, name)
    } else {
        output_field_name(name, access)
    }
}

fn output_field_name(name: &str, access: AccessModifier) -> String {
    match access {
        AccessModifier::Private | AccessModifier::Protected => format!("_{}", name),
        AccessModifier::Public => name.to_string(),
    }
}

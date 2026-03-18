use luao_parser::{AccessModifier, ClassDecl, ClassMember, Expression};

use crate::emitter::Emitter;
use crate::expression_emitter::emit_expression;

pub fn emit_class(emitter: &mut Emitter, class: &ClassDecl) {
    let class_name = class.name.name.to_string();
    let parent_name = class
        .parent
        .as_ref()
        .map(|p| p.name.name.to_string());

    emitter.current_class = Some(class_name.clone());
    emitter.current_class_parent = parent_name.clone();

    if let Some(ref parent) = parent_name {
        emitter.writeln(&format!(
            "local {} = setmetatable({{}}, {{ __index = {} }})",
            class_name, parent
        ));
    } else {
        emitter.writeln(&format!("local {} = {{}}", class_name));
    }
    emitter.writeln(&format!("{}.__index = {}", class_name, class_name));
    emitter.newline();

    let has_properties = class.members.iter().any(|m| matches!(m, ClassMember::Property(_)));
    if has_properties {
        emit_property_interceptors(emitter, class, &class_name);
    }

    for member in &class.members {
        match member {
            ClassMember::Field(field) => {
                if field.is_static {
                    let field_name = member_output_name(emitter, &class_name, &field.name.name, field.access, field.is_extern);
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
                emit_method(emitter, method, &class_name, &parent_name);
            }
            ClassMember::Property(_) => {}
        }
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

    emitter.writeln(&format!("function {}._new({})", class_name, params));
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
            emitter.writeln(&format!("local self = {}._new()", parent));
            emitter.writeln(&format!("setmetatable(self, {})", class_name));
        } else {
            emitter.writeln(&format!("local self = setmetatable({{}}, {})", class_name));
        }
    }

    emit_default_fields(emitter, class);

    for stmt in &ctor.body.statements {
        emit_constructor_statement(emitter, stmt, parent_name, class_name);
    }

    emitter.writeln("return self");
    emitter.dedent();
    emitter.writeln("end");
    emitter.newline();
}

fn emit_default_fields(emitter: &mut Emitter, class: &ClassDecl) {
    let class_name = class.name.name.to_string();
    for member in &class.members {
        if let ClassMember::Field(field) = member {
            if !field.is_static {
                if let Some(ref val) = field.default_value {
                    let field_name = member_output_name(emitter, &class_name, &field.name.name, field.access, field.is_extern);
                    let v = emit_expression(emitter, val);
                    emitter.writeln(&format!("self.{} = {}", field_name, v));
                }
            }
        }
    }
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
                        emitter.writeln(&format!("local self = {}._new({})", parent, args));
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
) {
    let original_name = method.name.name.to_string();
    let method_name = member_output_name(emitter, class_name, &original_name, method.access, method.is_extern);
    let params = emitter.emit_params(&method.params);

    let is_operator = original_name.starts_with("__");

    let params = if is_operator && !method.is_static {
        if params.is_empty() {
            "self".to_string()
        } else {
            format!("self, {}", params)
        }
    } else {
        params
    };

    if method.is_abstract && method.body.is_none() {
        if method.is_static || is_operator {
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

    if method.is_static || is_operator {
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
        emitter.current_class_parent = parent_name.clone();
        emitter.emit_block(body);
        emitter.current_class_parent = saved_parent;
    }

    emitter.writeln("end");
    emitter.newline();
}

fn emit_property_interceptors(emitter: &mut Emitter, class: &ClassDecl, class_name: &str) {
    let mut getters = Vec::new();
    let mut setters = Vec::new();

    for member in &class.members {
        if let ClassMember::Property(prop) = member {
            let prop_name = prop.name.name.to_string();
            if prop.getter.is_some() {
                getters.push(prop_name.clone());
            }
            if prop.setter.is_some() {
                setters.push(prop_name);
            }
        }
    }

    if !getters.is_empty() {
        emitter.writeln(&format!("{}.__getters = {{}}", class_name));
        for member in &class.members {
            if let ClassMember::Property(prop) = member {
                if let Some(ref getter_body) = prop.getter {
                    let prop_name = member_output_name(emitter, class_name, &prop.name.name, prop.access, prop.is_extern);
                    emitter.writeln(&format!(
                        "{}.__getters[\"{}\"] = function(self)",
                        class_name, prop_name
                    ));
                    emitter.emit_block(getter_body);
                    emitter.writeln("end");
                }
            }
        }
        emitter.newline();
    }

    if !setters.is_empty() {
        emitter.writeln(&format!("{}.__setters = {{}}", class_name));
        for member in &class.members {
            if let ClassMember::Property(prop) = member {
                if let Some((ref param, ref setter_body)) = prop.setter {
                    let prop_name = member_output_name(emitter, class_name, &prop.name.name, prop.access, prop.is_extern);
                    emitter.writeln(&format!(
                        "{}.__setters[\"{}\"] = function(self, {})",
                        class_name, prop_name, param.name
                    ));
                    emitter.emit_block(setter_body);
                    emitter.writeln("end");
                }
            }
        }
        emitter.newline();
    }

    if !getters.is_empty() || !setters.is_empty() {
        let original_index = format!("{}.__original_index", class_name);
        emitter.writeln(&format!("{} = {}.__index", original_index, class_name));
        emitter.writeln(&format!(
            "{}.__index = function(t, k)",
            class_name
        ));
        emitter.indent();
        if !getters.is_empty() {
            emitter.writeln(&format!(
                "local getter = {}.__getters[k]",
                class_name
            ));
            emitter.writeln("if getter then return getter(t) end");
        }
        emitter.writeln(&format!(
            "if type({}) == \"table\" then return {}[k] end",
            original_index, original_index
        ));
        emitter.writeln(&format!("return {}", original_index));
        emitter.dedent();
        emitter.writeln("end");
        emitter.newline();

        if !setters.is_empty() {
            emitter.writeln(&format!(
                "{}.__newindex = function(t, k, v)",
                class_name
            ));
            emitter.indent();
            emitter.writeln(&format!(
                "local setter = {}.__setters[k]",
                class_name
            ));
            emitter.writeln("if setter then setter(t, v) return end");
            emitter.writeln("rawset(t, k, v)");
            emitter.dedent();
            emitter.writeln("end");
            emitter.newline();
        }
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

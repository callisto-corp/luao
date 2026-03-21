use luao_parser::EnumDecl;

use crate::emitter::Emitter;
use crate::expression_emitter::emit_expression;

pub fn emit_enum(emitter: &mut Emitter, enum_decl: &EnumDecl) {
    emitter.needs_enum_freeze = true;
    let name = emitter.rename_decl(&enum_decl.name.name);

    let mut entries = Vec::new();
    let mut reverse_entries = Vec::new();
    let mut next_value: i64 = 1;

    for variant in &enum_decl.variants {
        let original_name = variant.name.name.to_string();
        let output_name = if variant.is_extern {
            original_name.clone()
        } else {
            emitter.mangle_member(&name, &original_name)
        };
        let value = if let Some(expr) = &variant.value {
            let val_str = emit_expression(emitter, expr);
            if let luao_parser::Expression::Number(n, _) = expr {
                if let Ok(v) = n.parse::<i64>() {
                    next_value = v + 1;
                }
            }
            val_str
        } else {
            let v = next_value;
            next_value += 1;
            v.to_string()
        };
        entries.push(format!("{} = {}", output_name, value));
        reverse_entries.push(format!("[{}] = \"{}\"", value, output_name));
    }

    let local_prefix = if emitter.should_skip_local() || emitter.is_exported(&name) { "" } else { "local " };
    emitter.writeln(&format!(
        "{}{} = {{ {} }}",
        local_prefix, name,
        entries.join(", ")
    ));
    let values_name = emitter.mangle_shared("_values");
    emitter.writeln(&format!(
        "{}.{} = {{ {} }}",
        name, values_name,
        reverse_entries.join(", ")
    ));
    emitter.writeln(&format!("__luao_enum_freeze({})", name));
    emitter.newline();
}

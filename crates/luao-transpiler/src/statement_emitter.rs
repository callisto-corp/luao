use luao_parser::{Block, Statement};

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

            // Skip `local` in bundle globals mode or if all names are exported (forward-declared)
            let skip_local = emitter.should_skip_local() || names.iter().all(|n| emitter.is_exported(n));
            let prefix = if skip_local { "" } else { "local " };
            if la.values.is_empty() {
                if !skip_local {
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
            let saved_var_types = emitter.local_var_types.clone();
            let saved_in_switch = emitter.in_switch_case;
            let saved_switch_ret = emitter.in_switch_return_mode;
            emitter.in_switch_case = false;
            emitter.in_switch_return_mode = false;
            // Track parameter types
            track_param_types(emitter, &fd.params);

            if fd.is_async {
                emitter.needs_async = true;
            }

            if fd.is_local && !emitter.should_skip_local() && !emitter.is_exported(&name) {
                emitter.writeln(&format!("local function {}({})", name, params));
            } else {
                emitter.writeln(&format!("function {}({})", name, params));
            }

            if fd.is_generator || fd.is_async {
                let wrapper = if fd.is_async { "__luao_async" } else { "coroutine.wrap" };
                let saved_async_ctx = emitter.in_async_context;
                emitter.in_async_context = fd.is_async;
                emitter.indent();
                emitter.writeln(&format!("return {}(function()", wrapper));
                emitter.emit_block(&fd.body);
                emitter.writeln("end)");
                emitter.dedent();
                emitter.in_async_context = saved_async_ctx;
            } else {
                let saved_async_ctx = emitter.in_async_context;
                emitter.in_async_context = false;
                emitter.emit_block(&fd.body);
                emitter.in_async_context = saved_async_ctx;
            }
            emitter.writeln("end");
            emitter.local_var_types = saved_var_types;
            emitter.in_switch_case = saved_in_switch;
            emitter.in_switch_return_mode = saved_switch_ret;
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
            let saved_switch = emitter.in_switch_case;
            emitter.in_switch_case = false;
            let cond = emit_expression(emitter, &ws.condition);
            emitter.writeln(&format!("while {} do", cond));
            emitter.emit_block(&ws.body);
            emitter.writeln("end");
            emitter.in_switch_case = saved_switch;
        }
        Statement::RepeatStatement(rs) => {
            let saved_switch = emitter.in_switch_case;
            emitter.in_switch_case = false;
            emitter.writeln("repeat");
            emitter.emit_block(&rs.body);
            let cond = emit_expression(emitter, &rs.condition);
            emitter.writeln(&format!("until {}", cond));
            emitter.in_switch_case = saved_switch;
        }
        Statement::ForNumeric(f) => {
            let saved_switch = emitter.in_switch_case;
            emitter.in_switch_case = false;
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
            emitter.in_switch_case = saved_switch;
        }
        Statement::ForGeneric(f) => {
            let saved_switch = emitter.in_switch_case;
            emitter.in_switch_case = false;
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
            emitter.in_switch_case = saved_switch;
        }
        Statement::SwitchStatement(sw) => {
            emit_switch(emitter, sw);
        }
        Statement::DoBlock(block) => {
            emitter.writeln("do");
            emitter.emit_block(block);
            emitter.writeln("end");
        }
        Statement::ReturnStatement(ret) => {
            if emitter.in_switch_return_mode {
                if ret.values.is_empty() {
                    emitter.writeln("return true");
                } else {
                    let values: Vec<_> = ret
                        .values
                        .iter()
                        .map(|v| emit_expression(emitter, v))
                        .collect();
                    emitter.writeln(&format!("return true, {}", values.join(", ")));
                }
            } else if ret.values.is_empty() {
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
            if emitter.in_switch_case {
                emitter.writeln("return");
            } else {
                emitter.writeln("break");
            }
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
                            match &method.return_type {
                                luao_resolver::LuaoType::Class(class_id) => {
                                    for (cname, csym) in &emitter.symbol_table.classes {
                                        if csym.id == *class_id {
                                            return Some(cname.clone());
                                        }
                                    }
                                }
                                luao_resolver::LuaoType::TypeParam(name) => {
                                    if emitter.symbol_table.classes.contains_key(name) {
                                        return Some(name.clone());
                                    }
                                }
                                _ => {}
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
                                match &method.return_type {
                                    luao_resolver::LuaoType::Class(class_id) => {
                                        for (cname, csym) in &emitter.symbol_table.classes {
                                            if csym.id == *class_id {
                                                return Some(cname.clone());
                                            }
                                        }
                                    }
                                    luao_resolver::LuaoType::TypeParam(name) => {
                                        if emitter.symbol_table.classes.contains_key(name) {
                                            return Some(name.clone());
                                        }
                                    }
                                    _ => {}
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

// =============================================================================
// Switch statement codegen — table-based O(1) dispatch with cascading
// =============================================================================

/// Check if a block's last statement is `break` (not inside a loop).
fn case_body_ends_with_break(block: &Block) -> bool {
    block
        .statements
        .last()
        .map_or(false, |s| matches!(s, Statement::Break(_)))
}

/// Check if a block's last statement is a `return` (unconditional, top-level).
fn case_body_ends_with_return(block: &Block) -> bool {
    block
        .statements
        .last()
        .map_or(false, |s| matches!(s, Statement::ReturnStatement(_)))
}

/// Check if a block is effectively empty (no statements, or only a break).
fn case_body_is_empty(block: &Block) -> bool {
    block.statements.is_empty()
        || (block.statements.len() == 1 && matches!(block.statements[0], Statement::Break(_)))
}

/// Recursively check if a block contains any `return` statement.
/// Skips into if/do/switch blocks but NOT into function bodies.
fn block_contains_return(block: &Block) -> bool {
    for stmt in &block.statements {
        if stmt_contains_return(stmt) {
            return true;
        }
    }
    false
}

fn stmt_contains_return(stmt: &Statement) -> bool {
    match stmt {
        Statement::ReturnStatement(_) => true,
        Statement::IfStatement(ifs) => {
            block_contains_return(&ifs.then_block)
                || ifs
                    .elseif_clauses
                    .iter()
                    .any(|(_, b)| block_contains_return(b))
                || ifs
                    .else_block
                    .as_ref()
                    .map_or(false, |b| block_contains_return(b))
        }
        Statement::DoBlock(b) => block_contains_return(b),
        Statement::WhileStatement(w) => block_contains_return(&w.body),
        Statement::RepeatStatement(r) => block_contains_return(&r.body),
        Statement::ForNumeric(f) => block_contains_return(&f.body),
        Statement::ForGeneric(f) => block_contains_return(&f.body),
        Statement::SwitchStatement(sw) => {
            sw.cases.iter().any(|c| block_contains_return(&c.body))
                || sw
                    .default
                    .as_ref()
                    .map_or(false, |b| block_contains_return(b))
        }
        // Don't descend into function bodies — return there is local to the fn
        _ => false,
    }
}

/// A group of case values that share a single callback function.
struct CaseGroup<'a> {
    values: Vec<String>,
    body: &'a Block,
    ends_with_break: bool,
    ends_with_return: bool,
    fn_name: String,
}

fn emit_switch(emitter: &mut Emitter, sw: &luao_parser::SwitchStatement) {
    // Edge case: completely empty switch
    if sw.cases.is_empty() && sw.default.is_none() {
        // Evaluate subject for side effects only
        let subject = emit_expression(emitter, &sw.subject);
        emitter.writeln(&format!("local _ = {}", subject));
        return;
    }

    // Edge case: only default, no cases
    if sw.cases.is_empty() {
        if let Some(ref default_block) = sw.default {
            let subject = emit_expression(emitter, &sw.subject);
            emitter.writeln(&format!("local _ = {}", subject));
            emitter.writeln("do");
            emitter.emit_block(default_block);
            emitter.writeln("end");
        }
        return;
    }

    let id = emitter.next_temp_id();
    let subject = emit_expression(emitter, &sw.subject);

    // Phase 1: Analysis — determine if any case has return
    let any_return = sw.cases.iter().any(|c| block_contains_return(&c.body))
        || sw
            .default
            .as_ref()
            .map_or(false, |b| block_contains_return(b));

    // Phase 2: Build case groups — merge empty cases into next non-empty case
    let mut groups: Vec<CaseGroup> = Vec::new();
    let mut pending_values: Vec<String> = Vec::new();

    for case in &sw.cases {
        let case_values: Vec<String> = case
            .values
            .iter()
            .map(|v| emit_expression(emitter, v))
            .collect();

        if case_body_is_empty(&case.body) {
            pending_values.extend(case_values);
        } else {
            pending_values.extend(case_values);
            let ends_break = case_body_ends_with_break(&case.body);
            let ends_return = case_body_ends_with_return(&case.body);
            let fn_name = format!("__c{}_{}", id, groups.len());
            groups.push(CaseGroup {
                values: pending_values.drain(..).collect(),
                body: &case.body,
                ends_with_break: ends_break,
                ends_with_return: ends_return,
                fn_name,
            });
        }
    }

    // Trailing empty cases (pending_values left over) fall through to default.
    // They'll be handled: if default exists, they map to default via the `or` fallback.
    // If no default, they're no-ops (won't be in the lookup table).
    // But actually, if they should cascade to default, we need them in the map pointing
    // to default. We'll handle this by creating a default group if needed.
    let has_trailing_empty = !pending_values.is_empty();
    let _trailing_values = pending_values;

    // Phase 3: Emit lookup table
    // Maps case values to 1-based indices into the function array
    emitter.writeln(&format!("local __s{} = {{", id));
    emitter.indent();
    for (gi, group) in groups.iter().enumerate() {
        let idx = gi + 1; // 1-based for Lua arrays
        for val in &group.values {
            emitter.writeln(&format!("[{}] = {},", val, idx));
        }
    }
    emitter.dedent();
    emitter.writeln("}");

    // Phase 4: Emit case functions in reverse order
    // Default first (if exists), then last group to first group
    let default_fn_name = format!("__c{}_default", id);

    if let Some(ref default_block) = sw.default {
        let saved_switch = emitter.in_switch_case;
        let saved_ret = emitter.in_switch_return_mode;
        emitter.in_switch_case = true;
        emitter.in_switch_return_mode = any_return;

        emitter.writeln(&format!("local function {}()", default_fn_name));
        emitter.emit_block(default_block);
        emitter.writeln("end");

        emitter.in_switch_case = saved_switch;
        emitter.in_switch_return_mode = saved_ret;
    }

    for gi in (0..groups.len()).rev() {
        let saved_switch = emitter.in_switch_case;
        let saved_ret = emitter.in_switch_return_mode;
        emitter.in_switch_case = true;
        emitter.in_switch_return_mode = any_return;

        let fn_name = &groups[gi].fn_name;
        emitter.writeln(&format!("local function {}()", fn_name));
        emitter.indent();

        // Emit body statements, stripping the trailing break if present
        let stmts = &groups[gi].body.statements;
        let stmt_count = stmts.len();
        for (si, stmt) in stmts.iter().enumerate() {
            // Skip trailing break — it just means "don't cascade"
            if si == stmt_count - 1 && matches!(stmt, Statement::Break(_)) {
                continue;
            }
            emitter.emit_statement(stmt);
        }

        // If no break or return at end, cascade to next callback
        if !groups[gi].ends_with_break && !groups[gi].ends_with_return {
            // Determine the next function to call
            let next_fn = if gi + 1 < groups.len() {
                Some(groups[gi + 1].fn_name.clone())
            } else if sw.default.is_some() {
                Some(default_fn_name.clone())
            } else {
                None
            };
            if let Some(next) = next_fn {
                emitter.writeln(&format!("return {}()", next));
            }
        }

        emitter.dedent();
        emitter.writeln("end");

        emitter.in_switch_case = saved_switch;
        emitter.in_switch_return_mode = saved_ret;
    }

    // Phase 5: Emit function array
    let fn_names: Vec<&str> = groups.iter().map(|g| g.fn_name.as_str()).collect();
    emitter.writeln(&format!(
        "local __c{} = {{ {} }}",
        id,
        fn_names.join(", ")
    ));

    // Phase 6: Emit dispatch
    let has_default = sw.default.is_some();

    if any_return {
        if has_default {
            emitter.writeln(&format!(
                "local __ret{id} = {{(__c{id}[__s{id}[{subj}]] or {def})()}}",
                id = id,
                subj = subject,
                def = default_fn_name
            ));
        } else {
            emitter.writeln(&format!(
                "local __fn{} = __c{}[__s{}[{}]]",
                id, id, id, subject
            ));
            emitter.writeln(&format!("local __ret{id} = {{}}", id = id));
            emitter.writeln(&format!(
                "if __fn{id} then __ret{id} = {{__fn{id}()}} end",
                id = id
            ));
        }
        emitter.writeln(&format!(
            "if __ret{id}[1] then return select(2, unpack(__ret{id})) end",
            id = id
        ));
    } else {
        if has_default {
            // Handle trailing empty cases — they should call default
            if has_trailing_empty {
                // We need to check if subject matches trailing values
                // Since they're not in the lookup table, the `or default` handles it
                // But wait — ALL non-matched values go to default, so trailing empties
                // are naturally handled.
            }
            emitter.writeln(&format!(
                "(__c{id}[__s{id}[{subj}]] or {def})()",
                id = id,
                subj = subject,
                def = default_fn_name
            ));
        } else {
            emitter.writeln(&format!(
                "local __fn{} = __c{}[__s{}[{}]]",
                id, id, id, subject
            ));
            emitter.writeln(&format!("if __fn{id} then __fn{id}() end", id = id));
        }
    }
}

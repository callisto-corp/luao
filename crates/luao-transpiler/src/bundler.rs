use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::mangler::Mangler;
use crate::{formatter, TranspileOptions};

/// Represents a parsed module with its imports, exports, and AST.
struct Module {
    path: PathBuf,
    #[allow(dead_code)]
    source: String,
    ast: luao_parser::SourceFile,
    symbol_table: luao_resolver::SymbolTable,
    imports: Vec<ImportInfo>,
    exports: Vec<String>,
}

struct ImportInfo {
    names: Vec<(String, String)>, // (original_name, local_alias)
    path: String,
}

/// Bundle a Luao project starting from the given entrypoint into a single output.
pub fn bundle(entrypoint: &Path, options: &TranspileOptions) -> Result<String, Vec<String>> {
    // Phase 1: Gather all modules recursively
    let mut modules: HashMap<PathBuf, Module> = HashMap::new();
    let mut load_order: Vec<PathBuf> = Vec::new();
    gather_modules(entrypoint, &mut modules, &mut load_order, &mut HashSet::new())?;

    let entry_canonical = canonicalize_path(entrypoint)?;

    // Phase 2: Topological sort (files)
    let sorted_files = topological_sort(&load_order, &entry_canonical);

    // Phase 3: Collect all exported names and check for conflicts
    let mut all_exports: Vec<(String, PathBuf)> = Vec::new();
    for path in &sorted_files {
        let module = &modules[path];
        for name in &module.exports {
            if let Some((_, existing_path)) = all_exports.iter().find(|(n, _)| n == name) {
                return Err(vec![format!(
                    "export name conflict: '{}' is exported by both '{}' and '{}'",
                    name,
                    existing_path.display(),
                    path.display()
                )]);
            }
            all_exports.push((name.clone(), path.clone()));
        }
    }

    // Phase 4: Assign globally unique names.
    // User code (locals, globals, free variables) gets priority — keeps original names.
    // Exports get renamed with numeric suffixes if they conflict.
    let mut used_names: HashSet<String> = HashSet::new();
    let mut file_rename_maps: HashMap<PathBuf, HashMap<String, String>> = HashMap::new();

    // Step 1: Reserve all non-exported names from all files first (user code has priority)
    for path in &sorted_files {
        let module = &modules[path];
        let exported_set: HashSet<&str> = module.exports.iter().map(|s| s.as_str()).collect();

        // Collect top-level locals
        let locals = collect_top_level_locals(&module.ast, &exported_set);
        for name in &locals {
            used_names.insert(name.clone());
        }

        // Collect free variable references (assignments without local, bare identifiers)
        let free_vars = collect_free_variables(&module.ast, &exported_set);
        for name in &free_vars {
            used_names.insert(name.clone());
        }
    }

    // Step 2: Assign unique names for exports — renamed if they conflict with user code
    let mut export_rename_map: HashMap<(PathBuf, String), String> = HashMap::new();
    for path in &sorted_files {
        let module = &modules[path];
        for name in &module.exports {
            let unique = get_unique_name(name, &mut used_names);
            if unique != *name {
                export_rename_map.insert((path.clone(), name.clone()), unique);
            }
        }
    }

    // Step 3: Build per-file rename maps
    // Non-exported locals that conflict with OTHER files' non-exported locals get renamed
    let mut seen_locals: HashSet<String> = HashSet::new();
    // Re-reserve export names (now with their final unique names)
    for path in &sorted_files {
        let module = &modules[path];
        for name in &module.exports {
            let final_name = export_rename_map
                .get(&(path.clone(), name.clone()))
                .cloned()
                .unwrap_or_else(|| name.clone());
            seen_locals.insert(final_name);
        }
    }

    for path in &sorted_files {
        let module = &modules[path];
        let exported_set: HashSet<&str> = module.exports.iter().map(|s| s.as_str()).collect();
        let mut rename_map = HashMap::new();

        // Add export renames for this file
        for name in &module.exports {
            if let Some(new_name) = export_rename_map.get(&(path.clone(), name.clone())) {
                rename_map.insert(name.clone(), new_name.clone());
            }
        }

        // Non-exported locals: rename if they conflict with another file's locals
        let locals = collect_top_level_locals(&module.ast, &exported_set);
        for local_name in &locals {
            if !seen_locals.insert(local_name.clone()) {
                // Already seen in another file — need a unique name
                let unique = get_unique_name(local_name, &mut seen_locals);
                rename_map.insert(local_name.clone(), unique);
            }
        }

        file_rename_maps.insert(path.clone(), rename_map);
    }

    // Phase 5: Build import alias → export name maps per module
    let mut alias_maps: HashMap<PathBuf, HashMap<String, String>> = HashMap::new();
    for path in &sorted_files {
        let module = &modules[path];
        let mut alias_map = HashMap::new();
        for import in &module.imports {
            let dep_path = resolve_import_path(&module.path, &import.path)?;
            let dep_module = modules.get(&dep_path).ok_or_else(|| {
                vec![format!(
                    "cannot resolve import '{}' from '{}'",
                    import.path,
                    module.path.display()
                )]
            })?;
            for (name, alias) in &import.names {
                if !dep_module.exports.contains(name) {
                    return Err(vec![format!(
                        "'{}' is not exported from '{}'",
                        name, import.path
                    )]);
                }
                // Get the final name of the export (may have been renamed)
                let final_export_name = export_rename_map
                    .get(&(dep_path.clone(), name.clone()))
                    .cloned()
                    .unwrap_or_else(|| name.clone());
                // alias in source → final export name in bundle
                // Always map if the final name differs from the alias
                if *alias != final_export_name {
                    alias_map.insert(alias.clone(), final_export_name);
                }
            }
        }
        alias_maps.insert(path.clone(), alias_map);
    }

    // Phase 6: Detect circular file dependencies for forward declarations
    let file_deps = build_file_deps(&modules, &sorted_files, &entry_canonical)?;
    let circular_files = detect_circular_files(&file_deps);

    // Collect forward declaration names (only for circular deps), using final renamed names
    let mut forward_decl_names: Vec<String> = Vec::new();
    for path in &circular_files {
        if let Some(module) = modules.get(path) {
            for name in &module.exports {
                let final_name = export_rename_map
                    .get(&(path.clone(), name.clone()))
                    .cloned()
                    .unwrap_or_else(|| name.clone());
                if !forward_decl_names.contains(&final_name) {
                    forward_decl_names.push(final_name);
                }
            }
        }
    }

    // Phase 7: Transpile each module
    let exported_set: HashSet<String> = if !forward_decl_names.is_empty() {
        forward_decl_names.iter().cloned().collect()
    } else {
        HashSet::new()
    };

    let mut runtime_needs_instanceof = false;
    let mut runtime_needs_enum_freeze = false;
    let mut file_outputs: Vec<(PathBuf, String)> = Vec::new();

    // Shared mangler across all files in the bundle so cross-file references resolve correctly
    let mut shared_mangler: Option<Mangler> = if options.mangle {
        Some(Mangler::new())
    } else {
        None
    };

    // Merge all symbol tables for cross-file type resolution (needed for mangling)
    let mut merged_symbols = luao_resolver::SymbolTable::new();
    for path in &sorted_files {
        let module = &modules[path];
        for (name, cls) in &module.symbol_table.classes {
            merged_symbols.classes.insert(name.clone(), cls.clone());
        }
        for (name, en) in &module.symbol_table.enums {
            merged_symbols.enums.insert(name.clone(), en.clone());
        }
        for (name, iface) in &module.symbol_table.interfaces {
            merged_symbols.interfaces.insert(name.clone(), iface.clone());
        }
    }

    for path in &sorted_files {
        let module = &modules[path];
        let rename_map = file_rename_maps.get(path).cloned().unwrap_or_default();
        let alias_map = alias_maps.get(path).cloned().unwrap_or_default();

        // Take the shared mangler for this file, put it back after
        let mangler = shared_mangler.take();
        let mut emitter = crate::Emitter::new(merged_symbols.clone(), mangler);

        // Forward-declared exported names skip `local`
        if circular_files.contains(path) {
            emitter.exported_names = exported_set.clone();
        }

        // Set rename maps separately: local renames for declarations, import aliases for references
        emitter.local_renames = rename_map;
        emitter.import_aliases = alias_map;

        // Emit statements, skipping imports and unwrapping exports
        for stmt in &module.ast.statements {
            match stmt {
                luao_parser::Statement::ImportDecl(_) => continue,
                luao_parser::Statement::ExportDecl(inner, _) => {
                    emitter.emit_statement(inner);
                }
                _ => {
                    emitter.emit_statement(stmt);
                }
            }
        }

        if emitter.needs_instanceof {
            runtime_needs_instanceof = true;
        }
        if emitter.needs_enum_freeze {
            runtime_needs_enum_freeze = true;
        }

        let output = std::mem::take(&mut emitter.output);
        // Return the shared mangler for the next file
        shared_mangler = emitter.mangler.take();
        file_outputs.push((path.clone(), output));
    }

    // Phase 8: Assemble the bundle
    let mut bundle = String::new();

    // Runtime functions (once at top)
    if runtime_needs_instanceof {
        bundle.push_str(crate::runtime::INSTANCEOF_FN);
        bundle.push_str("\n\n");
    }
    if runtime_needs_enum_freeze {
        bundle.push_str(crate::runtime::ENUM_FREEZE_FN);
        bundle.push_str("\n\n");
    }

    // Forward declarations for circular dependencies only
    if !forward_decl_names.is_empty() {
        bundle.push_str(&format!("local {}\n\n", forward_decl_names.join(", ")));
    }

    // Emit each module's code flat (no do...end wrapping)
    for (_, code) in &file_outputs {
        if !code.trim().is_empty() {
            bundle.push_str(code);
            if !bundle.ends_with('\n') {
                bundle.push('\n');
            }
        }
    }

    // Phase 9: Bundle require() calls for files that exist on disk
    let bundle = bundle_requires(&bundle, &entry_canonical)?;

    if options.minify {
        Ok(formatter::minify_lua(&bundle))
    } else {
        Ok(formatter::format_lua(&bundle))
    }
}

/// Collect all top-level local names declared in a file (excluding exports).
fn collect_top_level_locals(ast: &luao_parser::SourceFile, exported: &HashSet<&str>) -> Vec<String> {
    let mut names = Vec::new();
    for stmt in &ast.statements {
        match stmt {
            luao_parser::Statement::LocalAssignment(la) => {
                for name in &la.names {
                    if !exported.contains(name.name.as_str()) {
                        names.push(name.name.to_string());
                    }
                }
            }
            luao_parser::Statement::FunctionDecl(fd) => {
                if fd.is_local {
                    if let Some(part) = fd.name.parts.first() {
                        if !exported.contains(part.name.as_str()) {
                            names.push(part.name.to_string());
                        }
                    }
                }
            }
            luao_parser::Statement::ExportDecl(inner, _) => {
                // Exports are handled separately; skip their names
                let _ = inner;
            }
            // Classes and enums are always local in transpiled output
            luao_parser::Statement::ClassDecl(cd) => {
                if !exported.contains(cd.name.name.as_str()) {
                    names.push(cd.name.name.to_string());
                }
            }
            luao_parser::Statement::EnumDecl(ed) => {
                if !exported.contains(ed.name.name.as_str()) {
                    names.push(ed.name.name.to_string());
                }
            }
            _ => {}
        }
    }
    names
}

/// Collect free variable references — assignment targets and identifier uses that
/// are not local declarations and not from imports. These represent user globals.
fn collect_free_variables(ast: &luao_parser::SourceFile, exported: &HashSet<&str>) -> Vec<String> {
    let mut names = Vec::new();
    for stmt in &ast.statements {
        match stmt {
            luao_parser::Statement::Assignment(a) => {
                for target in &a.targets {
                    if let luao_parser::Expression::Identifier(id) = target {
                        if !exported.contains(id.name.as_str()) {
                            names.push(id.name.to_string());
                        }
                    }
                }
            }
            luao_parser::Statement::ExportDecl(inner, _) => {
                // Check inside export for any free vars (unlikely but thorough)
                if let luao_parser::Statement::Assignment(a) = inner.as_ref() {
                    for target in &a.targets {
                        if let luao_parser::Expression::Identifier(id) = target {
                            names.push(id.name.to_string());
                        }
                    }
                }
            }
            _ => {}
        }
    }
    names
}

fn get_unique_name(base_name: &str, used_names: &mut HashSet<String>) -> String {
    if !used_names.contains(base_name) {
        used_names.insert(base_name.to_string());
        return base_name.to_string();
    }
    let mut counter = 2;
    loop {
        let candidate = format!("{}{}", base_name, counter);
        if !used_names.contains(&candidate) {
            used_names.insert(candidate.clone());
            return candidate;
        }
        counter += 1;
    }
}

fn gather_modules(
    path: &Path,
    modules: &mut HashMap<PathBuf, Module>,
    load_order: &mut Vec<PathBuf>,
    visiting: &mut HashSet<PathBuf>,
) -> Result<(), Vec<String>> {
    let canonical = canonicalize_path(path)?;

    if modules.contains_key(&canonical) {
        return Ok(());
    }

    if !visiting.insert(canonical.clone()) {
        return Ok(()); // Circular — stop recursion
    }

    let source = std::fs::read_to_string(path).map_err(|e| {
        vec![format!("failed to read '{}': {}", path.display(), e)]
    })?;

    let (ast, parse_errors) = luao_parser::parse(&source);
    if !parse_errors.is_empty() {
        return Err(parse_errors.iter().map(|e| format!("{}: {}", path.display(), e)).collect());
    }

    let mut resolver = luao_resolver::Resolver::new();
    let symbol_table = resolver.resolve(&ast);
    let checker = luao_checker::Checker::new(&symbol_table);
    let diagnostics = checker.check(&ast);
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == luao_checker::DiagnosticSeverity::Error)
        .map(|d| format!("{}: {}", path.display(), d))
        .collect();
    if !errors.is_empty() {
        return Err(errors);
    }

    let mut imports = Vec::new();
    let mut exports = Vec::new();
    extract_module_info(&ast, &mut imports, &mut exports);

    // Recursively load dependencies
    for import in &imports {
        let dep_path = resolve_import_path(&canonical, &import.path)?;
        gather_modules(&dep_path, modules, load_order, visiting)?;
    }

    load_order.push(canonical.clone());
    modules.insert(canonical.clone(), Module {
        path: canonical.clone(),
        source,
        ast,
        symbol_table,
        imports,
        exports,
    });

    visiting.remove(&canonical);
    Ok(())
}

fn extract_module_info(
    ast: &luao_parser::SourceFile,
    imports: &mut Vec<ImportInfo>,
    exports: &mut Vec<String>,
) {
    for stmt in &ast.statements {
        match stmt {
            luao_parser::Statement::ImportDecl(import) => {
                let names = import
                    .names
                    .iter()
                    .map(|n| {
                        let original = n.name.name.to_string();
                        let alias = n
                            .alias
                            .as_ref()
                            .map(|a| a.name.to_string())
                            .unwrap_or_else(|| original.clone());
                        (original, alias)
                    })
                    .collect();
                imports.push(ImportInfo {
                    names,
                    path: import.path.to_string(),
                });
            }
            luao_parser::Statement::ExportDecl(inner, _) => {
                collect_exported_names(inner, exports);
            }
            _ => {}
        }
    }
}

fn collect_exported_names(stmt: &luao_parser::Statement, exports: &mut Vec<String>) {
    match stmt {
        luao_parser::Statement::LocalAssignment(la) => {
            for name in &la.names {
                exports.push(name.name.to_string());
            }
        }
        luao_parser::Statement::FunctionDecl(fd) => {
            if let Some(part) = fd.name.parts.first() {
                exports.push(part.name.to_string());
            }
        }
        luao_parser::Statement::ClassDecl(cd) => {
            exports.push(cd.name.name.to_string());
        }
        luao_parser::Statement::EnumDecl(ed) => {
            exports.push(ed.name.name.to_string());
        }
        luao_parser::Statement::InterfaceDecl(_) | luao_parser::Statement::TypeAlias(_) => {}
        _ => {}
    }
}

fn resolve_import_path(from_file: &Path, import_path: &str) -> Result<PathBuf, Vec<String>> {
    let base_dir = from_file.parent().unwrap_or(Path::new("."));

    // Handle @/ absolute imports (relative to project root)
    let (search_dir, clean_path) = if let Some(stripped) = import_path.strip_prefix("@/") {
        // Walk up to find project root (directory containing the entry point)
        // For now, use the working directory as project root
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        (cwd, stripped.to_string())
    } else {
        (base_dir.to_path_buf(), import_path.to_string())
    };

    // Try exact path
    let exact = search_dir.join(&clean_path);
    if exact.exists() {
        return canonicalize_path(&exact);
    }

    // Try with .luao extension
    let with_ext = search_dir.join(format!("{}.luao", clean_path));
    if with_ext.exists() {
        return canonicalize_path(&with_ext);
    }

    // Try with .lua extension (for Lua interop files)
    let with_lua = search_dir.join(format!("{}.lua", clean_path));
    if with_lua.exists() {
        return canonicalize_path(&with_lua);
    }

    // Try as directory with init.luao
    let dir_init = search_dir.join(&clean_path).join("init.luao");
    if dir_init.exists() {
        return canonicalize_path(&dir_init);
    }

    Err(vec![format!(
        "cannot resolve import '{}' from '{}'",
        import_path,
        from_file.display()
    )])
}

fn canonicalize_path(path: &Path) -> Result<PathBuf, Vec<String>> {
    std::fs::canonicalize(path).map_err(|e| {
        vec![format!("cannot resolve path '{}': {}", path.display(), e)]
    })
}

fn topological_sort(load_order: &[PathBuf], entry: &PathBuf) -> Vec<PathBuf> {
    // DFS post-order from gather_modules gives us dependency order.
    // Ensure entry is last.
    let mut sorted: Vec<PathBuf> = load_order
        .iter()
        .filter(|p| *p != entry)
        .cloned()
        .collect();
    if load_order.contains(entry) {
        sorted.push(entry.clone());
    }
    sorted
}

fn build_file_deps(
    modules: &HashMap<PathBuf, Module>,
    sorted_files: &[PathBuf],
    entry: &PathBuf,
) -> Result<HashMap<PathBuf, HashSet<PathBuf>>, Vec<String>> {
    let mut deps: HashMap<PathBuf, HashSet<PathBuf>> = HashMap::new();
    for path in sorted_files {
        let module = &modules[path];
        let mut file_deps = HashSet::new();
        for import in &module.imports {
            let dep_path = resolve_import_path(&module.path, &import.path)?;
            if dep_path != *entry && dep_path != *path {
                file_deps.insert(dep_path);
            }
        }
        deps.insert(path.clone(), file_deps);
    }
    Ok(deps)
}

fn detect_circular_files(file_deps: &HashMap<PathBuf, HashSet<PathBuf>>) -> HashSet<PathBuf> {
    let mut circular = HashSet::new();

    for start in file_deps.keys() {
        let mut visited = HashSet::new();
        let mut stack = HashSet::new();
        detect_cycle_dfs(start, file_deps, &mut visited, &mut stack, &mut circular);
    }

    circular
}

fn detect_cycle_dfs(
    node: &PathBuf,
    deps: &HashMap<PathBuf, HashSet<PathBuf>>,
    visited: &mut HashSet<PathBuf>,
    stack: &mut HashSet<PathBuf>,
    circular: &mut HashSet<PathBuf>,
) {
    if stack.contains(node) {
        circular.insert(node.clone());
        return;
    }
    if visited.contains(node) {
        return;
    }

    visited.insert(node.clone());
    stack.insert(node.clone());

    if let Some(node_deps) = deps.get(node) {
        for dep in node_deps {
            detect_cycle_dfs(dep, deps, visited, stack, circular);
            if circular.contains(dep) {
                circular.insert(node.clone());
            }
        }
    }

    stack.remove(node);
}

// --- Require bundling (luapack-style) ---

/// Scan Lua source for `require("path")` calls where the file exists on disk.
/// Replace them with module table lookups and prepend lazy-loading wrappers.
fn bundle_requires(lua_code: &str, entry_path: &Path) -> Result<String, Vec<String>> {
    let base_dir = entry_path.parent().unwrap_or(Path::new("."));

    let mut modules: Vec<RequireModule> = Vec::new();
    let mut processed: HashSet<PathBuf> = HashSet::new();
    let mut next_id: usize = 1;

    discover_requires(lua_code, base_dir, &mut modules, &mut processed, &mut next_id)?;

    if modules.is_empty() {
        return Ok(lua_code.to_string());
    }

    let table_name = "__luao_modules";

    // Build arg→replacement mapping
    let replacements: Vec<(String, String)> = modules
        .iter()
        .map(|m| (m.original_arg.clone(), format!("{}[{}]()", table_name, m.id)))
        .collect();

    // Replace requires in the main code
    let mut main_code = lua_code.to_string();
    for (arg, repl) in &replacements {
        main_code = replace_require(&main_code, arg, repl);
    }

    // Also replace requires inside each module's content (for nested requires)
    let mut final_preamble = String::new();
    final_preamble.push_str(&format!("local {} = {{}}\n\n", table_name));

    for module in &modules {
        let mut content = module.content.clone();
        for (arg, repl) in &replacements {
            content = replace_require(&content, arg, repl);
        }

        final_preamble.push_str("do\n");
        final_preamble.push_str("    local module = function()\n");
        for line in content.lines() {
            if !line.is_empty() {
                final_preamble.push_str("        ");
            }
            final_preamble.push_str(line);
            final_preamble.push('\n');
        }
        final_preamble.push_str("    end\n");
        final_preamble.push_str(&format!(
            "    {}[{}] = function()\n        local ret = module()\n        {}[{}] = function() return ret end\n        return ret\n    end\n",
            table_name, module.id, table_name, module.id
        ));
        final_preamble.push_str("end\n\n");
    }

    Ok(format!("{}{}", final_preamble, main_code))
}

struct RequireModule {
    id: usize,
    original_arg: String,
    #[allow(dead_code)]
    path: PathBuf,
    content: String,
}

fn discover_requires(
    source: &str,
    base_dir: &Path,
    modules: &mut Vec<RequireModule>,
    processed: &mut HashSet<PathBuf>,
    next_id: &mut usize,
) -> Result<(), Vec<String>> {
    let requires = find_require_calls(source);

    for req_arg in requires {
        if let Some((resolved_path, content)) = try_resolve_require(&req_arg, base_dir) {
            if processed.contains(&resolved_path) {
                continue;
            }
            processed.insert(resolved_path.clone());

            let id = *next_id;
            *next_id += 1;

            let file_dir = resolved_path.parent().unwrap_or(base_dir).to_path_buf();
            discover_requires(&content, &file_dir, modules, processed, next_id)?;

            modules.push(RequireModule {
                id,
                original_arg: req_arg,
                path: resolved_path,
                content,
            });
        }
    }

    Ok(())
}

fn find_require_calls(source: &str) -> Vec<String> {
    let mut results = Vec::new();
    let bytes = source.as_bytes();
    let len = bytes.len();
    let kw = b"require";

    let mut i = 0;
    while i < len {
        // Skip string literals
        if bytes[i] == b'"' || bytes[i] == b'\'' {
            let quote = bytes[i];
            i += 1;
            while i < len && bytes[i] != quote {
                if bytes[i] == b'\\' {
                    i += 1;
                }
                i += 1;
            }
            if i < len {
                i += 1;
            }
            continue;
        }

        // Skip long strings [[...]]
        if i + 1 < len && bytes[i] == b'[' && bytes[i + 1] == b'[' {
            i += 2;
            while i + 1 < len && !(bytes[i] == b']' && bytes[i + 1] == b']') {
                i += 1;
            }
            i += 2;
            continue;
        }

        // Skip comments
        if i + 1 < len && bytes[i] == b'-' && bytes[i + 1] == b'-' {
            i += 2;
            if i + 1 < len && bytes[i] == b'[' && bytes[i + 1] == b'[' {
                // Block comment
                i += 2;
                while i + 1 < len && !(bytes[i] == b']' && bytes[i + 1] == b']') {
                    i += 1;
                }
                i += 2;
            } else {
                // Line comment
                while i < len && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            continue;
        }

        if i + 7 <= len && &bytes[i..i + 7] == kw {
            if i > 0 && is_ident_byte(bytes[i - 1]) {
                i += 1;
                continue;
            }

            let mut j = i + 7;
            while j < len && bytes[j].is_ascii_whitespace() {
                j += 1;
            }

            let has_paren = j < len && bytes[j] == b'(';
            if has_paren {
                j += 1;
                while j < len && bytes[j].is_ascii_whitespace() {
                    j += 1;
                }
            }

            if j < len && (bytes[j] == b'"' || bytes[j] == b'\'') {
                let quote = bytes[j];
                j += 1;
                let start = j;
                while j < len && bytes[j] != quote {
                    if bytes[j] == b'\\' {
                        j += 1;
                    }
                    j += 1;
                }
                if j < len {
                    let arg = String::from_utf8_lossy(&bytes[start..j]).to_string();
                    if !results.contains(&arg) {
                        results.push(arg);
                    }
                    i = j + 1;
                    continue;
                }
            }
        }
        i += 1;
    }

    results
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn try_resolve_require(req_arg: &str, base_dir: &Path) -> Option<(PathBuf, String)> {
    let path_str = req_arg.replace('.', "/");

    let variations = [
        base_dir.join(&path_str),
        base_dir.join(format!("{}.lua", path_str)),
        base_dir.join(format!("{}.luau", path_str)),
        base_dir.join(&path_str).join("init.lua"),
        base_dir.join(req_arg),
        base_dir.join(format!("{}.lua", req_arg)),
    ];

    for candidate in &variations {
        if candidate.exists() && candidate.is_file() {
            if let Ok(content) = std::fs::read_to_string(candidate) {
                if let Ok(canonical) = std::fs::canonicalize(candidate) {
                    return Some((canonical, content));
                }
            }
        }
    }

    None
}

fn replace_require(source: &str, req_arg: &str, replacement: &str) -> String {
    let mut result = String::with_capacity(source.len());
    let bytes = source.as_bytes();
    let len = bytes.len();
    let kw = b"require";

    let mut i = 0;
    while i < len {
        // Skip string literals
        if bytes[i] == b'"' || bytes[i] == b'\'' {
            let quote = bytes[i];
            result.push(quote as char);
            i += 1;
            while i < len && bytes[i] != quote {
                if bytes[i] == b'\\' {
                    result.push(bytes[i] as char);
                    i += 1;
                    if i < len {
                        result.push(bytes[i] as char);
                        i += 1;
                    }
                } else {
                    result.push(bytes[i] as char);
                    i += 1;
                }
            }
            if i < len {
                result.push(bytes[i] as char);
                i += 1;
            }
            continue;
        }

        if i + 7 <= len && &bytes[i..i + 7] == kw && (i == 0 || !is_ident_byte(bytes[i - 1])) {
            let start_i = i;
            let mut j = i + 7;
            while j < len && bytes[j].is_ascii_whitespace() {
                j += 1;
            }

            let has_paren = j < len && bytes[j] == b'(';
            if has_paren {
                j += 1;
                while j < len && bytes[j].is_ascii_whitespace() {
                    j += 1;
                }
            }

            if j < len && (bytes[j] == b'"' || bytes[j] == b'\'') {
                let quote = bytes[j];
                j += 1;
                let arg_start = j;
                while j < len && bytes[j] != quote {
                    if bytes[j] == b'\\' {
                        j += 1;
                    }
                    j += 1;
                }
                if j < len {
                    let found_arg = String::from_utf8_lossy(&bytes[arg_start..j]).to_string();
                    j += 1; // closing quote
                    if has_paren {
                        while j < len && bytes[j].is_ascii_whitespace() {
                            j += 1;
                        }
                        if j < len && bytes[j] == b')' {
                            j += 1;
                        }
                    }
                    if found_arg == req_arg {
                        result.push_str(replacement);
                        i = j;
                        continue;
                    }
                }
            }

            result.push(bytes[start_i] as char);
            i = start_i + 1;
        } else {
            result.push(bytes[i] as char);
            i += 1;
        }
    }

    result
}

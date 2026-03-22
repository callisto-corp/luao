use std::collections::{HashMap, HashSet};

use luao_parser::{SourceFile, Statement};
use luao_resolver::SymbolTable;

use crate::class_emitter;
use crate::enum_emitter;
use crate::mangler::Mangler;
use crate::runtime;
use crate::statement_emitter;

pub struct Emitter {
    pub(crate) output: String,
    pub(crate) indent_level: usize,
    pub(crate) symbol_table: SymbolTable,
    pub(crate) needs_instanceof: bool,
    pub(crate) needs_enum_freeze: bool,
    pub(crate) needs_abstract_guard: bool,
    pub(crate) needs_async: bool,
    pub(crate) needs_array: bool,
    pub(crate) needs_tuple: bool,
    pub(crate) in_async_context: bool,
    pub(crate) current_class: Option<String>,
    pub(crate) current_class_parent: Option<String>,
    pub(crate) mangler: Option<Mangler>,
    /// Names that are forward-declared externally (used in bundling).
    /// When set, declarations for these names omit `local`.
    pub(crate) exported_names: HashSet<String>,
    /// Rename map for local variables (used in bundling to resolve name conflicts).
    /// Maps original name → unique name. Applied to declarations and their references.
    pub(crate) local_renames: HashMap<String, String>,
    /// Import alias map (used in bundling). Maps alias → export name.
    /// Only applied to identifier references, NOT to new local declarations.
    pub(crate) import_aliases: HashMap<String, String>,
    /// Properties with getters: (class_name, prop_name) → getter method name
    pub(crate) property_getters: HashMap<(String, String), String>,
    /// Properties with setters: (class_name, prop_name) → setter method name
    pub(crate) property_setters: HashMap<(String, String), String>,
    /// When true, methods use `.` with explicit self param instead of `:`
    pub(crate) no_self: bool,
    /// The name used for the self parameter when no_self is enabled
    pub(crate) self_param_name: Option<String>,
    /// Tracks the class type of local variables (var_name → class_name).
    /// Populated from type annotations and `new ClassName()` expressions.
    pub(crate) local_var_types: HashMap<String, String>,
    /// When set, table constructor field names are mangled using this type.
    pub(crate) table_target_type: Option<String>,
    /// When true, all top-level declarations omit the `local` keyword (bundled globals mode).
    pub(crate) bundle_globals_mode: bool,
    /// Counter for generating unique temporary variable names.
    pub(crate) temp_id: usize,
}

impl Emitter {
    pub fn new(symbol_table: SymbolTable, mangler: Option<Mangler>) -> Self {
        Self {
            output: String::new(),
            indent_level: 0,
            symbol_table,
            needs_instanceof: false,
            needs_enum_freeze: false,
            needs_abstract_guard: false,
            needs_async: false,
            needs_array: false,
            needs_tuple: false,
            in_async_context: false,
            current_class: None,
            current_class_parent: None,
            mangler,
            exported_names: HashSet::new(),
            local_renames: HashMap::new(),
            import_aliases: HashMap::new(),
            property_getters: HashMap::new(),
            property_setters: HashMap::new(),
            no_self: false,
            self_param_name: None,
            local_var_types: HashMap::new(),
            table_target_type: None,
            bundle_globals_mode: false,
            temp_id: 0,
        }
    }

    pub fn next_temp_id(&mut self) -> usize {
        let id = self.temp_id;
        self.temp_id += 1;
        id
    }

    pub fn emit(&mut self, file: &SourceFile) {
        for stmt in &file.statements {
            self.emit_statement(stmt);
        }
    }

    pub fn output(mut self) -> String {
        let mut preamble = String::new();
        if self.needs_instanceof {
            preamble.push_str(runtime::INSTANCEOF_FN);
            preamble.push('\n');
            preamble.push('\n');
        }
        if self.needs_enum_freeze {
            preamble.push_str(runtime::ENUM_FREEZE_FN);
            preamble.push('\n');
            preamble.push('\n');
        }
        if self.needs_abstract_guard {
            preamble.push_str(runtime::ABSTRACT_GUARD_FN);
            preamble.push('\n');
            preamble.push('\n');
        }
        if self.needs_async {
            preamble.push_str(runtime::PROMISE_RUNTIME);
            preamble.push('\n');
            preamble.push('\n');
        }
        if self.needs_tuple {
            preamble.push_str(runtime::TUPLE_FN);
            preamble.push('\n');
            preamble.push('\n');
        }
        if self.needs_array {
            preamble.push_str(runtime::ARRAY_RUNTIME);
            preamble.push('\n');
            preamble.push('\n');
        }
        if !preamble.is_empty() {
            preamble.push_str(&self.output);
            self.output = preamble;
        }
        self.output
    }

    pub fn emit_statement(&mut self, stmt: &Statement) {
        match stmt {
            Statement::ClassDecl(class) => {
                class_emitter::emit_class(self, class);
            }
            Statement::EnumDecl(enum_decl) => {
                enum_emitter::emit_enum(self, enum_decl);
            }
            Statement::InterfaceDecl(_) => {}
            _ => {
                statement_emitter::emit_statement(self, stmt);
            }
        }
    }

    pub fn write(&mut self, s: &str) {
        self.output.push_str(s);
    }

    pub fn writeln(&mut self, s: &str) {
        self.write_indent();
        self.output.push_str(s);
        self.output.push('\n');
    }

    pub fn write_indent(&mut self) {
        for _ in 0..self.indent_level {
            self.output.push_str("    ");
        }
    }

    pub fn indent(&mut self) {
        self.indent_level += 1;
    }

    pub fn dedent(&mut self) {
        if self.indent_level > 0 {
            self.indent_level -= 1;
        }
    }

    pub fn newline(&mut self) {
        self.output.push('\n');
    }

    pub fn emit_block(&mut self, block: &luao_parser::Block) {
        self.indent();
        for stmt in &block.statements {
            self.emit_statement(stmt);
        }
        self.dedent();
    }

    pub fn emit_params(&mut self, params: &[luao_parser::Parameter]) -> String {
        params
            .iter()
            .map(|p| {
                if p.is_vararg {
                    "...".to_string()
                } else {
                    p.name.name.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Mangle a member name for the given type. Returns the original name if mangling is disabled.
    pub fn mangle_member(&mut self, type_name: &str, member_name: &str) -> String {
        if let Some(ref mut mangler) = self.mangler {
            mangler.mangle(type_name, member_name)
        } else {
            member_name.to_string()
        }
    }

    /// Look up an already-mangled name without creating a new mapping.
    pub fn lookup_mangled(&self, type_name: &str, member_name: &str) -> Option<String> {
        self.mangler
            .as_ref()
            .and_then(|m| m.lookup(type_name, member_name))
    }

    /// Check if a name refers to a known class in the symbol table.
    pub fn is_class(&self, name: &str) -> bool {
        self.symbol_table.classes.contains_key(name)
    }

    /// Check if a name refers to a known interface in the symbol table.
    pub fn is_interface(&self, name: &str) -> bool {
        self.symbol_table.interfaces.contains_key(name)
    }

    /// Check if a name refers to a known class or interface.
    pub fn is_type(&self, name: &str) -> bool {
        self.is_class(name) || self.is_interface(name)
    }

    /// Check if a name refers to a known enum in the symbol table.
    pub fn is_enum(&self, name: &str) -> bool {
        self.symbol_table.enums.contains_key(name)
    }

    /// Check if a name is exported (should skip `local` in bundled output).
    pub fn is_exported(&self, name: &str) -> bool {
        self.exported_names.contains(name)
    }

    /// Check if `local` should be suppressed for a top-level declaration.
    /// True when in bundle globals mode and at the top-level scope (indent 0).
    pub fn should_skip_local(&self) -> bool {
        self.bundle_globals_mode && self.indent_level == 0
    }

    /// Get the mangled name for a shared member (like _new, _values).
    /// Returns the original name if mangling is disabled.
    pub fn mangle_shared(&mut self, name: &str) -> String {
        if let Some(ref mut mangler) = self.mangler {
            // Use an empty type name — shared names are type-independent
            mangler.mangle("", name)
        } else {
            name.to_string()
        }
    }

    /// Apply rename map for references (checks import aliases first, then local renames).
    pub fn rename(&self, name: &str) -> String {
        if name == "self" {
            if let Some(ref self_name) = self.self_param_name {
                return self_name.clone();
            }
        }
        if let Some(alias_target) = self.import_aliases.get(name) {
            return alias_target.clone();
        }
        self.local_renames
            .get(name)
            .cloned()
            .unwrap_or_else(|| name.to_string())
    }

    /// Apply rename map for declarations only (local renames, NOT import aliases).
    /// Used when declaring a new local that might shadow an import.
    pub fn rename_decl(&self, name: &str) -> String {
        self.local_renames
            .get(name)
            .cloned()
            .unwrap_or_else(|| name.to_string())
    }
}
